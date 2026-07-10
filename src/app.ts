import { invoke, isTauri } from "@tauri-apps/api/core";
import { renderProviderDetails } from "./details";
import {
  formatCooldown,
  formatInteger,
  localDayRange,
  summarizeProviders,
  type OnlineDetailSection,
} from "./domain";
import {
  providerDefinition,
  providerDefinitions,
  serializeProviderCredential,
  unconfiguredProviders,
  type ProviderDefinition,
} from "./providers";
import "./styles.css";

interface GlmSnapshot {
  planLevel: string;
  usedPercent: number;
  cooldownEndsAtMs: number;
  requests: number;
  totalTokens: number;
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
const configuredProviderIds = new Set<string>();
let selectedProvider = "glm";
let glmResetAt: number | null = null;
let glmSnapshot: GlmSnapshot | null = null;
const onlineSnapshots = new Map<string, OnlineSnapshot>();
let autoSyncTimer: number | null = null;
let isSyncing = false;

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
  const mark = document.createElement("span");
  mark.className = `provider-mark ${provider.markClass}`;
  mark.textContent = provider.mark;
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
  configure.textContent = "更新";

  const details = document.createElement("div");
  details.className = "provider-details";
  details.id = `${provider.id}-details`;
  details.setAttribute("aria-label", `${provider.name}完整明细`);
  details.hidden = true;
  row.append(identity, usage, quota, configure, details);
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
    const name = document.createElement("strong");
    name.textContent = provider.name;
    const subtitle = document.createElement("span");
    subtitle.textContent = provider.subtitle;
    button.append(name, subtitle);
    providerCatalog.append(button);
  }
}

function setStatus(message: string, state: "ready" | "syncing" | "error" = "ready") {
  syncStatus?.classList.toggle("syncing", state === "syncing");
  syncStatus?.classList.toggle("error", state === "error");
  if (syncStatus?.lastChild) syncStatus.lastChild.textContent = ` ${message}`;
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
    renderGlm(await invoke<GlmSnapshot>("sync_glm", localDayRange()));
  } catch (reason) {
    const error = reason as CommandError;
    setStatus(error.message ?? "同步失败，请稍后重试", "error");
  }
}

async function syncOnline(providerId: string) {
  if (!isTauri()) return;
  try {
    renderOnline(await invoke<OnlineSnapshot>("sync_online_provider", { providerId }));
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
    if (configuredProviderIds.has("glm")) await syncGlm();
    for (const provider of providerDefinitions.filter(
      (item) => item.id !== "glm" && configuredProviderIds.has(item.id),
    )) {
      await syncOnline(provider.id);
    }
    renderTotals();
  } finally {
    isSyncing = false;
  }
}

function providerName(providerId: string) {
  return providerDefinition(providerId)?.name ?? "供应商";
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
  if (button.dataset.action === "open-catalog") {
    renderProviderCatalog();
    catalogDialog?.showModal();
    return;
  }
  if (button.dataset.action === "configure") openProviderDialog(button.dataset.provider ?? "glm");
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
        ...localDayRange(),
      });
      renderGlm(snapshot);
    } else {
      const snapshot = await invoke<OnlineSnapshot>("configure_online_provider", {
        providerId: selectedProvider,
        apiKey: credential,
      });
      renderOnline(snapshot);
      setStatus(`${snapshot.label} 已完成在线同步`);
    }
    setProviderConfigured(selectedProvider);
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
window.setInterval(updateCooldown, 30_000);
initializeProviderRows();
void (async () => {
  const savedAutoSync = Number(window.localStorage.getItem("llm-usage:auto-sync-seconds") ?? "0");
  if (autoSyncInterval && Number.isFinite(savedAutoSync)) autoSyncInterval.value = String(savedAutoSync);
  applyAutoSync(Number.isFinite(savedAutoSync) ? savedAutoSync : 0);
  await loadConfiguredProviders();
  await loadCache();
  await syncAll();
})();
