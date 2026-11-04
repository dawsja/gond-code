// @vitest-environment happy-dom
// Replaces the former dark-only product guard with the System / Light / Dark contract.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { runInNewContext } from "node:vm";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  applyTheme,
  applyTint,
  DARK_QUERY,
  normalizeTheme,
  readStoredTheme,
  resolveTheme,
  THEMES,
  THEME_STORAGE_KEY,
  watchSystemTheme,
} from "./theme";
import { mockCall } from "./mock";

const root = join(import.meta.dirname, "../..");
const read = (path: string) => readFileSync(join(root, path), "utf8");
function sources(dir: string): string[] {
  return readdirSync(join(root, dir)).flatMap((name) => {
    const path = join(dir, name);
    return statSync(join(root, path)).isDirectory()
      ? sources(path)
      : path.endsWith(".tsx")
        ? [path]
        : [];
  });
}

function device(dark: boolean) {
  const media = new EventTarget();
  Object.assign(media, { matches: dark });
  vi.stubGlobal(
    "matchMedia",
    vi.fn((query: string) => {
      expect(query).toBe(DARK_QUERY);
      return media;
    }),
  );
  return media as EventTarget & { matches: boolean };
}

afterEach(() => {
  vi.unstubAllGlobals();
  localStorage.clear();
  document.documentElement.className = "";
  delete document.documentElement.dataset.theme;
});

describe("theme structure", () => {
  it("offers exactly System, Light and Dark, with matching semantic tokens", () => {
    expect(THEMES).toEqual(["system", "light", "dark"]);
    const css = read("src/styles/theme.css");
    const tokens = (selector: string) => {
      const source = css.match(new RegExp(`(?:^|\\n)${selector} \\{([^}]+)\\}`, "m"))?.[1];
      expect(source).toBeTruthy();
      return [...source!.matchAll(/(--[\w-]+):/g)].map((m) => m[1]).sort();
    };
    expect(tokens(":root")).toEqual(tokens("\\.dark"));
    expect(tokens(":root").length).toBeGreaterThan(40);
  });

  it("keeps component colours semantic (brand artwork is deliberately exempt)", () => {
    const hits = sources("src")
      .filter((path) => !path.includes("components/brand/") && !path.includes(".test."))
      .flatMap((path) =>
        read(path)
          .split("\n")
          .flatMap((line, i) =>
            /#[\da-f]{3,8}\b/i.test(line) ? [`${path}:${i + 1}: ${line.trim()}`] : [],
          ),
      );
    expect(hits).toEqual([]);
  });

  it("boots with a CSP-safe blocking first-paint script, not a forced dark class", () => {
    const html = read("index.html");
    expect(html).toContain('<script src="/theme-init.js"></script>');
    expect(html.indexOf("/theme-init.js")).toBeLessThan(html.indexOf("/src/main.tsx"));
    expect(html).not.toMatch(/<html[^>]*class="dark"/);
    const config = JSON.parse(read("src-tauri/tauri.conf.json"));
    expect(config.app.security.csp["script-src"]).toBe("'self'");
    expect(config.app.windows[0].theme).not.toBe("Dark");
  });
});

for (const dark of [false, true]) {
  for (const theme of THEMES) {
    it(`applies ${theme} with device dark=${dark}, retaining tint and matching first paint`, () => {
      device(dark);
      const root = document.documentElement;
      applyTint("green");
      applyTheme(theme);
      const resolved = theme === "system" ? (dark ? "dark" : "light") : theme;
      expect(resolveTheme(theme)).toBe(resolved);
      expect(root.classList.contains("dark")).toBe(resolved === "dark");
      expect(root.dataset.theme).toBe(theme);
      expect(root.style.colorScheme).toBe(resolved);
      expect(root.dataset.tint).toBe("green");
      expect(readStoredTheme()).toBe(theme);
      root.className = resolved === "dark" ? "" : "dark";
      root.dataset.theme = "stale";
      runInNewContext(read("public/theme-init.js"), {
        document,
        localStorage,
        matchMedia: window.matchMedia,
      });
      expect(root.classList.contains("dark")).toBe(resolved === "dark");
      expect(root.dataset.theme).toBe(theme);
      expect(root.style.colorScheme).toBe(resolved);
      expect(root.dataset.tint).toBe("green");
    });
  }
}

it("follows live device changes only for System and releases its listener", () => {
  const media = device(false);
  const stop = watchSystemTheme("system");
  applyTheme("system");
  media.matches = true;
  media.dispatchEvent(new Event("change"));
  expect(document.documentElement.classList.contains("dark")).toBe(true);
  stop();
  media.matches = false;
  media.dispatchEvent(new Event("change"));
  expect(document.documentElement.classList.contains("dark")).toBe(true);
  const stopLight = watchSystemTheme("light");
  applyTheme("light");
  media.matches = true;
  media.dispatchEvent(new Event("change"));
  expect(document.documentElement.classList.contains("dark")).toBe(false);
  stopLight();
});

it("round-trips settings patches without losing tint; unknown themes fall back to System", async () => {
  await mockCall("settings_set", { patch: { tint: "purple" } });
  for (const theme of THEMES) {
    await mockCall("settings_set", { patch: { theme } });
    expect(await mockCall("settings_get", {})).toMatchObject({ tint: "purple", theme });
  }
  expect(normalizeTheme("unknown")).toBe("system");
  await mockCall("settings_set", JSON.parse('{"patch":{"theme":"future-theme"}}'));
  expect(await mockCall("settings_get", {})).toMatchObject({ tint: "purple", theme: "system" });
  localStorage.setItem(THEME_STORAGE_KEY, "unknown");
  expect(readStoredTheme()).toBe("system");
  device(false);
  runInNewContext(read("public/theme-init.js"), {
    document,
    localStorage,
    matchMedia: window.matchMedia,
  });
  expect(document.documentElement.dataset.theme).toBe("system");
  expect(document.documentElement.classList.contains("dark")).toBe(false);
});

it("works with blocked localStorage", () => {
  device(true);
  vi.stubGlobal("localStorage", {
    getItem: () => {
      throw new Error("blocked");
    },
    setItem: () => {
      throw new Error("blocked");
    },
    clear: () => {},
  });
  expect(readStoredTheme()).toBe("system");
  expect(() => applyTheme("light")).not.toThrow();
  expect(document.documentElement.dataset.theme).toBe("light");
});
