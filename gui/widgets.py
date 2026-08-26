# -*- coding: utf-8 -*-
"""
ZDiskCleaner Pro - 自绘动画组件库
按钮悬停渐变 / 圆角卡片 / 环形仪表 / 流光进度条 / 数字滚动 / Toast / 滚动容器。
全部基于 Canvas 自绘 + after() 帧动画 (~60fps), 不依赖任何第三方库。
所有尺寸参数均为逻辑像素 (96dpi 基准), 内部经 px() 换算, 高 DPI 下比例一致。
"""

import tkinter as tk
import time
from .theme import (C, px, round_rect, lerp_color, ease_out_cubic, ease_out_back,
                    FONT_FAMILY)


# ---------------------------------------------------------------------------
# 按钮(Canvas 自绘, 悬停渐变 + 按下反馈)
# ---------------------------------------------------------------------------
class Btn(tk.Canvas):
    STYLES = {
        'primary': dict(fg='#ffffff', bg1='#4f8cff', bg2='#7c5cff', border=None),
        'ghost':   dict(fg=C['text'], bg1=C['card2'], bg2=C['card_hover'], border=C['border']),
        'soft':    dict(fg='#6da4ff', bg1=C['accent_soft'], bg2='#1e2c47', border=None),
        'danger':  dict(fg='#ffffff', bg1='#e5484d', bg2='#ff5c69', border=None),
        'green':   dict(fg='#06251a', bg1='#3dd68c', bg2='#5ce4a4', border=None),
    }

    def __init__(self, master, text, command=None, style='primary',
                 width=110, height=36, radius=None, font=None, padx=18, **kw):
        self.st = dict(self.STYLES[style])
        self.padx = padx
        try:
            bg = master['bg']
        except Exception:
            bg = C['card']
        super().__init__(master, width=px(width), height=px(height),
                         highlightthickness=0, bg=bg)
        self.hover_t = 0.0
        self.press_t = 0.0
        self.command = command
        self.text = text
        self.font = font or (FONT_FAMILY, 10, 'bold')
        self._radius = px(radius if radius is not None else height / 2)
        self.cw, self.ch = px(width), px(height)
        self._draw()
        self.bind('<Configure>', self._on_configure)
        self.bind('<Enter>', lambda e: self._animate('hover_t', 1.0))
        self.bind('<Leave>', lambda e: self._animate('hover_t', 0.0))
        self.bind('<Button-1>', self._on_press)
        self.bind('<ButtonRelease-1>', self._on_release)

    def _on_configure(self, e):
        # 被 fill='x' 拉伸时, 让按钮视觉跟随真实宽度
        if e.width > 1 and abs(e.width - self.cw) > 1:
            self.cw = e.width
            self._draw()

    def _animate(self, attr, target):
        start = getattr(self, attr)
        if abs(start - target) < 1e-6:
            return
        import time
        t0 = time.perf_counter()
        dur = 140

        def tick():
            t = min(1.0, (time.perf_counter() - t0) * 1000 / dur)
            v = start + (target - start) * ease_out_cubic(t)
            setattr(self, attr, v)
            self._draw()
            if t < 1.0:
                self.after(16, tick)

        tick()

    def _on_press(self, e):
        self.press_t = 1.0
        self._draw()
        self._ripple(e.x, e.y)

    def _ripple(self, x, y):
        """点击处扩散的圆环涟漪 (颜色由亮到背景渐隐)。"""
        try:
            bg_now = lerp_color(self.st['bg1'], self.st['bg2'], self.hover_t)
        except Exception:
            return
        max_r = max(self.cw, self.ch) * 0.75
        rid = self.create_oval(x - 2, y - 2, x + 2, y + 2,
                               outline='#ffffff', width=2)
        t0 = time.perf_counter()
        dur = 380

        def tick():
            t = (time.perf_counter() - t0) * 1000 / dur
            if t >= 1.0 or not self.winfo_exists():
                try:
                    self.delete(rid)
                except Exception:
                    pass
                return
            r = 2 + (max_r - 2) * ease_out_cubic(t)
            col = lerp_color('#ffffff', bg_now, ease_out_cubic(t))
            self.itemconfig(rid, outline=col,
                            width=max(0.5, 2.2 * (1 - t)))
            self.coords(rid, x - r, y - r, x + r, y + r)
            self.after(16, tick)

        tick()

    def _on_release(self, e):
        self.press_t = 0.0
        self._draw()
        if 0 <= e.x <= self.cw and 0 <= e.y <= self.ch and self.command:
            self.after(80, self.command)

    def _draw(self):
        self.delete('all')
        try:
            ww, wh = self.winfo_width(), self.winfo_height()
            if ww > 1 and abs(ww - self.cw) > 1:
                self.cw = ww
            if wh > 1 and abs(wh - self.ch) > 1:
                self.ch = wh
        except Exception:
            pass
        hover = self.hover_t
        press = self.press_t
        bg = lerp_color(self.st['bg1'], self.st['bg2'], hover)
        if press:
            bg = lerp_color(bg, '#000000', 0.18)
        r = self._radius * (1 - press * 0.06)
        round_rect(self, 1, 1, self.cw - 1, self.ch - 1, r, fill=bg, outline='')
        # 顶部高光, 增强立体感
        self.create_line(px(10), 2.5, self.cw - px(10), 2.5,
                         fill=lerp_color('#ffffff', bg, 0.65), width=1)
        off = 1 if press else 0
        self.create_text(self.cw // 2 + off, self.ch // 2 + off,
                         text=self.text, font=self.font, fill=self.st['fg'])

    def set_style(self, style):
        if style in self.STYLES:
            self.st = dict(self.STYLES[style])
            self._draw()


# ---------------------------------------------------------------------------
# 分段选择器
# ---------------------------------------------------------------------------
class Segmented(tk.Canvas):
    def __init__(self, master, options, command=None, width=200, height=32,
                 font=None, **kw):
        super().__init__(master, width=px(width), height=px(height),
                         highlightthickness=0, bg=C['card'])
        self.options = options
        self.command = command
        self.sel = 0
        self.pos = 0.0  # 动画位置
        self.font = font or (FONT_FAMILY, 9)
        self.cw, self.ch = px(width), px(height)
        self._seg_w = self.cw / len(options)
        self._draw()
        for i in range(len(options)):
            x1 = i * self._seg_w
            tag = f'seg{i}'
            self.create_rectangle(x1, 0, x1 + self._seg_w, self.ch,
                                  fill='', outline='', tags=tag)
            self.tag_bind(tag, '<Button-1>', lambda e, i=i: self.select(i))

    def select(self, i, fire=True):
        if i == self.sel and fire:
            return
        self.sel = i
        start = self.pos
        target = float(i)
        import time
        t0 = time.perf_counter()

        def tick():
            t = min(1.0, (time.perf_counter() - t0) * 1000 / 200)
            self.pos = start + (target - start) * ease_out_cubic(t)
            self._draw()
            if t < 1.0:
                self.after(16, tick)

        tick()
        if self.command and fire:
            self.command(i)

    def _draw(self):
        self.delete('all')
        r = self.ch // 2 - 1
        round_rect(self, 1, 1, self.cw - 1, self.ch - 1, r,
                   fill=C['card2'], outline=C['border'])
        x = self.pos * self._seg_w
        round_rect(self, x + 2, 2, x + self._seg_w - 2, self.ch - 2,
                   max(2, r - 1), fill=C['accent_soft'], outline='')
        for i, opt in enumerate(self.options):
            cx = i * self._seg_w + self._seg_w / 2
            active = i == self.sel
            self.create_text(cx, self.ch / 2, text=opt, font=self.font,
                             fill=C['accent'] if active else C['text_dim'])
        for i in range(1, len(self.options)):
            x = i * self._seg_w
            self.create_line(x, px(8), x, self.ch - px(8), fill=C['border'])


# ---------------------------------------------------------------------------
# 环形仪表 (动画填充)
# ---------------------------------------------------------------------------
class RingGauge(tk.Canvas):
    def __init__(self, master, size=110, thickness=11, value=0.0,
                 label='', sub='', color=None, track=None, font_big=None,
                 font_sub=None, animate=True):
        super().__init__(master, width=px(size), height=px(size),
                         highlightthickness=0,
                         bg=master['bg'] if hasattr(master, 'cget') else C['card'])
        self.size = px(size)
        self.th = px(thickness)
        self.value = value
        self.shown = 0.0
        self.label = label
        self.sub = sub
        self.color = color or C['accent']
        self.track = track or C['track']
        self.font_big = font_big or (FONT_FAMILY, 12, 'bold')
        self.font_sub = font_sub or (FONT_FAMILY, 7)
        self.animate = animate
        self._draw()

    def set_value(self, value, sub=None, color=None, animate=True):
        self.value = max(0.0, min(1.0, value))
        if sub is not None:
            self.sub = sub
        if color:
            self.color = color
        if not animate:
            self.shown = self.value
            self._draw()
            return
        start = self.shown
        import time
        t0 = time.perf_counter()

        def tick():
            t = min(1.0, (time.perf_counter() - t0) * 1000 / 800)
            self.shown = start + (self.value - start) * ease_out_cubic(t)
            self._draw()
            if t < 1.0:
                self.after(16, tick)

        tick()

    def _draw(self):
        self.delete('all')
        s = self.size
        pad = self.th / 2 + px(2)
        self.create_arc(pad, pad, s - pad, s - pad, start=90, extent=359.9,
                        style=tk.ARC, outline=self.track, width=self.th)
        ext = -359.9 * self.shown
        if self.shown > 0.003:
            self.create_arc(pad, pad, s - pad, s - pad, start=90, extent=ext,
                            style=tk.ARC, outline=self.color, width=self.th)
        cx, cy = s / 2, s / 2
        self.create_text(cx, cy - px(4), text=self.label, font=self.font_big,
                         fill=C['text'])
        if self.sub:
            self.create_text(cx, cy + px(13), text=self.sub, font=self.font_sub,
                             fill=C['text_dim'])


# ---------------------------------------------------------------------------
# 进度条: 确定进度 + 流光效果
# ---------------------------------------------------------------------------
class ProgressBar(tk.Canvas):
    def __init__(self, master, width=400, height=8, determinate=False):
        super().__init__(master, width=px(width), height=px(height),
                         highlightthickness=0,
                         bg=master['bg'] if hasattr(master, 'cget') else C['card'])
        self.cw, self.ch = px(width), px(height)
        self.determinate = determinate
        self.progress = 0.0
        self._shimmer_x = 0.0
        self._running = False
        self._draw()

    def set_progress(self, p):
        self.progress = max(0.0, min(1.0, p))
        self._draw()

    def start_shimmer(self):
        if not self._running:
            self._running = True
            self._shimmer_tick()

    def stop_shimmer(self):
        self._running = False
        self._draw()

    def _shimmer_tick(self):
        if not self._running:
            return
        self._shimmer_x = (self._shimmer_x + 0.014) % 1.4
        self._draw()
        self.after(16, self._shimmer_tick)

    def _draw(self):
        self.delete('all')
        try:
            ww = self.winfo_width()
            if ww > 1 and abs(ww - self.cw) > 1:
                self.cw = ww
        except Exception:
            pass
        r = self.ch / 2
        round_rect(self, 0, 0, self.cw, self.ch, r, fill=C['track'], outline='')
        if self.determinate and self.progress > 0.003:
            w = max(self.ch, self.cw * self.progress)
            round_rect(self, 0, 0, w, self.ch, r, fill=C['accent'], outline='')
            # 清理进行中: 已完成部分叠加流光, 呼吸感
            if self._running:
                sx = (self._shimmer_x * w) % max(1, w)
                grad_w = w * 0.25
                for i in range(10):
                    t = i / 10
                    xx = sx - grad_w + grad_w * t
                    if xx < 0 or xx > w:
                        continue
                    a = 0.35 * (1 - abs(t - 0.5) * 2)
                    if a <= 0.03:
                        continue
                    col = lerp_color(C['accent'], '#ffffff', a)
                    self.create_rectangle(xx, 2, min(w, xx + grad_w / 10 + 1),
                                          self.ch - 2, fill=col, outline='')
        elif self._running:
            base = self._shimmer_x * self.cw - self.cw * 0.3
            seg = self.cw * 0.3
            for i in range(12):
                t = i / 12
                x = base + seg * t
                a = 0.75 * (1 - abs(t - 0.5) * 2)
                if a <= 0.02:
                    continue
                x2 = min(self.cw, x + seg / 12 + 1)
                if x2 <= 0 or x >= self.cw:
                    continue  # 完全离屏的矩形不创建
                col = lerp_color(C['track'], C['accent'], a)
                self.create_rectangle(max(0, x), 1, x2,
                                      self.ch - 1, fill=col, outline='')


# ---------------------------------------------------------------------------
# 数字滚动标签
# ---------------------------------------------------------------------------
class CountUpLabel(tk.Label):
    def __init__(self, master, text='0', **kw):
        self._fmt = kw.pop('format', None)
        super().__init__(master, text=text, **kw)
        self._target_text = text
        self._cur = 0.0

    def set_value(self, value, animate=True, format=None):
        fmt = format or self._fmt
        target_text = fmt(value) if fmt else str(value)
        self._target_text = target_text
        if not animate:
            self.config(text=target_text)
            self._cur = float(value)
            return
        start = self._cur
        import time
        t0 = time.perf_counter()
        dur = 700

        def tick():
            t = min(1.0, (time.perf_counter() - t0) * 1000 / dur)
            v = start + (value - start) * ease_out_cubic(t)
            self._cur = v
            self.config(text=fmt(v) if fmt else f'{v:.0f}')
            if t < 1.0:
                self.after(16, tick)
            else:
                self.config(text=target_text)

        tick()


# ---------------------------------------------------------------------------
# Toast 通知
# ---------------------------------------------------------------------------
class Toast(tk.Toplevel):
    _instance = None

    def __init__(self, master, text, kind='success', duration=3200):
        if Toast._instance is not None:
            try:
                Toast._instance.destroy()
            except Exception:
                pass
        Toast._instance = self
        super().__init__(master)
        self.overrideredirect(True)
        self.attributes('-topmost', True)
        self.configure(bg=C['bg'])
        colors = {'success': C['green'], 'error': C['red'],
                  'info': C['accent'], 'warn': C['yellow']}
        icon = {'success': '✓', 'error': '✕', 'info': 'ⓘ', 'warn': '⚠'}[kind]
        col = colors.get(kind, C['accent'])

        import tkinter.font as tkfont
        H = px(52)
        f = tkfont.Font(family=FONT_FAMILY, size=10)
        text_w = f.measure(text)
        W = max(px(340), min(text_w + px(100), px(720)))
        cv = tk.Canvas(self, width=W, height=H, highlightthickness=0, bg=C['bg'])
        cv.pack(padx=1, pady=1)
        round_rect(cv, 0, 0, W, H, px(12), fill=C['card'], outline=C['border'])
        cv.create_oval(px(16), px(14), px(40), px(38),
                       fill=lerp_color(col, C['card'], 0.85), outline='')
        cv.create_text(px(28), px(26), text=icon,
                       font=(FONT_FAMILY, 11, 'bold'), fill=col)
        cv.create_text(px(54), px(26), anchor='w', text=text,
                       font=(FONT_FAMILY, 10), fill=C['text'])
        cv.create_line(0, 0, W, 0, fill=col, width=2)

        self._W = W
        self.update_idletasks()
        master.update_idletasks()
        mx = master.winfo_rootx() + master.winfo_width() // 2 - W // 2
        my = master.winfo_rooty() + px(20)
        self.geometry(f'+{mx}+{my - px(70)}')
        self._slide_in(my, duration)

    def _slide_in(self, target_y, duration):
        import time
        t0 = time.perf_counter()

        def tick():
            t = min(1.0, (time.perf_counter() - t0) * 1000 / 300)
            y = target_y - px(70) + px(70) * ease_out_back(t)
            mx = self.master.winfo_rootx() + self.master.winfo_width() // 2 - getattr(self, '_W', px(340)) // 2
            self.geometry(f'+{mx}+{int(y)}')
            if t < 1.0:
                self.after(16, tick)
            else:
                self.after(duration, self._slide_out, target_y)

        tick()

    def _slide_out(self, target_y):
        import time
        t0 = time.perf_counter()

        def tick():
            t = min(1.0, (time.perf_counter() - t0) * 1000 / 250)
            y = target_y - px(70) * ease_out_cubic(t)
            mx = self.master.winfo_rootx() + self.master.winfo_width() // 2 - getattr(self, '_W', px(340)) // 2
            self.geometry(f'+{mx}+{int(y)}')
            if t < 1.0:
                self.after(16, tick)
            else:
                self.destroy()
                if Toast._instance is self:
                    Toast._instance = None

        tick()


# ---------------------------------------------------------------------------
# 鼠标滚轮滚动容器
# ---------------------------------------------------------------------------
class ScrollFrame(tk.Frame):
    def __init__(self, master, bg=None, **kw):
        self._bg = bg or C['bg']
        super().__init__(master, bg=self._bg, **kw)
        self.canvas = tk.Canvas(self, bg=self._bg, highlightthickness=0, bd=0)
        self.vbar = tk.Canvas(self, width=px(6), bg=self._bg, highlightthickness=0)
        self.inner = tk.Frame(self.canvas, bg=self._bg)
        self._win = self.canvas.create_window((0, 0), window=self.inner,
                                              anchor='nw')
        self.canvas.pack(side='left', fill='both', expand=True)
        self.vbar.pack(side='right', fill='y')
        self.inner.bind('<Configure>', self._on_inner)
        self.canvas.bind('<Configure>', self._on_canvas)
        self._bind_mousewheel(self.inner)
        self._dragging = False
        self.vbar.bind('<Button-1>', self._on_bar_click)
        self.vbar.bind('<B1-Motion>', self._on_bar_drag)
        self._target = 0

    def _bind_mousewheel(self, widget):
        widget.bind('<Enter>', lambda e: self._wheel_bind())
        widget.bind('<Leave>', lambda e: self._wheel_unbind())
        for child in widget.winfo_children():
            self._bind_mousewheel(child)

    def _wheel_bind(self):
        self.canvas.bind_all('<MouseWheel>', self._on_wheel)

    def _wheel_unbind(self):
        try:
            self.canvas.unbind_all('<MouseWheel>')
        except Exception:
            pass

    def _on_wheel(self, e):
        step = -1 * int(e.delta / 40)
        self._target = self._smooth_clamp(self._target + step)
        self._animate_scroll()

    def _smooth_clamp(self, y):
        m = self._max_scroll()
        return max(0, min(m, y))

    def _max_scroll(self):
        return max(0, self.inner.winfo_reqheight() - self.canvas.winfo_height())

    def _animate_scroll(self):
        cur = self.canvas.canvasy(0)
        target = self._target
        if abs(cur - target) < 1:
            self.canvas.yview_moveto(target / max(1, self.inner.winfo_reqheight()))
            self._draw_thumb()
            return

        def tick():
            cur = self.canvas.canvasy(0)
            nxt = cur + (target - cur) * 0.25
            if abs(target - nxt) < 0.6:
                nxt = target
            self.canvas.yview_moveto(nxt / max(1, self.inner.winfo_reqheight()))
            self._draw_thumb()
            if abs(nxt - target) >= 0.6:
                self.after(16, tick)

        tick()

    def _on_inner(self, e):
        self.canvas.configure(scrollregion=(0, 0, e.width, e.height))
        self._target = self._smooth_clamp(self._target)
        self._draw_thumb()

    def _on_canvas(self, e):
        # 内层宽度始终等于视口宽度 (不随内容变宽, 防止行宽锁死)
        if e.width > 1:
            self.canvas.itemconfig(self._win, width=e.width)
        self._draw_thumb()

    # ---- 自绘滚动条 ----
    def _draw_thumb(self):
        self.vbar.delete('all')
        ch = self.vbar.winfo_height()
        ih = self.inner.winfo_reqheight()
        vh = self.canvas.winfo_height()
        if ih <= vh or ch < px(30):
            return
        thumb_h = max(px(36), ch * vh / ih)
        ratio = self.canvas.canvasy(0) / max(1, ih - vh)
        y = (ch - thumb_h) * ratio
        round_rect(self.vbar, 1, y, px(5), y + thumb_h, 2,
                   fill=C['scrollbar'], outline='')

    def _on_bar_click(self, e):
        ch = self.vbar.winfo_height()
        ih = self.inner.winfo_reqheight()
        vh = self.canvas.winfo_height()
        if ih <= vh:
            return
        thumb_h = max(px(36), ch * vh / ih)
        ratio = max(0, min(1, (e.y - thumb_h / 2) / (ch - thumb_h)))
        self._target = ratio * (ih - vh)
        self._animate_scroll()

    def _on_bar_drag(self, e):
        self._on_bar_click(e)


# ---------------------------------------------------------------------------
# 开关
# ---------------------------------------------------------------------------
class Toggle(tk.Canvas):
    def __init__(self, master, value=False, command=None, width=46, height=24):
        super().__init__(master, width=px(width), height=px(height),
                         highlightthickness=0,
                         bg=master['bg'] if hasattr(master, 'cget') else C['card'])
        self.value = value
        self.pos = 1.0 if value else 0.0
        self.command = command
        self.cw, self.ch = px(width), px(height)
        self._draw()
        self.bind('<Button-1>', self._toggle)

    def _toggle(self, e=None):
        self.set_value(not self.value)
        if self.command:
            self.command(self.value)

    def set_value(self, v, fire=False):
        self.value = v
        start = self.pos
        target = 1.0 if v else 0.0
        import time
        t0 = time.perf_counter()

        def tick():
            t = min(1.0, (time.perf_counter() - t0) * 1000 / 180)
            self.pos = start + (target - start) * ease_out_cubic(t)
            self._draw()
            if t < 1.0:
                self.after(16, tick)

        tick()

    def _draw(self):
        self.delete('all')
        bg = lerp_color(C['toggle_off'], C['accent'], self.pos)
        # 椭圆轨道
        self.create_oval(0, 0, self.cw, self.ch, fill=bg, outline='')
        knob_x = px(2) + (self.cw - self.ch) * self.pos
        d = self.ch - px(5)
        self.create_oval(knob_x + 0.5, px(2.5), knob_x + d + 0.5, d + px(2.5),
                         fill='#ffffff', outline='')


# ---------------------------------------------------------------------------
# 静态卡片
# ---------------------------------------------------------------------------
class Card(tk.Frame):
    def __init__(self, master, bg=None, padx=16, pady=14, **kw):
        bg = bg or C['card']
        super().__init__(master, bg=bg, **kw)
        self._border = tk.Frame(self, bg=C['border'], padx=1, pady=1)
        self._border.pack(fill='both', expand=True)
        self._inner = tk.Frame(self._border, bg=bg)
        self._inner.pack(fill='both', expand=True)

    @property
    def body(self):
        return self._inner


def chip(cv, x, y, text, fg, bg, font=None, padx=8):
    """在 canvas 上画一个小圆角标签。
    x 为徽章【左缘】(物理像素), 返回右缘 —— 调用方用 返回值+间距 排布下一个徽章。
    (旧实现把 x 当文字中心, 导致后一个徽章的背景左移盖住前一个徽章的文字)"""
    font = font or (FONT_FAMILY, 8)
    # 先测文本宽度
    probe = cv.create_text(0, 0, text=text, font=font, fill=fg)
    bbox = cv.bbox(probe)
    cv.delete(probe)
    tw = bbox[2] - bbox[0]
    h = px(18)
    x1 = x
    x2 = x + tw + px(padx) * 2
    round_rect(cv, x1, y - h / 2, x2, y + h / 2, h / 2 - 1, fill=bg, outline='')
    cv.create_text((x1 + x2) / 2, y, text=text, font=font, fill=fg)
    return x2
