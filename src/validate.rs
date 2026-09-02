//! 场景静态校验（`validate` 子命令）：不实际运行场景，预先检查
//! TOML 语法、动作合法性、必填参数、模板文件存在、控制流闭合与
//! OCR 模型存在，把常见错误挡在运行期之前——相当于「上线前体检」。

use std::path::Path;

use anyhow::Result;

use crate::adapter::key_from_str;
use crate::script::{Scenario, Step};

/// 校验级别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Error,
    Warning,
}

impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Level::Error => "ERROR",
            Level::Warning => "WARN",
        }
    }
}

/// 一条校验问题
#[derive(Debug, Clone)]
pub struct Issue {
    pub level: Level,
    /// 对应步骤序号（1-based）；None 表示场景级问题
    pub step: Option<usize>,
    pub message: String,
}

/// 校验结果：全部通过 = 无 Error
pub struct ValidationResult {
    pub issues: Vec<Issue>,
}

impl ValidationResult {
    pub fn errors(&self) -> usize {
        self.issues.iter().filter(|i| i.level == Level::Error).count()
    }
    pub fn warnings(&self) -> usize {
        self.issues.iter().filter(|i| i.level == Level::Warning).count()
    }
    pub fn passed(&self) -> bool {
        self.errors() == 0
    }
}

/// 合法动作集合（引擎支持的普通动作 + 控制流关键字）
const KNOWN_ACTIONS: &[&str] = &[
    // 普通动作
    "screenshot", "wait", "move_mouse", "click", "key_press", "key_combo", "type_text",
    "find_image", "wait_image", "click_image", "assert_image",
    "ocr_text", "click_text", "assert_text",
    // 控制流
    "repeat", "end_repeat", "if_image", "if_text", "else", "end_if",
];

/// 各动作的必填参数（缺少即 ERROR）
fn required_params(action: &str) -> &'static [&'static str] {
    match action {
        "move_mouse" | "click" => &["x", "y"],
        "key_press" => &["key"],
        "key_combo" => &["keys"],
        "type_text" => &["text"],
        "find_image" | "wait_image" | "click_image" | "assert_image" | "if_image" => &["image"],
        "if_text" | "click_text" | "assert_text" => &["text"],
        "repeat" => &["count"],
        _ => &[],
    }
}

/// 是否为需要 image 字段的动作
fn needs_image(action: &str) -> bool {
    matches!(
        action,
        "find_image" | "wait_image" | "click_image" | "assert_image" | "if_image"
    )
}

/// 是否为需要 OCR 模型的动作
fn needs_ocr(action: &str) -> bool {
    matches!(action, "ocr_text" | "if_text" | "click_text" | "assert_text")
}

/// OCR 模型依赖（随仓库提供，缺失则 OCR 动作必然运行失败）
const OCR_FILES: &[(&str, &str)] = &[
    ("ch_PP-OCRv4_det_mobile.onnx", "检测模型"),
    ("ch_PP-OCRv4_rec_mobile.onnx", "识别模型"),
    ("ppocr_keys_v1.txt", "字典"),
];

/// 校验单个步骤，追加问题到 issues
fn check_step(step: &Step, idx: usize, assets_dir: &Path, issues: &mut Vec<Issue>) {
    let step_no = idx + 1;
    let ctx = || format!("步骤 #{step_no} ({})", step.action);

    // 1. 动作合法性（未知动作后续检查无意义，直接返回）
    if !KNOWN_ACTIONS.contains(&step.action.as_str()) {
        issues.push(Issue {
            level: Level::Error,
            step: Some(step_no),
            message: format!("未知动作类型 {:?}", step.action),
        });
        return;
    }

    // 2. 必填参数
    for p in required_params(&step.action) {
        let ok = match *p {
            "x" => step.x.is_some(),
            "y" => step.y.is_some(),
            "key" => step.key.is_some(),
            "keys" => step.keys.is_some(),
            "text" => step.text.is_some(),
            "image" => step.image.is_some(),
            "count" => step.count.is_some(),
            _ => true,
        };
        if !ok {
            issues.push(Issue {
                level: Level::Error,
                step: Some(step_no),
                message: format!("{} 缺少必填参数 {p}", ctx()),
            });
        }
    }

    // 3. 模板文件存在（引擎 resolve_asset 语义：相对 assets 目录）
    if let Some(img) = &step.image {
        let p = assets_dir.join(img);
        if !p.exists() {
            issues.push(Issue {
                level: Level::Error,
                step: Some(step_no),
                message: format!("{} 模板文件不存在: {}", ctx(), p.display()),
            });
        }
    }

    // 4. key / keys 名称合法性
    if let Some(k) = &step.key {
        if key_from_str(k).is_err() {
            issues.push(Issue {
                level: Level::Error,
                step: Some(step_no),
                message: format!("{} 非法按键 {:?}", ctx(), k),
            });
        }
    }
    if let Some(keys) = &step.keys {
        for k in keys {
            if key_from_str(k).is_err() {
                issues.push(Issue {
                    level: Level::Error,
                    step: Some(step_no),
                    message: format!("{} 非法组合键 {:?}", ctx(), k),
                });
            }
        }
    }

    // 5. 数值范围
    if let Some(p) = step.precision {
        if !(0.0..=1.0).contains(&p) {
            issues.push(Issue {
                level: Level::Error,
                step: Some(step_no),
                message: format!("{} precision {p} 超出范围 [0, 1]", ctx()),
            });
        }
    }
    if let Some(t) = step.timeout {
        if t <= 0.0 {
            issues.push(Issue {
                level: Level::Warning,
                step: Some(step_no),
                message: format!("{} timeout {t} 应 > 0（引擎将回退默认 15s）", ctx()),
            });
        }
    }
    if let Some(c) = step.count {
        if c == 0 {
            issues.push(Issue {
                level: Level::Warning,
                step: Some(step_no),
                message: format!("{} repeat count=0 将不执行循环体", ctx()),
            });
        }
    }
    if let (Some(min), Some(max)) = (step.click_delay_min, step.click_delay) {
        if min >= max {
            issues.push(Issue {
                level: Level::Warning,
                step: Some(step_no),
                message: format!(
                    "{} click_delay_min({min}) >= click_delay({max})，随机延时区间非法，将不延时",
                    ctx()
                ),
            });
        }
    }
    if let Some(j) = step.jitter {
        if j > 200 {
            issues.push(Issue {
                level: Level::Warning,
                step: Some(step_no),
                message: format!("{} jitter={j} 过大，可能点出目标元素", ctx()),
            });
        }
    }

    // 6. region 合法性（宽高必须 > 0）
    if let Some(r) = step.region {
        if r.w == 0 || r.h == 0 {
            issues.push(Issue {
                level: Level::Error,
                step: Some(step_no),
                message: format!("{} region 宽高必须 > 0（当前 {}x{}）", ctx(), r.w, r.h),
            });
        }
    }

    // 7. OCR 模型依赖
    if needs_ocr(&step.action) {
        let ocr_dir = assets_dir.join("ocr");
        for (f, what) in OCR_FILES {
            if !ocr_dir.join(f).exists() {
                issues.push(Issue {
                    level: Level::Error,
                    step: Some(step_no),
                    message: format!(
                        "{} 使用 OCR 但缺少模型文件 {}/{}（{}）",
                        ctx(),
                        ocr_dir.display(),
                        f,
                        what
                    ),
                });
            }
        }
    }

    // 8. 冗余字段提示：region 配在非图像/OCR/坐标动作上无意义
    if step.region.is_some()
        && !needs_image(&step.action)
        && !needs_ocr(&step.action)
        && !matches!(step.action.as_str(), "click" | "move_mouse")
    {
        issues.push(Issue {
            level: Level::Warning,
            step: Some(step_no),
            message: format!("{} region 对动作 {} 无意义（仅图像/OCR/坐标类动作使用）", ctx(), step.action),
        });
    }
}

/// 校验场景文件，返回全部问题（ERROR / WARN）。
/// 解析失败等场景级错误也作为 ERROR 返回（Result 仍为 Ok，便于统一打印）。
pub fn validate_scenario(path: &Path, assets_dir: &Path) -> Result<ValidationResult> {
    let mut issues = Vec::new();

    // 1. 读取 + 解析 TOML（语法/字段错误 → 场景级 ERROR）
    let scenario = match Scenario::load(path) {
        Ok(s) => s,
        Err(e) => {
            issues.push(Issue {
                level: Level::Error,
                step: None,
                message: format!("场景解析失败: {e:#}"),
            });
            return Ok(ValidationResult { issues });
        }
    };

    // 2. 场景级检查
    if scenario.steps.is_empty() {
        issues.push(Issue {
            level: Level::Warning,
            step: None,
            message: "场景没有任何步骤（空场景）".to_string(),
        });
    }
    if scenario.meta.name.as_deref().map_or(true, |n| n.trim().is_empty()) {
        issues.push(Issue {
            level: Level::Warning,
            step: None,
            message: "场景未设置 [meta] name（报告将显示为“未命名场景”）".to_string(),
        });
    }

    // 3. 控制流闭合（复用引擎 compile 检查，单一事实来源）
    if let Err(e) = crate::engine::check_control_flow(&scenario.steps) {
        issues.push(Issue {
            level: Level::Error,
            step: None,
            message: format!("控制流错误: {e:#}"),
        });
    }

    // 4. 逐步骤检查
    for (i, step) in scenario.steps.iter().enumerate() {
        check_step(step, i, assets_dir, &mut issues);
    }

    Ok(ValidationResult { issues })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// 写一个临时场景文件并返回路径
    fn write_scenario(toml: &str, name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("auto_game_validate_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, toml).unwrap();
        path
    }

    fn dummy_assets() -> PathBuf {
        // 用不存在的目录：模板存在性检查会报错；测试据此验证
        PathBuf::from("__no_such_assets__")
    }

    #[test]
    fn valid_scenario_passes() {
        let p = write_scenario(
            r#"[meta]
name = "ok"
[[step]]
action = "wait"
seconds = 1
[[step]]
action = "click"
x = 100
y = 200
"#,
            "valid.toml",
        );
        let r = validate_scenario(&p, &dummy_assets()).unwrap();
        assert!(r.passed(), "无错误的场景应通过");
    }

    #[test]
    fn unknown_action_is_error() {
        let p = write_scenario(
            r#"[[step]]
action = "fly_to_moon"
"#,
            "unknown_action.toml",
        );
        let r = validate_scenario(&p, &dummy_assets()).unwrap();
        assert!(!r.passed());
        assert!(r.issues.iter().any(|i| i.message.contains("未知动作")));
    }

    #[test]
    fn missing_required_params_is_error() {
        let p = write_scenario(
            r#"[[step]]
action = "click"
"#,
            "missing_params.toml",
        );
        let r = validate_scenario(&p, &dummy_assets()).unwrap();
        assert!(!r.passed());
        assert!(r.issues.iter().any(|i| i.message.contains("缺少必填参数 x")));
        assert!(r.issues.iter().any(|i| i.message.contains("缺少必填参数 y")));
    }

    #[test]
    fn missing_template_is_error() {
        let p = write_scenario(
            r#"[[step]]
action = "click_image"
image = "no_such.png"
"#,
            "missing_tpl.toml",
        );
        let r = validate_scenario(&p, &dummy_assets()).unwrap();
        assert!(!r.passed());
        assert!(r.issues.iter().any(|i| i.message.contains("模板文件不存在")));
    }

    #[test]
    fn unclosed_control_flow_is_error() {
        let p = write_scenario(
            r#"[[step]]
action = "repeat"
count = 3
[[step]]
action = "wait"
seconds = 1
"#,
            "unclosed.toml",
        );
        let r = validate_scenario(&p, &dummy_assets()).unwrap();
        assert!(!r.passed());
        assert!(r.issues.iter().any(|i| i.message.contains("控制流")));
    }

    #[test]
    fn stray_else_is_error() {
        let p = write_scenario(
            r#"[[step]]
action = "else"
"#,
            "stray_else.toml",
        );
        let r = validate_scenario(&p, &dummy_assets()).unwrap();
        assert!(!r.passed());
    }

    #[test]
    fn bad_key_is_error() {
        let p = write_scenario(
            r#"[[step]]
action = "key_press"
key = "not_a_key"
"#,
            "bad_key.toml",
        );
        let r = validate_scenario(&p, &dummy_assets()).unwrap();
        assert!(!r.passed());
        assert!(r.issues.iter().any(|i| i.message.contains("非法按键")));
    }

    #[test]
    fn ocr_missing_model_is_error() {
        let p = write_scenario(
            r#"[[step]]
action = "click_text"
text = "开始游戏"
"#,
            "ocr_no_model.toml",
        );
        let r = validate_scenario(&p, &dummy_assets()).unwrap();
        assert!(!r.passed());
        assert!(r.issues.iter().any(|i| i.message.contains("缺少模型文件")));
    }

    #[test]
    fn precision_out_of_range_is_error() {
        let p = write_scenario(
            r#"[[step]]
action = "wait_image"
image = "a.png"
precision = 1.5
"#,
            "bad_precision.toml",
        );
        let r = validate_scenario(&p, &dummy_assets()).unwrap();
        assert!(!r.passed());
        assert!(r.issues.iter().any(|i| i.message.contains("precision")));
    }

    #[test]
    fn malformed_toml_is_scene_error() {
        let p = write_scenario(
            r#"[meta
name = "broken"
"#,
            "broken.toml",
        );
        let r = validate_scenario(&p, &dummy_assets()).unwrap();
        assert!(!r.passed());
        assert!(r.issues.iter().any(|i| i.message.contains("解析失败")));
    }

    #[test]
    fn empty_scenario_warns_not_errors() {
        let p = write_scenario("", "empty.toml");
        let r = validate_scenario(&p, &dummy_assets()).unwrap();
        assert!(r.passed(), "空场景只是警告，不算错误");
        assert!(r.issues.iter().any(|i| i.message.contains("没有任何步骤")));
    }
}
