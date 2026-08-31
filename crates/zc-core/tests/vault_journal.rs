//! v5 vault journal 集成测试（发布门槛）：
//! 1. stash_journal 全链路：move 前落 pending 台账 → 逐条 committed →
//!    副本在 vault、原件消失；purge 抹账后 live 名单不再含该 id；
//! 2. 全败批次不留账；目录搬运只走 rename——子文件被独占时整目录原样保留、
//!    vault 零残留；
//! 3. sweep 孤儿 GC 三保险：live 名单读取失败熔断（DB 损坏 → 孤儿不删）、
//!    mtime < 24h 新目录不删、pending 所属 id 不删；
//! 4. 过期批次（全 committed）到期物理删除 + drop_manifest。
//!
//! 与 e2e/ledger 同理：ZC_DATA_DIR 是进程级 env，本文件用例经 DATA_LOCK 串行。

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use zc_core::executor::CleanMode;
use zc_core::executor::vault;
use zc_core::ledger::LedgerStore;
use zc_core::manifest::CleanManifest;

static DATA_LOCK: Mutex<()> = Mutex::new(());

fn isolate() -> (MutexGuard<'static, ()>, tempfile::TempDir) {
    let g = DATA_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let d = tempfile::tempdir().expect("tempdir");
    std::env::set_var("ZC_DATA_DIR", d.path());
    (g, d)
}

fn touch(p: &Path, n: usize) {
    if let Some(par) = p.parent() {
        fs::create_dir_all(par).unwrap();
    }
    fs::write(p, vec![7u8; n]).unwrap();
}

/// 目录 mtime 回拨（孤儿 GC 的 24h 宽限期测试夹具）
fn backdate_dir(p: &Path, secs: u64) {
    use std::os::windows::ffi::OsStrExt as _;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, OPEN_EXISTING, SetFileTime,
    };
    const FILE_WRITE_ATTRIBUTES: u32 = 0x0000_0100;
    const SHARE_ALL: u32 = 1 | 2 | 4; // READ|WRITE|DELETE
    let wide: Vec<u16> = p.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let old = SystemTime::UNIX_EPOCH
        + Duration::from_secs(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .saturating_sub(secs),
        );
    let ticks = old
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64 / 100
        + 116_444_736_000_000_000;
    let ft = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    unsafe {
        let h = CreateFileW(
            wide.as_ptr(),
            FILE_WRITE_ATTRIBUTES,
            SHARE_ALL,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        );
        assert_ne!(h, INVALID_HANDLE_VALUE, "打开目录句柄失败: {p:?}");
        let ok = SetFileTime(h, std::ptr::null(), std::ptr::null(), &ft as *const FILETIME);
        CloseHandle(h);
        assert!(ok != 0, "回拨目录 mtime 失败: {p:?}");
    }
}

#[test]
fn stash_journal_pending_to_committed_and_purge() {
    let (_g, _d) = isolate();
    let src_root = tempfile::tempdir().unwrap();
    let f1 = src_root.path().join("one.tmp");
    let f2 = src_root.path().join("two.tmp");
    let dir = src_root.path().join("subdir");
    touch(&f1, 100);
    touch(&f2, 250);
    touch(&dir.join("inner.bin"), 64);

    let session = "sj-full";
    let session_dir = vault::vault_session_dir(session);
    let ledger = LedgerStore::open().unwrap();
    let (ok, failed) = vault::stash_journal(
        &session_dir,
        &[f1.as_path(), f2.as_path(), dir.as_path()],
        &ledger,
        session,
    )
    .expect("journal stash 应成功");
    assert!(failed.is_empty(), "{failed:?}");
    assert_eq!(ok.len(), 3);
    assert!(!f1.exists() && !f2.exists() && !dir.exists(), "原件必须全部离开原位");

    // pending → committed：journal 收尾后不存在 pending 行
    let ents = ledger.session_entries(session).unwrap();
    assert_eq!(ents.len(), 3);
    assert!(ents.iter().all(|e| e.status == "committed"), "{ents:?}");
    assert!(ents.iter().all(|e| e.size > 0));
    assert!(ledger.pending_session_ids().unwrap().is_empty());
    // 目录仅 rename：副本仍是目录且内容完好
    let dir_copy = &ents
        .iter()
        .find(|e| e.origin.ends_with("subdir"))
        .unwrap()
        .vault_rel;
    assert!(Path::new(dir_copy).join("inner.bin").exists(), "目录子树随 rename 完整迁移");

    // undo_entries / vault_copies 只见 committed；台账可正常 load
    let m = CleanManifest::load(session).unwrap();
    assert_eq!(m.entries.len(), 3);
    assert_eq!(m.mode, CleanMode::Vault);

    // purge：物理删除 + 抹账；之后 live 名单不含该 id
    let (deleted, freed, pfailed) = m.purge_forever().unwrap();
    assert!(pfailed.is_empty(), "{pfailed:?}");
    assert_eq!(deleted, 3);
    assert!(freed >= 414, "{freed}");
    assert!(CleanManifest::load(session).is_err(), "purge 后台账必须消失");
    let ledger = LedgerStore::open().unwrap();
    assert!(!ledger.live_manifest_ids().unwrap().iter().any(|id| id == session));
    assert!(!session_dir.exists(), "purge 后不留空壳会话目录");
}

#[test]
fn stash_journal_all_failed_leaves_no_ledger() {
    let (_g, _d) = isolate();
    let ghost = PathBuf::from(format!(
        r"C:\nonexistent-zc-{:x}\ghost.bin",
        std::process::id()
    ));
    let session_dir = vault::vault_session_dir("sj-ghost");
    let ledger = LedgerStore::open().unwrap();
    let (ok, failed) =
        vault::stash_journal(&session_dir, &[ghost.as_path()], &ledger, "sj-ghost").unwrap();
    assert!(ok.is_empty());
    assert_eq!(failed.len(), 1);
    // 全败回滚 + drop manifest：零条目台账不得污染孤儿 GC 名单
    assert!(ledger.load_manifest("sj-ghost").is_none());
    assert!(ledger.live_manifest_ids().unwrap().is_empty());
}

#[test]
fn locked_directory_is_never_copy_fallback_split() {
    let (_g, _d) = isolate();
    let src_root = tempfile::tempdir().unwrap();
    let dir = src_root.path().join("busydir");
    touch(&dir.join("inner.txt"), 32);
    // 独占句柄（无 DELETE 共享）锁住子文件 → 目录 rename 必败
    let hold = fs::OpenOptions::new().read(true).open(dir.join("inner.txt")).unwrap();

    let session = "sj-lockdir";
    let session_dir = vault::vault_session_dir(session);
    let ledger = LedgerStore::open().unwrap();
    let (ok, failed) =
        vault::stash_journal(&session_dir, &[dir.as_path()], &ledger, session).unwrap();
    drop(hold);
    assert!(ok.is_empty(), "被占用目录绝不部分搬移: {ok:?}");
    assert_eq!(failed.len(), 1);
    assert!(dir.join("inner.txt").exists(), "目录整体留在原位");
    // 两条硬约束：目录绝不复制 → vault 内零残留；台账抹干净
    let residue = fs::read_dir(&session_dir).map(|rd| rd.count()).unwrap_or(0);
    assert_eq!(residue, 0, "失败目录不得在 vault 留下半成品副本");
    assert!(ledger.load_manifest(session).is_none());
}

#[test]
fn sweep_gc_three_safeguards_and_expiry() {
    let (_g, _d) = isolate();
    let src = tempfile::tempdir().unwrap();

    // ① 过期批次：手工 journal 一个 created_unix=1（远古）的全 committed 批次
    //   （stash_journal 写 now，同秒内 cutoff 不满足 <，无法直接构造到期态）
    let _ = src;
    {
        let session_dir = vault::vault_session_dir("batch-old");
        let copy = session_dir.join("0000").join("stale.tmp");
        touch(&copy, 16);
        let ledger = LedgerStore::open().unwrap();
        let origin = r"C:\fixture\stale.tmp".to_string();
        ledger
            .begin_session(
                "batch-old",
                CleanMode::Vault,
                1,
                &[(origin.clone(), copy.display().to_string())],
            )
            .unwrap();
        ledger.mark_entry_committed("batch-old", &origin, 16).unwrap();
    }

    let vault_root = zc_core::manifest::data_dir().join("vault");
    fs::create_dir_all(&vault_root).unwrap();

    // ② 新鲜孤儿（台账无、mtime=now）→ 24h 宽限不删
    let orphan_new = vault_root.join("orphan-new");
    touch(&orphan_new.join("x"), 8);
    // ③ 陈旧孤儿（台账无、mtime 回拨 48h）→ 正常 GC
    let orphan_old = vault_root.join("orphan-old");
    touch(&orphan_old.join("x"), 8);
    backdate_dir(&orphan_old, 48 * 3600);
    // ④ pending 会话目录（台账有、状态 pending、回拨 48h）→ 永不触碰
    let pending_dir = vault_root.join("pend-1");
    fs::create_dir_all(&pending_dir).unwrap();
    {
        let ledger = LedgerStore::open().unwrap();
        ledger
            .begin_session(
                "pend-1",
                CleanMode::Vault,
                1,
                &[(r"C:\none\a".to_string(), format!(r"{}\a", pending_dir.display()))],
            )
            .unwrap();
    }
    backdate_dir(&pending_dir, 48 * 3600);

    let s = vault::sweep_expired(0).expect("sweep 应成功");
    assert!(!s.gc_skipped);
    assert!(s.sessions >= 2, "至少应清掉过期批次 + 陈旧孤儿: {s:?}");
    assert_eq!(s.bytes, 16, "过期批次记账来自 committed 条目实测字节");
    assert!(!orphan_old.exists(), "陈旧孤儿应被 GC");
    assert!(orphan_new.exists(), "24h 内的新孤儿必须豁免（防崩溃窗口误删）");
    assert!(pending_dir.exists(), "pending 会话目录 GC/过期清扫双豁免");
    assert!(!vault::vault_session_dir("batch-old").exists(), "过期批次副本被物理删除");
    {
        let ledger = LedgerStore::open().unwrap();
        assert!(ledger.load_manifest("batch-old").is_none());
        assert!(ledger.load_manifest("pend-1").is_some(), "pending 台账保留待复核");
    }

    // ⑤ live 名单读取失败 → GC 整体熔断：损坏 ledger.db 后 sweep 报错，
    //   任何会话目录分毫不动（吞错变空名单=全部暂存被删的灾难链已封死）
    let orphan_again = vault_root.join("orphan-corrupt-test");
    touch(&orphan_again.join("x"), 8);
    backdate_dir(&orphan_again, 48 * 3600);
    {
        // 关闭所有连接后，把库文件整体替换成垃圾字节
        let db = zc_core::manifest::data_dir().join("ledger.db");
        let mut f = fs::File::create(&db).unwrap();
        f.write_all(&[0u8; 512]).unwrap();
        f.sync_all().ok();
    }
    let err = vault::sweep_expired(0).unwrap_err();
    assert!(err.contains("not a database") || err.contains("database") || !err.is_empty());
    assert!(orphan_again.exists(), "熔断时孤儿不得被删");
    assert!(pending_dir.exists());
    assert!(orphan_new.exists());
}
