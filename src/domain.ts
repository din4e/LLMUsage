export interface ProviderMetrics {
  requests: number | null;
  totalTokens: number | null;
  estimatedCostCny: number | null;
}

export function localDayRange(date = new Date()): { startTime: string; endTime: string } {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  const prefix = `${year}-${month}-${day}`;
  return { startTime: `${prefix} 00:00:00`, endTime: `${prefix} 23:59:59` };
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
  if (value < 10_000) return Math.round(value).toLocaleString("zh-CN");
  return `${(value / 10_000).toFixed(1)}万`;
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
