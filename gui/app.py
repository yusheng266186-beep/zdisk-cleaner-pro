# -*- coding: utf-8 -*-
"""
ZDiskCleaner Pro - GUI 主壳
侧边栏导航 + 页面滑动切换 + 统一线程事件泵 (worker 线程通过 app.post 回 UI 线程)。
"""

import os
import sys
import time
import queue
import threading
import tkinter as tk

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from gui import theme
from gui.theme import (C, FONT_FAMILY, px, round_rect, ease_out_cubic,
                       lerp_color, apply_window_chrome, apply_window_icon)
from gui.widgets import Toast, Btn

APP_TITLE = 'ZDiskCleaner Pro'
APP_VERSION = 'v2.0.0'

NAV_ITEMS = [
    ('dashboard', '仪表盘', 'disk'),
    ('clean', '深度清理', 'clean'),
    ('move', '程序搬家', 'move'),
    ('analyze', '磁盘分析', 'analyze'),
    ('startup', '启动项', 'power'),
    ('report', '优化报告', 'doc'),
]

NAV_ICON = {key: icon for key, label, icon in NAV_ITEMS}


def draw_nav_icon(cv, kind, color, cx=12, cy=12, s=9):
    """在 canvas 上绘制极简几何图标, 返回 item ids。"""
    ids = []
    if kind == 'disk':  # 仪表盘: 四分格
        r = s
        for dx, dy, w, h in [(-r, -r, r * 0.82, r * 1.15), (0.12 * r, -r, r * 0.9, r * 0.6),
                             (-r, 0.12 * r, r * 0.6, r * 0.9), (0.12 * r, 0.35 * r, r * 0.9, r * 0.68)]:
            ids.append(round_rect(cv, cx + dx, cy + dy, cx + dx + w, cy + dy + h,
                                  2, fill=color, outline=''))
    elif kind == 'clean':  # 清理: 扫帚/刷子
        ids.append(cv.create_line(cx - 2, cy - 8, cx - 2, cy + 2, fill=color, width=2.4, capstyle='round'))
        ids.append(round_rect(cv, cx - 7, cy + 2, cx + 4, cy + 5, 2, fill=color, outline=''))
        for i in range(4):
            x = cx - 6.5 + i * 3.4
            ids.append(cv.create_line(x, cy + 5, x - 1.5 + i, cy + 9, fill=color, width=1.6, capstyle='round'))
    elif kind == 'move':  # 搬家: 盒子+箭头
        ids.append(round_rect(cv, cx - 9, cy - 6, cx - 1, cy + 6, 2, fill=color, outline=''))
        ids.append(cv.create_line(cx + 1, cy, cx + 8, cy, fill=color, width=2, capstyle='round'))
        ids.append(cv.create_line(cx + 5, cy - 3, cx + 8, cy, fill=color, width=2, capstyle='round'))
        ids.append(cv.create_line(cx + 5, cy + 3, cx + 8, cy, fill=color, width=2, capstyle='round'))
    elif kind == 'analyze':  # 放大镜
        ids.append(cv.create_oval(cx - 8, cy - 8, cx + 3, cy + 3, outline=color, width=2))
        ids.append(cv.create_line(cx + 1, cy + 1, cx + 8, cy + 8, fill=color, width=2.4, capstyle='round'))
    elif kind == 'power':  # 电源符号
        ids.append(cv.create_line(cx, cy - 8, cx, cy - 1, fill=color, width=2.4, capstyle='round'))
        ids.append(cv.create_arc(cx - 7, cy - 7, cx + 7, cy + 7, start=310, extent=260,
                                 style=tk.ARC, outline=color, width=2.2))
    elif kind == 'doc':  # 文档
        ids.append(round_rect(cv, cx - 6, cy - 9, cx + 6, cy + 9, 2, outline=color, width=2))
        for dy in (-3, 0, 3):
            ids.append(cv.create_line(cx - 3, cy + dy, cx + 3, cy + dy, fill=color, width=1.6, capstyle='round'))
    return ids


class App:
    """应用壳: 窗口 + 侧栏 + 页面容器。"""

    def __init__(self, root: tk.Tk):
        self.root = root
        self.q = queue.Queue()
        self.pages = {}
        self.current_page = None
        self.page_frames = {}

        # 共享状态
        self.scan_results = []
        self.selected_rules = set()
        self.scan_running = False
        self.clean_running = False
        self.scanner = None
        self.settings = {}

        self._build_window()
        self._build_layout()
        self.root.after(30, self._poll)
        self.root.protocol('WM_DELETE_WINDOW', self._on_close)
        self.switch_page('dashboard')

    # ------------------------------------------------------------------
    def _build_window(self):
        # 逻辑尺寸换算为物理像素, 保证高 DPI 下 UI 比例一致
        sw, sh = self.root.winfo_screenwidth(), self.root.winfo_screenheight()
        w = min(px(1200), sw - px(40))
        h = min(px(780), sh - px(60))
        sx = max(0, sw // 2 - w // 2)
        sy = max(0, sh // 2 - h // 2 - px(10))
        self.root.geometry(f'{w}x{h}+{sx}+{sy}')
        self.root.minsize(min(px(1080), sw - px(40)), min(px(680), sh - px(40)))
        self.root.title(APP_TITLE)
        self.root.configure(bg=C['bg'])
        # 深色标题栏 + 自定义图标 (需在窗口映射后调用)
        self.root.after(10, lambda: apply_window_chrome(self.root))
        self.root.after(10, lambda: apply_window_icon(self.root))
        try:
            self.root.attributes('-topmost', True)
            self.root.after(200, lambda: self.root.attributes('-topmost', False))
        except Exception:
            pass

    # ------------------------------------------------------------------
    def _build_layout(self):
        # 侧栏
        self.sidebar = tk.Frame(self.root, bg=C['sidebar'], width=px(212))
        self.sidebar.pack(side='left', fill='y')
        self.sidebar.pack_propagate(False)
        self._build_sidebar()

        # 内容区
        self.content = tk.Frame(self.root, bg=C['bg'])
        self.content.pack(side='right', fill='both', expand=True)
        self.page_host = tk.Frame(self.content, bg=C['bg'])
        self.page_host.pack(fill='both', expand=True)

    def _build_sidebar(self):
        sb = self.sidebar
        # Logo
        logo = tk.Frame(sb, bg=C['sidebar'])
        logo.pack(fill='x', padx=18, pady=(22, 6))
        lcv = tk.Canvas(logo, width=px(34), height=px(34), bg=C['sidebar'], highlightthickness=0)
        lcv.grid(row=0, column=0, padx=(0, 10))
        round_rect(lcv, 1, 1, px(33), px(33), px(9), fill=C['accent_soft'], outline='')
        lcv.create_arc(px(6), px(6), px(28), px(28), start=90, extent=-250, style=tk.ARC,
                       outline=C['accent'], width=3)
        lcv.create_oval(px(15), px(15), px(21), px(21), fill=C['accent2'], outline='')
        tk.Label(logo, text='ZDiskCleaner', font=(FONT_FAMILY, 12, 'bold'),
                 bg=C['sidebar'], fg=C['text']).grid(row=0, column=1, sticky='w')
        tk.Label(logo, text='PRO · 深度磁盘清理', font=(FONT_FAMILY, 8),
                 bg=C['sidebar'], fg=C['text_faint']).grid(row=1, column=1, sticky='w')

        # 导航指示条 (动画)
        self._nav_indicator = tk.Canvas(sb, width=px(4), bg=C['sidebar'],
                                        highlightthickness=0)
        self._nav_indicator.place(x=0, y=120, relheight=0.0, height=42)

        self._nav_items = {}
        nav_wrap = tk.Frame(sb, bg=C['sidebar'])
        nav_wrap.pack(fill='x', padx=12, pady=(14, 0))
        for key, label, icon in NAV_ITEMS:
            self._make_nav_item(nav_wrap, key, label, icon)

        # 底部版本信息
        footer = tk.Frame(sb, bg=C['sidebar'])
        footer.pack(side='bottom', fill='x', padx=18, pady=14)
        tk.Label(footer, text=f'{APP_VERSION} · by ZCode', font=(FONT_FAMILY, 8),
                 bg=C['sidebar'], fg=C['text_faint']).pack(anchor='w')
        tk.Label(footer, text='数据安全第一 · 默认回收站', font=(FONT_FAMILY, 8),
                 bg=C['sidebar'], fg=C['text_faint']).pack(anchor='w')

    def _make_nav_item(self, parent, key, label, icon):
        item = tk.Frame(parent, bg=C['sidebar'], cursor='hand2')
        item.pack(fill='x', pady=1)
        cv = tk.Canvas(item, width=px(26), height=px(26), bg=C['sidebar'],
                       highlightthickness=0)
        cv.pack(side='left', padx=(px(12), px(12)), pady=px(8))
        draw_nav_icon(cv, icon, C['text_faint'])
        lbl = tk.Label(item, text=label, font=(FONT_FAMILY, 10),
                       bg=C['sidebar'], fg=C['text_dim'])
        lbl.pack(side='left')
        self._nav_items[key] = dict(frame=item, cv=cv, lbl=lbl,
                                    hover=0.0, active=False)

        for w in (item, cv, lbl):
            w.bind('<Button-1>', lambda e, k=key: self.switch_page(k))
            w.bind('<Enter>', lambda e, k=key: self._nav_hover(k, 1.0))
            w.bind('<Leave>', lambda e, k=key: self._nav_hover(k, 0.0))

    def _nav_hover(self, key, target):
        info = self._nav_items[key]
        start = info['hover']
        if info['active'] and target == 1.0:
            target = 1.0
        if abs(start - target) < 1e-6:
            return
        t0 = time.perf_counter()

        def tick():
            t = min(1.0, (time.perf_counter() - t0) * 1000 / 130)
            v = start + (target - start) * ease_out_cubic(t)
            info['hover'] = v
            self._nav_paint(key)
            if t < 1.0:
                self.root.after(16, tick)

        tick()

    def _nav_paint(self, key):
        info = self._nav_items[key]
        h = info['hover']
        if info['active']:
            bg = C['sidebar_sel']
            fg = C['text']
            icon_c = C['accent']
        else:
            bg = lerp_color(C['sidebar'], C['card_hover'], h * 0.6)
            fg = lerp_color(C['text_dim'], C['text'], h)
            icon_c = lerp_color(C['text_faint'], C['text_dim'], h)
        info['frame'].config(bg=bg)
        info['cv'].config(bg=bg)
        info['cv'].delete('all')
        draw_nav_icon(info['cv'], NAV_ICON[key], icon_c)
        info['lbl'].config(bg=bg, fg=fg)

    def _move_nav_indicator(self, key):
        info = self._nav_items[key]
        self.root.update_idletasks()
        sb = self.sidebar
        y = info['frame'].winfo_rooty() - sb.winfo_rooty()
        h = info['frame'].winfo_height()
        cv = self._nav_indicator
        try:
            cur_y = float(cv.place_info().get('y') or y)
        except Exception:
            cur_y = y
        t0 = time.perf_counter()

        def tick():
            t = min(1.0, (time.perf_counter() - t0) * 1000 / 260)
            ny = cur_y + (y - cur_y) * ease_out_cubic(t)
            cv.place_configure(x=0, y=ny, height=h)
            cv.delete('all')
            cv.create_rectangle(0, 10, 4, h - 10, fill=C['accent'], outline='')
            if t < 1.0:
                self.root.after(16, tick)

        tick()

    # ------------------------------------------------------------------
    # 页面路由
    # ------------------------------------------------------------------
    def switch_page(self, key, **ctx):
        if self.current_page == key and not ctx:
            return
        if key not in self.pages:
            self._create_page(key)
        # 导航状态
        for k, info in self._nav_items.items():
            info['active'] = (k == key)
            self._nav_paint(k)
        self._move_nav_indicator(key)

        new_frame = self.page_frames[key]
        old_frame = self.page_frames.get(self.current_page) if self.current_page else None
        self.current_page = key

        if hasattr(self.pages[key], 'on_show'):
            self.pages[key].on_show(**ctx)

        # 滑动切换动画
        host = self.page_host
        if old_frame is not None:
            old_frame.place_forget()
        new_frame.place(in_=host, x=0, y=0, relwidth=1, relheight=1)
        t0 = time.perf_counter()
        w = host.winfo_width() or 900

        def tick():
            t = min(1.0, (time.perf_counter() - t0) * 1000 / 220)
            x = (1 - ease_out_cubic(t)) * 46
            new_frame.place_configure(x=x, y=0, relwidth=1, relheight=1)
            if t < 1.0:
                self.root.after(16, tick)
            else:
                new_frame.place_configure(x=0)

        tick()

    def _create_page(self, key):
        from gui.pages_a import DashboardPage, CleanPage
        from gui.pages_b import MovePage, AnalyzePage, StartupPage, ReportPage
        cls = {'dashboard': DashboardPage, 'clean': CleanPage,
               'move': MovePage, 'analyze': AnalyzePage,
               'startup': StartupPage, 'report': ReportPage}[key]
        frame = tk.Frame(self.page_host, bg=C['bg'])
        self.page_frames[key] = frame
        self.pages[key] = cls(self, frame)

    # ------------------------------------------------------------------
    # 线程事件泵: worker -> UI
    # ------------------------------------------------------------------
    def post(self, fn):
        self.q.put(fn)

    def _poll(self):
        try:
            while True:
                fn = self.q.get_nowait()
                try:
                    fn()
                except Exception:
                    pass
        except queue.Empty:
            pass
        self.root.after(30, self._poll)

    def run_worker(self, target):
        t = threading.Thread(target=target, daemon=True)
        t.start()
        return t

    def toast(self, text, kind='success'):
        Toast(self.root, text, kind)

    def _on_close(self):
        try:
            if self.scanner:
                self.scanner.stop()
        except Exception:
            pass
        self.root.destroy()

    # ------------------------------------------------------------------
    @staticmethod
    def page_header(parent, title, subtitle=''):
        head = tk.Frame(parent, bg=C['bg'])
        head.pack(fill='x', padx=28, pady=(24, 4))
        tk.Label(head, text=title, font=(FONT_FAMILY, 17, 'bold'),
                 bg=C['bg'], fg=C['text']).pack(anchor='w')
        if subtitle:
            tk.Label(head, text=subtitle, font=(FONT_FAMILY, 10),
                     bg=C['bg'], fg=C['text_dim']).pack(anchor='w', pady=(2, 0))
        return head


def main():
    theme.enable_high_dpi()
    root = tk.Tk()
    theme.set_scale(root)
    app = App(root)
    root.mainloop()
