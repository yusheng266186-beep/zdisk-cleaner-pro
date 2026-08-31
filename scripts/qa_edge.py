# -*- coding: utf-8 -*-
"""ZDiskCleaner Pro 边界/异常路径 QA 驱动(第二轮:专打 happy path 之外)。

覆盖:规则展开、空勾选守卫(v5:必须点 results-exec 才触发)、取消扫描、
重复文件真实夹具、迁移非法路径、雷达选中跨页跳转(data-k 元素级选中)、
清理→撤销闭环(v5:两段式执行 + 遮罩同一元素 + 还原后 History 行自动消失)、
刷新后状态持久化。
"""
import json
import os
import shutil
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from qa_drive import (CDP, S, goto, click_text, wait_expr, wait_toast, toasts,
                      native_set_input, probe, log, IMPL, REPORT, PAGE_IDS,
                      results_exec, overlay_arm, overlay_check, EXEC_SEL)
from qa_drive import qa_lock, run_steps, write_report, summarize

REPORT.clear()


def vault_bytes():
    v = os.path.join(os.environ.get("LOCALAPPDATA", ""), "ZDiskCleanerPro3", "vault")
    t = 0
    for r, _, fs in os.walk(v):
        for x in fs:
            try:
                t += os.path.getsize(os.path.join(r, x))
            except OSError:
                pass
    return t


def do_scan(cdp):
    goto(cdp, "体检台")
    click_text(cdp, "开始智能体检")
    wait_expr(cdp, f"({S}).phase === 'results'", 600, interval=1.5, desc="scan results")


def edge_expand_rule(cdp):
    """结果页规则展开/收起:点行 → expandedRule 置位 → 再点收起。"""
    t0 = time.time()
    first_id = cdp.evaluate(f"({S}).report.findings.find(f => f.hits.length > 0).rule_id")
    # 行按钮文案是规则中文名,不是 id;等行挂载后再点(Results 页过渡竞态)
    wait_expr(cdp, "!!document.querySelector('main button.min-w-0.flex-1.text-left')", 15,
              desc="rule row mounted")
    cdp.evaluate("document.querySelector('main button.min-w-0.flex-1.text-left')?.click()", timeout=20)
    got = cdp.evaluate(f"({S}).expandedRule")
    assert got == first_id, f"展开失败: {got} != {first_id}"
    hit_count = cdp.evaluate(
        f"({S}).report.findings.find(f => f.rule_id === {json.dumps(first_id)}).hits.length")
    cdp.evaluate("document.querySelector('main button.min-w-0.flex-1.text-left')?.click()", timeout=20)
    assert cdp.evaluate(f"({S}).expandedRule") is None, "收起失败"
    IMPL(step="expand_rule", ok=True, secs=round(time.time() - t0, 1), rule=first_id,
         hits=hit_count)
    log(f"expand_rule OK · {first_id}({hit_count} 命中) 展开/收起 ✓")


def edge_empty_selection_guard(cdp):
    """清空勾选后点 results-exec → v5 起空守卫必须点执行钮才触发:
    应 warn 且不进入清理态、不崩溃。"""
    t0 = time.time()
    click_text(cdp, "清空勾选")
    n = cdp.evaluate(f"({S}).selection.size")
    assert n == 0, f"清空后仍有勾选: {n}"
    before = toasts(cdp)
    # 空勾选一段点击即 warn;不用 results_exec(8s armed 轮询会耗尽 4.2s toast 窗口)
    cdp.evaluate(f"document.querySelector({json.dumps(EXEC_SEL)})?.click()", timeout=20)
    msg = wait_toast(cdp, "请至少选择一条规则", timeout=15, desc="empty guard")
    phase = cdp.evaluate(f"({S}).phase")
    assert phase == "results", f"空勾选竟改变了状态: {phase}"
    assert not cdp.evaluate(f"({S}).cleanOutcome && ({S}).phase === 'cleaning'"), "空勾选竟进入清理"
    IMPL(step="empty_selection_guard", ok=True, secs=round(time.time() - t0, 1), toast=msg,
         trigger="results-exec(两段式)", toasts_before=len(before or []))
    log(f"empty_selection_guard OK · {msg}")


def edge_cancel_scan(cdp):
    """扫描中点取消 → 回 idle + toast,应用存活。"""
    t0 = time.time()
    goto(cdp, "体检台")
    click_text(cdp, "开始智能体检")
    wait_expr(cdp, f"({S}).phase === 'scanning'", 20, desc="scanning")
    # 快扫可能 2 秒内完成:立即尝试点取消,点不到且已出结果则记「过快无法取消」
    cancelled = cdp.evaluate(
        "(() => { const b = [...document.querySelectorAll('button')].find(x => x.textContent.includes('取消扫描') && x.offsetParent); if (b) { b.click(); return true; } return false; })()",
        timeout=20)
    if not cancelled:
        phase = cdp.evaluate(f"({S}).phase")
        assert phase in ("results", "idle"), f"取消按钮消失且状态异常: {phase}"
        IMPL(step="cancel_scan", status="SKIP", secs=round(time.time() - t0, 1),
             skipped=f"扫描过快完成({phase}),取消按钮已消失,取消路径未触发")
        log(f"cancel_scan SKIP · 扫描过快完成({phase}),取消按钮已消失")
        return
    msg = wait_toast(cdp, "已请求取消", timeout=15, desc="cancel toast")
    wait_expr(cdp, f"({S}).phase === 'idle'", 60, interval=0.5, desc="back to idle")
    time.sleep(2)
    assert probe(cdp) < 200, "取消后 UI 失去响应"
    IMPL(step="cancel_scan", ok=True, secs=round(time.time() - t0, 1), toast=msg)
    log(f"cancel_scan OK · {msg} · 回 idle ✓")


def edge_dupes_fixture(cdp):
    """真实重复文件夹具:2 份相同 12MB + 1 份独有 → ≥1 组,可回收≈12MB。"""
    t0 = time.time()
    fx = r"C:\Temp\zc-dupes"
    shutil.rmtree(fx, ignore_errors=True)
    os.makedirs(fx)
    blob = os.urandom(12 * 1024 * 1024)
    open(os.path.join(fx, "a.bin"), "wb").write(blob)
    open(os.path.join(fx, "b.bin"), "wb").write(blob)
    open(os.path.join(fx, "c.bin"), "wb").write(os.urandom(12 * 1024 * 1024))
    goto(cdp, "重复文件")
    native_set_input(cdp, "(i.placeholder || '').includes('Photos')", fx)
    native_set_input(cdp, "i.className.includes('num') && !i.placeholder", "10")
    click_text(cdp, "猎取重复")
    wait_expr(cdp,
              "!![...document.querySelectorAll('button')].find(b => b.textContent.includes('猎取重复') && !b.disabled)",
              300, interval=1.0, desc="dupes hunt done")
    body = cdp.evaluate("document.querySelector('main').innerText")
    import re
    m = re.search(r"(\d+)\s*组", body)
    assert m and int(m.group(1)) >= 1, f"未发现夹具重复组: {body[:200]!r}"
    rec = re.search(r"([\d.]+) ?([KMG]B)", body)
    IMPL(step="dupes_fixture", ok=True, secs=round(time.time() - t0, 1),
         groups=int(m.group(1)), first_size=rec.group(0) if rec else "?")
    log(f"dupes_fixture OK · {m.group(1)} 组 · 可回收 {rec.group(0) if rec else '?'}")
    shutil.rmtree(fx, ignore_errors=True)


def edge_migrate_invalid_src(cdp):
    """迁移源不存在 → 诚实报错,不崩溃、不生成计划。"""
    t0 = time.time()
    goto(cdp, "迁移中心")
    cdp.evaluate(f"({S}).resetMigrateWizard(); 0", timeout=20)  # 清上套件遗留 done 态(单实例 store 持久)
    time.sleep(0.3)
    native_set_input(cdp, "(i.placeholder || '').includes('npm-cache')", r"C:\Temp\zc-no-such-dir-42")
    native_set_input(cdp, "(i.placeholder || '').includes('E:')", r"C:\Temp")
    click_text(cdp, "生成迁移计划")
    msg = wait_toast(cdp, "无法生成", "不存在", "失败", timeout=30, desc="invalid src")
    assert "确认执行迁移" not in cdp.evaluate("document.querySelector('main').innerText"), \
        "非法源竟生成了计划"
    IMPL(step="migrate_invalid_src", ok=True, secs=round(time.time() - t0, 1), toast=msg)
    log(f"migrate_invalid_src OK · {msg}")


def edge_radar_cross_page(cdp):
    """雷达 shift+点击最大块 → 选中条出现 → 「作为迁移源」跨页预填表单。

    v5:treemap 色块带 data-k=<key>,改用元素级 dispatchEvent MouseEvent——
    规避 WebView2 高 DPI 下 CDP Input 坐标换算歧义与遮挡残影。"""
    t0 = time.time()
    goto(cdp, "空间雷达")
    wait_expr(cdp, "document.body.innerText.includes('个文件')", 300, interval=1.0, desc="radar tree")
    hit = json.loads(cdp.evaluate(
        """(() => {
            const host = document.querySelector('.min-h-40.flex-1');
            if (!host) return JSON.stringify({ok: false, why: 'NO_HOST'});
            let best = null, area = 0;
            for (const d of host.children) {
                const st = getComputedStyle(d);
                const w = parseFloat(st.width), h = parseFloat(st.height);
                if (w * h > area) { area = w * h; best = d; }
            }
            if (!best) return JSON.stringify({ok: false, why: 'NO_TILE'});
            best.dispatchEvent(new MouseEvent('click', {shiftKey: true, bubbles: true, cancelable: true}));
            return JSON.stringify({ok: true, key: String(best.dataset.k ?? best.textContent).slice(0, 60)});
        })()""", timeout=30))
    assert hit["ok"], f"色块元素级选中失败: {hit['why']}"
    time.sleep(0.8)
    selected = cdp.evaluate(
        "[...document.querySelectorAll('[role=status]')].some(e => e.innerText.includes('已选中'))")
    assert selected, "shift+点击未出现选中条"
    click_text(cdp, "作为迁移源")
    wait_expr(cdp, f"({S}).activePage === 'migrate'", 10, desc="jump migrate")
    # 页面过渡 ~0.4s 后才挂载并回填,轮询等预填落地
    prefill = wait_expr(
        cdp,
        "([...document.querySelectorAll('input')].find(i => (i.placeholder||'').includes('npm-cache')) || {}).value || ''",
        15, interval=0.5, desc="migrate prefill")
    assert prefill, f"迁移表单未预填: pendingMigrateSrc={cdp.evaluate(f'({S}).pendingMigrateSrc')!r}"
    IMPL(step="radar_cross_page", ok=True, secs=round(time.time() - t0, 1), prefill=prefill[:60],
         sel_mode="元素级 MouseEvent(data-k 色块)", sel_key=hit["key"])
    log(f"radar_cross_page OK · 选中→跳转迁移中心,预填 {prefill[:60]}")


def _seed_old_temp():
    """播种 mtime 回拨 10 天的 %TEMP% 文件,保证 sys-user-temp(Safe,min_age=7)有命中。"""
    dd = os.environ.get("TEMP") or os.path.join(os.environ.get("LOCALAPPDATA", ""), "Temp")
    old = time.time() - 10 * 86400
    made = []
    for k in range(5):
        fp = os.path.join(dd, f"zc-qa-old-{int(time.time())}-{k}.tmp")
        try:
            with open(fp, "wb") as f:
                f.write(os.urandom(320 * 1024))
            os.utime(fp, (old, old))
            made.append(fp)
        except OSError:
            pass
    return made


def edge_clean_undo_cycle(cdp):
    """清理(results-exec 两段式 + 遮罩同一元素)→战报横幅「反悔」→
    数据原样搬回、vault 回吐、且 History 行自动消失(v5 新行为)。"""
    t0 = time.time()
    seeded = _seed_old_temp()
    assert seeded, "无法播种 %TEMP% 过期夹具"
    v0 = vault_bytes()
    do_scan(cdp)
    # 勾选只留播种规则(sys-user-temp):批次=自造 temp 文件,undo 必全成→走结清路径
    # (混入环境命中时部分还原失败→台账按设计保留重试,结清断言就变成赌博)
    cdp.evaluate(f"window.__zcStore.setState({{ selection: new Set(['sys-user-temp']) }}); 0", timeout=20)
    time.sleep(0.3)
    _sel = json.loads(cdp.evaluate(f"JSON.stringify([...({S}).selection])"))
    assert _sel == ["sys-user-temp"], f"selection 收窄失败: {_sel}"
    guard = cdp.evaluate(
        f"JSON.stringify([...({S}).selection].filter(id => ({S}).rules.find(r => r.id === id)?.risk !== 'safe'))")
    assert json.loads(guard) == [], f"选中含非安全规则: {guard}"
    armed = results_exec(cdp)
    assert armed, "results-exec 未进入「确认清理 N 项」二次确认态"
    wait_expr(cdp, f"({S}).phase === 'cleaning'", 20, desc="cleaning phase")
    overlay_arm(cdp)
    time.sleep(2.0)
    overlay_check(cdp)
    wait_expr(cdp, f"({S}).phase === 'idle' && ({S}).cleanOutcome", 600, interval=1.5, desc="clean done")
    oc = json.loads(cdp.evaluate(f"JSON.stringify((({S}).cleanOutcome ?? {{}}))"))
    session = cdp.evaluate(f"({S}).lastSessionId")
    v1 = vault_bytes()
    grew = v1 - v0
    log(f"清理 {oc['done_files']} 项 / {oc['done_bytes']} 字节 · vault {v0/2**20:.0f}→{v1/2**20:.0f}MB")
    assert grew >= oc["done_bytes"] * 0.9 or oc["done_bytes"] == 0, \
        f"vault 增量 {grew} 与账面 {oc['done_bytes']} 严重不符"
    # 战报横幅反悔(toast 为结构化「已还原 N 项」)
    click_text(cdp, "反悔 · 一键还原本批")
    undo_toast = wait_toast(cdp, "已还原", timeout=300, desc="undo toast")
    time.sleep(1.5)
    v2 = vault_bytes()
    log(f"撤销后 vault {v2/2**20:.0f}MB(回收 {grew - (v2 - v0):.0f} 字节级)")
    assert v2 <= v1 - grew * 0.9, f"撤销后 vault 未回吐: {v1}->{v2}"
    # v5.0 结清断言:undo 成功后 History 自动刷新——该 session 行必须变为「已还原」
    # 结清徽标且还原/彻底删除按钮消失(流水保留审计,死按钮绝迹);行整体移除亦接受
    if session:
        # 结清徽标在 History 页 DOM——必须导航过去再查(undo toast 发生在 home 战报横幅上)
        goto(cdp, "历史")
        wait_expr(cdp, f"!!({S}).history.length", 15, desc="history rows")
        settled = cdp.evaluate(
            f"""!!document.querySelector('[data-testid="settled-{session}"]')""")
        row_actionable = cdp.evaluate(
            f"""[...document.querySelectorAll('li[data-session="{session}"]')]
                .some(l => [...l.querySelectorAll('button')].some(b => b.innerText.includes('还原') || b.innerText.includes('详情')))""")
        hist_settled = cdp.evaluate(
            f"({S}).history.some(h => h.session_id === {json.dumps(session)} && (h.kind === 'undo' || h.live === false))")
        assert settled and not row_actionable, \
            f"还原后 History 未结清:session {session}(badge={settled} 仍可动作={row_actionable})"
        assert hist_settled, f"store.history 未反映结清(live/kind):{session}"
    for sp in seeded:
        try:
            os.remove(sp)
        except OSError:
            pass
    assert oc["done_files"] >= len(seeded), f"清理项数 {oc['done_files']} < 播种夹具数 {len(seeded)},链路未真正搬运"
    IMPL(step="clean_undo_cycle", ok=True, secs=round(time.time() - t0, 1),
         done_files=oc["done_files"], done_bytes=oc["done_bytes"], session=session,
         undo_toast=str(undo_toast)[:60], exec_mode="results-exec 两段式",
         overlay_same_node=True, history_row_settled=bool(session))
    log(f"clean_undo_cycle OK · {oc['done_files']} 项 / {oc['done_bytes']/2**20:.1f}MB 清理→撤销→History 结清徽标 ✓")


def edge_reload_persistence(cdp):
    """location.reload() 后:init 重载台账、主题/版本正常、无崩溃。"""
    t0 = time.time()
    hist_before = cdp.evaluate(f"({S}).history.length")
    theme_before = cdp.evaluate("localStorage.getItem('zc-theme')")
    cdp.evaluate("location.reload()", timeout=30)
    time.sleep(4)
    cdp.close = True
    c2 = CDP()  # reload 可能重置 WS,重连
    wait_expr(c2, f"!!({S}.appVersion)", 30, desc="store ready after reload")
    hist_after = c2.evaluate(f"({S}).history.length")
    theme_after = c2.evaluate("localStorage.getItem('zc-theme')")
    ver = c2.evaluate(f"({S}).appVersion")
    # 期望版本从 tauri.conf.json 读,避免硬编码
    conf = json.load(open(os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                       "..", "src-tauri", "tauri.conf.json"), encoding="utf-8"))
    assert ver == conf["version"] and hist_after >= hist_before and theme_after == theme_before, \
        f"刷新后状态丢失: ver={ver} hist {hist_before}->{hist_after} theme {theme_before}->{theme_after}"
    IMPL(step="reload_persistence", ok=True, secs=round(time.time() - t0, 1),
         hist=[hist_before, hist_after], theme=theme_after)
    log(f"reload_persistence OK · 历史 {hist_after} 条 · 主题 {theme_after} · {ver}")
    return c2


def main():
    qa_lock("qa_edge")
    cdp = CDP()
    cdp.evaluate(f"{S}.togglePalette(false); {S}.setActivePage('home'); 0", timeout=30)
    time.sleep(0.5)
    failures = []
    steps = [edge_expand_rule, edge_empty_selection_guard, edge_cancel_scan,
             edge_dupes_fixture, edge_migrate_invalid_src, edge_radar_cross_page,
             edge_clean_undo_cycle]
    # 前三个 + radar_cross_page 依赖 scan 结果页:先扫一次供 expand/guard 用
    try:
        do_scan(cdp)
    except Exception as e:
        from qa_drive import capture_fail
        shot = capture_fail(cdp, "pre_scan")
        failures.append(("scan", str(e)[:200]))
        IMPL(step="pre_scan", status="FAIL", error=str(e)[:300], screenshot=shot)
        log(f"✗ 前置扫描失败: {e}")
    failures += run_steps(cdp, steps)
    try:
        cdp = edge_reload_persistence(cdp) or cdp
    except Exception as e:
        from qa_drive import capture_fail
        shot = capture_fail(cdp, "reload_persistence")
        failures.append(("reload_persistence", str(e)[:300]))
        IMPL(step="reload_persistence", status="FAIL", error=str(e)[:300], screenshot=shot)
        log(f"✗ reload_persistence: {str(e)[:300]}")
    tag = time.strftime("%H%M%S")
    path = rf"C:\Temp\zc-qa-edge-{tag}.json"
    write_report(path, failures)
    return summarize("边界 QA", path)


if __name__ == "__main__":
    sys.exit(main())
