//! GA #5 · 安全守卫 property 测试：畸形路径轰炸禁删区。
//!
//! 确定性伪随机（LCG，无外部依赖），覆盖 v1/v2 时代真实踩过的坑：
//! 大小写混淆、`\\?\` 前缀、尾部点/空格、..、混合分隔符、UNC 路径。
//! 唯一不变量：**禁删区内或无法解析的路径，永远 Err；除此之外不误伤。**
//!
//! v5 增补（S4/A1 回归面）：8.3 短名、非 C 盘 SystemDrive 派生、
//! USERPROFILE 缺失 fail-closed、elevated allowlist 提权语义
//! （白名单目录放行、Windows 树其余仍拒）。env 快照经 Guard::with_env
//! 注入，全程不 set_var。

use std::path::{Path, PathBuf};
use zc_core::guard::{Guard, GuardEnv};

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 16
    }
}

fn mutate(base: &Path, rng: &mut Lcg) -> PathBuf {
    let s = base.to_string_lossy().to_string();
    let m = rng.next() % 7;
    match m {
        0 => PathBuf::from(s.to_uppercase()),
        1 => PathBuf::from(format!(r"\\?\{}", s)),
        2 => PathBuf::from(format!("{} ", s)),          // 尾空格
        3 => PathBuf::from(format!("{}.", s)),          // 尾点
        4 => PathBuf::from(format!(r"C:\Windows\..\Windows\System32\{}", rng.next() % 100)),
        5 => PathBuf::from(s.replace('\\', "/")),
        _ => PathBuf::from(format!(r"\\localhost\C$\Windows\{}", rng.next() % 100)), // UNC 管理共享
    }
}

#[test]
fn forbidden_zone_never_passes_vet_under_mutation() {
    let mut rng = Lcg(0x5EED_1A2B_3C4D_5E6F);
    let g = Guard::new();
    let bases = [
        r"C:\Windows\System32",
        r"C:\Program Files",
        r"C:\Program Files (x86)",
        r"C:\ProgramData\Microsoft",
    ];
    for base in bases {
        for i in 0..40u64 {
            let p = mutate(Path::new(base), &mut rng);
            let target = p.join(format!("f{}.sys", i));
            let res = g.vet([target.as_path()]);
            assert!(res.is_err(), "守卫放行了禁删区路径: {target:?}");
        }
    }
}

#[test]
fn temp_junk_always_passes_vet_under_benign_mutation() {
    let mut rng = Lcg(0xABCD_EF01_2345_6789);
    let g = Guard::new();
    let tmp = tempfile::tempdir().unwrap();
    let junk = tmp.path().join("junk.log");
    std::fs::write(&junk, b"x").unwrap();
    // 合法变换（大小写/分隔符）必须仍然放行——不因大小写误伤用户自己的缓存
    for i in 0..20u64 {
        let mut p = junk.clone().into_os_string().to_string_lossy().to_string();
        if i % 2 == 0 {
            p = p.to_uppercase();
        } else {
            p = p.replace('\\', "/");
        }
        let res = g.vet([Path::new(&p)]);
        assert!(res.is_ok(), "大小写/分隔符良性变体被误伤: {p}");
        let _ = rng.next();
    }
}

#[test]
fn norm_is_idempotent_and_lowercase() {
    for _ in 0..10 {
        let s = format!(r"\\?\C:\Users\{}.AppData\Local", "Test");
        let a = zc_core::norm(Path::new(&s));
        let b = zc_core::norm(Path::new(&a));
        assert_eq!(a, b, "norm 必须幂等");
        assert_eq!(a, a.to_lowercase());
    }
}

/* ═══════════════ v5 增补（S4 / A1 回归面）═══════════════ */

#[test]
fn short_83_names_and_uppercase_never_pass_forbidden_zone() {
    // 8.3 短名与全大写变体：要么 canonicalize 归真后命中禁删区，
    // 要么解析失败按 fail-closed 拒绝——两条路都必须 Err。
    let g = Guard::new();
    let mut rng = Lcg(0x83_83_83_83_83_83_83_83);
    let bases = [
        r"C:\PROGRA~1\Common Files",
        r"C:\WINDOWS\System32",
        r"C:\ProgramData\MICROS~1",
        r"C:\PROGRA~2\TEXTIL~1",
    ];
    for base in bases {
        for i in 0..10u64 {
            let p = Path::new(base).join(format!("x{i}.dll"));
            assert!(g.vet([p.as_path()]).is_err(), "短名/大写变体逃逸禁删区: {p:?}");
            let _ = rng.next();
        }
    }
}

#[test]
fn systemdrive_derived_roots_are_gated_not_c_literal_only() {
    // S4：系统装在 D: 的机器，真实 Windows 树必须进入禁删前缀。
    // D: 未必存在/可解析——用前缀表验证派生结果，vet 走 fail-closed 双保险。
    let env = GuardEnv {
        user_profile: Some(std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Users\guest"))),
        system_root: Some(PathBuf::from(r"D:\Windows")),
        program_files: Some(PathBuf::from(r"D:\Program Files")),
        program_files_x86: Some(PathBuf::from(r"D:\Program Files (x86)")),
        program_data: Some(PathBuf::from(r"D:\ProgramData")),
        system_drive: Some(PathBuf::from("D:")),
        data_dir: PathBuf::from(r"D:\zcdata"),
        elevated: false,
    };
    // allowlist 同样按派生盘构建（提权快照）
    let adm = Guard::with_env(GuardEnv { elevated: true, ..env.clone() });
    let g = Guard::with_env(env);
    assert!(g.forbidden_prefixes().iter().any(|p| p.starts_with("d:/windows")));
    assert!(g.forbidden_prefixes().iter().any(|p| p == "d:/program files"));
    assert!(g.forbidden_prefixes().iter().any(|p| p == "d:/programdata"));
    assert!(adm.allowlist().iter().any(|a| a == "d:/windows/temp"), "{:?}", adm.allowlist());
    assert!(adm.allowlist().iter().any(|a| a == "d:/perflogs"));
    assert!(adm.allowlist().iter().any(|a| a == "d:/$winreagent"));
}

#[test]
fn userprofile_missing_fails_closed_for_every_path() {
    let mut env = GuardEnv::from_process_env();
    env.user_profile = None;
    let g = Guard::with_env(env);
    let tmp = tempfile::tempdir().unwrap();
    let junk = tmp.path().join("benign.tmp");
    std::fs::write(&junk, b"x").unwrap();
    let err = g.vet([junk.as_path()]).unwrap_err().to_string();
    assert!(err.contains("USERPROFILE"), "缺失 USERPROFILE 必须显式拒绝而非静默放行: {err}");
}

#[test]
fn elevated_allowlist_admits_whitelisted_dirs_and_still_rejects_system32() {
    let base = GuardEnv::from_process_env();
    let windir = base.system_root.clone().expect("测试机必有 SystemRoot");
    let plain = Guard::with_env(GuardEnv { elevated: false, ..base.clone() });
    let adm = Guard::with_env(GuardEnv { elevated: true, ..base });

    // 白名单内（windows/temp 本体与其子孙语义由目录级前缀界定）
    let wtemp = windir.join("Temp");
    assert!(wtemp.is_dir());
    assert!(plain.vet([wtemp.as_path()]).is_err(), "未提权不得放行系统 Temp");
    assert!(adm.vet([wtemp.as_path()]).is_ok(), "提权批应放行 allowlist 内的系统 Temp");

    // 边界：temp 的兄弟目录、System32、根 windows 本体仍拒
    assert!(adm.vet([windir.join("System32").as_path()]).is_err());
    assert!(adm.vet([windir.as_path()]).is_err(), "Windows 根本体绝不在白名单");
    assert!(adm.vet([windir.join("Resources").as_path()]).is_err());
}
