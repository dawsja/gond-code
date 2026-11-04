// @vitest-environment happy-dom
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { setAppStateStore } from "./appState";
import { createStateStore } from "./state";
import { useSettings } from "./useSettings";
import { THEME_STORAGE_KEY } from "./theme";

vi.mock("./terminalFont", () => ({ setTerminalFont: vi.fn() }));
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  localStorage.clear();
});

it("replaces the first-paint mirror with saved settings and follows settings state patches", async () => {
  vi.stubGlobal("matchMedia", () => ({
    matches: false,
    addEventListener() {},
    removeEventListener() {},
  }));
  localStorage.setItem(THEME_STORAGE_KEY, "dark");
  const store = createStateStore({ epoch: "themes", version: 0, value: {} });
  setAppStateStore(store);
  const { result } = renderHook(() => useSettings());
  await waitFor(() => expect(result.current.settings.theme).toBe("system"));
  expect(document.documentElement.classList.contains("dark")).toBe(false);
  await act(async () => result.current.change({ theme: "dark" }));
  await waitFor(() => expect(result.current.settings.theme).toBe("dark"));
  expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe("dark");
  act(() => {
    store.receive({
      epoch: "themes",
      version: 1,
      ops: [{ op: "add", path: "/settings", value: { tint: "red", theme: "light" } }],
      cause: {},
    });
  });
  expect(result.current.settings.theme).toBe("light");
  expect(document.documentElement.classList.contains("dark")).toBe(false);
  expect(document.documentElement.dataset.tint).toBe("red");
  expect(localStorage.getItem(THEME_STORAGE_KEY)).toBe("light");
});
