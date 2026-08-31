//! 步骤报告与汇总

use std::time::{Duration, Instant};

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
        let passed = self.steps.iter().filter(|s| s.status == Status::Pass).count();
        let failed = self.steps.len() - passed;
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

    pub fn all_passed(&self) -> bool {
        self.steps.iter().all(|s| s.status == Status::Pass)
    }
}
