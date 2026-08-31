//! auto-game —— 通用电脑游戏自动化测试框架
//!
//! CLI 入口：auto-game run <场景.toml> [--assets <资源目录>]

use std::path::PathBuf;

use anyhow::{Result, bail};

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 || args[1] != "run" {
        bail!("用法: auto-game run <场景.toml> [--assets <资源目录>]");
    }

    let scenario_path = PathBuf::from(&args[2]);
    let mut assets_dir = PathBuf::from("assets");
    let mut i = 3;
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
