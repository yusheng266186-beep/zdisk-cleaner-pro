# -*- coding: utf-8 -*-
"""v5.0 GUI 新功能冒烟 QA(复用 qa_drive 的 CDP 客户端与三态框架)。

覆盖(全部是 v5 新 UI 锚点):
1. init-error      —— 正常启动不得出现 [data-testid=init-error];
2. recyclebin_card —— Home [data-card=recyclebin]:query>0 才真清(armed 二击含「确认」),=0 记 SKIP「回收站已空」;
3. radar_switch    —— select[data-testid=radar-root] 切分区:树重渲染、骨架不报错;
4. hist_detail     —— History li[data-session] 详情按钮 detail-<id> 下钻,entry-<id>-<i> 行数>0;
5. hist_chips      —— 筛选 chips [data-testid=hf-vault] 点击后行数变化;
6. busy_cancel     —— BigFiles 全盘 TopN 进行中点 [data-testid=busy-cancel] → toast/错误态,不挂死。

前置同 qa_drive:应用以 --remote-debugging-port=9223 启动。
"""
import json
import os
import re
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from qa_drive import (CDP, S, goto, click_text, wait_expr, wait_toast, probe,
                      log, IMPL, REPORT, qa_lock, run_steps, write_report, summarize)

REPORT.clear()


# ────────────────────────── 用例 ──────────────────────────

def feat_init_error(cdp):
    """正常启动:错误横幅锚点不得存在,store.initError 应为空。"""
    t0 = time.time()
    wait_expr(cdp, f"!!({S}.appVersion)", 30, desc="store ready")
    node = cdp.evaluate("!!document.querySelector('[data-testid=init-error]')")
    init_err = cdp.evaluate(f"({S}).initError ?? null")
    assert not node, "正常启动竟出现 [data-testid=init-error]"
    assert not init_err, f"store.initError 非空: {str(init_err)[:120]}"
    IMPL(step="init_error", ok=True, secs=round(time.time() - t0, 1),
         anchor="[data-testid=init-error] 不存在", store_init_error=None)
    log("init_error OK · 错误横幅未出现(锚点缺席=正常)")


def feat_recyclebin_card(cdp):
    """Home 回收站卡:query 项数 >0 才执行清空(armed 二击含「确认」);=0 真 SKIP。"""
    t0 = time.time()
    goto(cdp, "体检台")
    card = wait_expr(cdp, "!!document.querySelector('[data-card=recyclebin]')", 15,
                     interval=0.4, desc="recyclebin card")
    assert card, "Home 缺 [data-card=recyclebin] 卡"
    text = cdp.evaluate("document.querySelector('[data-card=recyclebin]').innerText")
    m = re.search(r"([\d,]+)\s*项", text or "")
    items = int(m.group(1).replace(",", "")) if m else (0 if re.search(r"空的|无需打理|已空|干净", text or "") else None)
    if items == 0:
        IMPL(step="recyclebin_card", status="SKIP", secs=round(time.time() - t0, 1),
             skipped="回收站已空(items=0),清空链路无从验证", card_text=(text or "")[:80])
        log("recyclebin_card SKIP · 回收站已空")
        return
    assert items is not None and items > 0, f"回收站卡无法解析项数: {text!r}"
    log(f"回收站 {items} 项 → 执行 UI 清空(armed 二击)")
    # 一段点击:卡内「清空回收站」按钮
    first = cdp.evaluate("""(() => {
        const card = document.querySelector('[data-card=recyclebin]');
        const btn = [...card.querySelectorAll('button')].find(b => b.textContent.includes('清空回收站'));
        if (!btn) return 'NO_BTN';
        btn.click(); return btn.textContent.trim();
    })()""", timeout=20)
    assert first != "NO_BTN", "回收站卡内找不到「清空回收站」按钮"
    # 二击:armed 后按钮文案应含「确认」(或出现独立含「确认」的按钮)
    armed = None
    deadline = time.time() + 8
    while time.time() < deadline:
        armed = cdp.evaluate("""(() => {
            const card = document.querySelector('[data-card=recyclebin]');
            const btn = [...card.querySelectorAll('button')]
                .find(b => b.textContent.includes('确认') || b.textContent.includes('再点'));
            return btn ? btn.textContent.trim() : null;
        })()""", timeout=10)
        if armed:
            break
        time.sleep(0.3)
    assert armed, f"一段点击后未出现含「确认」的二次按钮(armed 失效?): 卡文案={first!r}"
    cdp.evaluate(f"""(() => {{
        const card = document.querySelector('[data-card=recyclebin]');
        const btn = [...card.querySelectorAll('button')]
            .find(b => b.textContent.includes('确认') || b.textContent.includes('再点'));
        btn.click(); return true;
    }})()""", timeout=20)
    msg = wait_toast(cdp, "回收站", "清空", timeout=120, desc="recycle empty toast")
    # 清空后复查:卡重查应归零(store 或卡文案)
    time.sleep(2.5)
    text2 = cdp.evaluate("document.querySelector('[data-card=recyclebin]').innerText") or ""
    m2 = re.search(r"([\d,]+)\s*项", text2)
    parsed2 = int(m2.group(1).replace(",", "")) if m2 else None
    empty_state = parsed2 == 0 or ("项" not in text2 and any(k in text2 for k in ("已空", "干净", "0")))
    assert empty_state, f"清空后回收站卡未复查归零: {text2[:100]!r}"
    items2 = parsed2 if parsed2 is not None else 0
    IMPL(step="recyclebin_card", ok=True, secs=round(time.time() - t0, 1),
         items_before=items, toast=str(msg)[:80], items_after=items2, armed_text=armed[:30])
    log(f"recyclebin_card OK · {items}→0 项 · {str(msg)[:60]}")


def feat_radar_switch(cdp):
    """雷达分区切换:select[data-testid=radar-root] 选另一分区 → 树重渲染且骨架不报错。"""
    t0 = time.time()
    goto(cdp, "空间雷达")
    wait_expr(cdp, "document.body.innerText.includes('个文件')", 300, interval=1.0, desc="radar tree 原根")
    info = json.loads(cdp.evaluate("""(() => {
        const sel = document.querySelector('select[data-testid=radar-root]');
        if (!sel) return JSON.stringify({found: false});
        return JSON.stringify({found: true, value: sel.value,
                               options: [...sel.options].map(o => o.value)});
    })()"""))
    if not info.get("found"):
        IMPL(step="radar_switch", status="SKIP", skipped="select[data-testid=radar-root] 不存在(UI 未接线,记为未尽事项)")
        log("radar_switch SKIP · 根选择器不存在")
        return
    others = [v for v in info["options"] if v != info["value"]]
    if not others:
        IMPL(step="radar_switch", status="SKIP", secs=round(time.time() - t0, 1),
             skipped=f"仅一个可选分区 {info['options']},无「另一分区」可切")
        log(f"radar_switch SKIP · 仅分区 {info['options']}")
        return
    target = others[0]
    # React 受控 select:native setter + change 事件
    changed = cdp.evaluate(
        f"""(() => {{
            const sel = document.querySelector('select[data-testid=radar-root]');
            const setter = Object.getOwnPropertyDescriptor(window.HTMLSelectElement.prototype, 'value').set;
            setter.call(sel, {json.dumps(target)});
            sel.dispatchEvent(new Event('change', {{ bubbles: true }}));
            return sel.value === {json.dumps(target)};
        }})()""", timeout=20)
    assert changed, f"select 值未被接受: {target}"
    # 新根重分析是忙任务:等它起跑(容忍秒完)、再等归零,之后才判渲染
    try:
        wait_expr(cdp, f"({S}).busyRunning === true", 10, interval=0.3, desc="radar busy start")
    except TimeoutError:
        pass
    wait_expr(cdp, f"({S}).busyRunning !== true", 600, interval=2.0, desc="radar reanalysis settle")
    time.sleep(1.0)
    ok = cdp.evaluate("document.body.innerText.includes('个文件')")
    assert ok, "切换分区后未重新渲染出统计行(树缺失/骨架卡死)"
    err = cdp.evaluate(f"({S}).initError ?? null") or cdp.evaluate("!!document.querySelector('[data-testid=init-error]')")
    assert not err, f"切换分区后出现错误态: {str(err)[:120]}"
    stats = cdp.evaluate(
        "document.body.innerText.match(/([\\d.,]+ ?[KMG]?B) · ([\\d,]+) 个文件 · ([\\d,]+) 个目录/)?.slice(1) || []")
    # 还原到原分区(礼貌复位;失败不影响本用例结论)
    try:
        cdp.evaluate(
            f"""(() => {{
                const sel = document.querySelector('select[data-testid=radar-root]');
                const setter = Object.getOwnPropertyDescriptor(window.HTMLSelectElement.prototype, 'value').set;
                setter.call(sel, {json.dumps(info['value'])});
                sel.dispatchEvent(new Event('change', {{ bubbles: true }}));
                return true;
            }})()""", timeout=20)
        wait_expr(cdp, f"({S}).busyRunning !== true", 600, interval=2.0, desc="radar settle back")
    except Exception as e:
        log(f"⚠ 分区复位失败(不判负): {str(e)[:80]}")
    IMPL(step="radar_switch", ok=True, secs=round(time.time() - t0, 1),
         from_root=info["value"], to_target=target, stats=stats)
    log(f"radar_switch OK · {info['value']} → {target} · {stats}")


def _history_rows(cdp):
    return cdp.evaluate(
        "[...document.querySelectorAll('li[data-session]')].map(li => li.dataset.session)")


def feat_history_detail(cdp):
    """History 详情下钻:最近(优先 vault 批次)行点 detail-<id> → entry-<id>-<i> 行数>0。"""
    t0 = time.time()
    goto(cdp, "历史")
    wait_expr(cdp, f"({S}).history.length !== undefined", 15, desc="history store")
    n_hist = cdp.evaluate(f"({S}).history.length")
    if not n_hist:
        IMPL(step="history_detail", status="SKIP", secs=round(time.time() - t0, 1),
             skipped="台账无任何批次(先跑 qa_drive/qa_edge 产生历史后再验)")
        log("history_detail SKIP · 无历史批次")
        return
    rows = _history_rows(cdp)
    assert rows, "store.history 非空但页面没有 li[data-session] 行"
    # 选最近一个「vault/暂存」类批次行:行文本含 暂存/vault;找不到则用首行(最近)
    sid = json.loads(cdp.evaluate(
        f"""JSON.stringify(((({S}).history.find(h => h.mode === 'vault' && h.live !== false) || {{}})).session_id || null)"""))
    if not sid:
        IMPL(step="history_detail", status="SKIP", secs=round(time.time() - t0, 1),
             skipped=f"历史 {len(rows)} 行中无 vault/暂存类批次(详情下钻以 vault 批次为夹具)")
        log("history_detail SKIP · 台账无 vault 批次")
        return
    opened = cdp.evaluate(
        f"""(() => {{
            const li = [...document.querySelectorAll('li[data-session]')]
                .find(l => l.dataset.session === {json.dumps(sid)});
            const btn = li && (li.querySelector('[data-testid^="detail-"]') ||
                               li.querySelector('button[id^="detail-"]') ||
                               [...li.querySelectorAll('button')].find(b => /详情|展开/.test(b.textContent)));
            if (!btn) return false;
            btn.click(); return true;
        }})()""", timeout=20)
    assert opened, f"session {sid} 行内找不到详情按钮 detail-<id>"

    def entry_counts():
        return json.loads(cdp.evaluate(
            f"""(() => {{
                const els = [...document.querySelectorAll('[data-testid], [id]')]
                    .filter(e => ((e.dataset && e.dataset.testid) || e.id || '').startsWith('entry-'));
                const mine = els.filter(e => ((e.dataset && e.dataset.testid) || e.id || '')
                    .startsWith('entry-' + {json.dumps(sid)}));
                return JSON.stringify([mine.length, els.length]);
            }})()""", timeout=15))

    deadline = time.time() + 15
    mine = total = 0
    while time.time() < deadline:
        mine, total = entry_counts()
        if mine or total:
            break
        time.sleep(0.5)
    assert mine or total, f"详情下钻后 entry-<id>-<i> 行为 0(sid={sid})"
    IMPL(step="history_detail", ok=True, secs=round(time.time() - t0, 1), session=sid,
         entries=mine or total, entries_attributed_to_session=mine)
    log(f"history_detail OK · {sid[:24]} 展开 {mine or total} 条 entries(归属本批 {mine})")


def feat_history_chips(cdp):
    """History 筛选 chips:[data-testid=hf-vault] 点击前后 li[data-session] 行数变化。"""
    t0 = time.time()
    goto(cdp, "历史")
    has_chip = cdp.evaluate("!!document.querySelector('[data-testid=hf-vault]')")
    rows0 = _history_rows(cdp)
    if not has_chip:
        IMPL(step="history_chips", status="SKIP", skipped="[data-testid=hf-vault] chip 不存在(UI 未接线)")
        log("history_chips SKIP · 无 hf-vault chip")
        return
    if not rows0:
        IMPL(step="history_chips", status="SKIP", skipped="无历史行可筛选")
        log("history_chips SKIP · 无历史行")
        return
    all_vault = cdp.evaluate(
        "[...document.querySelectorAll('li[data-session]')].every(li => /暂存|vault/i.test(li.innerText))")
    non_vault = cdp.evaluate(f"({S}).history.filter(h => !/vault|clean|manual|recycle|empty/i.test(h.kind || '')).length")
    if all_vault and non_vault == 0:
        IMPL(step="history_chips", status="SKIP", skipped="台账全部为 vault 批次,vault 筛选天然无行数差")
        log("history_chips SKIP · 全 vault 批次")
        return
    cdp.evaluate("document.querySelector('[data-testid=hf-vault]').click()", timeout=20)
    time.sleep(0.9)
    rows1 = _history_rows(cdp)
    assert len(rows1) != len(rows0), \
        f"点 hf-vault chip 前后行数未变化({len(rows0)}→{len(rows1)}),筛选未生效或全量同型"
    # 复位 chip(取消筛选)恢复全量
    cdp.evaluate("document.querySelector('[data-testid=hf-vault]').click()", timeout=20)
    time.sleep(0.9)
    rows2 = _history_rows(cdp)
    assert len(rows2) == len(rows0), f"二次点击 chip 未复位: {len(rows0)}→{len(rows1)}→{len(rows2)}"
    IMPL(step="history_chips", ok=True, secs=round(time.time() - t0, 1),
         rows=[len(rows0), len(rows1), len(rows2)])
    log(f"history_chips OK · 行数 {len(rows0)}→{len(rows1)}(筛选)→{len(rows2)}(复位)")


def feat_busy_cancel(cdp):
    """忙任务取消:BigFiles 全盘 TopN 扫描进行中点 [data-testid=busy-cancel] →
    toast/错误态回执、busyRunning 归零、UI 不挂死。"""
    t0 = time.time()
    goto(cdp, "大文件")
    # 复位:上一用例若留下忙态则等待
    wait_expr(cdp, f"({S}).busyRunning !== true", 60, interval=1.0, desc="idle before start")
    btn_ready = cdp.evaluate(
        "!![...document.querySelectorAll('button')].find(b => b.textContent.trim().startsWith('扫描') && !b.disabled)")
    assert btn_ready, "找不到可用的「扫描」按钮"
    click_text(cdp, "扫描")
    # 忙窗口出现?(全盘 TopN 正常应持续数秒;极快收尾则真 SKIP)
    try:
        wait_expr(cdp, f"({S}).busyRunning === true", 6, interval=0.3, desc="busyRunning=true")
        started = True
    except TimeoutError:
        started = False
    if not started:
        time.sleep(2)
        fast_done = cdp.evaluate(
            "!![...document.querySelectorAll('button')].find(b => b.textContent.trim().startsWith('扫描') && !b.disabled)"
        ) and cdp.evaluate(f"({S}).busyRunning !== true")
        if fast_done:
            IMPL(step="busy_cancel", status="SKIP", secs=round(time.time() - t0, 1),
                 skipped="全盘扫描过快收尾,忙任务窗口未出现,取消钮无从点")
            log("busy_cancel SKIP · 扫描过快")
            return
    cancel_ok = wait_expr(cdp, "!!document.querySelector('[data-testid=busy-cancel]')",
                          15, interval=0.4, desc="busy-cancel visible")
    assert cancel_ok, "[data-testid=busy-cancel] 未出现(忙态无取消入口?)"
    cdp.evaluate("document.querySelector('[data-testid=busy-cancel]').click()", timeout=20)
    # 取消回执:取消类 toast 或错误态;随后 busyRunning 归假、UI 不挂死
    msg = None
    try:
        msg = wait_toast(cdp, "取消", timeout=20, desc="busy cancel toast")
    except TimeoutError:
        pass
    settled = wait_expr(cdp, f"({S}).busyRunning !== true", 90, interval=1.0, desc="busy settled")
    assert settled, "取消后 busyRunning 未归零(疑似挂死)"
    p = probe(cdp)
    assert p < 500, f"取消后 UI 探针 {p}ms,疑似假死"
    btn_back = cdp.evaluate(
        "!![...document.querySelectorAll('button')].find(b => b.textContent.trim().startsWith('扫描') && !b.disabled)")
    err_state = cdp.evaluate(
        "document.querySelector('main') ? /失败|错误|取消/.test(document.querySelector('main').innerText) : false")
    assert msg or btn_back or err_state, "取消后既无 toast 回执、无错误态,扫描按钮也未恢复(挂死或静默吞失败)"
    IMPL(step="busy_cancel", ok=True, secs=round(time.time() - t0, 1),
         toast=str(msg)[:80] if msg else "(无 toast)", probe_ms=p,
         btn_recovered=bool(btn_back), err_state=bool(err_state))
    log(f"busy_cancel OK · cancel 点击 → busyRunning 归零 · 探针 {p}ms · {str(msg or '')[:40]}")


# ────────────────────────── 主流程 ──────────────────────────

def main():
    qa_lock("qa_new_features")
    cdp = CDP()
    cdp.evaluate(f"{S}.togglePalette(false); {S}.setActivePage('home'); 0", timeout=30)
    time.sleep(0.5)
    steps = [feat_init_error, feat_recyclebin_card, feat_radar_switch,
             feat_history_detail, feat_history_chips, feat_busy_cancel]
    failures = run_steps(cdp, steps)
    path = rf"C:\Temp\zc-qa-newfeat-{time.strftime('%H%M%S')}.json"
    write_report(path, failures)
    return summarize("v5 新功能冒烟", path)


if __name__ == "__main__":
    sys.exit(main())
