// Gond Code is dark only. These fail any change that brings light mode back:
// a light theme option or switch, a prefers-color-scheme query, or tokens scoped to `.dark`.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";
import { applyTint } from "./theme";

const root = join(import.meta.dirname, "../..");
const read = (path: string) => readFileSync(join(root, path), "utf8");

function sources(dir: string): string[] {
  return readdirSync(join(root, dir)).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(join(root, path)).isDirectory()) return sources(path);
    return /\.(ts|tsx|css)$/.test(name) && !/\.test\.tsx?$/.test(name) ? [path] : [];
  });
}

const FORBIDDEN: [RegExp, string][] = [
  [/prefers-color-scheme/, "follows the system light/dark setting"],
  [/color-scheme:\s*light/, "declares a light colour scheme"],
  [/["']light["']/, "has a 'light' theme value"],
  [/["']system["']\s*\|/, "has a 'system' theme value"],
  [/classList\.(remove|toggle)\(\s*["']dark["']/, "can take the dark class off"],
  [/\.light\b|:not\(\.dark\)/, "styles a light (non-dark) state"],
];

// Not a theme: a Gond Loop's size, "light" or "full".
const NOT_A_THEME = /\bsize\??:\s*["']light["']/;

describe("dark only", () => {
  it("has no light mode anywhere in src/ or index.html", () => {
    const hits: string[] = [];
    for (const path of [...sources("src"), "index.html"]) {
      read(path)
        .split("\n")
        .forEach((line, i) => {
          if (NOT_A_THEME.test(line)) return;
          for (const [pattern, why] of FORBIDDEN) {
            if (pattern.test(line))
              hits.push(`${relative(root, join(root, path))}:${i + 1} ${why}: ${line.trim()}`);
          }
        });
    }
    expect(hits).toEqual([]);
  });

  it("starts dark: <html> carries the dark class before any script runs", () => {
    expect(read("index.html")).toMatch(/<html[^>]*\bclass="[^"]*\bdark\b/);
  });

  it("puts the tokens on:root as a dark scheme, with every dark: variant always on", () => {
    const css = read("src/styles/theme.css");
    expect(css).toMatch(/:root\s*\{[^}]*color-scheme:\s*dark/);
    expect(css).toMatch(/@custom-variant dark \(&:where\(\*\)\)/);
    expect(css).not.toMatch(/^\.dark\s*\{/m);
  });

  it("applyTint keeps the root dark whatever the tint", () => {
    const classes = new Set<string>();
    const fake = { classList: { add: (c: string) => classes.add(c) }, dataset: {} as DOMStringMap };
    applyTint("green", fake as unknown as HTMLElement);
    expect(classes.has("dark")).toBe(true);
    expect(fake.dataset.tint).toBe("green");
  });
});
