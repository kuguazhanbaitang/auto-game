//! 流程引擎：按顺序执行场景中的动作，收集并输出报告

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use image::RgbaImage;

use crate::action::Actions;
use crate::adapter::Key;
use crate::report::{Report, Status};
use crate::script::{Scenario, Step};

/// 流程引擎
pub struct Engine {
    actions: Actions,
    report: Report,
    assets_dir: PathBuf,
    reports_dir: PathBuf,
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
        }
    }

    /// 运行整个场景，返回是否全部通过
    pub fn run(&mut self, scenario: &Scenario) -> Result<bool> {
        for (idx, step) in scenario.steps.iter().enumerate() {
            let start = Instant::now();
            let result = self.execute(step, idx + 1);
            let duration = start.elapsed();
            match result {
                Ok(detail) => {
                    self.report
                        .record(idx + 1, &step.action, detail, Status::Pass, duration);
                }
                Err(e) => {
                    self.report
                        .record(idx + 1, &step.action, format!("{e:#}"), Status::Fail, duration);
                }
            }
        }
        self.report.print();
        Ok(self.report.all_passed())
    }

    /// 分发到具体动作实现
    fn execute(&self, step: &Step, index: usize) -> Result<String> {
        match step.action.as_str() {
            "screenshot" => self.exec_screenshot(index),
            "wait" => self.exec_wait(step),
            "move_mouse" => self.exec_move_mouse(step),
            "click" => self.exec_click(step),
            "key_press" => self.exec_key_press(step),
            "type_text" => self.exec_type_text(step),
            "find_image" => self.exec_find_image(step),
            "wait_image" => self.exec_wait_image(step),
            "click_image" => self.exec_click_image(step),
            "assert_image" => self.exec_assert_image(step),
            other => bail!("未知动作类型: {other}"),
        }
    }

    // ---- 采集/输入类动作 ----

    fn exec_screenshot(&self, index: usize) -> Result<String> {
        let img = self.actions.capture_full()?;
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
        self.actions.move_mouse(x, y)?;
        self.actions.click()?;
        Ok(format!("点击坐标 ({x}, {y})"))
    }

    fn exec_key_press(&self, step: &Step) -> Result<String> {
        let key = step.key.as_deref().ok_or_else(|| anyhow!("key_press 缺少 key 参数"))?;
        let key = match key {
            "enter" => Key::Enter,
            "escape" => Key::Escape,
            "space" => Key::Space,
            other => bail!("暂不支持的按键: {other}"),
        };
        self.actions.key_press(key)?;
        Ok(format!("按下按键 {key:?}"))
    }

    fn exec_type_text(&self, step: &Step) -> Result<String> {
        let text = step.text.as_deref().ok_or_else(|| anyhow!("type_text 缺少 text 参数"))?;
        self.actions.type_text(text)?;
        Ok(format!("输入文本: {text}"))
    }

    // ---- 图像识别类动作 ----

    fn exec_find_image(&self, step: &Step) -> Result<String> {
        let template = self.load_template(step)?;
        let precision = req_precision(step);
        match self.actions.find_image(&template, precision)? {
            Some(m) => Ok(format!(
                "找到模板：位置 ({}, {}), 置信度 {:.4}",
                m.x, m.y, m.confidence
            )),
            None => Ok("未找到模板".to_string()),
        }
    }

    fn exec_wait_image(&self, step: &Step) -> Result<String> {
        let template = self.load_template(step)?;
        let precision = req_precision(step);
        let timeout = req_timeout(step);
        match self.actions.wait_image(&template, precision, timeout)? {
            Some(m) => Ok(format!(
                "等待到模板：位置 ({}, {}), 置信度 {:.4}",
                m.x, m.y, m.confidence
            )),
            None => bail!("超时 {timeout:?} 内未等到模板"),
        }
    }

    fn exec_click_image(&self, step: &Step) -> Result<String> {
        let template = self.load_template(step)?;
        let precision = req_precision(step);
        self.actions.click_image(&template, precision)?;
        Ok(format!("点击模板图像中心（precision={precision}）"))
    }

    fn exec_assert_image(&self, step: &Step) -> Result<String> {
        let template = self.load_template(step)?;
        let precision = req_precision(step);
        let timeout = req_timeout(step);
        self.actions.assert_image(&template, precision, timeout)?;
        Ok(format!("断言通过：模板存在（precision={precision}）"))
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
