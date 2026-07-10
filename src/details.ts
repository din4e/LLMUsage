import {
  formatDuration,
  formatQuotaDetailValue,
  type OnlineDetailEntry,
  type OnlineDetailSection,
} from "./domain";

export function renderProviderDetails(
  providerId: string,
  sections: OnlineDetailSection[] | undefined,
) {
  const root = document.getElementById(`${providerId}-details`);
  if (!root) return;
  const wasOpen = root.querySelector("details")?.open ?? false;
  root.replaceChildren();
  if (!sections?.length) {
    root.hidden = true;
    return;
  }

  const details = document.createElement("details");
  details.className = "detail-disclosure";
  details.open = wasOpen;
  const summary = document.createElement("summary");
  const entryCount = sections.reduce((total, section) => total + section.entries.length, 0);
  summary.textContent = `完整明细 · ${entryCount} 项`;
  const content = document.createElement("div");
  content.className = "detail-content";

  for (const detailSection of sections) {
    const section = document.createElement("section");
    section.className = "detail-section";
    const heading = document.createElement("h4");
    heading.textContent = detailSection.title;
    const grid = document.createElement("div");
    grid.className = "detail-grid";
    for (const entry of detailSection.entries) grid.append(renderDetailEntry(entry));
    section.append(heading, grid);
    content.append(section);
  }
  details.append(summary, content);
  root.append(details);
  root.hidden = false;
}

function renderDetailEntry(entry: OnlineDetailEntry): HTMLElement {
  const card = document.createElement("article");
  card.className = "detail-entry";
  card.setAttribute("aria-label", `${entry.label}：${formatQuotaDetailValue(entry)}`);

  const heading = document.createElement("div");
  heading.className = "detail-entry-heading";
  const label = document.createElement("strong");
  label.textContent = entry.label;
  heading.append(label);
  if (isPercent(entry.usedPercent)) {
    const percent = document.createElement("span");
    percent.textContent = `${entry.usedPercent.toFixed(1)}%`;
    heading.append(percent);
  }

  const values = document.createElement("dl");
  appendValue(values, "已用", entry.used, entry.unit);
  appendValue(values, "剩余", entry.remaining, entry.unit);
  appendValue(values, "上限", entry.limit, entry.unit);
  card.append(heading, values);

  if (isPercent(entry.usedPercent)) {
    const progress = document.createElement("progress");
    progress.max = 100;
    progress.value = entry.usedPercent;
    progress.setAttribute("aria-label", `${entry.label}已用比例`);
    card.append(progress);
  }

  const metadata = detailMetadata(entry);
  if (metadata.length) {
    const meta = document.createElement("p");
    meta.className = "detail-meta";
    meta.textContent = metadata.join(" · ");
    card.append(meta);
  }
  return card;
}

function appendValue(root: HTMLDListElement, label: string, value: string | null | undefined, unit: string) {
  if (value == null) return;
  const item = document.createElement("div");
  const term = document.createElement("dt");
  term.textContent = label;
  const description = document.createElement("dd");
  description.textContent = `${value}${unit}`;
  item.append(term, description);
  root.append(item);
}

function detailMetadata(entry: OnlineDetailEntry): string[] {
  const values: string[] = [];
  if (entry.window) values.push(entry.window);
  if (validTimestamp(entry.startAtMs)) values.push(`开始 ${formatLocalTime(entry.startAtMs)}`);
  if (validTimestamp(entry.resetAtMs)) values.push(`重置 ${formatLocalTime(entry.resetAtMs)}`);
  if (entry.remainingMs != null && Number.isFinite(entry.remainingMs) && entry.remainingMs >= 0) {
    values.push(`剩余时间 ${formatDuration(entry.remainingMs)}`);
  }
  return values;
}

function formatLocalTime(timestampMs: number): string {
  const date = new Date(timestampMs);
  if (!Number.isFinite(date.getTime())) return "时间无效";
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  }).format(date);
}

function isPercent(value: number | null | undefined): value is number {
  return value != null && Number.isFinite(value) && value >= 0 && value <= 100;
}

function validTimestamp(value: number | null | undefined): value is number {
  return value != null && Number.isFinite(value) && value > 0 && value <= 8_640_000_000_000_000;
}
