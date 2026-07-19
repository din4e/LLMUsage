import { describe, expect, it } from "vitest";
import {
  computeTimeWindowElapsedPercent,
  credentialHint,
  formatCooldown,
  formatDuration,
  formatInteger,
  formatQuotaDetailValue,
  formatQuarterSlot,
  formatResetRemainingText,
  localDayRange,
  localDayRangeMs,
  localQuarterSlot,
  selectDailyTrend,
  summarizeProviders,
  type DailyUsageRecord,
} from "./domain";

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
