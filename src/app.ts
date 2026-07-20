import { invoke, isTauri } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { listen } from "@tauri-apps/api/event";
import { renderProviderDetails } from "./details";
import {
  formatCooldown,
  formatInteger,
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
  providerDefinition,
  providerDefinitions,
  serializeProviderCredential,
  unconfiguredProviders,
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
let pendingDeleteProvider: string | null = null;
const configuredProviderIds = new Set<string>();
let selectedProvider = "glm";
let glmResetAt: number | null = null;
let glmSnapshot: GlmSnapshot | null = null;
const onlineSnapshots = new Map<string, OnlineSnapshot>();
let autoSyncTimer: number | null = null;
let isSyncing = false;
let dailyUsageRecords: DailyUsageRecord[] = [];
let selectedTrendRange: TrendRange = "7d";
const APP_VERSION_FALLBACK = "0.1.4";

function renderTrendProviderOptions() {
  if (!trendProvider) return;
  const selected = trendProvider.value || "all";
  const providerIds = new Set([
    ...configuredProviderIds,
    ...dailyUsageRecords.map((record) => record.providerId),
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
  const points = selectDailyTrend(dailyUsageRecords, selectedTrendRange, providerId);
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

function initializeProviderRows() {
  if (!providerList) return;
  providerList.replaceChildren(...providerDefinitions.map(createProviderRow));
  renderProviderVisibility();
}

function createProviderRow(provider: ProviderDefinition): HTMLElement {
  const row = document.createElement("article");
  row.className = `provider-row${provider.id === "glm" ? " featured" : ""}`;
  row.dataset.provider = provider.id;
  row.hidden = true;

  const identity = document.createElement("div");
  identity.className = "provider-identity";
  const mark = document.createElement("img");
  mark.className = "provider-mark";
  mark.src = provider.logo;
  mark.alt = `${provider.name} 图标`;
  const identityCopy = document.createElement("div");
  const heading = document.createElement("h3");
  heading.textContent = provider.name;
  const subtitle = document.createElement("p");
  subtitle.textContent = provider.subtitle;
  identityCopy.append(heading, subtitle);
  identity.append(mark, identityCopy);

  const usage = document.createElement("div");
  usage.className = "usage-cell";
  const usageLabel = document.createElement("span");
  usageLabel.textContent = provider.id === "glm" ? "今日 Token" : "在线摘要";
  const usageValue = document.createElement("strong");
  usageValue.id = provider.id === "glm" ? "glm-tokens" : `${provider.id}-primary`;
  usageValue.textContent = "等待同步";
  const usageHint = document.createElement("small");
  usageHint.id = provider.id === "glm" ? "glm-requests" : `${provider.id}-secondary`;
  usageHint.textContent = "已保存凭据";
  usage.append(usageLabel, usageValue, usageHint);

  const quota = document.createElement("div");
  quota.className = "quota-cell";
  const quotaHeading = document.createElement("div");
  const quotaLabel = document.createElement("span");
  quotaLabel.id = provider.id === "glm" ? "" : `${provider.id}-quota-label`;
  quotaLabel.textContent = provider.id === "glm" ? "5 小时窗口" : "在线口径";
  const quotaValue = document.createElement("b");
  quotaValue.id = provider.id === "glm" ? "glm-percent" : `${provider.id}-quota-value`;
  quotaValue.textContent = "—";
  quotaHeading.append(quotaLabel, quotaValue);
  const progress = document.createElement("progress");
  progress.id = provider.id === "glm" ? "glm-progress" : `${provider.id}-progress`;
  progress.max = 100;
  progress.value = 0;
  const quotaHint = document.createElement("small");
  quotaHint.id = provider.id === "glm" ? "glm-cooldown" : `${provider.id}-quota-hint`;
  quotaHint.textContent = provider.id === "glm" ? "重置时间未知" : "等待在线返回";
  quota.append(quotaHeading, progress, quotaHint);

  const configure = document.createElement("button");
  configure.className = "row-action";
  configure.type = "button";
  configure.dataset.action = "configure";
  configure.dataset.provider = provider.id;
  configure.textContent = "修改配置";

  const remove = document.createElement("button");
  remove.className = "row-action danger";
  remove.type = "button";
  remove.dataset.action = "delete-provider";
  remove.dataset.provider = provider.id;
  remove.setAttribute("aria-label", `删除 ${provider.name}`);
  remove.textContent = "删除";

  const actions = document.createElement("div");
  actions.className = "row-actions";
  actions.append(configure, remove);

  const details = document.createElement("div");
  details.className = "provider-details";
  details.id = `${provider.id}-details`;
  details.setAttribute("aria-label", `${provider.name}完整明细`);
  details.hidden = true;
  row.append(identity, usage, quota, actions, details);
  return row;
}

function setProviderConfigured(providerId: string) {
  configuredProviderIds.add(providerId);
  renderProviderVisibility();
}

function renderProviderVisibility() {
  for (const provider of providerDefinitions) {
    const row = document.querySelector<HTMLElement>(`.provider-row[data-provider="${provider.id}"]`);
    if (row) row.hidden = !configuredProviderIds.has(provider.id);
  }
  if (providerEmpty) providerEmpty.hidden = configuredProviderIds.size > 0;
  renderProviderCatalog();
  renderTrendProviderOptions();
  renderTotals();
}

function renderProviderCatalog() {
  if (!providerCatalog) return;
  providerCatalog.replaceChildren();
  const available = unconfiguredProviders(configuredProviderIds);
  if (!available.length) {
    const complete = document.createElement("p");
    complete.className = "catalog-complete";
    complete.textContent = "全部供应商都已配置。";
    providerCatalog.append(complete);
    return;
  }
  for (const provider of available) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "catalog-item";
    button.dataset.action = "configure";
    button.dataset.provider = provider.id;
    const mark = document.createElement("img");
    mark.className = "catalog-mark";
    mark.src = provider.logo;
    mark.alt = "";
    const name = document.createElement("strong");
    name.textContent = provider.name;
    const subtitle = document.createElement("span");
    subtitle.textContent = provider.subtitle;
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

function renderGlm(snapshot: GlmSnapshot) {
  setProviderConfigured("glm");
  glmSnapshot = snapshot;
  glmResetAt = snapshot.cooldownEndsAtMs;
  const tokens = byId<HTMLElement>("glm-tokens");
  const requests = byId<HTMLElement>("glm-requests");
  const percent = byId<HTMLElement>("glm-percent");
  const progress = byId<HTMLProgressElement>("glm-progress");
  if (tokens) tokens.textContent = formatInteger(snapshot.totalTokens);
  if (requests) requests.textContent = `${formatInteger(snapshot.requests)} 次调用 · ${snapshot.planLevel}`;
  if (percent) percent.textContent = `${snapshot.usedPercent.toFixed(1)}%`;
  if (progress) progress.value = snapshot.usedPercent;
  renderProviderDetails("glm", snapshot.detailSections);
  updateCooldown();
  renderTotals();
  setStatus("刚刚完成在线同步");
}

function renderOnline(snapshot: OnlineSnapshot) {
  setProviderConfigured(snapshot.providerId);
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
    ...(glmSnapshot ? [{ requests: glmSnapshot.requests, totalTokens: glmSnapshot.totalTokens, estimatedCostCny: null }] : []),
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
  if (coverage) coverage.textContent = `${configuredProviderIds.size} / ${providerDefinitions.length}`;
}

function updateCooldown() {
  const cooldown = byId<HTMLElement>("glm-cooldown");
  if (cooldown && glmResetAt !== null) cooldown.textContent = formatCooldown(glmResetAt);
}

async function syncGlm() {
  if (!isTauri()) {
    setStatus("浏览器预览模式");
    return;
  }
  setStatus("正在连接 GLM", "syncing");
  try {
    renderGlm(await invoke<GlmSnapshot>("sync_glm", {
      localDate: localDateKey(),
      slot: localQuarterSlot(),
      ...localDayRange(),
    }));
  } catch (reason) {
    const error = reason as CommandError;
    setStatus(error.message ?? "同步失败，请稍后重试", "error");
  }
}

async function syncOnline(providerId: string) {
  if (!isTauri()) return;
  try {
    renderOnline(await invoke<OnlineSnapshot>("sync_online_provider", {
      providerId,
      localDate: localDateKey(),
      slot: localQuarterSlot(),
      ...localDayRangeMs(),
    }));
  } catch (reason) {
    const error = reason as CommandError;
    setStatus(`${providerName(providerId)}：${error.message ?? "同步失败"}`, "error");
  }
}

async function syncAll() {
  if (isSyncing) return;
  if (!configuredProviderIds.size) {
    setStatus("请先添加供应商");
    return;
  }
  isSyncing = true;
  try {
    const syncTasks: Promise<unknown>[] = [];
    if (configuredProviderIds.has("glm")) syncTasks.push(syncGlm());
    syncTasks.push(
      ...providerDefinitions
        .filter((item) => item.id !== "glm" && configuredProviderIds.has(item.id))
        .map((provider) => syncOnline(provider.id)),
    );
    await Promise.all(syncTasks);
    renderTotals();
    await loadDailyUsage();
  } finally {
    isSyncing = false;
  }
}

function providerName(providerId: string) {
  return providerDefinition(providerId)?.name ?? "供应商";
}

function deleteProvider(providerId: string) {
  const provider = providerDefinition(providerId);
  if (!provider || !isTauri()) return;
  pendingDeleteProvider = providerId;
  if (confirmMessage) {
    confirmMessage.textContent = `删除「${provider.name}」会清除本机保存的 API Key 与缓存摘要，但不影响已保存的历史趋势。确定继续吗？`;
  }
  confirmDialog?.showModal();
}

async function loadConfiguredProviders() {
  if (!isTauri()) {
    renderProviderVisibility();
    return;
  }
  const configured = await Promise.all(providerDefinitions.map(async (provider) => {
    const command = provider.id === "glm" ? "has_glm_credential" : "has_online_credential";
    const args = provider.id === "glm" ? undefined : { providerId: provider.id };
    try {
      return [provider.id, await invoke<boolean>(command, args)] as const;
    } catch {
      return [provider.id, false] as const;
    }
  }));
  for (const [providerId, exists] of configured) if (exists) configuredProviderIds.add(providerId);
  renderProviderVisibility();
}

async function loadCache() {
  if (!isTauri()) return;
  try {
    const cached = await invoke<CachedSnapshot[]>("load_cached_snapshots");
    for (const entry of cached) {
      if (!configuredProviderIds.has(entry.providerId)) continue;
      if (entry.kind === "glm") renderGlm(entry.snapshot as GlmSnapshot);
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
    openProviderDialog(button.dataset.provider ?? "glm");
    return;
  }
  if (button.dataset.action === "delete-provider") {
    deleteProvider(button.dataset.provider ?? "");
    return;
  }
  if (button.dataset.action === "close-confirm-dialog") {
    confirmDialog?.close();
  }
});

function openProviderDialog(providerId: string) {
  const provider = providerDefinition(providerId);
  if (!provider || !credentialFields) return;
  selectedProvider = provider.id;
  catalogDialog?.close();
  if (dialogTitle) dialogTitle.textContent = `配置 ${provider.name}`;
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
    const credential = serializeProviderCredential(selectedProvider, values);
    if (selectedProvider === "glm") {
      const snapshot = await invoke<GlmSnapshot>("configure_glm", {
        apiKey: credential,
        localDate: localDateKey(),
        slot: localQuarterSlot(),
        ...localDayRange(),
      });
      renderGlm(snapshot);
    } else {
      const snapshot = await invoke<OnlineSnapshot>("configure_online_provider", {
        providerId: selectedProvider,
        apiKey: credential,
        localDate: localDateKey(),
        slot: localQuarterSlot(),
        ...localDayRangeMs(),
      });
      renderOnline(snapshot);
      setStatus(`${snapshot.label} 已完成在线同步`);
    }
    setProviderConfigured(selectedProvider);
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
  const providerId = pendingDeleteProvider;
  if (!providerId || !confirmAccept) {
    confirmDialog?.close();
    return;
  }
  confirmAccept.disabled = true;
  confirmAccept.textContent = "删除中…";
  try {
    await invoke("delete_provider", { providerId });
    configuredProviderIds.delete(providerId);
    onlineSnapshots.delete(providerId);
    if (providerId === "glm") glmSnapshot = null;
    renderProviderVisibility();
    renderTotals();
    await loadDailyUsage();
    setStatus(`已删除 ${providerName(providerId)}`);
  } catch (reason) {
    const error = reason as CommandError;
    setStatus(`${providerName(providerId)}：${error.message ?? "删除失败"}`, "error");
  } finally {
    pendingDeleteProvider = null;
    confirmDialog?.close();
    confirmAccept.disabled = false;
    confirmAccept.textContent = "删除";
  }
});

confirmDialog?.addEventListener("close", () => {
  pendingDeleteProvider = null;
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
initializeProviderRows();
void populateAboutMetadata();
void (async () => {
  const savedAutoSync = Number(window.localStorage.getItem("llm-usage:auto-sync-seconds") ?? "0");
  if (autoSyncInterval && Number.isFinite(savedAutoSync)) autoSyncInterval.value = String(savedAutoSync);
  applyAutoSync(Number.isFinite(savedAutoSync) ? savedAutoSync : 0);
  await loadConfiguredProviders();
  await loadCache();
  await loadDailyUsage();
  await syncAll();
})();
