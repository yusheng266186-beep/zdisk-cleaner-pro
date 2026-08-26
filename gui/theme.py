# -*- coding: utf-8 -*-
"""
ZDiskCleaner Pro - 主题与全局样式
深色高级感设计 + 高 DPI 适配 (原版未做 DPI 声明, 高分屏下整体模糊发虚)。
"""

import ctypes
import tkinter as tk
import tkinter.font as tkfont


# ---------------------------------------------------------------------------
# 配色
# ---------------------------------------------------------------------------
C = {
    'bg':          '#0d1017',   # 主背景
    'sidebar':     '#11141c',   # 侧栏
    'card':        '#161a24',   # 卡片
    'card2':       '#1a1f2b',   # 次级面
    'card_hover':  '#1d2331',   # 悬停
    'border':      '#232937',   # 边框
    'border_soft': '#1c2230',
    'text':        '#edf0f6',   # 主文字 (高对比)
    'text_dim':    '#b7c1d3',   # 次级文字 (高对比)
    'text_faint':  '#98a2b8',   # 弱文字 (高对比)
    'accent':      '#4f8cff',   # 主色
    'accent2':     '#7c5cff',   # 渐变副色
    'accent_soft': '#182436',
    'green':       '#3dd68c',
    'yellow':      '#ffb454',
    'red':         '#ff5c69',
    'sidebar_sel': '#1d2536',   # 侧栏选中
}

# 近似透明混合的实体色
C['green_soft'] = '#12291f'
C['yellow_soft'] = '#2b2314'
C['red_soft'] = '#2b181c'
C['track'] = '#232a3a'        # 进度条/仪表轨道
C['toggle_off'] = '#2a3143'   # 开关未启用
C['scrollbar'] = '#2c3447'    # 滚动条

# 近似透明混合的实体色
C['green_soft'] = '#e2f5ea'
C['yellow_soft'] = '#fdf0dd'
C['red_soft'] = '#fdeaea'
C['track'] = '#e3e9f2'        # 进度条/仪表轨道
C['toggle_off'] = '#cdd5e3'   # 开关未启用
C['scrollbar'] = '#c3ccdb'    # 滚动条

# 近似透明混合的实体色
C['green_soft'] = '#12291f'
C['yellow_soft'] = '#2b2314'
C['red_soft'] = '#2b181c'

FONT_FAMILY = 'Microsoft YaHei UI'
MONO_FAMILY = 'Consolas'

# ---------------------------------------------------------------------------
# 布局缩放: 所有像素尺寸以 96dpi 逻辑像素书写, 经 px() 换算为物理像素。
# 这保证 100%/125%/150%/200% 缩放下 UI 比例一致且文字清晰不模糊。
# ---------------------------------------------------------------------------
_S = 1.0


def set_scale(root):
    global _S
    try:
        _S = root.winfo_fpixels('1i') / 96.0
    except Exception:
        _S = 1.0
    if _S <= 0.05:
        _S = 1.0
    return _S


def px(n):
    """逻辑像素 -> 物理像素。"""
    return int(round(n * _S))


def scale():
    return _S


def fonts(scale=1.0):
    """返回常用字体 (size 为磅数, 自动跟随系统缩放)。"""
    return {
        'title':   (FONT_FAMILY, 15, 'bold'),
        'h1':      (FONT_FAMILY, 13, 'bold'),
        'h2':      (FONT_FAMILY, 11, 'bold'),
        'body':    (FONT_FAMILY, 10),
        'body_b':  (FONT_FAMILY, 10, 'bold'),
        'small':   (FONT_FAMILY, 9),
        'small_b': (FONT_FAMILY, 9, 'bold'),
        'tiny':    (FONT_FAMILY, 8),
        'big_num': (FONT_FAMILY, 20, 'bold'),
        'mid_num': (FONT_FAMILY, 15, 'bold'),
        'mono':    (MONO_FAMILY, 9),
    }


def hex_to_rgb(h):
    h = h.lstrip('#')
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


def rgb_to_hex(rgb):
    return '#%02x%02x%02x' % tuple(max(0, min(255, int(x))) for x in rgb)


def lerp_color(c1, c2, t):
    """两色插值 t∈[0,1]。"""
    a, b = hex_to_rgb(c1), hex_to_rgb(c2)
    return rgb_to_hex(tuple(a[i] + (b[i] - a[i]) * t for i in range(3)))


def mix_alpha(fg, bg, alpha):
    """前景色按透明度混合到背景色上。"""
    return lerp_color(bg, fg, alpha)


def enable_high_dpi():
    """进程级 DPI 感知, 修复高分屏模糊。"""
    try:
        # Per-Monitor V2 (Win10 1703+)
        ctypes.windll.user32.SetProcessDpiAwarenessContext(
            ctypes.c_void_p(-4))
        return
    except Exception:
        pass
    try:
        ctypes.windll.shcore.SetProcessDpiAwareness(2)
        return
    except Exception:
        pass
    try:
        ctypes.windll.user32.SetProcessDPIAware()
    except Exception:
        pass


def get_scale(root):
    """布局缩放系数 (96dpi 为 1.0)。"""
    try:
        return root.winfo_fpixels('1i') / 96.0
    except Exception:
        return 1.0


# ---------------------------------------------------------------------------
# Canvas 圆角矩形
# ---------------------------------------------------------------------------
def round_rect(cv, x1, y1, x2, y2, r=10, **kw):
    """在 Canvas 上绘制平滑圆角矩形, 返回 item id。"""
    points = [x1 + r, y1, x2 - r, y1, x2, y1, x2, y1 + r,
              x2, y2 - r, x2, y2, x2 - r, y2, x1 + r, y2,
              x1, y2, x1, y2 - r, x1, y1 + r, x1, y1]
    return cv.create_polygon(points, smooth=True, **kw)


# ---------------------------------------------------------------------------
# 窗口外观: 深色标题栏 + 标题栏配色 (Win10/11), 告别默认的亮色"Python 框"
# ---------------------------------------------------------------------------
def apply_window_chrome(root, bg_hex=C['sidebar'], fg_hex=C['text']):
    """标题栏与窗口主题一致: 浅色主题下关闭深色标题栏, 并定制底色/文字色。"""
    import ctypes
    try:
        hwnd = ctypes.windll.user32.GetParent(root.winfo_id())
        if not hwnd:
            hwnd = root.winfo_id()
        # 深色主题: 启用沉浸式深色标题栏
        val = ctypes.c_int(1)
        ok = False
        for attr in (20, 19):
            if ctypes.windll.dwmapi.DwmSetWindowAttribute(
                    hwnd, attr, ctypes.byref(val), 4) == 0:
                ok = True
                break
        # Win11: 标题栏颜色 / 文字颜色 (COLORREF = 0x00BBGGRR)
        def colorref(hx):
            h = hx.lstrip('#')
            r, g, b = int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16)
            return ctypes.c_uint((b << 16) | (g << 8) | r)
        ctypes.windll.dwmapi.DwmSetWindowAttribute(
            hwnd, 35, ctypes.byref(colorref(bg_hex)), 4)   # CAPTION_COLOR
        ctypes.windll.dwmapi.DwmSetWindowAttribute(
            hwnd, 36, ctypes.byref(colorref(fg_hex)), 4)   # TEXT_COLOR
        return ok
    except Exception:
        return False


def app_icon_path():
    """定位 app_icon.ico (开发环境 / PyInstaller 冻结环境)。"""
    import os, sys
    if getattr(sys, 'frozen', False):
        base = getattr(sys, '_MEIPASS', os.path.dirname(sys.executable))
    else:
        base = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    return os.path.join(base, 'app_icon.ico')


def apply_window_icon(root):
    import os
    try:
        p = app_icon_path()
        if os.path.exists(p):
            root.iconbitmap(p)
            return True
    except Exception:
        pass
    return False


def ease_out_cubic(t):
    return 1 - (1 - t) ** 3


def ease_out_back(t):
    c1, c3 = 1.70158, 2.70158
    return 1 + c3 * (t - 1) ** 3 + c1 * (t - 1) ** 2
