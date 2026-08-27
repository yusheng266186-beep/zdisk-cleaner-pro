//! 清理台账：每一批操作的完整账目，可整批/单项还原。
//! 存储层已迁至 SQLite（见 [`crate::ledger`] 与 ADR-002）；本模块只承载
//! 结构定义与业务语义，对 CLI/UI 的公共 API 保持不变。

use crate::error::Result;
use crate::executor::CleanMode;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub origin: String,
    /// vault 内副本绝对路径；回收站模式为空串
    #[serde(default)]
    pub vault_rel: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanManifest {
    pub id: String,
    pub created_unix: u64,
    pub mode: CleanMode,
    pub entries: Vec<ManifestEntry>,
}

/// 数据根目录：优先 `ZC_DATA_DIR`（测试与便携模式），否则 `%LOCALAPPDATA%\ZDiskCleanerPro3`。
pub fn data_dir() -> PathBuf {
    if let Ok(d) = std::env::var("ZC_DATA_DIR") {
        if !d.is_empty() {
            return PathBuf::from(d);
        }
    }
    let lad = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(lad).join("ZDiskCleanerPro3")
}

impl CleanManifest {
    fn db_path() -> PathBuf {
        data_dir().join(crate::ledger::LEDGER_DB_FILE)
    }

    pub fn save(&self) -> Result<()> {
        crate::ledger::LedgerStore::open()?.save_manifest(self)?;
        Ok(())
    }

    pub fn load(id: &str) -> Result<Self> {
        let store = crate::ledger::LedgerStore::open()?;
        store.load_manifest(id).ok_or_else(|| {
            crate::error::Error::Other(format!(
                "台账 {id} 不存在或不可读（台账库 {:?}）",
                Self::db_path()
            ))
        })
    }

    /// 还原 vault 批次。回收站批次无法程序化还原，返回说明性错误。
    pub fn undo(&self) -> Result<(usize, Vec<(std::path::PathBuf, String)>)> {
        if self.mode != CleanMode::Vault {
            return Err(crate::error::Error::Other(
                "回收站模式请在系统回收站中还原（本批未进 vault）".into(),
            ));
        }
        let moved: Vec<(PathBuf, PathBuf)> = crate::ledger::LedgerStore::open()?
            .undo_entries(&self.id)
            .into_iter()
            .map(|(origin, vault_rel)| (PathBuf::from(origin), PathBuf::from(vault_rel)))
            .collect();
        let (done, failed) = super::executor::vault::restore(&moved);
        Ok((done, failed))
    }

    pub fn purge_vault_copies(&self) -> Result<usize> {
        let mut n = 0;
        for (_, vault_rel) in crate::ledger::LedgerStore::open()?.undo_entries(&self.id) {
            if !vault_rel.is_empty() {
                let p = PathBuf::from(&vault_rel);
                if p.is_file() {
                    fs::remove_file(&p)?;
                } else if p.is_dir() {
                    fs::remove_dir_all(&p)?;
                }
                n += 1;
            }
        }
        Ok(n)
    }
}
