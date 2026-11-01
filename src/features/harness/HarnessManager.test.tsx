// @vitest-environment happy-dom
// dev.1492: Install in Settings → Harnesses sent harness.install { harness, when } and stuck on
// "Updates after their current step". It now uses first run's job path and the real inputs.
import { useState } from "react";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { check } from "../../../e2e/legibility/legibility";
import type { Harness } from "@/lib/harnesses";

const runCommand = vi.fn();
vi.mock("@/lib/appState", () => ({
  runCommand: (name: string, input: unknown) => runCommand(name, input),
  useAppState: (select: (t: object) => unknown) => {
    useState(0);
    return select({});
  },
  liveBackend: () => true,
  isRegistered: () => true,
}));
afterEach(() => {
  cleanup();
  runCommand.mockReset();
});

const claude: Harness = {
  id: "claude",
  label: "Claude Code",
  brand: "claude",
  installed: false,
};

async function show(onCommand = vi.fn()) {
  const { HarnessManager } = await import("./HarnessManager");
  const { useHarnessInstalls } = await import("./useHarnessInstalls");
  function Host() {
    const installs = useHarnessInstalls(() => {});
    return <HarnessManager harnesses={[claude]} onCommand={onCommand} installs={installs} />;
  }
  render(<Host />);
  return onCommand;
}

it("Install on a missing harness runs harness.install { harnessId } at once, no queue", async () => {
  runCommand.mockResolvedValue({ jobId: "job_1" });
  await show();
  const card = screen.getByRole("listitem");
  await act(async () => fireEvent.click(within(card).getByRole("button", { name: /Install/ })));
  expect(runCommand).toHaveBeenCalledWith("harness.install", { harnessId: "claude" });
  expect(card.textContent).not.toMatch(/current step/);
  expect(check(card)).toEqual([]);
});

it("a refused install resets the card and shows the error with Retry", async () => {
  runCommand.mockRejectedValue({ code: "invalid", message: "winget isn't available." });
  await show();
  const card = screen.getByRole("listitem");
  await act(async () => fireEvent.click(within(card).getByRole("button", { name: /Install/ })));
  expect(within(card).getByRole("alert").textContent).toContain("winget isn't available.");
  expect(within(card).getByRole("button", { name: "Retry" })).toHaveProperty("disabled", false);
  expect(card.textContent).not.toMatch(/current step/);
});

it("Check all sends harness.check with no input", async () => {
  const onCommand = await show();
  fireEvent.click(screen.getByRole("button", { name: /Check all/ }));
  expect(onCommand).toHaveBeenCalledWith("harness.check", {});
});

it("an installed card's footer: Check on the left, Uninstall last, and a ⋯ for narrow cards", async () => {
  const { HarnessManager } = await import("./HarnessManager");
  render(
    <HarnessManager
      harnesses={[{ ...claude, installed: true, version: "2.1.302", latest: "2.1.302" }]}

      onCommand={vi.fn()}
    />,
  );
  const footer = screen.getByRole("listitem").querySelector("footer")!;
  const names = within(footer)
    .getAllByRole("button")
    .map((b) => b.getAttribute("aria-label") ?? b.textContent?.trim());
  expect(names).toEqual(["Check", "Uninstall", "More for Claude Code"]);
  expect(check(screen.getByRole("listitem"))).toEqual([]);
});
