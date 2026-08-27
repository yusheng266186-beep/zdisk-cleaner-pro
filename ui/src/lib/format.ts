/** 人类可读字节数 —— 与内核 models::human_size 同一口径 */
export function humanSize(bytes: number): string {
  if (!isFinite(bytes)) return "—";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = bytes;
  let u = 0;
  while (v >= 1024 && u < units.length - 1) {
    v /= 1024;
    u += 1;
  }
  return u === 0 ? `${Math.round(v)} ${units[u]}` : `${v.toFixed(2)} ${units[u]}`;
}

export function thousand(n: number): string {
  return n.toLocaleString("en-US");
}

export function timeAgo(unixSec: number): string {
  const diff = Date.now() / 1000 - unixSec;
  if (diff < 90) return "刚刚";
  if (diff < 3600) return `${Math.round(diff / 60)} 分钟前`;
  if (diff < 86400) return `${Math.round(diff / 3600)} 小时前`;
  return new Date(unixSec * 1000).toLocaleDateString("zh-CN");
}
