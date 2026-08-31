//! 可插拔后端抽象层
//!
//! 底层库（xcap / enigo / rustautogui）全部封装在 trait 之后，
//! 上层只依赖 trait，替换实现不影响业务逻辑。

pub mod capture;
pub mod input;
pub mod vision;

pub use capture::{CaptureBackend, CaptureTrait};
pub use input::{InputBackend, InputTrait, Key};
pub use vision::{Match, VisionBackend, VisionTrait};
