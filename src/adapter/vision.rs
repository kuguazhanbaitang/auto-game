//! 识别后端：模板匹配（基于 imageproc 纯 Rust 实现）

use anyhow::{Result, anyhow};
use image::{DynamicImage, RgbaImage};
use imageproc::template_matching::{find_extremes, match_template, MatchTemplateMethod};

/// 识别后端
pub struct VisionBackend;

/// 一次匹配结果
#[derive(Debug, Clone)]
pub struct Match {
    /// 匹配到的左上角 x 坐标（屏幕坐标）
    pub x: i32,
    /// 匹配到的左上角 y 坐标（屏幕坐标）
    pub y: i32,
    /// 模板宽度（用于计算中心点）
    pub width: u32,
    /// 模板高度
    pub height: u32,
    /// 置信度（0.0 ~ 1.0）
    pub confidence: f64,
}

impl Match {
    /// 模板中心点坐标（点击用）
    pub fn center(&self) -> (i32, i32) {
        (
            self.x + self.width as i32 / 2,
            self.y + self.height as i32 / 2,
        )
    }
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
        screen: &RgbaImage,
        template: &RgbaImage,
        precision: f64,
    ) -> Result<Option<Match>> {
        if template.width() > screen.width() || template.height() > screen.height() {
            return Err(anyhow!(
                "模板尺寸大于屏幕图像（模板 {}x{} vs 屏幕 {}x{}）",
                template.width(),
                template.height(),
                screen.width(),
                screen.height()
            ));
        }

        // 转灰度后做归一化交叉相关匹配
        let screen_gray = DynamicImage::ImageRgba8(screen.clone()).to_luma8();
        let template_gray = DynamicImage::ImageRgba8(template.clone()).to_luma8();
        let result = match_template(
            &screen_gray,
            &template_gray,
            MatchTemplateMethod::CrossCorrelationNormalized,
        );

        let extremes = find_extremes(&result);
        let max = extremes.max_value;
        if max >= precision as f32 {
            let (x, y) = extremes.max_value_location;
            Ok(Some(Match {
                x: x as i32,
                y: y as i32,
                width: template.width(),
                height: template.height(),
                confidence: max as f64,
            }))
        } else {
            Ok(None)
        }
    }
}
