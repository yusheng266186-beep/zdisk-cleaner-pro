import type {
    CleanOutcome,
    HistoryRecord,
    Risk,
    RuleMeta,
    ScanReport,
} from "./types";
import { SAMPLE_TREE } from "./tree";
import type { TreeNode } from "./tree";

/** 桌面壳 ↔ 纯浏览器 双通道桥。
 *  浏览器模式服务于 UI 开发（pnpm dev），数据为「真机采样」的确定性样本，
 *  并由 store 打上 DEMO 徽标 —— 不假装是真实清理。 */

export const isDesktop = (): boolean => "__TAURI_INTERNALS__" in window;

type ProgressCb = (files: number, bytesSeen: number) => void;
let progressCb: ProgressCb | null = null;

export function coreVersion(): Promise<string> {
    if (!isDesktop()) return Promise.resolve("browser-dev");
    return import("@tauri-apps/api/core").then((m) => m.invoke<string>("ping"));
}

export interface DriveInfo { label: string; total_bytes: number; free_bytes: number }

export async function listDrives(): Promise<DriveInfo[]> {
    if (!isDesktop()) {
        return [
            { label: "C:", total_bytes: 512 * 110_374_182_770 / 512, free_bytes: 19.3 * 1024 ** 3 },
            { label: "D:", total_bytes: 1024 * 107_374_182_400 / 1024, free_bytes: 640 * 1024 ** 3 },
        ];
    }
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<DriveInfo[]>("drives_overview");
}

export async function loadRuleMeta(): Promise<RuleMeta[]> {
    if (!isDesktop()) return SAMPLE_RULES;
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<RuleMeta[]>("rules_meta");
}

export async function loadHistory(): Promise<HistoryRecord[]> {
    if (!isDesktop()) return [
        { session_id: "demo-1", created_unix: Date.now() / 1000 - 86400 * 2.1, mode: "vault", files: 1418, bytes_moved: 132 * 1024 ** 2 },
        { session_id: "demo-2", created_unix: Date.now() / 1000 - 86400 * 6.4, mode: "recycle_bin", files: 306, bytes_moved: 41 * 1024 ** 2 },
    ];
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<HistoryRecord[]>("history_list");
}

/* ── 扫描 ─────────────────────────────────────────────── */

let cancelledFlag = false;

export async function startScan(cb: ProgressCb, onDone: (r: ScanReport) => void): Promise<void> {
    cancelledFlag = false;
    if (isDesktop()) {
        const { invoke } = await import("@tauri-apps/api/core");
        const { listen } = await import("@tauri-apps/api/event");
        progressCb = cb;
        await listen<number[]>("scan://progress", (ev) => {
            progressCb?.(ev.payload[0], ev.payload[1]);
        });
        const rep = await invoke<ScanReport>("scan_now", {});
        if (!rep.cancelled) onDone(rep);
        return;
    }

    // 浏览器：按真机样本走一段压缩时间轴
    const target = SAMPLE_FINDINGS.reduce(
        (a, f) => ({
            files: a.files + hitCount(f),
            bytes: a.bytes + byteSum(f),
        }),
        { files: 0, bytes: 0 },
    );
    let t = 0;
    const timer = setInterval(() => {
        t += 90;
        const p = Math.min(t / 2400, 1);
        const ease = 1 - Math.pow(1 - p, 3);
        cb(Math.round(28_236 * ease), Math.round(target.bytes * ease));
        if (p >= 1 || cancelledFlag) clearInterval(timer);
    }, 90);

    setTimeout(() => {
        if (cancelledFlag) return;
        onDone({
            id: `demo-${Date.now().toString(36)}`,
            started_unix: Date.now() / 1000,
            duration_ms: 2400,
            files_seen: 28_236,
            bytes_seen: target.bytes * 220,
            cancelled: false,
            findings: SAMPLE_FINDINGS,
        });
    }, 2500);
}

export async function cancelScan(): Promise<void> {
    cancelledFlag = true;
    if (!isDesktop()) return;
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("cancel_scan");
}

/* ── 清理 / 还原 ───────────────────────────────────────── */

export async function cleanSelected(
    report: ScanReport,
    ruleIds: string[],
    mode: "recycle_bin" | "vault",
): Promise<CleanOutcome> {
    const requestedBytes = report.findings
        .filter((f) => ruleIds.includes(f.rule_id))
        .reduce((a, f) => a + byteSum(f), 0);
    const requestedFiles = report.findings
        .filter((f) => ruleIds.includes(f.rule_id))
        .reduce((a, f) => a + hitCount(f), 0);

    if (!isDesktop()) {
        // 演示态：只做延迟动画，不改任何真实文件
        await new Promise((r) => setTimeout(r, 1500));
        return {
            requested_files: requestedFiles,
            requested_bytes: requestedBytes,
            done_files: requestedFiles,
            done_bytes: requestedBytes,
            failed: [],
            semantics_note:
                mode === "vault"
                    ? "[演示] 已移入暂存区 vault：7 天内可一键还原"
                    : "[演示] 已移入回收站：清空回收站前不会真正释放磁盘空间",
        };
    }
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<CleanOutcome>("clean_selected", {
        report,
        ruleIds,
        mode: mode === "vault" ? "vault" : "recycle_bin",
    });
}

export async function undoSession(_id: string): Promise<string> {
    if (!isDesktop()) {
        await new Promise((r) => setTimeout(r, 900));
        return "[演示] 已还原本批全部条目";
    }
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<string>("undo_session", { id: _id });
}

/* ── 空间雷达 ─────────────────────────────────────────── */

export async function analyzeTree(path = "", depth = 4): Promise<TreeNode> {
    if (!isDesktop()) return SAMPLE_TREE;
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<TreeNode>("analyze_tree", { path, depth });
}

const hitCount = (f: { hits: unknown[]; overflow_hits: number }) =>
    f.hits.length + (f.overflow_hits ?? 0);
const byteSum = (f: { hits: { size: number }[]; overflow_bytes: number }) =>
    f.hits.reduce((a, h) => a + h.size, 0) + (f.overflow_bytes ?? 0);

/* ── 真机采样的演示数据（来源：本仓库 2026-08-27 实测扫描） ── */

export const SAMPLE_RULES: RuleMeta[] = [
    { id: "sys-user-temp", name_zh: "用户临时文件", domain: "system", risk: "safe", admin_required: false },
    { id: "sys-thumbnails", name_zh: "缩略图缓存", domain: "system", risk: "safe", admin_required: false },
    { id: "sys-dx-shader", name_zh: "DirectX 着色器缓存", domain: "system", risk: "safe", admin_required: false },
    { id: "edge-cache", name_zh: "Edge 缓存", domain: "browser", risk: "safe", admin_required: false },
    { id: "dev-pnpm-metadata", name_zh: "pnpm 状态缓存", domain: "dev", risk: "safe", admin_required: false },
    { id: "dev-uv-cache", name_zh: "uv 下载缓存", domain: "dev", risk: "safe", admin_required: false },
    { id: "dev-playwright", name_zh: "Playwright 浏览器二进制", domain: "dev", risk: "safe", admin_required: false },
    { id: "app-amd-shader", name_zh: "AMD 着色器缓存", domain: "apps", risk: "safe", admin_required: false },
];

function mk(id: string, n: [string, number][], overFiles = 0, overBytes = 0): {
    rule_id: string;
    hits: { path: string; size: number; is_dir: boolean }[];
    overflow_hits: number;
    overflow_bytes: number;
} {
    return {
        rule_id: id,
        hits: n.map(([path, size]) => ({ path, size, is_dir: false })),
        overflow_hits: overFiles,
        overflow_bytes: overBytes,
    };
}

/** 数字与实际真机扫描一致；hits 仅列代表项，overflow 承载余量 */
export const SAMPLE_FINDINGS = [
    mk("app-amd-shader", [[String.raw`C:\Users\yusheng\AppData\Local\AMD\DxcCache`, 0]], 1020, 83_522_000),
    mk("sys-user-temp", [[String.raw`C:\Users\yusheng\AppData\Local\Temp\tmp_a.log`, 812_400]], 1453, 52_689_000 - 812_400),
    mk("sys-thumbnails", [[String.raw`C:\Users\yusheng\AppData\Local\Microsoft\Windows\Explorer\thumbcache_256.db`, 16_860_000]]),
    mk("edge-cache", [[String.raw`C:\Users\yusheng\AppData\Local\Microsoft\Edge\User Data\Default\Cache\f1`, 5_850_000]], 8, 0),
    mk("sys-dx-shader", [[String.raw`C:\Users\yusheng\AppData\Local\D3DSCache`, 0]], 69, 921_000),
    mk("dev-pnpm-metadata", [[String.raw`C:\Users\yusheng\AppData\Local\pnpm-state\v3\state.json`, 56]]),
    mk("dev-uv-cache", [], 10, 44),
    mk("dev-playwright", [], 6, 0),
].map((f) => f as never as ScanReport["findings"][number]);

export type { Risk };
