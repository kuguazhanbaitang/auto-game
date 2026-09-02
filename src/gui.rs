//! GUI 录制器：egui 桌面界面（`auto-game gui`）
//!
//! 目标：把「手写 TOML + 采集模板 + 量坐标」变成「录一遍就有」——
//! - **实时画面预览**：全屏/窗口模式截图显示在界面中；
//! - **录制**：用户手动操作游戏，后台线程捕获点击/按键 → 自动生成场景步骤；
//! - **模板采集**：在预览图上框选区域 → 生成模板 PNG 到 assets/；
//! - **步骤编辑**：列表增删改、调参数（x/y/jitter/image/precision/region…）；
//! - **导出 + 运行**：一键导出 TOML 场景，复用引擎直接运行。
//!
//! 录制原理：复用 device_query（与 failsafe 同源）轮询全局鼠标/键盘，
//! 检测左键「按下→释放」为一次点击、按键边沿为一次按键，事件经 channel 回 GUI。
//! 注意：录制期间请把焦点放在游戏窗口上操作，避免在 GUI 里按键盘被误录。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use device_query::{DeviceQuery, DeviceState, Keycode};
use eframe::egui;
use image::{RgbaImage, imageops::crop_imm};

use crate::adapter::{CaptureBackend, CaptureTrait, Region};
use crate::script::Step;

/// 启动 GUI（阻塞直到窗口关闭）
pub fn run_gui(assets_dir: PathBuf) -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 840.0])
            .with_min_inner_size([960.0, 640.0])
            .with_title("auto-game · GUI 录制器"),
        ..Default::default()
    };
    eframe::run_native(
        "auto-game-gui",
        options,
        Box::new(move |cc| Ok(Box::new(GuiApp::new(cc, assets_dir)))),
    )
    .map_err(|e| anyhow::anyhow!("GUI 启动失败: {e}"))
}

// ---------------- 录制事件 ----------------

enum RecEvent {
    Click { x: i32, y: i32 },
    Key { name: String },
}

/// device_query Keycode → 我们的 key 名（与 key_from_str 输入一致）；
/// 返回 None 的键（功能键 F13+、小键盘、标点）不录制。
fn keycode_to_keyname(k: Keycode) -> Option<String> {
    use Keycode::*;
    // 字母与数字（显式列出，枚举不支持 range pattern）
    let ch = match k {
        A => Some('a'), B => Some('b'), C => Some('c'), D => Some('d'), E => Some('e'),
        F => Some('f'), G => Some('g'), H => Some('h'), I => Some('i'), J => Some('j'),
        K => Some('k'), L => Some('l'), M => Some('m'), N => Some('n'), O => Some('o'),
        P => Some('p'), Q => Some('q'), R => Some('r'), S => Some('s'), T => Some('t'),
        U => Some('u'), V => Some('v'), W => Some('w'), X => Some('x'), Y => Some('y'),
        Z => Some('z'),
        Key0 => Some('0'), Key1 => Some('1'), Key2 => Some('2'), Key3 => Some('3'),
        Key4 => Some('4'), Key5 => Some('5'), Key6 => Some('6'), Key7 => Some('7'),
        Key8 => Some('8'), Key9 => Some('9'),
        _ => None,
    };
    if let Some(c) = ch {
        return Some(c.to_string());
    }
    // 功能键 / 控制键
    let name = match k {
        F1 => "f1", F2 => "f2", F3 => "f3", F4 => "f4", F5 => "f5", F6 => "f6",
        F7 => "f7", F8 => "f8", F9 => "f9", F10 => "f10", F11 => "f11", F12 => "f12",
        Enter => "enter", Escape => "escape", Space => "space", Tab => "tab",
        Backspace => "backspace", Delete => "delete", Insert => "insert",
        Home => "home", End => "end", PageUp => "pageup", PageDown => "pagedown",
        Up => "up", Down => "down", Left => "left", Right => "right",
        LControl | RControl => "ctrl", LShift | RShift => "shift", LAlt | RAlt => "alt",
        LMeta | RMeta | Command | RCommand | LOption | ROption => "meta",
        _ => return None,
    };
    Some(name.to_string())
}


/// 安全裁剪全屏中的区域为模板图（clamp 到屏幕内，避免负坐标/超界 panic）。
fn safe_crop(x: i32, y: i32, w: u32, h: u32) -> Result<RgbaImage> {
    let full = CaptureBackend.capture_full()?;
    let (fw, fh) = (full.width() as i64, full.height() as i64);
    let mut x = x as i64;
    let mut y = y as i64;
    let mut w = w as i64;
    let mut h = h as i64;
    x = x.max(0);
    y = y.max(0);
    if x + w > fw {
        w = (fw - x).max(0);
    }
    if y + h > fh {
        h = (fh - y).max(0);
    }
    if w <= 0 || h <= 0 {
        anyhow::bail!("裁剪区域超出屏幕范围");
    }
    Ok(crop_imm(&full, x as u32, y as u32, w as u32, h as u32).to_image())
}

/// 后台录制线程：轮询全局输入，检测点击/按键边沿，发送事件。
fn spawn_recorder(stop: Arc<AtomicBool>, tx: Sender<RecEvent>) -> Option<std::thread::JoinHandle<()>> {
    let device = DeviceState::checked_new()?;
    Some(std::thread::spawn(move || {
        let mut prev_left = false;
        let mut held: Vec<Keycode> = Vec::new();
        while !stop.load(Ordering::Relaxed) {
            let mouse = device.get_mouse();
            let left = mouse.button_pressed.get(1).copied().unwrap_or(false);
            if left && !prev_left {
                let (x, y) = mouse.coords;
                let _ = tx.send(RecEvent::Click { x, y });
            }
            prev_left = left;

            let keys = device.get_keys();
            for k in &keys {
                if !held.contains(k) {
                    if let Some(name) = keycode_to_keyname(*k) {
                        let _ = tx.send(RecEvent::Key { name });
                    }
                }
            }
            held = keys;
            std::thread::sleep(Duration::from_millis(8));
        }
    }))
}

// ---------------- GUI 应用 ----------------

pub struct GuiApp {
    /// 已录/编辑的步骤
    steps: Vec<Step>,
    /// 录制状态
    recording: bool,
    rec_stop: Option<Arc<AtomicBool>>,
    rec_rx: Option<Receiver<RecEvent>>,
    rec_handle: Option<std::thread::JoinHandle<()>>,
    /// 预览截图
    preview_tex: Option<egui::TextureHandle>,
    last_shot: Option<RgbaImage>,
    last_shot_time: Instant,
    /// 预览截图在中央面板中的显示尺寸（用于坐标映射）
    shot_display: Option<(egui::Rect, usize, usize)>,
    /// 框选模板
    crop_start: Option<egui::Pos2>,
    crop_end: Option<egui::Pos2>,
    /// 配置
    window_kw: String,
    use_window: bool,
    scenario_name: String,
    assets_dir: PathBuf,
    /// 状态/日志
    status: String,
    /// 运行场景线程
    run_handle: Option<std::thread::JoinHandle<()>>,
    run_rx: Option<Receiver<bool>>,
}

impl GuiApp {
    fn new(cc: &eframe::CreationContext<'_>, assets_dir: PathBuf) -> Self {
        cc.egui_ctx.set_pixels_per_point(1.0);
        Self {
            steps: Vec::new(),
            recording: false,
            rec_stop: None,
            rec_rx: None,
            rec_handle: None,
            preview_tex: None,
            last_shot: None,
            last_shot_time: Instant::now() - Duration::from_secs(1),
            shot_display: None,
            crop_start: None,
            crop_end: None,
            window_kw: String::new(),
            use_window: false,
            scenario_name: "未命名场景".to_string(),
            assets_dir,
            status: "就绪：点击「开始录制」后操作游戏，点击/按键将自动生成步骤".to_string(),
            run_handle: None,
            run_rx: None,
        }
    }

    /// 采集当前画面（全屏或窗口模式）
    fn capture_screen(&self) -> Result<RgbaImage> {
        if self.use_window && !self.window_kw.trim().is_empty() {
            let shot = CaptureBackend.capture_window(self.window_kw.trim())?;
            Ok(shot.image)
        } else {
            Ok(CaptureBackend.capture_full()?)
        }
    }

    /// 刷新预览纹理（限制频率）
    fn refresh_preview(&mut self, ctx: &egui::Context, force: bool) {
        let interval = if self.recording {
            Duration::from_millis(150)
        } else {
            Duration::from_millis(500)
        };
        if !force && self.last_shot_time.elapsed() < interval {
            return;
        }
        match self.capture_screen() {
            Ok(img) => {
                let size = [img.width() as usize, img.height() as usize];
                let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
                self.preview_tex = Some(ctx.load_texture(
                    "preview",
                    color,
                    egui::TextureOptions::LINEAR,
                ));
                self.last_shot = Some(img);
            }
            Err(e) => {
                self.status = format!("预览截图失败: {e:#}");
            }
        }
        self.last_shot_time = Instant::now();
    }

    /// 开始录制：拉起后台线程
    fn start_recording(&mut self) {
        if self.recording {
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let handle = match spawn_recorder(stop.clone(), tx) {
            Some(h) => h,
            None => {
                self.status = "录制启动失败：无法读取键盘/鼠标状态（非交互会话？）".to_string();
                return;
            }
        };
        self.rec_stop = Some(stop);
        self.rec_rx = Some(rx);
        self.rec_handle = Some(handle);
        self.recording = true;
        self.status = "正在录制：请操作游戏窗口，点击/按键将生成步骤；点「停止录制」结束".to_string();
    }

    fn stop_recording(&mut self) {
        if let Some(stop) = &self.rec_stop {
            stop.store(true, Ordering::Relaxed);
        }
        if let Some(h) = self.rec_handle.take() {
            let _ = h.join();
        }
        self.rec_stop = None;
        self.rec_rx = None;
        self.recording = false;
        self.status = "录制结束".to_string();
    }

    /// 从 channel 拉取录制事件并追加步骤
    fn drain_events(&mut self) {
        let Some(rx) = &self.rec_rx else { return };
        let mut events = Vec::new();
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
        for e in events {
            match e {
                RecEvent::Click { x, y } => {
                    self.steps.push(Step {
                        action: "click".to_string(),
                        x: Some(x),
                        y: Some(y),
                        ..Default::default()
                    });
                    self.status = format!("录制：点击 ({x}, {y})");
                }
                RecEvent::Key { name } => {
                    self.steps.push(Step {
                        action: "key_press".to_string(),
                        key: Some(name.clone()),
                        ..Default::default()
                    });
                    self.status = format!("录制：按键 {name}");
                }
            }
        }
    }

    /// 把第 idx 个步骤转为「click_image」（以点击坐标为中心截 64×64 模板）
    fn convert_to_template(&mut self, idx: usize) {
        let Some(step) = self.steps.get(idx) else { return };
        let (Some(x), Some(y)) = (step.x, step.y) else {
            self.status = "仅坐标点击步骤可转为模板".to_string();
            return;
        };
        let half = 32i32;
        let (rx, ry) = (x - half, y - half);
        match safe_crop(rx, ry, 64, 64) {
            Ok(img) => {
                let name = format!("tpl_{idx}.png");
                let path = self.assets_dir.join(&name);
                if let Err(e) = std::fs::create_dir_all(&self.assets_dir) {
                    self.status = format!("创建模板目录失败: {e:#}");
                    return;
                }
                if let Err(e) = img.save(&path) {
                    self.status = format!("保存模板失败: {e:#}");
                    return;
                }
                let mut s = Step {
                    action: "click_image".to_string(),
                    image: Some(name.clone()),
                    precision: Some(0.85),
                    region: Some(Region { x: rx, y: ry, w: 64, h: 64 }),
                    ..Default::default()
                };
                // 保留 jitter / click_delay 拟人化参数
                let old = self.steps[idx].clone();
                s.jitter = old.jitter;
                s.click_delay = old.click_delay;
                s.click_delay_min = old.click_delay_min;
                self.steps[idx] = s;
                self.status = format!("已转为模板 {name}（以点击 ({x},{y}) 为中心截 64×64）");
            }
            Err(e) => self.status = format!("截取模板失败: {e:#}"),
        }
    }

    /// 导出场景为 TOML 字符串
    fn export_toml(&self) -> String {
        let mut s = String::new();
        s.push_str("[meta]\n");
        s.push_str(&format!("name = \"{}\"\n", self.scenario_name));
        if self.use_window && !self.window_kw.trim().is_empty() {
            s.push_str(&format!("window = \"{}\"\n", self.window_kw.trim()));
        }
        s.push('\n');
        for step in &self.steps {
            s.push_str("[[step]]\n");
            s.push_str(&format!("action = \"{}\"\n", step.action));
            if let Some(v) = &step.image {
                s.push_str(&format!("image = \"{}\"\n", v));
            }
            if let Some(v) = step.precision {
                s.push_str(&format!("precision = {}\n", v));
            }
            if let Some(v) = step.timeout {
                s.push_str(&format!("timeout = {}\n", v));
            }
            if let Some(v) = step.x {
                s.push_str(&format!("x = {}\n", v));
            }
            if let Some(v) = step.y {
                s.push_str(&format!("y = {}\n", v));
            }
            if let Some(v) = step.jitter {
                s.push_str(&format!("jitter = {}\n", v));
            }
            if let Some(v) = step.click_delay {
                s.push_str(&format!("click_delay = {}\n", v));
            }
            if let Some(v) = step.click_delay_min {
                s.push_str(&format!("click_delay_min = {}\n", v));
            }
            if let Some(v) = &step.text {
                s.push_str(&format!("text = \"{}\"\n", v));
            }
            if let Some(v) = &step.key {
                s.push_str(&format!("key = \"{}\"\n", v));
            }
            if let Some(v) = step.seconds {
                s.push_str(&format!("seconds = {}\n", v));
            }
            if let Some(v) = &step.keys {
                let joined = v
                    .iter()
                    .map(|k| format!("\"{k}\""))
                    .collect::<Vec<_>>()
                    .join(", ");
                s.push_str(&format!("keys = [{joined}]\n"));
            }
            if let Some(r) = step.region {
                s.push_str(&format!(
                    "region = {{ x = {}, y = {}, w = {}, h = {} }}\n",
                    r.x, r.y, r.w, r.h
                ));
            }
            if let Some(v) = step.verify_exact {
                s.push_str(&format!("verify_exact = {}\n", v));
            }
            s.push('\n');
        }
        s
    }

    /// 导出场景到 scenarios/<name>.toml
    fn save_scenario(&self) -> Result<PathBuf> {
        let dir = PathBuf::from("scenarios");
        std::fs::create_dir_all(&dir)?;
        let safe: String = self
            .scenario_name
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
            .collect();
        let path = dir.join(format!("{safe}.toml"));
        std::fs::write(&path, self.export_toml())?;
        Ok(path)
    }

    /// 在后台线程运行场景（复用引擎），结果经 channel 回 GUI
    fn run_scenario(&mut self) {
        if self.run_handle.is_some() {
            self.status = "已有场景在运行中".to_string();
            return;
        }
        let path = match self.save_scenario() {
            Ok(p) => p,
            Err(e) => {
                self.status = format!("导出场景失败: {e:#}");
                return;
            }
        };
        let assets = self.assets_dir.clone();
        let display = path.display().to_string();
        let (tx, rx) = channel();
        let handle = std::thread::spawn(move || {
            let passed = match crate::engine::run_scenario(&path, assets) {
                Ok(p) => p,
                Err(_) => false,
            };
            let _ = tx.send(passed);
        });
        self.run_handle = Some(handle);
        self.run_rx = Some(rx);
        self.status = format!(
            "正在运行场景：{display}（注意：引擎会接管鼠标键盘，按 F9 可中止）"
        );
    }

    fn poll_run(&mut self) {
        if self.run_handle.is_none() {
            return;
        }
        if let Some(rx) = &self.run_rx {
            if let Ok(passed) = rx.try_recv() {
                if let Some(h) = self.run_handle.take() {
                    let _ = h.join();
                }
                self.run_rx = None;
                self.status = if passed {
                    "✅ 场景运行完成：全部通过".to_string()
                } else {
                    "❌ 场景运行完成：存在失败步骤（详见控制台报告）".to_string()
                };
            }
        }
    }

    // ---------------- UI ----------------

    fn ui_topbar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("topbar").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("auto-game · GUI 录制器");
                ui.separator();

                // 录制控制
                if self.recording {
                    if ui.button("⏹ 停止录制").clicked() {
                        self.stop_recording();
                    }
                } else {
                    if ui.button("⏺ 开始录制").clicked() {
                        self.start_recording();
                    }
                }
                ui.separator();

                // 窗口模式
                ui.checkbox(&mut self.use_window, "窗口模式");
                ui.add_enabled(
                    self.use_window,
                    egui::TextEdit::singleline(&mut self.window_kw)
                        .hint_text("窗口标题关键字（如 龙之谷 / MuMu）"),
                );
                ui.separator();

                // 场景名 + 导出/运行
                ui.label("场景名");
                ui.add(
                    egui::TextEdit::singleline(&mut self.scenario_name).desired_width(160.0),
                );
                if ui.button("💾 导出 TOML").clicked() {
                    match self.save_scenario() {
                        Ok(p) => self.status = format!("场景已导出: {}", p.display()),
                        Err(e) => self.status = format!("导出失败: {e:#}"),
                    }
                }
                if ui.button("▶ 运行场景").clicked() {
                    self.run_scenario();
                }
            });
        });
    }

    fn ui_steps_panel(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("steps")
            .resizable(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong(format!("步骤（{}）", self.steps.len()));
                    if ui.small_button("清空").clicked() {
                        self.steps.clear();
                        self.status = "已清空步骤".to_string();
                    }
                    if ui.small_button("添加等待").clicked() {
                        self.steps.push(Step {
                            action: "wait".to_string(),
                            seconds: Some(1.0),
                            ..Default::default()
                        });
                    }
                    if ui.small_button("添加截图").clicked() {
                        self.steps.push(Step {
                            action: "screenshot".to_string(),
                            ..Default::default()
                        });
                    }
                });
                ui.separator();

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let mut remove: Option<usize> = None;
                        let mut move_up: Option<usize> = None;
                        let mut move_down: Option<usize> = None;
                        let mut to_tpl: Option<usize> = None;

                        for (i, step) in self.steps.iter_mut().enumerate() {
                            egui::Frame::group(ui.style()).show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(format!("#{}", i + 1));
                                    let mut action = step.action.clone();
                                    let resp = egui::ComboBox::from_id_salt(("action", i))
                                        .selected_text(&action)
                                        .show_ui(ui, |ui| {
                                            for a in [
                                                "click",
                                                "click_image",
                                                "click_text",
                                                "wait_image",
                                                "assert_image",
                                                "assert_text",
                                                "find_image",
                                                "ocr_text",
                                                "wait",
                                                "key_press",
                                                "key_combo",
                                                "type_text",
                                                "move_mouse",
                                                "screenshot",
                                            ] {
                                                ui.selectable_value(&mut action, a.to_string(), a);
                                            }
                                        });
                                    if resp.response.changed() {
                                        step.action = action;
                                    }
                                    if ui.small_button("✖").on_hover_text("删除").clicked() {
                                        remove = Some(i);
                                    }
                                    if ui.small_button("↑").on_hover_text("上移").clicked() {
                                        move_up = Some(i);
                                    }
                                    if ui.small_button("↓").on_hover_text("下移").clicked() {
                                        move_down = Some(i);
                                    }
                                });

                                ui.horizontal_wrapped(|ui| {
                                    match step.action.as_str() {
                                        "click" | "move_mouse" => {
                                            ui.label("x");
                                            ui.add(egui::DragValue::new(step.x.get_or_insert(0)));
                                            ui.label("y");
                                            ui.add(egui::DragValue::new(step.y.get_or_insert(0)));
                                            if step.action == "click" {
                                                ui.label("jitter");
                                                ui.add(egui::DragValue::new(
                                                    step.jitter.get_or_insert(0),
                                                ));
                                                ui.label("延时");
                                                ui.add(
                                                    egui::DragValue::new(
                                                        step.click_delay.get_or_insert(0.0),
                                                    )
                                                    .speed(0.01),
                                                );
                                                if ui.small_button("→模板").on_hover_text("以点击坐标为中心截 64×64 模板并转为 click_image").clicked() {
                                                    to_tpl = Some(i);
                                                }
                                            }
                                        }
                                        "click_image" | "wait_image" | "assert_image"
                                        | "find_image" => {
                                            ui.label("image");
                                            ui.add(
                                                egui::TextEdit::singleline(
                                                    step.image.get_or_insert_with(String::new),
                                                )
                                                .desired_width(130.0),
                                            );
                                            ui.label("精度");
                                            ui.add(
                                                egui::DragValue::new(
                                                    step.precision.get_or_insert(0.85),
                                                )
                                                .speed(0.01)
                                                .range(0.0..=1.0),
                                            );
                                            if step.action == "click_image" {
                                                ui.label("jitter");
                                                ui.add(egui::DragValue::new(
                                                    step.jitter.get_or_insert(0),
                                                ));
                                            }
                                            ui.label("timeout");
                                            ui.add(
                                                egui::DragValue::new(
                                                    step.timeout.get_or_insert(15.0),
                                                )
                                                .speed(0.5),
                                            );
                                        }
                                        "click_text" | "assert_text" => {
                                            ui.label("text");
                                            ui.add(
                                                egui::TextEdit::singleline(
                                                    step.text.get_or_insert_with(String::new),
                                                )
                                                .desired_width(130.0),
                                            );
                                            if step.action == "click_text" {
                                                ui.label("jitter");
                                                ui.add(egui::DragValue::new(
                                                    step.jitter.get_or_insert(0),
                                                ));
                                            }
                                            if step.action == "assert_text" {
                                                ui.label("timeout");
                                                ui.add(
                                                    egui::DragValue::new(
                                                        step.timeout.get_or_insert(15.0),
                                                    )
                                                    .speed(0.5),
                                                );
                                            }
                                        }
                                        "ocr_text" => {
                                            ui.label("region 可选：在预览图上框选后点「生成模板/区域」");
                                        }
                                        "wait" => {
                                            ui.label("seconds");
                                            ui.add(
                                                egui::DragValue::new(
                                                    step.seconds.get_or_insert(1.0),
                                                )
                                                .speed(0.1),
                                            );
                                        }
                                        "key_press" => {
                                            ui.label("key");
                                            ui.add(
                                                egui::TextEdit::singleline(
                                                    step.key.get_or_insert_with(String::new),
                                                )
                                                .desired_width(100.0),
                                            );
                                        }
                                        "key_combo" => {
                                            ui.label("keys 如 ctrl,a（逗号分隔）");
                                            let joined = step
                                                .keys
                                                .clone()
                                                .unwrap_or_default()
                                                .join(",");
                                            let mut buf = joined;
                                            let resp = ui.add(
                                                egui::TextEdit::singleline(&mut buf)
                                                    .desired_width(140.0),
                                            );
                                            if resp.changed() {
                                                step.keys = Some(
                                                    buf.split(',')
                                                        .map(|s| s.trim().to_string())
                                                        .filter(|s| !s.is_empty())
                                                        .collect(),
                                                );
                                            }
                                        }
                                        "type_text" => {
                                            ui.label("text");
                                            ui.add(
                                                egui::TextEdit::singleline(
                                                    step.text.get_or_insert_with(String::new),
                                                )
                                                .desired_width(160.0),
                                            );
                                        }
                                        _ => {}
                                    }
                                });

                                // region 编辑（图像类）
                                if matches!(
                                    step.action.as_str(),
                                    "click_image"
                                        | "wait_image"
                                        | "assert_image"
                                        | "find_image"
                                        | "ocr_text"
                                        | "click_text"
                                        | "assert_text"
                                ) {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label("region");
                                        let r = step.region.get_or_insert(Region {
                                            x: 0,
                                            y: 0,
                                            w: 0,
                                            h: 0,
                                        });
                                        ui.label("x");
                                        ui.add(egui::DragValue::new(&mut r.x));
                                        ui.label("y");
                                        ui.add(egui::DragValue::new(&mut r.y));
                                        ui.label("w");
                                        ui.add(egui::DragValue::new(&mut r.w));
                                        ui.label("h");
                                        ui.add(egui::DragValue::new(&mut r.h));
                                    });
                                }
                            });
                        }

                        // 应用行操作（先收集后应用，避免借用冲突）
                        if let Some(i) = to_tpl {
                            self.convert_to_template(i);
                        }
                        if let Some(i) = remove {
                            self.steps.remove(i);
                        }
                        if let Some(i) = move_up {
                            if i > 0 {
                                self.steps.swap(i, i - 1);
                            }
                        }
                        if let Some(i) = move_down {
                            if i + 1 < self.steps.len() {
                                self.steps.swap(i, i + 1);
                            }
                        }
                    });
            });
    }

    fn ui_preview(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("画面预览");
                if ui.small_button("刷新").clicked() {
                    self.refresh_preview(&ctx, true);
                }
                if ui.small_button("清除框选").clicked() {
                    self.crop_start = None;
                    self.crop_end = None;
                }
                ui.label(&self.status);
            });
            ui.separator();

            if let Some(tex) = &self.preview_tex {
                let avail = ui.available_size();
                let img_size = tex.size_vec2();
                let scale = (avail.x / img_size.x).min(avail.y / img_size.y).min(1.0);
                let display_size = egui::vec2(img_size.x * scale, img_size.y * scale);
                let (rect, _) = ui.allocate_exact_size(display_size, egui::Sense::click_and_drag());
                self.shot_display = Some((
                    rect,
                    tex.size()[0],
                    tex.size()[1],
                ));
                ui.painter().image(
                    tex.id(),
                    rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    egui::Color32::WHITE,
                );

                // 框选交互：预览图内拖拽
                let response = ui.interact(rect, egui::Id::new("preview_crop"), egui::Sense::drag());
                if response.drag_started() {
                    self.crop_start = Some(response.interact_pointer_pos().unwrap_or(rect.min));
                    self.crop_end = None;
                }
                if response.dragged() {
                    self.crop_end = Some(response.interact_pointer_pos().unwrap_or(rect.min));
                }
                if response.drag_stopped() {
                    self.crop_end = Some(response.interact_pointer_pos().unwrap_or(rect.min));
                }

                // 绘制框选
                if let (Some(a), Some(b)) = (self.crop_start, self.crop_end) {
                    let r = egui::Rect::from_two_pos(a, b);
                    ui.painter().rect_stroke(
                        r,
                        0.0,
                        egui::Stroke::new(2.0, egui::Color32::from_rgb(0, 200, 80)),
                        egui::StrokeKind::Outside,
                    );
                    if r.width() >= 4.0 && r.height() >= 4.0 {
                        // 显示像素坐标信息
                        if let Some((drect, iw, ih)) = self.shot_display {
                            let px = ((r.min.x - drect.min.x) / drect.width() * iw as f32) as i32;
                            let py = ((r.min.y - drect.min.y) / drect.height() * ih as f32) as i32;
                            let pw = (r.width() / drect.width() * iw as f32) as u32;
                            let ph = (r.height() / drect.height() * ih as f32) as u32;
                            ui.painter().text(
                                r.min + egui::vec2(0.0, -16.0),
                                egui::Align2::LEFT_TOP,
                                format!("{px},{py} {pw}x{ph}"),
                                egui::FontId::monospace(12.0),
                                egui::Color32::from_rgb(0, 200, 80),
                            );
                        }
                    }
                }

                // 框选后生成模板/区域按钮
                if let (Some(a), Some(b)) = (self.crop_start, self.crop_end) {
                    let r = egui::Rect::from_two_pos(a, b);
                    if r.width() >= 8.0 && r.height() >= 8.0 {
                        ui.horizontal(|ui| {
                            let mut tpl_name = format!("tpl_{}.png", self.steps.len());
                            ui.label("模板名");
                            ui.add(egui::TextEdit::singleline(&mut tpl_name).desired_width(120.0));
                            if ui.button("保存模板").clicked() {
                                if let Some((drect, iw, ih)) = self.shot_display {
                                    let px = ((r.min.x - drect.min.x) / drect.width() * iw as f32) as i32;
                                    let py = ((r.min.y - drect.min.y) / drect.height() * ih as f32) as i32;
                                    let pw = (r.width() / drect.width() * iw as f32).max(1.0) as u32;
                                    let ph = (r.height() / drect.height() * ih as f32).max(1.0) as u32;
                                    self.save_template(&tpl_name, px, py, pw, ph);
                                }
                            }
                            if ui.button("插入 region").clicked() {
                                if let Some((drect, iw, ih)) = self.shot_display {
                                    let px = ((r.min.x - drect.min.x) / drect.width() * iw as f32) as i32;
                                    let py = ((r.min.y - drect.min.y) / drect.height() * ih as f32) as i32;
                                    let pw = (r.width() / drect.width() * iw as f32).max(1.0) as u32;
                                    let ph = (r.height() / drect.height() * ih as f32).max(1.0) as u32;
                                    self.steps.push(Step {
                                        action: "wait_image".to_string(),
                                        image: Some("请填写模板名.png".to_string()),
                                        region: Some(Region { x: px, y: py, w: pw, h: ph }),
                                        ..Default::default()
                                    });
                                    self.status = format!("已插入 wait_image 步骤（region {px},{py} {pw}x{ph}）");
                                }
                            }
                        });
                    }
                }
            } else {
                ui.label("（无画面：点击「刷新」或开始录制后自动刷新）");
            }
        });
    }

    /// 保存框选区域为模板 PNG 到 assets/<name>
    fn save_template(&mut self, name: &str, x: i32, y: i32, w: u32, h: u32) {
        match safe_crop(x, y, w, h) {
            Ok(img) => {
                if let Err(e) = std::fs::create_dir_all(&self.assets_dir) {
                    self.status = format!("创建模板目录失败: {e:#}");
                    return;
                }
                let fname = if name.ends_with(".png") {
                    name.to_string()
                } else {
                    format!("{name}.png")
                };
                let path = self.assets_dir.join(&fname);
                match img.save(&path) {
                    Ok(_) => self.status = format!(
                        "模板已保存: {}（{}x{} @ ({x},{y})），可在步骤的 image 字段引用",
                        path.display(),
                        img.width(),
                        img.height()
                    ),
                    Err(e) => self.status = format!("保存模板失败: {e:#}"),
                }
            }
            Err(e) => self.status = format!("截取模板失败: {e:#}"),
        }
    }
}

impl eframe::App for GuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // 录制中：拉取事件 + 高频刷新画面
        if self.recording {
            self.drain_events();
        }
        self.poll_run();
        self.refresh_preview(&ctx, false);

        self.ui_topbar(ui);
        self.ui_steps_panel(ui);
        self.ui_preview(ui);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.recording {
            self.stop_recording();
        }
    }
}
