import { invoke, isTauri } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { listen } from "@tauri-apps/api/event";
import { disable as disableAutostart, enable as enableAutostart, isEnabled as isAutostartEnabled } from "@tauri-apps/plugin-autostart";
import { open as openFileDialog, save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import { renderProviderDetails } from "./details";
import {
  baseProviderId,
  formatCny,
  formatCooldown,
  formatInteger,
  formatProviderChangeValue,
  formatQuarterSlot,
  instanceIndexOf,
  isProviderInstanceId,
  localDateKey,
  localDayRange,
  localDayRangeMs,
  localQuarterSlot,
  selectBalanceTrend,
  selectDailyTrend,
  selectLatestProviderChange,
  selectProviderChangeSeries,
  selectTodaySpend,
  selectProviderIdsWithMetric,
  summarizeProviders,
  type DailyUsageRecord,
  type OnlineDetailSection,
  type ProviderChangeMetric,
  type TrendRange,
} from "./domain";
import {
  INSTANCE_REMARK_MAX_LENGTH,
  deserializeProviderCredential,
  hasConfiguredInstance,
  instanceBadgeLabel,
  instanceDisplayName,
  nextInstanceId,
  providerDefinition,
  providerDefinitions,
  sanitizeInstanceRemark,
  serializeProviderCredential,
} from "./providers";
import {
  buildExportRemarks,
  exportDefaultFileName,
  importResultLines,
  importSummaryText,
  type ImportEntryResult,
} from "./transfer";
import { initializeWindowControls } from "./window-controls";
import { renderBalanceTrendChart, renderDailyTrendChart, renderProviderChangeChart } from "./trend-chart";
import "./styles.css";

interface GlmSnapshot {
  planLevel: string;
  usedPercent: number;
  cooldownEndsAtMs: number;
  requests: number;
  totalTokens: number;
  detailSections?: OnlineDetailSection[];
}

interface OnlineSnapshot {
  providerId: string;
  label: string;
  source: string;
  experimental: boolean;
  balanceCny?: number | null;
  quotaUsedPercent?: number | null;
  cooldownEndsAtMs?: number | null;
  requests?: number | null;
  totalTokens?: number | null;
  estimatedCostCny?: number | null;
  primaryLabel: string;
  primaryValue: string;
  secondaryValue: string;
  detailSections?: OnlineDetailSection[];
}

interface CommandError {
  code?: string;
  message?: string;
}

interface CachedSnapshot {
  providerId: string;
  kind: "glm" | "online";
  savedAtMs: number;
  snapshot: unknown;
}

const byId = <T extends HTMLElement>(id: string) => document.getElementById(id) as T | null;
const refreshButton = byId<HTMLButtonElement>("refresh-button");
const syncStatus = byId<HTMLElement>("sync-status");
const themeButton = byId<HTMLButtonElement>("theme-toggle");
const autostartToggle = byId<HTMLButtonElement>("autostart-toggle");
const autoSyncOptions = byId<HTMLElement>("auto-sync-options");
const dialog = byId<HTMLDialogElement>("provider-dialog");
const providerForm = byId<HTMLFormElement>("provider-form");
const dialogTitle = byId<HTMLElement>("dialog-title");
const dialogCopy = byId<HTMLElement>("dialog-copy");
const credentialFields = byId<HTMLElement>("credential-fields");
const saveButton = byId<HTMLButtonElement>("save-provider");
const catalogDialog = byId<HTMLDialogElement>("catalog-dialog");
const providerCatalog = byId<HTMLElement>("provider-catalog");
const providerList = byId<HTMLElement>("provider-list");
const providerEmpty = byId<HTMLElement>("provider-empty");
const pageProviderCatalog = byId<HTMLElement>("page-provider-catalog");
const providerInstances = byId<HTMLElement>("provider-instances");
const providerInstancesEmpty = byId<HTMLElement>("provider-instances-empty");
const trendProvider = byId<HTMLSelectElement>("trend-provider");
const trendRange = byId<HTMLElement>("trend-range");
const trendMetric = byId<HTMLElement>("trend-metric");
const trendTitle = byId<HTMLElement>("trend-title");
const trendChart = document.getElementById("trend-chart") as SVGSVGElement | null;
const trendEmpty = byId<HTMLElement>("trend-empty");
const trendDescription = byId<HTMLElement>("trend-description");
const recentChangeProvider = byId<HTMLSelectElement>("recent-change-provider");
const recentChangeMetric = byId<HTMLElement>("recent-change-metric");
const recentChangeValues = byId<HTMLElement>("recent-change-values");
const recentChangeEmpty = byId<HTMLElement>("recent-change-empty");
const recentChangeLabel = byId<HTMLElement>("recent-change-label");
const recentChangeDelta = byId<HTMLElement>("recent-change-delta");
const recentChangeDirection = byId<HTMLElement>("recent-change-direction");
const recentChangeCurrent = byId<HTMLElement>("recent-change-current");
const recentChangePrevious = byId<HTMLElement>("recent-change-previous");
const recentChangePeriod = byId<HTMLElement>("recent-change-period");
const recentChangeChart = document.getElementById("recent-change-chart") as SVGSVGElement | null;
const confirmDialog = byId<HTMLDialogElement>("confirm-dialog");
const confirmForm = byId<HTMLFormElement>("confirm-form");
const confirmTitle = byId<HTMLElement>("confirm-title");
const confirmMessage = byId<HTMLElement>("confirm-message");
const confirmAccept = byId<HTMLButtonElement>("confirm-accept");
// The confirm dialog is shared by delete and full-export; each flow sets the
// pending intent plus the accept label, and the close handler restores both.
let pendingDeleteInstance: string | null = null;
let pendingExportMode: "full" | "status" | null = null;
let confirmAcceptLabel = "删除";
const exportDialog = byId<HTMLDialogElement>("export-dialog");
const importResultDialog = byId<HTMLDialogElement>("import-result-dialog");
const importResultSummary = byId<HTMLElement>("import-result-summary");
const importResultList = byId<HTMLElement>("import-result-list");
const renameDialog = byId<HTMLDialogElement>("rename-dialog");
const renameForm = byId<HTMLFormElement>("rename-form");
const renameTitle = byId<HTMLElement>("rename-title");
const renameInput = byId<HTMLInputElement>("rename-input");
let pendingRenameInstance: string | null = null;
const configuredInstanceIds = new Set<string>();
let selectedInstance = "glm";
const glmSnapshots = new Map<string, GlmSnapshot>();
const onlineSnapshots = new Map<string, OnlineSnapshot>();
/** Instance ids whose latest sync failed; their rows show cached data. */
const failedSyncInstances = new Map<string, string>();
const PROVIDER_ORDER_KEY = "llm-usage:provider-order";
const INSTANCE_REMARKS_KEY = "llm-usage:instance-remarks";
const instanceRemarks = loadSavedInstanceRemarks();
const savedInstanceOrder = loadSavedInstanceOrder();
let dragSourceRow: HTMLElement | null = null;
let autoSyncTimer: number | null = null;
let isSyncing = false;
let dailyUsageRecords: DailyUsageRecord[] = [];
let selectedTrendRange: TrendRange = "7d";
let selectedTrendMetric: "tokens" | "balance" = "tokens";
let selectedRecentChangeMetric: ProviderChangeMetric = "tokens";
const APP_VERSION_FALLBACK = "0.1.5";

function renderTrendProviderOptions() {
  if (!trendProvider) return;
  const selected = trendProvider.value || "all";
  const providerIds = new Set(
    selectProviderIdsWithMetric(dailyUsageRecords, selectedTrendMetric, true),
  );
  const options = [new Option(selectedTrendMetric === "balance" ? "合计余额" : "全部提供商", "all")];
  for (const provider of providerDefinitions) {
    if (providerIds.has(provider.id)) options.push(new Option(provider.name, provider.id));
  }
  trendProvider.replaceChildren(...options);
  trendProvider.value = options.some((option) => option.value === selected) ? selected : "all";
}

function renderTrend() {
  if (!trendChart) return;
  const providerId = trendProvider?.value || "all";
  const records = providerId === "all"
    ? dailyUsageRecords
    : dailyUsageRecords.filter((record) => isProviderInstanceId(record.providerId, providerId));
  const selectedName = providerId === "all"
    ? selectedTrendMetric === "balance" ? "合计余额" : "全部提供商"
    : providerName(providerId);
  if (trendTitle) {
    trendTitle.textContent = selectedTrendMetric === "balance"
      ? selectedTrendRange === "24h" ? "今日余额变化" : "每日余额变化"
      : selectedTrendRange === "24h" ? "今日 Token 消耗" : "每日 Token 消耗";
  }
  if (selectedTrendMetric === "balance") {
    const points = selectBalanceTrend(records, selectedTrendRange, providerId);
    renderBalanceTrendChart(
      trendChart,
      trendEmpty,
      trendDescription,
      points,
      selectedName,
      selectedTrendRange === "24h",
    );
    if (!points.length && trendEmpty) {
      trendEmpty.textContent = `${selectedName}在所选范围暂无余额历史，请切换范围或同步后再查看。`;
    }
    return;
  }
  const points = selectDailyTrend(records, selectedTrendRange, providerId);
  renderDailyTrendChart(trendChart, trendEmpty, trendDescription, points, selectedName, selectedTrendRange === "24h");
  if (!points.length && trendEmpty) {
    trendEmpty.textContent = `${selectedName}在所选范围暂无 Token 历史，请切换范围或同步后再查看。`;
  }
}

const recentChangeMetricLabels: Record<ProviderChangeMetric, string> = {
  requests: "请求",
  tokens: "Token",
  balance: "余额",
  cost: "成本",
};

function renderRecentChangeProviderOptions() {
  if (!recentChangeProvider) return;
  const selected = recentChangeProvider.value;
  const options = orderedConfiguredInstances().map(
    (instanceId) => new Option(providerName(instanceId), instanceId),
  );
  recentChangeProvider.replaceChildren(...options);
  if (options.some((option) => option.value === selected)) recentChangeProvider.value = selected;
}

function recentSampleLabel(date: string, slot: number | null): string {
  const dateLabel = date.slice(5).replace("-", "/");
  return slot == null ? dateLabel : `${dateLabel} ${formatQuarterSlot(slot)}`;
}

function renderRecentChange() {
  const instanceId = recentChangeProvider?.value;
  if (!instanceId) {
    recentChangeChart?.replaceChildren();
    if (recentChangeValues) recentChangeValues.hidden = true;
    if (recentChangeEmpty) {
      recentChangeEmpty.hidden = false;
      recentChangeEmpty.textContent = "添加并同步 Provider 后，可单独查看最近变化。";
    }
    return;
  }

  const change = selectLatestProviderChange(
    dailyUsageRecords,
    instanceId,
    selectedRecentChangeMetric,
  );
  if (!change) {
    recentChangeChart?.replaceChildren();
    if (recentChangeValues) recentChangeValues.hidden = true;
    if (recentChangeEmpty) {
      recentChangeEmpty.hidden = false;
      recentChangeEmpty.textContent = `${providerName(instanceId)} 的${recentChangeMetricLabels[selectedRecentChangeMetric]}至少需要两条可比采样。`;
    }
    return;
  }

  if (recentChangeEmpty) recentChangeEmpty.hidden = true;
  if (recentChangeValues) recentChangeValues.hidden = false;
  if (recentChangeChart) {
    renderProviderChangeChart(
      recentChangeChart,
      selectProviderChangeSeries(dailyUsageRecords, instanceId, selectedRecentChangeMetric),
      selectedRecentChangeMetric,
      providerName(instanceId),
    );
  }
  if (recentChangeLabel) {
    recentChangeLabel.textContent = `${recentChangeMetricLabels[selectedRecentChangeMetric]}变化`;
  }
  if (recentChangeDelta) {
    recentChangeDelta.textContent = formatProviderChangeValue(
      selectedRecentChangeMetric,
      change.delta,
      true,
    );
    recentChangeDelta.dataset.direction = change.delta > 0
      ? "increase"
      : change.delta < 0 ? "decrease" : "steady";
  }
  if (recentChangeDirection) {
    recentChangeDirection.textContent = change.delta > 0
      ? "较上次增加"
      : change.delta < 0 ? "较上次减少" : "较上次无变化";
  }
  if (recentChangeCurrent) {
    recentChangeCurrent.textContent = formatProviderChangeValue(
      selectedRecentChangeMetric,
      change.currentValue,
      false,
    );
  }
  if (recentChangePrevious) {
    recentChangePrevious.textContent = formatProviderChangeValue(
      selectedRecentChangeMetric,
      change.previousValue,
      false,
    );
  }
  if (recentChangePeriod) {
    recentChangePeriod.textContent = `${recentSampleLabel(change.previousDate, change.previousSlot)} → ${recentSampleLabel(change.currentDate, change.currentSlot)}`;
  }
}

async function loadDailyUsage() {
  if (isTauri()) {
    try {
      dailyUsageRecords = await invoke<DailyUsageRecord[]>("load_daily_usage");
    } catch {
      dailyUsageRecords = [];
    }
  }
  renderTrendProviderOptions();
  renderTrend();
  renderTodaySpend();
  renderRecentChangeProviderOptions();
  renderRecentChange();
}

/** Writes the per-row 「今日消耗」 figures and the aggregate metric tile.
 *  Both derive from history records, so they refresh together with
 *  loadDailyUsage() — at startup and after every sync round. */
function renderTodaySpend() {
  const spendByInstance = selectTodaySpend(dailyUsageRecords);
  for (const instanceId of configuredInstanceIds) {
    const spend = spendByInstance.get(instanceId);
    for (const row of providerRows(instanceId)) {
      const element = row.querySelector<HTMLElement>(".today-spend");
      if (!element) continue;
      if (!spend) {
        element.hidden = true;
        element.removeAttribute("title");
        continue;
      }
      element.hidden = false;
      element.textContent = spend.source === "balance-diff"
        ? `今日消耗 ${formatCny(spend.spendCny)} · 估算`
        : `今日消耗 ${formatCny(spend.spendCny)}`;
      element.title = spend.source === "balance-diff"
        ? "按余额变化估算 · 充值不计入"
        : "官方成本接口";
    }
  }
  // Sum only configured instances: deleted accounts keep their history (the
  // trend charts rely on it) but must drop out of the live aggregate, matching
  // how renderTotals() drops their snapshots.
  let total = 0;
  let contributors = 0;
  for (const instanceId of configuredInstanceIds) {
    const spend = spendByInstance.get(instanceId);
    if (!spend) continue;
    total += spend.spendCny;
    contributors += 1;
  }
  const tile = byId<HTMLElement>("today-spend");
  if (tile) tile.textContent = contributors > 0 ? formatCny(total) : "—";
}

function ensureProviderRow(instanceId: string): HTMLElement | null {
  // The dashboard list and the providers page each keep their own copy of the
  // row; both are created together so renders never see a half-updated view.
  for (const container of [providerList, providerInstances]) {
    if (!container) continue;
    if (!container.querySelector(`.provider-row[data-provider="${instanceId}"]`)) {
      container.append(createProviderRow(instanceId) ?? document.createComment("unknown provider"));
    }
  }
  return providerList?.querySelector<HTMLElement>(
    `.provider-row[data-provider="${instanceId}"]`,
  ) ?? null;
}

function createProviderRow(instanceId: string): HTMLElement | null {
  const provider = providerDefinition(instanceId);
  if (!provider) return null;
  const base = provider.id;
  const isGlm = base === "glm";
  const row = document.createElement("article");
  row.className = `provider-row${isGlm ? " featured" : ""}`;
  row.dataset.provider = instanceId;
  row.hidden = true;

  const handle = document.createElement("button");
  handle.type = "button";
  handle.className = "drag-handle";
  handle.textContent = "⠿";
  handle.title = "拖拽调整顺序，或按 Alt+↑/↓ 移动";
  attachRowDragging(row, handle);

  const identity = document.createElement("div");
  identity.className = "provider-identity";
  identity.title = `${provider.name} · ${provider.subtitle}`;
  const mark = document.createElement("img");
  mark.className = "provider-mark";
  mark.src = provider.logo;
  mark.alt = "";
  const heading = document.createElement("h3");
  heading.className = "provider-name";
  identity.append(mark, heading);

  const usage = document.createElement("div");
  usage.className = "usage-cell";
  const usageLabel = document.createElement("span");
  usageLabel.textContent = isGlm ? "今日 Token" : "在线摘要";
  const usageValue = document.createElement("strong");
  usageValue.className = "usage-value";
  usageValue.textContent = "等待同步";
  const usageHint = document.createElement("small");
  usageHint.className = "usage-hint";
  usageHint.textContent = "已保存凭据";
  const todaySpend = document.createElement("small");
  todaySpend.className = "today-spend";
  todaySpend.hidden = true;
  usage.append(usageLabel, usageValue, usageHint, todaySpend);

  const quota = document.createElement("div");
  quota.className = "quota-cell";
  const quotaLabel = document.createElement("span");
  quotaLabel.className = "quota-label";
  quotaLabel.textContent = isGlm ? "窗口" : "在线口径";
  const quotaValue = document.createElement("b");
  quotaValue.className = "quota-value";
  quotaValue.textContent = "—";
  const progress = document.createElement("progress");
  progress.className = "quota-progress";
  progress.max = 100;
  progress.value = 0;
  const quotaHint = document.createElement("small");
  quotaHint.className = "quota-hint";
  quotaHint.textContent = isGlm ? "重置时间未知" : "等待在线返回";
  quota.append(quotaLabel, quotaValue, progress, quotaHint);

  const configure = document.createElement("button");
  configure.className = "row-action";
  configure.type = "button";
  configure.dataset.action = "configure";
  configure.dataset.provider = instanceId;
  configure.textContent = "修改配置";

  const remarkButton = document.createElement("button");
  remarkButton.className = "row-action";
  remarkButton.type = "button";
  remarkButton.dataset.action = "rename-provider";
  remarkButton.dataset.provider = instanceId;
  remarkButton.textContent = "备注";

  const remove = document.createElement("button");
  remove.className = "row-action danger";
  remove.type = "button";
  remove.dataset.action = "delete-provider";
  remove.dataset.provider = instanceId;
  remove.textContent = "删除";

  const actions = document.createElement("div");
  actions.className = "row-actions";
  actions.append(configure, remarkButton, remove);

  const details = document.createElement("div");
  details.className = "provider-details";
  details.hidden = true;
  row.append(handle, identity, usage, quota, actions, details);
  applyRowIdentity(row, instanceId);
  return row;
}

/** Every rendered copy of an instance row: the dashboard list and the
 *  providers page render the same structure and update in lockstep. */
function providerRows(instanceId: string): HTMLElement[] {
  return Array.from(
    document.querySelectorAll<HTMLElement>(`.provider-row[data-provider="${instanceId}"]`),
  );
}

/** Writes the provider name, instance badge, and identity aria-labels onto
 *  every rendered copy of the row. Called at creation and after renames. */
function applyRowIdentity(row: HTMLElement, instanceId: string) {
  const provider = providerDefinition(instanceId);
  if (!provider) return;
  const index = instanceIndexOf(instanceId);
  const remark = instanceRemark(instanceId);
  const displayName = instanceDisplayName(provider, index, remark);
  const heading = row.querySelector<HTMLElement>(".provider-name");
  if (heading) {
    heading.replaceChildren(provider.name);
    const badgeLabel = instanceBadgeLabel(index, remark);
    if (badgeLabel) {
      const badge = document.createElement("span");
      badge.className = "instance-badge";
      badge.textContent = badgeLabel;
      heading.append(badge);
    }
  }
  row.querySelector<HTMLElement>(".drag-handle")?.setAttribute(
    "aria-label",
    `调整 ${displayName} 顺序：按住 Alt 并使用上下箭头移动`,
  );
  row.querySelector<HTMLButtonElement>('button[data-action="rename-provider"]')?.setAttribute(
    "aria-label",
    `设置 ${displayName} 备注名称`,
  );
  row.querySelector<HTMLButtonElement>('button[data-action="delete-provider"]')?.setAttribute(
    "aria-label",
    `删除 ${displayName}`,
  );
  row.querySelector<HTMLElement>(".provider-details")?.setAttribute(
    "aria-label",
    `${displayName}完整明细`,
  );
}

function applyInstanceIdentities(instanceId: string) {
  for (const row of providerRows(instanceId)) applyRowIdentity(row, instanceId);
  renderRecentChangeProviderOptions();
  renderRecentChange();
}

/** Wires drag-to-reorder plus an Alt+arrow keyboard equivalent onto a row. */
function attachRowDragging(row: HTMLElement, handle: HTMLElement) {
  const releaseDraggable = () => {
    row.draggable = false;
  };
  handle.addEventListener("mousedown", () => {
    row.draggable = true;
  });
  // A plain click on the grip must not leave the whole row draggable.
  handle.addEventListener("mouseup", releaseDraggable);
  handle.addEventListener("pointercancel", releaseDraggable);
  row.addEventListener("dragstart", (event) => {
    if (!row.draggable) {
      event.preventDefault();
      return;
    }
    dragSourceRow = row;
    row.classList.add("dragging");
    event.dataTransfer?.setData("text/plain", row.dataset.provider ?? "");
    if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
  });
  row.addEventListener("dragend", () => {
    row.draggable = false;
    row.classList.remove("dragging");
    clearDropIndicators();
    if (dragSourceRow === row) persistInstanceOrder(row.parentElement);
    dragSourceRow = null;
  });
  row.addEventListener("dragover", (event) => {
    if (!dragSourceRow || dragSourceRow === row) return;
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
    const after = dropIsAfter(row, event);
    row.classList.toggle("drop-after", after);
    row.classList.toggle("drop-before", !after);
  });
  row.addEventListener("dragleave", () => {
    row.classList.remove("drop-before", "drop-after");
  });
  row.addEventListener("drop", (event) => {
    if (!dragSourceRow || dragSourceRow === row) return;
    event.preventDefault();
    if (dropIsAfter(row, event)) row.after(dragSourceRow);
    else row.before(dragSourceRow);
    clearDropIndicators();
    persistInstanceOrder(row.parentElement);
  });
  handle.addEventListener("keydown", (event) => {
    if (!event.altKey || (event.key !== "ArrowUp" && event.key !== "ArrowDown")) return;
    event.preventDefault();
    const up = event.key === "ArrowUp";
    const sibling = up ? row.previousElementSibling : row.nextElementSibling;
    if (!(sibling instanceof HTMLElement)) return;
    if (up) sibling.before(row);
    else sibling.after(row);
    persistInstanceOrder(row.parentElement);
    (handle as HTMLElement).focus();
  });
}

function dropIsAfter(row: HTMLElement, event: DragEvent): boolean {
  const rect = row.getBoundingClientRect();
  return event.clientY > rect.top + rect.height / 2;
}

function clearDropIndicators() {
  for (const row of document.querySelectorAll<HTMLElement>(".provider-row")) {
    row.classList.remove("drop-before", "drop-after");
  }
}

function loadSavedInstanceOrder(): string[] {
  try {
    const raw = window.localStorage.getItem(PROVIDER_ORDER_KEY);
    const parsed = raw ? JSON.parse(raw) : [];
    return Array.isArray(parsed)
      ? parsed.filter((id): id is string => typeof id === "string" && id.length <= 64)
      : [];
  } catch {
    return [];
  }
}

/** Persists the drag order from the container the reorder happened in, then
 *  re-sorts the other container's copy so both stay in lockstep. */
function persistInstanceOrder(source?: ParentNode | null) {
  const order = rowInstanceIds(source ?? providerList ?? document);
  savedInstanceOrder.splice(0, savedInstanceOrder.length, ...order);
  try {
    window.localStorage.setItem(PROVIDER_ORDER_KEY, JSON.stringify(order));
  } catch {
    // Ordering is a convenience; ignore storage failures.
  }
  sortProviderRows();
}

function rowInstanceIds(container: ParentNode = document): string[] {
  return Array.from(
    container.querySelectorAll<HTMLElement>(".provider-row"),
    (row) => row.dataset.provider ?? "",
  ).filter(Boolean);
}

function loadSavedInstanceRemarks(): Map<string, string> {
  const remarks = new Map<string, string>();
  try {
    const raw = window.localStorage.getItem(INSTANCE_REMARKS_KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : {};
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return remarks;
    for (const [instanceId, remark] of Object.entries(parsed as Record<string, unknown>)) {
      if (typeof remark !== "string" || instanceId.length > 64) continue;
      const sanitized = sanitizeInstanceRemark(remark);
      if (sanitized) remarks.set(instanceId, sanitized);
    }
  } catch {
    // Remarks are a convenience; ignore storage failures.
  }
  return remarks;
}

function persistInstanceRemarks() {
  try {
    window.localStorage.setItem(
      INSTANCE_REMARKS_KEY,
      JSON.stringify(Object.fromEntries(instanceRemarks)),
    );
  } catch {
    // Remarks are a convenience; ignore storage failures.
  }
}

function instanceRemark(instanceId: string): string {
  return instanceRemarks.get(instanceId) ?? "";
}

/** Sort key honoring the saved drag order, then catalog order as fallback. */
function instanceOrderKey(instanceId: string): [number, number, number] {
  const saved = savedInstanceOrder.indexOf(instanceId);
  const catalog = providerDefinitions.findIndex(
    (provider) => provider.id === baseProviderId(instanceId),
  );
  return saved === -1
    ? [1, catalog, instanceIndexOf(instanceId)]
    : [0, saved, instanceIndexOf(instanceId)];
}

function sortProviderRows() {
  for (const container of [providerList, providerInstances]) {
    if (!container) continue;
    const rows = Array.from(container.querySelectorAll<HTMLElement>(".provider-row"));
    rows.sort((left, right) => {
      const leftKey = instanceOrderKey(left.dataset.provider ?? "");
      const rightKey = instanceOrderKey(right.dataset.provider ?? "");
      return (
        leftKey[0] - rightKey[0] || leftKey[1] - rightKey[1] || leftKey[2] - rightKey[2]
      );
    });
    for (const row of rows) container.append(row);
  }
}

function setInstanceConfigured(instanceId: string) {
  configuredInstanceIds.add(instanceId);
  renderProviderVisibility();
}

function configuredBaseIds(): Set<string> {
  return new Set(Array.from(configuredInstanceIds, baseProviderId));
}

function renderProviderVisibility() {
  for (const instanceId of configuredInstanceIds) ensureProviderRow(instanceId);
  sortProviderRows();
  for (const row of document.querySelectorAll<HTMLElement>(".provider-row")) {
    row.hidden = !configuredInstanceIds.has(row.dataset.provider ?? "");
  }
  if (providerEmpty) providerEmpty.hidden = configuredInstanceIds.size > 0;
  if (providerInstancesEmpty) providerInstancesEmpty.hidden = configuredInstanceIds.size > 0;
  setText("providers-configured-count", String(configuredInstanceIds.size));
  renderProviderCatalog();
  renderTrendProviderOptions();
  renderRecentChangeProviderOptions();
  renderRecentChange();
  renderTotals();
}

/** Catalog entries shared by the dialog and the providers page. */
function buildCatalogButtons(): HTMLButtonElement[] {
  return providerDefinitions.map((provider) => {
    const configured = hasConfiguredInstance(provider.id, configuredInstanceIds);
    const button = document.createElement("button");
    button.type = "button";
    button.className = "catalog-item";
    button.dataset.action = "configure";
    button.dataset.provider = provider.id;
    button.dataset.mode = "add";
    const mark = document.createElement("img");
    mark.className = "catalog-mark";
    mark.src = provider.logo;
    mark.alt = "";
    const name = document.createElement("strong");
    name.textContent = provider.name;
    const subtitle = document.createElement("span");
    subtitle.textContent = configured ? `${provider.subtitle} · 已配置，可添加实例` : provider.subtitle;
    const copy = document.createElement("span");
    copy.className = "catalog-copy";
    copy.append(name, subtitle);
    button.append(mark, copy);
    return button;
  });
}

function renderProviderCatalog() {
  const buttons = buildCatalogButtons();
  providerCatalog?.replaceChildren(...buttons);
  pageProviderCatalog?.replaceChildren(...buildCatalogButtons());
}

function setStatus(message: string, state: "ready" | "syncing" | "error" = "ready") {
  syncStatus?.classList.toggle("syncing", state === "syncing");
  syncStatus?.classList.toggle("error", state === "error");
  if (!syncStatus) return;
  // The indicator ships with an empty text node after its dot; keep writes on
  // that node so the dot element itself is never turned into a text container.
  if (syncStatus.lastChild?.nodeType === Node.TEXT_NODE) syncStatus.lastChild.textContent = ` ${message}`;
  else syncStatus.append(` ${message}`);
  // The bar truncates long errors; the full text stays available on hover.
  if (message.trim()) syncStatus.title = message.trim();
  else syncStatus.removeAttribute("title");
}

type ViewId = "dashboard" | "providers" | "about";

function routeFromHash(hash: string): { view: ViewId } {
  if (hash === "about") return { view: "about" };
  if (hash === "providers") return { view: "providers" };
  return { view: "dashboard" };
}

function applyRoute() {
  const hash = window.location.hash.slice(1);
  const { view } = routeFromHash(hash);
  byId("dashboard")?.toggleAttribute("hidden", view !== "dashboard");
  byId("providers")?.toggleAttribute("hidden", view !== "providers");
  byId("about")?.toggleAttribute("hidden", view !== "about");
  updateNavActive(hash || "dashboard");
  const root = byId(view);
  root?.scrollTo({ top: 0 });
}

function updateNavActive(hash: string) {
  document.querySelectorAll<HTMLElement>(".rail .nav-item[href]").forEach((item) => {
    const href = item.getAttribute("href")?.slice(1) ?? "dashboard";
    const isActive = href === hash;
    item.classList.toggle("active", isActive);
    if (isActive) item.setAttribute("aria-current", "page");
    else item.removeAttribute("aria-current");
  });
}

function setText(id: string, text: string) {
  const el = byId<HTMLElement>(id);
  if (el) el.textContent = text;
}

async function populateAboutMetadata() {
  const version = isTauri() ? await getVersion().catch(() => APP_VERSION_FALLBACK) : APP_VERSION_FALLBACK;
  setText("app-version", version);
  setText("app-version-detail", version);
  setText("window-version", `v${version}`);
  setText("about-provider-count", String(providerDefinitions.length));
}

function renderGlm(instanceId: string, snapshot: GlmSnapshot) {
  setInstanceConfigured(instanceId);
  glmSnapshots.set(instanceId, snapshot);
  for (const row of providerRows(instanceId)) {
    const value = row.querySelector<HTMLElement>(".usage-value");
    if (value) value.textContent = formatInteger(snapshot.totalTokens);
    const hint = row.querySelector<HTMLElement>(".usage-hint");
    if (hint) hint.textContent = `${formatInteger(snapshot.requests)} 次调用 · ${snapshot.planLevel}`;
    const percent = row.querySelector<HTMLElement>(".quota-value");
    if (percent) percent.textContent = `${snapshot.usedPercent.toFixed(1)}%`;
    const progress = row.querySelector<HTMLProgressElement>(".quota-progress");
    if (progress) progress.value = snapshot.usedPercent;
  }
  renderProviderDetails(instanceId, snapshot.detailSections);
  updateCooldown();
  renderTotals();
}

function renderOnline(snapshot: OnlineSnapshot) {
  setInstanceConfigured(snapshot.providerId);
  onlineSnapshots.set(snapshot.providerId, snapshot);
  for (const row of providerRows(snapshot.providerId)) {
    const primary = row.querySelector<HTMLElement>(".usage-value");
    const secondary = row.querySelector<HTMLElement>(".usage-hint");
    const quotaLabel = row.querySelector<HTMLElement>(".quota-label");
    const quotaValue = row.querySelector<HTMLElement>(".quota-value");
    const quotaHint = row.querySelector<HTMLElement>(".quota-hint");
    const progress = row.querySelector<HTMLProgressElement>(".quota-progress");
    if (primary) primary.textContent = snapshot.primaryValue;
    if (secondary) secondary.textContent = snapshot.secondaryValue;
    if (quotaLabel) quotaLabel.textContent = snapshot.primaryLabel;
    if (quotaValue) {
      quotaValue.textContent = snapshot.quotaUsedPercent == null
        ? "在线余额"
        : `${snapshot.quotaUsedPercent.toFixed(1)}%`;
    }
    if (quotaHint) {
      quotaHint.textContent = snapshot.cooldownEndsAtMs
        ? formatCooldown(snapshot.cooldownEndsAtMs)
        : sourceLabel(snapshot);
    }
    if (progress) {
      progress.hidden = snapshot.quotaUsedPercent == null;
      if (snapshot.quotaUsedPercent != null) progress.value = snapshot.quotaUsedPercent;
    }
  }
  renderProviderDetails(snapshot.providerId, snapshot.detailSections);
  renderTotals();
}

function sourceLabel(snapshot: OnlineSnapshot) {
  if (snapshot.experimental) return "实验接口 · 可能随平台变化";
  if (snapshot.source === "official_balance") return "官方余额接口";
  if (snapshot.source === "official_organization_usage") return "OpenAI 官方组织用量";
  if (snapshot.source === "official_claude_code_analytics") return "Claude Code 官方日汇总";
  if (snapshot.source === "official_messages_usage") return "Anthropic 官方用量报告";
  if (snapshot.source === "official_prepaid_balance") return "xAI Management 预付余额";
  if (snapshot.source === "official_cloud_monitoring") return "Google Cloud Monitoring";
  if (snapshot.source === "official_prometheus_monitoring") return "百炼 Prometheus 监控";
  return "在线接口";
}

function renderTotals() {
  const totalRequests = byId<HTMLElement>("total-requests");
  const totalTokens = byId<HTMLElement>("total-tokens");
  const totalCost = byId<HTMLElement>("total-cost");
  const coverage = byId<HTMLElement>("coverage");
  const snapshots = Array.from(onlineSnapshots.values());
  const balance = snapshots
    .reduce((sum, snapshot) => sum + (snapshot.balanceCny ?? 0), 0);
  const totals = summarizeProviders([
    ...Array.from(glmSnapshots.values(), (snapshot) => ({
      requests: snapshot.requests,
      totalTokens: snapshot.totalTokens,
      estimatedCostCny: null,
    })),
    ...snapshots.map((snapshot) => ({
      requests: snapshot.requests ?? null,
      totalTokens: snapshot.totalTokens ?? null,
      estimatedCostCny: snapshot.estimatedCostCny ?? null,
    })),
  ]);
  const estimatedCost = totals.estimatedCostCny ?? 0;
  if (totalRequests) totalRequests.textContent = totals.requests == null ? "—" : formatInteger(totals.requests);
  if (totalTokens) totalTokens.textContent = totals.totalTokens == null ? "—" : formatInteger(totals.totalTokens);
  if (totalCost) {
    totalCost.textContent = estimatedCost > 0
      ? `¥${estimatedCost.toFixed(2)}`
      : balance > 0 ? `¥${balance.toFixed(2)}` : "—";
  }
  if (coverage) coverage.textContent = `${configuredBaseIds().size} / ${providerDefinitions.length}`;
}

function updateCooldown() {
  for (const [instanceId, snapshot] of glmSnapshots) {
    // Failed instances keep their sync-error hint; a stale cooldown countdown
    // computed from cached data would read as live again.
    if (failedSyncInstances.has(instanceId)) continue;
    for (const row of providerRows(instanceId)) {
      const hint = row.querySelector<HTMLElement>(".quota-hint");
      if (hint) hint.textContent = formatCooldown(snapshot.cooldownEndsAtMs);
    }
  }
}

/**
 * Marks a provider's rows as failed so the last cached numbers never
 * masquerade as live data; the full error stays on the row via tooltip.
 */
function markSyncFailed(instanceId: string, message: string) {
  failedSyncInstances.set(instanceId, message);
  for (const row of providerRows(instanceId)) {
    row.classList.add("sync-failed");
    const hint = row.querySelector<HTMLElement>(".quota-hint");
    if (hint) {
      hint.textContent = "同步失败 · 显示缓存数据";
      hint.title = message;
    }
  }
}

function clearSyncFailed(instanceId: string) {
  if (!failedSyncInstances.delete(instanceId)) return;
  for (const row of providerRows(instanceId)) {
    row.classList.remove("sync-failed");
    row.querySelector<HTMLElement>(".quota-hint")?.removeAttribute("title");
  }
}

/** Syncs one GLM instance; resolves false when the attempt failed. */
async function syncGlm(instanceId: string): Promise<boolean> {
  if (!isTauri()) {
    setStatus("浏览器预览模式");
    return false;
  }
  setStatus(`正在连接 ${providerName(instanceId)}`, "syncing");
  try {
    renderGlm(instanceId, await invoke<GlmSnapshot>("sync_glm", {
      providerId: instanceId,
      localDate: localDateKey(),
      slot: localQuarterSlot(),
      ...localDayRange(),
    }));
    clearSyncFailed(instanceId);
    return true;
  } catch (reason) {
    const error = reason as CommandError;
    const message = error.message ?? "同步失败，请稍后重试";
    setStatus(message, "error");
    markSyncFailed(instanceId, message);
    return false;
  }
}

/** Syncs one online provider instance; resolves false when the attempt failed. */
async function syncOnline(instanceId: string): Promise<boolean> {
  if (!isTauri()) return false;
  try {
    renderOnline(await invoke<OnlineSnapshot>("sync_online_provider", {
      providerId: instanceId,
      localDate: localDateKey(),
      slot: localQuarterSlot(),
      ...localDayRangeMs(),
    }));
    clearSyncFailed(instanceId);
    return true;
  } catch (reason) {
    const message = instanceError(instanceId, "同步失败", reason);
    setStatus(message, "error");
    markSyncFailed(instanceId, message);
    return false;
  }
}

function orderedConfiguredInstances(): string[] {
  // Sync in the visual (drag-ordered) sequence when rows exist. Read the
  // dashboard list only — the providers page renders a second copy of each row.
  const visualOrder = rowInstanceIds(providerList ?? document).filter((instanceId) =>
    configuredInstanceIds.has(instanceId),
  );
  if (visualOrder.length === configuredInstanceIds.size) return visualOrder;
  return Array.from(configuredInstanceIds).sort((left, right) => {
    const leftKey = instanceOrderKey(left);
    const rightKey = instanceOrderKey(right);
    return (
      leftKey[0] - rightKey[0] || leftKey[1] - rightKey[1] || leftKey[2] - rightKey[2]
    );
  });
}

async function syncAll() {
  if (isSyncing) return;
  if (!configuredInstanceIds.size) {
    setStatus("请先添加供应商");
    return;
  }
  isSyncing = true;
  try {
    const syncTasks = orderedConfiguredInstances().map((instanceId) =>
      baseProviderId(instanceId) === "glm" ? syncGlm(instanceId) : syncOnline(instanceId),
    );
    // Success stays silent: the status bar only narrates syncing, errors, and
    // guidance, so a fully successful round clears any leftover text.
    const results = await Promise.all(syncTasks);
    if (results.every((succeeded) => succeeded)) setStatus("");
    renderTotals();
    await loadDailyUsage();
  } finally {
    isSyncing = false;
  }
}

function providerName(instanceId: string) {
  const provider = providerDefinition(instanceId);
  if (!provider) return "供应商";
  return instanceDisplayName(provider, instanceIndexOf(instanceId), instanceRemark(instanceId));
}

/** Prefixes a sync/delete error with the instance name unless it already has it. */
function instanceError(instanceId: string, fallback: string, reason: unknown): string {
  const message = (reason as CommandError)?.message ?? fallback;
  const name = providerName(instanceId);
  return message.startsWith(name) ? message : `${name}：${message}`;
}

/** Styles the shared confirm dialog; callers own their pending-intent vars. */
function prepareConfirmDialog(title: string, message: string, acceptLabel: string) {
  confirmAcceptLabel = acceptLabel;
  if (confirmTitle) confirmTitle.textContent = title;
  if (confirmMessage) confirmMessage.textContent = message;
  if (confirmAccept) confirmAccept.textContent = acceptLabel;
}

function deleteProviderInstance(instanceId: string) {
  if (!providerDefinition(instanceId) || !isTauri()) return;
  pendingDeleteInstance = instanceId;
  prepareConfirmDialog(
    "确认删除",
    `删除「${providerName(instanceId)}」会清除本机保存的 API Key 与缓存摘要，但不影响已保存的历史趋势。确定继续吗？`,
    "删除",
  );
  confirmDialog?.showModal();
}

/** Picks a save path and writes the transfer file via the Rust command. */
async function runExport(mode: "full" | "status") {
  setStatus("正在导出…", "syncing");
  const path = await saveFileDialog({
    title: mode === "full" ? "导出完整备份" : "导出状态报告",
    defaultPath: exportDefaultFileName(mode, localDateKey()),
    filters: [{ name: "JSON", extensions: ["json"] }],
  });
  if (!path) {
    // Cancelling the native dialog is not an error; clear the busy status.
    setStatus("");
    return;
  }
  try {
    const summary = await invoke<{ instanceCount: number }>("export_provider_backup", {
      path,
      mode,
      remarks: buildExportRemarks(instanceRemarks, configuredInstanceIds),
    });
    setStatus(
      `已导出 ${summary.instanceCount} 个实例${mode === "full" ? "（含明文密钥，请妥善保管）" : ""}`,
    );
  } catch (reason) {
    setStatus((reason as CommandError)?.message ?? "导出失败，请稍后重试", "error");
  }
}

/**
 * Imports a transfer file. The backend saves credentials without any network
 * traffic; remarks follow their assigned ids and syncing stays a separate,
 * user-triggered step.
 */
async function runImport() {
  if (!isTauri()) {
    setStatus("导入/导出仅桌面可用", "error");
    return;
  }
  const path = await openFileDialog({
    title: "导入供应商配置",
    multiple: false,
    directory: false,
    filters: [{ name: "JSON", extensions: ["json"] }],
  });
  if (!path) return;
  try {
    const results = await invoke<ImportEntryResult[]>("import_provider_backup", { path });
    for (const result of results) {
      if (result.outcome !== "saved" || !result.assignedInstanceId || !result.remark) continue;
      // Assigned ids are always freshly created, so a file remark can never
      // overwrite an existing local one.
      instanceRemarks.set(result.assignedInstanceId, sanitizeInstanceRemark(result.remark));
    }
    persistInstanceRemarks();
    await loadProviderInstances();
    if (importResultSummary) importResultSummary.textContent = importSummaryText(results);
    if (importResultList) {
      importResultList.replaceChildren(
        ...importResultLines(results).map((line) => {
          const item = document.createElement("li");
          item.textContent = line;
          return item;
        }),
      );
    }
    importResultDialog?.showModal();
    setStatus(importSummaryText(results));
  } catch (reason) {
    setStatus((reason as CommandError)?.message ?? "导入失败，请稍后重试", "error");
  }
}

async function loadProviderInstances() {
  if (!isTauri()) {
    renderProviderVisibility();
    return;
  }
  try {
    const instances = await invoke<string[]>("list_provider_instances");
    for (const instanceId of instances) {
      if (providerDefinition(instanceId)) configuredInstanceIds.add(instanceId);
    }
  } catch {
    // Leave the dashboard empty when credentials cannot be listed.
  }
  renderProviderVisibility();
}

async function loadCache() {
  if (!isTauri()) return;
  try {
    const cached = await invoke<CachedSnapshot[]>("load_cached_snapshots");
    for (const entry of cached) {
      if (!configuredInstanceIds.has(entry.providerId)) continue;
      if (entry.kind === "glm") renderGlm(entry.providerId, entry.snapshot as GlmSnapshot);
      if (entry.kind === "online") renderOnline(entry.snapshot as OnlineSnapshot);
    }
  } catch {
    setStatus("本地缓存不可用", "error");
  }
}

function applyAutoSync(seconds: number) {
  if (autoSyncTimer !== null) {
    window.clearInterval(autoSyncTimer);
    autoSyncTimer = null;
  }
  if (seconds <= 0) {
    setStatus("自动拉取已关闭");
    return;
  }
  autoSyncTimer = window.setInterval(() => void syncAll(), seconds * 1000);
  setStatus(`自动拉取：${seconds < 60 ? `${seconds} 秒` : `${Math.round(seconds / 60)} 分钟`}`);
}

function autoSyncLabel(seconds: number): string {
  if (seconds <= 0) return "关闭";
  return seconds < 60 ? `${seconds} 秒` : `${Math.round(seconds / 60)} 分钟`;
}

/** Reflects the chosen interval on the about-page segmented control. */
function updateAutoSyncOptions(seconds: number) {
  for (const item of autoSyncOptions?.querySelectorAll<HTMLButtonElement>("button[data-seconds]") ?? []) {
    item.setAttribute("aria-pressed", String(Number(item.dataset.seconds) === seconds));
  }
}

refreshButton?.addEventListener("click", async () => {
  refreshButton.disabled = true;
  refreshButton.classList.add("working");
  await syncAll();
  refreshButton.disabled = false;
  refreshButton.classList.remove("working");
});

themeButton?.addEventListener("click", () => {
  const light = document.documentElement.toggleAttribute("data-light");
  themeButton.setAttribute("aria-label", light ? "切换深色主题" : "切换浅色主题");
});

/** Reflects the boot-start registration on the about-page switch. */
function updateAutostartToggle(enabled: boolean) {
  autostartToggle?.setAttribute("aria-checked", String(enabled));
  autostartToggle?.closest(".setting-row")?.classList.toggle("enabled", enabled);
  if (autostartToggle) autostartToggle.title = `开机自启动：${enabled ? "开" : "关"}`;
}

/** Reads the OS registration once at startup so the toggle shows real state. */
async function initAutostartToggle() {
  if (!autostartToggle) return;
  if (!isTauri()) {
    autostartToggle.disabled = true;
    autostartToggle.title = "开机自启动仅桌面可用";
    return;
  }
  try {
    updateAutostartToggle(await isAutostartEnabled());
  } catch {
    // Registry/keychain probe failed: leave it off rather than guess.
    updateAutostartToggle(false);
  }
}

autostartToggle?.addEventListener("click", async () => {
  if (!isTauri() || autostartToggle.disabled) return;
  autostartToggle.disabled = true;
  try {
    const target = !(await isAutostartEnabled());
    if (target) await enableAutostart();
    else await disableAutostart();
    updateAutostartToggle(target);
    setStatus(target ? "开机自启动已开启" : "开机自启动已关闭");
  } catch (reason) {
    const error = reason as CommandError;
    setStatus(error.message ?? "开机自启动设置失败", "error");
  } finally {
    autostartToggle.disabled = false;
  }
});

document.addEventListener("click", (event) => {
  const target = event.target as Element | null;
  const button = target?.closest<HTMLButtonElement>("button[data-action]");
  if (!button) return;
  if (button.dataset.action === "close-provider-dialog") {
    dialog?.close();
    return;
  }
  if (button.dataset.action === "close-catalog-dialog") {
    catalogDialog?.close();
    return;
  }
  if (button.dataset.action === "open-catalog") {
    renderProviderCatalog();
    catalogDialog?.showModal();
    return;
  }
  if (button.dataset.action === "configure") {
    const target = button.dataset.provider ?? "glm";
    const instanceId = button.dataset.mode === "add" || !configuredInstanceIds.has(target)
      ? nextInstanceId(baseProviderId(target), configuredInstanceIds)
      : target;
    openProviderDialog(instanceId);
    return;
  }
  if (button.dataset.action === "delete-provider") {
    deleteProviderInstance(button.dataset.provider ?? "");
    return;
  }
  if (button.dataset.action === "close-confirm-dialog") {
    confirmDialog?.close();
  }
  if (button.dataset.action === "set-auto-sync") {
    const seconds = Number(button.dataset.seconds ?? "0");
    window.localStorage.setItem("llm-usage:auto-sync-seconds", String(seconds));
    applyAutoSync(seconds);
    updateAutoSyncOptions(seconds);
    return;
  }
  if (button.dataset.action === "export-providers") {
    if (!isTauri()) {
      setStatus("导入/导出仅桌面可用", "error");
      return;
    }
    exportDialog?.showModal();
    return;
  }
  if (button.dataset.action === "close-export-dialog") {
    exportDialog?.close();
    return;
  }
  if (button.dataset.action === "choose-export-full") {
    exportDialog?.close();
    pendingExportMode = "full";
    prepareConfirmDialog(
      "导出完整备份",
      "完整备份将以明文包含所有已配置实例的 API Key。任何拿到该文件的人都能使用你的密钥。确定继续导出吗？",
      "继续导出",
    );
    confirmDialog?.showModal();
    return;
  }
  if (button.dataset.action === "choose-export-status") {
    exportDialog?.close();
    void runExport("status");
    return;
  }
  if (button.dataset.action === "import-providers") {
    void runImport();
    return;
  }
  if (button.dataset.action === "close-import-result") {
    importResultDialog?.close();
    return;
  }
  if (button.dataset.action === "rename-provider") {
    openRenameDialog(button.dataset.provider ?? "");
    return;
  }
  if (button.dataset.action === "close-rename-dialog") {
    renameDialog?.close();
  }
});

function openProviderDialog(instanceId: string) {
  const provider = providerDefinition(instanceId);
  if (!provider || !credentialFields) return;
  selectedInstance = instanceId;
  catalogDialog?.close();
  if (dialogTitle) {
    dialogTitle.textContent = `配置 ${instanceDisplayName(provider, instanceIndexOf(instanceId), instanceRemark(instanceId))}`;
  }
  if (dialogCopy) dialogCopy.textContent = provider.credentialHint;
  credentialFields.replaceChildren(...provider.fields.map((field) => {
    const wrapper = document.createElement("div");
    wrapper.className = "credential-field";
    const label = document.createElement("label");
    label.htmlFor = `credential-${field.id}`;
    label.textContent = field.label;
    const input = document.createElement("input");
    input.id = `credential-${field.id}`;
    input.name = field.id;
    input.type = field.type;
    input.placeholder = field.placeholder;
    input.setAttribute("autocomplete", field.autocomplete ?? "off");
    input.spellcheck = false;
    input.required = true;
    input.addEventListener("input", () => input.setCustomValidity(""));
    wrapper.append(label, input);
    if (field.type === "password") {
      input.classList.add("has-reveal");
      wrapper.append(createRevealToggle(input, field.label));
    }
    return wrapper;
  }));
  dialog?.showModal();
  credentialFields.querySelector<HTMLInputElement>("input")?.focus();
  void prefillCredentialFields(instanceId);
}

/** Eye toggle that reveals a (possibly stored) secret field in the dialog. */
function createRevealToggle(input: HTMLInputElement, label: string): HTMLButtonElement {
  const toggle = document.createElement("button");
  toggle.type = "button";
  toggle.className = "credential-toggle";
  toggle.textContent = "显示";
  toggle.setAttribute("aria-label", `显示 ${label}`);
  toggle.addEventListener("click", () => {
    const reveal = input.type === "password";
    input.type = reveal ? "text" : "password";
    toggle.textContent = reveal ? "隐藏" : "显示";
    toggle.setAttribute("aria-label", `${reveal ? "隐藏" : "显示"} ${label}`);
  });
  return toggle;
}

/** Refills the dialog with the stored credential so it can be viewed or tweaked. */
async function prefillCredentialFields(instanceId: string) {
  if (!isTauri() || !credentialFields || !configuredInstanceIds.has(instanceId)) return;
  try {
    const credential = await invoke<string>("load_provider_credential", {
      providerId: instanceId,
    });
    const values = deserializeProviderCredential(instanceId, credential);
    for (const input of credentialFields.querySelectorAll<HTMLInputElement>("input")) {
      const value = values[input.name];
      if (typeof value === "string") input.value = value;
    }
  } catch {
    // Not configured or vault unavailable: keep the fields blank.
  }
}

function openRenameDialog(instanceId: string) {
  if (!providerDefinition(instanceId) || !renameDialog) return;
  pendingRenameInstance = instanceId;
  if (renameTitle) renameTitle.textContent = `备注 · ${providerName(instanceId)}`;
  if (renameInput) {
    renameInput.value = instanceRemark(instanceId);
    renameInput.maxLength = INSTANCE_REMARK_MAX_LENGTH;
  }
  renameDialog.showModal();
  renameInput?.focus();
  renameInput?.select();
}

providerForm?.addEventListener("submit", async (event) => {
  event.preventDefault();
  if (!credentialFields || !saveButton) return;
  if (!isTauri()) {
    setStatus("请在桌面应用中保存密钥", "error");
    dialog?.close();
    return;
  }
  saveButton.disabled = true;
  saveButton.textContent = "验证中…";
  try {
    const values = Object.fromEntries(
      Array.from(credentialFields.querySelectorAll<HTMLInputElement>("input")).map((input) => [input.name, input.value]),
    );
    const credential = serializeProviderCredential(selectedInstance, values);
    if (baseProviderId(selectedInstance) === "glm") {
      const snapshot = await invoke<GlmSnapshot>("configure_glm", {
        providerId: selectedInstance,
        apiKey: credential,
        localDate: localDateKey(),
        slot: localQuarterSlot(),
        ...localDayRange(),
      });
      renderGlm(selectedInstance, snapshot);
    } else {
      const snapshot = await invoke<OnlineSnapshot>("configure_online_provider", {
        providerId: selectedInstance,
        apiKey: credential,
        localDate: localDateKey(),
        slot: localQuarterSlot(),
        ...localDayRangeMs(),
      });
      renderOnline(snapshot);
    }
    await loadDailyUsage();
    dialog?.close();
  } catch (reason) {
    const error = reason as CommandError;
    const firstInput = credentialFields.querySelector<HTMLInputElement>("input");
    firstInput?.setCustomValidity(error.message ?? (reason instanceof Error ? reason.message : "凭据验证失败"));
    firstInput?.reportValidity();
  } finally {
    saveButton.disabled = false;
    saveButton.textContent = "保存并同步";
  }
});

dialog?.addEventListener("close", () => credentialFields?.replaceChildren());

confirmForm?.addEventListener("submit", async (event) => {
  event.preventDefault();
  const exportMode = pendingExportMode;
  if (exportMode) {
    if (confirmAccept) {
      confirmAccept.disabled = true;
      confirmAccept.textContent = "导出中…";
    }
    try {
      await runExport(exportMode);
    } finally {
      pendingExportMode = null;
      confirmDialog?.close();
      if (confirmAccept) {
        confirmAccept.disabled = false;
        confirmAccept.textContent = confirmAcceptLabel;
      }
    }
    return;
  }
  const instanceId = pendingDeleteInstance;
  if (!instanceId || !confirmAccept) {
    confirmDialog?.close();
    return;
  }
  confirmAccept.disabled = true;
  confirmAccept.textContent = "删除中…";
  try {
    await invoke("delete_provider", { providerId: instanceId });
    configuredInstanceIds.delete(instanceId);
    onlineSnapshots.delete(instanceId);
    glmSnapshots.delete(instanceId);
    // Freed instance ids are reused by nextInstanceId, so a stale remark must
    // not survive into a future instance of the same slot.
    instanceRemarks.delete(instanceId);
    persistInstanceRemarks();
    for (const row of providerRows(instanceId)) row.remove();
    renderProviderVisibility();
    renderTotals();
    await loadDailyUsage();
  } catch (reason) {
    setStatus(instanceError(instanceId, "删除失败", reason), "error");
  } finally {
    pendingDeleteInstance = null;
    confirmDialog?.close();
    confirmAccept.disabled = false;
    confirmAccept.textContent = confirmAcceptLabel;
  }
});

confirmDialog?.addEventListener("close", () => {
  pendingDeleteInstance = null;
  pendingExportMode = null;
  if (confirmAccept) {
    confirmAccept.disabled = false;
    confirmAccept.textContent = confirmAcceptLabel;
  }
});

renameForm?.addEventListener("submit", (event) => {
  event.preventDefault();
  const instanceId = pendingRenameInstance;
  if (!instanceId) {
    renameDialog?.close();
    return;
  }
  const remark = sanitizeInstanceRemark(renameInput?.value ?? "");
  if (remark) instanceRemarks.set(instanceId, remark);
  else instanceRemarks.delete(instanceId);
  persistInstanceRemarks();
  applyInstanceIdentities(instanceId);
  renameDialog?.close();
});

renameDialog?.addEventListener("close", () => {
  pendingRenameInstance = null;
});
trendProvider?.addEventListener("change", renderTrend);
trendMetric?.addEventListener("click", (event) => {
  const button = (event.target as Element).closest<HTMLButtonElement>("button[data-metric]");
  if (!button) return;
  selectedTrendMetric = button.dataset.metric as typeof selectedTrendMetric;
  for (const item of trendMetric.querySelectorAll<HTMLButtonElement>("button[data-metric]")) {
    item.setAttribute("aria-pressed", String(item === button));
  }
  renderTrendProviderOptions();
  renderTrend();
});
trendRange?.addEventListener("click", (event) => {
  const button = (event.target as Element).closest<HTMLButtonElement>("button[data-range]");
  if (!button) return;
  selectedTrendRange = button.dataset.range as TrendRange;
  for (const item of trendRange.querySelectorAll<HTMLButtonElement>("button[data-range]")) {
    item.setAttribute("aria-pressed", String(item === button));
  }
  renderTrend();
});
recentChangeProvider?.addEventListener("change", renderRecentChange);
recentChangeMetric?.addEventListener("click", (event) => {
  const button = (event.target as Element).closest<HTMLButtonElement>("button[data-change-metric]");
  if (!button) return;
  selectedRecentChangeMetric = button.dataset.changeMetric as ProviderChangeMetric;
  for (const item of recentChangeMetric.querySelectorAll<HTMLButtonElement>("button[data-change-metric]")) {
    item.setAttribute("aria-pressed", String(item === button));
  }
  renderRecentChange();
});
window.setInterval(updateCooldown, 30_000);
void initializeWindowControls();
void initAutostartToggle();
if (isTauri()) void listen("tray-sync", () => void syncAll());
window.addEventListener("hashchange", applyRoute);
applyRoute();
void populateAboutMetadata();
void (async () => {
  const savedAutoSync = Number(window.localStorage.getItem("llm-usage:auto-sync-seconds") ?? "0");
  const autoSyncSeconds = Number.isFinite(savedAutoSync) ? savedAutoSync : 0;
  updateAutoSyncOptions(autoSyncSeconds);
  applyAutoSync(autoSyncSeconds);
  await loadProviderInstances();
  await loadCache();
  await loadDailyUsage();
  await syncAll();
})();
