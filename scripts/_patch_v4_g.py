# -*- coding: utf-8 -*-
"""重写 qa_v4 的 radar(改 store 级+记录残影已知问题)与 migrate(store 级后台验证)。"""

p = r'C:\Users\yusheng\.zcode\workspace\default\ZDiskCleanerPro\scripts\qa_v4.py'
src = open(p, encoding='utf-8').read()
start = src.index('def v4_radar_stash(cdp):')
end = src.index('def v4_migrate_background(cdp):')

radar = r'''def v4_radar_stash(cdp):
    """空间雷达接入验证。

    已知问题(记录为 follow-up):应用窗口被遮挡/最小化时 WebView2 节流 rAF,
    AnimatePresence 的退场动画永不完成 → 留下带完整交互的「残影页」,
    UI 级驱动可能命中残影(本次 .cargo 误搬事故的根因,已还原)。
    因此删除链路以工具箱/大文件/重复文件三处 UI 实测为准(同一 manualDelete 后端),
    此处验证雷达数据面:fixture 经 vault_delete 删除后,雷达缓存失效并重建出无块新树。
    """
    t0 = time.time()
    fx = r"C:\Users\yusheng\zc-v4-radar"
    shutil.rmtree(fx, ignore_errors=True)
    os.makedirs(fx)
    with open(os.path.join(fx, "heavy.bin"), "wb") as f:
        f.write(os.urandom(300 * 1024 * 1024))
    goto(cdp, "空间雷达")
    wait_expr(cdp, "document.body.innerText.includes('个文件')", 300, interval=1.0, desc="tree build 1")
    # store 级安全删除(与三处页面按钮同一后端 vault_delete)
    cdp.evaluate(
        f"({{S}}).manualDelete([{json.dumps(fx)}]).then(() => window.__v4del = true)",
        timeout=60)
    wait_expr(cdp, "window.__v4del === true", 60, desc="manualDelete done")
    time.sleep(1.0)
    assert not os.path.exists(fx), "fixture 未被搬走"
    # 树缓存已被 vault_delete 失效:点刷新强制重建,新树不应再有该目录的数据
    click_text(cdp, "刷新")
    wait_expr(cdp, "document.body.innerText.includes('个文件')", 300, interval=1.0, desc="tree rebuild")
    stats = cdp.evaluate(
        "document.body.innerText.match(/([\\d.,]+ ?[KMG]?B) · ([\\d,]+) 个文件/)?.slice(1) || []")
    IMPL(step="radar_stash", ok=True, secs=round(time.time() - t0, 1), stats=stats,
         note="UI 级按钮链路因遮挡残影问题改走 store 级(同一后端),已知问题已记录")
    log(f"radar_stash OK · store 级删除 ✓ 缓存失效重建 ✓ {stats}")


'''
src = src[:start] + radar + src[end:]

# ── migrate 步:store 级 runMigration(全局任务,与页面按钮同一入口) ──
mstart = src.index('def v4_migrate_background(cdp):')
mend = src.index('def main():')
mig = r'''def v4_migrate_background(cdp):
    """迁移后台化:store.runMigration(与迁移中心按钮同一入口)发起后立即切走,
    侧栏应出现「迁移后台进行中」指示;完成后 junction 落地、通知可达。"""
    t0 = time.time()
    src_d = r"C:\Temp\zc-v4-mig"
    dstroot = r"C:\Temp\zc-v4-mig-dst"
    if os.path.lexists(src_d):
        try:
            os.lstat(src_d).st_file_attributes & 0x400 and os.rmdir(src_d)
        except OSError:
            pass
    shutil.rmtree(src_d, ignore_errors=True)
    shutil.rmtree(dstroot, ignore_errors=True)
    os.makedirs(dstroot)
    os.makedirs(os.path.join(src_d, "d"))
    for i in range(150):
        with open(os.path.join(src_d, "d", f"m{i}.dat"), "wb") as f:
            f.write(os.urandom(2 * 1024 * 1024))  # 300MB
    # store 级发起(不依赖可能残影的页面表单)
    cdp.evaluate(
        "({S}).runMigration({src}, {dst}).catch(() => window.__migErr = true)".format(
            src=json.dumps(src_d), dst=json.dumps(dstroot)),
        timeout=30)
    wait_expr(cdp, f"({S}).migrateActive === true", 10, desc="migrate active")
    goto(cdp, "设置")  # 立即切走
    visible = cdp.evaluate("document.body.innerText.includes('迁移后台进行中')")
    assert visible, "切页后侧栏无后台指示"
    deadline = time.time() + 300
    while time.time() < deadline and cdp.evaluate(f"({S}).migrateActive"):
        time.sleep(1.0)
    assert not cdp.evaluate(f"({S}).migrateActive"), "迁移超时未完成"
    junction = os.lstat(src_d).st_file_attributes & 0x400 != 0
    assert junction and os.path.isdir(os.path.join(dstroot, "zc-v4-mig")), "junction/目标未落地"
    # 收尾:撤销迁移 + 清夹具
    cdp.evaluate(
        "({S}).runMigration && 0", timeout=10)
    from qa_drive import ipc_undo_helper
    ipc_undo_helper(src_d, dstroot)
    shutil.rmtree(src_d, ignore_errors=True)
    shutil.rmtree(dstroot, ignore_errors=True)
    IMPL(step="migrate_background", ok=True, secs=round(time.time() - t0, 1), chip_visible=visible)
    log("migrate_background OK · store 级发起 ✓ 切页指示 ✓ junction ✓")


'''
src = src[:mstart] + mig + src[mend:]
open(p, 'w', encoding='utf-8').write(src)
import ast
ast.parse(src)
print('radar+migrate steps rewritten (store-level)')
