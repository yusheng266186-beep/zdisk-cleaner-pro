# -*- coding: utf-8 -*-
"""ZDiskCleaner Pro 边界/异常路径 QA 驱动(第二轮:专打 happy path 之外)。

覆盖:规则展开、空勾选守卫、取消扫描、重复文件真实夹具、迁移非法路径、
雷达选中跨页跳转、清理→撤销闭环、刷新后状态持久化。
"""
import json
import os
import shutil
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from qa_drive import (CDP, S, goto, click_text, wait_expr, wait_toast, toasts,
                      native_set_input, probe, log, IMPL, REPORT, PAGE_IDS)

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
    """清空勾选后点清理 → 应 warn 且不进入清理态、不崩溃。"""
    t0 = time.time()
    click_text(cdp, "清空勾选")
    n = cdp.evaluate(f"({S}).selection.size")
    assert n == 0, f"清空后仍有勾选: {n}"
    click_text(cdp, "暂存区")
    msg = wait_toast(cdp, "请至少选择一条规则", timeout=15, desc="empty guard")
    phase = cdp.evaluate(f"({S}).phase")
    assert phase == "results", f"空勾选竟改变了状态: {phase}"
    IMPL(step="empty_selection_guard", ok=True, secs=round(time.time() - t0, 1), toast=msg)
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
        IMPL(step="cancel_scan", ok=True, secs=round(time.time() - t0, 1),
             toast="(扫描过快完成,取消路径未触发)")
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
    native_set_input(cdp, "(i.placeholder || '').includes('npm-cache')", r"C:\Temp\zc-no-such-dir-42")
    native_set_input(cdp, "(i.placeholder || '').includes('E:')", r"C:\Temp")
    click_text(cdp, "生成迁移计划")
    msg = wait_toast(cdp, "无法生成", "不存在", "失败", timeout=30, desc="invalid src")
    assert "确认执行迁移" not in cdp.evaluate("document.querySelector('main').innerText"), \
        "非法源竟生成了计划"
    IMPL(step="migrate_invalid_src", ok=True, secs=round(time.time() - t0, 1), toast=msg)
    log(f"migrate_invalid_src OK · {msg}")


def edge_radar_cross_page(cdp):
    """雷达 shift+点击最大块 → 选中条出现 → 「作为迁移源」跨页预填表单。"""
    t0 = time.time()
    goto(cdp, "空间雷达")
    wait_expr(cdp, "document.body.innerText.includes('个文件')", 300, interval=1.0, desc="radar tree")
    from qa_drive import _cdp_input_mouse
    host_rect = json.loads(cdp.evaluate(
        "JSON.stringify(document.querySelector('.min-h-40.flex-1').getBoundingClientRect())"))
    big = json.loads(cdp.evaluate(
        """(() => {
            const host = document.querySelector('.min-h-40.flex-1');
            let best = null;
            for (const d of host.children) {
                const st = getComputedStyle(d);
                const w = parseFloat(st.width), h = parseFloat(st.height);
                if (!best || w*h > best.w*best.h) best = { dx: d.offsetLeft + w/2, dy: d.offsetTop + h/2 };
            }
            return JSON.stringify(best);
        })()""", timeout=30))
    # shift+点击 → 选中(不下钻)
    for typ in ("mousePressed", "mouseReleased"):
        cdp.mid += 1
        req = json.dumps({"id": cdp.mid, "method": "Input.dispatchMouseEvent", "params": {
            "type": typ, "x": host_rect["x"] + big["dx"], "y": host_rect["y"] + big["dy"],
            "button": "left", "clickCount": 1, "modifiers": 8}})
        cdp._send_frame(0x1, req.encode())
        time.sleep(0.3)
        deadline = time.time() + 10
        while time.time() < deadline:
            payload = cdp._read_frame(10)
            d = json.loads(payload.decode("utf-8", "replace"))
            if d.get("id") == cdp.mid:
                break
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
    IMPL(step="radar_cross_page", ok=True, secs=round(time.time() - t0, 1), prefill=prefill[:60])
    log(f"radar_cross_page OK · 选中→跳转迁移中心,预填 {prefill[:60]}")


def edge_clean_undo_cycle(cdp):
    """清理→战报横幅「反悔」→ 数据原样搬回,vault 归零差值。"""
    t0 = time.time()
    v0 = vault_bytes()
    do_scan(cdp)
    guard = cdp.evaluate(
        f"JSON.stringify([...({S}).selection].filter(id => ({S}).rules.find(r => r.id === id)?.risk !== 'safe'))")
    assert json.loads(guard) == [], f"选中含非安全规则: {guard}"
    click_text(cdp, "暂存区")
    wait_expr(cdp, f"({S}).phase === 'idle' && ({S}).cleanOutcome", 600, interval=1.5, desc="clean done")
    oc = json.loads(cdp.evaluate(f"JSON.stringify((({S}).cleanOutcome ?? {{}}))"))
    v1 = vault_bytes()
    grew = v1 - v0
    log(f"清理 {oc['done_files']} 项 / {oc['done_bytes']} 字节 · vault {v0/2**20:.0f}→{v1/2**20:.0f}MB")
    assert grew >= oc["done_bytes"] * 0.9 or oc["done_bytes"] == 0, \
        f"vault 增量 {grew} 与账面 {oc['done_bytes']} 严重不符"
    # 战报横幅反悔
    click_text(cdp, "反悔 · 一键还原本批")
    wait_toast(cdp, "已还原", timeout=300, desc="undo toast")
    time.sleep(1.5)
    v2 = vault_bytes()
    log(f"撤销后 vault {v2/2**20:.0f}MB(回收 {grew - (v2 - v0):.0f} 字节级)")
    assert v2 <= v1 - grew * 0.9, f"撤销后 vault 未回吐: {v1}->{v2}"
    IMPL(step="clean_undo_cycle", ok=True, secs=round(time.time() - t0, 1),
         done_files=oc["done_files"], done_bytes=oc["done_bytes"],
         vault_mb=[round(v0 / 2**20), round(v1 / 2**20), round(v2 / 2**20)])
    log(f"clean_undo_cycle OK · {oc['done_files']} 项 / {oc['done_bytes']/2**20:.1f}MB 清理→撤销闭环 ✓")


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
        failures.append(("scan", str(e)[:200]))
        log(f"✗ 前置扫描失败: {e}")
    for fn in steps:
        try:
            fn(cdp)
        except Exception as e:
            failures.append((fn.__name__, str(e)[:300]))
            IMPL(step=fn.__name__, ok=False, error=str(e)[:300])
            log(f"✗ {fn.__name__}: {str(e)[:300]}")
    try:
        cdp = edge_reload_persistence(cdp) or cdp
    except Exception as e:
        failures.append(("reload_persistence", str(e)[:300]))
        IMPL(step="reload_persistence", ok=False, error=str(e)[:300])
        log(f"✗ reload_persistence: {str(e)[:300]}")
    tag = time.strftime("%H%M%S")
    path = rf"C:\Temp\zc-qa-edge-{tag}.json"
    with open(path, "w", encoding="utf-8") as f:
        json.dump({"steps": REPORT, "failures": failures}, f, ensure_ascii=False, indent=1)
    log(f"===== 边界 QA 完成: {len(REPORT) - len(failures)}/{len(REPORT)} 通过 · 报告 {path} =====")
    for name, err in failures:
        log(f"  ✗ {name}: {err}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
