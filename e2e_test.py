# -*- coding: utf-8 -*-
"""E2E 测试: GUI 内真实全量扫描 -> 进度更新 -> dry-run 预览清理。"""
import sys
import time

sys.path.insert(0, '.')
from gui.theme import enable_high_dpi, set_scale
enable_high_dpi()

import tkinter as tk
from gui.app import App

root = tk.Tk()
set_scale(root)
root.geometry('1200x780+40+30')
app = App(root)

app.switch_page('clean')
clean = app.pages['clean']
root.update()

print('=== 启动全量扫描 ===', flush=True)
clean.start_scan()

t0 = time.time()
last_status = ''

def poll():
    global last_status
    root.update()
    status = clean.status_label.cget('text')
    scanned = sum(1 for st in clean.rows.values() if st['scanned'])
    if status != last_status and int(time.time() - t0) % 10 < 1:
        pass
    if not app.scan_running:
        finish()
        return
    root.after(2000, poll)

def finish():
    elapsed = time.time() - t0
    scanned = sum(1 for st in clean.rows.values() if st['scanned'])
    total = sum(r.total_size for r in app.scan_results)
    print(f'扫描完成: {scanned}/46 规则, 可清理 {total/1024/1024/1024:.2f} GB, 耗时 {elapsed:.0f}s')
    print(f'状态栏: {clean.status_label.cget("text")}')
    print(f'选中统计: {clean.sel_label.cget("text")}')
    ok1 = scanned >= 44
    ok2 = total > 100 * 1024 * 1024
    ok3 = '预计释放' in clean.sel_label.cget('text')
    print(f'[{ "PASS" if ok1 else "FAIL"}] 规则全部扫描 ({scanned}/46)')
    print(f'[{ "PASS" if ok2 else "FAIL"}] 发现空间 > 100MB ({total/1024/1024:.0f}MB)')
    print(f'[{ "PASS" if ok3 else "FAIL"}] 选中统计更新')
    # dry-run 预览清理 (安全项)
    print('=== dry-run 预览清理 ===', flush=True)
    clean.dry_run.set(True)
    clean._auto_clean = False
    app.selected_rules = {n for n, st in clean.rows.items()
                          if st['selected'] and st['scanned']}
    clean.dry_run.set(True)
    clean.start_clean()

    def poll_clean():
        root.update()
        if app.clean_running:
            root.after(500, poll_clean)
            return
        st = clean.status_label.cget('text')
        print(f'预览结果: {st}')
        ok4 = '预览' in st or '将删除' in st
        print(f'[{ "PASS" if ok4 else "FAIL"}] dry-run 预览完成')
        print('E2E DONE', 'PASS' if (ok1 and ok2 and ok3 and ok4) else 'FAIL')
        root.destroy()

    root.after(1000, poll_clean)

root.after(2000, poll)
root.mainloop()
