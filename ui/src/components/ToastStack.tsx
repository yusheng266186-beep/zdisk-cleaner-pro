import { AnimatePresence, motion } from "motion/react";
import { useStore } from "../store";
import { springSnappy } from "../lib/motion";
import { CheckCircle2, AlertTriangle, Info, XCircle } from "lucide-react";

const ICON = {
    ok: CheckCircle2,
    warn: AlertTriangle,
    err: XCircle,
    info: Info,
};
const COLOR = {
    ok: "var(--zc-ok)",
    warn: "var(--zc-warn)",
    err: "var(--zc-danger)",
    info: "var(--zc-accent-b)",
};

/** Toast 队列：右下角堆叠，新消息从下滑入，4.2s 自动淡出 */
export function ToastStack() {
    const toasts = useStore((s) => s.toasts);
    return (
        <div className="pointer-events-none fixed bottom-5 right-5 z-50 flex w-[min(380px,80vw)] flex-col gap-2">
            <AnimatePresence>
                {toasts.map((t) => {
                    const Icon = ICON[t.kind];
                    return (
                        <motion.div
                            key={t.id}
                            layout
                            initial={{ opacity: 0, y: 24, scale: 0.96 }}
                            animate={{ opacity: 1, y: 0, scale: 1, transition: springSnappy }}
                            exit={{ opacity: 0, x: 40, transition: { duration: 0.18 } }}
                            className="flex items-start gap-2.5 rounded-xl border p-3 text-sm"
                            style={{
                                background: "color-mix(in srgb, var(--zc-surface-1) 88%, transparent)",
                                borderColor: "var(--zc-border)",
                                boxShadow: "var(--zc-shadow-panel)",
                                backdropFilter: "blur(8px)",
                            }}
                        >
                            <Icon size={16} style={{ color: COLOR[t.kind], marginTop: 2 }} />
                            <span style={{ color: "var(--zc-text-1)" }}>{t.msg}</span>
                        </motion.div>
                    );
                })}
            </AnimatePresence>
        </div>
    );
}
