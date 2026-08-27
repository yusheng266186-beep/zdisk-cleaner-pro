import { useEffect, useRef, useState } from "react";
import { humanSize } from "../lib/format";

/** 数字滚动：目标变化时以 easeOutCubic 在 ~650ms 内追赶新值。
 *  bytes 模式走 humanSize；plain 模式千分位。 */
export function RollNumber({
    value,
    mode = "bytes",
    suffix,
}: {
    value: number;
    mode?: "bytes" | "plain";
    suffix?: string;
}) {
    const [shown, setShown] = useState(value);
    const fromRef = useRef(value);

    useEffect(() => {
        const from = fromRef.current;
        const t0 = performance.now();
        const dur = 650;
        let raf = 0;
        const tick = (t: number) => {
            const p = Math.min((t - t0) / dur, 1);
            const ease = 1 - Math.pow(1 - p, 3);
            setShown(from + (value - from) * ease);
            if (p < 1) raf = requestAnimationFrame(tick);
            else fromRef.current = value;
        };
        raf = requestAnimationFrame(tick);
        return () => cancelAnimationFrame(raf);
    }, [value]);

    return (
        <span className="num">
            {mode === "bytes" ? humanSize(shown) : Math.round(shown).toLocaleString("en-US")}
            {suffix && <span style={{ color: "var(--zc-text-3)" }}>{suffix}</span>}
        </span>
    );
}
