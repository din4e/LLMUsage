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
