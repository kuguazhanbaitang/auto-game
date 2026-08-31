//! TOML 场景解析：把场景配置反序列化为可执行的动作序列

use std::path::Path;

use anyhow::{Result, anyhow};
use serde::Deserialize;

/// 场景
#[derive(Debug, Clone, Deserialize)]
pub struct Scenario {
    #[serde(default)]
    pub meta: Meta,
    /// TOML 中数组表名为 `[[step]]`，故此处重命名映射
    #[serde(default, rename = "step")]
    pub steps: Vec<Step>,
}

/// 场景元信息
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Meta {
    pub name: Option<String>,
    pub window: Option<String>,
}

/// 单步动作
#[derive(Debug, Clone, Deserialize)]
pub struct Step {
    /// 动作类型：screenshot / wait / move_mouse / click / key_press /
    /// type_text / find_image / wait_image / click_image / assert_image
    pub action: String,
    /// 模板图像路径（相对 assets 目录）
    #[serde(default)]
    pub image: Option<String>,
    /// 匹配精度（0.0 ~ 1.0）
    #[serde(default)]
    pub precision: Option<f64>,
    /// 超时（秒）
    #[serde(default)]
    pub timeout: Option<f64>,
    /// 坐标
    #[serde(default)]
    pub x: Option<i32>,
    #[serde(default)]
    pub y: Option<i32>,
    /// 输入文本
    #[serde(default)]
    pub text: Option<String>,
    /// 按键名称（enter / escape / space）
    #[serde(default)]
    pub key: Option<String>,
    /// 固定延时（秒）
    #[serde(default)]
    pub seconds: Option<f64>,
}

impl Scenario {
    /// 从 TOML 文件加载场景
    pub fn load(path: &Path) -> Result<Scenario> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow!("读取场景 {} 失败: {e}", path.display()))?;
        let scenario: Scenario = toml::from_str(&content)
            .map_err(|e| anyhow!("解析场景 {} 失败: {e}", path.display()))?;
        Ok(scenario)
    }
}
