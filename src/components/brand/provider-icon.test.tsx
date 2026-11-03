// @vitest-environment happy-dom
import { cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { ProviderIcon } from "./provider-icon";

afterEach(() => {
  cleanup();
  document.documentElement.classList.remove("dark");
});

// Fixed brand colours must survive both palettes; mono (and Kimi's main shape) keeps the
// asset's currentColor instead of being rewritten to a hard-coded light/dark fill.
it.each(["light", "dark"])("preserves bundled paints in %s mode", async (theme) => {
  document.documentElement.classList.toggle("dark", theme === "dark");
  const view = render(
    <>
      <ProviderIcon id="claude" />
      <ProviderIcon id="pi" />
      <ProviderIcon id="kimi" />
      <ProviderIcon id="claude" variant="mono" />
      <ProviderIcon id="openai" variant="color" />
    </>,
  );
  await waitFor(() => expect(view.container.querySelectorAll("svg")).toHaveLength(5), {
    timeout: 5000,
  });
  const { COLOR, MONO } = await import("./svgs");
  const assets = [COLOR.claude, COLOR.pi, COLOR.kimi, MONO.claude, MONO.openai];
  const icons = [...view.container.querySelectorAll('[data-slot="provider-icon"]')];
  icons.forEach((icon, index) => {
    const expected = document.createElement("div");
    expected.innerHTML = assets[index];
    expect(icon.querySelector("svg")?.outerHTML).toBe(expected.querySelector("svg")?.outerHTML);
    // No blanket inversion, tint or ink tile for marks that already read in both modes.
    expect(icon.getAttribute("class")).not.toMatch(/(?:invert|filter|fill-|stroke-|bg-)/);
    expect(icon.getAttribute("style")).not.toMatch(/(?:color|filter|background)/);
  });
});

// Two copies of a logo with a gradient (Codex's) must not share its id: when the first copy is
// in a hidden session, every other copy drew blank, a white square.
it("gives each copy of a logo its own gradient ids", async () => {
  const view = render(
    <>
      <ProviderIcon id="codex" />
      <ProviderIcon id="codex" />
    </>,
  );
  // The logo chunk loads on first use; under a full test run that can take a few seconds.
  await waitFor(() => expect(view.container.querySelectorAll("svg")).toHaveLength(2), {
    timeout: 5000,
  });
  const ids = [...view.container.querySelectorAll("[id]")].map((e) => e.id);
  expect(ids.length).toBeGreaterThan(1);
  expect(new Set(ids).size).toBe(ids.length);
  for (const svg of view.container.querySelectorAll("svg")) {
    const own = new Set([...svg.querySelectorAll("[id]")].map((e) => e.id));
    for (const ref of svg.innerHTML.matchAll(/url\(#([^)]+)\)/g))
      expect(own.has(ref[1])).toBe(true);
  }
});
