//! 清理历史：JSONL 追加，一行一次批次。供仪表盘趋势与口径审计。

use crate::executor::CleanMode;
use crate::manifest::data_dir;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryRecord {
    pub session_id: String,
    pub created_unix: u64,
    pub mode: CleanMode,
    pub files: u64,
    /// 本次移入回收站/vault 的字节数（≠真实释放，口径见语义说明）
    pub bytes_moved: u64,
}

pub fn history_path() -> PathBuf {
    data_dir().join("history.jsonl")
}

pub fn append(rec: &HistoryRecord) -> std::io::Result<()> {
    fs::create_dir_all(data_dir())?;
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(history_path())?;
    f.write_all(serde_json::to_vec(rec)?.as_slice())?;
    f.write_all(b"\n")?;
    Ok(())
}

/// 全量读取（文件量级极小；Phase 4 引入索引缓存后再做分页）。
pub fn read_all() -> Vec<HistoryRecord> {
    match fs::read_to_string(history_path()) {
        Ok(s) => s
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect(),
        Err(_) => Vec::new(),
    }
}
