import { describe, expect, it, vi } from "vitest";
import { maximizeControlLabel, performWindowAction, type WindowController } from "./window-controls";

function fakeWindow(): WindowController {
  return {
    hide: vi.fn().mockResolvedValue(undefined),
    toggleMaximize: vi.fn().mockResolvedValue(undefined),
  };
}

describe("window controls", () => {
  it.each([
    ["minimize", "hide"],
    ["maximize", "toggleMaximize"],
    ["close", "hide"],
  ] as const)("maps %s to the matching Tauri window command", async (action, method) => {
    const window = fakeWindow();

    await performWindowAction(action, window);

    expect(window[method]).toHaveBeenCalledOnce();
  });

  it("describes whether the maximize button will maximize or restore", () => {
    expect(maximizeControlLabel(false)).toBe("最大化");
    expect(maximizeControlLabel(true)).toBe("还原");
  });
});
