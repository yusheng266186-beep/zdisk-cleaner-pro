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
    undoLast: () => Promise<void>;
    setExpanded: (id: string | null) => void;
    togglePalette: (open?: boolean) => void;
    toast: (kind: Toast["kind"], msg: string) => void;
}

let toastSeq = 1;

export const useStore = create<StoreState>((set, get) => ({
    demo: !ipc.isDesktop(),
    version: "",
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

    async init() {
        document.documentElement.dataset.theme = get().theme;
        const [version, rules, drives, history] = await Promise.all([
            ipc.coreVersion(),
            ipc.loadRuleMeta(),
            ipc.listDrives(),
            ipc.loadHistory(),
        ]);
        set({ version, rules, drives, history });
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
                session_id: `${outcome.requested_files}-${Date.now()}`,
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

    async undoLast() {
        const id = get().lastSessionId;
        if (!id) {
            get().toast("warn", "没有可还原的批次");
            return;
        }
        try {
            const msg = await ipc.undoSession(id);
            set({ cleanOutcome: null });
            get().toast("ok", msg);
        } catch (e) {
            get().toast("err", `还原失败：${e instanceof Error ? e.message : String(e)}`);
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
