import { animate, motion, useMotionValue, useTransform } from "motion/react";
import { useEffect, useId } from "react";

/** 健康环 / 磁盘环：SVG stroke 由弹簧驱动的 motion value 绘制，
 *  数值变化时平滑追赶（可中断），负值/超界自动收敛。
 *  v5：gid 改 useId（去重不依赖尺寸）；渐变开关显式化——
 *  `gradient`（默认 true）不再靠 color 字符串比对暗判：
 *  显式传入 color 时以 color 为准（实心档），不传 color 且 gradient=true 才走品牌渐变。 */
export function Ring({
    size = 180,
    stroke = 12,
    pct,
    color,
    gradient = true,
    track = "var(--zc-surface-3)",
    children,
}: {
    size?: number;
    stroke?: number;
    pct: number; // 0..1
    /** 实心描边色；传入即优先于渐变（保持调用方语义） */
    color?: string;
    /** 未显式给 color 时是否使用品牌渐变描边（默认 true） */
    gradient?: boolean;
    track?: string;
    children?: React.ReactNode;
}) {
    const r = (size - stroke) / 2;
    const c = 2 * Math.PI * r;
    const mv = useMotionValue(0);
    const offset = useTransform(mv, (v) => c - c * Math.min(Math.max(v, 0), 1));
    const uid = useId().replace(/[^a-zA-Z0-9_-]/g, "");
    const gid = `ring-grad-${uid}`;

    useEffect(() => {
        const controls = animate(mv, Math.min(Math.max(pct, 0), 1), {
            type: "spring",
            stiffness: 90,
            damping: 20,
        });
        return () => controls.stop();
    }, [pct, mv]);

    const useGrad = color === undefined && gradient;
    const strokePaint = color ?? (gradient ? `url(#${gid})` : "var(--zc-accent-a)");

    return (
        <div className="relative grid place-items-center" style={{ width: size, height: size }}>
            <svg width={size} height={size} className="-rotate-90">
                {useGrad && (
                    <defs>
                        <linearGradient id={gid} x1="0%" y1="0%" x2="100%" y2="100%">
                            <stop offset="0%" stopColor="var(--zc-accent-a)" />
                            <stop offset="100%" stopColor="var(--zc-accent-b)" />
                        </linearGradient>
                    </defs>
                )}
                <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke={track} strokeWidth={stroke} />
                <motion.circle
                    cx={size / 2}
                    cy={size / 2}
                    r={r}
                    fill="none"
                    stroke={strokePaint}
                    strokeWidth={stroke}
                    strokeLinecap="round"
                    strokeDasharray={c}
                    style={{ strokeDashoffset: offset, filter: "drop-shadow(0 0 6px color-mix(in srgb, var(--zc-accent-b) 35%, transparent))" }}
                />
            </svg>
            <div className="absolute inset-0 grid place-items-center">{children}</div>
        </div>
    );
}
