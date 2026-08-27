//! `zc-core` — ZDiskCleaner Pro v3 的纯逻辑内核。
//!
//! 分层：
//! - [`models`] 数据契约（Risk/Domain/Finding/ScanReport）
//! - [`scanner`] 单遍多 glob 并行 Walk 引擎 + 取消 + 真实进度事件
//! - [`guard`]   fail-closed 安全守卫（禁删区 / 自保护 / fail-closed 解析）
//! - [`executor`] 回收站 / vault / 重启队列三种执行模式 + 编排
//! - [`ledger`]   台账/历史统一 SQLite 存储（含旧 JSON 一次性导入）
//! - [`manifest`] 清理台账与还原
//! - [`history`] 历史记录（诚实口径）
//!
//! 本 crate 不依赖 UI 与 Tauri，可在 headless CLI 下独立验证。

pub mod analyze;
pub mod dedup;
pub mod error;
pub mod executor;
pub mod guard;
pub mod history;
pub mod ledger;
pub mod manifest;
pub mod migrate;
pub mod models;
pub mod patterns;
pub mod scanner;
pub mod startup;
pub mod system;

pub use error::{Error, Result};
pub use executor::CleanMode;
pub use models::{Domain, FileHit, Finding, Risk, ScanEvent, ScanReport};
pub use patterns::{expand_env, literal_root, norm};
pub use scanner::{dedup_nested, new_session_id, now_unix, RuleMatcher, ScanHandle};
pub use system::is_elevated;
