//! auto-game —— 通用电脑游戏自动化测试框架
//!
//! CLI 入口：
//!   auto-game run <场景.toml> [--assets <资源目录>]
//!   auto-game template [选项]   # 模板采集：截图存 assets + 输出坐标/TOML 片段

use std::path::PathBuf;

use anyhow::{Result, bail};
use image::RgbaImage;

use auto_game::adapter::{CaptureBackend, CaptureTrait};

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("");
    match cmd {
        "run" => cmd_run(&args[2..]),
        "template" => cmd_template(&args[2..]),
        _ => bail!(
            "用法:\n  auto-game run <场景.toml> [--assets <资源目录>]\n  auto-game template [选项]  # 模板采集"
        ),
    }
}

// ---------------- run：执行场景 ----------------

fn cmd_run(args: &[String]) -> Result<()> {
    if args.is_empty() {
        bail!("用法: auto-game run <场景.toml> [--assets <资源目录>]");
    }
    let scenario_path = PathBuf::from(&args[0]);
    let mut assets_dir = PathBuf::from("assets");
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--assets" if i + 1 < args.len() => {
                assets_dir = PathBuf::from(&args[i + 1]);
                i += 2;
            }
            _ => i += 1,
        }
    }

    let passed = auto_game::engine::run_scenario(&scenario_path, assets_dir)?;
    if passed {
        println!("✅ 场景全部通过");
        Ok(())
    } else {
        println!("❌ 场景存在失败步骤（详见上方报告）");
        Ok(())
    }
}

// ---------------- template：模板采集 ----------------
//
// 两种采集模式：
//   1) 区域模式：--x --y --w --h 精确截取指定屏幕区域
//   2) 鼠标模式：--at-mouse [--center] --w --h 以鼠标位置为左上角/中心截取
//   可选 --full 全屏截图（用于定位元素坐标）、--preview 额外存全屏预览
//
// 输出：模板 PNG 保存到 --out（默认 assets/），并打印可直接粘贴进场景的 TOML 片段
// （含 image 路径与 region 坐标，region 同时用于限定搜索范围以提升性能）。

fn cmd_template(args: &[String]) -> Result<()> {
    let mut name: Option<String> = None;
    let mut out = PathBuf::from("assets");
    let mut x: Option<i32> = None;
    let mut y: Option<i32> = None;
    let mut w: Option<u32> = None;
    let mut h: Option<u32> = None;
    let mut at_mouse = false;
    let mut center = false;
    let mut full = false;
    let mut preview = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--name" if i + 1 < args.len() => {
                name = Some(args[i + 1].clone());
                i += 2;
            }
            "--out" if i + 1 < args.len() => {
                out = PathBuf::from(&args[i + 1]);
                i += 2;
            }
            "--x" if i + 1 < args.len() => {
                x = args[i + 1].parse().ok();
                i += 2;
            }
            "--y" if i + 1 < args.len() => {
                y = args[i + 1].parse().ok();
                i += 2;
            }
            "--w" if i + 1 < args.len() => {
                w = args[i + 1].parse().ok();
                i += 2;
            }
            "--h" if i + 1 < args.len() => {
                h = args[i + 1].parse().ok();
                i += 2;
            }
            "--at-mouse" => {
                at_mouse = true;
                i += 1;
            }
            "--center" => {
                center = true;
                i += 1;
            }
            "--full" => {
                full = true;
                i += 1;
            }
            "--preview" => {
                preview = true;
                i += 1;
            }
            other => bail!("未知参数: {other}"),
        }
    }

    let name = name.ok_or_else(|| anyhow::anyhow!("template 需要 --name <模板名>（如 login_btn）"))?;

    // 计算截图区域并采集
    let capture = CaptureBackend;
    let (img, region): (RgbaImage, (i32, i32, u32, u32));

    if full {
        img = capture.capture_full()?;
        region = (0, 0, img.width(), img.height());
    } else {
        let (w, h) = match (w, h) {
            (Some(w), Some(h)) => (w, h),
            _ => bail!("区域截图需要 --w <宽> --h <高>"),
        };
        let (rx, ry) = if at_mouse {
            use device_query::{DeviceQuery, DeviceState};
            let ds = DeviceState::new();
            let (mx, my) = ds.get_mouse().coords;
            let (mut rx, mut ry) = if center {
                (mx - (w / 2) as i32, my - (h / 2) as i32)
            } else {
                (mx, my)
            };
            // 防止越界（crop 越界会 panic）
            let monitor = xcap::Monitor::from_point(0, 0)?;
            let max_x = monitor.width().saturating_sub(w) as i32;
            let max_y = monitor.height().saturating_sub(h) as i32;
            rx = rx.clamp(0, max_x);
            ry = ry.clamp(0, max_y);
            (rx, ry)
        } else {
            let x = x.ok_or_else(|| anyhow::anyhow!("区域模式需要 --x <坐标> --y <坐标>，或用 --at-mouse 以鼠标定位"))?;
            let y = y.ok_or_else(|| anyhow::anyhow!("区域模式需要 --x <坐标> --y <坐标>，或用 --at-mouse 以鼠标定位"))?;
            (x, y)
        };
        img = capture.capture_region(rx, ry, w, h)?;
        region = (rx, ry, w, h);
    }

    // 保存模板
    std::fs::create_dir_all(&out)?;
    let fname = format!("{name}.png");
    let path = out.join(&fname);
    img.save(&path)?;
    println!("✅ 模板已保存: {}  ({}x{})", path.display(), img.width(), img.height());

    // 可选：全屏预览（便于在图片中定位其它元素坐标）
    if preview {
        let full_img = capture.capture_full()?;
        let reports = PathBuf::from("reports");
        std::fs::create_dir_all(&reports)?;
        let pv = reports.join(format!("preview_{name}.png"));
        full_img.save(&pv)?;
        println!("📋 全屏预览: {}（打开后可用看图工具读取任意元素像素坐标）", pv.display());
    }

    // 输出可直接粘贴进场景的 TOML 片段
    if !full {
        let (rx, ry, rw, rh) = region;
        println!();
        println!("在场景中直接使用（复制以下片段）：");
        println!("[[step]]");
        println!("action = \"click_image\"");
        println!("image = \"{fname}\"");
        println!("precision = 0.85");
        println!("region = {{ x = {rx}, y = {ry}, w = {rw}, h = {rh} }}");
    } else {
        println!();
        println!("全屏模板通常用于定位元素：打开 {fname} 找到元素像素坐标 (px, py)，");
        println!("再用 auto-game template --name <元素> --x <px> --y <py> --w <宽> --h <高> 精确裁剪。");
    }
    Ok(())
}
