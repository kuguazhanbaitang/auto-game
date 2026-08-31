//! 截图后端：基于 xcap（跨平台、支持多显示器）

use anyhow::Result;
use image::{RgbaImage, imageops::crop_imm};
use xcap::Monitor;

/// 截图后端（默认主显示器）
pub struct CaptureBackend;

/// 截图抽象契约
pub trait CaptureTrait {
    /// 捕获主显示器全屏
    fn capture_full(&self) -> Result<RgbaImage>;
    /// 捕获指定区域（屏幕坐标，x/y 为左上角）
    fn capture_region(&self, x: i32, y: i32, w: u32, h: u32) -> Result<RgbaImage>;
}

impl CaptureTrait for CaptureBackend {
    fn capture_full(&self) -> Result<RgbaImage> {
        let monitor = Monitor::from_point(0, 0)?;
        Ok(monitor.capture_image()?)
    }

    fn capture_region(&self, x: i32, y: i32, w: u32, h: u32) -> Result<RgbaImage> {
        let monitor = Monitor::from_point(x, y)?;
        let full = monitor.capture_image()?;
        let region = crop_imm(&full, x as u32, y as u32, w, h).to_image();
        Ok(region)
    }
}
