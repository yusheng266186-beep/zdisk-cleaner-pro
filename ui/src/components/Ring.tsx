import { animate, useMotionValue, useTransform } from "motion/react";
import { useEffect } from "react";

/** 健康环 / 磁盘环：SVG stroke 由弹簧驱动的 motion value 绘制，
 *  数值变化时平滑追赶（可中断），负值/超界自动收敛。 */
export function Ring({
    size = 180,
    stroke = 12,
    pct,
    color = "var(--zc-accent-a)",
    track = "var(--zc-surface-3)",
    children,
}: {
    size?: number;
    stroke?: number;
    pct: number; // 0..1
    color?: string;
    track?: string;
    children?: React.ReactNode;
}) {
    const r = (size - stroke) / 2;
    const c = 2 * Math.PI * r;
    const mv = useMotionValue(0);
    const offset = useTransform(mv, (v) => c - c * Math.min(Math.max(v, 0), 1));

    useEffect(() => {
        const controls = animate(mv, Math.min(Math.max(pct, 0), 1), {
            type: "spring",
            stiffness: 90,
            damping: 20,
        });
        return () => controls.stop();
    }, [pct, mv]);

    const gid = `ring-grad-${Math.round(size)}-${stroke}`;
    return (
        <div className="relative grid place-items-center" style={{ width: size, height: size }}>
            <svg width={size} height={size} className="-rotate-90">
                <defs>
                    <linearGradient id={gid} x1="0%" y1="0%" x2="100%" y2="100%">
                        <stop offset="0%" stopColor="var(--zc-accent-a)" />
                        <stop offset="100%" stopColor="var(--zc-accent-b)" />
                    </linearGradient>
                </defs>
                <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke={track} strokeWidth={stroke} />
                <motion.circle
                    cx={size / 2}
                    cy={size / 2}
                    r={r}
                    fill="none"
                    stroke={color === "var(--zc-accent-a)" ? `url(#${gid})` : color}
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

// 局部重导出，避免每个使用处重复 import motion
import { motion } from "motion/react";
