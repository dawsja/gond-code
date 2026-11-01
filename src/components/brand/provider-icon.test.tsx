// @vitest-environment happy-dom
import { cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { ProviderIcon } from "./provider-icon";

afterEach(cleanup);

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
