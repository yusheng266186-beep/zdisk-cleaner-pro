import { AnimatePresence, motion } from "motion/react";
import { Archive, ChevronDown, Trash2, RotateCcw } from "lucide-react";
import { RiskBadge } from "../components/RiskBadge";
import { cascade, pageVariants, springSnappy } from "../lib/motion";
import { selectableFindings, useStore, cleanableBytes } from "../store";
import { humanSize } from "../lib/format";
import { DOMAIN_ZH } from "../lib/types";

export function Results() {
    const report = useStore((s) => s.report);
    const rules = useStore((s) => s.rules);
    const selection = useStore((s) => s.selection);
    const toggleSelect = useStore((s) => s.toggleSelect);
    const selectSafeOnly = useStore((s) => s.selectSafeOnly);
    const clearSelection = useStore((s) => s.clearSelection);
    const cleanSelected = useStore((s) => s.cleanSelected);
    const expandedRule = useStore((s) => s.expandedRule);
    const setExpanded = useStore((s) => s.setExpanded);

    const findings = [...selectableFindings(report)].sort(
        (a, b) => bytesOf(b) - bytesOf(a),
    );

    const selectedBytes = report
        ? (report.findings ?? [])
              .filter((f) => selection.has(f.rule_id))
              .reduce((a, f) => a + bytesOf(f), 0)
        : 0;
    const totalBytes = cleanableBytes(report ?? { findings: [] } as never);

    return (
        <motion.div variants={pageVariants} initial="initial" animate="animate" exit="exit" className="mx-auto max-w-3xl pb-28">
            <div className="mb-5 flex items-end justify-between">
                <div>
                    <h1 className="text-xl font-semibold">体检结果</h1>
                    <p className="mt-0.5 text-xs" style={{ color: "var(--zc-text-3)" }}>
                        共发现 {humanSize(totalBytes)} 可清理 · 按体积降序 · 已勾选{" "}
                        <b style={{ color: "var(--zc-accent-b)" }}>{humanSize(selectedBytes)}</b>
                    </p>
                </div>
                <div className="flex gap-2 text-xs">
                    <ChipBtn onClick={selectSafeOnly}>只选安全</ChipBtn>
                    <ChipBtn onClick={clearSelection}>清空勾选</ChipBtn>
                </div>
            </div>

            <ul className="flex flex-col gap-2.5">
                {findings.map((f, i) => {
                    const meta = rules.find((r) => r.id === f.rule_id);
                    const checked = selection.has(f.rule_id);
                    const open = expandedRule === f.rule_id;
                    const bytes = bytesOf(f);
                    return (
                        <motion.li key={f.rule_id} variants={cascade(i)} initial="initial" animate="animate" exit="exit" layout>
                            <div
                                className="rounded-xl border transition-colors"
                                style={{
                                    background: "var(--zc-surface-1)",
                                    borderColor: checked ? "color-mix(in srgb, var(--zc-accent-b) 45%, transparent)" : "var(--zc-border)",
                                    boxShadow: checked ? "0 0 0 1px color-mix(in srgb, var(--zc-accent-b) 25%, transparent)" : "none",
                                }}
                            >
                                <div className="flex items-center gap-3 p-3.5">
                                    {/* 勾选框 */}
                                    <button
                                        onClick={() => toggleSelect(f.rule_id)}
                                        className="grid h-5 w-5 shrink-0 place-items-center rounded-md border transition-all"
                                        style={{
                                            borderColor: checked ? "transparent" : "var(--zc-border-strong)",
                                            background: checked ? "linear-gradient(135deg,var(--zc-accent-a),var(--zc-accent-b))" : "transparent",
                                        }}
                                        aria-pressed={checked}
                                    >
                                        <AnimatePresence>
                                            {checked && (
                                                <motion.svg
                                                    initial={{ scale: 0 }} animate={{ scale: 1 }} exit={{ scale: 0 }}
                                                    transition={springSnappy}
                                                    width="12" height="12" viewBox="0 0 24 24" fill="none"
                                                >
                                                    <path d="M4 12l6 6L20 6" stroke="#fff" strokeWidth="3.2" strokeLinecap="round" strokeLinejoin="round" />
                                                </motion.svg>
                                            )}
                                        </AnimatePresence>
                                    </button>

                                    <button className="min-w-0 flex-1 text-left" onClick={() => setExpanded(open ? null : f.rule_id)}>
                                        <div className="flex items-center gap-2">
                                            <span className="truncate text-sm font-medium">{meta?.name_zh ?? f.rule_id}</span>
                                            {meta && <RiskBadge risk={meta.risk} />}
                                            <span className="text-[11px]" style={{ color: "var(--zc-text-3)" }}>
                                                {meta ? DOMAIN_ZH[meta.domain] : ""}
                                            </span>
                                        </div>
                                        <div className="num mt-0.5 text-xs" style={{ color: "var(--zc-text-3)" }}>
                                            {countOf(f)} 项
                                        </div>
                                    </button>

                                    <div className="num shrink-0 text-sm font-medium" style={{ color: bytes > 0 ? "var(--zc-text-1)" : "var(--zc-text-3)" }}>
                                        {humanSize(bytes)}
                                    </div>
                                    <motion.span animate={{ rotate: open ? 180 : 0 }} transition={springSnappy}>
                                        <ChevronDown size={16} style={{ color: "var(--zc-text-3)" }} />
                                    </motion.span>
                                </div>

                                {/* 明细抽屉 */}
                                <AnimatePresence>
                                    {open && (
                                        <motion.div
                                            initial={{ height: 0, opacity: 0 }}
                                            animate={{ height: "auto", opacity: 1, transition: springSnappy }}
                                            exit={{ height: 0, opacity: 0, transition: { duration: 0.18 } }}
                                            className="overflow-hidden"
                                        >
                                            <ul className="border-t px-4 py-2" style={{ borderColor: "var(--zc-border)" }}>
                                                {(f.hits.length ? f.hits.slice(0, 10) : [{ path: "（明细超出展示上限，overflow 计数）", size: f.overflow_bytes, is_dir: false }]).map(
                                                    (h, j) => (
                                                        <li key={j} className="num truncate py-1 font-mono text-[11px]" style={{ color: "var(--zc-text-2)" }} title={h.path}>
                                                            {h.path || `+${f.overflow_hits} 项 ≈ ${humanSize(f.overflow_bytes)}`}
                                                        </li>
                                                    ),
                                                )}
                                                {f.overflow_hits > 0 && (
                                                    <li className="py-1 text-[11px]" style={{ color: "var(--zc-text-3)" }}>
                                                        … 另有 {f.overflow_hits.toLocaleString("en-US")} 项未展开（{humanSize(f.overflow_bytes)}）
                                                    </li>
                                                )}
                                            </ul>
                                        </motion.div>
                                    )}
                                </AnimatePresence>
                            </div>
                        </motion.li>
                    );
                })}
            </ul>

            {/* 底部操作条 */}
            <AnimatePresence>
                {selection.size > 0 && (
                    <motion.div
                        initial={{ y: 90 }}
                        animate={{ y: 0, transition: springSnappy }}
                        exit={{ y: 90 }}
                        className="fixed bottom-5 left-1/2 z-40 -translate-x-1/2"
                    >
                        <div
                            className="flex items-center gap-3 rounded-full border py-2 pl-5 pr-2"
                            style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border-strong)", boxShadow: "var(--zc-shadow-pop)" }}
                        >
                            <RotateCcw size={14} style={{ color: "var(--zc-ok)" }} />
                            <span className="text-sm">
                                清理 <b>{selection.size}</b> 条 ·{" "}
                                <b className="num">{humanSize(selectedBytes)}</b>
                            </span>
                            <IconAction icon={<Archive size={15} />} label="暂存区" onClick={() => void cleanSelected("vault")} />
                            <IconAction icon={<Trash2 size={15} />} label="回收站" onClick={() => void cleanSelected("recycle_bin")} danger />
                        </div>
                    </motion.div>
                )}
            </AnimatePresence>
        </motion.div>
    );
}

function bytesOf(f: { hits: { size: number }[]; overflow_bytes: number }): number {
    return f.hits.reduce((a, h) => a + h.size, 0) + f.overflow_bytes;
}
function countOf(f: { hits: unknown[]; overflow_hits: number }): number {
    return f.hits.length + f.overflow_hits;
}

function ChipBtn({ children, onClick }: { children: React.ReactNode; onClick: () => void }) {
    return (
        <button
            onClick={onClick}
            className="rounded-lg border px-2.5 py-1 transition-colors hover:opacity-75"
            style={{ borderColor: "var(--zc-border)", color: "var(--zc-text-2)" }}
        >
            {children}
        </button>
    );
}

function IconAction({ icon, label, onClick, danger }: { icon: React.ReactNode; label: string; onClick: () => void; danger?: boolean }) {
    return (
        <button
            onClick={onClick}
            className="flex items-center gap-1.5 rounded-full px-4 py-1.5 text-xs font-medium text-white transition-transform hover:-translate-y-px active:scale-95"
            style={{ background: danger ? "var(--zc-danger)" : "linear-gradient(135deg,var(--zc-accent-a),var(--zc-accent-b))" }}
        >
            {icon}
            {label}
        </button>
    );
}
