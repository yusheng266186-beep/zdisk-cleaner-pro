import { useEffect, useState } from "react";
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
import { errMsg, MIGRATE_PHASE_LABEL, migrateUndo, planMigration } from "../lib/ipc";
import type { MigratePhaseKey } from "../lib/ipc";
import { humanSize } from "../lib/format";
import { useStore } from "../store";

/** 向导态机：form 填参数 → plan 审计划 → done 收结果。
 *  v5：step/plan/src/dst 全部住进 store —— 切页回来还在；
 *  阶段进度由 App 全局订阅的 store.migratePhase 驱动（不再本页私订）。 */
type Step = "form" | "plan" | "done";

/** 内核五阶段 → 完整句文案（短语版在 MIGRATE_PHASE_LABEL，供侧栏胶囊用） */
const PHASE_LABEL: Record<MigratePhaseKey, string> = {
    copy: "正在复制内容…",
    verify: "尺寸校验中…",
    link: "建立 junction 中…",
    smoke: "冒烟验证中…",
    cleanup: "清理备份中…",
};

export function MigrateCenter() {
    const toast = useStore((s) => s.toast);
    // 雷达页「作为迁移源」跨页联动：进页即消费清除
    const pendingSrc = useStore((s) => s.pendingMigrateSrc);
    const setPendingMigrateSrc = useStore((s) => s.setPendingMigrateSrc);

    const step = useStore((s) => s.migrateStep);
    const setStep = useStore((s) => s.setMigrateStep);
    const src = useStore((s) => s.migrateSrc);
    const dstRoot = useStore((s) => s.migrateDst);
    const setForm = useStore((s) => s.setMigrateForm);
    const plan = useStore((s) => s.migratePlan);
    const setPlan = useStore((s) => s.setMigratePlan);
    const resultId = useStore((s) => s.migrateResultId);
    const setResultId = useStore((s) => s.setMigrateResult);
    const undoMsg = useStore((s) => s.migrateUndoMsg);
    const setUndoMsg = useStore((s) => s.setMigrateUndoMsg);
    // 测量计划是瞬时的页面级请求态（结果 plan 已迁 store，切页保留）
    const [planning, setPlanning] = useState(false);
    const migrateActive = useStore((s) => s.migrateActive);
    const migratePhase = useStore((s) => s.migratePhase);
    const runMigration = useStore((s) => s.runMigration);

    // 执行态以 store 为准：applying = 全局任务在跑且处于计划审阅页
    const applying = migrateActive && step === "plan";
    const currentPhaseLabel = migratePhase ? PHASE_LABEL[migratePhase.phase] : null;

    useEffect(() => {
        if (!pendingSrc) return;
        setForm(pendingSrc, dstRoot);
        setStep("form");
        setPlan(null);
        setResultId(null);
        setUndoMsg(null);
        setPendingMigrateSrc(null); // 进页即消费，回来不重复回填
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [pendingSrc]);

    async function makePlan() {
        if (planning) return;
        if (!src.trim() || !dstRoot.trim()) {
            toast("warn", "请先填写源目录与目标盘根");
            return;
        }
        setPlanning(true);
        try {
            const p = await planMigration(src.trim(), dstRoot.trim());
            setPlan(p);
            setStep("plan");
            setResultId(null);
            setUndoMsg(null);
        } catch (e) {
            toast("err", `无法生成迁移计划：${errMsg(e)}`);
        } finally {
            setPlanning(false);
        }
    }

    async function applyNow() {
        if (applying || !plan) return;
        try {
            // 后端会以当前参数重新 plan 校验，防跨参数篡改。
            // 执行体住 store：切到任何页面都不中断，完成/失败通知全局可达；
            // 阶段事件由 App 全局订阅写进 store.migratePhase，本页只消费。
            const id = await runMigration(src.trim(), dstRoot.trim());
            setResultId(id);
            setUndoMsg(null);
            setStep("done");
        } catch (e) {
            // 内核失败即自动回滚，store 已把失败 toast 送到眼前
            void e;
        }
    }

    async function undoThis() {
        if (!plan || step !== "done" || undoMsg) return;
        try {
            const r = await migrateUndo(plan.src, plan.dst);
            const failNote = r.failed.length ? `，${r.failed.length} 项未能复位` : "";
            const m = `撤销完成：已复位 ${r.restored} 项${failNote}，原路径已脱离 junction`;
            setUndoMsg(m);
            toast(r.failed.length ? "warn" : "ok", "撤销完成，原目录数据已复位");
            void useStore.getState().reloadHistory(); // 迁移历史行随之后端失效刷新
        } catch (e) {
            toast("err", `撤销失败：${errMsg(e)}`);
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
                {migrateActive && (
                    <span
                        className="inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-[11px]"
                        style={{ background: "color-mix(in srgb, var(--zc-accent-b) 12%, transparent)", color: "var(--zc-accent-text)" }}
                    >
                        {migratePhase ? `进行中：${MIGRATE_PHASE_LABEL[migratePhase.phase]}` : "后台任务进行中"}
                    </span>
                )}
            </div>
            <p className="mt-1 text-xs leading-relaxed" style={{ color: "var(--zc-text-3)" }}>
                安全说明：robocopy 搬运后做体积校验；成功前源目录保留为 .old 备份；任何一步失败自动回滚 —— 你的数据不会丢。
                迁移完成后原路径以 junction 继续可用，并写入历史记录，随时可撤销。
            </p>

            {/* 步骤指示 */}
            <div className="mt-4 flex items-center gap-2 text-[11px]" style={{ color: "var(--zc-text-3)" }}>
                {(["form", "plan", "done"] as Step[]).map((s, i) => (
                    <span key={s} className="flex items-center gap-2">
                        {i > 0 && <span aria-hidden>—</span>}
                        <span
                            className={step === s ? "font-medium" : undefined}
                            style={{ color: step === s ? "var(--zc-accent-text)" : undefined }}
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
                                onChange={(e) => setForm(e.target.value, dstRoot)}
                                placeholder={String.raw`C:\Users\you\AppData\Local\npm-cache`}
                                spellCheck={false}
                                className="num w-full rounded-lg border px-3 py-2 text-sm outline-none transition-colors focus:border-[var(--zc-accent-b)]"
                                style={{ background: "var(--zc-surface-2)", borderColor: "var(--zc-border-strong)", color: "var(--zc-text-1)" }}
                            />
                        </Field>
                        <Field label="目标盘根" hint="目标盘符根目录（不含末段目录名），迁移时会自动拼上源目录名">
                            <input
                                value={dstRoot}
                                onChange={(e) => setForm(src, e.target.value)}
                                placeholder={"E:\\"}
                                spellCheck={false}
                                className="num w-full rounded-lg border px-3 py-2 text-sm outline-none transition-colors focus:border-[var(--zc-accent-b)]"
                                style={{ background: "var(--zc-surface-2)", borderColor: "var(--zc-border-strong)", color: "var(--zc-text-1)" }}
                            />
                        </Field>
                        <button
                            onClick={() => void makePlan()}
                            disabled={planning || migrateActive}
                            className="zc-press mt-4 flex items-center gap-2 rounded-lg px-4 py-2 text-sm font-medium transition-transform active:scale-95 disabled:cursor-not-allowed disabled:opacity-50"
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
                            <FolderOutput size={15} style={{ color: "var(--zc-accent-text)" }} />
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
                                className="zc-press rounded-lg border px-4 py-2 text-sm font-medium transition-transform active:scale-95 disabled:cursor-not-allowed disabled:opacity-60"
                                style={{
                                    borderColor: "color-mix(in srgb, var(--zc-danger) 55%, transparent)",
                                    background: applying ? "transparent" : "color-mix(in srgb, var(--zc-danger) 18%, transparent)",
                                    color: applying ? "var(--zc-text-2)" : "var(--zc-danger-text)",
                                }}
                            >
                                {applying ? "迁移中…（robocopy 多线程搬运）" : "确认执行迁移"}
                            </button>
                            <button
                                onClick={() => (applying ? undefined : setStep("form"))}
                                disabled={applying}
                                className="zc-press flex items-center gap-1 rounded-lg border px-4 py-2 text-sm transition-colors hover:opacity-75 disabled:cursor-not-allowed disabled:opacity-40"
                                style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-text-2)" }}
                            >
                                <ArrowLeft size={13} /> 返回修改
                            </button>
                        </div>

                        {/* 执行等待区：store.migratePhase 真实阶段推送（切页回来还在，非估算进度） */}
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
                                    阶段来自内核真实步骤推送，切页不中断、回来还在
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
                                disabled={migrateActive}
                                className="zc-press mt-4 flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs transition-colors hover:opacity-75 disabled:cursor-not-allowed disabled:opacity-40"
                                style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-text-2)" }}
                            >
                                <Undo2 size={12} /> {migrateActive ? "任务进行中…" : "撤销本次迁移"}
                            </button>
                        )}
                        <div className="mt-5 flex gap-2">
                            <button
                                onClick={() => useStore.getState().resetMigrateWizard()}
                                className="zc-press rounded-lg border px-3 py-1.5 text-xs transition-colors hover:opacity-75"
                                style={{ borderColor: "var(--zc-border-strong)", color: "var(--zc-accent-text)" }}
                            >
                                开始新的迁移
                            </button>
                        </div>
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
