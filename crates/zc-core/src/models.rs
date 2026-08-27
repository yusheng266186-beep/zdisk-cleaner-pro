//! 贯穿扫描/清理全流程的数据模型。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 规则风险四级。默认只勾选 [`Risk::Safe`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    Safe,
    Caution,
    Risky,
    Expert,
}

/// 规则五大域。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Domain {
    System,
    Browser,
    Dev,
    Apps,
    Logs,
}

/// 一条命中文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileHit {
    pub path: PathBuf,
    pub size: u64,
    #[serde(default)]
    pub is_dir: bool,
}

/// 单条规则的扫描发现。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Finding {
    pub rule_id: String,
    pub hits: Vec<FileHit>,
    /// 命中数超出采集上限后仅累计的字节数（防止极端目录撑爆内存）。
    #[serde(default)]
    pub overflow_hits: u64,
    #[serde(default)]
    pub overflow_bytes: u64,
}

impl Finding {
    pub fn new(rule_id: impl Into<String>) -> Self {
        Self {
            rule_id: rule_id.into(),
            ..Default::default()
        }
    }

    pub fn total_bytes(&self) -> u64 {
        self.hits.iter().map(|h| h.size).sum::<u64>() + self.overflow_bytes
    }

    pub fn total_count(&self) -> u64 {
        self.hits.len() as u64 + self.overflow_hits
    }
}

/// 扫描进度事件（机制上杜绝假进度：只上报真实发生的事实）。
#[derive(Debug, Clone, Copy)]
pub enum ScanEvent {
    Entry { files: u64, bytes_seen: u64 },
    Done { files: u64, bytes_seen: u64 },
}

/// 一次完整扫描的报告。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanReport {
    pub id: String,
    pub started_unix: u64,
    pub duration_ms: u64,
    pub files_seen: u64,
    pub bytes_seen: u64,
    pub cancelled: bool,
    pub findings: Vec<Finding>,
}

impl ScanReport {
    pub fn cleanable_bytes(&self) -> u64 {
        self.findings.iter().map(|f| f.total_bytes()).sum()
    }

    pub fn cleanable_count(&self) -> u64 {
        self.findings.iter().map(|f| f.total_count()).sum()
    }
}

/// 人类可读字节数（沿用 v2 的"诚实"口径，不做单位虚高换算）。
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{bytes:.0} {}", UNITS[u])
    } else {
        format!("{v:.2} {}", UNITS[u])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_units() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1024), "1.00 KB");
        assert_eq!(human_size(1536 * 1024 * 1024), "1.50 GB");
    }

    #[test]
    fn finding_sums_with_overflow() {
        let mut f = Finding::new("r");
        f.hits.push(FileHit { path: PathBuf::from("a"), size: 10, is_dir: false });
        f.overflow_bytes = 5;
        f.overflow_hits = 1;
        assert_eq!(f.total_bytes(), 15);
        assert_eq!(f.total_count(), 2);
    }
}
