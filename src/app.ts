import { invoke, isTauri } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { listen } from "@tauri-apps/api/event";
import { renderProviderDetails } from "./details";
import {
  baseProviderId,
  formatCooldown,
  formatInteger,
  instanceIndexOf,
  isProviderInstanceId,
  localDateKey,
  localDayRange,
  localDayRangeMs,
  localQuarterSlot,
  selectDailyTrend,
  summarizeProviders,
  type DailyUsageRecord,
  type OnlineDetailSection,
  type TrendRange,
} from "./domain";
import {
  hasConfiguredInstance,
  nextInstanceId,
  providerDefinition,
  providerDefinitions,
  serializeProviderCredential,
  type ProviderDefinition,
} from "./providers";
import { initializeWindowControls } from "./window-controls";
import { renderDailyTrendChart } from "./trend-chart";
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
const autoSyncInterval = byId<HTMLSelectElement>("auto-sync-interval");
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
const trendProvider = byId<HTMLSelectElement>("trend-provider");
const trendRange = byId<HTMLElement>("trend-range");
const trendChart = document.getElementById("trend-chart") as SVGSVGElement | null;
const trendEmpty = byId<HTMLElement>("trend-empty");
const trendDescription = byId<HTMLElement>("trend-description");
const confirmDialog = byId<HTMLDialogElement>("confirm-dialog");
const confirmForm = byId<HTMLFormElement>("confirm-form");
const confirmMessage = byId<HTMLElement>("confirm-message");
const confirmAccept = byId<HTMLButtonElement>("confirm-accept");
let pendingDeleteInstance: string | null = null;
const configuredInstanceIds = new Set<string>();
let selectedInstance = "glm";
const glmSnapshots = new Map<string, GlmSnapshot>();
const onlineSnapshots = new Map<string, OnlineSnapshot>();
const PROVIDER_ORDER_KEY = "llm-usage:provider-order";
const savedInstanceOrder = loadSavedInstanceOrder();
let dragSourceRow: HTMLElement | null = null;
let autoSyncTimer: number | null = null;
let isSyncing = false;
let dailyUsageRecords: DailyUsageRecord[] = [];
let selectedTrendRange: TrendRange = "7d";
const APP_VERSION_FALLBACK = "0.1.4";

function renderTrendProviderOptions() {
  if (!trendProvider) return;
  const selected = trendProvider.value || "all";
  const providerIds = new Set([
    ...Array.from(configuredInstanceIds, baseProviderId),
    ...dailyUsageRecords.map((record) => baseProviderId(record.providerId)),
  ]);
  const options = [new Option("全部提供商", "all")];
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
  const points = selectDailyTrend(records, selectedTrendRange, providerId);
  const selectedName = providerId === "all" ? "全部提供商" : providerName(providerId);
  renderDailyTrendChart(trendChart, trendEmpty, trendDescription, points, selectedName, selectedTrendRange === "24h");
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
}

function ensureProviderRow(instanceId: string): HTMLElement | null {
  const existing = providerList?.querySelector<HTMLElement>(
    `.provider-row[data-provider="${instanceId}"]`,
  );
  if (existing) return existing;
  const row = createProviderRow(instanceId);
  if (row) providerList?.append(row);
  return row;
}

function createProviderRow(instanceId: string): HTMLElement | null {
  const provider = providerDefinition(instanceId);
  if (!provider) return null;
  const base = provider.id;
  const isGlm = base === "glm";
  const index = instanceIndexOf(instanceId);
  const displayName = instanceDisplayName(provider, index);
  const row = document.createElement("article");
  row.className = `provider-row${isGlm ? " featured" : ""}`;
  row.dataset.provider = instanceId;
  row.hidden = true;

  const handle = document.createElement("button");
  handle.type = "button";
  handle.className = "drag-handle";
  handle.textContent = "⠿";
  handle.title = "拖拽调整顺序，或按 Alt+↑/↓ 移动";
  handle.setAttribute("aria-label", `调整 ${displayName} 顺序：按住 Alt 并使用上下箭头移动`);
  attachRowDragging(row, handle);

  const identity = document.createElement("div");
  identity.className = "provider-identity";
  identity.title = `${provider.name} · ${provider.subtitle}`;
  const mark = document.createElement("img");
  mark.className = "provider-mark";
  mark.src = provider.logo;
  mark.alt = "";
  const heading = document.createElement("h3");
  heading.textContent = provider.name;
  if (index >= 2) {
    const badge = document.createElement("span");
    badge.className = "instance-badge";
    badge.textContent = `实例 ${index}`;
    heading.append(badge);
  }
  identity.append(mark, heading);

  const usage = document.createElement("div");
  usage.className = "usage-cell";
  const usageLabel = document.createElement("span");
  usageLabel.textContent = isGlm ? "今日 Token" : "在线摘要";
  const usageValue = document.createElement("strong");
  usageValue.id = isGlm ? `${instanceId}-tokens` : `${instanceId}-primary`;
  usageValue.textContent = "等待同步";
  const usageHint = document.createElement("small");
  usageHint.id = isGlm ? `${instanceId}-requests` : `${instanceId}-secondary`;
  usageHint.textContent = "已保存凭据";
  usage.append(usageLabel, usageValue, usageHint);

  const quota = document.createElement("div");
  quota.className = "quota-cell";
  const quotaLabel = document.createElement("span");
  quotaLabel.id = isGlm ? "" : `${instanceId}-quota-label`;
  quotaLabel.textContent = isGlm ? "窗口" : "在线口径";
  const quotaValue = document.createElement("b");
  quotaValue.id = isGlm ? `${instanceId}-percent` : `${instanceId}-quota-value`;
  quotaValue.textContent = "—";
  const progress = document.createElement("progress");
  progress.id = `${instanceId}-progress`;
  progress.max = 100;
  progress.value = 0;
  const quotaHint = document.createElement("small");
  quotaHint.id = isGlm ? `${instanceId}-cooldown` : `${instanceId}-quota-hint`;
  quotaHint.textContent = isGlm ? "重置时间未知" : "等待在线返回";
  quota.append(quotaLabel, quotaValue, progress, quotaHint);

  const configure = document.createElement("button");
  configure.className = "row-action";
  configure.type = "button";
  configure.dataset.action = "configure";
  configure.dataset.provider = instanceId;
  configure.textContent = "修改配置";

  const remove = document.createElement("button");
  remove.className = "row-action danger";
  remove.type = "button";
  remove.dataset.action = "delete-provider";
  remove.dataset.provider = instanceId;
  remove.setAttribute("aria-label", `删除 ${displayName}`);
  remove.textContent = "删除";

  const actions = document.createElement("div");
  actions.className = "row-actions";
  actions.append(configure, remove);

  const details = document.createElement("div");
  details.className = "provider-details";
  details.id = `${instanceId}-details`;
  details.setAttribute("aria-label", `${displayName}完整明细`);
  details.hidden = true;
  row.append(handle, identity, usage, quota, actions, details);
  return row;
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
    if (dragSourceRow === row) persistInstanceOrder();
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
    persistInstanceOrder();
  });
  handle.addEventListener("keydown", (event) => {
    if (!event.altKey || (event.key !== "ArrowUp" && event.key !== "ArrowDown")) return;
    event.preventDefault();
    const up = event.key === "ArrowUp";
    const sibling = up ? row.previousElementSibling : row.nextElementSibling;
    if (!(sibling instanceof HTMLElement)) return;
    if (up) sibling.before(row);
    else sibling.after(row);
    persistInstanceOrder();
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

function persistInstanceOrder() {
  const order = rowInstanceIds();
  savedInstanceOrder.splice(0, savedInstanceOrder.length, ...order);
  try {
    window.localStorage.setItem(PROVIDER_ORDER_KEY, JSON.stringify(order));
  } catch {
    // Ordering is a convenience; ignore storage failures.
  }
}

function rowInstanceIds(): string[] {
  return Array.from(
    providerList?.querySelectorAll<HTMLElement>(".provider-row") ?? [],
    (row) => row.dataset.provider ?? "",
  ).filter(Boolean);
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
  const rows = Array.from(providerList?.querySelectorAll<HTMLElement>(".provider-row") ?? []);
  rows.sort((left, right) => {
    const leftKey = instanceOrderKey(left.dataset.provider ?? "");
    const rightKey = instanceOrderKey(right.dataset.provider ?? "");
    return (
      leftKey[0] - rightKey[0] || leftKey[1] - rightKey[1] || leftKey[2] - rightKey[2]
    );
  });
  for (const row of rows) providerList?.append(row);
}

function instanceDisplayName(provider: ProviderDefinition, index: number): string {
  return index >= 2 ? `${provider.name} · 实例 ${index}` : provider.name;
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
  for (const row of providerList?.querySelectorAll<HTMLElement>(".provider-row") ?? []) {
    row.hidden = !configuredInstanceIds.has(row.dataset.provider ?? "");
  }
  if (providerEmpty) providerEmpty.hidden = configuredInstanceIds.size > 0;
  renderProviderCatalog();
  renderTrendProviderOptions();
  renderTotals();
}

function renderProviderCatalog() {
  if (!providerCatalog) return;
  providerCatalog.replaceChildren();
  for (const provider of providerDefinitions) {
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
    providerCatalog.append(button);
  }
}

function setStatus(message: string, state: "ready" | "syncing" | "error" = "ready") {
  syncStatus?.classList.toggle("syncing", state === "syncing");
  syncStatus?.classList.toggle("error", state === "error");
  if (syncStatus?.lastChild) syncStatus.lastChild.textContent = ` ${message}`;
}

type ViewId = "dashboard" | "about";

function routeFromHash(hash: string): { view: ViewId; anchor: string | null } {
  if (hash === "about") return { view: "about", anchor: null };
  if (hash === "providers") return { view: "dashboard", anchor: "providers" };
  return { view: "dashboard", anchor: null };
}

function applyRoute() {
  const hash = window.location.hash.slice(1);
  const { view, anchor } = routeFromHash(hash);
  byId("dashboard")?.toggleAttribute("hidden", view !== "dashboard");
  byId("about")?.toggleAttribute("hidden", view !== "about");
  updateNavActive(hash || "dashboard");
  if (anchor) {
    requestAnimationFrame(() => byId(anchor)?.scrollIntoView({ behavior: "smooth", block: "start" }));
  } else {
    const root = view === "about" ? byId("about") : byId("dashboard");
    root?.scrollTo({ top: 0 });
  }
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
  setText(`${instanceId}-tokens`, formatInteger(snapshot.totalTokens));
  setText(`${instanceId}-requests`, `${formatInteger(snapshot.requests)} 次调用 · ${snapshot.planLevel}`);
  setText(`${instanceId}-percent`, `${snapshot.usedPercent.toFixed(1)}%`);
  const progress = byId<HTMLProgressElement>(`${instanceId}-progress`);
  if (progress) progress.value = snapshot.usedPercent;
  renderProviderDetails(instanceId, snapshot.detailSections);
  updateCooldown();
  renderTotals();
  setStatus("刚刚完成在线同步");
}

function renderOnline(snapshot: OnlineSnapshot) {
  setInstanceConfigured(snapshot.providerId);
  onlineSnapshots.set(snapshot.providerId, snapshot);
  const primary = byId<HTMLElement>(`${snapshot.providerId}-primary`);
  const secondary = byId<HTMLElement>(`${snapshot.providerId}-secondary`);
  const quotaLabel = byId<HTMLElement>(`${snapshot.providerId}-quota-label`);
  const quotaValue = byId<HTMLElement>(`${snapshot.providerId}-quota-value`);
  const quotaHint = byId<HTMLElement>(`${snapshot.providerId}-quota-hint`);
  const progress = byId<HTMLProgressElement>(`${snapshot.providerId}-progress`);
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
    setText(`${instanceId}-cooldown`, formatCooldown(snapshot.cooldownEndsAtMs));
  }
}

async function syncGlm(instanceId: string) {
  if (!isTauri()) {
    setStatus("浏览器预览模式");
    return;
  }
  setStatus(`正在连接 ${providerName(instanceId)}`, "syncing");
  try {
    renderGlm(instanceId, await invoke<GlmSnapshot>("sync_glm", {
      providerId: instanceId,
      localDate: localDateKey(),
      slot: localQuarterSlot(),
      ...localDayRange(),
    }));
  } catch (reason) {
    const error = reason as CommandError;
    setStatus(error.message ?? "同步失败，请稍后重试", "error");
  }
}

async function syncOnline(instanceId: string) {
  if (!isTauri()) return;
  try {
    renderOnline(await invoke<OnlineSnapshot>("sync_online_provider", {
      providerId: instanceId,
      localDate: localDateKey(),
      slot: localQuarterSlot(),
      ...localDayRangeMs(),
    }));
  } catch (reason) {
    const error = reason as CommandError;
    setStatus(`${providerName(instanceId)}：${error.message ?? "同步失败"}`, "error");
  }
}

function orderedConfiguredInstances(): string[] {
  // Sync in the visual (drag-ordered) sequence when rows exist.
  const visualOrder = rowInstanceIds().filter((instanceId) =>
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
    await Promise.all(syncTasks);
    renderTotals();
    await loadDailyUsage();
  } finally {
    isSyncing = false;
  }
}

function providerName(instanceId: string) {
  const provider = providerDefinition(instanceId);
  if (!provider) return "供应商";
  return instanceDisplayName(provider, instanceIndexOf(instanceId));
}

function deleteProviderInstance(instanceId: string) {
  if (!providerDefinition(instanceId) || !isTauri()) return;
  pendingDeleteInstance = instanceId;
  if (confirmMessage) {
    confirmMessage.textContent = `删除「${providerName(instanceId)}」会清除本机保存的 API Key 与缓存摘要，但不影响已保存的历史趋势。确定继续吗？`;
  }
  confirmDialog?.showModal();
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
    if (cached.length > 0) setStatus("已载入本地缓存，正在刷新");
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

refreshButton?.addEventListener("click", async () => {
  refreshButton.disabled = true;
  refreshButton.textContent = "同步中…";
  await syncAll();
  refreshButton.disabled = false;
  refreshButton.textContent = "立即同步";
});

themeButton?.addEventListener("click", () => {
  const light = document.documentElement.toggleAttribute("data-light");
  themeButton.setAttribute("aria-label", light ? "切换深色主题" : "切换浅色主题");
});

autoSyncInterval?.addEventListener("change", () => {
  const seconds = Number(autoSyncInterval.value);
  window.localStorage.setItem("llm-usage:auto-sync-seconds", String(seconds));
  applyAutoSync(seconds);
});

document.addEventListener("click", (event) => {
  const button = (event.target as Element | null)?.closest<HTMLButtonElement>("button[data-action]");
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
});

function openProviderDialog(instanceId: string) {
  const provider = providerDefinition(instanceId);
  if (!provider || !credentialFields) return;
  selectedInstance = instanceId;
  catalogDialog?.close();
  if (dialogTitle) {
    dialogTitle.textContent = `配置 ${instanceDisplayName(provider, instanceIndexOf(instanceId))}`;
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
    return wrapper;
  }));
  dialog?.showModal();
  credentialFields.querySelector<HTMLInputElement>("input")?.focus();
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
      setStatus(`${snapshot.label} 已完成在线同步`);
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
    providerList
      ?.querySelector<HTMLElement>(`.provider-row[data-provider="${instanceId}"]`)
      ?.remove();
    renderProviderVisibility();
    renderTotals();
    await loadDailyUsage();
    setStatus(`已删除 ${providerName(instanceId)}`);
  } catch (reason) {
    const error = reason as CommandError;
    setStatus(`${providerName(instanceId)}：${error.message ?? "删除失败"}`, "error");
  } finally {
    pendingDeleteInstance = null;
    confirmDialog?.close();
    confirmAccept.disabled = false;
    confirmAccept.textContent = "删除";
  }
});

confirmDialog?.addEventListener("close", () => {
  pendingDeleteInstance = null;
  if (confirmAccept) {
    confirmAccept.disabled = false;
    confirmAccept.textContent = "删除";
  }
});
trendProvider?.addEventListener("change", renderTrend);
trendRange?.addEventListener("click", (event) => {
  const button = (event.target as Element).closest<HTMLButtonElement>("button[data-range]");
  if (!button) return;
  selectedTrendRange = button.dataset.range as TrendRange;
  for (const item of trendRange.querySelectorAll<HTMLButtonElement>("button[data-range]")) {
    item.setAttribute("aria-pressed", String(item === button));
  }
  renderTrend();
});
window.setInterval(updateCooldown, 30_000);
void initializeWindowControls();
if (isTauri()) void listen("tray-sync", () => void syncAll());
window.addEventListener("hashchange", applyRoute);
applyRoute();
void populateAboutMetadata();
void (async () => {
  const savedAutoSync = Number(window.localStorage.getItem("llm-usage:auto-sync-seconds") ?? "0");
  if (autoSyncInterval && Number.isFinite(savedAutoSync)) autoSyncInterval.value = String(savedAutoSync);
  applyAutoSync(Number.isFinite(savedAutoSync) ? savedAutoSync : 0);
  await loadProviderInstances();
  await loadCache();
  await loadDailyUsage();
  await syncAll();
})();
