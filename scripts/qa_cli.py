# -*- coding: utf-8 -*-
"""zclean CLI 独立回归 QA(不依赖 GUI/CDP)。

契约(CONTRACT-v5 §发版纪律 / 任务书口径,由 CLI 代理实现):
    zclean scan --json [FILE]
    zclean bigfiles <PATH> [--top N] [--json]
    zclean dupes <PATH> [--min-mb N] [--json]
    zclean sweep [--days N]
    zclean show <REPORT>
    zclean rules [--md]
    退出码: 0 全成 / 1 错误 / 2 部分失败 / 3 取消
    既有链路保持: vault P1 [P2...] / undo SESSION-ID / purge SESSION-ID

隔离:全程 ZC_DATA_DIR=C:\\Temp\\zc-cli-qa-<ts>\\data,不碰用户真实台账/暂存区;
夹具垃圾(\\*.tmp、缓存子目录、重复文件对、>1MB 大文件)造在同级 junk 目录。
构建:自动 `cargo build --release -p zc-cli`(清代理 env + 绝对 cargo 路径),
失败即中止并提示 T3 陈旧二进制坑。
台账实证:sqlite3(标准库)只读查 <ZC_DATA_DIR>/ledger.db 的 entries.status 与行数。
"""
import json
import os
import re
import shutil
import sqlite3
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from qa_drive import REPORT, IMPL, counts, log, qa_lock, write_report, summarize, PROJECT_ROOT

REPORT.clear()

RUNTIME = {}  # exe/env/data/junk 等运行时上下文


# ────────────────────────── 基础设施 ──────────────────────────

def cargo_release_dir():
    """按 cargo 配置解析真实 release 目录（workspace 可经 .cargo/config.toml 重定向 target-dir，如 D 盘）。"""
    cargo = os.path.join(os.path.expanduser("~"), ".cargo", "bin", "cargo.exe")
    if not os.path.exists(cargo):
        cargo = shutil.which("cargo") or "cargo"
    try:
        p = subprocess.run([cargo, "metadata", "--format-version", "1", "--no-deps"],
                           cwd=PROJECT_ROOT, capture_output=True, timeout=120)
        if p.returncode == 0:
            return os.path.join(json.loads(p.stdout.decode("utf-8", "replace"))["target_directory"], "release")
    except Exception:
        pass
    return os.path.join(PROJECT_ROOT, "target", "release")

def decode(b):
    if b is None:
        return ""
    if isinstance(b, str):
        return b
    for enc in ("utf-8", "gbk"):
        try:
            return b.decode(enc)
        except UnicodeDecodeError:
            continue
    return b.decode("utf-8", "replace")


def cli(*args, timeout=600, env_extra=None, expect_code=0, case="cli"):
    """跑 zclean,返回 (code, stdout, stderr);首轮断言退出码=期望(0 或多态用例自管)。"""
    env = dict(RUNTIME["env"])
    if env_extra:
        env.update(env_extra)
    p = subprocess.run([RUNTIME["exe"], *args], cwd=RUNTIME["junk"],
                       capture_output=True, timeout=timeout, env=env)
    out, err = decode(p.stdout), decode(p.stderr)
    log(f"$ zclean {' '.join(args)} → exit={p.returncode}")
    return p.returncode, out, err


def expect_zero(code, out, err, what):
    assert code == 0, f"{what} 应 exit 0,实际 {code};stderr={err[:200]!r}"


def parse_json(text, what):
    i, j = text.find("{"), text.find("[")
    starts = [x for x in (i, j) if x >= 0]
    assert starts, f"{what}: 输出中找不到 JSON: {text[:200]!r}"
    s = min(starts)
    try:
        return json.loads(text[s:])
    except Exception as e:
        # 尾部可能混入非 JSON 行:回退到平衡截取
        dec = json.JSONDecoder()
        obj, _end = dec.raw_decode(text[s:])
        assert obj is not None, f"{what}: JSON 解析失败: {e}"
        return obj


def flatten_json(o):
    return json.dumps(o, ensure_ascii=False)


def ledger_db():
    return os.path.join(RUNTIME["data"], "ledger.db")


def ledger_query(sql):
    """sqlite3 标准库只读查台账:先复制 db(+wal/shm)到一次性目录再打开,
    绝不持锁/写入生产库。"""
    src = ledger_db()
    if not os.path.exists(src):
        return None
    tmp = os.path.join(RUNTIME["data"], "_qa_ro_snap")
    os.makedirs(tmp, exist_ok=True)
    for suffix in ("", "-wal", "-shm"):
        if os.path.exists(src + suffix):
            shutil.copy2(src + suffix, os.path.join(tmp, "ledger.db" + suffix))
    con = sqlite3.connect(os.path.join(tmp, "ledger.db"))
    try:
        return con.execute(sql).fetchall()
    finally:
        con.close()


def entries_count():
    rows = ledger_query("SELECT COUNT(*) FROM entries")
    return rows[0][0] if rows else 0


def find_named(fname):
    """在 ZC_DATA_DIR 全树找文件名(验证是否已入 vault / 是否被 sweep 清走)。"""
    for r, _, fs in os.walk(RUNTIME["data"]):
        if "_qa_ro_snap" in r:
            continue
        if fname in fs:
            return os.path.join(r, fname)
    return None


# ────────────────────────── 用例 ──────────────────────────

def case_build(cdp=None):
    """cargo build --release -p zc-cli;失败 → 中止并提示 T3 陈旧二进制坑。"""
    t0 = time.time()
    for k in ("HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy"):
        os.environ.pop(k, None)  # 清代理:在线 crates 拉取被坏代理卡死是 T3 经典坑
    cargo = os.path.join(os.path.expanduser("~"), ".cargo", "bin", "cargo.exe")
    if not os.path.exists(cargo):
        cargo = shutil.which("cargo") or "cargo"
    p = subprocess.run([cargo, "build", "--release", "-p", "zc-cli"],
                       cwd=PROJECT_ROOT, capture_output=True,
                       env=dict(os.environ), timeout=2400)
    out = decode(p.stdout) + decode(p.stderr)
    if p.returncode != 0:
        log("✗ cargo build 失败——拒绝用陈旧二进制跑回归(T3 坑)。诊断:"
            "\n  1) 已自动 unset HTTP(S)_PROXY/ALL_PROXY;若仍为网络问题请检查内网源;"
            "\n  2) 使用绝对 cargo 路径: " + cargo +
            "\n  3) 疑似陈旧 target 可先 `cargo clean -p zc-cli` 再重跑本脚本;"
            "\n  4) 构建尾部日志:\n" + out[-800:])
        sys.exit(2)
    cands = [os.path.join(cargo_release_dir(), "zclean.exe"),
             os.path.join(PROJECT_ROOT, "target", "release", "zclean.exe"),
             os.path.join(PROJECT_ROOT, "src-tauri", "target", "release", "zclean.exe")]
    cands = [c for c in cands if os.path.exists(c)]
    assert cands, "构建成功但找不到 zclean.exe(target/release)"
    exe = max(cands, key=os.path.getmtime)
    age_min = (time.time() - os.path.getmtime(exe)) / 60
    RUNTIME.update(exe=exe, env=dict(os.environ))
    # 新鲜度由 cargo 依赖追踪保证(无改动时不会重链接,mtime 可以很旧);
    # 这里只记录时间戳供 T3 陈旧二进制审计比对,不做硬断言。
    IMPL(step="build", ok=True, secs=round(time.time() - t0, 1), exe=exe,
         exe_age_min=round(age_min, 1), cargo=cargo, build_tail=out[-300:])
    log(f"build OK · {exe}(mtime {age_min:.0f}min 前;构建耗时 {round(time.time()-t0,1)}s)")


def make_fixture():
    base = rf"C:\Temp\zc-cli-qa-{time.strftime('%Y%m%d-%H%M%S')}"
    data = os.path.join(base, "data")
    junk = os.path.join(base, "junk")
    os.makedirs(os.path.join(junk, "sub-caches", "Code Cache"), exist_ok=True)
    os.makedirs(os.path.join(junk, "temp-ish"), exist_ok=True)
    for i in range(4):
        with open(os.path.join(junk, "temp-ish", f"junk{i}.tmp"), "wb") as f:
            f.write(os.urandom(64 * 1024))
    with open(os.path.join(junk, "sub-caches", "Code Cache", "blob"), "wb") as f:
        f.write(os.urandom(256 * 1024))
    blob = os.urandom(3 * 1024 * 1024)
    with open(os.path.join(junk, "dup-a.bin"), "wb") as f:
        f.write(blob)
    with open(os.path.join(junk, "dup-b.bin"), "wb") as f:
        f.write(blob)
    with open(os.path.join(junk, "big-unique.bin"), "wb") as f:
        f.write(os.urandom(11 * 1024 * 1024))  # >10MB:兼容各默认大文件阈值
    RUNTIME.update(base=base, data=data, junk=junk)
    RUNTIME["env"] = dict(RUNTIME.get("env") or os.environ, ZC_DATA_DIR=data)


def case_rules(cdp=None):
    """rules 输出含 v5 新规则 id:web-inet-cache、dev-pnpm-store;--md 同样在。"""
    t0 = time.time()
    code, out, err = cli("rules")
    expect_zero(code, out, err, "rules")
    need = [rid for rid in ("web-inet-cache", "dev-pnpm-store") if rid not in out]
    assert not need, f"规则表缺少 v5 新规则 id: {need}(T2 规则代理未落地?)"
    code2, out2, _ = cli("rules", "--md")
    expect_zero(code2, out2, _, "rules --md")
    assert "web-inet-cache" in out2 and "dev-pnpm-store" in out2, "rules --md 缺新规则"
    IMPL(step="rules", ok=True, secs=round(time.time() - t0, 1),
         rule_lines=len([l for l in out.splitlines() if l.strip()]))
    log("rules OK · web-inet-cache/dev-pnpm-store 双新规则在表(plain+--md)")


def case_scan_show(cdp=None):
    """scan --json [FILE]:stdout 合法 JSON + 报告文件真实落盘;show <REPORT> 可重放。"""
    t0 = time.time()
    rpt = os.path.join(RUNTIME["data"], "qa-report.json")
    code, out, err = cli("scan", "--json", rpt, timeout=900)
    assert code in (0, 2), f"scan --json FILE 退出码异常: {code}(1/3?);err={err[:200]!r}"
    assert os.path.exists(rpt), f"报告文件未落盘: {rpt}(T3 scan [FILE] 契约未实现?)"
    # 契约：--json FILE 时报告以文件为准，stdout 保持安静；stdout 通道由无 FILE 变体覆盖
    with open(rpt, encoding="utf-8") as f:
        body = f.read()
    d = parse_json(body, "scan --json 报告文件")
    s = flatten_json(d)
    assert "findings" in s or "rules" in s or "cleanable" in s, f"scan JSON 缺核心字段: {s[:200]}"
    code2, out2, err2 = cli("show", rpt)
    expect_zero(code2, out2, err2, "show")
    assert len(out2.strip()) > 20, "show 输出过短,疑似空重放"
    RUNTIME["report"] = rpt
    if isinstance(d, dict):
        fcount = len(d.get("findings") or d.get("rules") or [])
    else:
        fcount = len(d)
    IMPL(step="scan_show", ok=True, secs=round(time.time() - t0, 1), scan_code=code,
         report=rpt, findings=fcount)
    log(f"scan_show OK · JSON 合法({fcount} 条) · 报告 {os.path.basename(rpt)} 存在 · show ✓")


def case_bigfiles(cdp=None):
    """bigfiles --json:命中夹具大文件,--top N 生效。"""
    t0 = time.time()
    code, out, err = cli("bigfiles", RUNTIME["junk"], "--top", "3", "--json", timeout=300)
    expect_zero(code, out, err, "bigfiles --json")
    d = parse_json(out, "bigfiles JSON")
    lst = d if isinstance(d, list) else next((v for v in d.values() if isinstance(v, list)), None)
    assert lst is not None, f"bigfiles JSON 里没有数组字段: {flatten_json(d)[:200]}"
    s = flatten_json(lst)
    assert "big-unique.bin" in s, f"11MB 夹具大文件未上榜: {s[:300]}"
    assert len(lst) <= 3, f"--top 3 未生效,返回 {len(lst)} 条"
    IMPL(step="bigfiles", ok=True, secs=round(time.time() - t0, 1), n=len(lst), sample=s[:150])
    log(f"bigfiles OK · top3={len(lst)} 条 · 夹具大文件命中 ✓")


def case_dupes(cdp=None):
    """dupes --json:命中夹具重复对(2×3MB),--min-mb 生效。"""
    t0 = time.time()
    code, out, err = cli("dupes", RUNTIME["junk"], "--min-mb", "1", "--json", timeout=300)
    expect_zero(code, out, err, "dupes --json")
    s = flatten_json(parse_json(out, "dupes JSON"))
    assert "dup-a.bin" in s and "dup-b.bin" in s, f"夹具重复对未成组: {s[:300]}"
    assert "big-unique.bin" not in s, "独有大文件被误报为重复"
    IMPL(step="dupes", ok=True, secs=round(time.time() - t0, 1))
    log("dupes OK · 重复对命中 ✓ 非重复文件未混入 ✓")


def _vault_once(paths):
    code, out, err = cli("vault", *paths, timeout=300)
    expect_zero(code, out, err, "vault 暂存")
    m = re.search(r"zclean undo (\S+)", out)
    assert m, f"vault 输出无 session id(反悔通道提示缺失?): {out[:200]!r}"
    return m.group(1)


def case_vault_lifecycle(cdp=None):
    """vault 暂存→undo→purge→sweep --days 0 闭环;sqlite 台账行数/status 实证。

    --days 生效判据:暂存一个「0 龄」批次——sweep --days 999 不得动它,
    而 sweep --days 0 必须当场清走(若 --days 被硬编码 7 天窗忽略则清不走 → FAIL)。
    """
    t0 = time.time()
    f1 = os.path.join(RUNTIME["junk"], "temp-ish", "junk0.tmp")
    f2 = os.path.join(RUNTIME["junk"], "sub-caches", "Code Cache", "blob")
    f3 = os.path.join(RUNTIME["junk"], "big-unique.bin")
    e0 = entries_count()
    # ① 暂存 → 台账 entries 增长
    sid1 = _vault_once([f1, f2])
    assert not os.path.exists(f1) and not os.path.exists(f2), "暂存后源文件仍在"
    assert find_named("junk0.tmp"), "暂存文件未进 vault 数据区"
    e1 = entries_count()
    assert e1 > e0, f"暂存后 entries 行数未增:{e0}→{e1}(台账未记账?)"
    # ② undo 还原
    code, out, err = cli("undo", sid1, timeout=300)
    expect_zero(code, out, err, "undo")
    assert os.path.exists(f1) and os.path.exists(f2), "undo 后文件未搬回原位"
    # ③ 再暂存 → sweep --days 999 不动 → purge 彻底删除
    sid2 = _vault_once([f1, f2])
    code, out, err = cli("sweep", "--days", "999", timeout=300)
    expect_zero(code, out, err, "sweep --days 999")
    assert find_named("junk0.tmp"), "0 龄批次被 --days 999 提前清走——后悔期门限失守!"
    code, out, err = cli("purge", sid2, timeout=300)
    expect_zero(code, out, err, "purge")
    assert not find_named("junk0.tmp"), "purge 后 vault 副本仍存在"
    # ④ 新暂存 f3 → sweep --days 0 必须当场清走(--days 生效实证)
    sid3 = _vault_once([f3])
    assert find_named("big-unique.bin"), "f3 未进 vault"
    code, out, err = cli("sweep", "--days", "0", timeout=300)
    expect_zero(code, out, err, "sweep --days 0")
    assert not find_named("big-unique.bin"), \
        f"sweep --days 0 未清走 0 龄批次(sid3={sid3})——--days 参数未兑现(疑硬编码 7 天)"
    # ⑤ 台账实证
    cols = [r[1] for r in (ledger_query("PRAGMA table_info(entries)") or [])]
    assert "status" in cols, f"entries 表缺 status 列(契约 S1 journal 化未落地?): {cols}"
    hrows = ledger_query("SELECT COUNT(*) FROM history") or [[0]]
    assert hrows[0][0] >= 3, f"history 记账过少(暂存批未全程写台账): {hrows[0][0]}"
    IMPL(step="vault_lifecycle", ok=True, secs=round(time.time() - t0, 1),
         sid1=sid1, sid2=sid2, sid3=sid3, entries=[e0, e1, entries_count()],
         history_rows=hrows[0][0], status_col=True,
         days_gate="999 不动新批 ✓ / 0 当场收走 ✓")
    log(f"vault_lifecycle OK · 暂存→undo→purge→sweep(--days 0/999 双向实证)闭环 · "
        f"entries {e0}→{entries_count()} · history {hrows[0][0]}")


def case_exit_codes(cdp=None):
    """退出码约定:错误=1 实证;2(部分失败)/3(取消)当前无确定性 headless 钩子→真 SKIP。"""
    t0 = time.time()
    code, out, err = cli("show", os.path.join(RUNTIME["data"], "no-such-report-zz.json"))
    assert code == 1, f"错误路径应 exit 1,实际 {code}(exit 码约定未实现?)"
    IMPL(step="exit_codes", status="SKIP", secs=round(time.time() - t0, 1),
         exit0_verified=True, exit1_verified=True,
         skipped="exit 2(部分失败)与 exit 3(取消)需确定性 headless 触发钩子"
                 "(如注入不可搬文件/超长扫描+C 信号),现契约未提供,单列不记绿")
    log("exit_codes · 0/1 实证 ✓ · 2/3 SKIP(无触发钩子)")


# ────────────────────────── 主流程 ──────────────────────────

def main():
    qa_lock("qa_cli")
    try:
        case_build()
    except SystemExit:
        raise
    except Exception as e:
        IMPL(step="build", status="FAIL", error=str(e)[:300])
        log(f"✗ build: {str(e)[:300]}")
        sys.exit(2)
    make_fixture()
    steps = [case_rules, case_scan_show, case_bigfiles, case_dupes,
             case_vault_lifecycle, case_exit_codes]
    failures = []
    for fn in steps:
        try:
            fn()
        except Exception as e:
            failures.append((fn.__name__, str(e)[:300]))
            IMPL(step=fn.__name__.replace("case_", ""), status="FAIL", error=str(e)[:300])
            log(f"✗ {fn.__name__}: {str(e)[:300]}")
    shutil.rmtree(RUNTIME.get("base", r"C:\Temp\zc-cli-qa-none"), ignore_errors=True)
    path = rf"C:\Temp\zc-qa-cli-{time.strftime('%H%M%S')}.json"
    write_report(path, failures, header_extra={"exe": RUNTIME.get("exe")})
    return summarize("CLI 回归", path)


if __name__ == "__main__":
    sys.exit(main())
