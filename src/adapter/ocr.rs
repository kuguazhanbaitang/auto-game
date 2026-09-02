//! OCR 抽象层：文字识别（可选能力）
//!
//! 目标：识别游戏界面上的文字（数值/标题/弹窗文案），与模板匹配互补——
//! 模板匹配回答"某图像在不在/在哪"，OCR 回答"这片区域写了什么"。
//!
//! 实现后端：`paddleocr_rs_onnx`（ONNX Runtime + PaddleOCR 模型）+ PP-OCRv4 中文模型。
//! 模型文件（assets/ocr/，从 ModelScope RapidAI/RapidOCR 仓库下载）：
//!   - ch_PP-OCRv4_det_mobile.onnx  文本检测（DBNet，定位文字区域）
//!   - ch_PP-OCRv4_rec_mobile.onnx  文本识别（把文字区域转为字符序列）
//!   - ppocr_keys_v1.txt           识别模型字符集（alphabet，模型输出索引 → 字符）
//!
//! 运行时依赖：ONNX Runtime 动态库 onnxruntime.dll（已随项目提供于 libs/）。
//! 若运行时提示找不到 onnxruntime，请设置环境变量 ORT_DYLIB_PATH 指向该 dll。

use std::path::Path;

use anyhow::{Result, anyhow, bail};
use image::RgbaImage;

use crate::adapter::Region;

/// OCR 识别出的一行文本（坐标 = 输入图像内的坐标）
#[derive(Debug, Clone)]
pub struct OcrLine {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    /// 识别置信度（0.0~1.0）
    pub confidence: f32,
}

impl OcrLine {
    /// 行中心点
    pub fn center(&self) -> (i32, i32) {
        (self.x + self.w as i32 / 2, self.y + self.h as i32 / 2)
    }
}

/// OCR 后端抽象
pub trait OcrTrait {
    /// 识别图像中的文本行（坐标相对输入图像左上角）
    fn recognize(&self, img: &RgbaImage) -> Result<Vec<OcrLine>>;
}

/// PP-OCRv4 后端（ONNX Runtime 推理）
pub struct OcrBackend {
    engine: paddleocr_rs_onnx::OcrEngine,
}

impl OcrBackend {
    /// 从模型目录加载 det/rec 模型与字符集文件
    pub fn load(model_dir: &Path) -> Result<Self> {
        let det_path = model_dir.join("ch_PP-OCRv4_det_mobile.onnx");
        let rec_path = model_dir.join("ch_PP-OCRv4_rec_mobile.onnx");
        let dict_path = model_dir.join("ppocr_keys_v1.txt");
        for p in [&det_path, &rec_path, &dict_path] {
            if !p.exists() {
                bail!(
                    "OCR 模型缺失: {}。请先下载 PP-OCRv4 中文模型到 {}（ModelScope RapidAI/RapidOCR 仓库）",
                    p.display(),
                    model_dir.display()
                );
            }
        }
        let det = std::fs::read(&det_path)
            .map_err(|e| anyhow!("读取检测模型失败: {e}"))?;
        let rec = std::fs::read(&rec_path)
            .map_err(|e| anyhow!("读取识别模型失败: {e}"))?;
        let keys = std::fs::read(&dict_path)
            .map_err(|e| anyhow!("读取字符集失败: {e}"))?;
        let engine = paddleocr_rs_onnx::OcrEngine::new(&det, &rec, &keys)
            .map_err(|e| anyhow!("初始化 OCR 引擎失败: {e}"))?;
        Ok(Self { engine })
    }
}

impl OcrTrait for OcrBackend {
    fn recognize(&self, img: &RgbaImage) -> Result<Vec<OcrLine>> {
        let dyn_img = image::DynamicImage::ImageRgba8(img.clone());
        let blocks = self
            .engine
            .recognize_all(&dyn_img, paddleocr_rs_onnx::OrderBy::Horizontal)
            .map_err(|e| anyhow!("OCR 识别失败: {e}"))?;
        Ok(blocks
            .into_iter()
            .map(|b| OcrLine {
                text: b.text,
                x: b.x as i32,
                y: b.y as i32,
                w: b.width as u32,
                h: b.height as u32,
                confidence: b.confidence,
            })
            .collect())
    }
}

impl OcrBackend {
    /// 对图像中限定区域做识别；region 为 None 时识别整图
    pub fn recognize_region(&self, img: &RgbaImage, region: Option<Region>) -> Result<Vec<OcrLine>> {
        match region {
            Some(r) => {
                let x = r.x.max(0) as u32;
                let y = r.y.max(0) as u32;
                let w = r.w.min(img.width().saturating_sub(x));
                let h = r.h.min(img.height().saturating_sub(y));
                let crop = image::imageops::crop_imm(img, x, y, w, h).to_image();
                let mut lines = self.recognize(&crop)?;
                // 坐标加回区域偏移
                for l in &mut lines {
                    l.x += r.x;
                    l.y += r.y;
                }
                Ok(lines)
            }
            None => self.recognize(img),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 模型加载 + 识别流程 smoke 测试（需要 assets/ocr 模型；模型未就绪时跳过）
    #[test]
    fn ocr_engine_loads_and_runs_smoke() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let model_dir = manifest.join("assets/ocr");
        let backend = match OcrBackend::load(&model_dir) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("跳过：OCR 模型加载失败: {e}");
                return;
            }
        };
        // 全白图：检测应无文本，流程可运行且不 panic
        let img = RgbaImage::from_pixel(200, 80, image::Rgba([255, 255, 255, 255]));
        let lines = backend.recognize(&img).expect("识别流程应可运行");
        assert!(lines.is_empty(), "全白图不应识别出文本，实际: {lines:?}");
    }
}
