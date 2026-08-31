//! `zc-core` — ZDiskCleaner Pro v5 的纯逻辑内核。
//!
//! 分层：
//! - [`models`] 数据契约（Risk/Domain/Finding/ScanReport，含 skipped/诚实溢出）
//! - [`scanner`] 多 glob 单遍 **rayon 并行** Walk 引擎 + 取消 + 真实进度 + 年龄过滤
//! - [`guard`]   fail-closed 安全守卫（env 派生禁删区 / USERPROFILE fail-closed /
//!   elevated allowlist 目录级豁免，仅提权进程生效）
//! - [`executor`] 回收站 / vault（journal 化）/ 重启队列三种执行模式 + 编排
//! - [`ledger`]   台账/历史统一 SQLite 存储（WAL、幂等列迁移、journal 辅助）
//! - [`manifest`] 清理台账与还原
//! - [`history`] 历史记录（诚实口径）
//! - [`recycle_bin`] 回收站查询与一键清空（v5 新增）
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
pub mod recycle_bin;
pub mod scanner;
pub mod startup;
pub mod system;

pub use error::{Error, Result};
pub use executor::CleanMode;
pub use models::{Domain, FileHit, Finding, Risk, ScanEvent, ScanReport};
pub use patterns::{expand_env, literal_root, norm};
pub use recycle_bin::{RecycleBinInfo, RecycleBinSummary};
pub use scanner::{dedup_nested, new_session_id, now_unix, RuleMatcher, ScanHandle};
pub use system::is_elevated;
