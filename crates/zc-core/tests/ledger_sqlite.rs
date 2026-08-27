//! 台账 SQLite 层验证：roundtrip 一致 / 旧 JSON 一次性导入并改名 /
//! undo_entries 序稳定。沿用仓库惯例：ZC_DATA_DIR 指向 tempdir，
//! 零污染真实 AppData（本文件内用例互斥锁串行化，防环境变量竞态）。

use std::fs;
use std::sync::{Mutex, MutexGuard};

use zc_core::{
    executor::CleanMode,
    history::{self, HistoryRecord},
    ledger::LedgerStore,
    manifest::{CleanManifest, ManifestEntry},
};

/// 同进程内多个 #[test] 并行跑，而 ZC_DATA_DIR 是全局环境变量——互斥。
static DATA_LOCK: Mutex<()> = Mutex::new(());

fn isolate() -> (MutexGuard<'static, ()>, tempfile::TempDir) {
    let guard = DATA_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let d = tempfile::tempdir().expect("tempdir");
    std::env::set_var("ZC_DATA_DIR", d.path());
    (guard, d)
}

fn sample_manifest(id: &str, mode: CleanMode) -> CleanManifest {
    CleanManifest {
        id: id.to_string(),
        created_unix: 1_790_000_000,
        mode,
        // 故意乱序：若实现偷懒按字典序读出，测试立即失败
        entries: vec![
            ManifestEntry {
                origin: r"C:\z\zz-last.txt".into(),
                vault_rel: r"C:\vault\zz.bin".into(),
                size: 3,
            },
            ManifestEntry {
                origin: r"C:\a\aa-first.tmp".into(),
                vault_rel: r"C:\vault\aa.bin".into(),
                size: 111,
            },
            ManifestEntry {
                origin: r"C:\m\mm-mid.log".into(),
                vault_rel: String::new(), // 回收站条目形态（vault_rel 为空）
                size: 9,
            },
        ],
    }
}

/// 用序列化快照做全等比较（避免为测试给结构体强加 PartialEq）。
fn snap<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap()
}

#[test]
fn save_then_load_roundtrips_exactly() {
    let (_g, _d) = isolate();

    let vault_batch = sample_manifest("rt-vault", CleanMode::Vault);
    let recycle_batch = sample_manifest("rt-bin", CleanMode::RecycleBin);
    LedgerStore::open()
        .unwrap()
        .save_manifest(&vault_batch)
        .unwrap();
    LedgerStore::open()
        .unwrap()
        .save_manifest(&recycle_batch)
        .unwrap();

    for original in [&vault_batch, &recycle_batch] {
        let got = LedgerStore::open()
            .unwrap()
            .load_manifest(original.id.as_str())
            .expect("save 后必须能 load 回来");
        assert_eq!(snap(&got), snap(original), "roundtrip 必须完全一致");
    }
}

#[test]
fn undo_entries_return_order_is_stable() {
    let (_g, _d) = isolate();

    let m = sample_manifest("order-1", CleanMode::Vault);
    LedgerStore::open().unwrap().save_manifest(&m).unwrap();

    let expect: Vec<(String, String)> = m
        .entries
        .iter()
        .map(|e| (e.origin.clone(), e.vault_rel.clone()))
        .collect();

    // 同一实例多次调用 + 独立重开库，返回序都必须等于台账插入序
    let s1 = LedgerStore::open().unwrap();
    assert_eq!(s1.undo_entries("order-1"), expect);
    assert_eq!(s1.undo_entries("order-1"), expect);
    assert_eq!(LedgerStore::open().unwrap().undo_entries("order-1"), expect);

    // 重存同 id（覆盖式）后顺序仍跟随最新写入序
    let mut reordered = sample_manifest("order-1", CleanMode::Vault);
    reordered.entries.reverse();
    s1.save_manifest(&reordered).unwrap();
    let expect2: Vec<(String, String)> = reordered
        .entries
        .iter()
        .map(|e| (e.origin.clone(), e.vault_rel.clone()))
        .collect();
    assert_eq!(
        LedgerStore::open().unwrap().undo_entries("order-1"),
        expect2,
        "覆盖保存后应得到新序"
    );
}

fn sample_history(session_id: &str, mode: CleanMode) -> HistoryRecord {
    HistoryRecord {
        session_id: session_id.to_string(),
        created_unix: 1_790_123_456,
        mode,
        files: 42,
        bytes_moved: 987_654_321,
    }
}

#[test]
fn imports_legacy_json_renames_files_and_is_idempotent() {
    let (_g, dir) = isolate();
    let root = dir.path();

    // 预置遗留数据：两个 manifest JSON + 两行 history.jsonl
    let m1 = sample_manifest("legacy-a", CleanMode::Vault);
    let mut m2 = sample_manifest("legacy-b", CleanMode::RecycleBin);
    m2.entries.pop(); // 与 m1 形态差异化

    let mfdir = root.join("manifests");
    fs::create_dir_all(&mfdir).unwrap();
    fs::write(
        mfdir.join("legacy-a.json"),
        serde_json::to_string_pretty(&m1).unwrap(),
    )
    .unwrap();
    fs::write(
        mfdir.join("legacy-b.json"),
        serde_json::to_string_pretty(&m2).unwrap(),
    )
    .unwrap();

    let h1 = sample_history("h-0001", CleanMode::RecycleBin);
    let h2 = sample_history("h-0002", CleanMode::Vault);
    fs::write(
        root.join("history.jsonl"),
        format!(
            "{}\n{}\n",
            serde_json::to_string(&h1).unwrap(),
            serde_json::to_string(&h2).unwrap()
        ),
    )
    .unwrap();

    // 打开即触发迁移
    let store = LedgerStore::open().expect("首开应成功");
    assert_eq!(snap(&store.load_manifest("legacy-a").unwrap()), snap(&m1));
    assert_eq!(snap(&store.load_manifest("legacy-b").unwrap()), snap(&m2));

    // 历史全量迁入且保持行序
    assert_eq!(
        snap(&history::read_all()),
        snap(&vec![h1.clone(), h2.clone()])
    );
    assert_eq!(store.read_history().len(), 2);

    // 旧文件已改名 .imported
    assert!(!mfdir.join("legacy-a.json").exists());
    assert!(mfdir.join("legacy-a.json.imported").is_file());
    assert!(!mfdir.join("legacy-b.json").exists());
    assert!(mfdir.join("legacy-b.json.imported").is_file());
    assert!(!root.join("history.jsonl").exists());
    assert!(root.join("history.jsonl.imported").is_file());

    // 幂等：再次打开不重复导入、数据不变
    let again = LedgerStore::open().unwrap();
    assert_eq!(snap(&again.load_manifest("legacy-a").unwrap()), snap(&m1));
    assert_eq!(snap(&history::read_all()), snap(&vec![h1, h2]));
}

#[test]
fn clean_manifest_public_api_stays_compatible_via_ledger() {
    let (_g, _d) = isolate();

    // 走 CleanManifest::save/load 公共路径（CLI/UI 的用法），同时校验
    // 找不到时的错误文案格式「台账 <id> 不存在…」
    let m = sample_manifest("api-1", CleanMode::Vault);
    m.save().unwrap();
    let got = CleanManifest::load("api-1").unwrap();
    assert_eq!(snap(&got), snap(&m));

    let err = CleanManifest::load("no-such-id").unwrap_err().to_string();
    assert!(err.starts_with("台账 no-such-id 不存在"), "实际文案: {err}");

    // 通过公共 API 追加/读取历史（干净数据目录内恰为该一条）
    let rec = sample_history("api-hist", CleanMode::Vault);
    history::append(&rec).unwrap();
    assert_eq!(snap(&history::read_all()), snap(&vec![rec]));
}
