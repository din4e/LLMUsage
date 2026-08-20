import { describe, expect, it } from "vitest";
import {
  buildExportRemarks,
  exportDefaultFileName,
  importResultLines,
  importSummaryText,
  type ImportEntryResult,
} from "./transfer";

const entry = (overrides: Partial<ImportEntryResult>): ImportEntryResult => ({
  sourceProviderId: "kimi_cn",
  assignedInstanceId: null,
  remark: null,
  outcome: "saved",
  reason: null,
  ...overrides,
});

describe("importSummaryText", () => {
  it("counts each outcome and nudges toward a manual sync", () => {
    const results = [
      entry({}),
      entry({ sourceProviderId: "deepseek", assignedInstanceId: "deepseek_2" }),
      entry({ sourceProviderId: "glm", outcome: "skipped", reason: "状态报告不含凭据" }),
      entry({ sourceProviderId: "mistral", outcome: "invalid", reason: "供应商不受支持或已下线" }),
    ];

    expect(importSummaryText(results)).toBe("已导入 2 · 跳过 1 · 无效 1 · 点击 ↻ 立即同步");
  });

  it("omits the sync nudge when nothing was saved", () => {
    const results = [entry({ outcome: "skipped", reason: "状态报告不含凭据" })];

    expect(importSummaryText(results)).toBe("已导入 0 · 跳过 1 · 无效 0");
  });

  it("treats an empty import as guidance, not an error", () => {
    expect(importSummaryText([])).toBe("没有可导入的实例");
  });
});

describe("importResultLines", () => {
  it("uses provider names and shows only reassigned ids", () => {
    const results = [
      entry({}),
      entry({ sourceProviderId: "deepseek", assignedInstanceId: "deepseek_2" }),
    ];

    expect(importResultLines(results)).toEqual([
      "Kimi Code：已导入",
      "DeepSeek → deepseek_2：已导入",
    ]);
  });

  it("carries the backend reason for skipped and invalid entries", () => {
    const results = [
      entry({ sourceProviderId: "glm", outcome: "skipped", reason: "状态报告不含凭据" }),
      entry({ sourceProviderId: "xai", outcome: "invalid", reason: "凭据格式无效" }),
    ];

    expect(importResultLines(results)).toEqual([
      "智谱 GLM：跳过 · 状态报告不含凭据",
      "xAI / Grok：无效 · 凭据格式无效",
    ]);
  });

  it("falls back to the raw id for unknown providers", () => {
    const results = [entry({ sourceProviderId: "future_provider", outcome: "invalid", reason: "供应商不受支持或已下线" })];

    expect(importResultLines(results)).toEqual([
      "future_provider：无效 · 供应商不受支持或已下线",
    ]);
  });
});

describe("buildExportRemarks", () => {
  it("keeps remarks of configured instances only", () => {
    const remarks = new Map([
      ["kimi_cn", "工作账号"],
      ["deepseek", "  "],
      ["deleted_provider", "残留"],
    ]);

    expect(buildExportRemarks(remarks, new Set(["kimi_cn", "deepseek"]))).toEqual({
      kimi_cn: "工作账号",
    });
  });

  it("handles empty inputs", () => {
    expect(buildExportRemarks(new Map(), new Set())).toEqual({});
  });
});

describe("exportDefaultFileName", () => {
  it("names backups and status reports distinctly", () => {
    expect(exportDefaultFileName("full", "2026-08-20")).toBe("llm-usage-backup-2026-08-20.json");
    expect(exportDefaultFileName("status", "2026-08-20")).toBe("llm-usage-status-2026-08-20.json");
  });
});
