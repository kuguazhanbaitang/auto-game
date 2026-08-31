//! auto-game —— 通用电脑游戏自动化测试框架
//!
//! M1 演示：模板匹配链路验证（小区域快速验证）
//! 只截取屏幕左上角 500x400 区域 → 从该区域裁剪模板 → 区域内自匹配。
//!
//! 说明：全屏匹配在 debug 模式下计算量大（imageproc 朴素算法），
//! 故演示限定搜索区域。实际使用时也建议按区域搜索（见 docs/design.md 性能对策）。

use anyhow::Result;
use auto_game::adapter::{CaptureBackend, CaptureTrait, VisionBackend, VisionTrait};
use image::{RgbaImage, imageops::crop_imm};
use tracing::info;

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!(
        "auto-game v{} M1 演示：模板匹配（小区域）",
        env!("CARGO_PKG_VERSION")
    );

    let capture = CaptureBackend;
    let vision = VisionBackend;

    // 1. 采集：只截取左上角 500x400 区域作为搜索范围（避免全屏匹配过慢）
    let region = capture.capture_region(0, 0, 500, 400)?;
    info!("已截取搜索区域 500x400");

    // 2. 从区域左上角裁剪 120x80 作为模板
    let template: RgbaImage = crop_imm(&region, 0, 0, 120, 80).to_image();
    info!("已裁剪模板 120x80");

    // 3. 识别：在区域内查找该模板（自匹配，预期高置信度）
    match vision.find_template(&region, &template, 0.8)? {
        Some(m) => info!(
            "模板匹配成功：位置 ({}, {})，尺寸 {}x{}，置信度 {:.4}",
            m.x, m.y, m.width, m.height, m.confidence
        ),
        None => info!("模板匹配未命中（precision=0.8）"),
    }

    info!("M1 演示完成：截图与识别链路可用");
    Ok(())
}
