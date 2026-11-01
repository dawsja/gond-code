import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

// Reads the real theme.css, so a token change that breaks contrast fails here.
const css = readFileSync(new URL("./theme.css", import.meta.url), "utf8");
const root = css.slice(css.indexOf(":root {"), css.indexOf("}", css.indexOf(":root {")));
const hex = (block: string, name: string) =>
  block.match(new RegExp(`--${name}:\\s*(#[0-9a-f]{6})`, "i"))?.[1];
const token = (name: string) => {
  const value = hex(root, name);
  if (!value) throw new Error(`--${name} has no hex value in :root`);
  return value;
};

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
  "raised",
  "hover",
  "secondary",
  "muted",
  "accent",
];

describe("dark theme contrast (WCAG AA, 4.5:1)", () => {
  it("keeps text readable on every surface", () => {
    for (const text of [
      "foreground",
      "muted-foreground",
      "tone-ok",
      "tone-warn",
      "tone-bad",
      "tone-info",
      "tint-ink",
    ]) {
      for (const surface of SURFACES) {
        expect(
          contrast(token(text), token(surface)),
          `${text} on ${surface}`,
        ).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it("keeps white readable on every tint's primary button", () => {
    const solids = [
      ...css.matchAll(/\[data-tint="(\w+)"\] \{[^}]*--tint-solid:\s*(#[0-9a-f]{6})/gi),
    ];
    expect(solids.length).toBe(7);
    for (const [, name, solid] of solids) {
      expect(contrast("#ffffff", solid), `white on ${name}`).toBeGreaterThanOrEqual(4.5);
    }
  });

  it("matches T3's measured surfaces", () => {
    expect([
      token("background"),
      token("sidebar"),
      token("card"),
      token("border"),
      token("foreground"),
    ]).toEqual(["#000000", "#030303", "#0e0e0e", "#1c1c1c", "#ededed"]);
  });
});
