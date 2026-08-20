import { providerDefinition } from "./providers";

/** Mirrors the Rust `ImportEntryResult` returned by `import_provider_backup`. */
export interface ImportEntryResult {
  sourceProviderId: string;
  assignedInstanceId?: string | null;
  remark?: string | null;
  outcome: "saved" | "skipped" | "invalid";
  reason?: string | null;
}

/** One-line summary for the status bar; empty input is not an error. */
export function importSummaryText(results: readonly ImportEntryResult[]): string {
  if (!results.length) return "没有可导入的实例";
  const saved = results.filter((result) => result.outcome === "saved").length;
  const skipped = results.filter((result) => result.outcome === "skipped").length;
  const invalid = results.filter((result) => result.outcome === "invalid").length;
  const suffix = saved > 0 ? " · 点击 ↻ 立即同步" : "";
  return `已导入 ${saved} · 跳过 ${skipped} · 无效 ${invalid}${suffix}`;
}

/** Per-entry lines for the import result dialog, keyed by display name. */
export function importResultLines(results: readonly ImportEntryResult[]): string[] {
  return results.map((result) => {
    const name = importEntryName(result.sourceProviderId);
    const target =
      result.assignedInstanceId && result.assignedInstanceId !== result.sourceProviderId
        ? ` → ${result.assignedInstanceId}`
        : "";
    if (result.outcome === "saved") return `${name}${target}：已导入`;
    const label = result.outcome === "skipped" ? "跳过" : "无效";
    return `${name}${target}：${label} · ${result.reason ?? "未知原因"}`;
  });
}

function importEntryName(sourceProviderId: string): string {
  const provider = providerDefinition(sourceProviderId);
  return provider ? provider.name : sourceProviderId;
}

/** Remarks payload for the export command: configured instances, non-empty only. */
export function buildExportRemarks(
  instanceRemarks: ReadonlyMap<string, string>,
  configured: ReadonlySet<string>,
): Record<string, string> {
  const remarks: Record<string, string> = {};
  for (const [instanceId, remark] of instanceRemarks) {
    if (configured.has(instanceId) && remark.trim()) remarks[instanceId] = remark;
  }
  return remarks;
}

/** Default save-dialog filename per mode, dated with the local date key. */
export function exportDefaultFileName(mode: "full" | "status", localDate: string): string {
  const kind = mode === "full" ? "backup" : "status";
  return `llm-usage-${kind}-${localDate}.json`;
}
