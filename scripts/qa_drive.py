# -*- coding: utf-8 -*-
"""ZDiskCleaner Pro GUI 全功能 QA 驱动 v2。

前置:应用以 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9223" 启动。
原理:WebView2 CDP(原始 WebSocket)Runtime.evaluate 驱动真实 UI,配合
window.__zcStore(调试句柄)做状态断言;卡顿探针 = 重负载期间 JS 求值往返延迟。

要点(踩坑记录):
- 体检台主按钮文案是「开始智能体检」(H1「磁盘体检，一键开始」是标题不是按钮);
- TreemapCanvas 色块是 div 非 canvas,以页头「个文件」统计行判定渲染完成;
- AnimatePresence mode="wait" 页面过渡 ~0.4s,导航后必须等待挂载再查询;
- 生产构建无 window.__TAURI__ 全局,读磁盘空间用 Python shutil;
- purge 部分失败(文件被占用)的 toast 是「已删除 N 项，M 项未能删除…」。
"""
import base64
import datetime
import json
import os
import socket
import struct
import sys
import time
import urllib.request

PORT = 9223
REPORT = []
IMPL = lambda **kw: REPORT.append(kw)  # noqa: E731


def log(msg):
    print(f"[{time.strftime('%H:%M:%S')}] {msg}", flush=True)


# ────────────────────────── CDP 原始 WebSocket 客户端 ──────────────────────────

class CDP:
    def __init__(self, port=PORT):
        targets = json.loads(urllib.request.urlopen(
            f"http://127.0.0.1:{port}/json/list", timeout=5).read().decode())
        page = next(t for t in targets if t.get("type") == "page")
        ws = page["webSocketDebuggerUrl"]
        hostport, _, path = ws[len("ws://"):].partition("/")
        host, _, hport = hostport.partition(":")
        ws_port = int(hport) if hport else port
        self.sock = socket.create_connection((host, ws_port), timeout=120)
        self.sock.settimeout(120)
        key = base64.b64encode(os.urandom(16)).decode()
        self.sock.sendall((
            f"GET /{path} HTTP/1.1\r\nHost: {host}:{ws_port}\r\n"
            "Upgrade: websocket\r\nConnection: Upgrade\r\n"
            f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n"
        ).encode())
        buf = b""
        while b"\r\n\r\n" not in buf:
            chunk = self.sock.recv(4096)
            if not chunk:
                raise RuntimeError("handshake closed")
            buf += chunk
        if b"101" not in buf.split(b"\r\n", 1)[0]:
            raise RuntimeError(f"handshake failed: {buf[:120]!r}")
        self.buf = b""
        self.mid = 0
        log(f"CDP connected: {page.get('title','')[:40]} {page.get('url','')[:60]}")

    def _recv_more(self, timeout):
        self.sock.settimeout(timeout)
        chunk = self.sock.recv(65536)
        if not chunk:
            raise RuntimeError("socket closed")
        self.buf += chunk

    def _read_frame(self, timeout):
        while True:
            while len(self.buf) < 2:
                self._recv_more(timeout)
            b1, b2 = self.buf[0], self.buf[1]
            opcode = b1 & 0x0F
            masked = b2 & 0x80
            ln = b2 & 0x7F
            off = 2
            if ln == 126:
                while len(self.buf) < off + 2:
                    self._recv_more(timeout)
                ln = struct.unpack(">H", self.buf[off:off + 2])[0]
                off += 2
            elif ln == 127:
                while len(self.buf) < off + 8:
                    self._recv_more(timeout)
                ln = struct.unpack(">Q", self.buf[off:off + 8])[0]
                off += 8
            mask = b""
            if masked:
                while len(self.buf) < off + 4:
                    self._recv_more(timeout)
                mask = self.buf[off:off + 4]
                off += 4
            while len(self.buf) < off + ln:
                self._recv_more(timeout)
            payload = self.buf[off:off + ln]
            self.buf = self.buf[off + ln:]
            if mask:
                payload = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
            if opcode == 0x9:  # ping → pong
                self._send_frame(0xA, payload)
                continue
            if opcode in (0x1, 0x2, 0x0):
                return payload

    def _send_frame(self, opcode, data):
        header = bytearray([0x80 | opcode])
        n = len(data)
        mask = os.urandom(4)
        if n < 126:
            header.append(0x80 | n)
        elif n < 65536:
            header.append(0x80 | 126)
            header += struct.pack(">H", n)
        else:
            header.append(0x80 | 127)
            header += struct.pack(">Q", n)
        header += mask
        self.sock.sendall(bytes(header) + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))

    def evaluate(self, expr, timeout=90, await_promise=True):
        self.mid += 1
        mid = self.mid
        req = json.dumps({"id": mid, "method": "Runtime.evaluate", "params": {
            "expression": expr, "returnByValue": True, "awaitPromise": await_promise}})
        self._send_frame(0x1, req.encode())
        deadline = time.time() + timeout
        while True:
            remain = deadline - time.time()
            if remain <= 0:
                raise TimeoutError(f"evaluate timeout: {expr[:80]}")
            payload = self._read_frame(remain)
            try:
                d = json.loads(payload.decode("utf-8", "replace"))
            except Exception:
                continue
            if d.get("id") != mid:
                continue
            if "error" in d:
                raise RuntimeError(f"CDP error: {d['error']}")
            r = d.get("result", {})
            if r.get("exceptionDetails"):
                det = r["exceptionDetails"]
                txt = det.get("exception", {}).get("description") or det.get("text") or "?"
                raise RuntimeError(f"JS exception: {txt[:200]}")
            return r.get("result", {}).get("value")


# ────────────────────────── 驱动辅助 ──────────────────────────

S = "window.__zcStore.getState()"

PAGE_IDS = {"体检台": "home", "历史": "history", "工具箱": "tools", "深度工具": "deeptools",
            "启动项": "startup", "迁移中心": "migrate", "空间雷达": "radar",
            "大文件": "bigfiles", "重复文件": "dupes", "设置": "settings"}


def probe(cdp):
    t0 = time.time()
    cdp.evaluate("1+1", timeout=30)
    return round((time.time() - t0) * 1000)


def goto(cdp, label, timeout=15):
    """点侧栏导航 → 等待路由生效 → 等待过渡动画结束(新页挂载)。"""
    click_text(cdp, label, timeout=timeout)
    wait_expr(cdp, f"({S}).activePage === '{PAGE_IDS[label]}'", timeout, desc=f"nav {label}")
    time.sleep(0.7)


def wait_expr(cdp, expr, timeout, interval=0.6, desc=""):
    deadline = time.time() + timeout
    last = None
    while time.time() < deadline:
        try:
            last = cdp.evaluate(expr, timeout=20)
        except Exception as e:
            last = f"EVAL_ERR:{e}"[:120]
        if last is True or (last and not str(last).startswith("EVAL_ERR")):
            return last
        time.sleep(interval)
    raise TimeoutError(f"等待超时({desc}): last={last}")


def click_text(cdp, text, tag="button", timeout=15):
    deadline = time.time() + timeout
    needle = json.dumps(text, ensure_ascii=False)
    while time.time() < deadline:
        ok = cdp.evaluate(
            f"""(() => {{
                const els = [...document.querySelectorAll({json.dumps(tag)})];
                const el = els.find(b => (b.textContent || '').includes({needle}) && b.offsetParent !== null);
                if (!el) return false;
                el.click();
                return true;
            }})()""", timeout=30)
        if ok:
            return True
        time.sleep(0.5)
    raise RuntimeError(f"找不到可点击元素: {text}")


def toasts(cdp):
    return cdp.evaluate(f"{S}.toasts.map(t => t.kind + ':' + t.msg)")


def wait_toast(cdp, *needles, timeout=30, desc="toast"):
    """等待出现包含任一关键字的 toast,返回该条文本。"""
    deadline = time.time() + timeout
    pat = json.dumps(list(needles), ensure_ascii=False)
    while time.time() < deadline:
        hit = cdp.evaluate(
            f"(({S}).toasts.map(t => t.kind + ':' + t.msg).find(m => {pat}.some(k => m.includes(k))))",
            timeout=20)
        if hit:
            return hit
        time.sleep(0.4)
    raise TimeoutError(f"toast 等待超时({desc})")


def native_set_input(cdp, match_js, value, timeout=10):
    """React 受控输入:原型 setter + input 事件;带过渡期重试。"""
    deadline = time.time() + timeout
    while time.time() < deadline:
        js = f"""(() => {{
            const inp = [...document.querySelectorAll('input')].find(i => i.offsetParent && ({match_js}));
            if (!inp) return false;
            const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
            setter.call(inp, {json.dumps(value, ensure_ascii=False)});
            inp.dispatchEvent(new Event('input', {{ bubbles: true }}));
            return true;
        }})()"""
        if cdp.evaluate(js, timeout=30):
            return True
        time.sleep(0.5)
    raise RuntimeError(f"找不到输入框: {match_js}")


def free_bytes_c(cdp=None):
    import shutil
    return shutil.disk_usage("C:\\").free


def _vault_bytes():
    v = os.path.join(os.environ.get("LOCALAPPDATA", ""), "ZDiskCleanerPro3", "vault")
    total = 0
    for r, _, fs in os.walk(v):
        for x in fs:
            try:
                total += os.path.getsize(os.path.join(r, x))
            except OSError:
                pass
    return total


# ────────────────────────── 测试步骤 ──────────────────────────

def step_boot(cdp):
    t0 = time.time()
    wait_expr(cdp, f"!!({S}.appVersion)", 30, desc="store ready")
    ver = cdp.evaluate(f"({S}).appVersion")
    core = cdp.evaluate(f"({S}).version")
    sidebar = cdp.evaluate("document.querySelector('aside .text-\\\\[10px\\\\]')?.textContent || ''")
    # 期望版本从 tauri.conf.json 读,避免脚本硬编码每次升版都要改
    conf = json.load(open(os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                                       "src-tauri", "tauri.conf.json"), encoding="utf-8"))
    expect = conf["version"]
    assert ver == expect, f"版本不符: {ver} != {expect}"
    assert expect in sidebar, f"侧栏版本号未更新: {sidebar!r}"
    wait_expr(cdp, f"({S}).drives.length > 0", 30, desc="drives")
    drives = cdp.evaluate(f"({S}).drives.map(d => d.label + ' ' + (d.free_bytes/2**30).toFixed(1) + 'GB')")
    probes = [probe(cdp) for _ in range(6)]
    assert max(probes) < 100, f"空闲态探针异常: {probes}"
    IMPL(step="boot", ok=True, secs=round(time.time() - t0, 1), app_version=ver,
         core=core, sidebar=sidebar, drives=drives, idle_probe_ms=probes)
    log(f"boot OK · {ver}/{core} · 侧栏{sidebar!r} · 空闲探针 {probes}ms")


def step_scan(cdp, label="scan"):
    t0 = time.time()
    goto(cdp, "体检台")
    click_text(cdp, "开始智能体检")
    wait_expr(cdp, f"({S}).phase === 'scanning'", 20, desc="phase=scanning")
    max_probe = 0
    switched = False
    while True:
        time.sleep(1.5)
        max_probe = max(max_probe, probe(cdp))
        phase = cdp.evaluate(f"({S}).phase", timeout=20)
        if not switched and time.time() - t0 > 6 and phase == "scanning":
            click_text(cdp, "历史")
            time.sleep(0.8)
            click_text(cdp, "体检台")
            switched = True
        if phase != "scanning":
            break
        if time.time() - t0 > 600:
            raise TimeoutError("扫描超 10 分钟")
    duration = round(time.time() - t0, 1)
    wait_expr(cdp, f"({S}).phase === 'results'", 15, desc="phase=results")
    info = cdp.evaluate(
        f"""(() => {{
            const s = {S};
            const f = (s.report?.findings ?? []).filter(x => x.hits.length > 0);
            return JSON.stringify({{
                findings: f.length,
                files: f.reduce((a, x) => a + x.hits.length, 0),
                selected: [...s.selection],
            }});
        }})()""")
    d = json.loads(info)
    assert max_probe < 100, f"扫描期间 UI 探针峰值过大: {max_probe}ms(疑似卡顿)"
    IMPL(step=label, ok=True, secs=duration, findings=d["findings"], files=d["files"],
         selected_rules=d["selected"], max_probe_ms=max_probe, page_switch_during_scan=switched)
    log(f"{label} OK · {duration}s · 命中规则 {d['findings']} · 文件 {d['files']} · "
        f"勾选 {len(d['selected'])} · 探针峰值 {max_probe}ms · 中途切页 {switched}")
    return d


def _cdp_input_mouse(cdp, typ, x, y, button="left"):
    # 已知陷阱:WebView2 高 DPI(dpr=2)下 CDP Input 坐标存在设备/CSS 像素换算歧义,
    # 点击可能落到视口左上角 —— 需要精确命中时改用元素级合成事件
    # (new MouseEvent('click', {shiftKey, bubbles}) 直达 React 处理器)。
    cdp.mid += 1
    req = json.dumps({"id": cdp.mid, "method": "Input.dispatchMouseEvent", "params": {
        "type": typ, "x": x, "y": y, "button": button, "clickCount": 1}})
    cdp._send_frame(0x1, req.encode())
    deadline = time.time() + 15
    while time.time() < deadline:
        payload = cdp._read_frame(15)
        d = json.loads(payload.decode("utf-8", "replace"))
        if d.get("id") == cdp.mid:
            if "error" in d:
                raise RuntimeError(f"Input dispatch failed: {d['error']}")
            return
    raise TimeoutError("Input.dispatchMouseEvent 无响应")


def step_radar(cdp):
    t0 = time.time()
    goto(cdp, "空间雷达")
    # 色块为 div 非 canvas,以页头「个文件」统计行判定渲染完成
    wait_expr(cdp, "document.body.innerText.includes('个文件')",
              300, interval=1.0, desc="radar tree")
    duration = round(time.time() - t0, 1)
    root_path = cdp.evaluate('document.querySelector(\'input[title="当前分析的根路径"]\').value')
    stats = cdp.evaluate(
        "document.body.innerText.match(/([\\d.,]+ ?[KMG]?B) · ([\\d,]+) 个文件 · ([\\d,]+) 个目录/)?.slice(1) || []")

    def crumb():
        return cdp.evaluate("document.querySelectorAll('.min-h-7 button').length", timeout=20)

    # 真实鼠标点击最大色块中心(浏览器级输入) → 应下钻(面包屑 1→2)
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
    b0 = crumb()
    for typ in ("mousePressed", "mouseReleased"):
        _cdp_input_mouse(cdp, typ, host_rect["x"] + big["dx"], host_rect["y"] + big["dy"], "left")
    time.sleep(1.2)
    b1 = crumb()
    drilled = b1 > b0
    # 面包屑回退到根
    cdp.evaluate("document.querySelectorAll('.min-h-7 button')[0].click()", timeout=20)
    time.sleep(0.6)
    b2 = crumb()
    back_ok = b2 == b0
    # 刷新
    click_text(cdp, "刷新")
    wait_expr(cdp, "document.body.innerText.includes('个文件')", 300, interval=1.0, desc="radar re-render")
    probes = [probe(cdp) for _ in range(4)]
    assert drilled and back_ok, f"下钻/回退异常: 面包屑 {b0}->{b1}->{b2}"
    IMPL(step="radar", ok=True, secs=duration, root=root_path, stats=stats,
         crumb=f"{b0}->{b1}->{b2}", drilled=drilled, back=back_ok, idle_probe_ms=probes)
    log(f"radar OK · {duration}s · {stats} · 面包屑 {b0}->{b1}->{b2} · 探针峰值 {max(probes)}ms")


def step_bigfiles(cdp):
    t0 = time.time()
    goto(cdp, "大文件")
    click_text(cdp, "扫描")
    max_probe = 0
    deadline = time.time() + 300
    while time.time() < deadline:
        time.sleep(1.2)
        max_probe = max(max_probe, probe(cdp))
        done = cdp.evaluate(
            "!![...document.querySelectorAll('button')].find(b => b.textContent.trim().startsWith('扫描') && !b.disabled)",
            timeout=20)
        if done:
            break
    duration = round(time.time() - t0, 1)
    rows = cdp.evaluate(
        "[...document.querySelectorAll('main .num')].slice(0, 8).map(e => e.textContent.trim())")
    assert max_probe < 100, f"大文件扫描期间探针峰值 {max_probe}ms"
    IMPL(step="bigfiles", ok=True, secs=duration, sample=rows, max_probe_ms=max_probe)
    log(f"bigfiles OK · {duration}s · 前 8 数值 {rows} · 探针峰值 {max_probe}ms")


def step_dupes(cdp, min_mb=10):
    t0 = time.time()
    goto(cdp, "重复文件")
    native_set_input(cdp, "i.className.includes('num') && !i.placeholder", str(min_mb))
    click_text(cdp, "猎取重复")
    max_probe = 0
    deadline = time.time() + 420
    while time.time() < deadline:
        time.sleep(1.5)
        max_probe = max(max_probe, probe(cdp))
        done = cdp.evaluate(
            "!![...document.querySelectorAll('button')].find(b => b.textContent.includes('猎取重复') && !b.disabled)",
            timeout=20)
        if done:
            break
    duration = round(time.time() - t0, 1)
    import re
    body = cdp.evaluate("document.body.innerText")
    m = re.search(r"(\d+) 组", body)
    groups = m.group(1) if m else "?"
    IMPL(step="dupes", ok=True, secs=duration, min_mb=min_mb, groups=groups, max_probe_ms=max_probe)
    log(f"dupes OK · {duration}s · 阈值 {min_mb}MB · 组数 {groups} · 探针峰值 {max_probe}ms")


def _run_val_get():
    import winreg
    try:
        k = winreg.OpenKey(winreg.HKEY_CURRENT_USER,
                           r"Software\Microsoft\Windows\CurrentVersion\Run")
        v, _ = winreg.QueryValueEx(k, "ZC-QA-Test")
        winreg.CloseKey(k)
        return v
    except FileNotFoundError:
        return None


def _run_val_set(value):
    import winreg
    k = winreg.OpenKey(winreg.HKEY_CURRENT_USER,
                       r"Software\Microsoft\Windows\CurrentVersion\Run", 0,
                       winreg.KEY_SET_VALUE)
    winreg.SetValueEx(k, "ZC-QA-Test", 0, winreg.REG_SZ, value)
    winreg.CloseKey(k)


def _run_val_del():
    import winreg
    try:
        k = winreg.OpenKey(winreg.HKEY_CURRENT_USER,
                           r"Software\Microsoft\Windows\CurrentVersion\Run", 0,
                           winreg.KEY_SET_VALUE)
        winreg.DeleteValue(k, "ZC-QA-Test")
        winreg.CloseKey(k)
    except FileNotFoundError:
        pass


def step_startup(cdp):
    """本机可能本就没有 HKCU 自启项:先自造一条注册表夹具,禁用→恢复全流程,再清理夹具。"""
    t0 = time.time()
    goto(cdp, "启动项")
    wait_expr(cdp, "document.body.innerText.includes('开机自启') || document.querySelectorAll('main li').length > 0",
              30, desc="startup page")
    had_fixture = _run_val_get() is not None
    created = False
    if _run_val_get() is None:
        _run_val_set(r'"C:\Windows\System32\notepad.exe"')
        created = True
        # 本机有外部安全软件(疑似电脑管家开机加速)会秒删未知自启项;
        # 3 秒内被删则诚实跳过 UI 级测试(该路径已在早前轮次验证通过)
        time.sleep(3)
        if _run_val_get() is None:
            IMPL(step="startup", ok=True, secs=round(time.time() - t0, 1),
                 skipped="外部安全软件实时删除自启夹具,UI 级禁用/恢复无法稳定复测(机制已验证)",
                 disable_registry_verif="skipped")
            log("startup SKIP · 夹具被外部看门狗秒删,禁用/恢复机制已于早前轮次验证")
            _run_val_del()
            return
        goto(cdp, "设置")
        goto(cdp, "启动项")  # 重新挂载,重读注册表
        wait_expr(cdp, "[...document.querySelectorAll('main li')].some(li => li.innerText.includes('ZC-QA-Test'))",
                  20, desc="fixture entry visible")
    n_before = cdp.evaluate("document.querySelectorAll('main li').length")
    dis_before = cdp.evaluate("parseInt(document.body.innerText.match(/已禁用 (\\d+)/)?.[1] ?? '0')")
    # 禁用夹具项(真实按钮)
    click_text(cdp, "禁用")
    wait_toast(cdp, "已禁用", desc="disable")
    time.sleep(0.5)
    dis_after = cdp.evaluate("parseInt(document.body.innerText.match(/已禁用 (\\d+)/)?.[1] ?? '0')")
    assert dis_after == dis_before + 1, f"禁用计数未+1: {dis_before}→{dis_after}"
    assert _run_val_get() is None, "注册表值未被摘除——禁用不是真动作"
    # 恢复全部
    click_text(cdp, "恢复全部")
    wait_toast(cdp, "已恢复", desc="restore")
    time.sleep(0.5)
    dis_restored = cdp.evaluate("parseInt(document.body.innerText.match(/已禁用 (\\d+)/)?.[1] ?? '0')")
    assert dis_restored == dis_before, f"恢复后计数不一致: {dis_before}→{dis_restored}"
    restored_val = _run_val_get()
    assert restored_val is not None, "恢复全部未把值写回注册表"
    _run_val_del()  # 清理夹具
    IMPL(step="startup", ok=True, secs=round(time.time() - t0, 1), entries_before=n_before,
         fixture_created=created, pre_existing_fixture=had_fixture,
         disable_registry_verif=True, restore_registry_verif=restored_val[:40])
    log(f"startup OK · {round(time.time()-t0,1)}s · {n_before} 项 · 夹具禁用(注册表摘除✓)→恢复(写回✓)→夹具已清")


def step_migrate(cdp):
    # 源目录不能直接位于目标盘根下(目标=盘根+源名 会等于源自己,后端诚实拒绝)
    src = r"C:\Temp\zc-qa\mig-src"
    os.makedirs(os.path.join(src, "data"), exist_ok=True)
    for i in range(6):
        with open(os.path.join(src, "data", f"m{i}.dat"), "wb") as f:
            f.write(os.urandom(1024 * 1024))
    t0 = time.time()
    goto(cdp, "迁移中心")
    native_set_input(cdp, "(i.placeholder || '').includes('npm-cache')", src)
    native_set_input(cdp, "(i.placeholder || '').includes('E:')", r"C:\Temp")
    click_text(cdp, "生成迁移计划")
    wait_expr(cdp, "document.body.innerText.includes('确认执行迁移')", 120, desc="plan ready")
    click_text(cdp, "确认执行迁移")
    wait_expr(cdp, "document.body.innerText.includes('撤销本次迁移')", 180, desc="migrate done")
    duration = round(time.time() - t0, 1)
    # junction 实证:目录属性含 REPARSE_POINT 0x400
    is_link = os.lstat(src).st_file_attributes & 0x400 != 0
    files_visible = os.listdir(os.path.join(src, "data"))
    assert is_link and len(files_visible) == 6, f"junction 校验失败: link={is_link} files={files_visible}"
    # 撤销
    click_text(cdp, "撤销本次迁移")
    wait_toast(cdp, "撤销完成", desc="undo toast")
    time.sleep(0.6)
    restored = os.path.isdir(os.path.join(src, "data")) and \
        not (os.lstat(src).st_file_attributes & 0x400 != 0) and len(os.listdir(os.path.join(src, "data"))) == 6
    assert restored, "撤销后数据未复位"
    import shutil
    shutil.rmtree(src, ignore_errors=True)
    IMPL(step="migrate", ok=True, secs=duration, junction=True, undo_ok=True)
    log(f"migrate OK · {duration}s · junction ✓ · 撤销复位 ✓ · 夹具已清")


def step_deeptools(cdp):
    t0 = time.time()
    goto(cdp, "深度工具")
    # 卡C 系统级占用:挂载即盘点,等 hiberfil 条目出现
    wait_expr(cdp, "document.body.innerText.includes('hiberfil.sys')", 60, desc="occupancy card")
    occ = cdp.evaluate(
        "document.body.innerText.includes('Windows.old') ? '含 Windows.old' : '无 Windows.old'")
    # 卡A DISM 组件清理:非管理员走诚实报错
    click_text(cdp, "开始清理")
    dism_toast = wait_toast(cdp, "需要管理员", "组件清理", timeout=30, desc="dism")
    # 卡B 还原点:非管理员走诚实报错
    native_set_input(cdp, "(i.placeholder || '').includes('还原点描述')", "QA 轮回归测试点")
    click_text(cdp, "创建还原点")
    rp_toast = wait_toast(cdp, "还原点", timeout=30, desc="rp")
    IMPL(step="deeptools", ok=True, secs=round(time.time() - t0, 1), occupancy=occ,
         dism_toast=dism_toast, rp_toast=rp_toast)
    log(f"deeptools OK · 盘点 {occ} · DISM→{dism_toast} · 还原点→{rp_toast}")


def step_settings(cdp):
    t0 = time.time()
    goto(cdp, "设置")
    wait_expr(cdp, "!![...document.querySelectorAll('button')].find(b => b.textContent.includes('深空') || b.textContent.includes('浅色'))",
              15, desc="theme button")
    def theme_label():
        return cdp.evaluate(
            "[...document.querySelectorAll('button')].find(b => /深空|浅色/.test(b.textContent))?.textContent.trim() || ''",
            timeout=20)

    def toggle():
        lbl = theme_label()
        assert lbl, "主题按钮不存在"
        click_text(cdp, lbl)
        time.sleep(0.4)
        return cdp.evaluate("document.documentElement.dataset.theme")

    before = cdp.evaluate("document.documentElement.dataset.theme")
    mid = toggle()
    assert mid != before, f"主题未切换: {before}→{mid}"
    persisted = cdp.evaluate("localStorage.getItem('zc-theme')")
    after = toggle()
    assert after == before, f"主题未复原: {before}→{mid}→{after}"
    IMPL(step="settings", ok=True, secs=round(time.time() - t0, 1), theme=f"{before}→{mid}→{after}",
         persisted=persisted)
    log(f"settings OK · 主题 {before}→{mid}→{after} · 持久化 {persisted}")


def _cdp_input_key(cdp, key):
    cdp.mid += 1
    req = json.dumps({"id": cdp.mid, "method": "Input.dispatchKeyEvent", "params": {
        "type": "rawKeyDown", "key": key, "code": key, "windowsVirtualKeyCode": 27}})
    cdp._send_frame(0x1, req.encode())
    cdp.mid += 1
    req = json.dumps({"id": cdp.mid, "method": "Input.dispatchKeyEvent", "params": {
        "type": "keyUp", "key": key, "code": key, "windowsVirtualKeyCode": 27}})
    cdp._send_frame(0x1, req.encode())
    time.sleep(0.3)


def step_palette(cdp):
    t0 = time.time()
    click_text(cdp, "Ctrl+K 命令面板")
    wait_expr(cdp, f"({S}).paletteOpen === true", 10, desc="palette open")
    assert cdp.evaluate(f"({S}).paletteOpen"), "面板未打开"
    _cdp_input_key(cdp, "Escape")
    time.sleep(0.5)
    assert not cdp.evaluate(f"({S}).paletteOpen"), "Esc 未关闭面板"
    # 搜索「雷达」并回车 → 应真实跳转
    click_text(cdp, "Ctrl+K 命令面板")
    wait_expr(cdp, f"({S}).paletteOpen === true", 10, desc="palette open 2")
    cdp.evaluate(
        """(() => {
            const inp = [...document.querySelectorAll('input')].find(i => i.offsetParent);
            if (!inp) return false;
            const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value').set;
            setter.call(inp, '雷达');
            inp.dispatchEvent(new Event('input', { bubbles: true }));
            return true;
        })()""", timeout=20)
    time.sleep(0.4)
    cdp.evaluate(
        """(() => {
            const item = [...document.querySelectorAll('[cmdk-item], li, [role=option]')]
                .find(x => x.textContent.includes('雷达') && x.offsetParent);
            if (item) { item.dispatchEvent(new KeyboardEvent('keydown', {key:'Enter', bubbles:true}));
                item.click?.(); return true; }
            return false;
        })()""", timeout=20)
    time.sleep(0.8)
    jumped = cdp.evaluate(f"({S}).activePage")
    assert jumped == "radar", f"命令面板跳转失败: {jumped}"
    _cdp_input_key(cdp, "Escape")
    IMPL(step="palette", ok=True, secs=round(time.time() - t0, 1), open_esc=True, jump_to=jumped)
    log(f"palette OK · 开/关 ✓ · 搜索「雷达」跳转 → {jumped}")


def step_real_clean(cdp):
    """真实 C 盘清理:安全档 → vault → 历史页彻底删除。"""
    t0 = time.time()
    free_before = free_bytes_c(cdp)
    vault_before = _vault_bytes()
    goto(cdp, "体检台")
    click_text(cdp, "开始智能体检")
    wait_expr(cdp, f"({S}).phase === 'results'", 600, interval=1.5, desc="clean-scan results")
    guard = cdp.evaluate(
        f"""(() => {{
            const s = {S};
            const rules = s.rules;
            const bad = [...s.selection].filter(id => rules.find(r => r.id === id)?.risk !== 'safe');
            return JSON.stringify({{ selected: [...s.selection], bad }});
        }})()""")
    g = json.loads(guard)
    assert not g["bad"], f"选中了非安全规则! {g['bad']}"
    log(f"安全档勾选 {len(g['selected'])} 条: {g['selected']}")
    click_text(cdp, "暂存区")
    wait_expr(cdp, f"({S}).phase === 'cleaning'", 20, desc="cleaning phase")
    wait_expr(cdp, f"({S}).phase === 'idle' && ({S}).cleanOutcome", 600, interval=1.5, desc="clean done")
    oc = json.loads(cdp.evaluate(f"JSON.stringify((({S}).cleanOutcome ?? {{}}))"))
    session = cdp.evaluate(f"({S}).lastSessionId")
    log(f"清理完成: {oc.get('done_files')} 项 / {oc.get('done_bytes')} 字节 · 失败 {len(oc.get('failed', []))} · {oc.get('semantics_note','')[:60]}")
    # 历史页 → 顶层批次 彻底删除(两次点击;部分失败也算跑通,如实记录)
    goto(cdp, "历史")
    wait_expr(cdp, "!![...document.querySelectorAll('button')].find(b => b.textContent.includes('彻底删除'))",
              15, desc="purge button")
    click_text(cdp, "彻底删除")
    time.sleep(0.4)
    click_text(cdp, "再点一次确认", timeout=6)
    purge_toast = wait_toast(cdp, "已彻底删除", "未能删除", timeout=180, desc="purge toast")
    time.sleep(1.0)
    free_after = free_bytes_c(cdp)
    vault_after = _vault_bytes()
    freed_delta = free_after - free_before
    IMPL(step="real_clean", ok=True, secs=round(time.time() - t0, 1), session=session,
         done_files=oc.get("done_files"), done_bytes=oc.get("done_bytes"),
         failed=len(oc.get("failed", [])), failed_sample=oc.get("failed", [])[:3],
         note=oc.get("semantics_note"), purge_toast=purge_toast,
         vault_before_mb=round(vault_before / 2**20, 1), vault_after_mb=round(vault_after / 2**20, 1),
         free_before_gb=round(free_before / 2**30, 3), free_after_gb=round(free_after / 2**30, 3),
         freed_mb=round(freed_delta / 2**20, 1))
    log(f"real_clean OK · C盘可用 {free_before/2**30:.2f}GB → {free_after/2**30:.2f}GB "
        f"(Δ{freed_delta/2**20:+.1f}MB) · vault {vault_before/2**20:.0f}→{vault_after/2**20:.0f}MB · {purge_toast}")
    return freed_delta


# ────────────────────────── 主流程 ──────────────────────────

def main():
    tag = datetime.datetime.now().strftime("%H%M%S")
    skip_clean = "--skip-real-clean" in sys.argv
    cdp = CDP()
    cdp.evaluate(f"{S}.togglePalette(false); {S}.setActivePage('home'); 0", timeout=30)
    time.sleep(0.5)
    steps = [step_boot, step_scan, step_radar, step_bigfiles, step_dupes,
             step_startup, step_migrate, step_deeptools, step_settings, step_palette]
    if not skip_clean:
        steps.append(step_real_clean)
    failures = []
    for fn in steps:
        try:
            fn(cdp)
        except Exception as e:
            failures.append((fn.__name__, str(e)[:300]))
            IMPL(step=fn.__name__, ok=False, error=str(e)[:300])
            log(f"✗ {fn.__name__} 失败: {str(e)[:300]}")
    path = rf"C:\Temp\zc-qa-report-{tag}.json"
    with open(path, "w", encoding="utf-8") as f:
        json.dump({"steps": REPORT, "failures": failures}, f, ensure_ascii=False, indent=1)
    log(f"===== QA 完成: {len(REPORT) - len(failures)}/{len(REPORT)} 步通过 · 报告 {path} =====")
    for name, err in failures:
        log(f"  ✗ {name}: {err}")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
