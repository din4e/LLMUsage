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

interface CommandError {
  code?: string;
  message?: string;
}

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

function setStatus(message: string, state: "ready" | "syncing" | "error" = "ready") {
  syncStatus?.classList.toggle("syncing", state === "syncing");
  syncStatus?.classList.toggle("error", state === "error");
  if (syncStatus?.lastChild) syncStatus.lastChild.textContent = ` ${message}`;
}

function renderGlm(snapshot: GlmSnapshot) {
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
  const totalRequests = byId<HTMLElement>("total-requests");
  const totalTokens = byId<HTMLElement>("total-tokens");
  const coverage = byId<HTMLElement>("coverage");
  if (totalRequests) totalRequests.textContent = formatInteger(snapshot.requests);
  if (totalTokens) totalTokens.textContent = formatInteger(snapshot.totalTokens);
  if (coverage) coverage.textContent = "1 / 3";
  setStatus("刚刚完成在线同步");
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

refreshButton?.addEventListener("click", async () => {
  refreshButton.disabled = true;
  refreshButton.textContent = "同步中…";
  await syncGlm(true);
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
    const names: Record<string, string> = { glm: "智谱 GLM", kimi: "Kimi", minimax: "MiniMax" };
    if (dialogTitle) dialogTitle.textContent = `配置 ${names[selectedProvider] ?? "供应商"}`;
    dialog?.showModal();
    apiKeyInput?.focus();
  });
});

providerForm?.addEventListener("submit", async (event) => {
  event.preventDefault();
  if (!apiKeyInput?.value || !saveButton) return;
  if (selectedProvider !== "glm") {
    setStatus(`${dialogTitle?.textContent ?? "该供应商"} 在线适配即将开放`, "error");
    dialog?.close();
    return;
  }
  if (!isTauri()) {
    setStatus("请在桌面应用中保存密钥", "error");
    dialog?.close();
    return;
  }
  saveButton.disabled = true;
  saveButton.textContent = "验证中…";
  try {
    const snapshot = await invoke<GlmSnapshot>("configure_glm", {
      apiKey: apiKeyInput.value,
      ...localDayRange(),
    });
    renderGlm(snapshot);
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
void syncGlm();
