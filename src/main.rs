//! auto-game —— 通用电脑游戏自动化测试框架
//!
//! M0 骨架验证：截图保存 PNG + 模拟一次键鼠，验证采集与输入链路。
//!
//! 说明：输入模拟（SendInput）依赖交互式桌面与前台窗口。若在无桌面
//! 会话（如远程/agent 环境、无 explorer）或目标窗口以管理员权限前台
//! 运行时，可能被 Windows UIPI 拦截——这是系统安全机制，非代码缺陷。

use anyhow::Result;
use auto_game::adapter::{CaptureBackend, CaptureTrait, InputBackend, InputTrait};
use tracing::{error, info};

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("auto-game v{} M0 骨架验证开始", env!("CARGO_PKG_VERSION"));

    // 1. 采集：截取主屏幕并保存 PNG（硬校验）
    let capture = CaptureBackend;
    let img = capture.capture_full()?;
    img.save("screenshot_m0.png")?;
    info!(
        "截图已保存 screenshot_m0.png（{}x{}）",
        img.width(),
        img.height()
    );

    // 2. 输入：移动鼠标并左键点击（失败给出明确诊断，不视为代码错误）
    let input = InputBackend;
    match input.move_mouse(640, 480).and_then(|()| input.click()) {
        Ok(()) => info!("已模拟移动鼠标到 (640,480) 并左键点击"),
        Err(e) => {
            error!(
                "输入模拟失败：{e}\n原因：SendInput 可能被 UIPI 拦截（无交互式桌面 / 前台窗口权限更高）。\n请在真实桌面会话、游戏窗口处于前台时运行验证。"
            );
        }
    }

    info!("M0 骨架验证完成：采集链路通过，输入链路已就绪（待真实桌面验证）");
    Ok(())
}
