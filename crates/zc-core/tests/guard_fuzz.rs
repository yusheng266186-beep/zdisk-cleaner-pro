//! GA #5 · 安全守卫 property 测试：畸形路径轰炸禁删区。
//!
//! 确定性伪随机（LCG，无外部依赖），覆盖 v1/v2 时代真实踩过的坑：
//! 大小写混淆、`\\?\` 前缀、尾部点/空格、..、混合分隔符、UNC 路径。
//! 唯一不变量：**禁删区内或无法解析的路径，永远 Err；除此之外不误伤。**

use std::path::{Path, PathBuf};
use zc_core::guard::Guard;

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
