import "./styles.css";

const refreshButton = document.querySelector<HTMLButtonElement>("#refresh-button");
const syncStatus = document.querySelector<HTMLElement>("#sync-status");
const themeButton = document.querySelector<HTMLButtonElement>("#theme-toggle");
const dialog = document.querySelector<HTMLDialogElement>("#provider-dialog");
const dialogTitle = document.querySelector<HTMLElement>("#dialog-title");
const apiKeyInput = document.querySelector<HTMLInputElement>("#api-key");

refreshButton?.addEventListener("click", () => {
  refreshButton.disabled = true;
  refreshButton.textContent = "同步中…";
  syncStatus?.classList.add("syncing");
  if (syncStatus) syncStatus.lastChild!.textContent = " 正在连接供应商";
  window.setTimeout(() => {
    refreshButton.disabled = false;
    refreshButton.textContent = "立即同步";
    syncStatus?.classList.remove("syncing");
    if (syncStatus) syncStatus.lastChild!.textContent = " 尚未配置供应商";
  }, 650);
});

themeButton?.addEventListener("click", () => {
  const isLight = document.documentElement.toggleAttribute("data-light");
  themeButton.setAttribute("aria-label", isLight ? "切换深色主题" : "切换浅色主题");
});

document.querySelectorAll<HTMLButtonElement>("[data-action='configure']").forEach((button) => {
  button.addEventListener("click", () => {
    const names: Record<string, string> = { glm: "智谱 GLM", kimi: "Kimi", minimax: "MiniMax" };
    if (dialogTitle) dialogTitle.textContent = `配置 ${names[button.dataset.provider ?? ""] ?? "供应商"}`;
    dialog?.showModal();
    apiKeyInput?.focus();
  });
});

dialog?.addEventListener("close", () => {
  if (apiKeyInput) apiKeyInput.value = "";
});
