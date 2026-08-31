//! 端到端闭环测试（v5）：临时树构造 → 扫描 → vault 清理（journal 化）→ 还原。
//! 通过 ZC_DATA_DIR 重定向数据目录，零污染真实 AppData。
//!
//! 测试卫生（审计 §A）：同文件内用例并行 `env::set_var("ZC_DATA_DIR")`
//! 会互相污染，全部经 DATA_LOCK 串行化。

use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use zc_core::{
    executor,
    manifest::CleanManifest,
    models::*,
    scanner::{self, new_session_id, now_unix},
    Error, ScanHandle,
};

/// ZC_DATA_DIR 是进程级环境变量——本文件内用例互斥。
static DATA_LOCK: Mutex<()> = Mutex::new(());

fn isolate_data_dir() -> (MutexGuard<'static, ()>, tempfile::TempDir) {
    let g = DATA_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let d = tempfile::tempdir().expect("tempdir");
    std::env::set_var("ZC_DATA_DIR", d.path());
    (g, d)
}

fn norm_of(p: &std::path::Path) -> String {
    p.to_string_lossy().replace('\\', "/").to_lowercase()
}

#[test]
fn scan_clean_undo_full_cycle_on_fixture() {
    let (_g, _data_tmp) = isolate_data_dir();

    // 构造: <root>/cache/{a,b}.tmp + <root>/keep/keepme.txt
    let tree = tempfile::tempdir().unwrap();
    let root = tree.path();
    let cache = root.join("cache");
    fs::create_dir_all(&cache).unwrap();
    fs::write(cache.join("a.tmp"), vec![0u8; 1024]).unwrap();
    fs::write(cache.join("b.tmp"), vec![0u8; 2048]).unwrap();
    fs::write(cache.join("unmatched.bin"), vec![0u8; 512]).unwrap(); // 不命中但随目录整体搬移
    let keep = root.join("keep");
    fs::create_dir_all(&keep).unwrap();
    fs::write(keep.join("keepme.txt"), b"important").unwrap();

    let pairs = vec![("fixture".to_string(), format!("{}/cache/**", norm_of(root)))];

    // literal_root 取第一个通配段之前的字面前缀：遍历根即 <root>/cache，
    // files_seen = cache(1) + a.tmp + b.tmp + unmatched.bin = 4
    let handle = ScanHandle::default();
    let rep = scanner::scan(&pairs, &handle, |_| {}).unwrap();
    assert_eq!(rep.files_seen, 4);
    assert_eq!(rep.cleanable_count(), 3);
    assert_eq!(rep.cleanable_bytes(), 3584); // 1024+2048+512：所有子孙如实计入

    // vault 模式清理（v5：stash 走 journal，move 前落 pending 台账）
    let outcome = executor::apply(&rep, &["fixture".to_string()], executor::CleanMode::Vault)
        .expect("守卫应放行临时树");
    assert_eq!(outcome.done_files, 3);
    assert_eq!(outcome.requested_bytes, outcome.done_bytes);
    assert!(!cache.join("a.tmp").exists(), "vault 后原件消失");
    assert!(keep.join("keepme.txt").exists(), "非命中文件不受影响");

    // 台账还原
    let m = CleanManifest::load(&rep.id).unwrap();
    assert_eq!(m.entries.len(), 3);
    let (done, failed) = m.undo().unwrap();
    assert!(failed.is_empty(), "{failed:?}");
    assert_eq!(done, 3);
    assert!(cache.join("a.tmp").exists(), "还原后回到原位");
}

#[test]
fn guard_blocks_protected_zone_even_in_report() {
    let (_g, _data_tmp) = isolate_data_dir();

    // 构造一条「正中 Program Files 禁删区」的假报告
    use zc_core::{FileHit, Finding};
    let mut f = Finding::new("evil");
    f.hits.push(FileHit {
        path: PathBuf::from(r"C:\Program Files\evil.sys"),
        size: 1,
        is_dir: false,
    });
    let rep = ScanReport {
        id: new_session_id(),
        started_unix: now_unix(),
        duration_ms: 0,
        files_seen: 0,
        bytes_seen: 0,
        cancelled: false,
        findings: vec![f],
        ..Default::default()
    };
    let err = executor::apply(&rep, &["evil".to_string()], executor::CleanMode::RecycleBin)
        .unwrap_err();
    assert!(
        matches!(err, Error::GuardRejected { .. }),
        "fail-closed 守卫必须拦截"
    );
}

/// 取消语义：预置取消令牌后，扫描必须立即放弃且报告 cancelled=true，
/// 绝不允许出现「取消按钮点了但引擎继续跑满全程」。
#[test]
fn scan_respects_pre_cancelled_handle() {
    let tree = tempfile::tempdir().unwrap();
    let root = tree.path();
    for i in 0..300u32 {
        let d = root.join(format!("d{i}"));
        fs::create_dir_all(&d).unwrap();
        for j in 0..20u32 {
            fs::write(d.join(format!("f{j}.dat")), vec![0u8; 16]).unwrap();
        }
    }
    // 6,000 文件夹具

    let handle = ScanHandle::default();
    handle.cancel(); // 在任何遍历开始前置位
    let pairs = vec![("bench".to_string(), format!("{}/d*/**", norm_of(root)))];
    let rep = scanner::scan(&pairs, &handle, |_| {}).unwrap();

    assert!(rep.cancelled, "cancelled 标志必须置位");
    assert!(
        (rep.files_seen as u32) < 6000,
        "取消后不应遍历完全量（实际 {}）",
        rep.files_seen
    );
    assert_eq!(rep.cleanable_count(), 0, "被取消的报告不得携带可执行发现");
}

/// 取消契约闭环（v5）：半途取消的报告即使带 hits，apply 也必须拒绝执行。
#[test]
fn cancelled_report_cannot_be_applied() {
    let (_g, _data_tmp) = isolate_data_dir();
    use zc_core::{FileHit, Finding};
    let tmp = tempfile::tempdir().unwrap();
    let junk = tmp.path().join("j.tmp");
    fs::write(&junk, b"xxxxxxxx").unwrap();

    let mut f = Finding::new("r");
    f.hits.push(FileHit { path: junk.clone(), size: 8, is_dir: false });
    let rep = ScanReport {
        id: new_session_id(),
        started_unix: now_unix(),
        duration_ms: 0,
        files_seen: 1,
        bytes_seen: 8,
        cancelled: true, // 半截结果
        findings: vec![f],
        ..Default::default()
    };
    let err = executor::apply(&rep, &["r".to_string()], executor::CleanMode::Vault).unwrap_err();
    assert!(matches!(err, Error::Cancelled { .. }), "{err:?}");
    assert!(junk.exists(), "被拒的取消批绝不能部分生效");
}
