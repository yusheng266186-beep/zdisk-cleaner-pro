import { useState } from "react";
import { Archive, History as HistIcon, RotateCcw, Trash2 } from "lucide-react";
import { motion } from "motion/react";
import { cascade, pageVariants } from "../lib/motion";
import { useStore } from "../store";
import { humanSize, timeAgo } from "../lib/format";

export function History() {
    const history = useStore((s) => s.history);
    // 彻底删除二次确认：点一下变「再点一次确认」，4 秒未确认自动复位
    const [confirmId, setConfirmId] = useState<string | null>(null);
    const max = Math.max(1, ...history.map((h) => h.bytes_moved));

    return (
        <motion.div variants={pageVariants} initial="initial" animate="animate" exit="exit" className="mx-auto max-w-3xl">
            <h1 className="text-xl font-semibold">清理历史</h1>
            <p className="mt-1 text-xs" style={{ color: "var(--zc-text-3)" }}>
                「移入」不等于「真实释放」：回收站批次以清空系统回收站为准；vault 批次 7 天内可还原，
                随时可「彻底删除」立即释放空间，超过 7 天的批次应用启动时自动清扫。
            </p>

            {/* 趋势条形图（纯 CSS 高度动画） */}
            <div className="mt-6 flex h-28 items-end gap-2 rounded-xl border p-4" style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }}>
                {history.slice(0, 16).map((h, i) => (
                    <motion.div
                        key={h.session_id + i}
                        initial={{ height: 0 }}
                        animate={{ height: `${Math.max(8, (h.bytes_moved / max) * 100)}%` }}
                        transition={{ type: "spring", stiffness: 120, damping: 20, delay: i * 0.03 }}
                        className="flex-1 rounded-t-md"
                        style={{ background: "linear-gradient(180deg,var(--zc-accent-b),var(--zc-accent-a))", opacity: 0.35 + (h.bytes_moved / max) * 0.65 }}
                        title={`${humanSize(h.bytes_moved)} · ${timeAgo(h.created_unix)}`}
                    />
                ))}
                {history.length === 0 && (
                    <div className="w-full py-10 text-center text-sm" style={{ color: "var(--zc-text-3)" }}>
                        还没有清理记录 —— 完成第一次体检后这里会开始记账
                    </div>
                )}
            </div>

            <ul className="mt-5 flex flex-col gap-2">
                {history.map((h, i) => (
                    <motion.li
                        key={h.session_id + i}
                        variants={cascade(i)}
                        initial="initial"
                        animate="animate"
                        className="flex items-center gap-3 rounded-xl border p-3.5"
                        style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }}
                    >
                        {h.mode === "vault" ? (
                            <Archive size={15} style={{ color: "var(--zc-accent-b)" }} />
                        ) : (
                            <HistIcon size={15} style={{ color: "var(--zc-text-3)" }} />
                        )}
                        <span className="flex-1 text-sm">
                            {humanSize(h.bytes_moved)}
                            <span style={{ color: "var(--zc-text-3)" }}> · 搬运 {h.files.toLocaleString("en-US")} 项</span>
                        </span>
                        <span className="num text-xs" style={{ color: "var(--zc-text-3)" }}>{timeAgo(h.created_unix)}</span>
                        {h.session_id.startsWith("elev-") && (
                            <span className="rounded-full border px-2 py-0.5 text-[10px]" style={{ borderColor: "var(--zc-border)", color: "var(--zc-text-3)" }}>
                                提权批
                            </span>
                        )}
                        {h.mode === "vault" ? (
                            <>
                                <button
                                    onClick={() => void useStore.getState().undoSession(h.session_id)}
                                    className="flex items-center gap-1 rounded-lg border px-2.5 py-1 text-[11px] transition-colors hover:opacity-75"
                                    style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-ok)" }}
                                >
                                    <RotateCcw size={11} /> 还原
                                </button>
                                <button
                                    onClick={() => {
                                        if (confirmId === h.session_id) {
                                            setConfirmId(null);
                                            void useStore.getState().purgeSession(h.session_id);
                                        } else {
                                            setConfirmId(h.session_id);
                                            setTimeout(() => setConfirmId((c) => (c === h.session_id ? null : c)), 4000);
                                        }
                                    }}
                                    className="flex items-center gap-1 rounded-lg border px-2.5 py-1 text-[11px] transition-colors hover:opacity-75"
                                    style={{
                                        borderColor: confirmId === h.session_id ? "var(--zc-danger)" : "var(--zc-border-strong)",
                                        color: confirmId === h.session_id ? "var(--zc-danger)" : "var(--zc-text-3)",
                                    }}
                                >
                                    <Trash2 size={11} />
                                    {confirmId === h.session_id ? "再点一次确认" : "彻底删除"}
                                </button>
                            </>
                        ) : (
                            <span className="text-[10px]" style={{ color: "var(--zc-text-3)" }}>
                                在系统回收站
                            </span>
                        )}
                    </motion.li>
                ))}
            </ul>
        </motion.div>
    );
}
