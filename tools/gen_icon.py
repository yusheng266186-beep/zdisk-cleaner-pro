# -*- coding: utf-8 -*-
"""
生成应用图标 app_icon.ico (纯 Python, 无第三方依赖)
设计: 深色圆角方块底 + 蓝紫渐变圆环(磁盘意象) + 绿色扫除光点
输出: 经典 BMP 帧 ICO (16~256 全尺寸, 兼容 tkinter/资源管理器)
"""
import struct
import math
import os

SS = 4  # 超采样倍数


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


BLUE = (47, 107, 255)
PURPLE = (106, 77, 246)
GREEN = (15, 157, 88)
BG_TOP = (255, 255, 255)
BG_BOT = (219, 231, 250)
BORDER = (185, 200, 228)


def render(size):
    """渲染一帧, 返回 RGBA 行列表。"""
    S = size * SS
    half = S / 2
    # 几何参数 (逻辑比例)
    corner_r = S * 0.225          # 圆角半径
    ring_r = S * 0.30             # 圆环半径
    ring_th = S * 0.105           # 圆环粗细
    dot_r = S * 0.055             # 绿色扫除点半径
    # 扫除光点角度位置 (右上缺口处)
    dot_ang = -55 * math.pi / 180

    px = [[(0, 0, 0, 0)] * S for _ in range(S)]

    def smooth(edge0, edge1, x):
        t = max(0.0, min(1.0, (x - edge0) / (edge1 - edge0)))
        return t * t * (3 - 2 * t)

    for y in range(S):
        for x in range(S):
            fx, fy = x + 0.5, y + 0.5
            # ---- 圆角方块背景 (SDF) ----
            qx = abs(fx - half) - (half - corner_r)
            qy = abs(fy - half) - (half - corner_r)
            d_out = math.sqrt(max(qx, 0) ** 2 + max(qy, 0) ** 2)
            sdf = min(max(qx, qy), 0) + d_out - corner_r
            a_bg = smooth(1.2 * SS, -1.2 * SS, sdf)
            if a_bg <= 0:
                continue
            # 背景垂直渐变
            g = lerp(BG_TOP, BG_BOT, fy / S)
            r_, g_, b_ = g
            # 圆角描边 (浅色图标在白底桌面上需要轮廓)
            edge = smooth(1.6 * SS, 0.0, abs(sdf))
            border_mix = lerp(g, BORDER, edge * 0.9)
            r_, g_, b_ = border_mix

            # ---- 圆环: 渐变弧 300°, 缺口朝右上 ----
            dx, dy = fx - half, fy - half
            dist = math.sqrt(dx * dx + dy * dy)
            ang = math.atan2(-dy, dx)  # 屏幕坐标翻转
            # 弧从 -35° 到 245° (即缺口在 -55°~-25° 区域)
            a0, a1 = -0.62, 4.01  # rad: 约 -35.5° 到 230°
            in_arc = a0 <= ((ang - a0) % (2 * math.pi)) + a0
            ang_norm = (ang - a0) % (2 * math.pi)
            arc_ok = ang_norm <= (a1 - a0)
            ring_d = abs(dist - ring_r) - ring_th / 2
            a_ring = smooth(1.1 * SS, -1.1 * SS, ring_d) if arc_ok else 0.0
            if a_ring > 0:
                t = ang_norm / (a1 - a0)
                cr, cg, cb = lerp(BLUE, PURPLE, t)
                mix = a_ring
                r_ = r_ * (1 - mix) + cr * mix
                g_ = g_ * (1 - mix) + cg * mix
                b_ = b_ * (1 - mix) + cb * mix

            # ---- 缺口处的绿色扫除点 ----
            dcx = half + ring_r * math.cos(dot_ang)
            dcy = half - ring_r * math.sin(dot_ang)
            dd = math.sqrt((fx - dcx) ** 2 + (fy - dcy) ** 2) - dot_r
            a_dot = smooth(1.1 * SS, -1.1 * SS, dd)
            if a_dot > 0:
                r_ = r_ * (1 - a_dot) + GREEN[0] * a_dot
                g_ = g_ * (1 - a_dot) + GREEN[1] * a_dot
                b_ = b_ * (1 - a_dot) + GREEN[2] * a_dot

            # ---- 中心扫帚意象: 三条渐隐速度线 ----
            for k, off in enumerate((-0.10, 0.0, 0.10)):
                ly = half + S * off
                lx0 = half - S * 0.085
                lx1 = half + S * 0.085 - abs(off) * S * 0.5
                if ly - S*0.012 < fy < ly + S*0.012 and lx0 < fx < lx1:
                    fall = 1.0 - abs(fx - half) / (S * 0.085) * 0.55
                    m = 0.85 * max(0.0, fall) * (1 - a_ring)
                    r_ = r_ * (1 - m) + 236 * m
                    g_ = g_ * (1 - m) + 240 * m
                    b_ = b_ * (1 - m) + 248 * m

            px[y][x] = (int(max(0, min(255, r_))),
                        int(max(0, min(255, g_))),
                        int(max(0, min(255, b_))), int(a_bg * 255))

    # 降采样
    out = []
    for y in range(size):
        row = []
        for x in range(size):
            rs = gs = bs = as_ = 0
            for sy in range(SS):
                for sx in range(SS):
                    c = px[y * SS + sy][x * SS + sx]
                    rs += c[0]; gs += c[1]; bs += c[2]; as_ += c[3]
            n = SS * SS
            row.append((rs // n, gs // n, bs // n, as_ // n))
        out.append(row)
    return out


def bmp_frame(rows):
    """RGBA 行 -> ICO 的 BMP 帧 (BITMAPINFOHEADER + BGRA 自底向上 + AND 掩码)。"""
    h = len(rows)
    w = len(rows[0])
    header = struct.pack('<IiiHHIIiiII', 40, w, h * 2, 1, 32, 0,
                         w * h * 4, 0, 0, 0, 0)
    xor = bytearray()
    for y in range(h - 1, -1, -1):
        for x in range(w):
            r, g, b, a = rows[y][x]
            xor += struct.pack('BBBB', b, g, r, a)
    # AND 掩码: 每行按 32 位对齐 (32bpp 下全 0)
    row_bytes = ((w + 31) // 32) * 4
    mask = b'\x00' * (row_bytes * h)
    return header + bytes(xor) + mask


def build_ico(sizes, path):
    imgs = []
    for s in sizes:
        print(f'渲染 {s}x{s} ...')
        imgs.append((s, bmp_frame(render(s))))
    data = struct.pack('<HHH', 0, 1, len(imgs))
    offset = 6 + 16 * len(imgs)
    entries = b''
    body = b''
    for s, frame in imgs:
        entries += struct.pack('<BBBBHHII', s % 256, s % 256, 0, 0, 1, 32,
                               len(frame), offset)
        body += frame
        offset += len(frame)
    with open(path, 'wb') as f:
        f.write(data + entries + body)
    print(f'已生成 {path} ({os.path.getsize(path)} bytes)')


if __name__ == '__main__':
    here = os.path.dirname(os.path.abspath(__file__))
    build_ico([16, 24, 32, 48, 64, 128, 256], os.path.join(here, '..', 'app_icon.ico'))
