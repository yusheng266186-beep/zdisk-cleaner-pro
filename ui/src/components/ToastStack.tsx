import { AnimatePresence, motion } from "motion/react";
import { useStore } from "../store";
import { fastFade, springSnappy } from "../lib/motion";
import { CheckCircle2, AlertTriangle, Info, XCircle, X } from "lucide-react";

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
    info: "var(--zc-accent-text)",
};

/** store 无逐条删除 action（只有 setTimeout 自动过期）；用 zustand 现成的
 *  setState 就地过滤，不触碰 store.ts 本体。 */
function dismissToast(id: number) {
    useStore.setState((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
}

/** Toast 队列：右下角堆叠，新消息从下滑入，4.2s 自动淡出；
 *  v5：右上关闭 X（可手动阅后即焚）；hover 时进度线暂停（CSS animation-play-state）；
 *  进度线改纯 CSS 动画子元素，父级 exit 位移不再拖它重放入场。 */
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
                            className="zc-toast pointer-events-auto relative flex items-start gap-2.5 overflow-hidden rounded-xl border py-3 pl-3 pr-2 text-sm"
                            initial={{ opacity: 0, y: 24, scale: 0.96 }}
                            animate={{ opacity: 1, y: 0, scale: 1, transition: springSnappy }}
                            exit={{ opacity: 0, x: 40, transition: fastFade }}
                            style={{
                                background: "color-mix(in srgb, var(--zc-surface-1) 88%, transparent)",
                                borderColor: "var(--zc-border)",
                                boxShadow: "var(--zc-shadow-panel)",
                                backdropFilter: "blur(8px)",
                            }}
                        >
                            {/* 左缘状态色条 */}
                            <span
                                className="absolute inset-y-0 left-0 w-[3px]"
                                style={{ background: COLOR[t.kind] }}
                            />
                            <span
                                className="grid h-6 w-6 shrink-0 place-items-center rounded-full"
                                style={{ background: `color-mix(in srgb, ${COLOR[t.kind]} 16%, transparent)` }}
                            >
                                <Icon size={14} style={{ color: COLOR[t.kind] }} />
                            </span>
                            <span className="min-w-0 flex-1 pt-0.5" style={{ color: "var(--zc-text-1)" }}>{t.msg}</span>
                            <button
                                aria-label="关闭通知"
                                onClick={() => dismissToast(t.id)}
                                className="zc-press -mt-0.5 grid h-6 w-6 shrink-0 place-items-center rounded-md hover:bg-[var(--zc-hover)]"
                                style={{ color: "var(--zc-text-3)" }}
                            >
                                <X size={13} />
                            </button>
                            {/* 自动消失进度线:纯 CSS,时长令牌与 store 4.2s 严格同参,hover 暂停 */}
                            <span
                                className="zc-toast-progress"
                                style={{ background: COLOR[t.kind] }}
                            />
                        </motion.div>
                    );
                })}
            </AnimatePresence>
        </div>
    );
}
