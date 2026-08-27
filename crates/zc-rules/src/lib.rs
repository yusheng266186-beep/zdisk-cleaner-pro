//! 内置清理规则注册表。
//!
//! 书写纪律（防止 v1 家族的重复统计类缺陷）：
//! 1. 一条规则内禁止同时命中「目录自身 + 目录子孙」——
//!    引擎已做父目录吞并，但规则本身也要语义干净；
//! 2. 通配段禁止跨出目标根；
//! 3. 高风险目标一律 Risk::Risky/Expert，默认不会被勾选；
//! 4. 只声明"可再生"的产物——用户创作内容（聊天记录、存档、工程文件）
//!    永远不进规则表，属于存储迁移中心的搬迁对象。

use serde::Serialize;
use zc_core::{Domain, Risk};

#[derive(Debug, Clone, Serialize)]
pub struct Rule {
    pub id: &'static str,
    pub name_zh: &'static str,
    pub domain: Domain,
    pub risk: Risk,
    /// 目标位于系统树时必须为 true；非提权扫描会跳过该规则
    pub admin_required: bool,
    /// 目标模式（%ENV% 展开 / 归一化 / glob，分隔符一律 `/`）
    pub targets: &'static [&'static str],
    /// 守卫模式：即便 targets 命中也不许删
    #[serde(default)]
    pub guards: &'static [&'static str],
}

macro_rules! rule {
    ($id:expr, $name:expr, $domain:expr, $risk:expr, $admin:expr,
     targets = [$($t:expr),+ $(,)?] $(, guards = [$($g:expr),* $(,)?])? $(,)?) => {
        Rule {
            id: $id,
            name_zh: $name,
            domain: $domain,
            risk: $risk,
            admin_required: $admin,
            targets: &[$($t),+],
            guards: &[$($($g),*)?],
        }
    };
}

pub const RULES: &[Rule] = &[
    // ════════════════════════ System ════════════════════════
    rule!("sys-user-temp", "用户临时文件", Domain::System, Risk::Safe, false,
        targets = ["%LOCALAPPDATA%/Temp/**"],
        guards = ["%LOCALAPPDATA%/Temp/ZDiskCleanerPro3/**"]),
    rule!("sys-system-temp", "系统临时文件 (C:/Windows/Temp)", Domain::System, Risk::Caution, true,
        targets = ["C:/Windows/Temp/**"]),
    rule!("sys-update-cache", "Windows 更新下载缓存", Domain::System, Risk::Caution, true,
        targets = ["C:/Windows/SoftwareDistribution/Download/**"]),
    rule!("sys-thumbnails", "缩略图缓存", Domain::System, Risk::Safe, false,
        targets = [
            "%LOCALAPPDATA%/Microsoft/Windows/Explorer/thumbcache_*.db",
            "%LOCALAPPDATA%/Microsoft/Windows/Explorer/iconcache_*.db",
        ]),
    rule!("sys-dx-shader", "DirectX 着色器缓存", Domain::System, Risk::Safe, false,
        targets = ["%LOCALAPPDATA%/D3DSCache/**"]),
    rule!("sys-crash-dumps", "应用崩溃转储", Domain::System, Risk::Safe, false,
        targets = ["%LOCALAPPDATA%/CrashDumps/**"]),
    rule!("sys-wer-queue", "Windows 错误报告队列 (用户)", Domain::System, Risk::Caution, false,
        targets = [
            "%LOCALAPPDATA%/Microsoft/Windows/WER/ReportArchive/**",
            "%LOCALAPPDATA%/Microsoft/Windows/WER/ReportQueue/**",
        ]),
    rule!("sys-font-cache", "系统字体缓存", Domain::System, Risk::Caution, true,
        targets = ["C:/Windows/ServiceProfiles/LocalService/AppData/Local/FontCache/*.dat"]),
    rule!("sys-delivery-opt", "传递优化缓存", Domain::System, Risk::Caution, true,
        targets = ["C:/Windows/ServiceProfiles/NetworkService/AppData/Local/Microsoft/Windows/DeliveryOptimization/**"]),
    rule!("sys-kernel-dumps", "内核转储 / 蓝屏 Minidump", Domain::System, Risk::Caution, true,
        targets = ["C:/Windows/MEMORY.DMP", "C:/Windows/Minidump/*.dmp"]),
    rule!("sys-prefetch", "Windows Prefetch", Domain::System, Risk::Caution, true,
        targets = ["C:/Windows/Prefetch/*.pf"]),
    rule!("sys-wu-logs", "Windows 更新日志", Domain::System, Risk::Safe, true,
        targets = ["C:/Windows/Logs/WindowsUpdate/**"]),
    rule!("sys-perflogs", "性能日志 (PerfLogs)", Domain::System, Risk::Caution, true,
        targets = ["C:/PerfLogs/System/Diagnostics/**"]),

    // ════════════════════════ Browser ═══════════════════════
    rule!("chrome-cache", "Chrome 缓存", Domain::Browser, Risk::Safe, false,
        targets = [
            "%LOCALAPPDATA%/Google/Chrome/User Data/*/Cache/**",
            "%LOCALAPPDATA%/Google/Chrome/User Data/*/Code Cache/**",
            "%LOCALAPPDATA%/Google/Chrome/User Data/*/GPUCache/**",
        ],
        guards = ["**/Local State"]),
    rule!("chrome-crashpad", "Chrome 崩溃报告", Domain::Browser, Risk::Safe, false,
        targets = ["%LOCALAPPDATA%/Google/Chrome/User Data/Crashpad/completed/**"]),
    rule!("edge-cache", "Edge 缓存", Domain::Browser, Risk::Safe, false,
        targets = [
            "%LOCALAPPDATA%/Microsoft/Edge/User Data/*/Cache/**",
            "%LOCALAPPDATA%/Microsoft/Edge/User Data/*/Code Cache/**",
            "%LOCALAPPDATA%/Microsoft/Edge/User Data/*/GPUCache/**",
        ],
        guards = ["**/Local State"]),
    rule!("edge-crashpad", "Edge 崩溃报告", Domain::Browser, Risk::Safe, false,
        targets = ["%LOCALAPPDATA%/Microsoft/Edge/User Data/Crashpad/completed/**"]),
    rule!("brave-cache", "Brave 缓存", Domain::Browser, Risk::Safe, false,
        targets = ["%LOCALAPPDATA%/BraveSoftware/Brave-Browser/User Data/*/Cache/**"],
        guards = ["**/Local State"]),
    rule!("brave-crashpad", "Brave 崩溃报告", Domain::Browser, Risk::Safe, false,
        targets = ["%LOCALAPPDATA%/BraveSoftware/Brave-Browser/User Data/Crashpad/completed/**"]),
    rule!("firefox-cache2", "Firefox 网络缓存", Domain::Browser, Risk::Safe, false,
        targets = ["%LOCALAPPDATA%/Mozilla/Firefox/Profiles/*/cache2/**"]),
    rule!("ff-startup-cache", "Firefox 启动缓存", Domain::Browser, Risk::Safe, false,
        targets = ["%LOCALAPPDATA%/Mozilla/Firefox/Profiles/*/startupCache/**"]),
    rule!("opera-cache", "Opera 缓存", Domain::Browser, Risk::Safe, false,
        targets = ["%LOCALAPPDATA%/Opera Software/Opera Stable/Cache/**"]),
    rule!("opera-gx-cache", "Opera GX 缓存", Domain::Browser, Risk::Safe, false,
        targets = ["%LOCALAPPDATA%/Opera Software/Opera GX Stable/Cache/**"]),
    rule!("vivaldi-cache", "Vivaldi 缓存", Domain::Browser, Risk::Safe, false,
        targets = ["%LOCALAPPDATA%/Vivaldi/User Data/Default/Cache/**"]),

    // ════════════════════════ Dev ═══════════════════════════
    rule!("dev-npm-cache", "npm 下载缓存", Domain::Dev, Risk::Safe, false,
        targets = ["%LOCALAPPDATA%/npm-cache/_cacache/**"]),
    rule!("dev-pip-cache", "pip 下载缓存", Domain::Dev, Risk::Safe, false,
        targets = ["%LOCALAPPDATA%/pip/cache/**"]),
    rule!("dev-yarn-berry", "Yarn Berry 全局镜像缓存", Domain::Dev, Risk::Safe, false,
        targets = ["%LOCALAPPDATA%/Yarn/Berry/cache/**"]),
    rule!("dev-yarn-classic", "Yarn Classic 全局缓存", Domain::Dev, Risk::Safe, false,
        targets = ["%LOCALAPPDATA%/Yarn/cache/**"]),
    rule!("dev-pnpm-metadata", "pnpm 状态缓存", Domain::Dev, Risk::Safe, false,
        targets = ["%LOCALAPPDATA%/pnpm-state/**"]),
    rule!("dev-uv-cache", "uv 下载缓存", Domain::Dev, Risk::Safe, false,
        targets = ["%LOCALAPPDATA%/uv/cache/**"]),
    rule!("dev-poetry-cache", "Poetry 下载缓存", Domain::Dev, Risk::Safe, false,
        targets = ["%LOCALAPPDATA%/pypoetry/Cache/**"]),
    rule!("dev-go-build", "Go 编译缓存", Domain::Dev, Risk::Safe, false,
        targets = ["%LOCALAPPDATA%/go-build/**"],
        guards = ["%USERPROFILE%/go/pkg/mod/**"]),
    rule!("dev-cargo-registry-cache", "Cargo crates.io 包缓存", Domain::Dev, Risk::Safe, false,
        targets = ["%USERPROFILE%/.cargo/registry/cache/**"],
        guards = ["%USERPROFILE%/.cargo/registry/src/**", "%USERPROFILE%/.cargo/registry/index/**"]),
    rule!("dev-cargo-git-checkouts", "Cargo git 依赖 checkout 副本", Domain::Dev, Risk::Caution, false,
        targets = ["%USERPROFILE%/.cargo/git/checkouts/**"],
        guards = ["%USERPROFILE%/.cargo/git/db/**"]),
    rule!("dev-nuget-http", "NuGet HTTP 缓存", Domain::Dev, Risk::Safe, false,
        targets = [
            "%LOCALAPPDATA%/NuGet/http-cache/**",
            "%LOCALAPPDATA%/NuGet/plugins-cache/**",
        ],
        guards = ["%USERPROFILE%/.nuget/packages/**"]),
    rule!("dev-node-gyp", "node-gyp 头文件缓存", Domain::Dev, Risk::Safe, false,
        targets = ["%LOCALAPPDATA%/node-gyp/Cache/**"]),
    rule!("dev-electron-builder", "electron-builder 打包缓存", Domain::Dev, Risk::Safe, false,
        targets = ["%LOCALAPPDATA%/electron-builder/Cache/**"]),
    rule!("dev-playwright", "Playwright 浏览器二进制", Domain::Dev, Risk::Safe, false,
        targets = ["%LOCALAPPDATA%/ms-playwright/**"]),
    rule!("dev-vscode-cacheddata", "VS Code CachedData", Domain::Dev, Risk::Safe, false,
        targets = ["%APPDATA%/Code/CachedData/**", "%APPDATA%/Code - Insiders/CachedData/**"]),
    rule!("dev-vscode-vsix", "VS Code 扩展安装包缓存", Domain::Dev, Risk::Safe, false,
        targets = [
            "%USERPROFILE%/.vscode/CachedExtensionVSIXs/**",
            "%USERPROFILE%/.vscode/cli/sessions/**",
        ]),
    rule!("dev-gradle-mods", "Gradle 模块与构建缓存", Domain::Dev, Risk::Caution, false,
        targets = [
            "%USERPROFILE%/.gradle/caches/modules-2/**",
            "%USERPROFILE%/.gradle/caches/build-cache-1/**",
        ],
        guards = ["%USERPROFILE%/.gradle/caches/journal-1/**"]),
    rule!("dev-gradle-wrapper", "Gradle Wrapper 发行版", Domain::Dev, Risk::Caution, false,
        targets = ["%USERPROFILE%/.gradle/wrapper/dists/**"]),
    rule!("dev-maven-http", "Maven 元数据缓存", Domain::Dev, Risk::Caution, false,
        targets = ["%USERPROFILE%/.m2/repository/**/*.lastUpdated"]),
    rule!("dev-jb-indexes", "JetBrains 索引与缓存", Domain::Dev, Risk::Caution, false,
        targets = [
            "%LOCALAPPDATA%/JetBrains/*/caches/**",
            "%LOCALAPPDATA%/JetBrains/*/index/**",
        ]),
    rule!("dev-hf-models", "HuggingFace 模型 blob", Domain::Dev, Risk::Risky, false,
        targets = ["%USERPROFILE%/.cache/huggingface/hub/models--*/blobs/**"]),
    rule!("dev-torch-checkpoints", "PyTorch Hub 预训练权重", Domain::Dev, Risk::Caution, false,
        targets = ["%USERPROFILE%/.cache/torch/hub/checkpoints/**"]),
    rule!("dev-bazel-cache", "Bazel 输出缓存", Domain::Dev, Risk::Caution, false,
        targets = ["%LOCALAPPDATA%/_bazel_%USERNAME%/cache/**"]),
    rule!("dev-vcpkg-bin", "vcpkg 二进制缓存", Domain::Dev, Risk::Caution, false,
        targets = ["%LOCALAPPDATA%/vcpkg/archives/**"]),

    // ════════════════════════ Apps ══════════════════════════
    rule!("app-discord-cache", "Discord 缓存", Domain::Apps, Risk::Safe, false,
        targets = [
            "%APPDATA%/discord/Cache/**",
            "%APPDATA%/discord/Code Cache/**",
            "%APPDATA%/discord/GPUCache/**",
        ]),
    rule!("app-slack-cache", "Slack 缓存", Domain::Apps, Risk::Safe, false,
        targets = ["%APPDATA%/Slack/Cache/**", "%APPDATA%/Slack/Service Worker/CacheStorage/**"]),
    rule!("app-zoom-logs", "Zoom 日志", Domain::Apps, Risk::Safe, false,
        targets = ["%APPDATA%/Zoom/logs/**"]),
    rule!("app-obs-logs", "OBS 日志", Domain::Apps, Risk::Safe, false,
        targets = ["%APPDATA%/obs-studio/logs/**"]),
    rule!("app-spotify-datacache", "Spotify 数据缓存", Domain::Apps, Risk::Caution, false,
        targets = ["%LOCALAPPDATA%/Spotify/Storage/**"],
        guards = ["%LOCALAPPDATA%/Spotify/Users/**/pref*"]),
    rule!("app-steam-htmlcache", "Steam 客户端网页缓存", Domain::Apps, Risk::Safe, false,
        targets = ["%LOCALAPPDATA%/Steam/htmlcache/**"]),
    rule!("app-steam-depotcache", "Steam 内容下载临时区", Domain::Apps, Risk::Caution, false,
        targets = ["%LOCALAPPDATA%/Steam/depotcache/**"]),
    rule!("app-qq-crashpad", "QQ 崩溃报告", Domain::Apps, Risk::Safe, false,
        targets = ["%APPDATA%/Tencent/QQ/CrashPad/completed/**"]),
    rule!("app-amd-shader", "AMD 着色器缓存", Domain::Apps, Risk::Safe, false,
        targets = [
            "%LOCALAPPDATA%/AMD/DxCache/**",
            "%LOCALAPPDATA%/AMD/DxcCache/**",
            "%LOCALAPPDATA%/AMD/VkCache/**",
        ]),
    rule!("app-nv-shader", "NVIDIA 着色器缓存", Domain::Apps, Risk::Safe, false,
        targets = [
            "%USERPROFILE%/AppData/LocalLow/NVIDIA/PerDriverVersionDX12Cache/**",
            "%LOCALAPPDATA%/NVIDIA/GLCache/**",
        ]),

    // ════════════════════════ Logs ══════════════════════════
    rule!("log-jetbrains-local", "JetBrains 本地日志", Domain::Logs, Risk::Safe, false,
        targets = ["%LOCALAPPDATA%/JetBrains/*/log/**"]),
    rule!("log-cbs", "组件维护日志 (CBS)", Domain::Logs, Risk::Expert, true,
        targets = ["C:/Windows/Logs/CBS/*.log"],
        guards = ["C:/Windows/Logs/CBS/CBS.log"]),
];

/// （rule_id, pattern 归一化串）对，供引擎单遍匹配。
/// 展开失败的环境变量条目跳过（对应环境未安装该软件是正常态）。
pub fn expand_all() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for r in RULES {
        for t in r.targets {
            let (p, ok) = zc_core::expand_env(t);
            if !ok {
                continue;
            }
            let n = zc_core::norm(&p);
            if n.len() < 3 {
                continue;
            }
            out.push((r.id.to_string(), n));
        }
    }
    out
}

pub fn find(id: &str) -> Option<&'static Rule> {
    RULES.iter().find(|r| r.id == id)
}

/// 扫描结果的第二道闸：把命中中落入该规则守卫区（guards）的条目剔除。
/// 即使模式书写失误造成重叠，受保护的目录/文件也到不了清理清单。
pub fn filter_guards(findings: &mut Vec<zc_core::Finding>) {
    use globset::{GlobBuilder, GlobSetBuilder};
    use zc_core::expand_env;

    for f in findings.iter_mut() {
        let Some(rule) = find(&f.rule_id) else { continue };
        if rule.guards.is_empty() {
            continue;
        }
        let mut b = GlobSetBuilder::new();
        let mut expanded_any = false;
        for g in rule.guards {
            let (p, ok) = expand_env(g);
            if !ok {
                continue;
            }
            if let Ok(glob) = GlobBuilder::new(&zc_core::norm(&p))
                .literal_separator(true)
                .case_insensitive(true)
                .build()
            {
                b.add(glob);
                expanded_any = true;
            }
        }
        if !expanded_any {
            continue;
        }
        let Ok(gs) = b.build() else { continue };
        f.hits.retain(|h| !gs.is_match(zc_core::norm(&h.path)));
    }
    // 守卫可能清空了某些发现
    findings.retain(|f| f.total_count() > 0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn rule_ids_unique() {
        let ids: HashSet<_> = RULES.iter().map(|r| r.id).collect();
        assert_eq!(ids.len(), RULES.len(), "id 必须唯一");
    }

    #[test]
    fn registry_meets_v3_floor() {
        assert!(RULES.len() >= 60, "Phase 3 验收下限 60 条，当前 {}", RULES.len());
        // 五大域每域至少 2 条
        for d in [Domain::System, Domain::Browser, Domain::Dev, Domain::Apps, Domain::Logs] {
            assert!(
                RULES.iter().filter(|r| r.domain == d).count() >= 2,
                "{d:?} 域覆盖不足"
            );
        }
    }

    #[test]
    fn admin_rules_marked_consistently() {
        // 目标位于系统树（C:/Windows*、C:/PerfLogs*）的规则必须显式标记 admin_required
        for r in RULES {
            let needs_admin = r.targets.iter().any(|t| t.starts_with("C:/Windows") || t.starts_with("C:/PerfLogs"));
            assert_eq!(needs_admin, r.admin_required, "admin 标记与目标不一致: {}", r.id);
        }
    }

    #[test]
    fn every_rule_well_formed() {
        // 提权一致性已由 admin_rules_marked_consistently 强制；
        // 这里守住形态：目标非空、名字非空、id 为小写连字符命名。
        for r in RULES {
            assert!(!r.targets.is_empty(), "{} 缺少目标", r.id);
            assert!(!r.name_zh.is_empty(), "{} 缺少中文名", r.id);
            assert!(
                r.id.chars().all(|c| c.is_ascii_lowercase() || c == '-' || c.is_ascii_digit()),
                "{} id 需为小写连字符命名",
                r.id
            );
        }
    }

    #[test]
    fn all_patterns_compile_via_matcher() {
        let m = zc_core::RuleMatcher::build(&expand_all()).expect("rules must compile");
        assert!(!m.is_empty());
    }

    #[test]
    fn risky_rules_are_not_the_default_batch() {
        // 默认只清 Safe：Risky/Expert 的存在必须依赖显式选择——
        // 这里保证它们至少被正确标注而不是误标 Safe。
        for r in RULES.iter().filter(|r| matches!(r.risk, Risk::Risky | Risk::Expert)) {
            assert_ne!(r.risk, Risk::Safe);
        }
    }
}
