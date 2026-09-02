//! 可插拔后端抽象层
//!
//! 底层库（xcap / enigo / rustautogui）全部封装在 trait 之后，
//! 上层只依赖 trait，替换实现不影响业务逻辑。

use serde::Deserialize;

pub mod capture;
pub mod input;
pub mod vision;

pub use capture::{CaptureBackend, CaptureTrait};
pub use input::{InputBackend, InputTrait, Key, key_from_str};
pub use vision::{Match, VisionBackend, VisionTrait};

/// 屏幕区域（左上角坐标 + 宽高），用于限定模板匹配搜索范围
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}
