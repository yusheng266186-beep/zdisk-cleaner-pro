import { useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { Archive, ChevronDown, RotateCcw, ScanSearch, Trash2 } from "lucide-react";
import { RiskBadge } from "../components/RiskBadge";
import { cascade, pageVariants, springSnappy } from "../lib/motion";
import { selectableFindings, useStore, cleanableBytes } from "../store";
import { humanSize } from "../lib/format";
import { DOMAIN_ZH } from "../lib/types";
import { useArm } from "./useArmEsc";

/** 体检结果页（v5 U1 修复）：
 *  - 侧栏「体检结果」再入项（report 存在即出现，App 侧控制）；
 *  - 执行统一两段式：任何档位都要 results-exec 确认这一步；
 *  - 非 safe 行默认禁用勾选，展开明细后才解锁（兑现设置页「UI 强制」承诺）；
 *  - 零命中空态 + 重新扫描。 */
export function Results() {
    const report = useStore((s) => s.report);
    const rules = useStore((s) => s.rules);
    const selection = useStore((s) => s.selection);
    const toggleSelect = useStore((s) => s.toggleSelect);
    const selectSafeOnly = useStore((s) => s.selectSafeOnly);
    const selectAll = useStore((s) => s.selectAll);
    const clearSelection = useStore((s) => s.clearSelection);
    const cleanSelected = useStore((s) => s.cleanSelected);
    const expandedRule = useStore((s) => s.expandedRule);
    const setExpanded = useStore((s) => s.setExpanded);
    const startScan = useStore((s) => s.startScan);

    // 暂存区 / 回收站模式选择（默认暂存区：7 天可还原）
    const [mode, setMode] = useState<"vault" | "recycle_bin">("vault");
    // 两段式执行确认：点一次 armed「确认清理 N 项」，4s 超时回退
    const { armed, arm, disarm } = useArm(4000);

    const findings = [...selectableFindings(report)].sort(
        (a, b) => bytesOf(b) - bytesOf(a),
    );

    const selectedBytes = report
        ? (report.findings ?? [])
              .filter((f) => selection.has(f.rule_id))
              .reduce((a, f) => a + bytesOf(f), 0)
        : 0;
    const totalBytes = cleanableBytes(report ?? { findings: [] } as never);

    function exec() {
        if (selection.size === 0) {
            // 空勾选直接走 store 守卫 toast「请至少选择一条规则」，不进入 armed
            void cleanSelected(mode);
            return;
        }
        if (armed) {
            disarm();
            void cleanSelected(mode);
        } else {
            arm();
        }
    }

    /* ── 零命中空态 / 无报告兜底 ── */
    if (!report || findings.length === 0) {
        return (
            <motion.div variants={pageVariants} initial="initial" animate="animate" exit="exit" className="mx-auto max-w-3xl">
                <h1 className="text-xl font-semibold">体检结果</h1>
                <div
                    className="mt-6 flex flex-col items-center rounded-xl border px-6 py-14"
                    style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }}
                >
                    <ScanSearch size={22} style={{ color: "var(--zc-text-3)" }} />
                    <p className="mt-3 text-sm" style={{ color: "var(--zc-text-2)" }}>
                        {report ? "本次扫描没有发现可清理项 —— 磁盘很干净" : "报告已失效（清理完成或扫描已重启）"}
                    </p>
                    <button
                        onClick={() => void startScan()}
                        className="zc-press mt-5 flex items-center gap-2 rounded-full border px-5 py-2 text-sm transition-colors hover:opacity-80"
                        style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-accent-text)" }}
                    >
                        <RotateCcw size={14} /> 重新扫描
                    </button>
                </div>
            </motion.div>
        );
    }

    return (
        <motion.div variants={pageVariants} initial="initial" animate="animate" exit="exit" className="mx-auto max-w-3xl pb-28">
            <div className="mb-5 flex items-end justify-between">
                <div>
                    <h1 className="text-xl font-semibold">体检结果</h1>
                    <p className="mt-0.5 text-xs" style={{ color: "var(--zc-text-3)" }}>
                        共发现 {humanSize(totalBytes)} 可清理 · 按体积降序 · 已勾选{" "}
                        <b style={{ color: "var(--zc-accent-text)" }}>{humanSize(selectedBytes)}</b>
                    </p>
                </div>
                <div className="flex gap-2 text-xs">
                    <ChipBtn onClick={selectSafeOnly}>只选安全</ChipBtn>
                    <ChipBtn onClick={selectAll}>全选</ChipBtn>
                    <ChipBtn onClick={clearSelection}>清空勾选</ChipBtn>
                </div>
            </div>

            <ul className="flex flex-col gap-2.5">
                {findings.map((f, i) => {
                    const meta = rules.find((r) => r.id === f.rule_id);
                    const checked = selection.has(f.rule_id);
                    const open = expandedRule === f.rule_id;
                    const bytes = bytesOf(f);
                    // 「UI 强制」承诺：注意及以上档位必须展开明细确认可还原后才允许勾选
                    const risk = meta?.risk ?? "caution";
                    const checkAllowed = risk === "safe" || open;
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
                                    {/* 勾选框：非 safe 且未展开 → 禁用 */}
                                    <button
                                        onClick={() => checkAllowed && toggleSelect(f.rule_id)}
                                        disabled={!checkAllowed}
                                        className="grid h-5 w-5 shrink-0 place-items-center rounded-md border transition-all disabled:cursor-not-allowed disabled:opacity-35"
                                        style={{
                                            borderColor: checked ? "transparent" : "var(--zc-border-strong)",
                                            background: checked ? "linear-gradient(135deg,var(--zc-accent-a),var(--zc-accent-b))" : "transparent",
                                        }}
                                        aria-pressed={checked}
                                        title={checkAllowed ? undefined : "注意及以上档位：展开明细确认后才可勾选"}
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
                                            {risk !== "safe" && !open && (
                                                <span className="text-[10px]" style={{ color: "var(--zc-text-3)" }}>展开明细后可勾选</span>
                                            )}
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
                                                        <li key={h.path || j} className="num truncate py-1 font-mono text-[11px]" style={{ color: "var(--zc-text-2)" }} title={h.path}>
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

            {/* 底部操作条：常驻（选择/执行分离）；z-40 低于清理遮罩 z-[55]，清理期间被盖住 */}
            <motion.div
                initial={{ y: 90 }}
                animate={{ y: 0, transition: springSnappy }}
                className="fixed bottom-5 left-1/2 z-40 -translate-x-1/2"
            >
                <div
                    className="flex items-center gap-2.5 rounded-full border py-2 pl-5 pr-2"
                    style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border-strong)", boxShadow: "var(--zc-shadow-pop)" }}
                >
                    <RotateCcw size={14} style={{ color: "var(--zc-ok)" }} />
                    <span className="text-sm">
                        清理 <b>{selection.size}</b> 条 ·{" "}
                        <b className="num">{humanSize(selectedBytes)}</b>
                    </span>
                    {/* 去向模式：暂存区（7 天可还原）/ 回收站 */}
                    <div className="flex items-center gap-1 rounded-full p-0.5" style={{ background: "var(--zc-surface-2)" }}>
                        <ModeChip active={mode === "vault"} onClick={() => { setMode("vault"); disarm(); }} icon={<Archive size={12} />}>
                            暂存区
                        </ModeChip>
                        <ModeChip active={mode === "recycle_bin"} onClick={() => { setMode("recycle_bin"); disarm(); }} icon={<Trash2 size={12} />}>
                            回收站
                        </ModeChip>
                    </div>
                    <button
                        data-testid="results-exec"
                        onClick={exec}
                        className="zc-press flex shrink-0 items-center gap-1.5 rounded-full px-4 py-1.5 text-xs font-medium transition-colors"
                        style={{
                            background: armed
                                ? "color-mix(in srgb, var(--zc-danger) 18%, transparent)"
                                : "linear-gradient(135deg,var(--zc-accent-a),var(--zc-accent-b))",
                            color: armed ? "var(--zc-danger-text)" : "#ffffff",
                        }}
                        title={mode === "vault" ? "移入暂存区：7 天内可一键还原" : "移入回收站：清空回收站前不会真正释放磁盘空间"}
                    >
                        {armed ? `确认清理 ${selection.size} 项` : `清理 ${selection.size} 项`}
                    </button>
                </div>
            </motion.div>
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
            className="rounded-lg border px-2.5 py-1 transition-colors hover:bg-[var(--zc-hover)]"
            style={{ borderColor: "var(--zc-border)", color: "var(--zc-text-2)" }}
        >
            {children}
        </button>
    );
}

function ModeChip({ active, onClick, icon, children }: {
    active: boolean; onClick: () => void; icon: React.ReactNode; children: React.ReactNode;
}) {
    return (
        <button
            onClick={onClick}
            className="flex items-center gap-1 rounded-full px-2.5 py-1 text-[11px] font-medium transition-colors"
            style={{
                background: active ? "var(--zc-surface-1)" : "transparent",
                color: active ? "var(--zc-text-1)" : "var(--zc-text-3)",
                boxShadow: active ? "var(--zc-shadow-1)" : "none",
            }}
            aria-pressed={active}
        >
            {icon}
            {children}
        </button>
    );
}
