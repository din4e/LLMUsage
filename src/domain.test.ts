import { describe, expect, it } from "vitest";
import {
  credentialHint,
  formatCooldown,
  formatDuration,
  formatInteger,
  formatQuotaDetailValue,
  localDayRange,
  localDayRangeMs,
  summarizeProviders,
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
