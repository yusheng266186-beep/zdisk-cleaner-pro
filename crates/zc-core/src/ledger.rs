//! 台账/历史统一 SQLite 存储层（ADR-002 收口）。
//!
//! 库文件为 `{data_dir()}/ledger.db`（rusqlite `bundled` 特性自带编译，
//! 无外部 SQLite 依赖）。[`LedgerStore::open`] 自动建表并执行**一次性导入**：
//! 若 manifests/history 表为空而旧 JSON（`manifests/*.json`、
//! `history.jsonl`）存在，则解析入库后把旧文件改名加后缀 `.imported`
//! 留档。导入置于 IMMEDIATE 事务内，配合 busy_timeout 把并发首开串行化，
//! 多进程同时启动也只会迁移一次。
//!
//! 序稳定性约定：entries 行按 rowid（插入序）读取，因此
//! [`LedgerStore::undo_entries`] 的返回序与台账条目原序一致；
//! history 同样按追加序返回。

use crate::executor::CleanMode;
use crate::history::HistoryRecord;
use crate::manifest::{data_dir, CleanManifest, ManifestEntry};
use rusqlite::{Connection, TransactionBehavior};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 台账库文件名（位于 [`data_dir`] 下）。
pub(crate) const LEDGER_DB_FILE: &str = "ledger.db";

/// 表结构冻结自 ADR-002 决策第 4 条：字段平铺、无嵌套多态。
const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS manifests (
    id TEXT PRIMARY KEY,
    created_unix INTEGER NOT NULL,
    mode TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS entries (
    manifest_id TEXT NOT NULL,
    origin TEXT NOT NULL,
    vault_rel TEXT NOT NULL,
    size INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS history (
    session_id TEXT PRIMARY KEY,
    created_unix INTEGER NOT NULL,
    mode TEXT NOT NULL,
    files INTEGER NOT NULL,
    bytes_moved INTEGER NOT NULL
);
";

fn sqlite_io(e: rusqlite::Error) -> io::Error {
    io::Error::new(io::ErrorKind::Other, e.to_string())
}

/// 遗留旧文件改名：任意路径安全地附加 `.imported` 后缀。
fn imported_path(p: &Path) -> PathBuf {
    let mut os = p.as_os_str().to_os_string();
    os.push(".imported");
    PathBuf::from(os)
}

/// mode 字段沿用 executor::CleanMode 的 serde 蛇形命名（"recycle_bin"/"vault"），
/// 不另写一份映射，防止两处定义漂移。
fn mode_to_str(mode: &CleanMode) -> String {
    serde_json::to_string(mode)
        .expect("无字段枚举序列化不可能失败")
        .trim_matches('"')
        .to_string()
}

fn mode_from_str(s: &str) -> Option<CleanMode> {
    serde_json::from_str(&format!("\"{s}\"")).ok()
}

fn upsert_manifest(c: &Connection, m: &CleanManifest) -> rusqlite::Result<()> {
    c.execute(
        "INSERT OR REPLACE INTO manifests(id, created_unix, mode) VALUES (?1, ?2, ?3)",
        rusqlite::params![m.id, m.created_unix as i64, mode_to_str(&m.mode)],
    )?;
    // 幂等重存：先清旧行再按原序插入，保证 entries 的 rowid 序 == 原条目序
    c.execute(
        "DELETE FROM entries WHERE manifest_id = ?1",
        rusqlite::params![m.id],
    )?;
    for e in &m.entries {
        c.execute(
            "INSERT INTO entries(manifest_id, origin, vault_rel, size) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![m.id, e.origin, e.vault_rel, e.size as i64],
        )?;
    }
    Ok(())
}

fn table_count(c: &Connection, table: &str) -> rusqlite::Result<i64> {
    c.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
}

/// 一次性导入旧 JSON：对应表为空且旧文件存在才触发。
/// 成功入库的文件在事务提交后统一改名 `.imported` 留档（解析失败的损坏文件
/// 保留原地，供人工处置，不静默丢弃）。
fn import_legacy_json(conn: &mut Connection) -> io::Result<()> {
    // IMMEDIATE 写锁 + busy_timeout：并发首开时后到者等锁，醒来看到表非空即跳过
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(sqlite_io)?;

    let mut consumed: Vec<(PathBuf, PathBuf)> = Vec::new();

    // ---- manifests/*.json → manifests/entries 表
    if table_count(&tx, "manifests").map_err(sqlite_io)? == 0 {
        if let Ok(rd) = fs::read_dir(data_dir().join("manifests")) {
            let mut files: Vec<PathBuf> = rd
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .collect();
            files.sort(); // 文件名序 = 导入序，保证确定性
            for p in files {
                let Ok(raw) = fs::read_to_string(&p) else {
                    continue;
                };
                let Ok(m) = serde_json::from_str::<CleanManifest>(&raw) else {
                    continue;
                };
                upsert_manifest(&tx, &m).map_err(sqlite_io)?;
                consumed.push((p.clone(), imported_path(&p)));
            }
        }
    }

    // ---- history.jsonl → history 表（逐行解析，坏行跳过）
    if table_count(&tx, "history").map_err(sqlite_io)? == 0 {
        let hp = data_dir().join("history.jsonl");
        if hp.is_file() {
            if let Ok(raw) = fs::read_to_string(&hp) {
                let mut any = false;
                for line in raw.lines() {
                    if line.is_empty() {
                        continue;
                    }
                    if let Ok(rec) = serde_json::from_str::<HistoryRecord>(line) {
                        insert_history(&tx, &rec).map_err(sqlite_io)?;
                        any = true;
                    }
                }
                if any {
                    consumed.push((hp.clone(), imported_path(&hp)));
                }
            }
        }
    }

    tx.commit().map_err(sqlite_io)?;

    // 提交成功后才动旧文件；单个改名失败不阻断启动（表已非空，不会重复导入）
    for (from, to) in consumed {
        let _ = fs::rename(from, to);
    }
    Ok(())
}

fn insert_history(c: &Connection, rec: &HistoryRecord) -> rusqlite::Result<()> {
    c.execute(
        "INSERT OR REPLACE INTO history(session_id, created_unix, mode, files, bytes_moved)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            rec.session_id,
            rec.created_unix as i64,
            mode_to_str(&rec.mode),
            rec.files as i64,
            rec.bytes_moved as i64
        ],
    )?;
    Ok(())
}

/// Ledger 存储：持有一条独立连接。频度极低（每批次一次），不做全局缓存。
pub struct LedgerStore {
    conn: Connection,
}

impl LedgerStore {
    /// 打开台账库：建表 → 一次性导入遗留 JSON → 就绪。
    pub fn open() -> io::Result<Self> {
        let dir = data_dir();
        fs::create_dir_all(&dir)?;
        let mut conn = Connection::open(dir.join(LEDGER_DB_FILE)).map_err(sqlite_io)?;
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(sqlite_io)?;
        conn.execute_batch(SCHEMA_SQL).map_err(sqlite_io)?;
        import_legacy_json(&mut conn)?;
        Ok(Self { conn })
    }

    /// 保存整批台账（幂等：同 id 重存为覆盖）。
    pub fn save_manifest(&self, m: &CleanManifest) -> io::Result<()> {
        let tx = self.conn.unchecked_transaction().map_err(sqlite_io)?;
        upsert_manifest(&tx, m).map_err(sqlite_io)?;
        tx.commit().map_err(sqlite_io)?;
        Ok(())
    }

    /// 找不到或记录损坏时返回 None（由调用方翻译成面向用户的错误文案）。
    pub fn load_manifest(&self, id: &str) -> Option<CleanManifest> {
        let (created_unix, mode): (i64, String) = self
            .conn
            .query_row(
                "SELECT created_unix, mode FROM manifests WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok()?;
        let entries: Vec<ManifestEntry> = self
            .conn
            .prepare_cached(
                "SELECT origin, vault_rel, size FROM entries WHERE manifest_id = ?1 ORDER BY rowid",
            )
            .ok()?
            .query_map([id], |r| {
                Ok(ManifestEntry {
                    origin: r.get(0)?,
                    vault_rel: r.get(1)?,
                    size: r.get::<_, i64>(2)? as u64,
                })
            })
            .ok()?
            .filter_map(|r| r.ok())
            .collect();
        Some(CleanManifest {
            id: id.to_string(),
            created_unix: created_unix as u64,
            mode: mode_from_str(&mode)?,
            entries,
        })
    }

    /// 还原用条目对 `(origin, vault_rel)`，按台账插入原序返回。
    pub fn undo_entries(&self, id: &str) -> Vec<(String, String)> {
        match self
            .conn
            .prepare("SELECT origin, vault_rel FROM entries WHERE manifest_id = ?1 ORDER BY rowid")
        {
            Ok(mut st) => st
                .query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    /// 追加历史（同 session 重放按主键覆盖）。
    pub fn append_history(&self, rec: &HistoryRecord) -> io::Result<()> {
        insert_history(&self.conn, rec).map_err(sqlite_io)?;
        Ok(())
    }

    /// 全量历史，按写入序返回。
    pub fn read_history(&self) -> Vec<HistoryRecord> {
        match self.conn.prepare(
            "SELECT session_id, created_unix, mode, files, bytes_moved FROM history ORDER BY rowid",
        ) {
            Ok(mut st) => st
                .query_map([], |r| {
                    Ok(HistoryRecord {
                        session_id: r.get(0)?,
                        created_unix: r.get::<_, i64>(1)? as u64,
                        mode: {
                            let s: String = r.get(2)?;
                            mode_from_str(&s).unwrap_or(CleanMode::RecycleBin)
                        },
                        files: r.get::<_, i64>(3)? as u64,
                        bytes_moved: r.get::<_, i64>(4)? as u64,
                    })
                })
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }
}
