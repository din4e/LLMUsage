import { describe, expect, it } from "vitest";
import {
  baseProviderId,
  computeTimeWindowElapsedPercent,
  credentialHint,
  formatCooldown,
  formatCny,
  formatProviderChangeValue,
  formatDuration,
  formatInteger,
  formatQuotaDetailValue,
  formatQuarterSlot,
  formatResetRemainingText,
  instanceIndexOf,
  isProviderInstanceId,
  localDayRange,
  localDayRangeMs,
  localQuarterSlot,
  selectBalanceTrend,
  selectDailyTrend,
  selectLatestProviderChange,
  selectProviderIdsWithMetric,
  selectTodaySpend,
  summarizeProviders,
  type DailyUsageRecord,
} from "./domain";

describe("provider instance ids", () => {
  it("strips only canonical instance suffixes", () => {
    expect(baseProviderId("glm")).toBe("glm");
    expect(baseProviderId("glm_2")).toBe("glm");
    expect(baseProviderId("kimi_cn")).toBe("kimi_cn");
    expect(baseProviderId("kimi_cn_2")).toBe("kimi_cn");
    expect(baseProviderId("siliconflow_global_12")).toBe("siliconflow_global");
    expect(baseProviderId("glm_1")).toBe("glm_1");
    expect(baseProviderId("glm_02")).toBe("glm_02");
    expect(baseProviderId("kimi_cn_x")).toBe("kimi_cn_x");
  });

  it("numbers instances with 1 for the bare base id", () => {
    expect(instanceIndexOf("glm")).toBe(1);
    expect(instanceIndexOf("glm_2")).toBe(2);
    expect(instanceIndexOf("qwen_global_12")).toBe(12);
  });

  it("matches instances of the same base provider", () => {
    expect(isProviderInstanceId("kimi_cn", "kimi_cn")).toBe(true);
    expect(isProviderInstanceId("kimi_cn_2", "kimi_cn")).toBe(true);
    expect(isProviderInstanceId("kimi_global_2", "kimi_cn")).toBe(false);
    expect(isProviderInstanceId("kimi_cn", "kimi_global")).toBe(false);
  });
});

describe("formatCooldown", () => {
  it("formats a future reset using hours and minutes", () => {
    expect(formatCooldown(1_800_000, 0)).toBe("30 分钟后");
    expect(formatCooldown(9_000_000, 0)).toBe("2 小时 30 分后");
  });

  it("reports a reset that has already arrived", () => {
    expect(formatCooldown(999, 1_000)).toBe("即将恢复");
  });
});

describe("formatInteger", () => {
  it("uses M and B units without losing small values", () => {
    expect(formatInteger(999)).toBe("999");
    expect(formatInteger(12_345)).toBe("12,345");
    expect(formatInteger(12_345_678)).toBe("12.3M");
    expect(formatInteger(1_234_567_890)).toBe("1.2B");
  });
});

describe("summarizeProviders", () => {
  it("only totals metrics that are actually available", () => {
    expect(
      summarizeProviders([
        { requests: 7, totalTokens: 1_000, estimatedCostCny: 0.25 },
        { requests: null, totalTokens: null, estimatedCostCny: null },
        { requests: 3, totalTokens: 500, estimatedCostCny: 0.1 },
      ]),
    ).toEqual({ requests: 10, totalTokens: 1_500, estimatedCostCny: 0.35 });
  });
});

describe("localDayRange", () => {
  it("formats the selected local calendar day for GLM monitor queries", () => {
    expect(localDayRange(new Date(2026, 6, 10, 14, 30))).toEqual({
      startTime: "2026-07-10 00:00:00",
      endTime: "2026-07-10 23:59:59",
    });
  });

  it("returns exact local midnight boundaries for online analytics", () => {
    const date = new Date(2026, 6, 10, 14, 30);
    expect(localDayRangeMs(date)).toEqual({
      startTimeMs: new Date(2026, 6, 10).getTime(),
      endTimeMs: new Date(2026, 6, 11).getTime(),
    });
  });
});

describe("selectDailyTrend", () => {
  const records: DailyUsageRecord[] = [
    { date: "2026-07-06", slot: null, providerId: "glm", requests: 2, totalTokens: 200, estimatedCostCny: null },
    { date: "2026-07-07", slot: null, providerId: "glm", requests: 3, totalTokens: 300, estimatedCostCny: null },
    { date: "2026-07-07", slot: null, providerId: "openai_codex", requests: 4, totalTokens: 700, estimatedCostCny: 1.2 },
    { date: "2026-07-13", slot: null, providerId: "openai_codex", requests: 5, totalTokens: 900, estimatedCostCny: 1.8 },
    { date: "2026-07-14", slot: null, providerId: "openai_codex", requests: 99, totalTokens: 99_000, estimatedCostCny: 99 },
  ];

  it("keeps the latest seven local calendar days and aggregates providers by date", () => {
    expect(selectDailyTrend(records, "7d", "all", new Date(2026, 6, 13))).toEqual([
      { date: "2026-07-07", label: "07/07", requests: 7, totalTokens: 1_000, estimatedCostCny: 1.2 },
      { date: "2026-07-13", label: "07/13", requests: 5, totalTokens: 900, estimatedCostCny: 1.8 },
    ]);
  });

  it("filters one provider and preserves the full available history", () => {
    expect(selectDailyTrend(records, "all", "glm", new Date(2026, 6, 13))).toEqual([
      { date: "2026-07-06", label: "07/06", requests: 2, totalTokens: 200, estimatedCostCny: null },
      { date: "2026-07-07", label: "07/07", requests: 3, totalTokens: 300, estimatedCostCny: null },
    ]);
  });

  it("does not render balance-only observations as zero Token consumption", () => {
    expect(selectDailyTrend([
      { date: "2026-07-13", slot: null, providerId: "deepseek", requests: null, totalTokens: null, estimatedCostCny: null },
    ], "7d", "deepseek", new Date(2026, 6, 13))).toEqual([]);
  });
});

describe("15-minute slot helpers", () => {
  it("maps local time to a 0..95 slot and back to HH:MM", () => {
    expect(localQuarterSlot(new Date(2026, 6, 13, 0, 0))).toBe(0);
    expect(localQuarterSlot(new Date(2026, 6, 13, 12, 7))).toBe(48);
    expect(localQuarterSlot(new Date(2026, 6, 13, 23, 59))).toBe(95);
    expect(formatQuarterSlot(0)).toBe("00:00");
    expect(formatQuarterSlot(48)).toBe("12:00");
    expect(formatQuarterSlot(95)).toBe("23:45");
  });
});

describe("selectBalanceTrend", () => {
  it("sums provider closing balances per day and carries the last known value", () => {
    const records: DailyUsageRecord[] = [
      // Day 1: Kimi 50 + MiniMax 30 = 80.
      { date: "2026-07-11", slot: 20, providerId: "kimi_cn", requests: null, totalTokens: null, estimatedCostCny: null, balanceCny: 50 },
      { date: "2026-07-11", slot: 44, providerId: "minimax_cn", requests: null, totalTokens: null, estimatedCostCny: null, balanceCny: 30 },
      // Day 2: only Kimi syncs; MiniMax's 30 carries forward → 45 + 30 = 75.
      { date: "2026-07-12", slot: 10, providerId: "kimi_cn", requests: null, totalTokens: null, estimatedCostCny: null, balanceCny: 45 },
      // Day 3: latest slot of the day wins for Kimi (48, not 40) → 40 + 30 = 70.
      { date: "2026-07-13", slot: 40, providerId: "kimi_cn", requests: null, totalTokens: null, estimatedCostCny: null, balanceCny: 42 },
      { date: "2026-07-13", slot: 48, providerId: "kimi_cn", requests: null, totalTokens: null, estimatedCostCny: null, balanceCny: 40 },
    ];
    expect(selectBalanceTrend(records, "all", "all", new Date(2026, 6, 13))).toEqual([
      { date: "2026-07-11", label: "07/11", balanceCny: 80, providers: 2 },
      { date: "2026-07-12", label: "07/12", balanceCny: 75, providers: 2 },
      { date: "2026-07-13", label: "07/13", balanceCny: 70, providers: 2 },
    ]);
  });

  it("sums every instance of one provider and ignores providers without balance", () => {
    const records: DailyUsageRecord[] = [
      { date: "2026-07-12", slot: null, providerId: "kimi_cn", balanceCny: 50, requests: 1, totalTokens: 10, estimatedCostCny: null },
      { date: "2026-07-12", slot: null, providerId: "kimi_cn_2", requests: null, totalTokens: null, estimatedCostCny: null, balanceCny: 25 },
      { date: "2026-07-12", slot: null, providerId: "glm", requests: 5, totalTokens: 500, estimatedCostCny: null },
    ];
    expect(selectBalanceTrend(records, "all", "kimi_cn", new Date(2026, 6, 13))).toEqual([
      { date: "2026-07-12", label: "07/12", balanceCny: 75, providers: 2 },
    ]);
  });

  it("carries intraday slot balances forward across providers in 24h mode", () => {
    const records: DailyUsageRecord[] = [
      { date: "2026-07-13", slot: 8, providerId: "kimi_cn", requests: null, totalTokens: null, estimatedCostCny: null, balanceCny: 50 },
      { date: "2026-07-13", slot: 12, providerId: "minimax_cn", requests: null, totalTokens: null, estimatedCostCny: null, balanceCny: 30 },
      { date: "2026-07-13", slot: 16, providerId: "kimi_cn", requests: null, totalTokens: null, estimatedCostCny: null, balanceCny: 45 },
      // Yesterday's samples must not leak into today's curve.
      { date: "2026-07-12", slot: 80, providerId: "kimi_cn", requests: null, totalTokens: null, estimatedCostCny: null, balanceCny: 99 },
    ];
    expect(selectBalanceTrend(records, "24h", "all", new Date(2026, 6, 13, 20, 0))).toEqual([
      { date: "2026-07-13", label: "02:00", balanceCny: 50, providers: 1 },
      { date: "2026-07-13", label: "03:00", balanceCny: 80, providers: 2 },
      { date: "2026-07-13", label: "04:00", balanceCny: 75, providers: 2 },
    ]);
  });

  it("returns nothing when no sampled balances exist", () => {
    expect(selectBalanceTrend([
      { date: "2026-07-13", slot: null, providerId: "glm", requests: 5, totalTokens: 500, estimatedCostCny: null },
    ], "all", "all", new Date(2026, 6, 13))).toEqual([]);
  });
});

describe("selectTodaySpend", () => {
  const today = new Date(2026, 6, 13, 20, 0);
  const balance = (date: string, slot: number | null, providerId: string, balanceCny: number): DailyUsageRecord => ({
    date, slot, providerId, requests: null, totalTokens: null, estimatedCostCny: null, balanceCny,
  });

  it("uses the latest official cost recorded today", () => {
    const records: DailyUsageRecord[] = [
      { date: "2026-07-12", slot: 40, providerId: "openai_codex", requests: 9, totalTokens: 900, estimatedCostCny: 9 },
      { date: "2026-07-13", slot: 48, providerId: "openai_codex", requests: 2, totalTokens: 200, estimatedCostCny: 0.5 },
      { date: "2026-07-13", slot: 52, providerId: "openai_codex", requests: 3, totalTokens: 300, estimatedCostCny: 0.8 },
    ];
    expect(selectTodaySpend(records, today).get("openai_codex"))
      .toEqual({ spendCny: 0.8, source: "cost-api" });
  });

  it("estimates balance-diff spend and clamps recharges to zero", () => {
    const records: DailyUsageRecord[] = [
      balance("2026-07-12", 88, "deepseek", 100),
      balance("2026-07-13", 20, "deepseek", 80),
      balance("2026-07-13", 40, "deepseek", 180),
      balance("2026-07-13", 60, "deepseek", 150),
    ];
    // 100→80 (¥20) + 充值 80→180 (不计) + 180→150 (¥30) = ¥50。
    expect(selectTodaySpend(records, today).get("deepseek"))
      .toEqual({ spendCny: 50, source: "balance-diff" });
  });

  it("diffs against yesterday's closing (latest slot) balance", () => {
    const records: DailyUsageRecord[] = [
      balance("2026-07-12", 10, "ppio", 40),
      balance("2026-07-12", 48, "ppio", 42),
      balance("2026-07-13", 30, "ppio", 39),
    ];
    expect(selectTodaySpend(records, today).get("ppio"))
      .toEqual({ spendCny: 3, source: "balance-diff" });
  });

  it("skips the first tracked day until two samples exist", () => {
    expect(selectTodaySpend([
      balance("2026-07-13", 20, "kimi_cn", 50),
    ], today).has("kimi_cn")).toBe(false);
    expect(selectTodaySpend([
      balance("2026-07-13", 20, "kimi_cn", 50),
      balance("2026-07-13", 44, "kimi_cn", 44),
    ], today).get("kimi_cn")).toEqual({ spendCny: 6, source: "balance-diff" });
  });

  it("keeps an instance that only recharged today at zero spend", () => {
    expect(selectTodaySpend([
      balance("2026-07-12", 80, "siliconflow_cn", 20),
      balance("2026-07-13", 30, "siliconflow_cn", 120),
    ], today).get("siliconflow_cn")).toEqual({ spendCny: 0, source: "balance-diff" });
  });

  it("does not inherit spend when the instance has not synced today", () => {
    expect(selectTodaySpend([
      balance("2026-07-12", 80, "deepseek", 100),
    ], today).size).toBe(0);
  });

  it("omits instances without balance or cost signals", () => {
    expect(selectTodaySpend([
      { date: "2026-07-13", slot: 30, providerId: "glm", requests: 5, totalTokens: 500, estimatedCostCny: null },
    ], today).size).toBe(0);
  });

  it("ignores future-dated records", () => {
    expect(selectTodaySpend([
      balance("2026-07-14", 10, "deepseek", 10),
    ], today).size).toBe(0);
  });
});

describe("selectLatestProviderChange", () => {
  it("isolates one provider instance and diffs the latest same-day request samples", () => {
    const records: DailyUsageRecord[] = [
      { date: "2026-08-24", slot: 20, providerId: "glm", requests: 10, totalTokens: 1_000, estimatedCostCny: null },
      { date: "2026-08-24", slot: 24, providerId: "glm_2", requests: 99, totalTokens: 9_900, estimatedCostCny: null },
      { date: "2026-08-24", slot: 28, providerId: "glm", requests: 13, totalTokens: 1_600, estimatedCostCny: null },
    ];

    expect(selectLatestProviderChange(records, "glm", "requests")).toEqual({
      providerId: "glm",
      metric: "requests",
      previousValue: 10,
      currentValue: 13,
      delta: 3,
      previousDate: "2026-08-24",
      previousSlot: 20,
      currentDate: "2026-08-24",
      currentSlot: 28,
    });
  });

  it("never compares cumulative Token values across a local-day reset", () => {
    const records: DailyUsageRecord[] = [
      { date: "2026-08-23", slot: 92, providerId: "glm", requests: 20, totalTokens: 8_000, estimatedCostCny: null },
      { date: "2026-08-24", slot: 4, providerId: "glm", requests: 1, totalTokens: 200, estimatedCostCny: null },
    ];

    expect(selectLatestProviderChange(records, "glm", "tokens")).toBeNull();
  });

  it("compares balances across days and keeps consumption as a negative delta", () => {
    const records: DailyUsageRecord[] = [
      { date: "2026-08-24", slot: 12, providerId: "deepseek", requests: null, totalTokens: null, estimatedCostCny: null, balanceCny: 88.5 },
      { date: "2026-08-23", slot: 80, providerId: "deepseek", requests: null, totalTokens: null, estimatedCostCny: null, balanceCny: 100 },
      { date: "2026-08-24", slot: 8, providerId: "deepseek", requests: null, totalTokens: null, estimatedCostCny: null, balanceCny: 90 },
    ];

    expect(selectLatestProviderChange(records, "deepseek", "balance")).toMatchObject({
      previousValue: 90,
      currentValue: 88.5,
      delta: -1.5,
      previousDate: "2026-08-24",
      previousSlot: 8,
      currentDate: "2026-08-24",
      currentSlot: 12,
    });
  });

  it("diffs official cost within the day and returns null for unsupported metrics", () => {
    const records: DailyUsageRecord[] = [
      { date: "2026-08-24", slot: 20, providerId: "openai_codex", requests: 2, totalTokens: 200, estimatedCostCny: 0.5 },
      { date: "2026-08-24", slot: 24, providerId: "openai_codex", requests: 3, totalTokens: 350, estimatedCostCny: 0.8 },
    ];

    const cost = selectLatestProviderChange(records, "openai_codex", "cost");
    expect(cost).toMatchObject({
      previousValue: 0.5,
      currentValue: 0.8,
    });
    expect(cost?.delta).toBeCloseTo(0.3);
    expect(selectLatestProviderChange(records, "openai_codex", "balance")).toBeNull();
  });
});

describe("selectProviderIdsWithMetric", () => {
  const records: DailyUsageRecord[] = [
    { date: "2026-08-24", slot: 70, providerId: "glm", requests: 3, totalTokens: 300, estimatedCostCny: null },
    { date: "2026-08-24", slot: 70, providerId: "glm_2", requests: 4, totalTokens: 400, estimatedCostCny: null },
    { date: "2026-08-24", slot: 70, providerId: "deepseek", requests: null, totalTokens: null, estimatedCostCny: null, balanceCny: 20 },
  ];

  it("keeps balance-only providers out of the Token selector", () => {
    expect(selectProviderIdsWithMetric(records, "tokens", true)).toEqual(["glm"]);
  });

  it("keeps Token-only providers out of the balance selector", () => {
    expect(selectProviderIdsWithMetric(records, "balance", true)).toEqual(["deepseek"]);
  });

  it("preserves instance ids for the recent-change selector", () => {
    expect(selectProviderIdsWithMetric(records, "tokens", false)).toEqual(["glm", "glm_2"]);
  });
});

describe("formatProviderChangeValue", () => {
  it("formats signed count and Token deltas", () => {
    expect(formatProviderChangeValue("requests", 3, true)).toBe("+3");
    expect(formatProviderChangeValue("tokens", -1_200, true)).toBe("-1,200");
    expect(formatProviderChangeValue("tokens", 0, true)).toBe("0");
  });

  it("formats current and signed currency values", () => {
    expect(formatProviderChangeValue("balance", 88.5, false)).toBe("¥88.50");
    expect(formatProviderChangeValue("balance", -1.5, true)).toBe("-¥1.50");
    expect(formatProviderChangeValue("cost", 0.3, true)).toBe("+¥0.30");
  });
});

describe("formatCny", () => {
  it("rounds to two decimals at display time", () => {
    expect(formatCny(12)).toBe("¥12.00");
    expect(formatCny(49.999999)).toBe("¥50.00");
  });
});

describe("selectDailyTrend intraday + latest-slot collapse", () => {
  it("buckets today's 15-minute slots and labels them HH:MM", () => {
    const today = new Date(2026, 6, 13, 20, 0);
    const records: DailyUsageRecord[] = [
      { date: "2026-07-13", slot: 48, providerId: "glm", requests: 1, totalTokens: 100, estimatedCostCny: null },
      { date: "2026-07-13", slot: 48, providerId: "openai_codex", requests: 2, totalTokens: 200, estimatedCostCny: 0.5 },
      { date: "2026-07-13", slot: 52, providerId: "glm", requests: 3, totalTokens: 500, estimatedCostCny: null },
      { date: "2026-07-12", slot: 10, providerId: "glm", requests: 9, totalTokens: 9_999, estimatedCostCny: null },
    ];
    expect(selectDailyTrend(records, "24h", "all", today)).toEqual([
      { date: "2026-07-13", label: "12:00", requests: 3, totalTokens: 300, estimatedCostCny: 0.5 },
      { date: "2026-07-13", label: "13:00", requests: 3, totalTokens: 500, estimatedCostCny: null },
    ]);
  });

  it("uses the latest 15-minute slot as the day's value in daily ranges", () => {
    const today = new Date(2026, 6, 13, 12, 0);
    const records: DailyUsageRecord[] = [
      { date: "2026-07-13", slot: 10, providerId: "glm", requests: 1, totalTokens: 100, estimatedCostCny: null },
      { date: "2026-07-13", slot: 40, providerId: "glm", requests: 5, totalTokens: 800, estimatedCostCny: null },
      { date: "2026-07-13", slot: 20, providerId: "glm", requests: 2, totalTokens: 300, estimatedCostCny: null },
    ];
    expect(selectDailyTrend(records, "7d", "glm", today)).toEqual([
      { date: "2026-07-13", label: "07/13", requests: 5, totalTokens: 800, estimatedCostCny: null },
    ]);
  });
});

describe("credentialHint", () => {
  it("distinguishes subscription keys from pay-as-you-go keys", () => {
    expect(credentialHint("kimi_cn")).toContain("sk-kimi-");
    expect(credentialHint("kimi_cn")).toContain("Moonshot");
    expect(credentialHint("minimax_cn")).toContain("sk-cp-");
    expect(credentialHint("minimax_cn")).toContain("按量 API Key 不可查询");
  });
});

describe("quota detail formatting", () => {
  it("keeps used, remaining and limit values visible", () => {
    expect(
      formatQuotaDetailValue({
        label: "5 小时窗口",
        used: "7",
        remaining: "93",
        limit: "100",
        unit: "%",
      }),
    ).toBe("已用 7% · 剩余 93% · 上限 100%");
  });

  it("formats provider window durations compactly", () => {
    expect(formatDuration(600_000)).toBe("10 分钟");
    expect(formatDuration(9_000_000)).toBe("2 小时 30 分钟");
  });
});

describe("time window progress", () => {
  it("computes elapsed time percent for a window", () => {
    const start = new Date("2026-07-13T00:00:00Z").getTime();
    const reset = new Date("2026-07-20T00:00:00Z").getTime();
    const now = new Date("2026-07-15T12:00:00Z").getTime();

    const percent = computeTimeWindowElapsedPercent(start, reset, now);
    expect(percent).toBeCloseTo(((now - start) / (reset - start)) * 100, 1);
  });

  it("returns null for invalid windows", () => {
    expect(computeTimeWindowElapsedPercent(100, 100)).toBeNull();
    expect(computeTimeWindowElapsedPercent(NaN, 100)).toBeNull();
  });

  it("formats remaining time text from a reset timestamp", () => {
    const reset = new Date("2026-07-20T00:00:00Z").getTime();
    const now = new Date("2026-07-15T12:00:00Z").getTime();

    const text = formatResetRemainingText(reset, now);
    expect(text).toContain("周三");
    expect(text).toContain("剩余");
  });

  it("returns null for invalid reset timestamps", () => {
    expect(formatResetRemainingText(NaN)).toBeNull();
  });
});
