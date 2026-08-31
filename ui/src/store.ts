/** 全局状态机：idle → scanning → results → cleaning → idle(+报告横幅) */
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

    init: () => Promise<void>;
    setActivePage: (p: Page) => void;
    setPendingMigrateSrc: (v: string | null) => void;
    toggleTheme: () => void;
    startScan: () => Promise<void>;
    cancelScan: () => void;
    toggleSelect: (id: string) => void;
    selectSafeOnly: () => void;
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
    setExpanded: (id: string | null) => void;
    togglePalette: (open?: boolean) => void;
    toast: (kind: Toast["kind"], msg: string) => void;
}

let toastSeq = 1;

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

    async init() {
        document.documentElement.dataset.theme = get().theme;
        const [version, appVer, rules, drives, history] = await Promise.all([
            ipc.coreVersion(),
            ipc.appVersion(),
            ipc.loadRuleMeta(),
            ipc.listDrives(),
            ipc.loadHistory(),
        ]);
        set({ version, appVersion: appVer, rules, drives, history });
        if (!ipc.isDesktop()) return;
    },

    toggleTheme() {
        const theme = get().theme === "dark" ? "light" : "dark";
        localStorage.setItem("zc-theme", theme);
        document.documentElement.dataset.theme = theme;
        set({ theme });
    },

    async startScan() {
        if (get().phase === "scanning") return;
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
            );
        } catch (e) {
            // 扫描失败绝不静默卡在 0：回 idle 并把原因抛给用户
            set({ phase: "idle" });
            get().toast("err", `扫描失败：${e instanceof Error ? e.message : String(e)}`);
        }
    },

    cancelScan() {
        void ipc.cancelScan();
        set({ phase: "idle" });
        get().toast("info", "已请求取消扫描");
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

    clearSelection: () => set({ selection: new Set() }),

    async cleanSelected(mode) {
        const { report, selection } = get();
        if (!report || selection.size === 0) {
            get().toast("warn", "请至少选择一条规则");
            return;
        }
        set({ phase: "cleaning" });
        try {
            // 交互节奏：先让执行页动画跑起来（1.2s），再等待真实/演示结果
            const [outcome] = await Promise.all([
                ipc.cleanSelected(report, [...selection], mode),
                new Promise((r) => setTimeout(r, 1200)),
            ]);
            const histEntry: HistoryRecord = {
                // 真实台账 ID(report.id):历史页的还原/彻底删除按钮按它调后端。
                // 后端 clean_selected 已用同一 ID 写过台账 history 行,这里 upsert 同一行;
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
            get().toast(outcome.failed.length ? "warn" : "ok", outcome.semantics_note);
        } catch (e) {
            // 失败绝不能把执行遮罩卡死：退回结果页并把原因亮出来
            set({ phase: get().report ? "results" : "idle" });
            get().toast("err", `清理失败：${e instanceof Error ? e.message : String(e)}`);
        }
    },

    async undoSession(id: string) {
        try {
            const msg = await ipc.undoSession(id);
            set({ cleanOutcome: null });
            get().toast("ok", msg);
        } catch (e) {
            get().toast("err", `还原失败：${e instanceof Error ? e.message : String(e)}`);
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
            const msg = await ipc.purgeSession(id);
            set({ cleanOutcome: null });
            get().toast("ok", msg);
        } catch (e) {
            get().toast("err", `彻底删除失败：${e instanceof Error ? e.message : String(e)}`);
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
            get().toast("err", `安全删除失败：${e instanceof Error ? e.message : String(e)}`);
        }
    },

    async runMigration(src, dst) {
        if (get().migrateActive) throw new Error("已有迁移任务在进行");
        set({ migrateActive: true, migrateInfo: { src, dst } });
        try {
            const id = await ipc.applyMigration(src, dst);
            get().toast("ok", `迁移完成：${src} 已由 junction 接管（可在历史/迁移中心撤销）`);
            return id;
        } catch (e) {
            // 内核失败即自动回滚；无论用户当前在哪个页面都要把结果送到眼前
            get().toast("err", `迁移失败（已自动回滚）：${e instanceof Error ? e.message : String(e)}`);
            throw e;
        } finally {
            set({ migrateActive: false });
        }
    },

    setActivePage: (p) => set({ activePage: p }),
    setPendingMigrateSrc: (v) => set({ pendingMigrateSrc: v }),

    setExpanded: (id) => set({ expandedRule: id }),
    togglePalette: (open) => set((s) => ({ paletteOpen: open ?? !s.paletteOpen })),

    toast(kind, msg) {
        const t: Toast = { id: toastSeq++, kind, msg };
        set((s) => ({ toasts: [...s.toasts.slice(-3), t] }));
        setTimeout(() => set((s) => ({ toasts: s.toasts.filter((x) => x.id !== t.id) })), 4200);
    },
}));

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
