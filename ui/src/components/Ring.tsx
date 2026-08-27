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

    return (
        <div className="relative grid place-items-center" style={{ width: size, height: size }}>
            <svg width={size} height={size} className="-rotate-90">
                <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke={track} strokeWidth={stroke} />
                <motion.circle
                    cx={size / 2}
                    cy={size / 2}
                    r={r}
                    fill="none"
                    stroke={color}
                    strokeWidth={stroke}
                    strokeLinecap="round"
                    strokeDasharray={c}
                    style={{ strokeDashoffset: offset }}
                />
            </svg>
            <div className="absolute inset-0 grid place-items-center">{children}</div>
        </div>
    );
}

// 局部重导出，避免每个使用处重复 import motion
import { motion } from "motion/react";
