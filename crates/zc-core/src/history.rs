//! 清理历史：一行一次批次。供仪表盘趋势与口径审计。
//! 存储已迁至 SQLite（[`crate::ledger`]，ADR-002 收口）；`history.jsonl`
//! 仅作为一次性导入的遗留载体，导入后改名 `.imported` 留档。

use crate::executor::CleanMode;
use crate::ledger::LedgerStore;
use crate::manifest::data_dir;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HistoryRecord {
    pub session_id: String,
    pub created_unix: u64,
    pub mode: CleanMode,
    pub files: u64,
    /// 本次移入回收站/vault 的字节数（≠真实释放，口径见语义说明）
    pub bytes_moved: u64,
    /// v5：批次种类标签（如 "clean" / "manual_vault" / "recycle_bin_empty"），
    /// 旧记录为 None（serde default 兼容）。
    #[serde(default)]
    pub kind: Option<String>,
    /// v5：迁移类历史的源路径（非迁移批次为 None）。
    #[serde(default)]
    pub src: Option<String>,
    /// v5：迁移类历史的目标路径（非迁移批次为 None）。
    #[serde(default)]
    pub dst: Option<String>,
}

/// 遗留 JSONL 的位置：仅一次性导入流程使用；导入成功后改名加 `.imported` 后缀。
pub fn history_path() -> PathBuf {
    data_dir().join("history.jsonl")
}

pub fn append(rec: &HistoryRecord) -> io::Result<()> {
    LedgerStore::open()?.append_history(rec)
}

/// 全量读取，按写入序返回（文件量级极小；Phase 4 引入索引缓存后再做分页）。
pub fn read_all() -> Vec<HistoryRecord> {
    match LedgerStore::open() {
        Ok(s) => s.read_history(),
        Err(_) => Vec::new(),
    }
}
