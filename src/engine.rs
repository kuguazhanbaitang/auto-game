//! 流程引擎：把步骤编译成指令序列，支持循环/条件分支，按序执行并输出报告
//!
//! 控制流语法（TOML 扁平步骤）：
//! - `repeat`（需 `count`）... `end_repeat`：循环
//! - `if_image`（需 `image`）... [`else`] ... `end_if`：条件分支

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Once};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use image::{Rgba, RgbaImage, imageops};

use crate::action::Actions;
use crate::adapter::{Key, Match, OcrBackend, key_from_str};
use crate::report::{Report, Status};
use crate::script::{Scenario, Step};

/// 紧急停止（failsafe）：运行中按 F9 中止场景
struct Failsafe {
    enabled: bool,
    device: Option<device_query::DeviceState>,
}

impl Failsafe {
    fn new() -> Self {
        let device = std::panic::catch_unwind(device_query::DeviceState::new).ok();
        let enabled = device.is_some();
        if !enabled {
            tracing::warn!("failsafe 不可用：无法读取键盘状态（非交互会话？）");
        }
        Failsafe { enabled, device }
    }

    fn triggered(&self) -> bool {
        use device_query::DeviceQuery;
        if !self.enabled {
            return false;
        }
        if let Some(d) = &self.device {
            d.get_keys().iter().any(|k| *k == device_query::Keycode::F9)
        } else {
            false
        }
    }
}

/// 编译后的指令
struct Instr {
    step: Step,
    kind: Kind,
}

/// 指令类型（跳转字段在编译期填充）
enum Kind {
    /// 普通动作
    Action,
    /// 循环开始；end = 对应 end_repeat 的索引
    Repeat { end: usize },
    /// 循环结束；back = 对应 repeat 的索引
    EndRepeat { back: usize },
    /// 条件判断；命中顺序执行 then，未命中跳到 else_or_end
    IfImage { else_or_end: usize, end: usize },
    /// else 分支；then 已执行则跳到 end
    Else { end: usize },
    /// 条件结束
    EndIf,
}

/// 运行时控制流帧
enum Frame {
    Repeat { count: u32, done: u32 },
    InThen,
    InElse,
}

/// 流程引擎
pub struct Engine {
    actions: Actions,
    report: Report,
    assets_dir: PathBuf,
    reports_dir: PathBuf,
    failsafe: Failsafe,
    /// fast→exact 确认开关（来自场景 [meta] verify_exact）
    verify_exact: bool,
    /// OCR 后端（懒加载：首次用到 OCR 动作时从 assets/ocr 加载模型）
    ocr: Mutex<Option<Arc<OcrBackend>>>,
}

impl Engine {
    pub fn new(scenario_name: &str, assets_dir: PathBuf) -> Self {
        let safe_name = scenario_name
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
            .collect::<String>();
        Engine {
            actions: Actions::new(),
            report: Report::new(scenario_name.to_string()),
            assets_dir,
            reports_dir: PathBuf::from("reports").join(safe_name),
            failsafe: Failsafe::new(),
            verify_exact: false,
            ocr: Mutex::new(None),
        }
    }

    /// 运行整个场景，返回是否全部通过（自动打印文本报告 + 写出 HTML 报告）
    pub fn run(&mut self, scenario: &Scenario) -> Result<bool> {
        self.verify_exact = scenario.meta.verify_exact;
        if self.verify_exact {
            tracing::info!("verify_exact 已开启：所有模板匹配将做 fast→exact 精确确认");
        }
        self.actions.set_window(scenario.meta.window.clone());
        if let Some(w) = &scenario.meta.window {
            tracing::info!("窗口捕获模式已启用：限定标题 {w:?}，识别/输入坐标自动映射到屏幕");
        }
        let instrs = compile(&scenario.steps)?;
        let mut frames: Vec<Frame> = Vec::new();
        let mut pc = 0usize;
        let mut exec_no = 0usize;

        while pc < instrs.len() {
            if self.failsafe.triggered() {
                tracing::warn!("failsafe 触发：检测到 F9，中止场景");
                exec_no += 1;
                self.report.record(
                    exec_no,
                    "failsafe",
                    "用户按 F9 手动中止".to_string(),
                    Status::Fail,
                    Duration::ZERO,
                );
                break;
            }
            let instr = &instrs[pc];
            match &instr.kind {
                Kind::Action => {
                    exec_no += 1;
                    let start = Instant::now();
                    let result = self.execute(&instr.step, exec_no);
                    let duration = start.elapsed();
                    match result {
                        Ok(detail) => self.report.record(
                            exec_no, &instr.step.action, detail, Status::Pass, duration,
                        ),
                        Err(e) => {
                            let snap = self.save_failure_snapshot(&instr.step, exec_no);
                            self.report.record(
                                exec_no,
                                &instr.step.action,
                                format!("{e:#}{snap}"),
                                Status::Fail,
                                duration,
                            );
                        }
                    }
                    pc += 1;
                }
                Kind::Repeat { .. } => {
                    let count = instr.step.count.unwrap_or(1);
                    frames.push(Frame::Repeat { count, done: 0 });
                    pc += 1;
                }
                Kind::EndRepeat { back } => {
                    match frames.last_mut() {
                        Some(Frame::Repeat { count, done }) if *done + 1 < *count => {
                            *done += 1;
                            pc = back + 1;
                        }
                        _ => {
                            frames.pop();
                            pc += 1;
                        }
                    }
                }
                Kind::IfImage { else_or_end, .. } => {
                    exec_no += 1;
                    let start = Instant::now();
                    let found = self.if_image_hit(&instr.step);
                    let duration = start.elapsed();
                    match found {
                        Ok(hit) => {
                            let detail = if hit {
                                "条件命中：找到模板，执行 then 分支".to_string()
                            } else {
                                "条件未命中：未找到模板，走 else/跳过".to_string()
                            };
                            self.report.record(
                                exec_no, &instr.step.action, detail, Status::Pass, duration,
                            );
                            if hit {
                                frames.push(Frame::InThen);
                                pc += 1;
                            } else {
                                frames.push(Frame::InElse);
                                pc = *else_or_end;
                            }
                        }
                        Err(e) => {
                            let snap = self.save_failure_snapshot(&instr.step, exec_no);
                            self.report.record(
                                exec_no,
                                &instr.step.action,
                                format!("{e:#}{snap}"),
                                Status::Fail,
                                duration,
                            );
                            frames.push(Frame::InElse);
                            pc = *else_or_end;
                        }
                    }
                }
                Kind::Else { end } => match frames.pop() {
                    Some(Frame::InThen) => {
                        frames.push(Frame::InElse);
                        pc = *end;
                    }
                    _ => {
                        frames.push(Frame::InElse);
                        pc += 1;
                    }
                },
                Kind::EndIf => {
                    frames.pop();
                    pc += 1;
                }
            }
        }

        self.report.print();
        self.report.write_html(&self.reports_dir.join("index.html"))?;
        Ok(self.report.all_passed())
    }

    /// 条件判断：`if_image` 用模板匹配，`if_text` 用 OCR 文字包含匹配；
    /// 模板加载/匹配出错时返回 Err
    fn if_image_hit(&self, step: &Step) -> Result<bool> {
        if step.action == "if_text" {
            return self.find_text_line(step).map(|l| l.is_some());
        }
        Ok(self.find_match(step)?.is_some())
    }

    /// 在屏幕或指定区域内查找模板。
    /// 开关粒度：步骤显式声明 `verify_exact` 时覆盖 [meta] 全局值，否则回退全局。
    fn find_match(&self, step: &Step) -> Result<Option<Match>> {
        let template = self.load_template(step)?;
        let precision = req_precision(step);
        let verify = step.verify_exact.unwrap_or(self.verify_exact);
        match step.region {
            Some(r) => {
                self.actions
                    .find_image_region(&template, precision, r, verify)
            }
            None => self.actions.find_image(&template, precision, verify),
        }
    }

    /// 轮询等待模板出现（支持区域限定），超时返回 None
    fn wait_match(&self, step: &Step, timeout: Duration) -> Result<Option<Match>> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(m) = self.find_match(step)? {
                return Ok(Some(m));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// 分发到具体动作实现
    fn execute(&self, step: &Step, index: usize) -> Result<String> {
        match step.action.as_str() {
            "screenshot" => self.exec_screenshot(index),
            "wait" => self.exec_wait(step),
            "move_mouse" => self.exec_move_mouse(step),
            "click" => self.exec_click(step),
            "key_press" => self.exec_key_press(step),
            "key_combo" => self.exec_key_combo(step),
            "type_text" => self.exec_type_text(step),
            "find_image" => self.exec_find_image(step),
            "wait_image" => self.exec_wait_image(step),
            "click_image" => self.exec_click_image(step),
            "assert_image" => self.exec_assert_image(step),
            "ocr_text" => self.exec_ocr_text(step),
            "click_text" => self.exec_click_text(step),
            "assert_text" => self.exec_assert_text(step),
            other => bail!("未知动作类型: {other}"),
        }
    }

    // ---- 采集/输入类动作 ----

    fn exec_screenshot(&self, index: usize) -> Result<String> {
        let img = self.actions.snapshot()?;
        std::fs::create_dir_all(&self.reports_dir)?;
        let path = self.reports_dir.join(format!("step_{index}.png"));
        img.save(&path)?;
        Ok(format!("已保存截图 {}x{} -> {}", img.width(), img.height(), path.display()))
    }

    fn exec_wait(&self, step: &Step) -> Result<String> {
        let secs = step.seconds.unwrap_or(0.0);
        std::thread::sleep(Duration::from_secs_f64(secs));
        Ok(format!("固定等待 {secs}s"))
    }

    fn exec_move_mouse(&self, step: &Step) -> Result<String> {
        let (x, y) = req_xy(step)?;
        self.actions.move_mouse(x, y)?;
        Ok(format!("鼠标移动到 ({x}, {y})"))
    }

    fn exec_click(&self, step: &Step) -> Result<String> {
        let (x, y) = req_xy(step)?;
        let (dx, dy) = jitter_offset(step.jitter.unwrap_or(0));
        let (cx, cy) = (x + dx, y + dy);
        self.actions.move_mouse(cx, cy)?;
        let delay = random_click_delay(step);
        if delay > 0.0 {
            std::thread::sleep(Duration::from_secs_f64(delay));
        }
        self.actions.click()?;
        let mut note = format!("点击坐标 ({cx}, {cy})");
        if dx != 0 || dy != 0 {
            note = format!("{note}（基座 ({x}, {y}) + jitter ({dx}, {dy})）");
        }
        if delay > 0.0 {
            note = format!("{note}，点击前随机延时 {delay:.3}s");
        }
        Ok(note)
    }

    fn exec_key_press(&self, step: &Step) -> Result<String> {
        let name = step.key.as_deref().ok_or_else(|| anyhow!("key_press 缺少 key 参数"))?;
        let key = key_from_str(name)?;
        self.actions.key_press(key)?;
        Ok(format!("按下按键 {name}"))
    }

    fn exec_key_combo(&self, step: &Step) -> Result<String> {
        let names = step
            .keys
            .as_deref()
            .ok_or_else(|| anyhow!("key_combo 缺少 keys 参数（如 keys = [\"ctrl\", \"a\"]）"))?;
        let keys: Vec<Key> = names.iter().map(|n| key_from_str(n)).collect::<Result<_>>()?;
        self.actions.key_combo(&keys)?;
        Ok(format!("组合键 {}", names.join(" + ")))
    }

    fn exec_type_text(&self, step: &Step) -> Result<String> {
        let text = step.text.as_deref().ok_or_else(|| anyhow!("type_text 缺少 text 参数"))?;
        self.actions.type_text(text)?;
        Ok(format!("输入文本: {text}"))
    }

    // ---- 图像识别类动作 ----

    fn exec_find_image(&self, step: &Step) -> Result<String> {
        match self.find_match(step)? {
            Some(m) => Ok(format!(
                "找到模板：位置 ({}, {}), 置信度 {:.4}",
                m.x, m.y, m.confidence
            )),
            None => Ok("未找到模板".to_string()),
        }
    }

    fn exec_wait_image(&self, step: &Step) -> Result<String> {
        let timeout = req_timeout(step);
        match self.wait_match(step, timeout)? {
            Some(m) => Ok(format!(
                "等待到模板：位置 ({}, {}), 置信度 {:.4}",
                m.x, m.y, m.confidence
            )),
            None => bail!("超时 {timeout:?} 内未等到模板"),
        }
    }

    fn exec_click_image(&self, step: &Step) -> Result<String> {
        let m = self
            .find_match(step)?
            .ok_or_else(|| anyhow!("click_image 失败：未找到目标图像"))?;
        let (bx, by) = m.center();
        let (cx, cy) = if let Some(j) = step.jitter {
            let (dx, dy) = jitter_offset(j);
            // 限制在模板范围内，避免点出目标元素
            let half_w = m.width as i32 / 2;
            let half_h = m.height as i32 / 2;
            (
                (bx + dx).clamp(bx - half_w, bx + half_w),
                (by + dy).clamp(by - half_h, by + half_h),
            )
        } else {
            (bx, by)
        };
        self.actions.move_mouse(cx, cy)?;
        let delay = random_click_delay(step);
        if delay > 0.0 {
            std::thread::sleep(Duration::from_secs_f64(delay));
        }
        self.actions.click()?;
        if cx != bx || cy != by {
            Ok(format!(
                "点击模板中心附近 ({cx}, {cy})（基座 ({bx}, {by})，jitter={}，延时 {delay:.3}s），置信度 {:.4}",
                step.jitter.unwrap_or(0),
                m.confidence
            ))
        } else if delay > 0.0 {
            Ok(format!(
                "点击模板中心 ({cx}, {cy})（点击前随机延时 {delay:.3}s），置信度 {:.4}",
                m.confidence
            ))
        } else {
            Ok(format!("点击模板中心 ({cx}, {cy})，置信度 {:.4}", m.confidence))
        }
    }

    fn exec_assert_image(&self, step: &Step) -> Result<String> {
        let timeout = req_timeout(step);
        if self.wait_match(step, timeout)?.is_none() {
            bail!("assert_image 失败：{timeout:?} 内未找到目标图像");
        }
        Ok(format!("断言通过：模板存在（precision={}）", req_precision(step)))
    }

    // ---- OCR 文字识别类动作（与模板匹配互补，识别 UI 文案/动态文本）----

    /// 懒加载 OCR 后端：首次用到时从 assets/ocr 加载 PP-OCRv4 模型。
    /// 返回 Arc 克隆，避免跨线程借用。
    fn ensure_ocr(&self) -> Result<Arc<OcrBackend>> {
        let mut guard = self.ocr.lock().unwrap();
        if guard.is_none() {
            let model_dir = self.assets_dir.join("ocr");
            tracing::info!("首次使用 OCR：加载模型 {}", model_dir.display());
            *guard = Some(Arc::new(OcrBackend::load(&model_dir)?));
        }
        Ok(guard.as_ref().unwrap().clone())
    }

    /// 屏幕截图 + OCR 识别（region 限定识别区域，None 全屏）。
    fn ocr_screen(&self, step: &Step) -> Result<Vec<crate::adapter::OcrLine>> {
        let backend = self.ensure_ocr()?;
        let img = self.actions.snapshot()?;
        backend.recognize_region(&img, step.region)
    }

    /// 在 OCR 结果中查找包含指定文本的行（子串匹配，取置信度最高者）。
    fn find_text_line(&self, step: &Step) -> Result<Option<crate::adapter::OcrLine>> {
        let text = step
            .text
            .as_deref()
            .ok_or_else(|| anyhow!("动作 {} 缺少 text 参数", step.action))?;
        let lines = self.ocr_screen(step)?;
        Ok(lines
            .into_iter()
            .filter(|l| l.text.contains(text))
            .max_by(|a, b| a.confidence.partial_cmp(&b.confidence).unwrap_or(std::cmp::Ordering::Equal)))
    }

    fn exec_ocr_text(&self, step: &Step) -> Result<String> {
        let lines = self.ocr_screen(step)?;
        if lines.is_empty() {
            return Ok("未识别到文本".to_string());
        }
        // 最多展示前 20 行，避免报告过长；超出的行数单独提示
        let shown: Vec<String> = lines
            .iter()
            .take(20)
            .map(|l| {
                format!(
                    "{}[{}x{} @({},{}) 置信度{:.0}%]",
                    l.text,
                    l.w,
                    l.h,
                    l.x,
                    l.y,
                    l.confidence * 100.0
                )
            })
            .collect();
        let more = if lines.len() > 20 {
            format!("\n  … 共识别 {} 行，其余省略", lines.len())
        } else {
            String::new()
        };
        Ok(format!("识别到 {} 行文本：\n  {}", lines.len(), shown.join("\n  ")) + &more)
    }

    fn exec_click_text(&self, step: &Step) -> Result<String> {
        let line = self.find_text_line(step)?.ok_or_else(|| {
            anyhow!(
                "click_text 失败：未识别到包含 {:?} 的文本",
                step.text.as_deref().unwrap_or("")
            )
        })?;
        let (bx, by) = line.center();
        let (cx, cy) = if let Some(j) = step.jitter {
            let (dx, dy) = jitter_offset(j);
            // 限制在文字行范围内，避免点出目标元素
            let half_w = line.w as i32 / 2;
            let half_h = line.h as i32 / 2;
            (
                (bx + dx).clamp(bx - half_w, bx + half_w),
                (by + dy).clamp(by - half_h, by + half_h),
            )
        } else {
            (bx, by)
        };
        self.actions.move_mouse(cx, cy)?;
        let delay = random_click_delay(step);
        if delay > 0.0 {
            std::thread::sleep(Duration::from_secs_f64(delay));
        }
        self.actions.click()?;
        if cx != bx || cy != by {
            Ok(format!(
                "点击文本 {:?} 中心附近 ({cx}, {cy})（基座 ({bx}, {by})，jitter={}，延时 {delay:.3}s），置信度 {:.3}",
                line.text,
                step.jitter.unwrap_or(0),
                line.confidence
            ))
        } else if delay > 0.0 {
            Ok(format!(
                "点击文本 {:?} 中心 ({cx}, {cy})（点击前随机延时 {delay:.3}s），置信度 {:.3}",
                line.text, line.confidence
            ))
        } else {
            Ok(format!(
                "点击文本 {:?} 中心 ({cx}, {cy})，置信度 {:.3}",
                line.text, line.confidence
            ))
        }
    }

    fn exec_assert_text(&self, step: &Step) -> Result<String> {
        let timeout = req_timeout(step);
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(l) = self.find_text_line(step)? {
                return Ok(format!(
                    "断言通过：识别到文本 {:?}（置信度 {:.3}）",
                    l.text, l.confidence
                ));
            }
            if Instant::now() >= deadline {
                bail!(
                    "assert_text 失败：{timeout:?} 内未识别到包含 {:?} 的文本",
                    step.text.as_deref().unwrap_or("")
                );
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    // ---- 辅助 ----

    fn load_template(&self, step: &Step) -> Result<RgbaImage> {
        let name = step.image.as_deref().ok_or_else(|| anyhow!("动作 {} 缺少 image 参数", step.action))?;
        let path = self.resolve_asset(name)?;
        let img = image::open(&path)
            .map_err(|e| anyhow!("加载模板 {} 失败: {e}", path.display()))?;
        Ok(img.to_rgba8())
    }

    fn resolve_asset(&self, name: &str) -> Result<PathBuf> {
        let p = self.assets_dir.join(name);
        if !p.exists() {
            bail!("模板文件不存在: {}", p.display());
        }
        Ok(p)
    }

    /// 步骤失败时自动存档「现场截图」（有 region 用区域，否则全屏）；
    /// 若该步带模板，再生成一张「左=旧模板 / 右=现场」对照图，便于核对 UI 变更。
    /// 任一环节失败仅记录日志，不阻断主流程。返回追加到报告详情中的文本。
    fn save_failure_snapshot(&self, step: &Step, index: usize) -> String {
        let mut notes = Vec::new();
        let shot = match step.region {
            Some(r) => self.actions.snapshot_region(r),
            None => self.actions.snapshot(),
        };
        let shot_img = match shot {
            Ok(img) => img,
            Err(e) => {
                tracing::warn!("失败存档：现场截图失败 {e:#}");
                return String::new();
            }
        };
        if std::fs::create_dir_all(&self.reports_dir).is_err() {
            tracing::warn!("失败存档：无法创建报告目录 {}", self.reports_dir.display());
            return String::new();
        }
        // 现场截图
        let shot_path = self.reports_dir.join(format!("fail_step_{index}.png"));
        match shot_img.save(&shot_path) {
            Ok(_) => notes.push(format!("现场截图: {}", shot_path.display())),
            Err(e) => tracing::warn!("失败存档：保存现场截图失败 {e:#}"),
        }
        // 新旧对照图（左=模板，右=现场）
        if let Some(name) = step.image.as_deref() {
            if let Ok(tpl) = self.load_template(step) {
                let diff = build_diff_image(&tpl, &shot_img);
                let diff_path = self.reports_dir.join(format!("diff_step_{index}.png"));
                match diff.save(&diff_path) {
                    Ok(_) => notes.push(format!(
                        "新旧对照(左=模板 {} / 右=现场): {}",
                        name,
                        diff_path.display()
                    )),
                    Err(e) => tracing::warn!("失败存档：保存对照图失败 {e:#}"),
                }
            }
        }
        if notes.is_empty() {
            String::new()
        } else {
            format!("\n  失败存档: {}", notes.join("\n  失败存档: "))
        }
    }
}

/// 水平拼接「旧模板 + 分隔线 + 现场截图」为对照图（高度取最大，白底浅灰）
fn build_diff_image(old: &RgbaImage, new: &RgbaImage) -> RgbaImage {
    let h = old.height().max(new.height());
    let sep = 10u32;
    let w = old.width() + new.width() + sep;
    let mut canvas = RgbaImage::from_pixel(w, h, Rgba([244, 246, 250, 255]));
    imageops::overlay(&mut canvas, old, 0, 0);
    imageops::overlay(&mut canvas, new, (old.width() + sep) as i64, 0);
    canvas
}

/// —— 拟人化点击：轻量 xorshift64* 随机源（零依赖、线程安全）——
static RNG_SEED: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);
static RNG_INIT: Once = Once::new();

fn rng_next() -> u64 {
    RNG_INIT.call_once(|| {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        RNG_SEED.store(t ^ 0x9E37_79B9_7F4A_7C15, Ordering::Relaxed);
    });
    let mut x = RNG_SEED.load(Ordering::Relaxed);
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    RNG_SEED.store(x, Ordering::Relaxed);
    x.wrapping_mul(0x2545_F491_4F6C_DD1D)
}

/// 在 ±max_px 内生成随机整数偏移（拟人化点击；max_px=0 时无偏移）
fn jitter_offset(max_px: u32) -> (i32, i32) {
    if max_px == 0 {
        return (0, 0);
    }
    let span = max_px as u64 * 2 + 1;
    let dx = (rng_next() % span) as i32 - max_px as i32;
    let dy = (rng_next() % span) as i32 - max_px as i32;
    (dx, dy)
}

/// 点击前随机延时（秒）：在 [click_delay_min, click_delay] 内均匀分布（纳秒精度）。
/// 未配置（click_delay 缺省/≤0）或区间非法（min ≥ max）时返回 0（不延时）。
fn random_click_delay(step: &Step) -> f64 {
    let max = step.click_delay.unwrap_or(0.0);
    if max <= 0.0 {
        return 0.0;
    }
    let min = step.click_delay_min.unwrap_or(0.0).clamp(0.0, max);
    if min >= max {
        return 0.0;
    }
    // 用纳秒整数做区间采样，避免浮点取模误差；结果再换算回秒
    let span_ns = ((max - min) * 1e9) as u64;
    min + (rng_next() % span_ns) as f64 / 1e9
}

/// 把扁平步骤编译成指令序列（配对控制结构、填充跳转）
fn compile(steps: &[Step]) -> Result<Vec<Instr>> {
    enum Ctl {
        Repeat(usize),
        If { start: usize, else_idx: Option<usize> },
    }
    let mut instrs: Vec<Instr> = Vec::new();
    let mut stack: Vec<Ctl> = Vec::new();

    for step in steps {
        match step.action.as_str() {
            "repeat" => {
                let idx = instrs.len();
                instrs.push(Instr { step: step.clone(), kind: Kind::Repeat { end: usize::MAX } });
                stack.push(Ctl::Repeat(idx));
            }
            "end_repeat" => {
                let repeat_idx = match stack.pop() {
                    Some(Ctl::Repeat(i)) => i,
                    _ => bail!("end_repeat 无匹配的 repeat"),
                };
                let end_idx = instrs.len();
                if let Kind::Repeat { end } = &mut instrs[repeat_idx].kind {
                    *end = end_idx;
                }
                instrs.push(Instr { step: step.clone(), kind: Kind::EndRepeat { back: repeat_idx } });
            }
            "if_image" | "if_text" => {
                let idx = instrs.len();
                instrs.push(Instr {
                    step: step.clone(),
                    kind: Kind::IfImage { else_or_end: usize::MAX, end: usize::MAX },
                });
                stack.push(Ctl::If { start: idx, else_idx: None });
            }
            "else" => {
                let if_idx = match stack.last() {
                    Some(Ctl::If { start, .. }) => *start,
                    _ => bail!("else 无匹配的 if_image"),
                };
                let else_idx = instrs.len();
                // 记录 else 指令索引（end_if 时回填 Else.end）
                if let Some(Ctl::If { else_idx: slot, .. }) = stack.last_mut() {
                    *slot = Some(else_idx);
                }
                if let Kind::IfImage { else_or_end, .. } = &mut instrs[if_idx].kind {
                    *else_or_end = else_idx;
                }
                instrs.push(Instr { step: step.clone(), kind: Kind::Else { end: usize::MAX } });
            }
            "end_if" => {
                let (if_idx, else_idx) = match stack.pop() {
                    Some(Ctl::If { start, else_idx }) => (start, else_idx),
                    _ => bail!("end_if 无匹配的 if_image"),
                };
                let end_idx = instrs.len();
                if let Kind::IfImage { else_or_end, end } = &mut instrs[if_idx].kind {
                    if *else_or_end == usize::MAX {
                        *else_or_end = end_idx;
                    }
                    *end = end_idx;
                }
                // 若存在 else，把其跳转也指向 end_if
                if let Some(ei) = else_idx {
                    if let Kind::Else { end } = &mut instrs[ei].kind {
                        *end = end_idx;
                    }
                }
                instrs.push(Instr { step: step.clone(), kind: Kind::EndIf });
            }
            _ => {
                instrs.push(Instr { step: step.clone(), kind: Kind::Action });
            }
        }
    }
    if !stack.is_empty() {
        bail!("存在未闭合的控制结构（缺少 end_repeat / end_if）");
    }
    Ok(instrs)
}

fn req_xy(step: &Step) -> Result<(i32, i32)> {
    let x = step.x.ok_or_else(|| anyhow!("动作 {} 缺少 x 坐标", step.action))?;
    let y = step.y.ok_or_else(|| anyhow!("动作 {} 缺少 y 坐标", step.action))?;
    Ok((x, y))
}

fn req_precision(step: &Step) -> f64 {
    step.precision.unwrap_or(0.85)
}

fn req_timeout(step: &Step) -> Duration {
    Duration::from_secs_f64(step.timeout.unwrap_or(15.0))
}

/// 供外部使用的场景运行入口
pub fn run_scenario(scenario_path: &Path, assets_dir: PathBuf) -> Result<bool> {
    let scenario = Scenario::load(scenario_path)?;
    let name = scenario
        .meta
        .name
        .clone()
        .unwrap_or_else(|| "未命名场景".to_string());
    let mut engine = Engine::new(&name, assets_dir);
    engine.run(&scenario)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(action: &str) -> Step {
        toml::from_str(&format!("action = \"{action}\"")).expect("构造步骤失败")
    }

    fn kind_of(instr: &Instr) -> &Kind {
        &instr.kind
    }

    #[test]
    fn compile_simple_actions() {
        let steps = vec![step("wait"), step("click"), step("screenshot")];
        let instrs = compile(&steps).unwrap();
        assert_eq!(instrs.len(), 3);
        for i in 0..3 {
            assert!(matches!(kind_of(&instrs[i]), Kind::Action));
        }
    }

    #[test]
    fn compile_repeat_pairs_jumps() {
        let steps = vec![step("repeat"), step("click"), step("end_repeat")];
        let instrs = compile(&steps).unwrap();
        assert_eq!(instrs.len(), 3);
        match kind_of(&instrs[0]) {
            Kind::Repeat { end } => assert_eq!(*end, 2),
            _ => panic!("repeat 应为 Repeat"),
        }
        match kind_of(&instrs[2]) {
            Kind::EndRepeat { back } => assert_eq!(*back, 0),
            _ => panic!("end_repeat 应为 EndRepeat"),
        }
    }

    #[test]
    fn compile_if_else_fills_jumps() {
        let steps = vec![
            step("if_image"),
            step("click"),
            step("else"),
            step("wait"),
            step("end_if"),
        ];
        let instrs = compile(&steps).unwrap();
        assert_eq!(instrs.len(), 5);
        match kind_of(&instrs[0]) {
            Kind::IfImage { else_or_end, end } => {
                assert_eq!(*else_or_end, 2, "else 分支位置");
                assert_eq!(*end, 4, "end_if 位置");
            }
            _ => panic!("if_image 应为 IfImage"),
        }
        match kind_of(&instrs[2]) {
            Kind::Else { end } => assert_eq!(*end, 4),
            _ => panic!("else 应为 Else"),
        }
    }

    #[test]
    fn compile_if_text_uses_same_branch_as_if_image() {
        // if_text 与 if_image 共用条件分支编译（跳转填充逻辑一致）
        let steps = vec![step("if_text"), step("click"), step("end_if")];
        let instrs = compile(&steps).unwrap();
        assert_eq!(instrs.len(), 3);
        match kind_of(&instrs[0]) {
            Kind::IfImage { else_or_end, end } => {
                assert_eq!(*else_or_end, 2);
                assert_eq!(*end, 2);
            }
            _ => panic!("if_text 应编译为条件分支"),
        }
    }

    #[test]
    fn compile_if_without_else_jumps_to_end() {
        let steps = vec![step("if_image"), step("click"), step("end_if")];
        let instrs = compile(&steps).unwrap();
        match kind_of(&instrs[0]) {
            Kind::IfImage { else_or_end, end } => {
                assert_eq!(*else_or_end, 2);
                assert_eq!(*end, 2);
            }
            _ => panic!("if_image 应为 IfImage"),
        }
    }

    #[test]
    fn compile_unclosed_if_is_error() {
        let steps = vec![step("if_image"), step("click")];
        assert!(compile(&steps).is_err(), "缺少 end_if 应报错");
    }

    #[test]
    fn compile_stray_end_is_error() {
        assert!(compile(&[step("end_repeat")]).is_err());
        assert!(compile(&[step("end_if")]).is_err());
        assert!(compile(&[step("else")]).is_err());
    }

    #[test]
    fn compile_nested_repeat_and_if() {
        let steps = vec![
            step("repeat"),      // 0
            step("if_image"),    // 1
            step("click"),       // 2
            step("end_if"),      // 3
            step("end_repeat"),  // 4
        ];
        let instrs = compile(&steps).unwrap();
        assert_eq!(instrs.len(), 5);
        match kind_of(&instrs[0]) {
            Kind::Repeat { end } => assert_eq!(*end, 4),
            _ => panic!(),
        }
        match kind_of(&instrs[1]) {
            Kind::IfImage { else_or_end, end } => {
                assert_eq!(*else_or_end, 3);
                assert_eq!(*end, 3);
            }
            _ => panic!(),
        }
        match kind_of(&instrs[4]) {
            Kind::EndRepeat { back } => assert_eq!(*back, 0),
            _ => panic!(),
        }
    }

    #[test]
    fn diff_image_pads_to_max_height() {
        use image::RgbaImage;
        let old = RgbaImage::from_pixel(20, 10, Rgba([255, 0, 0, 255]));
        let new = RgbaImage::from_pixel(30, 14, Rgba([0, 0, 255, 255]));
        let diff = build_diff_image(&old, &new);
        // 宽 = 20 + 30 + 分隔 10；高取最大 14
        assert_eq!(diff.width(), 60);
        assert_eq!(diff.height(), 14);
        // 左侧应为旧模板红色像素
        assert_eq!(diff.get_pixel(0, 0), &Rgba([255, 0, 0, 255]));
        // 右侧应为新模板蓝色像素（偏移 = 20 + 10）
        assert_eq!(diff.get_pixel(30, 0), &Rgba([0, 0, 255, 255]));
    }

    #[test]
    fn jitter_zero_has_no_offset() {
        assert_eq!(jitter_offset(0), (0, 0));
    }

    #[test]
    fn jitter_stays_in_range_and_varies() {
        let mut saw_neg = false;
        let mut saw_pos = false;
        for _ in 0..1000 {
            let (dx, dy) = jitter_offset(10);
            assert!(dx >= -10 && dx <= 10, "dx={dx} 越界");
            assert!(dy >= -10 && dy <= 10, "dy={dy} 越界");
            if dx < 0 {
                saw_neg = true;
            }
            if dx > 0 {
                saw_pos = true;
            }
        }
        assert!(saw_neg && saw_pos, "偏移应覆盖正负两侧，保证点击位置动态分布");
    }

    #[test]
    fn click_delay_not_configured_returns_zero() {
        let s = Step { action: "click".into(), ..Default::default() };
        assert_eq!(random_click_delay(&s), 0.0);
    }

    #[test]
    fn click_delay_invalid_range_returns_zero() {
        // min >= max 视为区间非法，不延时
        let s = Step {
            action: "click".into(),
            click_delay: Some(0.2),
            click_delay_min: Some(0.3),
            ..Default::default()
        };
        assert_eq!(random_click_delay(&s), 0.0);
        // max <= 0 不延时
        let s2 = Step { action: "click".into(), click_delay: Some(0.0), ..Default::default() };
        assert_eq!(random_click_delay(&s2), 0.0);
    }

    #[test]
    fn click_delay_stays_in_range() {
        let s = Step {
            action: "click".into(),
            click_delay: Some(0.5),
            click_delay_min: Some(0.1),
            ..Default::default()
        };
        let mut saw_low = false;
        let mut saw_high = false;
        for _ in 0..1000 {
            let d = random_click_delay(&s);
            assert!((0.1..=0.5).contains(&d), "延时 {d} 越界 [0.1, 0.5]");
            if d < 0.2 {
                saw_low = true;
            }
            if d > 0.4 {
                saw_high = true;
            }
        }
        assert!(saw_low && saw_high, "延时应在区间内动态分布，而非固定值");
    }

    #[test]
    fn click_delay_min_defaults_to_zero() {
        let s = Step { action: "click".into(), click_delay: Some(0.5), ..Default::default() };
        for _ in 0..1000 {
            let d = random_click_delay(&s);
            assert!((0.0..=0.5).contains(&d), "延时 {d} 越界 [0, 0.5]");
        }
    }
}
