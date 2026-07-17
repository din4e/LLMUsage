export interface ProviderMetrics {
  requests: number | null;
  totalTokens: number | null;
  estimatedCostCny: number | null;
}

export type TrendRange = "7d" | "30d" | "all";

export interface DailyUsageRecord {
  date: string;
  providerId: string;
  requests: number | null;
  totalTokens: number | null;
  estimatedCostCny: number | null;
}

export interface DailyTrendPoint extends ProviderMetrics {
  date: string;
}

export interface OnlineDetailSection {
  title: string;
  entries: OnlineDetailEntry[];
}

export interface OnlineDetailEntry {
  label: string;
  used?: string | null;
  remaining?: string | null;
  limit?: string | null;
  unit: string;
  usedPercent?: number | null;
  window?: string | null;
  startAtMs?: number | null;
  resetAtMs?: number | null;
  remainingMs?: number | null;
}

export function localDayRange(date = new Date()): { startTime: string; endTime: string } {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  const prefix = `${year}-${month}-${day}`;
  return { startTime: `${prefix} 00:00:00`, endTime: `${prefix} 23:59:59` };
}

export function localDayRangeMs(date = new Date()): { startTimeMs: number; endTimeMs: number } {
  const start = new Date(date.getFullYear(), date.getMonth(), date.getDate());
  const end = new Date(date.getFullYear(), date.getMonth(), date.getDate() + 1);
  return { startTimeMs: start.getTime(), endTimeMs: end.getTime() };
}

export function localDateKey(date = new Date()): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

export function selectDailyTrend(
  records: DailyUsageRecord[],
  range: TrendRange,
  providerId: string,
  today = new Date(),
): DailyTrendPoint[] {
  const cutoff = new Date(today.getFullYear(), today.getMonth(), today.getDate());
  if (range !== "all") cutoff.setDate(cutoff.getDate() - (range === "7d" ? 6 : 29));
  const cutoffKey = localDateKey(cutoff);
  const todayKey = localDateKey(today);
  const byDate = new Map<string, ProviderMetrics[]>();

  for (const record of records) {
    if (record.date > todayKey || (range !== "all" && record.date < cutoffKey)) continue;
    if (providerId !== "all" && record.providerId !== providerId) continue;
    const metrics = byDate.get(record.date) ?? [];
    metrics.push(record);
    byDate.set(record.date, metrics);
  }

  return Array.from(byDate, ([date, metrics]) => ({ date, ...summarizeProviders(metrics) }))
    .filter((point) => point.totalTokens !== null)
    .sort((left, right) => left.date.localeCompare(right.date));
}

export function formatCooldown(resetAtMs: number, nowMs = Date.now()): string {
  const remainingSeconds = Math.max(0, Math.floor((resetAtMs - nowMs) / 1_000));
  if (remainingSeconds < 60) return "即将恢复";

  const totalMinutes = Math.floor(remainingSeconds / 60);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (hours === 0) return `${minutes} 分钟后`;
  if (minutes === 0) return `${hours} 小时后`;
  return `${hours} 小时 ${minutes} 分后`;
}

export function formatInteger(value: number): string {
  if (!Number.isFinite(value) || value < 0) return "—";
  if (value < 1_000_000) return Math.round(value).toLocaleString("en-US");
  if (value < 1_000_000_000) return `${(value / 1_000_000).toFixed(1)}M`;
  return `${(value / 1_000_000_000).toFixed(1)}B`;
}

export function formatQuotaDetailValue(entry: OnlineDetailEntry): string {
  const values: string[] = [];
  if (entry.used != null) values.push(`已用 ${entry.used}${entry.unit}`);
  if (entry.remaining != null) values.push(`剩余 ${entry.remaining}${entry.unit}`);
  if (entry.limit != null) values.push(`上限 ${entry.limit}${entry.unit}`);
  return values.join(" · ");
}

export function computeTimeWindowElapsedPercent(
  startAtMs: number,
  resetAtMs: number,
  nowMs: number = Date.now(),
): number | null {
  if (!Number.isFinite(startAtMs) || !Number.isFinite(resetAtMs) || resetAtMs <= startAtMs) return null;
  const total = resetAtMs - startAtMs;
  const elapsed = Math.max(0, Math.min(total, nowMs - startAtMs));
  const percent = (elapsed / total) * 100;
  return Number.isFinite(percent) ? Math.max(0, Math.min(100, percent)) : null;
}

const WEEKDAY_LABELS = ["周日", "周一", "周二", "周三", "周四", "周五", "周六"];

export function formatResetRemainingText(
  resetAtMs: number,
  nowMs: number = Date.now(),
): string | null {
  if (!Number.isFinite(resetAtMs)) return null;
  const remainingMs = Math.max(0, resetAtMs - nowMs);
  const dayIndex = new Date(nowMs).getDay();
  return `${WEEKDAY_LABELS[dayIndex]} · 剩余 ${formatDuration(remainingMs)}`;
}

export function formatDuration(durationMs: number): string {
  const totalMinutes = Math.max(0, Math.floor(durationMs / 60_000));
  if (totalMinutes < 1) return `${Math.max(0, Math.floor(durationMs / 1_000))} 秒`;
  const days = Math.floor(totalMinutes / 1_440);
  const hours = Math.floor((totalMinutes % 1_440) / 60);
  const minutes = totalMinutes % 60;
  return [
    days > 0 ? `${days} 天` : "",
    hours > 0 ? `${hours} 小时` : "",
    minutes > 0 ? `${minutes} 分钟` : "",
  ].filter(Boolean).join(" ");
}

export function credentialHint(providerId: string): string {
  if (providerId === "kimi_cn") {
    return "Kimi Code 请使用会员控制台生成的 Key（通常以 sk-kimi- 开头）；Moonshot 开放平台 Key 也可配置，将自动查询 API 余额。";
  }
  if (providerId === "minimax_cn" || providerId === "minimax_global") {
    return "请使用 Token Plan 订阅 Key（通常以 sk-cp- 开头）；普通按量 API Key 不可查询套餐用量。";
  }
  return "密钥由 Windows DPAPI 加密，仅当前用户可解密，不会写入数据库或日志。";
}

export function summarizeProviders(providers: ProviderMetrics[]): ProviderMetrics {
  return providers.reduce<ProviderMetrics>(
    (total, provider) => ({
      requests:
        provider.requests === null ? total.requests : (total.requests ?? 0) + provider.requests,
      totalTokens:
        provider.totalTokens === null
          ? total.totalTokens
          : (total.totalTokens ?? 0) + provider.totalTokens,
      estimatedCostCny:
        provider.estimatedCostCny === null
          ? total.estimatedCostCny
          : (total.estimatedCostCny ?? 0) + provider.estimatedCostCny,
    }),
    { requests: null, totalTokens: null, estimatedCostCny: null },
  );
}
