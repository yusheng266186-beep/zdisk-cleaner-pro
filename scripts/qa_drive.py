# -*- coding: utf-8 -*-
"""ZDiskCleaner Pro GUI 全功能 QA 驱动 v3(v5.0 三态计数 + 失败截图 + CDP 健壮化)。

前置:应用以 WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9223" 启动。
原理:WebView2 CDP(原始 WebSocket)Runtime.evaluate 驱动真实 UI,配合
window.__zcStore(调试句柄)做状态断言;卡顿探针 = 重负载期间 JS 求值往返延迟。

要点(踩坑记录):
- 体检台主按钮文案是「开始智能体检」(H1「磁盘体检，一键开始」是标题不是按钮);
- TreemapCanvas 色块是 div 非 canvas(v5 起带 data-k=<key>),以页头「个文件」统计行判定渲染完成;
- AnimatePresence 页面过渡 ~0.4s,导航后必须等待挂载再查询;
- 生产构建无 window.__TAURI__ 全局,读磁盘空间用 Python shutil;
- purge 部分失败(文件被占用)的 toast 是「已删除 N 项，M 项未能删除…」。

v5 变更:
- IMPL 三态:每条记录带 status ∈ PASS/SKIP/FAIL;SKIP 是明示的跳过,不算绿也不算红;
  全绿判定 = FAIL 为 0 且 SKIP 逐条在报告与控制台列明;
- 任何 FAIL 当场 Page.captureScreenshot 存 C:\\Temp\\zc-qa-fail-<case>-<ts>.png;
- 报告头部新增 git_sha / exe_sha256 / app_version / 时间戳;
- CDP 客户端:WebSocket continuation 分片重组、断线一次自动重连、超时分级
  (socket 120s / evaluate 默认 30s / wait_expr 轮询内 10s / 总预算各自独立);
- 三套 QA(qa_drive/qa_edge/qa_v4)互斥文件锁 C:\\Temp\\zc-qa.lock,防并发 C5。
"""
import atexit
import base64
import datetime
import hashlib
import json
import os
import socket
import struct
import subprocess
import sys
import time
import urllib.request

PORT = 9223
REPORT = []
LOCK_PATH = r"C:\Temp\zc-qa.lock"
PROJECT_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


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

def IMPL(**kw):
    """追加一条用例记录;status 三态(PASS/SKIP/FAIL),SKIP 不得混入 PASS。"""
    if "status" not in kw:
        if kw.get("skipped"):
            kw["status"] = "SKIP"
        else:
            kw["status"] = "PASS" if kw.get("ok") else "FAIL"
    if kw["status"] == "PASS":
        kw.setdefault("ok", True)
    elif kw["status"] == "FAIL":
        kw["ok"] = False
    REPORT.append(kw)


def counts():
    c = {"PASS": 0, "SKIP": 0, "FAIL": 0}
    for r in REPORT:
        c[r.get("status", "FAIL")] = c.get(r.get("status", "FAIL"), 0) + 1
    return c


def log(msg):
    print(f"[{time.strftime('%H:%M:%S')}] {msg}", flush=True)


# ────────────────────────── 互斥锁(三套 QA 不得并发) ──────────────────────────

def qa_lock(owner):
    """C:\\Temp\\zc-qa.lock 独占创建;被占且未过期则直接退出(防并发 C5)。"""
    try:
        if os.path.exists(LOCK_PATH):
            age = time.time() - os.path.getmtime(LOCK_PATH)
            try:
                holder = open(LOCK_PATH, encoding="utf-8", errors="replace").read()[:80]
            except OSError:
                holder = "?"
            if age < 4 * 3600:
                log(f"✗ QA 锁被占用({age/60:.0f} 分钟前,{holder!r});并发跑 QA 会互相踩 UI,拒绝启动")
                sys.exit(2)
            log(f"QA 锁过期({age/3600:.1f}h),视为崩溃残留,抢占")
        fd = os.open(LOCK_PATH, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        os.write(fd, f"{owner} pid={os.getpid()} {time.strftime('%F %T')}".encode("utf-8"))
        os.close(fd)

        def _release():
            try:
                if os.path.exists(LOCK_PATH):
                    os.remove(LOCK_PATH)
            except OSError:
                pass
        atexit.register(_release)
    except FileExistsError:
        log("✗ QA 锁竞争失败(另一套 QA 刚抢占),拒绝启动")
        sys.exit(2)
    except OSError as e:
        log(f"⚠ QA 锁不可用({e}),继续(降级为无锁)")


# ────────────────────────── CDP 原始 WebSocket 客户端 ──────────────────────────

class Disconnected(RuntimeError):
    """socket 层断线/关闭——区别于业务错误,可触发一次自动重连。"""


class CDP:
    SOCKET_TIMEOUT = 120   # 底层 socket 读上限(保留)
    EVAL_TIMEOUT = 30      # 单次 evaluate 默认超时(v5 从 90 收紧)

    def __init__(self, port=PORT):
        self.port = port
        self.mid = 0
        self.buf = b""
        self._page_enabled = False
        self._connect()
        log(f"CDP connected: {self.title}")

    def _connect(self):
        targets = json.loads(urllib.request.urlopen(
            f"http://127.0.0.1:{self.port}/json/list", timeout=5).read().decode())
        page = next(t for t in targets if t.get("type") == "page")
        ws = page["webSocketDebuggerUrl"]
        hostport, _, path = ws[len("ws://"):].partition("/")
        host, _, hport = hostport.partition(":")
        ws_port = int(hport) if hport else self.port
        self.sock = socket.create_connection((host, ws_port), timeout=self.SOCKET_TIMEOUT)
        self.sock.settimeout(self.SOCKET_TIMEOUT)
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
        self._page_enabled = False
        self.title = f"{page.get('title','')[:40]} {page.get('url','')[:60]}"

    def reconnect(self):
        """断线一次重连:关掉残破 socket,重新走 /json/list 拿新的 debugger URL。"""
        try:
            self.sock.close()
        except Exception:
            pass
        deadline = time.time() + 15
        last = None
        while time.time() < deadline:
            try:
                self._connect()
                log(f"CDP 重连成功: {self.title}")
                return
            except Exception as e:
                last = e
                time.sleep(1.0)
        raise Disconnected(f"CDP 重连失败: {last}")

    def _recv_more(self, timeout):
        self.sock.settimeout(min(timeout, self.SOCKET_TIMEOUT))
        try:
            chunk = self.sock.recv(65536)
        except socket.timeout:
            raise TimeoutError("CDP socket read timeout")
        except OSError as e:
            raise Disconnected(f"socket error: {e}")
        if not chunk:
            raise Disconnected("socket closed")
        self.buf += chunk

    def _read_frame(self, timeout):
        """读一个完整 WebSocket 帧,返回 (fin, opcode, payload);ping 自动回 pong。"""
        while True:
            while len(self.buf) < 2:
                self._recv_more(timeout)
            b1, b2 = self.buf[0], self.buf[1]
            fin = bool(b1 & 0x80)
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
            if opcode == 0x9:  # ping → pong,继续等下一帧
                self._send_frame(0xA, payload)
                continue
            if opcode == 0x8:
                raise Disconnected("server close frame")
            return fin, opcode, payload

    def _read_message(self, timeout):
        """continuation(opcode 0x0)分片重组:拼到 FIN 为止再返回。"""
        deadline = time.time() + timeout
        msg = b""
        while True:
            remain = deadline - time.time()
            if remain <= 0:
                raise TimeoutError("CDP message read timeout")
            fin, opcode, payload = self._read_frame(remain)
            if opcode in (0x1, 0x2):       # 新消息首帧(text/binary)
                msg = payload
            elif opcode == 0x0:            # continuation → 拼接
                msg += payload
            else:
                continue
            if fin:
                return msg

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
        try:
            self.sock.sendall(bytes(header) + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))
        except OSError as e:
            raise Disconnected(f"send failed: {e}")

    def _call(self, method, params, timeout):
        self.mid += 1
        mid = self.mid
        req = json.dumps({"id": mid, "method": method, "params": params})
        self._send_frame(0x1, req.encode())
        deadline = time.time() + timeout
        while True:
            remain = deadline - time.time()
            if remain <= 0:
                raise TimeoutError(f"{method} timeout")
            payload = self._read_message(remain)
            try:
                d = json.loads(payload.decode("utf-8", "replace"))
            except Exception:
                continue
            if d.get("id") != mid:
                continue
            if "error" in d:
                raise RuntimeError(f"CDP error: {d['error']}")
            return d.get("result", {})

    def evaluate(self, expr, timeout=None, await_promise=True):
        """Runtime.evaluate;断线自动重连一次再重试。默认超时收紧到 30s。"""
        timeout = timeout or self.EVAL_TIMEOUT
        params = {"expression": expr, "returnByValue": True, "awaitPromise": await_promise}
        try:
            r = self._call("Runtime.evaluate", params, timeout)
        except (Disconnected, ConnectionError) as e:
            log(f"CDP 断线({str(e)[:80]}),重连一次后重试该求值")
            self.reconnect()
            r = self._call("Runtime.evaluate", params, timeout)
        if r.get("exceptionDetails"):
            det = r["exceptionDetails"]
            txt = det.get("exception", {}).get("description") or det.get("text") or "?"
            raise RuntimeError(f"JS exception: {txt[:200]}")
        return r.get("result", {}).get("value")

    def capture_screenshot(self, path):
        """Page.captureScreenshot → PNG 落盘;截图自身失败绝不掩盖原始失败。"""
        try:
            if not self._page_enabled:
                self._call("Page.enable", {}, 15)
                self._page_enabled = True
            r = self._call("Page.captureScreenshot", {"format": "png"}, 30)
            with open(path, "wb") as f:
                f.write(base64.b64decode(r.get("data", "")))
            return path
        except Exception as e:
            log(f"⚠ 截图失败({str(e)[:100]})")
            return None


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
    """轮询求值直到真值;单次求值 10s(细分超时:总预算=本函数 timeout,独立)。"""
    deadline = time.time() + timeout
    last = None
    while time.time() < deadline:
        try:
            last = cdp.evaluate(expr, timeout=min(10, max(5, deadline - time.time())))
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


# ────────────────────────── v5 共享断言辅助(qa_edge/qa_v4/qa_new_features 复用) ──────────────────────────

EXEC_SEL = "[data-testid=results-exec]"
OVERLAY_SEL = "[data-testid=cleaning-overlay]"


def results_exec(cdp, timeout=8):
    """Results 执行钮两段式 armed:点 → 文案变「确认清理 N 项」→ 再点执行。
    返回是否进入二次确认态(空勾选守卫可能一段点击即触发 warn,返回 False)。"""
    found = cdp.evaluate(f"!!document.querySelector({json.dumps(EXEC_SEL)})")
    assert found, f"未找到执行按钮 {EXEC_SEL}"
    cdp.evaluate(f"document.querySelector({json.dumps(EXEC_SEL)})?.click()", timeout=20)
    armed = None
    deadline = time.time() + timeout
    while time.time() < deadline:
        armed = cdp.evaluate(
            f"""(() => {{
                const b = document.querySelector({json.dumps(EXEC_SEL)});
                return b && /确认|再点/.test(b.textContent || '') ? b.textContent.trim() : null;
            }})()""", timeout=10)
        if armed:
            break
        time.sleep(0.3)
    if armed:
        cdp.evaluate(f"document.querySelector({json.dumps(EXEC_SEL)})?.click()", timeout=20)
    return armed


def overlay_arm(cdp, timeout=10):
    """清理遮罩必须全程同一元素不重挂:把当前节点引用暂存 window.__ovA、
    outerHTML 首 200 字符暂存 window.__ovH0,2s 后用 isSameNode + 前缀比对。"""
    got = wait_expr(cdp, f"!!document.querySelector({json.dumps(OVERLAY_SEL)})", timeout,
                    interval=0.15, desc="cleaning overlay mount")
    assert got, "清理期未出现 [data-testid=cleaning-overlay]"
    cdp.evaluate(
        f"""(() => {{
            const el = document.querySelector({json.dumps(OVERLAY_SEL)});
            window.__ovA = el;
            window.__ovH0 = el.outerHTML.slice(0, 200);
            return true;
        }})()""", timeout=15)


def overlay_check(cdp):
    """两次取样间隔 ≥2s 后调用:同一 DOM 引用 且 outerHTML 首 200 字符不变 → 通过。
    唯一豁免:清理已整体收尾(phase≠cleaning)——<2s 的清理本不存在重挂窗口。"""
    raw = cdp.evaluate(
        f"""(() => {{
            const el = document.querySelector({json.dumps(OVERLAY_SEL)});
            if (!el) return JSON.stringify({{ok: false, why: 'overlay 中途消失'}});
            if (!window.__ovA) return JSON.stringify({{ok: false, why: '锚点未暂存'}});
            const same = el.isSameNode(window.__ovA);
            const html = el.outerHTML.slice(0, 200) === window.__ovH0;
            return JSON.stringify({{ok: same && html, same, html}});
        }})()""", timeout=15)
    d = json.loads(raw)
    if not d.get("ok") and d.get("why") == "overlay 中途消失":
        phase = cdp.evaluate(f"({S}).phase")
        if phase != "cleaning":
            log("overlay_check · 清理在复检窗口前已收尾(<2s),无重挂窗口,单样本恒等成立")
            d["fast_clean"] = True
            return d
    assert d.get("ok"), f"清理遮罩被重挂/改写(违反同一元素不重挂契约): {d}"
    return d


def capture_fail(cdp, case):
    """FAIL 当场截图:C:\\Temp\\zc-qa-fail-<case>-<ts>.png,返回路径或 None。"""
    if cdp is None:
        return None
    ts = time.strftime("%H%M%S")
    safe = "".join(ch for ch in case if ch.isalnum() or ch in "-_")[:48]
    return cdp.capture_screenshot(rf"C:\Temp\zc-qa-fail-{safe}-{ts}.png")


def report_meta():
    """报告头:git SHA、exe SHA-256、app 版本、时间戳(发版纪律的可追溯锚)。"""
    meta = {
        "generated_at": datetime.datetime.now().isoformat(timespec="seconds"),
        "git_sha": None,
        "exe_path": None,
        "exe_sha256": None,
        "app_version": None,
    }
    try:
        meta["git_sha"] = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=PROJECT_ROOT,
            capture_output=True, text=True, timeout=10).stdout.strip() or None
    except Exception:
        pass
    try:
        conf = json.load(open(os.path.join(PROJECT_ROOT, "src-tauri", "tauri.conf.json"),
                              encoding="utf-8"))
        meta["app_version"] = conf.get("version")
    except Exception:
        pass
    cands = [os.path.join(cargo_release_dir(), "zdiskcleaner-pro.exe"),
             os.path.join(PROJECT_ROOT, "src-tauri", "target", "release", "zdiskcleaner-pro.exe"),
             os.path.join(PROJECT_ROOT, "target", "release", "zdiskcleaner-pro.exe")]
    cands = [p for p in cands if os.path.exists(p)]
    if cands:
        # 双 target 目录并存时取更新的那个(即当前在跑的二进制候选)
        exe = max(cands, key=os.path.getmtime)
        h = hashlib.sha256()
        with open(exe, "rb") as f:
            for blk in iter(lambda: f.read(1 << 20), b""):
                h.update(blk)
        meta["exe_path"] = exe
        meta["exe_sha256"] = h.hexdigest()
    return meta


def write_report(path, failures, header_extra=None):
    """统一落盘:头部元信息 + 三态计数 + 每步记录 + 失败明细(含截图路径)。"""
    meta = report_meta()
    if header_extra:
        meta.update(header_extra)
    body = {
        "meta": meta,
        "counts": counts(),
        "steps": REPORT,
        "failures": failures,
    }
    with open(path, "w", encoding="utf-8") as f:
        json.dump(body, f, ensure_ascii=False, indent=1)
    return meta


def summarize(suite, path):
    """控制台三态汇总行;全绿 = FAIL=0 且 SKIP 明示列出。返回退出码。"""
    c = counts()
    skips = [r for r in REPORT if r.get("status") == "SKIP"]
    fails = [r for r in REPORT if r.get("status") == "FAIL"]
    log(f"===== {suite} 完成: {c['PASS']} PASS / {c['SKIP']} SKIP / {c['FAIL']} FAIL · 报告 {path} =====")
    for r in skips:
        log(f"  ○ SKIP {r.get('step')}: {str(r.get('skipped', ''))[:120]}")
    for r in fails:
        log(f"  ✗ FAIL {r.get('step')}: {str(r.get('error', ''))[:200]}"
            + (f" · 截图 {r['screenshot']}" if r.get("screenshot") else ""))
    if c["SKIP"]:
        log(f"  ⚠ 存在 {c['SKIP']} 条 SKIP,以上已逐条列明(不计绿)")
    return 1 if c["FAIL"] else 0


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
    # v5 锚点:Home [data-testid=admin-toggle] 默认不勾;QA 全程保持不勾(不点它)
    admin = json.loads(cdp.evaluate("""(() => {
        const el = document.querySelector('[data-testid=admin-toggle]');
        if (!el) return JSON.stringify({found: false});
        const on = el.getAttribute('aria-checked') === 'true' || el.checked === true;
        return JSON.stringify({found: true, on});
    })()""") or "{}")
    assert admin.get("found"), "[data-testid=admin-toggle] 缺失"
    assert admin.get("on") is False, "admin-toggle 默认态不是「不勾」"
    assert not cdp.evaluate(f"({S}).homeAdmin"), "store.homeAdmin 默认应为 falsy"
    click_text(cdp, "开始智能体检")
    assert not cdp.evaluate(f"({S}).homeAdmin"), "QA 未点 toggle,homeAdmin 却被置真"
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
         selected_rules=d["selected"], max_probe_ms=max_probe, page_switch_during_scan=switched,
         admin_toggle="默认不勾·全程未点(锚点存在✓)")
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
        payload = cdp._read_message(15)
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
    # v5 根路径展示:兼容旧 input[title=…] 与新 select[data-testid=radar-root]
    root_path = cdp.evaluate("""(() => {
        const inp = document.querySelector('input[title="当前分析的根路径"]');
        if (inp) return inp.value;
        const sel = document.querySelector('select[data-testid=radar-root]');
        if (sel) return sel.options[sel.selectedIndex] ? (sel.options[sel.selectedIndex].textContent || sel.value) : sel.value;
        return null;
    })()""")
    assert root_path, "雷达根路径控件缺失(input[title] 与 select[data-testid=radar-root] 均未命中)"
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


def _disabled_rows(cdp):
    """禁用区 = li[data-key-id] 且行内带「恢复」按钮的行。返回 [key_id, 文本前80]。"""
    return cdp.evaluate("""(() => {
        const rows = [...document.querySelectorAll('li[data-key-id]')]
            .filter(li => [...li.querySelectorAll('button')].some(b => b.textContent.includes('恢复')));
        return rows.map(li => [li.dataset.keyId, li.innerText.replace(/\\s+/g, ' ').slice(0, 80)]);
    })()""", timeout=15)


def _click_fixture_row_button(cdp, needle, btn_text):
    """元素级点击:先定位含 needle 文本的 li,再点该 li 行内的 btn_text 按钮。
    绝不做全页 click_text——防止误点用户真实启动项。"""
    return cdp.evaluate(
        f"""(() => {{
            const lis = [...document.querySelectorAll('main li, li')];
            const li = lis.find(l => l.innerText.includes({json.dumps(needle)}));
            if (!li) return 'NO_ROW';
            const btn = [...li.querySelectorAll('button')].find(b => b.textContent.includes({json.dumps(btn_text)}));
            if (!btn) return 'NO_BTN';
            btn.click();
            return 'OK';
        }})()""", timeout=15)


def step_startup(cdp):
    """HKCU 自启夹具(值名含 ZC-QA-Test)禁用→行内恢复全流程。

    v5 重写要点:
    - 禁用/恢复均「元素级」:定位含 ZC-QA-Test 的 li 再点行内按钮,不再全页 click_text;
    - 禁用后断言新 UI:夹具行进禁用区(li[data-key-id] + 行内「恢复」)且「已禁用 N 项」计数 +1;
    - 恢复走禁用区行内「恢复」单条恢复(reg query 实证写回);绝不点「恢复全部」——
      那会覆盖用户自己的禁用列表;
    - 夹具被外部安全软件秒删 → 真 SKIP(不再记 PASS)。
    """
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
        # 3 秒内被删 → 诚实 SKIP(三态计数,不再冒充 PASS)
        time.sleep(3)
        if _run_val_get() is None:
            IMPL(step="startup", status="SKIP", secs=round(time.time() - t0, 1),
                 skipped="外部安全软件实时删除自启夹具 ZC-QA-Test,UI 级禁用/恢复无法稳定复测",
                 disable_registry_verif="skipped")
            log("startup SKIP · 夹具被外部看门狗秒删(真 SKIP,不计入通过数)")
            _run_val_del()
            return
        goto(cdp, "设置")
        goto(cdp, "启动项")  # 重新挂载,重读注册表
        wait_expr(cdp, "[...document.querySelectorAll('main li')].some(li => li.innerText.includes('ZC-QA-Test'))",
                  20, desc="fixture entry visible")
    n_before = cdp.evaluate("document.querySelectorAll('main li').length")
    dis_rows_before = _disabled_rows(cdp)
    dis_before = len(dis_rows_before)
    dis_counter_before = cdp.evaluate(
        "parseInt(document.body.innerText.match(/已禁用 (\\d+)/)?.[1] ?? '0')")
    # ① 元素级禁用夹具行(v5 起非 safe 上下文?启动项页无展开明细概念,行内「禁用」直点)
    got = _click_fixture_row_button(cdp, "ZC-QA-Test", "禁用")
    assert got == "OK", f"夹具行内「禁用」按钮点击失败: {got}"
    wait_toast(cdp, "已禁用", desc="disable")
    time.sleep(0.5)
    dis_rows_after = _disabled_rows(cdp)
    assert len(dis_rows_after) == dis_before + 1, \
        f"禁用区行数未+1: {dis_before}→{len(dis_rows_after)} ({dis_rows_after})"
    assert any("ZC-QA-Test" in r[1] for r in dis_rows_after), "夹具未进入禁用区 li[data-key-id]"
    dis_counter_after = cdp.evaluate(
        "parseInt(document.body.innerText.match(/已禁用 (\\d+)/)?.[1] ?? '0')")
    assert dis_counter_after == dis_counter_before + 1, \
        f"「已禁用 N 项」计数未+1: {dis_counter_before}→{dis_counter_after}"
    assert _run_val_get() is None, "注册表值未被摘除——禁用不是真动作"
    # ② 禁用区行内「恢复」(单条恢复,不再「恢复全部」)
    got = _click_fixture_row_button(cdp, "ZC-QA-Test", "恢复")
    assert got == "OK", f"禁用区行内「恢复」按钮点击失败: {got}"
    wait_toast(cdp, "已恢复", desc="restore")
    time.sleep(0.5)
    dis_rows_restored = _disabled_rows(cdp)
    assert len(dis_rows_restored) == dis_before, \
        f"恢复后禁用区行数不一致: {dis_before}→{len(dis_rows_restored)}"
    assert not any("ZC-QA-Test" in r[1] for r in dis_rows_restored), "夹具仍滞留在禁用区"
    restored_val = _run_val_get()
    assert restored_val is not None, "行内恢复未把值写回注册表(reg query 实证失败)"
    _run_val_del()  # 清理夹具
    IMPL(step="startup", ok=True, secs=round(time.time() - t0, 1), entries_before=n_before,
         fixture_created=created, pre_existing_fixture=had_fixture,
         disable_registry_verif=True, restore_registry_verif=restored_val[:40],
         ui_path="元素级行内按钮 + li[data-key-id] 禁用区断言 + 单条恢复(未点「恢复全部」)")
    log(f"startup OK · {round(time.time()-t0,1)}s · {n_before} 项 · 夹具禁用(禁用区✓ 注册表摘除✓)"
        f"→行内恢复(注册表写回✓)→夹具已清")


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
    # 卡A DISM 组件清理:v5 改 armed 两段确认——「开始清理」后必须再点含「再点一次」文案的按钮
    click_text(cdp, "开始清理")
    wait_expr(cdp,
              "!![...document.querySelectorAll('button')].find(b => b.textContent.includes('再点一次') && b.offsetParent)",
              10, interval=0.4, desc="dism armed")
    click_text(cdp, "再点一次")
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
    """真实 C 盘清理:安全档 → results-exec 两段式 → vault(遮罩同一元素断言)→
    历史页 purge → History 自动刷新(行消失或更新)。"""
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
    assert g["selected"], "勾选集为空,执行链路无从验证"
    log(f"安全档勾选 {len(g['selected'])} 条: {g['selected']}")
    # v5:Results 执行钮两段式 armed(点 → 「确认清理 N 项」→ 再点执行)
    armed = results_exec(cdp)
    assert armed, "results-exec 一段点击后未进入「确认清理」二次态(两段式失效?)"
    wait_expr(cdp, f"({S}).phase === 'cleaning'", 20, desc="cleaning phase")
    # 遮罩全程同一元素不重挂:挂载即取样 → 2s 后复检同一 DOM 引用 + outerHTML 首 200 字符
    overlay_arm(cdp)
    time.sleep(2.0)
    overlay_check(cdp)
    wait_expr(cdp, f"({S}).phase === 'idle' && ({S}).cleanOutcome", 600, interval=1.5, desc="clean done")
    oc = json.loads(cdp.evaluate(f"JSON.stringify((({S}).cleanOutcome ?? {{}}))"))
    session = cdp.evaluate(f"({S}).lastSessionId")
    log(f"清理完成: {oc.get('done_files')} 项 / {oc.get('done_bytes')} 字节 · 失败 {len(oc.get('failed', []))} · {oc.get('semantics_note','')[:60]}")
    # 历史页 → 顶层批次 彻底删除(armed 两次点击;部分失败也算跑通,如实记录)
    goto(cdp, "历史")
    wait_expr(cdp, "!![...document.querySelectorAll('button')].find(b => b.textContent.includes('彻底删除'))",
              15, desc="purge button")
    row_text_before = cdp.evaluate(
        f"""(() => {{
            const li = [...document.querySelectorAll('li[data-session]')]
                .find(l => (l.dataset.session || '') === {json.dumps(session or '')});
            return li ? li.innerText.replace(/\\s+/g, ' ').slice(0, 120) : null;
        }})()""") if session else None
    click_text(cdp, "彻底删除")
    time.sleep(0.4)
    click_text(cdp, "再点一次确认", timeout=6)
    purge_toast = wait_toast(cdp, "已彻底删除", "未能删除", timeout=180, desc="purge toast")
    # v5 新断言:purge 成功后 History 自动刷新(该 session 行消失 = 已无 vault 残留;或行文案更新)
    time.sleep(1.5)
    row_after = cdp.evaluate(
        f"""(() => {{
            const li = [...document.querySelectorAll('li[data-session]')]
                .find(l => (l.dataset.session || '') === {json.dumps(session or '')});
            return li ? li.innerText.replace(/\\s+/g, ' ').slice(0, 120) : null;
        }})()""") if session else None
    hist_gone = cdp.evaluate(
        f"!({S}).history.some(h => h.session_id === {json.dumps(session or '')})") if session else True
    assert session is None or row_after is None or row_after != row_text_before or hist_gone, \
        f"purge 后 History 未自动刷新: 行文案不变且 session 仍在台账 ({row_text_before!r})"
    free_after = free_bytes_c(cdp)
    vault_after = _vault_bytes()
    freed_delta = free_after - free_before
    IMPL(step="real_clean", ok=True, secs=round(time.time() - t0, 1), session=session,
         done_files=oc.get("done_files"), done_bytes=oc.get("done_bytes"),
         failed=len(oc.get("failed", [])), failed_sample=oc.get("failed", [])[:3],
         note=oc.get("semantics_note"), purge_toast=purge_toast,
         exec_mode="results-exec 两段式 armed", overlay_same_node=True,
         history_refresh=("行消失" if (row_after is None and session) else "行已更新"),
         vault_before_mb=round(vault_before / 2**20, 1), vault_after_mb=round(vault_after / 2**20, 1),
         free_before_gb=round(free_before / 2**30, 3), free_after_gb=round(free_after / 2**30, 3),
         freed_mb=round(freed_delta / 2**20, 1))
    log(f"real_clean OK · C盘可用 {free_before/2**30:.2f}GB → {free_after/2**30:.2f}GB "
        f"(Δ{freed_delta/2**20:+.1f}MB) · vault {vault_before/2**20:.0f}→{vault_after/2**20:.0f}MB · {purge_toast}")
    return freed_delta


def step_results_reentry(cdp):
    """v5 新步骤 12:清理流程后 report 被战报横幅语义消费(置 null)——
    先跑一次全新扫描拿到 report(内嵌 pre_reentry_scan 用例),再验证
    切去历史页后侧栏「体检结果」再入项能带回结果页。"""
    t0 = time.time()
    step_scan(cdp, label="pre_reentry_scan")
    assert cdp.evaluate(f"!!({S}).report"), "前置缺失:重扫后 store.report 仍为空"
    goto(cdp, "历史")
    nav = wait_expr(cdp, "!!document.querySelector('nav [data-nav=results]')", 12,
                    interval=0.4, desc="results nav present")
    assert nav, "report 存在但侧栏无 nav [data-nav=results] 再入项"
    nav_txt = cdp.evaluate("document.querySelector('nav [data-nav=results]')?.textContent || ''")
    assert "体检结果" in nav_txt, f"再入项文案异常: {nav_txt!r}"
    cdp.evaluate("document.querySelector('nav [data-nav=results]').click()", timeout=20)
    wait_expr(cdp, f"!!document.querySelector({json.dumps(EXEC_SEL)}) && !!({S}).report",
              15, interval=0.5, desc="results view restored")
    IMPL(step="results_reentry", ok=True, secs=round(time.time() - t0, 1),
         nav_text=nav_txt.strip()[:20], report_kept=True)
    log(f"results_reentry OK · 历史页 → 侧栏「{nav_txt.strip()[:10]}」→ 结果页可回(report 仍在)")


# ────────────────────────── 主流程 ──────────────────────────

def run_steps(cdp, steps):
    """按序执行步骤;FAIL 当场截图并记录路径。返回 failures 列表。"""
    failures = []
    for fn in steps:
        try:
            fn(cdp)
        except Exception as e:
            shot = capture_fail(cdp, fn.__name__)
            failures.append((fn.__name__, str(e)[:300]))
            IMPL(step=fn.__name__, status="FAIL", error=str(e)[:300], screenshot=shot)
            log(f"✗ {fn.__name__} 失败: {str(e)[:300]}" + (f" · 截图 {shot}" if shot else ""))
    return failures


def main():
    tag = datetime.datetime.now().strftime("%H%M%S")
    skip_clean = "--skip-real-clean" in sys.argv
    qa_lock("qa_drive")
    cdp = CDP()
    cdp.evaluate(f"{S}.togglePalette(false); {S}.setActivePage('home'); 0", timeout=30)
    time.sleep(0.5)
    steps = [step_boot, step_scan, step_radar, step_bigfiles, step_dupes,
             step_startup, step_migrate, step_deeptools, step_settings, step_palette]
    if not skip_clean:
        steps += [step_real_clean, step_results_reentry]
    failures = run_steps(cdp, steps)
    path = rf"C:\Temp\zc-qa-report-{tag}.json"
    write_report(path, failures)
    return summarize("GUI 全功能 QA", path)


if __name__ == "__main__":
    sys.exit(main())
