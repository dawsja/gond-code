// @vitest-environment happy-dom
// Windows' own dark title bar (it carries the Gond Code name), then the app:
// the sidebar and the agents' panes, no header or rails of the app's own.
import { cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { Deck } from "@/features/deck/Deck";

afterEach(async () => {
  // Brand icons load on first use; let them land before the test environment goes.
  await import("@/components/brand/svgs");
  cleanup();
  window.history.replaceState(null, "", "/");
});

it("the app is the sidebar and the panes: no header, rail or Deck of its own", async () => {
  const view = render(<Deck />);
  await waitFor(() =>
    expect(
      view.container.querySelector('[data-entity="sidebar"] [data-entity="session"]'),
    ).not.toBeNull(),
  );
  expect(view.container.querySelector('[data-entity="workspace"]')).not.toBeNull();
  for (const gone of ["titlebar", "nav-rail", "crew-row"]) {
    expect(view.container.querySelector(`[data-entity="${gone}"]`)).toBeNull();
  }
  expect(view.container.querySelector("[data-tauri-drag-region]")).toBeNull();
});
