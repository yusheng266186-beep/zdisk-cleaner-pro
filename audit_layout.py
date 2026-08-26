# -*- coding: utf-8 -*-
"""遮挡审计: 遍历所有页面, 检查 (1) canvas 项是否超出画布 (2) 子控件是否超出父容器。
在多个窗口宽度下运行。"""
import sys

sys.path.insert(0, '.')
from gui.theme import enable_high_dpi, set_scale, px
enable_high_dpi()

import tkinter as tk
from gui.app import App

issues = []


def audit_canvas(cv, name):
    """canvas 项不应超出画布可视范围 (左右方向)。"""
    w = cv.winfo_width()
    h = cv.winfo_height()
    if w < 10:
        return
    for item in cv.find_all():
        try:
            x1, y1, x2, y2 = cv.bbox(item)
        except Exception:
            continue
        if x1 < -3 or x2 > w + 3:
            tags = cv.gettags(item)
            issues.append(f'{name}: item {item} {tags[:2]} x[{x1:.0f},{x2:.0f}] 超出画布宽 {w}')


def audit_tree(widget, path=''):
    """Frame 内子控件不应超出父宽度 (容差 2px)。"""
    for child in widget.winfo_children():
        try:
            cw = child.winfo_width()
            cx1 = child.winfo_x()
        except Exception:
            continue
        pw = widget.winfo_width()
        if pw > 10 and cw > 10 and cx1 + cw > pw + 2:
            issues.append(f'{path}: child 超宽 x={cx1} w={cw} > 父宽 {pw}')
        audit_tree(child, path + '/' + child.winfo_class())


def run(width, height):
    global root, app
    root.geometry(f'{px(width)}x{px(height)}+30+20')
    root.update_idletasks()
    root.update()
    # 给 WM 重排 + Configure 回调一拍时间, 避免瞬态误报
    root.after(120, lambda: None)
    root.update()
    root.update_idletasks()
    root.update()
    for key in ('dashboard', 'clean', 'move', 'analyze', 'startup', 'report'):
        app.switch_page(key)
        root.update_idletasks()
        root.update()
        page = app.pages[key]
        # canvas 审计
        def walk(w, path):
            if isinstance(w, tk.Canvas):
                audit_canvas(w, f'{key}{path}')
            for c in w.winfo_children():
                walk(c, path + f'/{c.winfo_class()}')
        walk(page.f, '')
    print(f'  窗口 {width}x{height}: {"发现 " + str(len(issues)) + " 个问题" if issues else "OK"}')


root = tk.Tk()
set_scale(root)
app = App(root)
# 注入模拟数据让列表都有内容
from core.scanner import ScanResult, FileItem
from core import config
mock = []
for rule in config.CLEAN_RULES:
    r = ScanResult(rule=rule)
    r.add(FileItem(path='C:/x/' + rule['name'], size=12345678, is_dir=True))
    mock.append(r)
app.scan_results = mock
app.switch_page('clean')
clean = app.pages['clean']
for res in mock:
    st = clean.rows[res.rule['name']]
    st.update(size=res.total_size, count=res.file_count, scanned=True)
    clean._render_row(res.rule['name'])
app.switch_page('analyze')
an = app.pages['analyze']
long_path = 'C:/Users/yusheng/AppData/Local/' + 'dir/' * 10 + 'longfile_name.bin'
an.data = {
    'large': [dict(path=long_path, size=5*1024**3, mtime=1756000000.0, ext='.bin')] * 5,
    'dups': [[dict(path=long_path, size=1024**3, hash='x'), dict(path='D:/d.bin', size=1024**3, hash='x')]],
    'old': [dict(path=long_path, size=100*1024**2, mtime=1500000000.0, days_old=3000)],
    'dirs': [dict(path='C:/Users', size=100*1024**3)] * 5,
}

print('=== 遮挡审计 ===')
for w, h in ((1080, 680), (1200, 780), (1500, 900)):
    issues.clear()
    run(w, h)
    if issues:
        for i in issues[:15]:
            print('   -', i)

if not issues:
    print('\nAUDIT RESULT: PASS')
else:
    print(f'\nAUDIT RESULT: {len(issues)} issues')
root.destroy()
