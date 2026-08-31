//! 识别后端：模板匹配（M1 阶段落地 rustautogui / imageproc 实现）

use anyhow::{Result, anyhow};
use image::RgbaImage;

/// 识别后端
pub struct VisionBackend;

/// 一次匹配结果
#[derive(Debug, Clone)]
pub struct Match {
    /// 匹配到的左上角 x 坐标（屏幕坐标）
    pub x: i32,
    /// 匹配到的左上角 y 坐标（屏幕坐标）
    pub y: i32,
    /// 置信度（0.0 ~ 1.0）
    pub confidence: f64,
}

/// 识别抽象契约
pub trait VisionTrait {
    /// 在屏幕图像中查找模板，返回最佳匹配（无匹配返回 None）
    fn find_template(
        &self,
        screen: &RgbaImage,
        template: &RgbaImage,
        precision: f64,
    ) -> Result<Option<Match>>;
}

impl VisionTrait for VisionBackend {
    fn find_template(
        &self,
        _screen: &RgbaImage,
        _template: &RgbaImage,
        _precision: f64,
    ) -> Result<Option<Match>> {
        Err(anyhow!("模板匹配将在 M1 阶段落地（rustautogui/imageproc）"))
    }
}
