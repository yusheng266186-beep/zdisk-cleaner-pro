import { useState } from "react";
import {
    ArrowLeft,
    ArrowRight,
    CheckCircle2,
    FolderOutput,
    ShieldCheck,
    Undo2,
} from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { pageVariants, springSnappy } from "../lib/motion";
import type { MigrationPlan, MigratePhaseKey } from "../lib/ipc";
import * as ipc from "../lib/ipc";
import { humanSize } from "../lib/format";
import { useStore } from "../store";

/** 向导态机：form 填参数 → plan 审计划 → done 收结果 */
type Step = "form" | "plan" | "done";

/** 内核五阶段 → 当前阶段文案（与 zclean CLI 的 [n/5] 行一一对应） */
const PHASE_LABEL: Record<MigratePhaseKey, string> = {
    copy: "正在复制内容…",
    verify: "尺寸校验中…",
    link: "建立 junction 中…",
    smoke: "冒烟验证中…",
    cleanup: "清理备份中…",
};

const msgOf = (e: unknown): string => (e instanceof Error ? e.message : String(e));

export function MigrateCenter() {
    const toast = useStore((s) => s.toast);

    const [step, setStep] = useState<Step>("form");
    const [src, setSrc] = useState("");
    const [dstRoot, setDstRoot] = useState("");

    const [plan, setPlan] = useState<MigrationPlan | null>(null);
    const [planning, setPlanning] = useState(false);
    const [applying, setApplying] = useState(false);
    const [currentPhaseLabel, setCurrentPhaseLabel] = useState<string | null>(null);

    const [resultId, setResultId] = useState<string | null>(null);
    const [undoing, setUndoing] = useState(false);
    const [undoMsg, setUndoMsg] = useState<string | null>(null);

    async function makePlan() {
        if (planning) return;
        if (!src.trim() || !dstRoot.trim()) {
            toast("warn", "请先填写源目录与目标盘根");
            return;
        }
        setPlanning(true);
        try {
            const p = await ipc.planMigration(src.trim(), dstRoot.trim());
            setPlan(p);
            setStep("plan");
        } catch (e) {
            toast("err", `无法生成迁移计划：${msgOf(e)}`);
        } finally {
            setPlanning(false);
        }
    }

    async function applyNow() {
        if (applying || !plan) return;
        setApplying(true);
        setCurrentPhaseLabel(null);
        let unlisten: (() => void) | undefined;
        try {
            // 先订阅内核真实阶段事件，再发起迁移 —— Start/End 推送驱动文案条
            unlisten = await ipc.onMigratePhase((p) => setCurrentPhaseLabel(PHASE_LABEL[p.phase]));
            // 后端会以当前参数重新 plan 校验，防跨参数篡改
            const id = await ipc.applyMigration(src.trim(), dstRoot.trim());
            setResultId(id);
            setUndoMsg(null);
            setStep("done");
            toast("ok", "迁移完成：原路径已通过 junction 保持可用");
        } catch (e) {
            // 内核失败即自动回滚，消息里带回滚结论
            toast("err", msgOf(e));
        } finally {
            unlisten?.();
            setCurrentPhaseLabel(null);
            setApplying(false);
        }
    }

    async function undoThis() {
        if (undoing || !plan || step !== "done") return;
        setUndoing(true);
        try {
            const m = await ipc.undoMigration(plan.src);
            setUndoMsg(m);
            toast("ok", "撤销完成，原目录数据已复位");
        } catch (e) {
            toast("err", `撤销失败：${msgOf(e)}`);
        } finally {
            setUndoing(false);
        }
    }

    return (
        <motion.div variants={pageVariants} initial="initial" animate="animate" exit="exit" className="mx-auto max-w-3xl">
            <div className="flex items-center gap-3">
                <h1 className="text-xl font-semibold">存储迁移中心</h1>
                <span
                    className="inline-flex items-center gap-1 rounded-full px-2.5 py-0.5 text-[11px]"
                    style={{ background: "color-mix(in srgb, var(--zc-ok) 12%, transparent)", color: "var(--zc-ok)" }}
                >
                    <ShieldCheck size={11} /> 数据无损
                </span>
            </div>
            <p className="mt-1 text-xs leading-relaxed" style={{ color: "var(--zc-text-3)" }}>
                安全说明：robocopy 搬运后做体积校验；成功前源目录保留为 .old 备份；任何一步失败自动回滚 —— 你的数据不会丢。
                迁移完成后原路径以 junction 继续可用。
            </p>

            {/* 步骤指示 */}
            <div className="mt-4 flex items-center gap-2 text-[11px]" style={{ color: "var(--zc-text-3)" }}>
                {(["form", "plan", "done"] as Step[]).map((s, i) => (
                    <span key={s} className="flex items-center gap-2">
                        {i > 0 && <span aria-hidden>—</span>}
                        <span
                            className={step === s ? "font-medium" : undefined}
                            style={{ color: step === s ? "var(--zc-accent-b)" : undefined }}
                        >
                            {s === "form" ? "① 填写路径" : s === "plan" ? "② 审阅计划" : "③ 完成"}
                        </span>
                    </span>
                ))}
            </div>

            <AnimatePresence mode="wait">
                {step === "form" && (
                    <motion.section
                        key="form"
                        initial={{ opacity: 0, y: 10 }}
                        animate={{ opacity: 1, y: 0 }}
                        exit={{ opacity: 0, y: -8, transition: springSnappy }}
                        className="mt-4 rounded-xl border p-5"
                        style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }}
                    >
                        <Field label="源目录" hint="要搬走的大型目录（如 D:\\Games\\StarRail 或 npm 缓存）">
                            <input
                                value={src}
                                onChange={(e) => setSrc(e.target.value)}
                                placeholder={String.raw`C:\Users\you\AppData\Local\npm-cache`}
                                spellCheck={false}
                                className="num w-full rounded-lg border px-3 py-2 text-sm outline-none transition-colors focus:border-[var(--zc-accent-b)]"
                                style={{ background: "var(--zc-surface-2)", borderColor: "var(--zc-border-strong)", color: "var(--zc-text-1)" }}
                            />
                        </Field>
                        <Field label="目标盘根" hint="目标盘符根目录（不含末段目录名），迁移时会自动拼上源目录名">
                            <input
                                value={dstRoot}
                                onChange={(e) => setDstRoot(e.target.value)}
                                placeholder={"E:\\"}
                                spellCheck={false}
                                className="num w-full rounded-lg border px-3 py-2 text-sm outline-none transition-colors focus:border-[var(--zc-accent-b)]"
                                style={{ background: "var(--zc-surface-2)", borderColor: "var(--zc-border-strong)", color: "var(--zc-text-1)" }}
                            />
                        </Field>
                        <button
                            onClick={() => void makePlan()}
                            disabled={planning}
                            className="mt-4 flex items-center gap-2 rounded-lg px-4 py-2 text-sm font-medium transition-transform active:scale-95 disabled:cursor-not-allowed disabled:opacity-50"
                            style={{ background: "linear-gradient(135deg,var(--zc-accent-a),var(--zc-accent-b))", color: "#ffffff" }}
                        >
                            {planning ? "正在测量…" : "生成迁移计划"} {!planning && <ArrowRight size={14} />}
                        </button>
                    </motion.section>
                )}

                {step === "plan" && plan && (
                    <motion.section
                        key="plan"
                        initial={{ opacity: 0, y: 10 }}
                        animate={{ opacity: 1, y: 0 }}
                        exit={{ opacity: 0, y: -8, transition: springSnappy }}
                        className="mt-4 rounded-xl border p-5"
                        style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }}
                    >
                        <div className="flex flex-wrap items-center gap-2 text-sm">
                            <FolderOutput size={15} style={{ color: "var(--zc-accent-b)" }} />
                            <span className="num break-all" title={plan.src}>{plan.src}</span>
                            <ArrowRight size={13} style={{ color: "var(--zc-text-3)" }} />
                            <span className="num break-all" title={plan.dst}>{plan.dst}</span>
                        </div>
                        <div className="mt-4 grid grid-cols-2 gap-3 sm:max-w-sm">
                            <Stat label="总体积" value={humanSize(plan.total_bytes)} />
                            <Stat label="文件数" value={`${plan.total_files.toLocaleString("en-US")} 个`} />
                        </div>
                        <p className="mt-4 text-[11px]" style={{ color: "var(--zc-warn)" }}>
                            执行期间请关闭占用该目录的程序（游戏、包管理器等），否则 robocopy 重试一次后放弃并回滚。
                        </p>
                        <div className="mt-4 flex flex-wrap items-center gap-3">
                            <button
                                onClick={() => void applyNow()}
                                disabled={applying}
                                className="rounded-lg border px-4 py-2 text-sm font-medium transition-transform active:scale-95 disabled:cursor-not-allowed disabled:opacity-60"
                                style={{
                                    borderColor: "color-mix(in srgb, var(--zc-danger) 55%, transparent)",
                                    background: applying ? "transparent" : "color-mix(in srgb, var(--zc-danger) 18%, transparent)",
                                    color: "var(--zc-danger)",
                                }}
                            >
                                {applying ? "迁移中…（robocopy 多线程搬运）" : "确认执行迁移"}
                            </button>
                            <button
                                onClick={() => (applying ? undefined : setStep("form"))}
                                disabled={applying}
                                className="flex items-center gap-1 rounded-lg border px-4 py-2 text-sm transition-colors hover:opacity-75 disabled:cursor-not-allowed disabled:opacity-40"
                                style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-text-2)" }}
                            >
                                <ArrowLeft size={13} /> 返回修改
                            </button>
                        </div>

                        {/* 执行等待区：内核真实阶段推送（非估算进度） */}
                        {applying && (
                            <div
                                className="mt-4 flex items-center gap-2.5 rounded-lg border px-3 py-2.5 text-xs"
                                style={{ background: "var(--zc-surface-2)", borderColor: "var(--zc-border-strong)" }}
                            >
                                <motion.span
                                    aria-hidden
                                    animate={{ opacity: [1, 0.2, 1] }}
                                    transition={{ repeat: Infinity, duration: 1.5, ease: "easeInOut" }}
                                    className="h-2 w-2 shrink-0 rounded-full"
                                    style={{ background: "linear-gradient(135deg,var(--zc-accent-a),var(--zc-accent-b))" }}
                                />
                                <motion.span
                                    key={currentPhaseLabel ?? "booting"}
                                    className="num"
                                    animate={{ opacity: [0.55, 1, 0.55] }}
                                    transition={{ repeat: Infinity, duration: 1.6, ease: "easeInOut" }}
                                    style={{ color: "var(--zc-text-1)" }}
                                >
                                    {currentPhaseLabel ?? "正在启动迁移…"}
                                </motion.span>
                                <span className="ml-auto" style={{ color: "var(--zc-text-3)" }}>
                                    阶段来自内核真实步骤推送，非估算进度
                                </span>
                            </div>
                        )}
                    </motion.section>
                )}

                {step === "done" && plan && resultId && (
                    <motion.section
                        key="done"
                        initial={{ opacity: 0, y: 10 }}
                        animate={{ opacity: 1, y: 0 }}
                        exit={{ opacity: 0, y: -8, transition: springSnappy }}
                        className="mt-4 rounded-xl border p-5"
                        style={{ background: "var(--zc-surface-1)", borderColor: "var(--zc-border)" }}
                    >
                        <div className="flex items-center gap-2 text-sm" style={{ color: "var(--zc-ok)" }}>
                            <CheckCircle2 size={16} />
                            迁移完成 —— 原路径仍可正常访问（junction 直通新位置）。
                        </div>
                        <div className="mt-4 text-xs" style={{ color: "var(--zc-text-3)" }}>
                            迁移清单 id：<span className="num" style={{ color: "var(--zc-text-1)" }}>{resultId}</span>
                            （已落盘于 %LOCALAPPDATA%\ZDiskCleanerPro3\migrations）
                        </div>
                        {undoMsg ? (
                            <div className="mt-4 rounded-lg border p-3 text-xs" style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-ok)" }}>
                                {undoMsg}
                            </div>
                        ) : (
                            <button
                                onClick={() => void undoThis()}
                                disabled={undoing}
                                className="mt-4 flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs transition-colors hover:opacity-75 disabled:cursor-not-allowed disabled:opacity-40"
                                style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-text-2)" }}
                            >
                                <Undo2 size={12} /> {undoing ? "撤销中…" : "撤销本次迁移"}
                            </button>
                        )}
                    </motion.section>
                )}
            </AnimatePresence>
        </motion.div>
    );
}

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
    return (
        <label className="mb-4 block last:mb-0">
            <span className="text-sm">{label}</span>
            {hint && <span className="mt-0.5 block text-[11px]" style={{ color: "var(--zc-text-3)" }}>{hint}</span>}
            <span className="mt-2 block">{children}</span>
        </label>
    );
}

function Stat({ label, value }: { label: string; value: string }) {
    return (
        <div className="rounded-lg border px-3 py-2.5" style={{ background: "var(--zc-surface-2)", borderColor: "var(--zc-border)" }}>
            <div className="text-[10px] uppercase tracking-wide" style={{ color: "var(--zc-text-3)" }}>{label}</div>
            <div className="num mt-1 text-base font-semibold">{value}</div>
        </div>
    );
}
