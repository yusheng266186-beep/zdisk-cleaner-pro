//! 清理台账（JSON）：每一批操作的完整账目，可整批/单项还原。
//! Phase 2 先用 JSON 存储；SQLite 迁移放到 MSVC 工具链就绪之后（见 ADR-002）。

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
    fn path(id: &str) -> PathBuf {
        data_dir().join("manifests").join(format!("{id}.json"))
    }

    pub fn save(&self) -> Result<()> {
        let p = Self::path(&self.id);
        fs::create_dir_all(p.parent().expect("manifest dir"))?;
        fs::write(p, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn load(id: &str) -> Result<Self> {
        let p = Self::path(id);
        let raw = fs::read_to_string(&p).map_err(|e| {
            crate::error::Error::Other(format!("台账 {id} 不存在或不可读（{p:?}）: {e}"))
        })?;
        Ok(serde_json::from_str(&raw)?)
    }

    /// 还原 vault 批次。回收站批次无法程序化还原，返回说明性错误。
    pub fn undo(&self) -> Result<(usize, Vec<(std::path::PathBuf, String)>)> {
        if self.mode != CleanMode::Vault {
            return Err(crate::error::Error::Other(
                "回收站模式请在系统回收站中还原（本批未进 vault）".into(),
            ));
        }
        let moved: Vec<(PathBuf, PathBuf)> = self
            .entries
            .iter()
            .map(|e| (PathBuf::from(&e.origin), PathBuf::from(&e.vault_rel)))
            .collect();
        let (done, failed) = super::executor::vault::restore(&moved);
        Ok((done, failed))
    }

    pub fn purge_vault_copies(&self) -> Result<usize> {
        let mut n = 0;
        for e in &self.entries {
            if !e.vault_rel.is_empty() {
                let p = PathBuf::from(&e.vault_rel);
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
