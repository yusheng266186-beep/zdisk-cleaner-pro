//! 端到端闭环测试：临时树构造 → 扫描 → vault 清理 → 还原。
//! 通过 ZC_DATA_DIR 重定向数据目录，零污染真实 AppData。

use std::fs;
use std::path::PathBuf;

use zc_core::{
    executor,
    manifest::CleanManifest,
    models::*,
    scanner::{self, new_session_id, now_unix},
    Error, ScanHandle,
};

fn norm_of(p: &std::path::Path) -> String {
    p.to_string_lossy().replace('\\', "/").to_lowercase()
}

#[test]
fn scan_clean_undo_full_cycle_on_fixture() {
    // 隔离数据目录（本文件是唯一改环境变量的集成测试）
    let data_tmp = tempfile::tempdir().unwrap();
    std::env::set_var("ZC_DATA_DIR", data_tmp.path());

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

    // vault 模式清理
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
    let data_tmp = tempfile::tempdir().unwrap();
    std::env::set_var("ZC_DATA_DIR", data_tmp.path());

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

