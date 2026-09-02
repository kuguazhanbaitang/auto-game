//! TOML 场景解析：把场景配置反序列化为可执行的动作序列

use std::path::Path;

use anyhow::{Result, anyhow};
use serde::Deserialize;

use crate::adapter::Region;

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
    /// fast→exact 确认开关：开启后，金字塔加速定位的每个模板
    /// 都会在最终位置再跑一次精确匹配确认（像素级精确 + 准确置信度）；
    /// 金字塔粗层无候选时自动回退全图精确匹配兜底。默认 false。
    #[serde(default)]
    pub verify_exact: bool,
}

/// 单步动作
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Step {
    /// 动作类型：screenshot / wait / move_mouse / click / key_press /
    /// type_text / find_image / wait_image / click_image / assert_image /
    /// ocr_text / if_text / click_text / assert_text
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
    /// 点击随机抖动（像素）：每次点击位置在目标周围 ±jitter 内动态分布，拟人化
    #[serde(default)]
    pub jitter: Option<u32>,
    /// 点击前随机延时（秒）：click / click_image 执行前随机等待 [0, click_delay]，拟人化；
    /// 与 jitter 配合让「位置 + 时机」都不固定。缺省/≤0 表示不延时。
    #[serde(default)]
    pub click_delay: Option<f64>,
    /// 点击随机延时下限（秒）：配合 click_delay 组成 [min, max] 区间；缺省 0
    #[serde(default)]
    pub click_delay_min: Option<f64>,
    /// 输入文本（type_text 使用）；OCR 文字类动作（if_text / click_text /
    /// assert_text）用作「期望包含的子串」匹配
    #[serde(default)]
    pub text: Option<String>,
    /// 按键名称（enter / escape / space）
    #[serde(default)]
    pub key: Option<String>,
    /// 固定延时（秒）
    #[serde(default)]
    pub seconds: Option<f64>,
    /// 循环次数（仅 repeat 动作使用）
    #[serde(default)]
    pub count: Option<u32>,
    /// 组合键列表（仅 key_combo 动作使用，如 ["ctrl", "a"]）
    #[serde(default)]
    pub keys: Option<Vec<String>>,
    /// 限定搜索区域（仅图像类动作使用，性能优化）
    #[serde(default)]
    pub region: Option<Region>,
    /// fast→exact 确认开关（按步骤粒度，覆盖 [meta] 全局 verify_exact）：
    /// 显式声明 true/false 时覆盖全局；缺省时回退 [meta] 全局值。
    #[serde(default)]
    pub verify_exact: Option<bool>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_scenario_with_all_fields() {
        let toml = r#"
[meta]
name = "解析测试"
window = "MyGame"
verify_exact = true

[[step]]
action = "wait_image"
image = "a.png"
precision = 0.9
timeout = 10

[[step]]
action = "key_combo"
keys = ["ctrl", "a"]

[[step]]
action = "repeat"
count = 3

[[step]]
action = "click"
x = 100
y = 200
jitter = 5
click_delay = 0.3
click_delay_min = 0.1

[[step]]
action = "if_image"
image = "b.png"
region = { x = 0, y = 0, w = 500, h = 400 }
verify_exact = false
"#;
        let s: Scenario = toml::from_str(toml).expect("场景应能解析");
        assert_eq!(s.meta.name.as_deref(), Some("解析测试"));
        assert_eq!(s.meta.window.as_deref(), Some("MyGame"));
        assert!(s.meta.verify_exact, "verify_exact 应解析为 true");
        assert_eq!(s.steps.len(), 5);

        assert_eq!(s.steps[0].action, "wait_image");
        assert_eq!(s.steps[0].image.as_deref(), Some("a.png"));
        assert_eq!(s.steps[0].precision, Some(0.9));
        assert_eq!(s.steps[0].timeout, Some(10.0));

        assert_eq!(s.steps[1].action, "key_combo");
        let keys = s.steps[1].keys.as_deref().unwrap();
        assert_eq!(keys, &["ctrl".to_string(), "a".to_string()][..]);

        assert_eq!(s.steps[2].action, "repeat");
        assert_eq!(s.steps[2].count, Some(3));

        assert_eq!(s.steps[3].x, Some(100));
        assert_eq!(s.steps[3].y, Some(200));
        assert_eq!(s.steps[3].jitter, Some(5));
        assert_eq!(s.steps[3].click_delay, Some(0.3));
        assert_eq!(s.steps[3].click_delay_min, Some(0.1));

        let r = s.steps[4].region.expect("region 应被解析");
        assert_eq!((r.x, r.y, r.w, r.h), (0, 0, 500, 400));
        // 步骤级 verify_exact 显式覆盖全局
        assert_eq!(s.steps[4].verify_exact, Some(false));
    }

    #[test]
    fn step_verify_exact_overrides_meta() {
        // meta=true，步骤 1 显式 false → 步骤级覆盖；步骤 2 缺省 → 回退全局 true
        let s: Scenario = toml::from_str(
            r#"[meta]
verify_exact = true

[[step]]
action = "click_image"
verify_exact = false

[[step]]
action = "wait_image"
"#,
        )
        .expect("场景应能解析");
        assert!(s.meta.verify_exact);
        assert_eq!(s.steps[0].verify_exact, Some(false), "步骤级显式 false 应覆盖全局");
        assert_eq!(s.steps[1].verify_exact, None, "步骤未声明时应为 None（回退全局）");
    }

    #[test]
    fn empty_scenario_is_valid() {
        let s: Scenario = toml::from_str("").expect("空场景应可解析");
        assert!(s.steps.is_empty());
    }

    #[test]
    fn meta_verify_exact_defaults_to_false() {
        let s: Scenario = toml::from_str("[meta]\nname = \"x\"\n").expect("场景应可解析");
        assert!(!s.meta.verify_exact, "未声明 verify_exact 时应默认 false");
    }

    #[test]
    fn missing_action_field_is_error() {
        let toml = "[[step]]\nimage = \"x.png\"\n";
        assert!(toml::from_str::<Scenario>(toml).is_err(), "缺少 action 应报错");
    }
}
