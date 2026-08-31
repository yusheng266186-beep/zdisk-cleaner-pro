//! 台账/历史统一 SQLite 存储层（ADR-002 收口）。
//!
//! 库文件为 `{data_dir()}/ledger.db`（rusqlite `bundled` 特性自带编译，
//! 无外部 SQLite 依赖）。[`LedgerStore::open`] 自动建表、开 WAL、执行
//! **幂等列迁移**（v5 journal/历史扩展列）并做**一次性导入**：
//! 若 manifests/history 表为空而旧 JSON（`manifests/*.json`、
//! `history.jsonl`）存在，则解析入库后把旧文件改名加后缀 `.imported`
//! 留档。导入置于 IMMEDIATE 事务内，配合 busy_timeout 把并发首开串行化，
//! 多进程同时启动也只会迁移一次。
//!
//! 序稳定性约定：entries 行按 rowid（插入序）读取，因此
//! [`LedgerStore::undo_entries`] 的返回序与台账条目原序一致；
//! history 同样按追加序返回。
//!
//! v5 契约（审计 S1/S2/S3、A1 台账侧）：
//! - `live_manifest_ids`/`vault_copies`/`undo_entries` 不再吞错：任何
//!   prepare/行解码失败一律 Err 上抛，孤儿 GC 据此熔断；
//! - entries 增列 `status`（'committed' | 'pending'）——vault stash 改为
//!   journal 化：move 前先落 pending 台账，逐条成功 UPDATE 为 committed；
//! - history 增列 `kind`/`src`/`dst`（可空）；
//! - 同 id 且已有条目的覆盖式 `save_manifest` 显式报错（S3：秒级批次 id
//!   碰撞不得再抹掉整批台账）。

use crate::executor::CleanMode;
use crate::history::HistoryRecord;
use crate::manifest::{data_dir, CleanManifest, ManifestEntry};
use rusqlite::{Connection, TransactionBehavior};
use serde::Serialize;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 台账库文件名（位于 [`data_dir`] 下）。
pub(crate) const LEDGER_DB_FILE: &str = "ledger.db";

/// 表结构冻结自 ADR-002 决策第 4 条：字段平铺、无嵌套多态。
/// v5 扩展列（status/kind/src/dst）经 [`migrate_columns`] 幂等补齐，
/// 老库与新库共用本 DDL——新库建表后也会被 ALTER 补列（无害）。
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
    io::Error::other(e.to_string())
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

/// 幂等列迁移：PRAGMA table_info 探测，缺列才 ALTER（可重入）。
fn migrate_columns(c: &Connection) -> io::Result<()> {
    let mut needed: Vec<&'static str> = Vec::new();
    if !column_exists(c, "entries", "status")? {
        needed.push("ALTER TABLE entries ADD COLUMN status TEXT NOT NULL DEFAULT 'committed'");
    }
    for (col, ddl) in [
        ("kind", "ALTER TABLE history ADD COLUMN kind TEXT"),
        ("src", "ALTER TABLE history ADD COLUMN src TEXT"),
        ("dst", "ALTER TABLE history ADD COLUMN dst TEXT"),
    ] {
        if !column_exists(c, "history", col)? {
            needed.push(ddl);
        }
    }
    for ddl in needed {
        c.execute_batch(ddl).map_err(sqlite_io)?;
    }
    Ok(())
}

fn column_exists(c: &Connection, table: &str, col: &str) -> io::Result<bool> {
    let mut st = c
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(sqlite_io)?;
    let rows = st.query_map([], |r| r.get::<_, String>(1)).map_err(sqlite_io)?;
    let mut found = false;
    for name in rows {
        if name.map_err(sqlite_io)? == col {
            found = true;
        }
    }
    Ok(found)
}

/// 覆盖式重存（仅供旧 JSON 一次性导入等确知可覆盖的路径使用）。
fn overwrite_manifest(c: &Connection, m: &CleanManifest) -> rusqlite::Result<()> {
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
                overwrite_manifest(&tx, &m).map_err(sqlite_io)?;
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
        "INSERT OR REPLACE INTO history(session_id, created_unix, mode, files, bytes_moved, kind, src, dst)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            rec.session_id,
            rec.created_unix as i64,
            mode_to_str(&rec.mode),
            rec.files as i64,
            rec.bytes_moved as i64,
            rec.kind,
            rec.src,
            rec.dst
        ],
    )?;
    Ok(())
}

/// 会话条目 DTO（历史下钻用，CONTRACT §1/§2 SessionEntryDto）。
#[derive(Debug, Clone, Serialize)]
pub struct SessionEntryDto {
    pub origin: String,
    pub vault_rel: String,
    pub size: u64,
    /// 'committed' | 'pending'
    pub status: String,
}

/// 过期批次三元组：(批次 id, 记录总字节, [(origin, vault_rel, size)])。
pub type ExpiredBatch = (String, u64, Vec<(String, String, u64)>);

/// Ledger 存储：持有一条独立连接。频度极低（每批次一次），不做全局缓存。
pub struct LedgerStore {
    conn: Connection,
}

impl LedgerStore {
    /// 打开台账库：建表 → WAL → 幂等列迁移 → 一次性导入遗留 JSON → 就绪。
    pub fn open() -> io::Result<Self> {
        let dir = data_dir();
        fs::create_dir_all(&dir)?;
        let mut conn = Connection::open(dir.join(LEDGER_DB_FILE)).map_err(sqlite_io)?;
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(sqlite_io)?;
        // WAL：读历史长查询与写台账不再互斥，消灭「清理成功但台账写失败」
        // 的 busy 超时窗口（审计 S2）。个别文件系统不支持时退回默认期刊模式，
        // 不阻断启动。
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        conn.execute_batch(SCHEMA_SQL).map_err(sqlite_io)?;
        migrate_columns(&conn)?;
        import_legacy_json(&mut conn)?;
        Ok(Self { conn })
    }

    /// 保存整批台账。S3 防线：id 已存在且台账条目非空时显式报错，
    /// 绝不静默覆盖抹账；空批次重存保持幂等。
    pub fn save_manifest(&self, m: &CleanManifest) -> io::Result<()> {
        let tx = self.conn.unchecked_transaction().map_err(sqlite_io)?;
        let existing: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM entries WHERE manifest_id = ?1",
                [m.id.as_str()],
                |r| r.get(0),
            )
            .map_err(sqlite_io)?;
        if existing > 0 && !m.entries.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("会话 id {} 已有 {} 条台账，拒绝覆盖（批次 id 碰撞？）", m.id, existing),
            ));
        }
        overwrite_manifest(&tx, m).map_err(sqlite_io)?;
        tx.commit().map_err(sqlite_io)?;
        Ok(())
    }

    /* ── stash journal（v5 契约 §1 executor 内部行为） ────────────── */

    /// move 之前先落账：manifest 行 + 全量 entries（status='pending'，size=0）。
    /// 崩溃窗口从此有账可查；id 已存在即报错（批次 id 必须唯一）。
    pub fn begin_session(
        &self,
        id: &str,
        mode: CleanMode,
        created_unix: u64,
        entries: &[(String, String)],
    ) -> io::Result<()> {
        let tx = self.conn.unchecked_transaction().map_err(sqlite_io)?;
        let dup: i64 = tx
            .query_row("SELECT COUNT(*) FROM manifests WHERE id = ?1", [id], |r| r.get(0))
            .map_err(sqlite_io)?;
        if dup > 0 {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("会话 id {id} 已存在台账，拒绝重复开账"),
            ));
        }
        tx.execute(
            "INSERT INTO manifests(id, created_unix, mode) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, created_unix as i64, mode_to_str(&mode)],
        )
        .map_err(sqlite_io)?;
        for (origin, vault_rel) in entries {
            tx.execute(
                "INSERT INTO entries(manifest_id, origin, vault_rel, size, status)
                 VALUES (?1, ?2, ?3, 0, 'pending')",
                rusqlite::params![id, origin, vault_rel],
            )
            .map_err(sqlite_io)?;
        }
        tx.commit().map_err(sqlite_io)?;
        Ok(())
    }

    /// 单条搬运成功：pending → committed 并落实测字节。
    pub fn mark_entry_committed(&self, id: &str, origin: &str, size: u64) -> io::Result<()> {
        let n = self
            .conn
            .execute(
                "UPDATE entries SET status = 'committed', size = ?3
                 WHERE manifest_id = ?1 AND origin = ?2 AND status = 'pending'",
                rusqlite::params![id, origin, size as i64],
            )
            .map_err(sqlite_io)?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("journal: 会话 {id} 无 pending 条目 {origin}"),
            ));
        }
        Ok(())
    }

    /// 单条搬运失败：数据完整留在原位、无副本存在——撤掉该 pending 行。
    pub fn abandon_entry(&self, id: &str, origin: &str) -> io::Result<()> {
        self.conn
            .execute(
                "DELETE FROM entries WHERE manifest_id = ?1 AND origin = ?2 AND status = 'pending'",
                rusqlite::params![id, origin],
            )
            .map_err(sqlite_io)?;
        Ok(())
    }

    /// 批次收尾：一条 committed 都没有（全败回滚）时抹掉空 manifest 行，
    /// 不留零条目台账污染孤儿 GC 名单。
    pub fn drop_session_if_no_entries(&self, id: &str) -> io::Result<()> {
        let n: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM entries WHERE manifest_id = ?1",
                [id],
                |r| r.get(0),
            )
            .map_err(sqlite_io)?;
        if n == 0 {
            self.conn
                .execute("DELETE FROM manifests WHERE id = ?1", [id])
                .map_err(sqlite_io)?;
        }
        Ok(())
    }

    /// 仍持有 pending 条目的会话 id：孤儿 GC 与过期清扫对它永不触碰，
    /// 待人工/下次 sweep 复核（journal 未完成态）。
    pub fn pending_session_ids(&self) -> io::Result<Vec<String>> {
        let mut st = self
            .conn
            .prepare("SELECT DISTINCT manifest_id FROM entries WHERE status <> 'committed'")
            .map_err(sqlite_io)?;
        let rows = st.query_map([], |r| r.get::<_, String>(0)).map_err(sqlite_io)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(sqlite_io)?);
        }
        Ok(out)
    }

    /* ── 查询 ────────────────────────────────────────────────────── */

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

    /// 彻底删除后调用：抹去某批次的 manifests+entries 行，
    /// 之后 [`crate::manifest::CleanManifest::load`] 会如实报「台账不存在」。
    /// history 表行保留——搬运量是已发生的历史事实，不随彻底删除消失。
    pub fn drop_manifest(&self, id: &str) -> io::Result<bool> {
        let n = self
            .conn
            .execute("DELETE FROM entries WHERE manifest_id = ?1", [id])
            .map_err(sqlite_io)?;
        let m = self
            .conn
            .execute("DELETE FROM manifests WHERE id = ?1", [id])
            .map_err(sqlite_io)?;
        Ok(n > 0 || m > 0)
    }

    /// 过期 vault 批次（仅 vault 模式且 created_unix 早于 cutoff）。
    /// 返回 (批次 id, 记录总字节, (origin, vault_rel, size) 列表)。
    /// v5：仍带 pending/异常状态条目的批次**不在过期清扫之列**（journal
    /// 未完成态，交给孤儿 GC 的保护逻辑长期持有）。
    pub fn expired_vault_batches(
        &self,
        cutoff_unix: i64,
    ) -> io::Result<Vec<ExpiredBatch>> {
        let mut out = Vec::new();
        let ids: Vec<String> = {
            let mut st = self
                .conn
                .prepare(
                    "SELECT m.id FROM manifests m
                     WHERE m.mode = 'vault' AND m.created_unix < ?1
                       AND NOT EXISTS (
                           SELECT 1 FROM entries e
                           WHERE e.manifest_id = m.id AND e.status <> 'committed')
                     ORDER BY m.created_unix",
                )
                .map_err(sqlite_io)?;
            let rows = st
                .query_map([cutoff_unix], |r| r.get::<_, String>(0))
                .map_err(sqlite_io)?;
            let mut ids = Vec::new();
            for r in rows {
                ids.push(r.map_err(sqlite_io)?);
            }
            ids
        };
        for id in ids {
            let mut st = self
                .conn
                .prepare(
                    "SELECT origin, vault_rel, size FROM entries WHERE manifest_id = ?1 ORDER BY rowid",
                )
                .map_err(sqlite_io)?;
            let rows = st
                .query_map([&id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))
                })
                .map_err(sqlite_io)?;
            let mut copies = Vec::new();
            let mut total: u64 = 0;
            for r in rows {
                let (o, v, s) = r.map_err(sqlite_io)?;
                total += s.max(0) as u64;
                copies.push((o, v, s.max(0) as u64));
            }
            out.push((id, total, copies));
        }
        Ok(out)
    }

    /// 仍存在于台账中的批次 id(孤儿 vault 会话目录 GC 用:不在名单内的目录可安全移除)。
    ///
    /// v5：不再吞错。prepare/行解码任何失败都 Err 上抛，调用方（GC）
    /// 据此熔断——吞错变空名单会把全部合法暂存副本当孤儿删光（审计 S1）。
    pub fn live_manifest_ids(&self) -> io::Result<Vec<String>> {
        let mut st = self
            .conn
            .prepare("SELECT id FROM manifests")
            .map_err(sqlite_io)?;
        let rows = st.query_map([], |r| r.get::<_, String>(0)).map_err(sqlite_io)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(sqlite_io)?);
        }
        Ok(out)
    }

    /// 批次条目三元组 `(origin, vault_rel, size)`（仅 committed），
    /// 按台账插入原序返回。彻底删除与过期清扫共用：size 取台账记录值，
    /// 删除后无需再量。错误不再吞（审计 S1 同款）。
    pub fn vault_copies(&self, id: &str) -> io::Result<Vec<(String, String, u64)>> {
        let mut st = self
            .conn
            .prepare(
                "SELECT origin, vault_rel, size FROM entries
                 WHERE manifest_id = ?1 AND status = 'committed' ORDER BY rowid",
            )
            .map_err(sqlite_io)?;
        let rows = st
            .query_map([id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?.max(0) as u64,
                ))
            })
            .map_err(sqlite_io)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(sqlite_io)?);
        }
        Ok(out)
    }

    /// 还原用条目对 `(origin, vault_rel)`（仅 committed），按台账插入原序返回。
    pub fn undo_entries(&self, id: &str) -> io::Result<Vec<(String, String)>> {
        let mut st = self
            .conn
            .prepare(
                "SELECT origin, vault_rel FROM entries
                 WHERE manifest_id = ?1 AND status = 'committed' ORDER BY rowid",
            )
            .map_err(sqlite_io)?;
        let rows = st.query_map([id], |r| Ok((r.get(0)?, r.get(1)?))).map_err(sqlite_io)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(sqlite_io)?);
        }
        Ok(out)
    }

    /// 会话全量条目（含 pending 状态）——历史下钻/审计视图用。
    pub fn session_entries(&self, id: &str) -> io::Result<Vec<SessionEntryDto>> {
        let mut st = self
            .conn
            .prepare(
                "SELECT origin, vault_rel, size, status FROM entries
                 WHERE manifest_id = ?1 ORDER BY rowid",
            )
            .map_err(sqlite_io)?;
        let rows = st
            .query_map([id], |r| {
                Ok(SessionEntryDto {
                    origin: r.get(0)?,
                    vault_rel: r.get(1)?,
                    size: r.get::<_, i64>(2)?.max(0) as u64,
                    status: r.get(3)?,
                })
            })
            .map_err(sqlite_io)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(sqlite_io)?);
        }
        Ok(out)
    }

    /// 追加历史（同 session 重放按主键覆盖）。
    pub fn append_history(&self, rec: &HistoryRecord) -> io::Result<()> {
        insert_history(&self.conn, rec).map_err(sqlite_io)?;
        Ok(())
    }

    /// 全量历史，按写入序返回。
    pub fn read_history(&self) -> Vec<HistoryRecord> {
        match self.conn.prepare(
            "SELECT session_id, created_unix, mode, files, bytes_moved, kind, src, dst
             FROM history ORDER BY rowid",
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
                        kind: r.get(5)?,
                        src: r.get(6)?,
                        dst: r.get(7)?,
                    })
                })
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(id: &str, n: usize) -> CleanManifest {
        CleanManifest {
            id: id.to_string(),
            created_unix: 1,
            mode: CleanMode::Vault,
            entries: (0..n)
                .map(|i| ManifestEntry {
                    origin: format!(r"C:\o\{i}.tmp"),
                    vault_rel: format!(r"C:\v\{i}.tmp"),
                    size: i as u64,
                })
                .collect(),
        }
    }

    /// save_manifest 的 S3 防线：已存在且条目非空的 id 必须 Err，
    /// 空条目重存保持幂等（结构测试不进 tests/ 集成套件，避免 env 竞争）。
    #[test]
    fn save_manifest_refuses_overwrite_nonempty_id() {
        let dir = tempfile::tempdir().unwrap();
        // 本测试独占 ZC_DATA_DIR；与集成套件的 DATA_LOCK 不重叠（lib 单测
        // 与集成测试是不同进程）
        static L: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _g = L.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("ZC_DATA_DIR", dir.path());

        let s = LedgerStore::open().unwrap();
        s.save_manifest(&manifest("dup", 2)).unwrap();
        let err = s.save_manifest(&manifest("dup", 1)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        // 原台账完好无损
        assert_eq!(s.load_manifest("dup").unwrap().entries.len(), 2);
        // 零条目重存幂等放行
        s.save_manifest(&manifest("dup", 0)).unwrap();
        assert_eq!(s.load_manifest("dup").unwrap().entries.len(), 0);

        std::env::remove_var("ZC_DATA_DIR");
    }

    #[test]
    fn journal_lifecycle_pending_commit_drop() {
        let dir = tempfile::tempdir().unwrap();
        static L: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _g = L.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var("ZC_DATA_DIR", dir.path());

        let s = LedgerStore::open().unwrap();
        let ents = vec![
            (r"C:\o\a".to_string(), r"C:\v\a".to_string()),
            (r"C:\o\b".to_string(), r"C:\v\b".to_string()),
        ];
        s.begin_session("j1", CleanMode::Vault, 5, &ents).unwrap();
        // id 重复开账必须拒绝
        assert!(s.begin_session("j1", CleanMode::Vault, 5, &ents).is_err());
        // pending 期：不进过期清扫名单、undo/vault_copies 看不见
        assert!(s.undo_entries("j1").unwrap().is_empty());
        assert_eq!(s.pending_session_ids().unwrap(), vec!["j1".to_string()]);
        assert_eq!(s.session_entries("j1").unwrap().len(), 2);

        // 全败路径：逐条撤 pending → 清零 → 空会话整行抹除
        s.abandon_entry("j1", &ents[0].0).unwrap();
        s.abandon_entry("j1", &ents[1].0).unwrap();
        s.drop_session_if_no_entries("j1").unwrap();
        assert!(s.load_manifest("j1").is_none());

        // 逐条提交路径：committed 行可见、undo 只见 committed、pending 名单清空；
        // committed 存在时 drop_session_if_no_entries 不得抹账
        s.begin_session("j2", CleanMode::Vault, 5, &ents).unwrap();
        s.mark_entry_committed("j2", &ents[0].0, 42).unwrap();
        s.abandon_entry("j2", &ents[1].0).unwrap();
        assert_eq!(s.undo_entries("j2").unwrap(), vec![ents[0].clone()]);
        assert!(s.pending_session_ids().unwrap().is_empty());
        let se = s.session_entries("j2").unwrap();
        assert_eq!(se.len(), 1);
        assert_eq!(se[0].status, "committed");
        assert_eq!(se[0].size, 42);
        s.drop_session_if_no_entries("j2").unwrap();
        assert!(s.load_manifest("j2").is_some());
        // committed 行不可被 abandon（防误撤已落账副本）
        s.abandon_entry("j2", &ents[0].0).unwrap();
        assert_eq!(s.session_entries("j2").unwrap().len(), 1);
        // drop_manifest（purge 语义）才整批抹除
        s.drop_manifest("j2").unwrap();
        assert!(s.load_manifest("j2").is_none());

        std::env::remove_var("ZC_DATA_DIR");
    }
}
