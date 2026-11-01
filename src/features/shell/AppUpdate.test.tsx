// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";

const run = vi.hoisted(() => vi.fn(() => Promise.resolve(null)));
vi.mock("@/lib/appState", async (orig) => ({
  ...(await orig<typeof import("@/lib/appState")>()),
  runCommand: run,
}));

afterEach(cleanup);

// A newer Gond Code was found but nothing showed it.
it("shows a newer Gond Code above Settings, and opens Settings → Updates", async () => {
  const { setAppStateStore } = await import("@/lib/appState");
  const { createStateStore } = await import("@/lib/state");
  const { AppUpdate } = await import("./AppUpdate");
  setAppStateStore(createStateStore({ epoch: "e", version: 1, value: { update: null } }));
  const open = vi.fn();
  const { container } = render(<AppUpdate onOpen={open} />);
  expect(container.textContent).toBe("");
  cleanup();
  setAppStateStore(
    createStateStore({
      epoch: "e",
      version: 2,
      value: { update: { current: "0.1.0", phase: "idle", available: { version: "0.1.1" } } },
    }),
  );
  render(<AppUpdate onOpen={open} />);
  fireEvent.click(screen.getByRole("button", { name: /Gond Code 0\.1\.1/ }));
  expect(open).toHaveBeenCalled();
  expect(run).not.toHaveBeenCalled();
});

// Restart to update must save the sessions and the terminals' screens before the app goes.
it("saves everything before it installs the update", async () => {
  const { onBeforeRestart } = await import("@/lib/restart");
  const { restartToUpdate } = await import("@/features/settings/UpdateSettings");
  const order: string[] = [];
  const stop = onBeforeRestart(async () => {
    await new Promise((done) => setTimeout(done, 20));
    order.push("saved");
  });
  run.mockImplementation(async () => {
    order.push("install");
    return null;
  });
  await restartToUpdate();
  stop();
  expect(order).toEqual(["saved", "install"]);
  expect(run).toHaveBeenCalledWith("app.update.install", {});
});
