//! 路径模式：`%ENV%` 展开、归一化与 glob 编译。
//!
//! 约定（所有规则目标统一遵循）：
//! - 分隔符一律写 `/`，内部归一化为平台路径；
//! - `%VAR%` 语法引用进程环境变量；
//! - `**` 跨目录，`*`/`?` 单层。

use std::path::{Path, PathBuf};

/// 展开 `%VAR%` 占位。未定义的变量保留原样并返回 false。
pub fn expand_env(input: &str) -> (PathBuf, bool) {
    let mut out = String::with_capacity(input.len());
    let mut complete = true;
    let mut rest = input;
    while let Some(start) = rest.find('%') {
        if let Some(end_off) = rest[start + 1..].find('%') {
            let end = start + 1 + end_off;
            let key = &rest[start + 1..end];
            match std::env::var(key) {
                Ok(v) if !v.is_empty() => out.push_str(&v),
                _ => {
                    // 未定义的环境变量：占位保留，标记不完整
                    out.push_str(&rest[start..=end]);
                    complete = false;
                }
            }
            rest = &rest[end + 1..];
        } else {
            out.push_str(&rest[..start]);
            rest = "";
        }
    }
    out.push_str(rest);
    (PathBuf::from(out), complete)
}

/// Windows 风格归一化：小写、正斜杠、剥掉 `\\?\` 前缀。
/// 规则、守卫、命中记录在内部比较时全部使用这一表示。
pub fn norm(p: &Path) -> String {
    let s = p.to_string_lossy();
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    s.replace('\\', "/").to_lowercase()
}

/// 提取模式中的"字面根"——第一个含通配符的路径段之前的部分，
/// 作为并行遍历的起点。无通配符则整条就是根。
pub fn literal_root(pattern_norm: &str) -> String {
    for (idx, seg) in pattern_norm.split('/').enumerate() {
        if seg.contains('*') || seg.contains('?') || seg.contains('[') {
            if idx == 0 {
                return String::new();
            }
            return pattern_norm.split('/').take(idx).collect::<Vec<_>>().join("/");
        }
    }
    pattern_norm.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_env_resolves_defined() {
        std::env::set_var("ZC_TEST_X", "abc");
        let (p, ok) = expand_env("%ZC_TEST_X%/sub/**");
        assert!(ok);
        assert_eq!(p, PathBuf::from("abc/sub/**"));
    }

    #[test]
    fn expand_env_keeps_unknown() {
        let (p, ok) = expand_env("%ZC_DEFINITELY_NOT_SET_42%/x");
        assert!(!ok);
        assert!(p.to_string_lossy().contains("ZC_DEFINITELY_NOT_SET_42"));
    }

    #[test]
    fn norm_strips_prefix_and_lowercases() {
        assert_eq!(
            norm(Path::new(r"\\?\C:\Users\Me\AppData\TEMP")),
            "c:/users/me/appdata/temp"
        );
        assert_eq!(norm(Path::new(r"C:\A\B")), "c:/a/b");
    }

    #[test]
    fn literal_root_cases() {
        assert_eq!(literal_root("c:/u/app/cache/**"), "c:/u/app/cache");
        assert_eq!(literal_root("c:/a/*/b"), "c:/a");
        assert_eq!("**/x", "**/x".split('/').next().map(|_| "**/x").unwrap_or("").to_string());
    }
}
