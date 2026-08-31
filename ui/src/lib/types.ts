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
  /** v5：批次类别（vault / recycle_bin / system / migrate）。缺省时以 mode 归类 */
  kind?: string;
  /** v5：迁移批次的源路径 */
  src?: string | null;
  /** v5：迁移批次的目标路径 */
  dst?: string | null;
  /** v5：台账行是否存活。false=已结清（还原/彻底删除/到期清扫），历史页隐藏动作；
   *  缺省（浏览器演示态）按存活处理 */
  live?: boolean;
}

/* ── v5 结构化 DTO（CONTRACT-v5 §2，serde snake_case 直传）────────── */

/** ErrorDto：全壳统一错误分类（io/guard/admin_required/not_found/busy/locked/cancelled/internal） */
export interface FailDto {
  path: string;
  error: string;
}

/** UndoResultDto / SessionOpDto（undo_session、purge_session 复用） */
export interface UndoResultDto {
  id: string;
  done: number;
  bytes: number;
  failed: FailDto[];
}

/** MigrateUndoDto */
export interface MigrateUndoDto {
  restored: number;
  failed: FailDto[];
}

/** SessionEntryDto：批次明细下钻（status=pending 为 journal 未完成警示） */
export interface SessionEntryDto {
  origin: string;
  vault_rel: string;
  size: number;
  status: string;
}

/** RecycleBinInfo（query_recycle_bin） */
export interface RecycleBinInfo {
  items: number;
  bytes: number;
}

/** RecycleBinSummary（empty_recycle_bin） */
export interface RecycleBinSummary {
  items_before: number;
  bytes_before: number;
  bytes_freed: number;
}

/** StartupDisabledEntry（startup_list_disabled） */
export interface StartupDisabledEntry {
  key_id: string;
  value: string;
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
