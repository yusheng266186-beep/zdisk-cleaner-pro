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
            .undo_entries(&self.id)?
            .into_iter()
            .map(|(origin, vault_rel)| (PathBuf::from(origin), PathBuf::from(vault_rel)))
            .collect();
        let (done, failed) = super::executor::vault::restore(&moved);
        Ok((done, failed))
    }

    /// 彻底删除 vault 副本，返回 (已删项数, 实际释放字节, 失败列表)。
    /// 全部成功才抹台账行——半失败时保留账本，剩余项可重试或照常还原；
    /// 已删掉的项此后还原会如实报「副本已不存在」。
    pub fn purge_forever(&self) -> Result<(usize, u64, Vec<(String, String)>)> {
        if self.mode != CleanMode::Vault {
            return Err(crate::error::Error::Other(
                "回收站批次没有 vault 副本，请在系统回收站中清空".into(),
            ));
        }
        let copies = crate::ledger::LedgerStore::open()?.vault_copies(&self.id)?;
        let mut deleted = 0usize;
        let mut freed = 0u64;
        let mut failed: Vec<(String, String)> = Vec::new();
        for (_, rel, size) in copies {
            if rel.is_empty() {
                continue;
            }
            let p = PathBuf::from(&rel);
            if !p.exists() {
                // 副本已不在（如此前整批还原过）：目标状态已达成，不算失败，
                // 否则这类批次的台账会永远无法抹除、7 天过期清扫也永远清不掉
                deleted += 1;
                continue;
            }
            let r = if p.is_dir() {
                fs::remove_dir_all(&p)
            } else {
                fs::remove_file(&p)
            };
            match r {
                Ok(()) => {
                    deleted += 1;
                    freed += size;
                }
                Err(e) => failed.push((rel, e.to_string())),
            }
        }
        if failed.is_empty() {
            crate::ledger::LedgerStore::open()?.drop_manifest(&self.id)?;
            // 空会话目录一并移除，vault 下不留空壳
            let _ = fs::remove_dir_all(crate::executor::vault::vault_session_dir(&self.id));
        }
        Ok((deleted, freed, failed))
    }
}
