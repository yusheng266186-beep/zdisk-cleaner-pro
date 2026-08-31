/** 全局状态机：idle → scanning → results → cleaning → idle(+报告横幅)
 *  v5（CONTRACT-v5 §5）：既有字段名一个不改（QA 读 __zcStore）；
 *  新增 initError / busyRunning / homeAdmin / migratePhase + 迁移向导与雷达根的承载字段。 */
import { create } from "zustand";
import type { CleanOutcome, HistoryRecord, RuleMeta, ScanReport } from "./lib/types";
import { totalHits } from "./lib/types";
import * as ipc from "./lib/ipc";
import { humanSize } from "./lib/format";

export interface Toast {
    id: number;
    kind: "ok" | "warn" | "err" | "info";
    msg: string;
}

export type Phase = "idle" | "scanning" | "results" | "cleaning";

/** 侧栏导航页（自 App.tsx 提升进 store，供任意页面跨页跳转） */
export type Page = "home" | "results" | "history" | "tools" | "deeptools" | "startup" | "migrate" | "radar" | "bigfiles" | "dupes" | "settings";

/** 迁移向导态（自 MigrateCenter 本地 state 迁入：切页不丢进度） */
export type MigrateStep = "form" | "plan" | "done";

/** 上次清理战报的 localStorage 载体：刷新不丢「反悔」入口 */
const BANNER_KEY = "zc-clean-banner-v5";
interface BannerPersist {
    sessionId: string;
    outcome: CleanOutcome;
    /** 摘要（id + bytes），仅作展示与调试对账用 */
    bytes: number;
}

function readBanner(): BannerPersist | null {
    try {
        const raw = localStorage.getItem(BANNER_KEY);
        if (!raw) return null;
        const b = JSON.parse(raw) as BannerPersist;
        if (!b || !b.outcome || typeof b.sessionId !== "string") return null;
        return b;
    } catch {
        try { localStorage.removeItem(BANNER_KEY); } catch { /* 无痕模式等，忽略 */ }
        return null;
    }
}

interface StoreState {
    demo: boolean;
    version: string;
    appVersion: string;
    theme: "dark" | "light";

    activePage: Page;
    phase: Phase;
    scanFiles: number;
    scanBytes: number;
    report: ScanReport | null;
    selection: Set<string>;
    expandedRule: string | null;

    cleanOutcome: CleanOutcome | null;
    lastSessionId: string | null;

    history: HistoryRecord[];
    rules: RuleMeta[];
    drives: ipc.DriveInfo[];

    paletteOpen: boolean;
    toasts: Toast[];

    /** 雷达页「作为迁移源」的暂存路径：非空时迁移中心表单预填 */
    pendingMigrateSrc: string | null;

    /** v5 · init() 失败原因（App 顶部 init-error 横幅消费；不再静默半残） */
    initError: string | null;
    /** v5 · 忙任务（big_files/find_dupes/analyze_tree）是否运行中，busy-cancel 按钮依据 */
    busyRunning: boolean;
    /** v5 · 体检台「包含系统级项目（需管理员）」开关，startScan 传值 */
    homeAdmin: boolean;
    /** v5 · 内核迁移阶段事件（App 全局订阅一次；侧栏胶囊与迁移中心进度条消费） */
    migratePhase: ipc.MigratePhaseEvent | null;

    /** v5 · 迁移向导承载（原 MigrateCenter 本地 state，切页保留） */
    migrateStep: MigrateStep;
    migrateSrc: string;
    migrateDst: string;
    migratePlan: ipc.MigrationPlan | null;
    migrateResultId: string | null;
    migrateUndoMsg: string | null;

    /** v5 · 雷达分析根：null/"" = 主目录；体检台磁盘环点击预填 */
    radarRootPath: string | null;

    init: () => Promise<void>;
    setActivePage: (p: Page) => void;
    setPendingMigrateSrc: (v: string | null) => void;
    toggleTheme: () => void;
    startScan: () => Promise<void>;
    cancelScan: () => void;
    /** v5 · 取消忙任务（big_files/find_dupes/analyze_tree） */
    cancelBusy: () => Promise<void>;
    setBusyRunning: (b: boolean) => void;
    setHomeAdmin: (b: boolean) => void;
    clearInitError: () => void;
    setRadarRoot: (p: string | null) => void;
    /** 重拉台账历史（迁移撤销等写命令后由页面调用） */
    reloadHistory: () => Promise<void>;
    /** 重拉磁盘概览（清空回收站等释放空间后刷新读数） */
    refreshDrives: () => Promise<void>;
    toggleSelect: (id: string) => void;
    selectSafeOnly: () => void;
    selectAll: () => void;
    clearSelection: () => void;
    cleanSelected: (mode: "vault" | "recycle_bin") => Promise<void>;
    undoSession: (id: string) => Promise<void>;
    undoLast: () => Promise<void>;
    purgeSession: (id: string) => Promise<void>;
    /** 手动安全删除（大文件/重复文件/雷达/工具箱共用）：守卫+暂存区+台账，可还原 */
    manualDelete: (paths: string[]) => Promise<void>;
    /** 迁移全局任务：跨页存活，完成后无论在哪个页面都弹通知 */
    migrateActive: boolean;
    migrateInfo: { src: string; dst: string } | null;
    runMigration: (src: string, dst: string) => Promise<string>;
    setMigrateStep: (s: MigrateStep) => void;
    setMigrateForm: (src: string, dst: string) => void;
    setMigratePlan: (p: ipc.MigrationPlan | null) => void;
    setMigrateResult: (id: string | null) => void;
    setMigrateUndoMsg: (m: string | null) => void;
    resetMigrateWizard: () => void;
    setExpanded: (id: string | null) => void;
    togglePalette: (open?: boolean) => void;
    toast: (kind: Toast["kind"], msg: string) => void;
}

let toastSeq = 1;

/** 统一错误文案提取：ZcError 也是 Error 子类 */
const msgOf = (e: unknown): string => (e instanceof Error ? e.message : String(e));

export const useStore = create<StoreState>((set, get) => ({
    demo: !ipc.isDesktop(),
    version: "",
    appVersion: "",
    theme: (localStorage.getItem("zc-theme") as "dark" | null) ?? "dark",

    activePage: "home",
    phase: "idle",
    scanFiles: 0,
    scanBytes: 0,
    report: null,
    selection: new Set(),
    expandedRule: null,

    cleanOutcome: null,
    lastSessionId: null,

    history: [],
    rules: [],
    drives: [],

    paletteOpen: false,
    toasts: [],
    pendingMigrateSrc: null,
    migrateActive: false,
    migrateInfo: null,

    initError: null,
    busyRunning: false,
    homeAdmin: false,
    migratePhase: null,

    migrateStep: "form",
    migrateSrc: "",
    migrateDst: "",
    migratePlan: null,
    migrateResultId: null,
    migrateUndoMsg: null,

    radarRootPath: null,

    async init() {
        document.documentElement.dataset.theme = get().theme;
        set({ initError: null });
        try {
            const [version, appVer, rules, drives, history] = await Promise.all([
                ipc.coreVersion(),
                ipc.appVersion(),
                ipc.loadRuleMeta(),
                ipc.listDrives(),
                ipc.loadHistory(),
            ]);
            set({ version, appVersion: appVer, rules, drives, history });
        } catch (e) {
            // 失败不再静默半残：置 initError，App 顶部亮横幅 + 可重试
            set({ initError: msgOf(e) });
        }
        // 战报横幅持久化回放：刷新后「反悔 · 一键还原本批」入口不丢
        const banner = readBanner();
        if (banner && !get().cleanOutcome) {
            set({ cleanOutcome: banner.outcome, lastSessionId: banner.sessionId });
        }
    },

    setActivePage: (p) => set({ activePage: p }),
    setPendingMigrateSrc: (v) => set({ pendingMigrateSrc: v }),
    setHomeAdmin: (b) => set({ homeAdmin: b }),
    setBusyRunning: (b) => set({ busyRunning: b }),
    clearInitError: () => set({ initError: null }),
    setRadarRoot: (p) => set({ radarRootPath: p }),
    reloadHistory: () => refreshHistory(set),
    async refreshDrives() {
        try {
            set({ drives: await ipc.listDrives() });
        } catch { /* 保留旧读数 */ }
    },

    toggleTheme() {
        const theme = get().theme === "dark" ? "light" : "dark";
        localStorage.setItem("zc-theme", theme);
        document.documentElement.dataset.theme = theme;
        set({ theme });
    },

    async startScan() {
        const { phase } = get();
        // 竞态守卫：scanning/cleaning 中不可再触发；results 页（含命令面板再扫描）
        // 先回体检台，避免结果页停留渲染「共 0 B」空壳
        if (phase === "scanning" || phase === "cleaning") return;
        if (phase === "results") set({ activePage: "home" });
        set({ phase: "scanning", scanFiles: 0, scanBytes: 0, report: null, selection: new Set(), cleanOutcome: null });
        try {
            await ipc.startScan(
                (files, bytes) => set({ scanFiles: files, scanBytes: bytes }),
                (report) => {
                    // 默认只勾选安全档，且剔除零命中
                    const safe = new Set(
                        report.findings
                            .filter((f) => totalHits(f) > 0)
                            .filter((f) => get().rules.find((r) => r.id === f.rule_id)?.risk === "safe")
                            .map((f) => f.rule_id),
                    );
                    set({ phase: "results", report, selection: safe });
                    const bytes = cleanableBytes(report);
                    const count = safe.size;
                    get().toast("info", `扫描完成：${humanSize(bytes)} 待清理 · 勾选 ${count} 条安全规则`);
                },
                get().homeAdmin,
            );
        } catch (e) {
            // 扫描失败绝不静默卡在 0：回 idle 并把原因抛给用户
            set({ phase: "idle" });
            get().toast("err", `扫描失败：${msgOf(e)}`);
        }
    },

    cancelScan() {
        void ipc.cancelScan();
        set({ phase: "idle" });
        get().toast("info", "已请求取消扫描");
    },

    async cancelBusy() {
        try {
            await ipc.cancelBusy();
        } catch (e) {
            get().toast("err", `取消失败：${msgOf(e)}`);
        }
    },

    toggleSelect(id) {
        const s = new Set(get().selection);
        if (s.has(id)) { s.delete(id); } else { s.add(id); }
        set({ selection: s });
    },

    selectSafeOnly() {
        const safe = new Set(
            (get().report?.findings ?? [])
                .filter((f) => get().rules.find((r) => r.id === f.rule_id)?.risk === "safe")
                .map((f) => f.rule_id),
        );
        set({ selection: safe });
    },

    /** 全选所有有命中的规则（非安全档仍需展开明细才允许手动勾选；执行统一两段式确认） */
    selectAll() {
        const all = new Set(
            (get().report?.findings ?? [])
                .filter((f) => totalHits(f) > 0)
                .map((f) => f.rule_id),
        );
        set({ selection: all });
    },

    clearSelection: () => set({ selection: new Set() }),

    async cleanSelected(mode) {
        const { report, selection } = get();
        if (!report || selection.size === 0) {
            get().toast("warn", "请至少选择一条规则");
            return;
        }
        set({ phase: "cleaning" });
        try {
            // v5：删除人为 1.2s 垫时，遮罩时长 = 真实执行时长
            const outcome = await ipc.cleanSelected(report, [...selection], mode);
            const histEntry: HistoryRecord = {
                // 真实台账 ID(report.id)：历史页的还原/彻底删除按钮按它调后端。
                // 后端 clean_selected 已用同一 ID 写过台账 history 行，这里 upsert 同一行；
                // 此前的 `${files}-${Date.now()}` 假 ID 会让第一行的还原/彻底删除永远报「台账不存在」。
                session_id: report.id,
                created_unix: Date.now() / 1000,
                mode,
                files: outcome.done_files,
                bytes_moved: outcome.done_bytes,
            };
            set({
                phase: "idle",
                // 结果页此刻已过期（文件已搬走）：清掉并回体检台看完成战报
                report: null,
                cleanOutcome: outcome,
                lastSessionId: report.id,
                history: [histEntry, ...get().history],
                selection: new Set(),
                activePage: "home",
            });
            try {
                const banner: BannerPersist = { sessionId: report.id, outcome, bytes: outcome.done_bytes };
                localStorage.setItem(BANNER_KEY, JSON.stringify(banner));
            } catch { /* 存储不可用：横幅退化为内存态，不影响主流程 */ }
            get().toast(outcome.failed.length ? "warn" : "ok", outcome.semantics_note);
        } catch (e) {
            // 失败绝不能把执行遮罩卡死：退回结果页并把原因亮出来
            set({ phase: get().report ? "results" : "idle" });
            get().toast("err", `清理失败：${msgOf(e)}`);
        }
    },

    async undoSession(id: string) {
        try {
            const r = await ipc.undoSession(id);
            const failNote = r.failed.length ? `，${r.failed.length} 项未能还原` : "";
            set({ cleanOutcome: null });
            dismissBanner(id);
            get().toast(r.failed.length ? "warn" : "ok", `已还原 ${r.done} 项 · ${humanSize(r.bytes)}${failNote}`);
            await refreshHistory(set);
        } catch (e) {
            get().toast("err", `还原失败：${msgOf(e)}`);
        }
    },

    async undoLast() {
        const id = get().lastSessionId;
        if (!id) {
            get().toast("warn", "没有可还原的批次");
            return;
        }
        return get().undoSession(id);
    },

    /** 彻底删除某 vault 批次副本：7 天后悔期内也可主动放弃后悔、立即释放空间。 */
    async purgeSession(id: string) {
        try {
            const r = await ipc.purgeSession(id);
            set({ cleanOutcome: null });
            dismissBanner(id);
            if (r.failed.length) {
                get().toast("warn", `已删除 ${r.done} 项，${r.failed.length} 项未能删除（可能被占用）`);
            } else {
                get().toast("ok", `已彻底删除本批副本 · 释放 ${humanSize(r.bytes)}`);
            }
            await refreshHistory(set);
        } catch (e) {
            get().toast("err", `彻底删除失败：${msgOf(e)}`);
        }
    },

    async manualDelete(paths) {
        if (paths.length === 0) {
            get().toast("warn", "没有可删除的路径");
            return;
        }
        try {
            const outcome = await ipc.vaultDelete(paths);
            // 刷新台账历史并选中该批，历史页立即可还原/彻底删除
            try {
                const history = await ipc.loadHistory();
                set({ history, lastSessionId: null });
            } catch { /* 历史刷新失败不阻断主流程 */ }
            const n = outcome.failed.length;
            get().toast(n ? "warn" : "ok", outcome.semantics_note);
        } catch (e) {
            get().toast("err", `安全删除失败：${msgOf(e)}`);
        }
    },

    async runMigration(src, dst) {
        if (get().migrateActive) throw new Error("已有迁移任务在进行");
        set({ migrateActive: true, migrateInfo: { src, dst }, migratePhase: null });
        try {
            const id = await ipc.applyMigration(src, dst);
            // 迁移历史已落台账：刷新后 History 页出现可撤销的迁移行
            await refreshHistory(set);
            get().toast("ok", `已迁移 ${src}：原路径由 junction 接管，可在历史记录中撤销`);
            return id;
        } catch (e) {
            // 内核失败即自动回滚；无论用户当前在哪个页面都要把结果送到眼前
            get().toast("err", `迁移失败（已自动回滚）：${msgOf(e)}`);
            throw e;
        } finally {
            set({ migrateActive: false, migratePhase: null });
        }
    },

    setMigrateStep: (s) => set({ migrateStep: s }),
    setMigrateForm: (src, dst) => set({ migrateSrc: src, migrateDst: dst }),
    setMigratePlan: (p) => set({ migratePlan: p }),
    setMigrateResult: (id) => set({ migrateResultId: id }),
    setMigrateUndoMsg: (m) => set({ migrateUndoMsg: m }),
    resetMigrateWizard() {
        set({
            migrateStep: "form", migratePlan: null,
            migrateResultId: null, migrateUndoMsg: null,
        });
    },

    setExpanded: (id) => set({ expandedRule: id }),
    togglePalette: (open) => set((s) => ({ paletteOpen: open ?? !s.paletteOpen })),

    toast(kind, msg) {
        const t: Toast = { id: toastSeq++, kind, msg };
        set((s) => ({ toasts: [...s.toasts.slice(-3), t] }));
        setTimeout(() => set((s) => ({ toasts: s.toasts.filter((x) => x.id !== t.id) })), 4200);
    },
}));

/** undo/purge/migrate 成功后必须重拉台账（v5 U2 修复：已还原/已删除的幽灵行不再驻留） */
async function refreshHistory(set: (partial: Partial<StoreState>) => void) {
    try {
        set({ history: await ipc.loadHistory() });
    } catch { /* 刷新失败保留旧列表，不阻断主流程 */ }
}

function dismissBanner(sessionId: string) {
    try {
        const raw = localStorage.getItem(BANNER_KEY);
        if (raw) {
            const b = JSON.parse(raw) as BannerPersist;
            if (b?.sessionId === sessionId) localStorage.removeItem(BANNER_KEY);
        }
    } catch { /* 忽略 */ }
}

/** 当前报告的可清理量文本（如 "151.56 MB (2,600)"） */
export function cleanableText(): string {
    const rep = useStore.getState().report;
    if (!rep) return "";
    return humanSize(cleanableBytes(rep));
}

export function cleanableBytes(rep: ScanReport): number {
    return (rep.findings ?? []).reduce(
        (a, f) => a + f.hits.reduce((x, h) => x + h.size, 0) + f.overflow_bytes,
        0,
    );
}

/** 辅助：仅列出有命中的规则发现 */
export function selectableFindings(report: ScanReport | null) {
    return (report?.findings ?? []).filter((f) => totalHits(f) > 0);
}

/** 调试/QA 句柄：CDP 驱动与自动化测试用（只读语义，勿在业务代码里绕过 hooks 直接操作）。 */
(window as unknown as { __zcStore?: unknown }).__zcStore = useStore;
