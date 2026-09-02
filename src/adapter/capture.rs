//! 截图后端：基于 xcap（跨平台、支持多显示器、支持指定窗口捕获）

use anyhow::{Result, anyhow};
use image::{RgbaImage, imageops::crop_imm};
use xcap::Monitor;

/// 截图后端（默认主显示器）
pub struct CaptureBackend;

/// 窗口截图结果：图像 + 窗口左上角屏幕坐标（用于把窗口内坐标映射到屏幕坐标）
#[derive(Debug, Clone)]
pub struct WindowShot {
    /// 窗口内容图像（窗口内坐标，(0,0) 为窗口客户区左上角）
    pub image: RgbaImage,
    /// 窗口左上角在屏幕上的 x 坐标
    pub offset_x: i32,
    /// 窗口左上角在屏幕上的 y 坐标
    pub offset_y: i32,
}

/// 供窗口选择的窗口元信息（从 xcap Window 提取，便于纯逻辑单测）
pub struct WindowMeta {
    pub title: String,
    pub minimized: bool,
}

/// 截图抽象契约
pub trait CaptureTrait {
    /// 捕获主显示器全屏
    fn capture_full(&self) -> Result<RgbaImage>;
    /// 捕获指定区域（屏幕坐标，x/y 为左上角；自动定位所在显示器）
    fn capture_region(&self, x: i32, y: i32, w: u32, h: u32) -> Result<RgbaImage>;
    /// 按窗口标题关键字捕获窗口内容，返回图像 + 窗口左上角屏幕坐标。
    /// 窗口可能被拖动/遮挡，故位置每次实时获取（用于坐标映射）。
    fn capture_window(&self, title_keyword: &str) -> Result<WindowShot>;
}

impl CaptureTrait for CaptureBackend {
    fn capture_full(&self) -> Result<RgbaImage> {
        let monitor = Monitor::from_point(0, 0)?;
        Ok(monitor.capture_image()?)
    }

    fn capture_region(&self, x: i32, y: i32, w: u32, h: u32) -> Result<RgbaImage> {
        let monitor = Monitor::from_point(x, y)?;
        let full = monitor.capture_image()?;
        let region = crop_imm(&full, x as u32, y as u32, w, h).to_image();
        Ok(region)
    }

    fn capture_window(&self, title_keyword: &str) -> Result<WindowShot> {
        use xcap::Window;
        let windows = Window::all()?;
        let metas: Vec<WindowMeta> = windows
            .iter()
            .map(|w| WindowMeta {
                title: w.title().to_string(),
                minimized: w.is_minimized(),
            })
            .collect();
        let idx = select_window(&metas, title_keyword)?.ok_or_else(|| {
            anyhow!(
                "未找到标题包含 {:?} 的窗口；当前可捕获窗口：{:?}",
                title_keyword,
                windows.iter().map(|w| w.title()).collect::<Vec<_>>()
            )
        })?;
        let win = &windows[idx];
        let image = win.capture_image()?;
        Ok(WindowShot {
            image,
            offset_x: win.x(),
            offset_y: win.y(),
        })
    }
}

/// 从窗口列表中按标题关键字挑选窗口（纯逻辑，便于单测）。
/// 规则：忽略最小化窗口；按标题包含关键字匹配；
/// 匹配 0 个返回 Ok(None)（调用方报错）；
/// 匹配多个时返回第一个（Window::all 已按 z 序排列，即最靠前的窗口），
/// 并打印警告提示「若为多开请用更精确的标题」。
fn select_window(windows: &[WindowMeta], keyword: &str) -> Result<Option<usize>> {
    let idxs: Vec<usize> = windows
        .iter()
        .enumerate()
        .filter(|(_, w)| !w.minimized && w.title.contains(keyword))
        .map(|(i, _)| i)
        .collect();
    match idxs.len() {
        0 => Ok(None),
        1 => Ok(Some(idxs[0])),
        n => {
            tracing::warn!(
                "窗口标题 {:?} 匹配到 {n} 个窗口，取 z 序最前者；若为多开实例请用更精确的标题关键字",
                keyword
            );
            Ok(Some(idxs[0]))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(title: &str, minimized: bool) -> WindowMeta {
        WindowMeta {
            title: title.to_string(),
            minimized,
        }
    }

    #[test]
    fn select_single_window_by_keyword() {
        let wins = vec![
            meta("Desktop", false),
            meta("MuMuPlayer12 - 阴阳师", false),
            meta("龙之谷 - DN", false),
        ];
        assert_eq!(select_window(&wins, "阴阳师").unwrap(), Some(1));
        assert_eq!(select_window(&wins, "龙之谷").unwrap(), Some(2));
    }

    #[test]
    fn select_filters_minimized_windows() {
        let wins = vec![
            meta("MuMuPlayer12 - 阴阳师", true), // 最小化，应被忽略
            meta("MuMuPlayer12 - 阴阳师", false),
        ];
        assert_eq!(select_window(&wins, "阴阳师").unwrap(), Some(1));
    }

    #[test]
    fn select_no_match_is_none() {
        let wins = vec![meta("Desktop", false)];
        assert_eq!(select_window(&wins, "不存在的窗口").unwrap(), None);
        // 关键字为空时匹配第一个非最小化窗口
        assert_eq!(select_window(&wins, "").unwrap(), Some(0));
    }

    #[test]
    fn select_multiple_takes_first_with_warning() {
        // 多开场景：两个同标题窗口（Window::all 已按 z 序返回），取枚举序最前者（索引 0）
        let wins = vec![
            meta("MuMuPlayer12 - 阴阳师", false),
            meta("MuMuPlayer12 - 阴阳师", false),
        ];
        assert_eq!(select_window(&wins, "阴阳师").unwrap(), Some(0));
    }
}
