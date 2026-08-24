export interface ProviderMetrics {
  requests: number | null;
  totalTokens: number | null;
  estimatedCostCny: number | null;
}

export type TrendRange = "24h" | "7d" | "30d" | "all";

export interface DailyUsageRecord {
  date: string;
  slot: number | null;
  providerId: string;
  requests: number | null;
  totalTokens: number | null;
  estimatedCostCny: number | null;
  balanceCny?: number | null;
}

export type ProviderChangeMetric = "requests" | "tokens" | "balance" | "cost";

export interface ProviderRecentChange {
  providerId: string;
  metric: ProviderChangeMetric;
  previousValue: number;
  currentValue: number;
  delta: number;
  previousDate: string;
  previousSlot: number | null;
  currentDate: string;
  currentSlot: number | null;
}

export interface DailyTrendPoint extends ProviderMetrics {
  date: string;
  label: string;
}

/** One sampled point on the balance curve: a stock value in ¥. */
export interface BalanceTrendPoint {
  date: string;
  label: string;
  balanceCny: number;
  /** How many provider instances contributed to the sampled sum. */
  providers: number;
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

const INSTANCE_SUFFIX_PATTERN = /_(\d+)$/;

/**
 * `kimi_cn_2` → `kimi_cn`; bare ids (including `kimi_cn`) stay unchanged.
 * Only canonical suffixes (index ≥ 2, no leading zero) are stripped.
 */
export function baseProviderId(instanceId: string): string {
  const match = INSTANCE_SUFFIX_PATTERN.exec(instanceId);
  const suffix = match?.[1];
  if (!suffix) return instanceId;
  const index = Number(suffix);
  const canonical = index >= 2 && !suffix.startsWith("0");
  return canonical ? instanceId.slice(0, match.index) : instanceId;
}

/** Instance position within its provider: 1 for the bare base id. */
export function instanceIndexOf(instanceId: string): number {
  const base = baseProviderId(instanceId);
  return base === instanceId ? 1 : Number(instanceId.slice(base.length + 1));
}

/** True when `instanceId` belongs to `baseId`, e.g. `kimi_cn_2` → `kimi_cn`. */
export function isProviderInstanceId(instanceId: string, baseId: string): boolean {
  return baseProviderId(instanceId) === baseId;
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

/** Local-day 15-minute slot index 0..95 (e.g. 12:07 → 48). */
export function localQuarterSlot(date = new Date()): number {
  return Math.floor((date.getHours() * 60 + date.getMinutes()) / 15);
}

/** Format a 15-minute slot index as HH:MM (slot 48 → "12:00"). */
export function formatQuarterSlot(slot: number): string {
  const totalMinutes = Math.max(0, Math.min(95, Math.trunc(slot))) * 15;
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}`;
}

export function selectDailyTrend(
  records: DailyUsageRecord[],
  range: TrendRange,
  providerId: string,
  today = new Date(),
): DailyTrendPoint[] {
  const todayKey = localDateKey(today);
  const slotRank = (record: DailyUsageRecord): number => record.slot ?? -1;

  if (range === "24h") {
    // Intraday: 15-minute slots for the current local day only.
    const bySlot = new Map<number, ProviderMetrics[]>();
    for (const record of records) {
      if (record.date !== todayKey || record.slot == null) continue;
      if (providerId !== "all" && record.providerId !== providerId) continue;
      const metrics = bySlot.get(record.slot) ?? [];
      metrics.push(record);
      bySlot.set(record.slot, metrics);
    }
    return Array.from(bySlot, ([slot, metrics]) => ({
      date: todayKey,
      label: formatQuarterSlot(slot),
      ...summarizeProviders(metrics),
    }))
      .filter((point) => point.totalTokens !== null)
      .sort((left, right) => left.label.localeCompare(right.label));
  }

  // Daily: usage figures are same-day cumulative snapshots, so each day's
  // representative is the newest 15-minute sample — collapse intraday detail to
  // the latest slot per (date, provider), never sum slots.
  const cutoff = new Date(today.getFullYear(), today.getMonth(), today.getDate());
  cutoff.setDate(cutoff.getDate() - (range === "7d" ? 6 : 29));
  const cutoffKey = localDateKey(cutoff);

  const latest = new Map<string, DailyUsageRecord>();
  for (const record of records) {
    if (record.date > todayKey || (range !== "all" && record.date < cutoffKey)) continue;
    if (providerId !== "all" && record.providerId !== providerId) continue;
    const key = `${record.date}|${record.providerId}`;
    const existing = latest.get(key);
    if (!existing || slotRank(record) >= slotRank(existing)) latest.set(key, record);
  }

  const byDate = new Map<string, ProviderMetrics[]>();
  for (const record of latest.values()) {
    const metrics = byDate.get(record.date) ?? [];
    metrics.push(record);
    byDate.set(record.date, metrics);
  }

  return Array.from(byDate, ([date, metrics]) => ({
    date,
    label: date.slice(5).replace("-", "/"),
    ...summarizeProviders(metrics),
  }))
    .filter((point) => point.totalTokens !== null)
    .sort((left, right) => left.date.localeCompare(right.date));
}

function providerChangeValue(
  record: DailyUsageRecord,
  metric: ProviderChangeMetric,
): number | null {
  const value = metric === "requests"
    ? record.requests
    : metric === "tokens"
      ? record.totalTokens
      : metric === "balance"
        ? record.balanceCny
        : record.estimatedCostCny;
  return value != null && Number.isFinite(value) ? value : null;
}

/** Returns only providers that have at least one real sample for the selected
 * metric. Trend controls collapse instances to their base provider; recent
 * changes keep exact instance ids. */
export function selectProviderIdsWithMetric(
  records: DailyUsageRecord[],
  metric: ProviderChangeMetric,
  collapseInstances: boolean,
): string[] {
  const providerIds = new Set<string>();
  for (const record of records) {
    if (providerChangeValue(record, metric) == null) continue;
    providerIds.add(collapseInstances ? baseProviderId(record.providerId) : record.providerId);
  }
  return Array.from(providerIds).sort((left, right) => left.localeCompare(right));
}

/** Compares the two newest persisted samples for exactly one provider
 * instance. Same-day cumulative metrics never cross the local-day boundary;
 * balances are stocks and may be compared across days. */
export function selectLatestProviderChange(
  records: DailyUsageRecord[],
  providerId: string,
  metric: ProviderChangeMetric,
): ProviderRecentChange | null {
  const slotRank = (record: DailyUsageRecord): number => record.slot ?? -1;
  const samples = records
    .filter((record) => record.providerId === providerId && providerChangeValue(record, metric) != null)
    .sort((left, right) => left.date.localeCompare(right.date) || slotRank(left) - slotRank(right));
  const current = samples[samples.length - 1];
  if (!current) return null;

  const previous = samples
    .slice(0, -1)
    .reverse()
    .find((record) => metric === "balance" || record.date === current.date);
  if (!previous) return null;

  const previousValue = providerChangeValue(previous, metric);
  const currentValue = providerChangeValue(current, metric);
  if (previousValue == null || currentValue == null) return null;

  return {
    providerId,
    metric,
    previousValue,
    currentValue,
    delta: currentValue - previousValue,
    previousDate: previous.date,
    previousSlot: previous.slot,
    currentDate: current.date,
    currentSlot: current.slot,
  };
}

/** Balance curve samples. Balances are stocks, not flows: each (date, slot,
 *  provider) holds the balance at sync time, so a bucket's representative is
 *  the provider's LAST KNOWN balance — never a sum over time. Providers sync
 *  at different moments, so earlier buckets carry forward each provider's
 *  last sample; "all" sums every carrying instance into 合计余额. */
export function selectBalanceTrend(
  records: DailyUsageRecord[],
  range: TrendRange,
  providerId: string,
  today = new Date(),
): BalanceTrendPoint[] {
  const todayKey = localDateKey(today);
  const inProvider = (record: DailyUsageRecord): boolean =>
    providerId === "all" || isProviderInstanceId(record.providerId, providerId);

  // bucket key = slot index (24h) or date string (daily); one sample per
  // (bucket, provider), keeping the newest when keys collide.
  const samples = new Map<string, { order: number; providers: Map<string, number> }>();
  const cutoff = new Date(today.getFullYear(), today.getMonth(), today.getDate());
  cutoff.setDate(cutoff.getDate() - (range === "7d" ? 6 : 29));
  const cutoffKey = localDateKey(cutoff);
  for (const record of records) {
    if (record.balanceCny == null || !inProvider(record)) continue;
    let bucketKey: string;
    let order: number;
    if (range === "24h") {
      if (record.date !== todayKey || record.slot == null) continue;
      bucketKey = String(record.slot);
      order = record.slot;
    } else {
      if (record.date > todayKey || (range !== "all" && record.date < cutoffKey)) continue;
      bucketKey = record.date;
      order = Number(record.date.replace(/-/g, ""));
    }
    const bucket = samples.get(bucketKey) ?? { order, providers: new Map<string, number>() };
    bucket.providers.set(record.providerId, record.balanceCny);
    samples.set(bucketKey, bucket);
  }
  if (!samples.size) return [];

  // Walk buckets in time order, carrying each provider's last known balance
  // forward, and emit the per-bucket sum of everything seen so far.
  const carried = new Map<string, number>();
  const points: BalanceTrendPoint[] = [];
  const buckets = Array.from(samples.entries()).sort((left, right) => left[1].order - right[1].order);
  for (const [bucketKey, { providers }] of buckets) {
    for (const [instance, balance] of providers) carried.set(instance, balance);
    let sum = 0;
    for (const balance of carried.values()) sum += balance;
    points.push({
      date: range === "24h" ? todayKey : bucketKey,
      label: range === "24h"
        ? formatQuarterSlot(Number(bucketKey))
        : bucketKey.slice(5).replace("-", "/"),
      balanceCny: sum,
      providers: carried.size,
    });
  }
  return points;
}

/** How an instance's today spend was measured. */
export type TodaySpendSource = "cost-api" | "balance-diff";

/** One instance's today fund consumption in ¥. */
export interface TodaySpend {
  spendCny: number;
  source: TodaySpendSource;
}

/** Per-instance today's fund consumption. Official cost wins when the
 *  provider reports one; otherwise the spend is estimated by diffing today's
 *  balance samples against the last pre-today closing balance — decreases
 *  accumulate, recharges (increases) are clamped to zero, never negative. */
export function selectTodaySpend(
  records: DailyUsageRecord[],
  today = new Date(),
): Map<string, TodaySpend> {
  const todayKey = localDateKey(today);
  const slotRank = (record: DailyUsageRecord): number => record.slot ?? -1;
  const chronological = (left: DailyUsageRecord, right: DailyUsageRecord): number =>
    left.date.localeCompare(right.date) || slotRank(left) - slotRank(right);

  const byInstance = new Map<string, DailyUsageRecord[]>();
  for (const record of records) {
    if (record.date > todayKey) continue;
    const list = byInstance.get(record.providerId) ?? [];
    list.push(record);
    byInstance.set(record.providerId, list);
  }

  const spend = new Map<string, TodaySpend>();
  for (const [instanceId, instanceRecords] of byInstance) {
    // Rule 1: official cost — today's records carry same-day cumulative cost,
    // so the newest slot holding a figure is today's spend so far.
    const costRecords = instanceRecords
      .filter((record) => record.date === todayKey && record.estimatedCostCny != null)
      .sort(chronological);
    const latestCost = costRecords.length ? costRecords[costRecords.length - 1] : null;
    if (latestCost) {
      spend.set(instanceId, { spendCny: latestCost.estimatedCostCny ?? 0, source: "cost-api" });
      continue;
    }

    // Rule 2: balance diff — walk from the last pre-today closing balance
    // through today's samples in order and sum the clamped decreases.
    const samples = instanceRecords
      .filter((record): record is DailyUsageRecord & { balanceCny: number } =>
        record.balanceCny != null)
      .sort(chronological);
    let baseline: number | null = null;
    const todaySamples: number[] = [];
    for (const record of samples) {
      if (record.date < todayKey) baseline = record.balanceCny;
      else todaySamples.push(record.balanceCny);
    }
    // First day of tracking has no known opening balance, and an instance
    // that has not synced today must not inherit yesterday's spend.
    if (todaySamples.length === 0) continue;
    if (baseline == null && todaySamples.length < 2) continue;

    const series = baseline == null ? todaySamples : [baseline, ...todaySamples];
    let total = 0;
    let previous: number | undefined;
    for (const sample of series) {
      if (previous != null) total += Math.max(0, previous - sample);
      previous = sample;
    }
    spend.set(instanceId, { spendCny: total, source: "balance-diff" });
  }
  return spend;
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

/** `¥12.34` — round only at display time so aggregations never double-round. */
export function formatCny(value: number): string {
  return `¥${value.toFixed(2)}`;
}

export function formatProviderChangeValue(
  metric: ProviderChangeMetric,
  value: number,
  signed: boolean,
): string {
  if (!Number.isFinite(value)) return "—";
  const magnitude = metric === "balance" || metric === "cost"
    ? formatCny(Math.abs(value))
    : formatInteger(Math.abs(value));
  if (value < 0) return `-${magnitude}`;
  if (signed && value > 0) return `+${magnitude}`;
  return magnitude;
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
