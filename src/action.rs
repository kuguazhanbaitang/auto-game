//! 动作原语：把「采集 → 识别 → 输入」组合成可复用动作
//!
//! M1 范围：find_image / click_image / wait_image / assert_image
//! M4 增强：窗口级捕获（meta.window 指定标题后，截图限定在窗口内，
//!         识别坐标自动映射回屏幕坐标用于输入）

use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use image::{RgbaImage, imageops::crop_imm};

use crate::adapter::{
    CaptureBackend, CaptureTrait, InputBackend, InputTrait, Key, Match, Region, VisionBackend,
    VisionTrait, WindowShot,
};

/// 组合动作执行器
pub struct Actions {
    capture: CaptureBackend,
    input: InputBackend,
    vision: VisionBackend,
    /// 窗口捕获模式：Some(标题关键字) 时截图限定在该窗口，
    /// 识别/输入坐标自动映射回屏幕坐标；None 时为全屏模式。
    window: Option<String>,
}

impl Actions {
    pub fn new() -> Self {
        Actions {
            capture: CaptureBackend,
            input: InputBackend,
            vision: VisionBackend,
            window: None,
        }
    }

    /// 设置窗口捕获模式（None = 全屏；Some = 按标题关键字捕获窗口）。
    /// 应在场景开始前调用（engine 从 [meta].window 读取）。
    pub fn set_window(&mut self, title: Option<String>) {
        self.window = title;
    }

    /// 是否处于窗口捕获模式
    pub fn in_window_mode(&self) -> bool {
        self.window.is_some()
    }

    /// 获取捕获目标：(目标图像, 屏幕偏移 ox, oy)。
    /// 窗口模式：窗口内容 + 窗口左上角屏幕坐标；全屏模式：全屏图 + (0,0)。
    fn capture_target(&self) -> Result<(RgbaImage, i32, i32)> {
        match &self.window {
            Some(kw) => {
                let shot: WindowShot = self.capture.capture_window(kw)?;
                Ok((shot.image, shot.offset_x, shot.offset_y))
            }
            None => Ok((self.capture.capture_full()?, 0, 0)),
        }
    }

    /// 全屏截图（历史接口，窗口模式下仍截全屏；新代码建议用 snapshot）
    pub fn capture_full(&self) -> Result<RgbaImage> {
        self.capture.capture_full()
    }

    /// 截取指定屏幕区域（左上角 + 宽高；历史接口）
    pub fn capture_region(&self, x: i32, y: i32, w: u32, h: u32) -> Result<RgbaImage> {
        self.capture.capture_region(x, y, w, h)
    }

    /// 窗口感知快照：窗口模式下截窗口内容，否则全屏
    pub fn snapshot(&self) -> Result<RgbaImage> {
        self.capture_target().map(|(img, _, _)| img)
    }

    /// 窗口感知区域快照：窗口模式下 region 为窗口内坐标（相对窗口左上角），
    /// 否则为屏幕坐标
    pub fn snapshot_region(&self, r: Region) -> Result<RgbaImage> {
        let (target, _, _) = self.capture_target()?;
        let x = r.x.max(0) as u32;
        let y = r.y.max(0) as u32;
        let w = r.w.min(target.width().saturating_sub(x));
        let h = r.h.min(target.height().saturating_sub(y));
        Ok(crop_imm(&target, x, y, w, h).to_image())
    }

    /// 移动鼠标到屏幕绝对坐标
    pub fn move_mouse(&self, x: i32, y: i32) -> Result<()> {
        self.input.move_mouse(x, y)
    }

    /// 左键单击（当前鼠标位置）
    pub fn click(&self) -> Result<()> {
        self.input.click()
    }

    /// 按下指定按键
    pub fn key_press(&self, key: Key) -> Result<()> {
        self.input.key_press(key)
    }

    /// 组合键：先全部按下，再逆序释放（如 Ctrl+A）
    pub fn key_combo(&self, keys: &[Key]) -> Result<()> {
        self.input.key_combo(keys)
    }

    /// 输入一段文本
    pub fn type_text(&self, text: &str) -> Result<()> {
        self.input.type_text(text)
    }

    /// 在捕获目标（窗口或全屏）上查找模板图像。
    /// `verify_exact` 开启时走「fast 粗定位 → exact 精确确认」路径。
    /// 返回的 Match 坐标恒为屏幕坐标（窗口模式下已自动加窗口偏移）。
    pub fn find_image(
        &self,
        template: &RgbaImage,
        precision: f64,
        verify_exact: bool,
    ) -> Result<Option<Match>> {
        let (screen, ox, oy) = self.capture_target()?;
        match self
            .vision
            .find_template(&screen, template, precision, verify_exact)?
        {
            Some(m) => Ok(Some(offset_match(m, ox, oy))),
            None => Ok(None),
        }
    }

    /// 在指定区域内查找模板（性能优化：避免全屏匹配）。
    /// 窗口模式下 region 为窗口内坐标（相对窗口左上角），否则为屏幕坐标；
    /// 返回的 Match 坐标恒为屏幕坐标。
    pub fn find_image_region(
        &self,
        template: &RgbaImage,
        precision: f64,
        region: Region,
        verify_exact: bool,
    ) -> Result<Option<Match>> {
        if self.window.is_some() {
            // 窗口模式：窗口图内 crop region（窗口内坐标），偏移回屏幕
            let (target, ox, oy) = self.capture_target()?;
            let x = region.x.max(0) as u32;
            let y = region.y.max(0) as u32;
            let w = region.w.min(target.width().saturating_sub(x));
            let h = region.h.min(target.height().saturating_sub(y));
            let win_img = crop_imm(&target, x, y, w, h).to_image();
            match self
                .vision
                .find_template(&win_img, template, precision, verify_exact)?
            {
                Some(m) => Ok(Some(region_match_to_screen(region, m, ox, oy))),
                None => Ok(None),
            }
        } else {
            // 全屏模式：xcap 区域截图（自动定位所在显示器）
            let screen = self
                .capture
                .capture_region(region.x, region.y, region.w, region.h)?;
            match self
                .vision
                .find_template(&screen, template, precision, verify_exact)?
            {
                Some(m) => Ok(Some(region_match_to_screen(region, m, 0, 0))),
                None => Ok(None),
            }
        }
    }

    /// 找到模板后移动到其中心并左键点击；找不到则报错
    pub fn click_image(
        &self,
        template: &RgbaImage,
        precision: f64,
        verify_exact: bool,
    ) -> Result<()> {
        match self.find_image(template, precision, verify_exact)? {
            Some(m) => {
                let (cx, cy) = m.center();
                self.input.move_mouse(cx, cy)?;
                self.input.click()?;
                Ok(())
            }
            None => bail!("click_image 失败：未找到目标图像（precision={precision}）"),
        }
    }

    /// 轮询等待模板出现（间隔 200ms），超时返回 None
    pub fn wait_image(
        &self,
        template: &RgbaImage,
        precision: f64,
        timeout: Duration,
        verify_exact: bool,
    ) -> Result<Option<Match>> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(m) = self.find_image(template, precision, verify_exact)? {
                return Ok(Some(m));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// 断言模板在超时内出现；否则视为失败
    pub fn assert_image(
        &self,
        template: &RgbaImage,
        precision: f64,
        timeout: Duration,
        verify_exact: bool,
    ) -> Result<()> {
        match self.wait_image(template, precision, timeout, verify_exact)? {
            Some(_) => Ok(()),
            None => bail!(
                "assert_image 失败：{timeout:?} 内未找到目标图像（precision={precision}）"
            ),
        }
    }
}

/// 把窗口内匹配坐标映射回屏幕坐标（+ 窗口左上角偏移）
fn offset_match(m: Match, ox: i32, oy: i32) -> Match {
    Match {
        x: m.x + ox,
        y: m.y + oy,
        ..m
    }
}

/// region 内匹配坐标 → 屏幕坐标：region 左上角（相对捕获目标）+ 匹配点 + 屏幕偏移
fn region_match_to_screen(region: Region, m: Match, ox: i32, oy: i32) -> Match {
    Match {
        x: region.x + m.x + ox,
        y: region.y + m.y + oy,
        ..m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(x: i32, y: i32) -> Match {
        Match {
            x,
            y,
            width: 50,
            height: 30,
            confidence: 0.9,
        }
    }

    #[test]
    fn offset_match_adds_window_origin() {
        let shifted = offset_match(m(100, 200), 640, 380);
        assert_eq!((shifted.x, shifted.y), (740, 580));
        // 尺寸与置信度保持不变
        assert_eq!((shifted.width, shifted.height), (50, 30));
        assert_eq!(shifted.confidence, 0.9);
        // 全屏模式偏移为 0
        let plain = offset_match(m(100, 200), 0, 0);
        assert_eq!((plain.x, plain.y), (100, 200));
    }

    #[test]
    fn region_match_maps_to_screen() {
        let region = Region {
            x: 300,
            y: 200,
            w: 400,
            h: 300,
        };
        // 窗口模式：region 窗口内坐标 + 匹配点 + 窗口偏移
        let r = region_match_to_screen(region, m(40, 60), 640, 380);
        assert_eq!((r.x, r.y), (300 + 40 + 640, 200 + 60 + 380));
        // 全屏模式：偏移为 0
        let r2 = region_match_to_screen(region, m(40, 60), 0, 0);
        assert_eq!((r2.x, r2.y), (340, 260));
    }

    #[test]
    fn window_mode_flag_and_setter() {
        let mut a = Actions::new();
        assert!(!a.in_window_mode());
        a.set_window(Some("龙之谷".to_string()));
        assert!(a.in_window_mode());
        a.set_window(None);
        assert!(!a.in_window_mode());
    }
}
