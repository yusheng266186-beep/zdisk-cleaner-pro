import { useEffect, useMemo, useRef, useState } from "react";
import { motion } from "motion/react";
import { ChevronRight } from "lucide-react";
import { humanSize } from "../lib/format";
import type { TreeNode } from "../lib/tree";

/** 空间雷达画布：单层 Squarified treemap + 点击下钻 / 面包屑回退。
 *  布局为纯函数，容器尺寸经 ResizeObserver 实测；色块间 1px 缝隙。 */

interface Props {
    node: TreeNode;
    /** 选中节点回调：点击叶子块或 shift+点击任意块触发；普通左键仍走下钻 */
    onSelectNode?: (node: TreeNode) => void;
}

const GAP = 1; // 色块缝隙（渲染时两侧各收 GAP/2）
const TEXT_MIN_W = 88; // 名称+体积可读的最小宽度
const TEXT_MIN_H = 26; // 名称+体积可读的最小高度
const SPRING = { type: "spring", stiffness: 300, damping: 30 } as const;

/** 各层亮底（accent-a 占比），随下钻深度递减；更深层取末档 */
const DEPTH_BASE = [34, 24, 16, 10];

interface Rect {
    x: number;
    y: number;
    w: number;
    h: number;
}

interface Tile {
    node: TreeNode;
    rect: Rect;
    /** color-mix 中 accent-a 的百分比：按层基准 ± size 占比微调，clamp 到 [8,42] */
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
export function squarify(children: TreeNode[], frame: Rect, basePct: number): Tile[] {
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
            // 占比越大越亮：以「相对层均值」为自变量，log2 折半减一档、翻倍加一档，
            // 每 3 个数量级倍数走满 ±6，最终整体夹回 [8,42]
            const ratioOfAvg = avgSize > 0 ? Math.max(it.node.size, 0) / avgSize : 1;
            const offset = clamp(Math.log2(ratioOfAvg) * 3, -6, 6);
            tiles.push({ node: it.node, rect, pct: clamp(basePct + offset, 8, 42) });
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

export function TreemapCanvas({ node, onSelectNode }: Props) {
    // 缩放栈：栈底 = 传入根，栈顶 = 当前展示层
    const [zoomStack, setZoomStack] = useState<TreeNode[]>([node]);
    useEffect(() => setZoomStack([node]), [node]);

    const current = zoomStack[zoomStack.length - 1];
    const layerIndex = zoomStack.length - 1; // 0 = 首层
    const basePct = DEPTH_BASE[Math.min(layerIndex, DEPTH_BASE.length - 1)];

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

    const [hoveredKey, setHoveredKey] = useState<string | null>(null);

    const tiles = useMemo(
        () => squarify(current?.children ?? [], { x: 0, y: 0, w: box.w, h: box.h }, basePct),
        [current, box.w, box.h, basePct],
    );

    const drill = (child: TreeNode) => {
        if (child.children.length === 0) return;
        setZoomStack((s) => [...s, child]);
    };

    /** 点击分发：叶子或 shift+点击 → 选中回调；其余普通左键保持下钻 */
    const handleTileClick = (n: TreeNode, ev: React.MouseEvent) => {
        if (ev.shiftKey || n.children.length === 0) {
            onSelectNode?.(n);
            return;
        }
        drill(n);
    };

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
                                style={{ color: last ? "var(--zc-text-1)" : "var(--zc-accent-b)" }}
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
                className="relative min-h-40 flex-1 overflow-hidden rounded-xl border"
                style={{ background: "var(--zc-surface-2)", borderColor: "var(--zc-border)" }}
            >
                {tiles.map((t) => {
                    // 统一内缩 GAP/2，形成恒定 1px 缝隙
                    const r: Rect = {
                        x: t.rect.x + GAP / 2,
                        y: t.rect.y + GAP / 2,
                        w: Math.max(t.rect.w - GAP, 0),
                        h: Math.max(t.rect.h - GAP, 0),
                    };
                    const key = t.node.path || `${t.node.name}#${Math.round(r.x)}x${Math.round(r.y)}`;
                    const hovered = hoveredKey === key;
                    const showText = r.w >= TEXT_MIN_W && r.h >= TEXT_MIN_H;
                    return (
                        <motion.div
                            key={key}
                            initial={{ opacity: 0 }}
                            animate={{ opacity: 1, x: r.x, y: r.y, width: r.w, height: r.h }}
                            transition={SPRING}
                            className="absolute left-0 top-0 overflow-hidden"
                            style={{
                                background: `color-mix(in srgb, var(--zc-accent-a) ${t.pct.toFixed(2)}%, var(--zc-surface-2))`,
                                borderRadius: "var(--zc-r-sm)",
                                boxShadow: hovered ? "inset 0 0 0 2px var(--zc-accent-b)" : undefined,
                                cursor: t.node.children.length > 0 ? "pointer" : "default",
                            }}
                            onClick={(e) => handleTileClick(t.node, e)}
                            onMouseEnter={() => setHoveredKey(key)}
                            onMouseLeave={() => setHoveredKey((p) => (p === key ? null : p))}
                        >
                            {showText && (
                                <div className="pointer-events-none absolute left-2 right-1 top-1.5">
                                    <div className="truncate text-[12px] font-medium leading-tight">{t.node.name}</div>
                                    <div className="num truncate text-[10px] leading-tight" style={{ color: "var(--zc-text-2)" }}>
                                        {humanSize(t.node.size)}
                                    </div>
                                </div>
                            )}
                        </motion.div>
                    );
                })}
            </div>
        </div>
    );
}
