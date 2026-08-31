//! 输入后端：基于 enigo（跨平台键鼠模拟）

use anyhow::Result;
use enigo::{Button, Coordinate, Direction, Enigo, Keyboard, Mouse, Settings};

/// 输入后端
pub struct InputBackend;

/// 键盘键位（M0 先覆盖常用键，后续按需扩充）
pub enum Key {
    Enter,
    Escape,
    Space,
}

/// 输入抽象契约
pub trait InputTrait {
    /// 移动鼠标到屏幕绝对坐标
    fn move_mouse(&self, x: i32, y: i32) -> Result<()>;
    /// 左键单击
    fn click(&self) -> Result<()>;
    /// 输入一段文本
    fn type_text(&self, text: &str) -> Result<()>;
    /// 按键（如 enter / escape / space）
    fn key_press(&self, key: Key) -> Result<()>;
}

impl InputTrait for InputBackend {
    fn move_mouse(&self, x: i32, y: i32) -> Result<()> {
        let mut enigo = Enigo::new(&Settings::default())?;
        enigo.move_mouse(x, y, Coordinate::Abs)?;
        Ok(())
    }

    fn click(&self) -> Result<()> {
        let mut enigo = Enigo::new(&Settings::default())?;
        enigo.button(Button::Left, Direction::Click)?;
        Ok(())
    }

    fn type_text(&self, text: &str) -> Result<()> {
        let mut enigo = Enigo::new(&Settings::default())?;
        enigo.text(text)?;
        Ok(())
    }

    fn key_press(&self, key: Key) -> Result<()> {
        let mut enigo = Enigo::new(&Settings::default())?;
        let key = match key {
            Key::Enter => enigo::Key::Return,
            Key::Escape => enigo::Key::Escape,
            Key::Space => enigo::Key::Space,
        };
        enigo.key(key, Direction::Click)?;
        Ok(())
    }
}
