// @vitest-environment happy-dom
import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it, vi } from "vitest";
import { onTerminalTheme, terminalTheme, TERMINAL_THEMES } from "../lib/terminalTheme";

// Read the real stylesheet, including aliases, so token changes really check contrast.
const css = readFileSync("src/styles/theme.css", "utf8");
function block(selector: string) {
  const start = css.indexOf(`${selector} {`);
  if (start < 0) throw new Error(`Missing ${selector}`);
  return css.slice(start, css.indexOf("}", start));
}
function token(source: string, name: string): string {
  const value = source.match(new RegExp(`--${name}:\\s*([^;]+);`))?.[1];
  if (!value) throw new Error(`--${name} is missing`);
  const alias = value.match(/^var\(--([\w-]+)\)$/)?.[1];
  return alias ? token(source, alias) : value;
}

function luminance(h: string) {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16) / 255);
  const lin = (c: number) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}
function contrast(a: string, b: string) {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

const SURFACES = [
  "background",
  "sidebar",
  "card",
  "popover",
  "raised",
  "pane-header",
  "hover",
  "secondary",
  "muted",
  "accent",
];
const TINTS = ["blue", "purple", "pink", "red", "orange", "amber", "green"];
const TONES = ["tone-ok", "tone-warn", "tone-bad", "tone-info"];

for (const mode of ["light", "dark"] as const) {
  const source = block(mode === "dark" ? ".dark" : ":root");
  describe(`${mode} theme contrast (WCAG AA, 4.5:1)`, () => {
    it("keeps semantic foregrounds and muted text readable on every surface", () => {
      for (const text of ["foreground", "muted-foreground", "tint-ink"]) {
        for (const surface of SURFACES) {
          expect(
            contrast(token(source, text), token(source, surface)),
            `${text} on ${surface}`,
          ).toBeGreaterThanOrEqual(4.5);
        }
      }
      for (const [text, surface] of [
        ["card-foreground", "card"],
        ["popover-foreground", "popover"],
        ["secondary-foreground", "secondary"],
        ["accent-foreground", "accent"],
        ["sidebar-foreground", "sidebar"],
        ["sidebar-accent-foreground", "sidebar-accent"],
        ["primary-foreground", "primary"],
        ["sidebar-primary-foreground", "sidebar-primary"],
      ]) {
        expect(
          contrast(token(source, text), token(source, surface)),
          `${text} on ${surface}`,
        ).toBeGreaterThanOrEqual(4.5);
      }
    });

    it("keeps Gond's status text readable on its intended surfaces", () => {
      // Dark's original tones work on all steps; Gond's light tones are page/paper ink.
      for (const text of TONES) {
        for (const surface of mode === "dark" ? SURFACES : ["background", "card"]) {
          expect(
            contrast(token(source, text), token(source, surface)),
            `${text} on ${surface}`,
          ).toBeGreaterThanOrEqual(4.5);
        }
      }
    });

    it("keeps white on solid and ink on wash readable for all seven scoped tints", () => {
      for (const tint of TINTS) {
        const shades = block(
          `:root${mode === "dark" ? ".dark" : ":not(.dark)"}[data-tint="${tint}"]`,
        );
        expect(
          contrast("#ffffff", token(shades, "tint-solid")),
          `white on ${tint}`,
        ).toBeGreaterThanOrEqual(4.5);
        expect(
          contrast(token(shades, "tint-ink"), token(shades, "tint-wash")),
          `${tint} ink on wash`,
        ).toBeGreaterThanOrEqual(4.5);
      }
    });

    it("keeps terminal text readable, including ANSI colours", () => {
      const palette = TERMINAL_THEMES[mode];
      expect(contrast(palette.foreground, palette.background)).toBeGreaterThanOrEqual(4.5);
      expect(contrast(palette.cursor, palette.cursorAccent)).toBeGreaterThanOrEqual(4.5);
      const ansi = [
        "black",
        "red",
        "green",
        "yellow",
        "blue",
        "magenta",
        "cyan",
        "white",
        "brightBlack",
        "brightRed",
        "brightGreen",
        "brightYellow",
        "brightBlue",
        "brightMagenta",
        "brightCyan",
        "brightWhite",
      ] as const;
      for (const colour of ansi) {
        // ANSI black intentionally remains nearly black in the original dark palette.
        if (mode === "dark" && colour === "black") continue;
        expect(
          contrast(palette[colour], palette.background),
          `${colour} on terminal background`,
        ).toBeGreaterThanOrEqual(3);
      }
    });
  });
}

it("preserves T3's measured dark surfaces and inactive pane header", () => {
  const dark = block(".dark");
  expect([
    token(dark, "background"),
    token(dark, "sidebar"),
    token(dark, "card"),
    token(dark, "border"),
    token(dark, "foreground"),
    token(dark, "pane-header"),
  ]).toEqual(["#000000", "#030303", "#0e0e0e", "#1c1c1c", "#ededed", "#070707"]);
  expect(css).toContain("@custom-variant dark (&:where(.dark, .dark *));");
  expect(css).not.toContain("@custom-variant dark (&:where(*));");
});

afterEach(() => {
  document.documentElement.className = "";
  delete document.documentElement.dataset.theme;
});

it("updates a mounted terminal on html mutations and theme events, and unsubscribes", async () => {
  const root = document.documentElement;
  const changed = vi.fn();
  const stop = onTerminalTheme(changed);
  try {
    expect(changed).toHaveBeenLastCalledWith(TERMINAL_THEMES.light);
    root.classList.add("dark");
    window.dispatchEvent(new Event("gond:theme"));
    // applyTheme's event is synchronous; the following mutation must not duplicate it.
    expect(changed).toHaveBeenLastCalledWith(TERMINAL_THEMES.dark);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(changed).toHaveBeenCalledTimes(2);

    root.classList.remove("dark");
    await vi.waitFor(() => expect(changed).toHaveBeenLastCalledWith(TERMINAL_THEMES.light));
    root.dataset.theme = "dark";
    await vi.waitFor(() => expect(changed).toHaveBeenLastCalledWith(TERMINAL_THEMES.dark));
    expect(terminalTheme()).toBe(TERMINAL_THEMES.dark);

    stop();
    changed.mockClear();
    root.dataset.theme = "light";
    window.dispatchEvent(new Event("gond:theme"));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(changed).not.toHaveBeenCalled();
  } finally {
    stop();
  }
});
