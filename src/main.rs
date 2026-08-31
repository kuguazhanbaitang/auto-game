//! auto-game —— 通用电脑游戏自动化测试框架
//!
//! M1 演示：模板匹配链路验证
//! 截屏 → 从屏幕裁剪一块区域作为模板 → 在全屏中查找该模板（自匹配）。
//! 自匹配应返回高置信度（≈1.0），用于验证「截图 → 识别」链路可用。
//!
//! 后续配合真实游戏截图生成模板后，可改用 Actions 的
//! find_image / click_image / wait_image / assert_image 驱动测试流程。

use anyhow::Result;
use auto_game::adapter::{CaptureBackend, CaptureTrait, VisionBackend, VisionTrait};
use image::{RgbaImage, imageops::crop_imm};
use tracing::info;

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("auto-game v{} M1 演示：模板匹配", env!("CARGO_PKG_VERSION"));

    // 1. 采集：截取主屏幕
    let capture = CaptureBackend;
    let screen = capture.capture_full()?;
    info!("已截屏 {}x{}", screen.width(), screen.height());

    // 2. 从屏幕左上角裁剪 200x120 区域作为模板
    let template: RgbaImage = crop_imm(&screen, 0, 0, 200, 120).to_image();
    info!(
        "已裁剪模板 {}x{}（屏幕左上角区域）",
        template.width(),
        template.height()
    );

    // 3. 识别：在全屏中查找该模板（自匹配，预期高置信度）
    let vision = VisionBackend;
    match vision.find_template(&screen, &template, 0.8)? {
        Some(m) => info!(
            "模板匹配成功：位置 ({}, {})，尺寸 {}x{}，置信度 {:.4}",
            m.x, m.y, m.width, m.height, m.confidence
        ),
        None => info!("模板匹配未命中（precision=0.8）"),
    }

    info!("M1 演示完成：截图与识别链路可用");
    Ok(())
}
