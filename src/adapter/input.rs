//! 输入后端：基于 enigo（跨平台键鼠模拟）

use anyhow::{Result, bail};
use enigo::{Button, Coordinate, Direction, Enigo, Keyboard, Mouse, Settings};

/// 输入后端
pub struct InputBackend;

/// 键盘键位（覆盖游戏常用键：字母/数字/功能键/方向/修饰键）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    // 字母
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    // 主键盘数字
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    // 功能键
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    // 方向
    Up,
    Down,
    Left,
    Right,
    // 控制/编辑
    Enter,
    Escape,
    Space,
    Tab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    // 修饰键
    Control,
    Shift,
    Alt,
    Meta,
}

impl Key {
    /// 映射到 enigo 键位
    pub fn to_enigo(&self) -> enigo::Key {
        use Key::*;
        match self {
            A => enigo::Key::A,
            B => enigo::Key::B,
            C => enigo::Key::C,
            D => enigo::Key::D,
            E => enigo::Key::E,
            F => enigo::Key::F,
            G => enigo::Key::G,
            H => enigo::Key::H,
            I => enigo::Key::I,
            J => enigo::Key::J,
            K => enigo::Key::K,
            L => enigo::Key::L,
            M => enigo::Key::M,
            N => enigo::Key::N,
            O => enigo::Key::O,
            P => enigo::Key::P,
            Q => enigo::Key::Q,
            R => enigo::Key::R,
            S => enigo::Key::S,
            T => enigo::Key::T,
            U => enigo::Key::U,
            V => enigo::Key::V,
            W => enigo::Key::W,
            X => enigo::Key::X,
            Y => enigo::Key::Y,
            Z => enigo::Key::Z,
            Num0 => enigo::Key::Num0,
            Num1 => enigo::Key::Num1,
            Num2 => enigo::Key::Num2,
            Num3 => enigo::Key::Num3,
            Num4 => enigo::Key::Num4,
            Num5 => enigo::Key::Num5,
            Num6 => enigo::Key::Num6,
            Num7 => enigo::Key::Num7,
            Num8 => enigo::Key::Num8,
            Num9 => enigo::Key::Num9,
            F1 => enigo::Key::F1,
            F2 => enigo::Key::F2,
            F3 => enigo::Key::F3,
            F4 => enigo::Key::F4,
            F5 => enigo::Key::F5,
            F6 => enigo::Key::F6,
            F7 => enigo::Key::F7,
            F8 => enigo::Key::F8,
            F9 => enigo::Key::F9,
            F10 => enigo::Key::F10,
            F11 => enigo::Key::F11,
            F12 => enigo::Key::F12,
            Up => enigo::Key::UpArrow,
            Down => enigo::Key::DownArrow,
            Left => enigo::Key::LeftArrow,
            Right => enigo::Key::RightArrow,
            Enter => enigo::Key::Return,
            Escape => enigo::Key::Escape,
            Space => enigo::Key::Space,
            Tab => enigo::Key::Tab,
            Backspace => enigo::Key::Backspace,
            Delete => enigo::Key::Delete,
            Insert => enigo::Key::Insert,
            Home => enigo::Key::Home,
            End => enigo::Key::End,
            PageUp => enigo::Key::PageUp,
            PageDown => enigo::Key::PageDown,
            Control => enigo::Key::Control,
            Shift => enigo::Key::Shift,
            Alt => enigo::Key::Alt,
            Meta => enigo::Key::Meta,
        }
    }
}

/// 把按键名称字符串解析为 Key（不区分大小写）
pub fn key_from_str(s: &str) -> Result<Key> {
    use Key::*;
    let k = match s.trim().to_ascii_lowercase().as_str() {
        "a" => A,
        "b" => B,
        "c" => C,
        "d" => D,
        "e" => E,
        "f" => F,
        "g" => G,
        "h" => H,
        "i" => I,
        "j" => J,
        "k" => K,
        "l" => L,
        "m" => M,
        "n" => N,
        "o" => O,
        "p" => P,
        "q" => Q,
        "r" => R,
        "s" => S,
        "t" => T,
        "u" => U,
        "v" => V,
        "w" => W,
        "x" => X,
        "y" => Y,
        "z" => Z,
        "0" => Num0,
        "1" => Num1,
        "2" => Num2,
        "3" => Num3,
        "4" => Num4,
        "5" => Num5,
        "6" => Num6,
        "7" => Num7,
        "8" => Num8,
        "9" => Num9,
        "f1" => F1,
        "f2" => F2,
        "f3" => F3,
        "f4" => F4,
        "f5" => F5,
        "f6" => F6,
        "f7" => F7,
        "f8" => F8,
        "f9" => F9,
        "f10" => F10,
        "f11" => F11,
        "f12" => F12,
        "up" => Up,
        "down" => Down,
        "left" => Left,
        "right" => Right,
        "enter" | "return" => Enter,
        "escape" | "esc" => Escape,
        "space" => Space,
        "tab" => Tab,
        "backspace" => Backspace,
        "delete" | "del" => Delete,
        "insert" | "ins" => Insert,
        "home" => Home,
        "end" => End,
        "pageup" | "pgup" => PageUp,
        "pagedown" | "pgdn" => PageDown,
        "ctrl" | "control" => Control,
        "shift" => Shift,
        "alt" => Alt,
        "meta" | "win" => Meta,
        other => bail!("不支持的按键: {other}"),
    };
    Ok(k)
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
    /// 组合键：先全部按下，再逆序释放（如 Ctrl+A）
    fn key_combo(&self, keys: &[Key]) -> Result<()>;
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
        enigo.key(key.to_enigo(), Direction::Click)?;
        Ok(())
    }

    fn key_combo(&self, keys: &[Key]) -> Result<()> {
        let mut enigo = Enigo::new(&Settings::default())?;
        let mapped: Vec<enigo::Key> = keys.iter().map(|k| k.to_enigo()).collect();
        for k in &mapped {
            enigo.key(*k, Direction::Press)?;
        }
        for k in mapped.iter().rev() {
            enigo.key(*k, Direction::Release)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_from_str_parses_common_names() {
        assert_eq!(key_from_str("enter").unwrap(), Key::Enter);
        assert_eq!(key_from_str("Return").unwrap(), Key::Enter);
        assert_eq!(key_from_str("esc").unwrap(), Key::Escape);
        assert_eq!(key_from_str("W").unwrap(), Key::W);
        assert_eq!(key_from_str("f9").unwrap(), Key::F9);
        assert_eq!(key_from_str("0").unwrap(), Key::Num0);
        assert_eq!(key_from_str("up").unwrap(), Key::Up);
        assert_eq!(key_from_str("ctrl").unwrap(), Key::Control);
        assert_eq!(key_from_str("  space  ").unwrap(), Key::Space);
    }

    #[test]
    fn key_from_str_rejects_unknown() {
        assert!(key_from_str("nope").is_err());
        assert!(key_from_str("").is_err());
    }

    #[test]
    fn key_to_enigo_mapping() {
        assert_eq!(Key::Enter.to_enigo(), enigo::Key::Return);
        assert_eq!(Key::Escape.to_enigo(), enigo::Key::Escape);
        assert_eq!(Key::Up.to_enigo(), enigo::Key::UpArrow);
        assert_eq!(Key::A.to_enigo(), enigo::Key::A);
        assert_eq!(Key::Num3.to_enigo(), enigo::Key::Num3);
        assert_eq!(Key::F5.to_enigo(), enigo::Key::F5);
        assert_eq!(Key::Control.to_enigo(), enigo::Key::Control);
    }
}
