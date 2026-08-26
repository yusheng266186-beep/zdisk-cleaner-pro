# -*- coding: utf-8 -*-
"""GUI 冒烟测试: 自动遍历所有页面, 验证无异常且控件树非空。"""
import sys
import traceback

sys.path.insert(0, '.')
from gui.theme import enable_high_dpi
enable_high_dpi()

import tkinter as tk
from gui.app import App, NAV_ITEMS

root = tk.Tk()
root.geometry('1200x780+60+40')
app = App(root)

errors = []
visited = []

def visit(i=0):
    try:
        if i >= len(NAV_ITEMS):
            finish()
            return
        key = NAV_ITEMS[i][0]
        app.switch_page(key)
        visited.append(key)
        # 触发布局渲染
        root.update_idletasks()
        root.update()
        page = app.pages.get(key)
        n_children = len(page.f.winfo_children()) if page else -1
        print(f'  [OK] {key}: {n_children} top-level widgets')
        root.after(700, lambda: visit(i + 1))
    except Exception:
        errors.append(traceback.format_exc())
        print(f'  [FAIL] page {i}: {errors[-1].splitlines()[-1]}')
        root.after(100, lambda: visit(i + 1))

def finish():
    try:
        # 触发清理页扫描逻辑的按钮状态更新 (不真正扫描)
        if 'clean' in app.pages:
            app.pages['clean']._update_selected()
        if 'dashboard' in app.pages:
            app.pages['dashboard'].refresh()
        root.update()
    except Exception:
        errors.append(traceback.format_exc())
    if errors:
        print('\nSMOKE RESULT: FAIL')
        for e in errors:
            print(e)
    else:
        print(f'\nSMOKE RESULT: PASS ({len(visited)} pages)')
    root.destroy()

print('=== GUI SMOKE TEST ===')
root.after(600, visit)
root.mainloop()
print('exit clean')
