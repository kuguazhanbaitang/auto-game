//! auto-game —— 通用电脑游戏自动化测试框架
//!
//! 分层：adapter（可插拔后端）→ action（动作原语）→ engine（流程引擎）→ script（TOML 场景）

pub mod action;
pub mod adapter;
pub mod engine;
pub mod gui;
pub mod report;
pub mod script;
