import type { Risk } from "../lib/types";
import { RISK_ZH } from "../lib/types";

const STYLES: Record<Risk, { bg: string; fg: string }> = {
    safe: { bg: "color-mix(in srgb, var(--zc-ok) 16%, transparent)", fg: "var(--zc-ok)" },
    caution: { bg: "color-mix(in srgb, var(--zc-warn) 16%, transparent)", fg: "var(--zc-warn)" },
    risky: { bg: "color-mix(in srgb, var(--zc-danger) 18%, transparent)", fg: "var(--zc-danger)" },
    expert: { bg: "transparent", fg: "var(--zc-text-2)" },
};

export function RiskBadge({ risk }: { risk: Risk }) {
    const s = STYLES[risk];
    return (
        <span
            className="inline-grid place-items-center rounded-full px-2.5 py-0.5 text-[11px] font-medium"
            style={{
                background: s.bg,
                color: s.fg,
                border: risk === "expert" ? "1px dashed var(--zc-border-strong)" : "none",
            }}
        >
            {RISK_ZH[risk]}
        </span>
    );
}
