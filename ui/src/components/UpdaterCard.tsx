import { useState } from "react";
import { RefreshCw, DownloadCloud } from "lucide-react";
import { isDesktop } from "../lib/ipc";

type State = "idle" | "checking" | "available" | "uptodate" | "installing" | "installed";

/** 应用内更新卡：检查 GitHub Releases latest.json → 下载安装（重启由用户手动完成） */
export function UpdaterCard() {
    const [st, setSt] = useState<State>("idle");
    const [ver, setVer] = useState("");

    async function run(installAfter: boolean) {
        if (!isDesktop()) return;
        try {
            setSt("checking");
            const { check } = await import("@tauri-apps/plugin-updater");
            const upd = await check();
            if (!upd) { setSt("uptodate"); return; }
            setVer(upd.version ?? "");
            if (!installAfter) { setSt("available"); return; }
            setSt("installing");
            await upd.downloadAndInstall();
            setSt("installed");
        } catch {
            setSt("idle");
        }
    }

    if (!isDesktop()) return null;

    const label: Record<State, string> = {
        idle: "检查更新", checking: "检查中…", available: `发现新版 ${ver}，立即安装`,
        uptodate: "已是最新版本", installing: "下载安装中…", installed: "已安装，重启应用后生效",
    };

    return (
        <div className="flex items-center justify-between gap-3 px-4 py-3" style={{ borderTop: "1px solid var(--zc-border)" }}>
            <div className="text-sm">应用内更新</div>
            <button
                onClick={() => void run(st === "available")}
                disabled={st === "checking" || st === "installing"}
                className="flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs transition-opacity hover:opacity-80 disabled:opacity-50"
                style={{ borderColor: "var(--zc-border-strong)", color: st === "available" ? "var(--zc-ok)" : "var(--zc-text-2)" }}
            >
                {st === "available" ? <DownloadCloud size={13} /> : <RefreshCw size={13} />}
                {label[st]}
            </button>
        </div>
    );
}
