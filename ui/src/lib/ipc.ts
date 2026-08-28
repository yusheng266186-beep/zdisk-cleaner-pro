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

/** 在资源管理器中打开目录。浏览器开发态无壳可调，直接 resolve。 */
export async function revealInExplorer(path: string): Promise<void> {
    if (!isDesktop()) return;
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("reveal_in_explorer", { path });
}

/* ── 大文件 / 重复文件猎手 ────────────────────────────── */

export interface BigFile { path: string; size: number }

/** 内核 DuplicateGroup 同构（XXH3-128 hex + 稳定排序文件清单） */
export interface DuplicateGroup { size: number; hash: string; files: string[] }

export async function bigFiles(path = "", top = 50): Promise<BigFile[]> {
    if (!isDesktop()) {
        await wait(900);
        return demoBigFiles.map((f) => ({ ...f }));
    }
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<BigFile[]>("big_files", { path, top });
}

export async function findDupes(path: string, minMb = 10): Promise<DuplicateGroup[]> {
    if (!isDesktop()) {
        await wait(1500);
        return demoDupes.map((g) => ({ ...g, files: [...g.files] }));
    }
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<DuplicateGroup[]>("find_dupes", { path, minMb });
}

const MB = 1024 ** 2;
const GB = 1024 ** 3;

/** 浏览器演示态：GB~MB 跨量级 6 条，模拟真机 Top-N 排行 */
const demoBigFiles: BigFile[] = [
    { path: String.raw`C:\Users\demo\Downloads\starfield-ultimate-edition.iso`, size: 87.4 * GB },
    { path: String.raw`C:\Users\demo\.ollama\models\blobs\qwen2.5-72b-q4.gguf`, size: 41.6 * GB },
    { path: String.raw`C:\Users\demo\Videos\4K\gopro-summer-2025.mp4`, size: 12.8 * GB },
    { path: String.raw`C:\Users\demo\AppData\Local\Docker\wsl\data\ext4.vhdx`, size: 6.2 * GB },
    { path: String.raw`C:\Users\demo\Downloads\unity-editor-setup-6000.zip`, size: 830 * MB },
    { path: String.raw`C:\Users\demo\Documents\obs-recordings\2026-08-11.mkv`, size: 42.5 * MB },
];

/** 浏览器演示态：2 组重复（XXH3-128 十六进制同内核口径） */
const demoDupes: DuplicateGroup[] = [
    {
        size: 3_221_225_472,
        hash: "8f14e45fceea167a5a36dedd4bea2543",
        files: [
            String.raw`D:\Photos\RAW\IMG_20250712_0041.ARW`,
            String.raw`E:\Backup\Photos\2025\IMG_20250712_0041.ARW`,
        ],
    },
    {
        size: 18_874_368,
        hash: "c9b5d1fd4e3b7a02f8d6c1e90a47b356",
        files: [
            String.raw`C:\Users\demo\Downloads\setup-sdk-v12.exe`,
            String.raw`D:\Installers\setup-sdk-v12.exe`,
            String.raw`E:\Archive\tools\setup-sdk-v12.exe`,
        ],
    },
];

/* ── 启动项管家 ───────────────────────────────────────── */

export interface StartupEntry {
    key_id: string;
    hive: string;
    subkey: string;
    run_once: boolean;
    name: string;
    command: string;
}

export async function listStartups(): Promise<StartupEntry[]> {
    if (!isDesktop()) {
        await wait(350);
        return [...demoStartups];
    }
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<StartupEntry[]>("startup_list");
}

export async function disabledCount(): Promise<number> {
    if (!isDesktop()) return demoStartupBackup.length;
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<number>("startup_disabled_count");
}

export async function disableStartup(keyId: string): Promise<boolean> {
    if (!isDesktop()) {
        await wait(500);
        const hit = demoStartups.find((e) => e.key_id === keyId);
        if (!hit) return false;
        demoStartups = demoStartups.filter((e) => e.key_id !== keyId);
        demoStartupBackup.push(hit);
        return true;
    }
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<boolean>("startup_disable", { keyId });
}

export async function enableAllStartups(): Promise<number> {
    if (!isDesktop()) {
        await wait(700);
        const n = demoStartupBackup.length;
        demoStartups = [...demoStartups, ...demoStartupBackup];
        demoStartupBackup = [];
        return n;
    }
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<number>("startup_enable_all");
}

/* ── 存储迁移中心 ─────────────────────────────────────── */

export interface MigrationPlan {
    src: string;
    dst: string;
    total_bytes: number;
    total_files: number;
}

/** 内核迁移五阶段（snake_case，与 Rust 侧序列化名一致） */
export type MigratePhaseKey = "copy" | "verify" | "link" | "smoke" | "cleanup";
export type MigratePhaseEvent = { phase: MigratePhaseKey; state: "start" | "end" };

type MigratePhaseCb = (p: MigratePhaseEvent) => void;
let migratePhaseCb: MigratePhaseCb | null = null;

/** 订阅内核真实阶段事件（migrate://phase，payload = [phase, state]）。
 *  浏览器模式无内核，返回 noop unlisten —— 数据由 applyMigration 模拟推送。 */
export async function onMigratePhase(cb: MigratePhaseCb): Promise<() => void> {
    migratePhaseCb = cb;
    if (!isDesktop()) return () => { migratePhaseCb = null; };
    const { listen } = await import("@tauri-apps/api/event");
    const unlisten = await listen<string[]>("migrate://phase", (ev) => {
        cb({ phase: ev.payload[0] as MigratePhaseKey, state: ev.payload[1] as "start" | "end" });
    });
    return () => {
        unlisten();
        migratePhaseCb = null;
    };
}

export async function planMigration(src: string, dstRoot: string): Promise<MigrationPlan> {
    if (!isDesktop()) {
        await wait(900);
        // 固定体积假数据：真机口径量级，浏览器开发只看排版
        return {
            src,
            dst: `${dstRoot.replace(/[\\/]+$/, "")}\\${dirNameOf(src)}`,
            total_bytes: 47_512_809_234,
            total_files: 218_304,
        };
    }
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<MigrationPlan>("migrate_plan", { src, dstRoot });
}

export async function applyMigration(src: string, dstRoot: string): Promise<string> {
    if (!isDesktop()) {
        // 演示态：按内核真实事件流同构的相位序列走一段时间轴，
        // 每个 Start/End 间隔 250ms 依次推给订阅者，最后 resolve。
        const phases: MigratePhaseKey[] = ["copy", "verify", "link", "smoke", "cleanup"];
        for (const phase of phases) {
            await wait(250);
            migratePhaseCb?.({ phase, state: "start" });
            await wait(250);
            migratePhaseCb?.({ phase, state: "end" });
        }
        await wait(250);
        return `demo-${Date.now().toString(36)}`;
    }
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<string>("migrate_apply", { src, dstRoot });
}

export async function undoMigration(src: string): Promise<string> {
    if (!isDesktop()) {
        await wait(1200);
        return "[演示] junction 已摘除，原目录数据已复位";
    }
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<string>("migrate_undo", { src });
}

/* ── 深度工具：系统级占用 / WinSxS 组件清理 / 还原点 ───── */

/** 内核 OccupancyItem 同构（size=null = ACL 拒绝，诚实标「未知」） */
export interface OccupancyItem {
    name: string;
    path: string;
    size: number | null;
    guide_zh: string;
}

export async function systemOccupancy(): Promise<OccupancyItem[]> {
    if (!isDesktop()) {
        await wait(600);
        // 3 条样例：含 hiberfil size:null（ACL 拒绝口径）
        return [
            { name: "Windows.old", path: String.raw`C:\Windows.old`, size: 23.4 * GB, guide_zh: "设置→系统→存储→临时文件→以前的 Windows 安装" },
            { name: "hiberfil.sys", path: String.raw`C:\hiberfil.sys`, size: null, guide_zh: "管理员运行 powercfg /h off 可关闭休眠并释放" },
            { name: "pagefile.sys", path: String.raw`C:\pagefile.sys`, size: 8 * GB, guide_zh: "此为虚拟内存，建议通过 系统属性→高级→性能→虚拟内存 调整" },
        ];
    }
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<OccupancyItem[]>("system_occupancy");
}

type DismProgressCb = (pct: number) => void;
let dismProgressCb: DismProgressCb | null = null;

/** 订阅 DISM 真实百分比（dism://progress，payload = f32）。
 *  浏览器模式无内核，返回 noop unlisten —— 推进由 dismCleanup 模拟。 */
export async function onDismProgress(cb: DismProgressCb): Promise<() => void> {
    dismProgressCb = cb;
    if (!isDesktop()) return () => { dismProgressCb = null; };
    const { listen } = await import("@tauri-apps/api/event");
    const unlisten = await listen<number>("dism://progress", (ev) => cb(ev.payload));
    return () => {
        unlisten();
        dismProgressCb = null;
    };
}

/** WinSxS 组件清理：未提权时桌面端以 Err("需要管理员：…") 拒绝，
 *  由 UI 层展示提权引导。 */
export async function dismCleanup(): Promise<void> {
    if (!isDesktop()) {
        // 演示态：4 次 25% 间隔推进后 resolve（4×400ms = 1.6s）
        for (let i = 1; i <= 4; i++) {
            await wait(400);
            dismProgressCb?.(i * 25);
        }
        return;
    }
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("dism_component_cleanup");
}

export async function createRestorePoint(desc: string): Promise<void> {
    if (!isDesktop()) {
        await wait(1200);
        return;
    }
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("create_restore_point", { desc });
}

const hitCount = (f: { hits: unknown[]; overflow_hits: number }) =>
    f.hits.length + (f.overflow_hits ?? 0);
const byteSum = (f: { hits: { size: number }[]; overflow_bytes: number }) =>
    f.hits.reduce((a, h) => a + h.size, 0) + (f.overflow_bytes ?? 0);

const wait = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
const dirNameOf = (p: string) =>
    p.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? "target-dir";

/* ── 启动项管家的浏览器演示态（可交互：禁用/恢复都在内存里走） ── */

let demoStartups: StartupEntry[] = [
    {
        key_id: "hkcu|Software\\Microsoft\\Windows\\CurrentVersion\\Run|CloudDrive",
        hive: "hkcu",
        subkey: String.raw`Software\Microsoft\Windows\CurrentVersion\Run`,
        run_once: false,
        name: "CloudDrive",
        command: String.raw`"C:\Users\demo\AppData\Local\CloudDrive\CloudDrive.exe" /min`,
    },
    {
        key_id: "hkcu|Software\\Microsoft\\Windows\\CurrentVersion\\Run|WeChat",
        hive: "hkcu",
        subkey: String.raw`Software\Microsoft\Windows\CurrentVersion\Run`,
        run_once: false,
        name: "WeChat",
        command: String.raw`"C:\Program Files\Tencent\WeChat\WeChat.exe" -startup`,
    },
    {
        key_id: "hkcu|Software\\Microsoft\\Windows\\CurrentVersion\\RunOnce|SetupRebuild",
        hive: "hkcu",
        subkey: String.raw`Software\Microsoft\Windows\CurrentVersion\RunOnce`,
        run_once: true,
        name: "SetupRebuild",
        command: String.raw`C:\Users\demo\AppData\Local\Setup\rebuild-index.cmd`,
    },
    {
        key_id: "hkcu|Software\\Microsoft\\Windows\\CurrentVersion\\Run|Everything",
        hive: "hkcu",
        subkey: String.raw`Software\Microsoft\Windows\CurrentVersion\Run`,
        run_once: false,
        name: "Everything",
        command: String.raw`"C:\Tools\Everything\Everything.exe" -startup`,
    },
];

let demoStartupBackup: StartupEntry[] = [];

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
