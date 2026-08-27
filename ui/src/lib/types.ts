/** 与 Rust 侧对齐的数据契约（serde 蛇形命名 → 保持一致，避免映射层） */
export type Risk = "safe" | "caution" | "risky" | "expert";
export type Domain = "system" | "browser" | "dev" | "apps" | "logs";

export interface FileHit {
  path: string;
  size: number;
  is_dir: boolean;
}

export interface Finding {
  rule_id: string;
  hits: FileHit[];
  overflow_hits: number;
  overflow_bytes: number;
}

export interface ScanReport {
  id: string;
  started_unix: number;
  duration_ms: number;
  files_seen: number;
  bytes_seen: number;
  cancelled: boolean;
  findings: Finding[];
}

export interface RuleMeta {
    id: string;
    name_zh: string;
    domain: Domain;
    risk: Risk;
    admin_required: boolean;
}

/** 与内核 Finding::total_count 同口径：明细数 + overflow 数 */
export function totalHits(f: Finding): number {
    return f.hits.length + (f.overflow_hits ?? 0);
}

/** 与内核 Finding::total_bytes 同口径 */
export function totalBytesOf(f: Finding): number {
    return f.hits.reduce((a, h) => a + h.size, 0) + (f.overflow_bytes ?? 0);
}

export interface CleanOutcome {
  requested_files: number;
  requested_bytes: number;
  done_files: number;
  done_bytes: number;
  failed: [string, string][];
  semantics_note: string;
}

export interface HistoryRecord {
  session_id: string;
  created_unix: number;
  mode: "recycle_bin" | "vault";
  files: number;
  bytes_moved: number;
}

export const DOMAIN_ZH: Record<Domain, string> = {
  system: "系统",
  browser: "浏览器",
  dev: "开发",
  apps: "应用",
  logs: "日志",
};

export const RISK_ZH: Record<Risk, string> = {
  safe: "安全",
  caution: "注意",
  risky: "风险",
  expert: "专家",
};
