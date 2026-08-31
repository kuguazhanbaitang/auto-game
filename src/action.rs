//! 动作原语：把「采集 → 识别 → 输入」组合成可复用动作
//!
//! M1 范围：find_image / click_image / wait_image / assert_image

use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use image::RgbaImage;

use crate::adapter::{
    CaptureBackend, CaptureTrait, InputBackend, InputTrait, Match, VisionBackend, VisionTrait,
};

/// 组合动作执行器
pub struct Actions {
    capture: CaptureBackend,
    input: InputBackend,
    vision: VisionBackend,
}

impl Actions {
    pub fn new() -> Self {
        Actions {
            capture: CaptureBackend,
            input: InputBackend,
            vision: VisionBackend,
        }
    }

    /// 在屏幕上查找模板图像
    pub fn find_image(&self, template: &RgbaImage, precision: f64) -> Result<Option<Match>> {
        let screen = self.capture.capture_full()?;
        self.vision.find_template(&screen, template, precision)
    }

    /// 找到模板后移动到其中心并左键点击；找不到则报错
    pub fn click_image(&self, template: &RgbaImage, precision: f64) -> Result<()> {
        match self.find_image(template, precision)? {
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
    ) -> Result<Option<Match>> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(m) = self.find_image(template, precision)? {
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
    ) -> Result<()> {
        match self.wait_image(template, precision, timeout)? {
            Some(_) => Ok(()),
            None => bail!(
                "assert_image 失败：{timeout:?} 内未找到目标图像（precision={precision}）"
            ),
        }
    }
}
