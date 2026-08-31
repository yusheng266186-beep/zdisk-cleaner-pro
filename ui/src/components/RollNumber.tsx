import { useEffect, useRef } from "react";
import { animate, motionValue, type MotionValue } from "motion/react";
import { humanSize } from "../lib/format";

/** 数字滚动：目标变化时以 easeOutCubic 在 ~650ms 内追赶新值。
 *  bytes 模式走 humanSize；plain 模式千分位。
 *  v5 修复（AUDIT §3E）：
 *  - 旧裸 rAF 实现把「上一段起点」当作 from，高频更新（扫描进度）下数字反复回拽追不上真值；
 *    现在改用 motion value + animate()：新目标天然打断旧动画并从当前显示值续跑；
 *  - 值经 change 监听直接写进 DOM 文本，动画期间不重渲染宿主子树（旧版 60fps setState）；
 *  - prefers-reduced-motion 下直接落值，不做补间。 */
export function RollNumber({
    value,
    mode = "bytes",
    suffix,
}: {
    value: number;
    mode?: "bytes" | "plain";
    suffix?: string;
}) {
    const hostRef = useRef<HTMLSpanElement | null>(null);
    const mvRef = useRef<MotionValue<number> | null>(null);
    if (!mvRef.current) mvRef.current = motionValue(value);
    const mv = mvRef.current;

    // 显示值 → DOM 文本（不动 React 树）
    useEffect(() => {
        const fmt = (v: number) =>
            mode === "bytes" ? humanSize(v) : Math.round(v).toLocaleString("en-US");
        const unsub = mv.on("change", (v) => {
            if (hostRef.current) hostRef.current.textContent = fmt(v);
        });
        return unsub;
    }, [mv, mode]);

    // 目标变化：打断续跑（起点 = motion value 当前值，天然连续）
    useEffect(() => {
        if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) {
            mv.set(value);
            return;
        }
        const controls = animate(mv, value, {
            duration: 0.65,
            ease: [0.215, 0.61, 0.355, 1],
        });
        return () => controls.stop();
    }, [mv, value]);

    const initial =
        mode === "bytes"
            ? humanSize(mv.get())
            : Math.round(mv.get()).toLocaleString("en-US");

    return (
        <span className="num">
            <span ref={hostRef}>{initial}</span>
            {suffix && <span style={{ color: "var(--zc-text-3)" }}>{suffix}</span>}
        </span>
    );
}
