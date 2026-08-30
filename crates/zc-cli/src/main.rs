//! `zclean` — headless CLI，是内核的第一消费者（UI 只是第二个）。
//!
//! 用法：
//!   zclean scan [--json FILE]                 扫描内置规则，打印发现清单
//!   zclean apply REPORT.json --mode vault     按报告执行清理（trash|vault）
//!   zclean undo SESSION-ID                    还原一次 vault 批次
//!   zclean show REPORT.json                   重放展示某次扫描结果
//!   zclean rules                              列出全部规则与风险档位
//!   zclean selftest                           端到端自检（临时树，零风险）

use std::io::Write as _;
use std::path::{PathBuf, Path};
use std::process::ExitCode;
use zc_core::{executor, history::HistoryRecord, manifest::CleanManifest, models::*, *};

mod elevate;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("错误: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> zc_core::Result<ExitCode> {
    match args.first().map(|s| s.as_str()) {
        Some("scan") => cmd_scan(),
        Some("apply") => cmd_apply(args),
        Some("undo") => cmd_undo(args),
        Some("show") => cmd_show(args),
        Some("rules") => cmd_rules(args),
        Some("elevated-run") => cmd_elevated_run(args),
        Some("tree") => cmd_tree(args),
        Some("dupes") => cmd_dupes(args),
        Some("startup") => cmd_startup(args),
        Some("migrate") => cmd_migrate(args),
        _ => {
            print_help();
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn print_help() {
    println!(
        "zclean v3 — ZDiskCleaner Pro headless 内核客户端

命令:
  scan                        扫描并保存会话报告
  apply REPORT --mode MODE    执行清理 (MODE=trash|vault)
          [--rules id1,id2]   显式规则（缺省=仅安全档）
          [--admin]           需要管理员的规则走一次性 UAC 提权批
  undo SESSION-ID             还原 vault 批次
  show REPORT                 展示历史报告
  rules [--md]                规则列表 / Markdown 手册
  elevated-run --job SPEC     [内部] 提权 worker，由 run_elevated 拉起"
    );
}

/// 非 admin 时过滤 admin_required 规则：直接查进程令牌提权态。
fn is_admin_probe() -> bool {
    zc_core::is_elevated()
}

fn save_report(rep: &ScanReport) -> Result<PathBuf> {
    let p = manifest::data_dir().join("sessions").join(format!("{}.json", rep.id));
    std::fs::create_dir_all(p.parent().unwrap())?;
    std::fs::write(&p, serde_json::to_vec_pretty(rep)?)?;
    Ok(p)
}

fn load_report(path: &str) -> Result<ScanReport> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| Error::Other(format!("读取 {path} 失败: {e}")))?;
    Ok(serde_json::from_str(&raw)?)
}

fn cmd_scan() -> Result<ExitCode> {
    let admin = is_admin_probe();
    if !admin {
        println!("· 未检测到管理员权限：管理员规则已跳过");
    }

    let pairs: Vec<(String, String)> = zc_rules::expand_all()
        .into_iter()
        .filter(|(id, _)| admin || !zc_rules::find(id).is_some_and(|r| r.admin_required))
        .collect();
    let rule_count = pairs.iter().map(|(id, _)| id.as_str()).collect::<std::collections::BTreeSet<_>>().len();
    println!("启用规则 {rule_count} 条");

    let handle = ScanHandle::default();
    let t0 = std::time::Instant::now();
    let mut last_flush = std::time::Instant::now();
    let mut rep = scanner::scan(&pairs, &handle, |ev| {
        if let ScanEvent::Entry { files, bytes_seen } = ev {
            if last_flush.elapsed().as_millis() > 250 {
                print!("\r扫描中… {:>9} 项 / {:<10}", files, human_size(bytes_seen));
                let _ = std::io::stdout().flush();
                last_flush = std::time::Instant::now();
            }
        }
    })?;
    zc_rules::filter_guards(&mut rep.findings);
    println!();

    let saved = save_report(&rep)?;
    println!(
        "\n扫描完成 · 遍历 {} 文件 · 历时 {:.1}s",
        format_number(rep.files_seen),
        t0.elapsed().as_secs_f64()
    );
    print_findings(&rep);
    println!(
        "\n可清理合计: {} ({})",
        human_size(rep.cleanable_bytes()),
        format_number(rep.cleanable_count())
    );
    println!("报告已保存: {}", saved.display());
    println!("执行清理:   zclean apply \"{}\" --mode vault", saved.display());

    // 基准脚本消费的机器可读行（与本地化文本解耦，规避管道编码差异）
    if std::env::var_os("ZC_BENCH").is_some() {
        println!(
            "[bench] files={} duration_ms={} cleanable_bytes={} cleanable_count={}",
            rep.files_seen, rep.duration_ms, rep.cleanable_bytes(), rep.cleanable_count()
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn print_findings(rep: &ScanReport) {
    println!("{:<28} {:>12} {:>10}", "规则", "体积", "条目数");
    println!("{}", "-".repeat(54));
    for f in &rep.findings {
        let name = zc_rules::find(&f.rule_id)
            .map(|r| r.name_zh)
            .unwrap_or(f.rule_id.as_str());
        println!(
            "{name:<24} {:>12} {:>8}",
            human_size(f.total_bytes()),
            format_number(f.total_count())
        );
    }
}

fn format_number(n: u64) -> String {
    let s = n.to_string();
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in b.iter().enumerate() {
        if i > 0 && (b.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*c as char);
    }
    out
}

fn parse_mode(v: Option<&String>) -> Result<CleanMode> {
    match v.map(|s| s.as_str()) {
        Some("trash") => Ok(CleanMode::RecycleBin),
        Some("vault") => Ok(CleanMode::Vault),
        other => Err(Error::Other(format!("--mode 仅支持 trash|vault，收到 {other:?}"))),
    }
}

fn cmd_apply(args: &[String]) -> Result<ExitCode> {
    // 解析: apply REPORT --mode MODE [--rules id1,id2] [--admin]
    let mut report_path: Option<String> = None;
    let mut mode = CleanMode::Vault;
    let mut rules: Option<Vec<String>> = None;
    let mut include_admin = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--mode" => {
                mode = parse_mode(args.get(i + 1))?;
                i += 2;
            }
            "--rules" => {
                rules = Some(
                    args.get(i + 1)
                        .ok_or_else(|| Error::Other("--rules 需要参数".into()))?
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect(),
                );
                i += 2;
            }
            "--admin" => {
                include_admin = true;
                i += 1;
            }
            p if !p.starts_with('-') => {
                report_path = Some(p.to_string());
                i += 1;
            }
            other => return Err(Error::Other(format!("未知参数 {other}"))),
        }
    }
    let path = report_path.ok_or_else(|| Error::Other("缺少报告路径".into()))?;
    let rep = load_report(&path)?;

    // 未显式指定规则时默认只清 Safe 档（UI 接管后由用户勾选）
    let mut only: Vec<String> = rules.unwrap_or_else(|| {
        rep.findings
            .iter()
            .filter_map(|f| {
                zc_rules::find(&f.rule_id)
                    .filter(|r| r.risk == Risk::Safe)
                    .map(|_| f.rule_id.clone())
            })
            .collect()
    });

    let mode_cn = match mode {
        CleanMode::RecycleBin => "回收站",
        CleanMode::Vault => "vault 暂存区",
    };

    // 拆分提权批：--admin 时把 admin_required 规则转给一次性 UAC worker
    if include_admin && !is_admin_probe() {
        let (user_rules, admin_rules): (Vec<String>, Vec<String>) =
            only.clone().into_iter().partition(|id| {
                !zc_rules::find(id).is_some_and(|r| r.admin_required)
            });
        if !admin_rules.is_empty() {
            println!(
                "◆ {} 条管理员规则 → 一次性 UAC 提权批（拒绝则跳过，不影响其余）",
                admin_rules.len()
            );
            let spec = elevate::JobSpec {
                id: format!("elev-{}", rep.id),
                created_unix: now_unix(),
                action: elevate::JobAction::CleanRules {
                    report_path: path.clone(),
                    rule_ids: admin_rules,
                    mode,
                },
            };
            match elevate::run_elevated(
                &std::env::current_exe()?,
                &spec,
                std::time::Duration::from_secs(15 * 60),
            ) {
                Ok(res) if res.success => {
                    if let Some(o) = res.outcome {
                        println!("  ↳ 提权批完成: {} 项 / {}", o.done_files, human_size(o.done_bytes));
                        history::append(&HistoryRecord {
                            session_id: spec.id.clone(),
                            created_unix: now_unix(),
                            mode,
                            files: o.done_files,
                            bytes_moved: o.done_bytes,
                        })?;
                    }
                }
                Ok(res) => eprintln!("  ↳ 提权批失败: {}", res.message),
                Err(e) => eprintln!("  ↳ 跳过提权批: {e}"),
            }
        }
        only = user_rules;
    } else if !include_admin {
        // 用户未要求提权：照旧剔除 admin 规则并提示
        let had_admin = only
            .iter()
            .any(|id| zc_rules::find(id).is_some_and(|r| r.admin_required));
        if had_admin {
            println!("· 含管理员规则但未启用 --admin：已按用户态处理（添加 --admin 可走 UAC 批）");
            only.retain(|id| !zc_rules::find(id).is_some_and(|r| r.admin_required));
        }
    }
    println!(
        "模式: {mode_cn} · 计划批次 {} 条规则\n守卫校验中…",
        only.len()
    );

    let outcome = executor::apply(&rep, &only, mode)?;
    history::append(&HistoryRecord {
        session_id: rep.id.clone(),
        created_unix: now_unix(),
        mode,
        files: outcome.done_files,
        bytes_moved: outcome.done_bytes,
    })?;

    println!(
        "\n完成 {}/{} 项 · 计 {} (移入)",
        outcome.done_files,
        outcome.requested_files,
        human_size(outcome.done_bytes)
    );
    if !outcome.failed.is_empty() {
        println!("失败 {} 项：", outcome.failed.len());
        for (p, e) in outcome.failed.iter().take(20) {
            println!("  ✗ {p} — {e}");
        }
    }
    println!("\n▶ {}", outcome.semantics_note);
    if mode == CleanMode::Vault {
        println!("▶ 反悔通道: zclean undo {}", rep.id);
    }
    Ok(ExitCode::SUCCESS)
}

fn result_path_for(spec_path: &Path) -> PathBuf {
    let s = spec_path.to_string_lossy();
    PathBuf::from(s.replace(".spec.json", ".result.json"))
}

/// [内部] 提权 worker：加载 spec → 执行 → 回写结果文件。
fn cmd_elevated_run(args: &[String]) -> Result<ExitCode> {
    let job = args
        .iter()
        .position(|a| a == "--job")
        .and_then(|i| args.get(i + 1))
        .ok_or_else(|| Error::Other("elevated-run 需要 --job <spec.json>".into()))?;
    if !zc_core::is_elevated() {
        return Err(Error::Other("elevated-run 必须在提升进程中运行".into()));
    }
    let spec_path = PathBuf::from(job);
    let raw = std::fs::read_to_string(&spec_path)
        .map_err(|e| Error::Other(format!("读取任务失败: {e}")))?;
    let spec: elevate::JobSpec = serde_json::from_str(&raw)?;
    let res = elevate::execute_as_worker(&spec);

    // 无论成败都回写结果（launcher 靠文件出现判断 UAC 结果）
    let out_path = result_path_for(&spec_path);
    std::fs::write(&out_path, serde_json::to_vec_pretty(&res)?)?;
    println!("{}", res.message);
    Ok(if res.success { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

fn cmd_undo(args: &[String]) -> Result<ExitCode> {
    let id = args.get(1).ok_or_else(|| Error::Other("用法: zclean undo SESSION-ID".into()))?;
    let m = CleanManifest::load(id)?;
    let (done, failed) = m.undo()?;
    println!("已还原 {done}/{} 项", m.entries.len());
    for (p, e) in failed.iter().take(20) {
        println!("  ✗ {} — {e}", p.display());
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_show(args: &[String]) -> Result<ExitCode> {
    let p = args.get(1).ok_or_else(|| Error::Other("用法: zclean show REPORT.json".into()))?;
    let rep = load_report(p)?;
    println!("会话 {} · 扫描用时 {}ms", rep.id, rep.duration_ms);
    print_findings(&rep);
    Ok(ExitCode::SUCCESS)
}

fn cmd_rules(args: &[String]) -> Result<ExitCode> {
    if args.iter().any(|a| a == "--md") {
        println!("# 清理规则手册");
        println!();
        println!(
            "> 本文档由 `zclean rules --md` 自动生成，请勿手改。\n\
             > 源文件：`crates/zc-rules/src/lib.rs`\n"
        );
        println!("共 **{}** 条内置规则。", zc_rules::RULES.len());
        println!("默认只勾选「安全」档；风险档位见下表；标注 ⚙ 的规则需要管理员权限。");
        println!();
        println!("| ID | 名称 | 域 | 风险 |");
        println!("| --- | --- | --- | :--: |");
        for r in zc_rules::RULES {
            let risk = match r.risk {
                Risk::Safe => "安全",
                Risk::Caution => "注意",
                Risk::Risky => "**风险**",
                Risk::Expert => "**专家**",
            };
            let dom = match r.domain {
                Domain::System => "system",
                Domain::Browser => "browser",
                Domain::Dev => "dev",
                Domain::Apps => "apps",
                Domain::Logs => "logs",
            };
            let gear = if r.admin_required { " ⚙" } else { "" };
            println!("| `{}` | {}{gear} | {dom} | {risk} |", r.id, r.name_zh);
        }
        return Ok(ExitCode::SUCCESS);
    }

    println!("{:<30} {:<8} {:<6}", "ID", "域", "风险");
    for r in zc_rules::RULES {
        let risk = match r.risk {
            Risk::Safe => "安全",
            Risk::Caution => "注意",
            Risk::Risky => "风险",
            Risk::Expert => "专家",
        };
        let dom = match r.domain {
            Domain::System => "system",
            Domain::Browser => "browser",
            Domain::Dev => "dev",
            Domain::Apps => "apps",
            Domain::Logs => "logs",
        };
        let flag = if r.admin_required { " [admin]" } else { "" };
        println!("{:<30} {:<8} {:<6} {}{flag}", r.id, dom, risk, r.name_zh);
    }
    Ok(ExitCode::SUCCESS)
}

// ── selftest ────────────────────────────────────────────────────────────────

// ── 工具箱：tree / dupes / startup / migrate ──────────────────────────────

fn cmd_tree(args: &[String]) -> Result<ExitCode> {
    let mut path: Option<String> = None;
    let mut depth = 4u32;
    let mut as_json = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--depth" => { depth = args.get(i+1).and_then(|s| s.parse().ok()).unwrap_or(4); i += 2; }
            "--json" => { as_json = true; i += 1; }
            p if !p.starts_with('-') && path.is_none() => { path = Some(p.to_string()); i += 1; }
            other => return Err(Error::Other(format!("未知参数 {other}"))),
        }
    }
    let root = Path::new(path.as_deref().ok_or_else(|| Error::Other("用法: zclean tree <目录> [--depth N] [--json]".into()))?);
    use zc_core::analyze::{build_tree, TreeOptions};
    let t = build_tree(root, TreeOptions { max_depth: depth, max_children: 40 })
        .map_err(|e| Error::Other(e.to_string()))?;

    if as_json {
        println!("{}", serde_json::to_string_pretty(&t)?);
        return Ok(ExitCode::SUCCESS);
    }
    fn walk(n: &zc_core::analyze::TreeNode, indent: usize) {
        if n.size == 0 && n.children.is_empty() { return; }
        println!(
            "{:>10} {:indent$}{name}",
            human_size(n.size),
            "",
            indent = indent * 2,
            name = n.name
        );
        for c in &n.children {
            walk(c, indent + 1);
        }
    }
    walk(&t, 0);
    Ok(ExitCode::SUCCESS)
}

fn cmd_dupes(args: &[String]) -> Result<ExitCode> {
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut min_mb = 1u64;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--min-mb" => { min_mb = args.get(i+1).and_then(|s| s.parse().ok()).unwrap_or(1); i += 2; }
            p if !p.starts_with('-') => { paths.push(PathBuf::from(p)); i += 1; }
            other => return Err(Error::Other(format!("未知参数 {other}"))),
        }
    }
    if paths.is_empty() {
        return Err(Error::Other("用法: zclean dupes <目录...> [--min-mb N]".into()));
    }

    let t0 = std::time::Instant::now();
    let groups = zc_core::dedup::find_duplicates(
        &paths,
        &zc_core::dedup::DupOptions { min_size: min_mb * 1024 * 1024 },
    )?;

    let waste: u64 = groups.iter().map(|g| g.size * (g.files.len() as u64 - 1)).sum();
    println!(
        "发现 {} 组重复（≥{}MB）· 可回收 ≈ {} · 耗时 {:.1}s",
        groups.len(), min_mb, human_size(waste), t0.elapsed().as_secs_f64()
    );
    for (gi, g) in groups.iter().enumerate().take(50) {
        println!("\n#{:<3} {} ×{}", gi + 1, human_size(g.size), g.files.len());
        // 建议保留最新的一个（修改时间最大；无法读取时间的一律不标保留）
        let times: Vec<Option<std::time::SystemTime>> = g.files.iter()
            .map(|p| fs::metadata(p).and_then(|m| m.modified()).ok())
            .collect();
        let newest = times.iter().flatten().max().copied();
        for (f, mt) in g.files.iter().zip(&times) {
            let keep = newest.is_some() && mt.is_some() && *mt == newest;
            println!("  {} {}", if keep { "保留→" } else { "      " }, f.display());
        }
    }
    if groups.len() > 50 {
        println!("\n… 另有 {} 组未列出", groups.len() - 50);
    }
    Ok(ExitCode::SUCCESS)
}
use std::fs;

fn cmd_startup(args: &[String]) -> Result<ExitCode> {
    match args.get(1).map(|s| s.as_str()) {
        Some("list") | None => {
            let entries = zc_core::startup::list_user_startup().map_err(|e| Error::Other(e.to_string()))?;
            println!("{:<6} {:<24}", "#", "键名");
            for (i, e) in entries.iter().enumerate() {
                println!(
                    "{:<6} {:<24} {}",
                    i,
                    e.name.chars().take(22).collect::<String>(),
                    e.command.chars().take(70).collect::<String>()
                );
            }
            let d = zc_core::startup::disabled_count();
            if d > 0 {
                println!("\n已禁用 {d} 项（enable-all 可恢复）");
            }
        }
        Some("disable") => {
            let key = args.get(2).ok_or_else(|| Error::Other("用法: startup disable <index|名称>".into()))?;
            let entries = zc_core::startup::list_user_startup().map_err(|e| Error::Other(e.to_string()))?;
            let target = entries.iter()
                .find(|e| &e.name == key)
                .or_else(|| key.parse::<usize>().ok().and_then(|ix| entries.get(ix)));
            let t = target.ok_or_else(|| Error::Other("找不到该启动项".into()))?;
            let changed = zc_core::startup::disable(&t.key_id)?;
            println!("{}", if changed { format!("已禁用「{}」并备份", t.name) } else { "无需变更".into() });
        }
        Some("enable-all") => {
            let n = zc_core::startup::enable_all().map_err(|e| Error::Other(e.to_string()))?;
            println!("已恢复 {n} 个被禁用的启动项");
        }
        Some(other) => return Err(Error::Other(format!("未知子命令 startup {other}（list|disable|enable-all）"))),
    }
    Ok(ExitCode::SUCCESS)
}

/// 迁移相位 → 行号与中文文案（与 UI 阶段条一一对应）
fn migrate_phase_row(p: zc_core::migrate::MigratePhase) -> (usize, &'static str) {
    use zc_core::migrate::MigratePhase as P;
    match p {
        P::Copy => (1, "正在复制内容"),
        P::Verify => (2, "尺寸校验"),
        P::Link => (3, "建立 junction"),
        P::Smoke => (4, "冒烟验证"),
        P::Cleanup => (5, "清理备份"),
    }
}

fn cmd_migrate(args: &[String]) -> Result<ExitCode> {
    let action = args.get(1).map(|s| s.as_str()).unwrap_or("plan");
    let src = PathBuf::from(args.get(2).cloned().unwrap_or_default());
    if action != "undo" {
        let dst_root = PathBuf::from(args.get(3).cloned().unwrap_or_default());
        if src.as_os_str().is_empty() || dst_root.as_os_str().is_empty() {
            return Err(Error::Other("用法: migrate plan|apply <源目录> <目标盘根/父目录>\n       migrate undo <原路径>".into()));
        }
        let plan = zc_core::migrate::plan(&src, &dst_root)?;
        println!(
            "迁移计划：{} → {}\n体积 {} · 文件 {}",
            plan.src.display(), plan.dst.display(),
            human_size(plan.total_bytes), plan.total_files
        );
        if action == "apply" {
            if !args.contains(&"--yes".to_string()) {
                return Err(Error::Other("apply 需要显式 --yes 确认（含自动回滚保障）".into()));
            }
            let res = zc_core::migrate::apply_with_phases(&plan, &mut |phase, state| {
                let (n, label) = migrate_phase_row(phase);
                match state {
                    zc_core::migrate::PhaseState::Start => print!("\r[{n}/5] {label}…"),
                    zc_core::migrate::PhaseState::End => println!("\r[{n}/5] {label} ✓ 完成"),
                }
                let _ = std::io::stdout().flush();
            })
            .map_err(|e| Error::Other(e.to_string()));
            // 失败路径不补发 End：把悬着的进行中行收掉再上抛
            let id = match res {
                Ok(id) => id,
                Err(e) => {
                    println!();
                    return Err(e);
                }
            };
            println!("✓ 迁移完成，junction 已建立。清单 id={id}");
            println!("  undo 方式: zclean migrate undo \"{}\"", plan.src.display());
        }
        return Ok(ExitCode::SUCCESS);
    }
    // undo
    let src_ref: &String = if src.as_os_str().is_empty() {
        args.get(2).or_else(|| args.get(3)).ok_or_else(|| Error::Other("migrate undo 需要原路径".into()))?
    } else {
        args.get(2).ok_or_else(|| Error::Other("migrate undo 需要原路径".into()))?
    };
    let msg = zc_core::migrate::undo(Path::new(src_ref), None).map_err(|e| Error::Other(e.to_string()))?;
    println!("{msg}");
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    #[test]
    fn format_number_groups() {
        assert_eq!(super::format_number(1234567), "1,234,567");
        assert_eq!(super::format_number(999), "999");
    }

    #[test]
    fn parse_mode_rejects_unknown() {
        let v = String::from("nuke");
        assert!(super::parse_mode(Some(&v)).is_err());
        let ok = String::from("vault");
        assert!(super::parse_mode(Some(&ok)).is_ok());
    }
}
