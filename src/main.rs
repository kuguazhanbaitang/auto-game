//! auto-game —— 电脑游戏自动化测试项目
//!
//! 规划中的模块：
//! - capture: 屏幕截图采集
//! - input:   键盘 / 鼠标模拟输入
//! - vision:  图像识别（模板匹配、像素比对）
//! - config:  测试配置加载

use anyhow::Result;

fn main() -> Result<()> {
    println!("auto-game v{} 已启动", env!("CARGO_PKG_VERSION"));
    println!("这是一个用于电脑游戏自动化测试的 Rust 项目骨架。");
    Ok(())
}
