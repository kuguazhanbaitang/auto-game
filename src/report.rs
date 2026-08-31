//! 步骤报告与汇总（文本 + HTML）

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::Result;

/// 步骤状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pass,
    Fail,
}

/// 单步报告
#[derive(Debug, Clone)]
pub struct StepReport {
    pub index: usize,
    pub action: String,
    pub detail: String,
    pub status: Status,
    pub duration: Duration,
}

/// 报告收集器
#[derive(Debug, Default)]
pub struct Report {
    pub scenario_name: String,
    pub started: Option<Instant>,
    pub steps: Vec<StepReport>,
}

impl Report {
    pub fn new(scenario_name: String) -> Self {
        Report {
            scenario_name,
            started: Some(Instant::now()),
            steps: Vec::new(),
        }
    }

    pub fn record(
        &mut self,
        index: usize,
        action: &str,
        detail: String,
        status: Status,
        duration: Duration,
    ) {
        self.steps.push(StepReport {
            index,
            action: action.to_string(),
            detail,
            status,
            duration,
        });
    }

    /// 输出文本报告
    pub fn print(&self) {
        let (passed, failed) = self.summary();
        println!("==== auto-game 测试报告 ====");
        println!("场景: {}", self.scenario_name);
        println!("结果: 通过 {} / 失败 {} / 总计 {}", passed, failed, self.steps.len());
        for s in &self.steps {
            let mark = if s.status == Status::Pass { "PASS" } else { "FAIL" };
            println!(
                "[{:>2}] {:<4} {:<16} {:>8}ms  {}",
                s.index,
                mark,
                s.action,
                s.duration.as_millis(),
                s.detail
            );
        }
        println!("============================");
    }

    /// 写出 HTML 报告
    pub fn write_html(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.render_html())?;
        Ok(())
    }

    fn summary(&self) -> (usize, usize) {
        let passed = self.steps.iter().filter(|s| s.status == Status::Pass).count();
        (passed, self.steps.len() - passed)
    }

    fn render_html(&self) -> String {
        let (passed, failed) = self.summary();
        let total_ms: u128 = self.steps.iter().map(|s| s.duration.as_millis()).sum();
        let mut rows = String::new();
        for s in &self.steps {
            let cls = if s.status == Status::Pass { "pass" } else { "fail" };
            let mark = if s.status == Status::Pass { "PASS" } else { "FAIL" };
            rows.push_str(&format!(
                "<tr class=\"{}\"><td>{}</td><td>{}</td><td>{}</td><td>{} ms</td><td>{}</td></tr>\n",
                cls,
                s.index,
                mark,
                html_escape(&s.action),
                s.duration.as_millis(),
                html_escape(&s.detail),
            ));
        }
        format!(
            r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<title>auto-game 测试报告 - {}</title>
<style>
  body {{ font-family: "Segoe UI", "Microsoft YaHei", sans-serif; margin: 2rem; color: #333; }}
  h1 {{ font-size: 1.4rem; border-bottom: 2px solid #4a7dff; padding-bottom: .4rem; }}
  .summary {{ margin: 1rem 0; padding: .8rem 1rem; background: #f5f7fa; border-radius: 6px; }}
  .summary b.pass {{ color: #2e9e44; }}
  .summary b.fail {{ color: #d93025; }}
  table {{ border-collapse: collapse; width: 100%; }}
  th, td {{ border: 1px solid #e0e0e0; padding: .45rem .7rem; text-align: left; font-size: .9rem; }}
  th {{ background: #eef2ff; }}
  tr.pass td:nth-child(2) {{ color: #2e9e44; font-weight: 600; }}
  tr.fail td:nth-child(2) {{ color: #d93025; font-weight: 600; }}
  tr.fail {{ background: #fef0ef; }}
</style>
</head>
<body>
<h1>auto-game 测试报告</h1>
<div class="summary">
  场景：<b>{}</b><br>
  结果：通过 <b class="pass">{}</b> / 失败 <b class="fail">{}</b> / 总计 {} 步 · 总耗时 {} ms
</div>
<table>
<thead><tr><th>#</th><th>状态</th><th>动作</th><th>耗时</th><th>详情</th></tr></thead>
<tbody>
{}
</tbody>
</table>
</body>
</html>
"#,
            html_escape(&self.scenario_name),
            html_escape(&self.scenario_name),
            passed,
            failed,
            self.steps.len(),
            total_ms,
            rows,
        )
    }

    pub fn all_passed(&self) -> bool {
        self.steps.iter().all(|s| s.status == Status::Pass)
    }
}

/// 转义 HTML 特殊字符，防止注入
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
