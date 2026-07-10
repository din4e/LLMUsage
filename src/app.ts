import { invoke, isTauri } from "@tauri-apps/api/core";
import { formatCooldown, formatInteger, localDayRange } from "./domain";
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

const providers = [
  { id: "glm", name: "智谱 GLM", configured: "has_glm_credential" },
  { id: "kimi_cn", name: "Kimi 国内" },
  { id: "kimi_global", name: "Kimi Global" },
  { id: "deepseek", name: "DeepSeek" },
  { id: "minimax_cn", name: "MiniMax 国内" },
  { id: "minimax_global", name: "MiniMax Global" },
] as const;

const byId = <T extends HTMLElement>(id: string) => document.getElementById(id) as T | null;
const refreshButton = byId<HTMLButtonElement>("refresh-button");
const syncStatus = byId<HTMLElement>("sync-status");
const themeButton = byId<HTMLButtonElement>("theme-toggle");
const dialog = byId<HTMLDialogElement>("provider-dialog");
const providerForm = byId<HTMLFormElement>("provider-form");
const dialogTitle = byId<HTMLElement>("dialog-title");
const apiKeyInput = byId<HTMLInputElement>("api-key");
const saveButton = byId<HTMLButtonElement>("save-provider");
let selectedProvider = "glm";
let glmResetAt: number | null = null;
let glmSnapshot: GlmSnapshot | null = null;
const onlineSnapshots = new Map<string, OnlineSnapshot>();

function setStatus(message: string, state: "ready" | "syncing" | "error" = "ready") {
  syncStatus?.classList.toggle("syncing", state === "syncing");
  syncStatus?.classList.toggle("error", state === "error");
  if (syncStatus?.lastChild) syncStatus.lastChild.textContent = ` ${message}`;
}

function renderGlm(snapshot: GlmSnapshot) {
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
  if (progress && snapshot.quotaUsedPercent != null) progress.value = snapshot.quotaUsedPercent;
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
  const balance = Array.from(onlineSnapshots.values())
    .reduce((sum, snapshot) => sum + (snapshot.balanceCny ?? 0), 0);
  const configured = (glmSnapshot ? 1 : 0) + onlineSnapshots.size;
  if (totalRequests) totalRequests.textContent = glmSnapshot ? formatInteger(glmSnapshot.requests) : "—";
  if (totalTokens) totalTokens.textContent = glmSnapshot ? formatInteger(glmSnapshot.totalTokens) : "—";
  if (totalCost) totalCost.textContent = balance > 0 ? `¥${balance.toFixed(2)}` : "—";
  if (coverage) coverage.textContent = `${configured} / ${providers.length}`;
}

function updateCooldown() {
  const cooldown = byId<HTMLElement>("glm-cooldown");
  if (cooldown && glmResetAt !== null) cooldown.textContent = formatCooldown(glmResetAt);
}

async function syncGlm(showUnconfigured = false) {
  if (!isTauri()) {
    setStatus("浏览器预览模式");
    return;
  }
  setStatus("正在连接 GLM", "syncing");
  try {
    renderGlm(await invoke<GlmSnapshot>("sync_glm", localDayRange()));
  } catch (reason) {
    const error = reason as CommandError;
    if (error.code === "PROVIDER_NOT_CONFIGURED" && !showUnconfigured) setStatus("尚未配置供应商");
    else setStatus(error.message ?? "同步失败，请稍后重试", "error");
  }
}

async function syncOnline(providerId: string, showUnconfigured = false) {
  if (!isTauri()) return;
  try {
    renderOnline(await invoke<OnlineSnapshot>("sync_online_provider", { providerId }));
  } catch (reason) {
    const error = reason as CommandError;
    if (error.code === "PROVIDER_NOT_CONFIGURED" && !showUnconfigured) return;
    setStatus(`${providerName(providerId)}：${error.message ?? "同步失败"}`, "error");
  }
}

async function syncAll(showUnconfigured = false) {
  await syncGlm(showUnconfigured);
  for (const provider of providers.filter((item) => item.id !== "glm")) {
    await syncOnline(provider.id, showUnconfigured);
  }
  renderTotals();
}

function providerName(providerId: string) {
  return providers.find((provider) => provider.id === providerId)?.name ?? "供应商";
}

async function loadCache() {
  if (!isTauri()) return;
  try {
    const cached = await invoke<CachedSnapshot[]>("load_cached_snapshots");
    for (const entry of cached) {
      if (entry.kind === "glm") renderGlm(entry.snapshot as GlmSnapshot);
      if (entry.kind === "online") renderOnline(entry.snapshot as OnlineSnapshot);
    }
    if (cached.length > 0) setStatus("已载入本地缓存，正在刷新");
  } catch {
    setStatus("本地缓存不可用", "error");
  }
}

refreshButton?.addEventListener("click", async () => {
  refreshButton.disabled = true;
  refreshButton.textContent = "同步中…";
  await syncAll(true);
  refreshButton.disabled = false;
  refreshButton.textContent = "立即同步";
});

themeButton?.addEventListener("click", () => {
  const light = document.documentElement.toggleAttribute("data-light");
  themeButton.setAttribute("aria-label", light ? "切换深色主题" : "切换浅色主题");
});

document.querySelectorAll<HTMLButtonElement>("[data-action='configure']").forEach((button) => {
  button.addEventListener("click", () => {
    selectedProvider = button.dataset.provider ?? "glm";
    if (dialogTitle) dialogTitle.textContent = `配置 ${providerName(selectedProvider)}`;
    dialog?.showModal();
    apiKeyInput?.focus();
  });
});

providerForm?.addEventListener("submit", async (event) => {
  event.preventDefault();
  if (!apiKeyInput?.value || !saveButton) return;
  if (!isTauri()) {
    setStatus("请在桌面应用中保存密钥", "error");
    dialog?.close();
    return;
  }
  saveButton.disabled = true;
  saveButton.textContent = "验证中…";
  try {
    if (selectedProvider === "glm") {
      const snapshot = await invoke<GlmSnapshot>("configure_glm", {
        apiKey: apiKeyInput.value,
        ...localDayRange(),
      });
      renderGlm(snapshot);
    } else {
      const snapshot = await invoke<OnlineSnapshot>("configure_online_provider", {
        providerId: selectedProvider,
        apiKey: apiKeyInput.value,
      });
      renderOnline(snapshot);
      setStatus(`${snapshot.label} 已完成在线同步`);
    }
    dialog?.close();
  } catch (reason) {
    const error = reason as CommandError;
    apiKeyInput.setCustomValidity(error.message ?? "密钥验证失败");
    apiKeyInput.reportValidity();
  } finally {
    saveButton.disabled = false;
    saveButton.textContent = "保存并同步";
  }
});

apiKeyInput?.addEventListener("input", () => apiKeyInput.setCustomValidity(""));
dialog?.addEventListener("close", () => { if (apiKeyInput) apiKeyInput.value = ""; });
window.setInterval(updateCooldown, 30_000);
void (async () => {
  await loadCache();
  await syncAll();
})();
