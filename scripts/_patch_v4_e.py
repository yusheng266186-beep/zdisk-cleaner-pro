# -*- coding: utf-8 -*-
"""重写 qa_v4 的 radar 步(路径匹配版)与 migrate 步(预建目标根)。"""

p = r'C:\Users\yusheng\.zcode\workspace\default\ZDiskCleanerPro\scripts\qa_v4.py'
src = open(p, encoding='utf-8').read()

# ── radar 步整体替换 ──
start = src.index('def v4_radar_stash(cdp):')
end = src.index('def v4_migrate_background(cdp):')
radar = r'''def v4_radar_stash(cdp):
    """空间雷达:1GB 顶层夹具。小块可能不到出字门槛(88px),
    所以遍历所有色块 shift+点击,以选中条的真实路径匹配夹具,再安全删除。"""
    t0 = time.time()
    fx = r"C:\Users\yusheng\zc-v4-radar"
    shutil.rmtree(fx, ignore_errors=True)
    os.makedirs(fx)
    with open(os.path.join(fx, "heavy.bin"), "wb") as f:
        f.write(os.urandom(1024 * 1024 * 1024))
    goto(cdp, "空间雷达")
    wait_expr(cdp, "document.body.innerText.includes('个文件')", 300, interval=1.0, desc="tree")
    host_rect = json.loads(cdp.evaluate(
        "JSON.stringify(document.querySelector('.min-h-40.flex-1').getBoundingClientRect())"))
    tiles = json.loads(cdp.evaluate(
        """JSON.stringify([...document.querySelectorAll('.min-h-40.flex-1 > div')].map(d => ({
            dx: d.offsetLeft + parseFloat(getComputedStyle(d).width)/2,
            dy: d.offsetTop + parseFloat(getComputedStyle(d).height)/2,
        })))""", timeout=30))
    target_path = None
    for t in tiles:
        for typ in ("mousePressed", "mouseReleased"):
            cdp.mid += 1
            req = json.dumps({"id": cdp.mid, "method": "Input.dispatchMouseEvent", "params": {
                "type": typ, "x": host_rect["x"] + t["dx"], "y": host_rect["y"] + t["dy"],
                "button": "left", "clickCount": 1, "modifiers": 8}})
            cdp._send_frame(0x1, req.encode())
            deadline = time.time() + 10
            while time.time() < deadline:
                d = json.loads(cdp._read_frame(10).decode("utf-8", "replace"))
                if d.get("id") == cdp.mid:
                    break
        time.sleep(0.35)
        sel = cdp.evaluate(
            "document.querySelector('[role=status] .num')?.textContent || ''", timeout=20)
        if "zc-v4-radar" in sel:
            target_path = sel
            break
    assert target_path, "遍历全部色块未选中到夹具目录"
    log(f"  选中条命中: {target_path}")
    click_text(cdp, "移入暂存区")
    time.sleep(0.4)
    click_text(cdp, "再点一次确认删除", timeout=6)
    wait_expr(cdp, f"({S}).toasts.some(t => t.msg.includes('已移入暂存区'))", 60, desc="radar stash toast")
    wait_expr(cdp, "document.body.innerText.includes('个文件')", 300, interval=1.0, desc="tree rebuild")
    assert not os.path.exists(fx), "夹具目录仍在磁盘"
    IMPL(step="radar_stash", ok=True, secs=round(time.time() - t0, 1))
    log("radar_stash OK · 选中(按路径匹配)→暂存区→磁盘已无 ✓")


'''
src = src[:start] + radar + src[end:]

# ── migrate 步:残链清理 + 预建目标根 ──
old = r'''    src = r"C:\Temp\zc-v4-mig"
    shutil.rmtree(src, ignore_errors=True)
    os.makedirs(os.path.join(src, "d"))'''
new = r'''    src = r"C:\Temp\zc-v4-mig"
    dstroot = r"C:\Temp\zc-v4-mig-dst"
    # 上轮残留的 junction 先摘链再清数据目录;目标根必须真实存在
    if os.path.lexists(src):
        try:
            os.lstat(src).st_file_attributes & 0x400 and os.rmdir(src)
        except OSError:
            pass
    shutil.rmtree(src, ignore_errors=True)
    shutil.rmtree(dstroot, ignore_errors=True)
    os.makedirs(dstroot)
    os.makedirs(os.path.join(src, "d"))'''
assert old in src, "mig fixture block"
src = src.replace(old, new, 1)
src = src.replace(
    "native_set_input(cdp, \"(i.placeholder || '').includes('E:')\", r\"C:\\Temp\\zc-v4-mig-dst\")",
    "native_set_input(cdp, \"(i.placeholder || '').includes('E:')\", dstroot)")

open(p, 'w', encoding='utf-8').write(src)
import ast
ast.parse(src)
print('radar+migrate rewritten OK')
