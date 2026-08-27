//! Phase 3 夹具测试：合成真实软件目录结构，验证「该删的删、不该删的一个不碰」。
//!
//! 技巧：把 `%LOCALAPPDATA%`/`%USERPROFILE%` 等环境变量重定向到临时目录，
//! 让内置规则的通配模式直接解析进夹具树——全链路（展开→编译→匹配→守卫）。

use std::fs;
use std::path::PathBuf;
use zc_core::{models::*, scanner, ScanHandle};
use zc_rules::filter_guards;

fn norm_of(p: &std::path::Path) -> String {
    p.to_string_lossy().replace('\\', "/").to_lowercase()
}

fn touch(p: &std::path::Path, n: u64) {
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(p, vec![0u8; n as usize]).unwrap();
}

/// 收集报告中全部命中路径的小写归一化集合
fn hit_paths(rep: &ScanReport) -> Vec<String> {
    rep.findings
        .iter()
        .flat_map(|f| f.hits.iter())
        .map(|h| norm_of(&h.path))
        .collect()
}

/// 涉及环境变量重定向的测试共享此锁：std 环境变量是进程级的，
/// 并发覆盖会互相污染，因此三个场景串行执行。
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn chrome_fixture_hits_cache_but_never_local_state_or_bookmarks() {
    let _guard = ENV_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("LOCALAPPDATA", tmp.path());

    let base = PathBuf::from("Google/Chrome/User Data");
    let root = tmp.path().join(&base);
    // 应命中：Default profile 的缓存树
    touch(&root.join("Default").join("Cache").join("f_000001"), 100);
    touch(&root.join("Default").join("Code Cache").join("js"), 200);
    // 绝不命中：用户数据文件
    touch(&root.join("Local State"), 50);
    touch(&root.join("Default").join("Bookmarks"), 30);

    let pairs = zc_rules::expand_all()
        .into_iter()
        .filter(|(id, _)| id.starts_with("chrome-"))
        .collect::<Vec<_>>();
    assert!(pairs.len() >= 2, "应同时启用 cache 与 crashpad 规则");
    let rep = scanner::scan(&pairs, &ScanHandle::default(), |_| {}).unwrap();
    let hits = hit_paths(&rep);
    assert!(hits.iter().any(|h| h.ends_with("/cache/f_000001")), "{hits:?}");
    assert!(!hits.iter().any(|h| h.contains("local state")), "{hits:?}");
    assert!(!hits.iter().any(|h| h.contains("bookmarks")), "{hits:?}");
}

#[test]
fn cargo_fixture_guard_keeps_src_and_index() {
    let _guard = ENV_LOCK.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("USERPROFILE", tmp.path());

    let reg = tmp.path().join(".cargo").join("registry");
    touch(&reg.join("cache").join("serde-1.0.crate"), 500);
    touch(&reg.join("src").join("serde-1.0").join("lib.rs"), 900);
    touch(&reg.join("index").join("crates.io-index"), 700);

    let pairs = zc_rules::expand_all()
        .into_iter()
        .filter(|(id, _)| id == "dev-cargo-registry-cache")
        .collect::<Vec<_>>();
    let mut rep = scanner::scan(&pairs, &ScanHandle::default(), |_| {}).unwrap();
    filter_guards(&mut rep.findings);

    let hits = hit_paths(&rep);
    assert!(hits.iter().any(|h| h.ends_with("serde-1.0.crate")), "{hits:?}");
    assert!(!hits.is_empty(), "cache 命中不应为空");
    assert!(
        hits.iter().all(|h| h.contains("/cache/")),
        "src/index 即便被波及也必须被守卫剔除: {hits:?}"
    );
}

#[test]
fn guard_filter_removes_protected_hits_at_api_level() {
    let _guard = ENV_LOCK.lock().unwrap();
    use zc_core::{FileHit, Finding};

    let mut f = Finding::new("dev-go-build"); // guards: %USERPROFILE%/go/pkg/mod/**
    f.hits.push(FileHit { path: PathBuf::from(r"C:\u\go\pkg\mod\locked.zip"), size: 1, is_dir: false });
    f.hits.push(FileHit { path: PathBuf::from(r"C:\u\go-build\obj.o"), size: 2, is_dir: false });
    let mut findings = vec![f];

    std::env::set_var("USERPROFILE", r"C:\u");
    filter_guards(&mut findings);
    assert_eq!(findings[0].total_count(), 1, "mod 守卫区条目应被剔除");
    assert_eq!(norm_of(&findings[0].hits[0].path), "c:/u/go-build/obj.o");
}
