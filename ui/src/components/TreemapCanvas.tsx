import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { motion } from "motion/react";
import { ChevronRight } from "lucide-react";
import { humanSize } from "../lib/format";
import { layoutEase } from "../lib/motion";
import type { TreeNode } from "../lib/tree";

/** 空间雷达画布：单层 Squarified treemap + 点击下钻 / 面包屑回退。
 *  布局为纯函数，容器尺寸经 ResizeObserver 实测；色块间 1px 缝隙。
 *  v5 性能改造（AUDIT §3E「跟手性最大坑」）：
 *  - hover 改容器级 pointerover/pointerout 事件委托，悬停视觉挂 CSS class
 *    （.zc-tile-hover，见 global.css），不再每 tile onMouseEnter setState 触发整层 reconcile；
 *  - 每个 tile React.memo：位置/配色 props 不变则完全跳过；
 *  - 下钻动画从「width/height 弹簧」改为 0.28s 标准缓动（layoutEase，SPRING 常量已收编进
 *    motion 词汇表），保留四轴布局属性过渡；
 *  - selectedKey（Radar 页传入）渲染 2px 品牌描边选中环 + aria-current；
 *  - 容器 overscroll-behavior:contain。视觉配色保持不动。 */

interface Props {
    node: TreeNode;
    /** 选中节点回调：点击叶子块或 shift+点击任意块触发；普通左键仍走下钻 */
    onSelectNode?: (node: TreeNode) => void;
    /** 当前选中节点的 path：渲染持久选中环（Radar 页传入） */
    selectedKey?: string | null;
}

const GAP = 2; // 色块缝隙（渲染时两侧各收 GAP/2）
const TEXT_MIN_W = 88; // 名称+体积可读的最小宽度
const TEXT_MIN_H = 26; // 名称+体积可读的最小高度

/* ── 高级感配色系统 ──────────────────────────────────────
 * 8 个精选色相（低饱和、深色亲和）：顶层块按路径哈希取色，
 * 同一目录永远同一色 —— 用久了能形成「颜色=目录」的空间记忆；
 * 下钻后子块继承父色相并做 ± 微漂移，家族色统一、层间有节奏；
 * 明度随深度递减、随体量微调，块面纵向微渐变 + 顶部高光提质感。 */
const PALETTE = [212, 174, 262, 330, 24, 152, 198, 286];
const DEPTH_L = [55, 50, 46, 42]; // 各层基准明度，随下钻递减

function hash01(path: string): number {
    let h = 0;
    for (let i = 0; i < path.length; i++) h = (h * 31 + path.charCodeAt(i)) | 0;
    return Math.abs(h % 1000) / 1000;
}

interface Rect {
    x: number;
    y: number;
    w: number;
    h: number;
}

interface Tile {
    node: TreeNode;
    rect: Rect;
    /** 体量相对层均值的 log2 偏移（clamp ±6）：驱动明度/饱和度微调 */
    pct: number;
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/** 一行矩形的最差长宽比：thickness = 行面积/短边，逐块算 len/thick 取最坏 */
function worstRatio(row: { area: number }[], sum: number, side: number): number {
    const thick = sum / side;
    let worst = 0;
    for (const it of row) {
        const len = thick > 0 ? it.area / thick : 0;
        const r = len <= 0 ? Number.POSITIVE_INFINITY : Math.max(len / thick, thick / len);
        if (r > worst) worst = r;
    }
    return worst;
}

/** 标准 Squarified（Bruls et al.）：size 降序贪心成排铺满 frame。
 *  输出矩形恰好致密平铺（不留缝），1px 缝隙由渲染层统一内缩产生。 */
export function squarify(children: TreeNode[], frame: Rect): Tile[] {
    const tiles: Tile[] = [];
    if (children.length === 0 || frame.w <= 0 || frame.h <= 0) return tiles;

    const sizes = children.map((c) => Math.max(c.size, 0));
    const total = sizes.reduce((a, b) => a + b, 0);
    const canvasArea = frame.w * frame.h;
    // 全零时均分面积，保证仍可点击浏览
    const items = children
        .map((node, i) => ({
            node,
            area: total > 0 ? (sizes[i] / total) * canvasArea : canvasArea / children.length,
        }))
        .sort((a, b) => b.area - a.area);

    const avgSize = items.length > 0 && total > 0 ? total / items.length : 0;

    let rest: Rect = { ...frame };
    let i = 0;
    while (i < items.length && rest.w > 0 && rest.h > 0) {
        const side = Math.min(rest.w, rest.h);
        const row = [items[i]];
        let sum = row[0].area;
        let best = worstRatio(row, sum, side);
        while (i + row.length < items.length) {
            const cand = items[i + row.length];
            const candSum = sum + cand.area;
            const ratio = worstRatio([...row, cand], candSum, side);
            if (!(ratio < best)) break; // 变差不收；退化（∞=∞）照常并入，避免死循环
            row.push(cand);
            sum = candSum;
            best = ratio;
        }

        const thick = sum / side;
        let off = 0;
        const pushTile = (it: (typeof items)[number], rect: Rect) => {
            // 体量相对层均值的 log2 偏移（±6）：越大越亮，驱动明度/饱和微调
            const ratioOfAvg = avgSize > 0 ? Math.max(it.node.size, 0) / avgSize : 1;
            const offset = clamp(Math.log2(ratioOfAvg) * 3, -6, 6);
            tiles.push({ node: it.node, rect, pct: offset });
        };

        if (rest.w >= rest.h) {
            // 靠左的竖条：行内自上而下堆叠
            for (const it of row) {
                const h = thick > 0 ? it.area / thick : 0;
                pushTile(it, { x: rest.x, y: rest.y + off, w: thick, h });
                off += h;
            }
            rest = { x: rest.x + thick, y: rest.y, w: rest.w - thick, h: rest.h };
        } else {
            // 靠顶的横条：行内自左向右排开
            for (const it of row) {
                const w = thick > 0 ? it.area / thick : 0;
                pushTile(it, { x: rest.x + off, y: rest.y, w, h: thick });
                off += w;
            }
            rest = { x: rest.x, y: rest.y + thick, w: rest.w, h: rest.h - thick };
        }
        i += row.length;
    }
    return tiles;
}

/** 渲染前的 tile 描述：全原始 props（字符串/数字/布尔），React.memo 可浅比较。 */
interface TileView {
    k: string;
    name: string;
    size: number;
    x: number;
    y: number;
    w: number;
    h: number;
    hue: number;
    bg: string;
    showText: boolean;
    clickable: boolean;
    selected: boolean;
}

const TileRect = memo(function TileRect({
    t,
    onActivate,
}: {
    t: TileView;
    onActivate: (k: string, ev: React.MouseEvent) => void;
}) {
    return (
        <motion.div
            data-k={t.k}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, x: t.x, y: t.y, width: t.w, height: t.h }}
            transition={layoutEase}
            className={
                "zc-tile absolute left-0 top-0 overflow-hidden" +
                (t.selected ? " zc-tile-sel" : "")
            }
            aria-current={t.selected ? "true" : undefined}
            style={
                {
                    background: t.bg,
                    borderRadius: "var(--zc-r-sm)",
                    cursor: t.clickable ? "pointer" : "default",
                    "--tile-hue": t.hue,
                } as React.CSSProperties
            }
            onClick={(e) => onActivate(t.k, e)}
        >
            {t.showText && (
                <div className="pointer-events-none absolute left-2 right-1 top-1.5">
                    <div
                        className="truncate text-[12px] font-medium leading-tight"
                        style={{ color: "white", textShadow: "0 1px 2px rgb(0 0 0 / .45)" }}
                    >
                        {t.name}
                    </div>
                    <div
                        className="num truncate text-[10px] leading-tight"
                        style={{ color: "rgb(255 255 255 / .78)", textShadow: "0 1px 2px rgb(0 0 0 / .4)" }}
                    >
                        {humanSize(t.size)}
                    </div>
                </div>
            )}
        </motion.div>
    );
});

export function TreemapCanvas({ node, onSelectNode, selectedKey }: Props) {
    // 缩放栈：栈底 = 传入根，栈顶 = 当前展示层
    const [zoomStack, setZoomStack] = useState<TreeNode[]>([node]);
    useEffect(() => setZoomStack([node]), [node]);

    const current = zoomStack[zoomStack.length - 1];
    const layerIndex = zoomStack.length - 1; // 0 = 首层
    const baseL = DEPTH_L[Math.min(layerIndex, DEPTH_L.length - 1)];

    // 父链色相：顶层块各自从调色板取色；更深层沿面包屑逐级 ± 漂移，形成家族色
    const parentHue = useMemo(() => {
        let hue = PALETTE[Math.floor(hash01(zoomStack[0]?.path ?? "") * PALETTE.length) % PALETTE.length];
        for (let i = 1; i <= layerIndex; i++) {
            hue = (hue + Math.round((hash01(zoomStack[i].path) - 0.5) * 40) + 360) % 360;
        }
        return hue;
    }, [zoomStack, layerIndex]);

    const hostRef = useRef<HTMLDivElement | null>(null);
    const [box, setBox] = useState({ w: 0, h: 0 });
    useEffect(() => {
        const el = hostRef.current;
        if (!el) return;
        const ro = new ResizeObserver((entries) => {
            const cr = entries[0]?.contentRect;
            if (cr) setBox({ w: cr.width, h: cr.height });
        });
        ro.observe(el);
        return () => ro.disconnect();
    }, []);

    const tiles = useMemo(
        () => squarify(current?.children ?? [], { x: 0, y: 0, w: box.w, h: box.h }),
        [current, box.w, box.h],
    );

    /** 当前层某块的色相：顶层=调色板哈希取色；深层=父色相 ± 微漂移 */
    const views = useMemo(() => {
        const byKey = new Map<string, TreeNode>();
        const out: TileView[] = tiles.map((t) => {
            // 统一内缩 GAP/2，形成恒定缝隙
            const r: Rect = {
                x: t.rect.x + GAP / 2,
                y: t.rect.y + GAP / 2,
                w: Math.max(t.rect.w - GAP, 0),
                h: Math.max(t.rect.h - GAP, 0),
            };
            const k = t.node.path || `${t.node.name}#${Math.round(r.x)}x${Math.round(r.y)}`;
            byKey.set(k, t.node);
            const hue =
                layerIndex === 0
                    ? PALETTE[Math.floor(hash01(t.node.path) * PALETTE.length) % PALETTE.length]
                    : (parentHue + Math.round((hash01(t.node.path) - 0.5) * 40) + 360) % 360;
            // 色相:顶层=调色板取色,深层=家族色;明度=层基准±体量;低饱和克制的渐变面
            const L = clamp(baseL + t.pct * 1.4, 28, 66);
            const S = clamp(42 + t.pct * 1.2, 30, 56);
            const bg = `linear-gradient(165deg, hsl(${hue} ${S}% ${clamp(L + 6, 30, 72)}%) 0%, hsl(${hue} ${clamp(S - 9, 22, 56)}% ${clamp(L - 7, 20, 64)}%) 100%)`;
            return {
                k,
                name: t.node.name,
                size: t.node.size,
                x: r.x,
                y: r.y,
                w: r.w,
                h: r.h,
                hue,
                bg,
                showText: r.w >= TEXT_MIN_W && r.h >= TEXT_MIN_H,
                clickable: t.node.children.length > 0,
                selected: selectedKey != null && selectedKey === t.node.path,
            };
        });
        return { out, byKey };
    }, [tiles, layerIndex, parentHue, baseL, selectedKey]);

    /** 点击分发：叶子或 shift+点击 → 选中回调；其余普通左键保持下钻 */
    const handleActivate = useCallback(
        (k: string, ev: React.MouseEvent) => {
            const n = views.byKey.get(k);
            if (!n) return;
            if (ev.shiftKey || n.children.length === 0) {
                onSelectNode?.(n);
                return;
            }
            setZoomStack((s) => [...s, n]);
        },
        [views, onSelectNode],
    );

    // hover 事件委托：容器上收 pointerover，悬停视觉 = 给目标 tile 挂 CSS class。
    // 不进 React 状态 → 数百色块零 reconcile；也替代了旧的 filter:brightness。
    const hoverEl = useRef<HTMLElement | null>(null);
    const handlePointerOver = (ev: React.PointerEvent<HTMLDivElement>) => {
        const el = (ev.target as HTMLElement).closest<HTMLElement>("[data-k]");
        if (!el || el === hoverEl.current) return;
        hoverEl.current?.classList.remove("zc-tile-hover");
        if (el.isConnected) {
            el.classList.add("zc-tile-hover");
            hoverEl.current = el;
        } else {
            hoverEl.current = null; // 下钻重挂瞬间旧节点已离屏
        }
    };
    const clearHover = () => {
        hoverEl.current?.classList.remove("zc-tile-hover");
        hoverEl.current = null;
    };
    // 下钻后 tile 集合更换，清掉指向已卸载节点的悬停引用
    useEffect(clearHover, [zoomStack]);

    return (
        <div className="flex h-full min-h-0 flex-col gap-2">
            {/* ── 面包屑：整链可点回退 ── */}
            <div className="flex min-h-7 flex-wrap items-center gap-1 text-xs">
                {zoomStack.map((n, i) => {
                    const last = i === zoomStack.length - 1;
                    return (
                        <span key={n.path || `${n.name}@${i}`} className="flex items-center gap-1">
                            {i > 0 && <ChevronRight size={12} style={{ color: "var(--zc-text-3)" }} />}
                            <button
                                onClick={() => !last && setZoomStack((s) => s.slice(0, i + 1))}
                                disabled={last}
                                className="max-w-52 truncate rounded-md px-1.5 py-0.5 transition-colors"
                                style={{ color: last ? "var(--zc-text-1)" : "var(--zc-accent-text)" }}
                            >
                                {n.name}
                            </button>
                        </span>
                    );
                })}
                {current && (
                    <span className="num ml-auto pl-3 text-[11px]" style={{ color: "var(--zc-text-3)" }}>
                        {humanSize(current.size)} · {current.children.length} 个子目录
                    </span>
                )}
            </div>

            {/* ── treemap 本体 ── */}
            <div
                ref={hostRef}
                onPointerOver={handlePointerOver}
                onPointerLeave={clearHover}
                className="relative min-h-40 flex-1 overflow-hidden rounded-xl border"
                style={{
                    background: "var(--zc-surface-2)",
                    borderColor: "var(--zc-border)",
                    overscrollBehavior: "contain",
                }}
            >
                {views.out.map((t) => (
                    <TileRect key={t.k} t={t} onActivate={handleActivate} />
                ))}
            </div>
        </div>
    );
}
