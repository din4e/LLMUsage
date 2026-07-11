import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

export type WindowAction = "minimize" | "maximize" | "close";

export interface WindowController {
  minimize(): Promise<void>;
  toggleMaximize(): Promise<void>;
  close(): Promise<void>;
}

export function performWindowAction(
  action: WindowAction,
  window: WindowController,
): Promise<void> {
  if (action === "minimize") return window.minimize();
  if (action === "maximize") return window.toggleMaximize();
  return window.close();
}

export function maximizeControlLabel(maximized: boolean): "最大化" | "还原" {
  return maximized ? "还原" : "最大化";
}

export async function initializeWindowControls(root: ParentNode = document): Promise<void> {
  const header = root.querySelector<HTMLElement>("#window-header");
  const maximizeButton = root.querySelector<HTMLButtonElement>(
    '[data-window-action="maximize"]',
  );
  const buttons = Array.from(
    root.querySelectorAll<HTMLButtonElement>("button[data-window-action]"),
  );
  if (!header || !maximizeButton || !buttons.length) return;

  if (!isTauri()) {
    for (const button of buttons) button.disabled = true;
    return;
  }

  const appWindow = getCurrentWindow();
  header.dataset.runtime = "tauri";

  const refreshMaximizeState = async () => {
    const maximized = await appWindow.isMaximized();
    maximizeButton.setAttribute("aria-label", maximizeControlLabel(maximized));
    maximizeButton.dataset.maximized = String(maximized);
  };

  for (const button of buttons) {
    button.addEventListener("click", () => {
      const action = button.dataset.windowAction as WindowAction;
      void performWindowAction(action, appWindow)
        .then(async () => {
          if (action === "maximize") await refreshMaximizeState();
        })
        .catch(() => undefined);
    });
  }

  header.addEventListener("dblclick", (event) => {
    if ((event.target as Element | null)?.closest("button")) return;
    void performWindowAction("maximize", appWindow)
      .then(refreshMaximizeState)
      .catch(() => undefined);
  });

  await refreshMaximizeState();
  await appWindow.onResized(() => void refreshMaximizeState().catch(() => undefined));
}
