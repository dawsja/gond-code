// @vitest-environment happy-dom
// an agent's Install went through harness.install and then showed nothing. This drives
// Settings → Agents through the same command path (runCommand → /installs/{jobId}) and checks every state the card shows.
import { useState } from "react";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import type { Harness } from "@/lib/harnesses";

const tree: { installs?: Record<string, unknown> } = {};
let listeners: (() => void)[] = [];
const runCommand = vi.fn();
vi.mock("@/lib/appState", () => ({
  runCommand: (name: string, input: unknown) => runCommand(name, input),
  useAppState: (select: (t: typeof tree) => unknown) => {
    const [, bump] = useState(0);
    listeners.push(() => bump((n) => n + 1));
    return select(tree);
  },
  liveBackend: () => true,
  isRegistered: () => true,
}));
const setTree = (installs: Record<string, unknown>) =>
  act(() => {
    tree.installs = installs;
    listeners.forEach((l) => l());
  });

afterEach(() => {
  cleanup();
  listeners = [];
  tree.installs = undefined;
  runCommand.mockReset();
});

const codex: Harness = {
  id: "codex",
  label: "Codex",
  brand: "codex",
  installed: false,
};

async function show(onDone = vi.fn(), harness: Harness = codex) {
  const { HarnessManager } = await import("./HarnessManager");
  const { useHarnessInstalls } = await import("./useHarnessInstalls");
  function Host() {
    const installs = useHarnessInstalls(onDone);
    return <HarnessManager harnesses={[harness]} onCommand={vi.fn()} installs={installs} />;
  }
  render(<Host />);
  return onDone;
}

it("Install starts harness.install, follows its job to done, then re-detects the agents", async () => {
  runCommand.mockResolvedValue({ jobId: "job_1", steps: [] });
  const onDone = await show();
  fireEvent.click(screen.getByRole("button", { name: /Install/ }));
  expect(runCommand).toHaveBeenCalledWith("harness.install", { harnessId: "codex" });
  expect(await screen.findByText("Starting Codex…")).toBeTruthy();
  await waitFor(() => expect(runCommand).toHaveBeenCalledTimes(1));
  await setTree({
    job_1: {
      id: "job_1",
      title: "Install Codex",
      status: "running",
      percent: 40,
      steps: [{ label: "Installing Codex", status: "running" }],
    },
  });
  expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("40");
  await setTree({
    job_1: {
      id: "job_1",
      title: "Install Codex",
      status: "done",
      steps: [{ label: "Installing Codex", status: "done" }],
    },
  });
  expect(screen.getByText("Codex installed. Checking…")).toBeTruthy();
  expect(onDone).toHaveBeenCalledTimes(1);
});

it("a refused call or a failed job is one sentence with Retry", async () => {
  runCommand.mockRejectedValueOnce({
    code: "invalid",
    message: "Install Codex needs winget, which isn't installed.",
  });
  await show();
  fireEvent.click(screen.getByRole("button", { name: /Install/ }));
  expect((await screen.findByRole("alert")).textContent).toContain("needs winget");
  runCommand.mockResolvedValueOnce({ jobId: "job_2", steps: [] });
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  await waitFor(() => expect(runCommand).toHaveBeenCalledTimes(2));
  await setTree({
    job_2: {
      id: "job_2",
      title: "Install Codex",
      status: "failed",
      error: "The download was interrupted.",
      steps: [{ label: "Installing Codex", status: "failed" }],
    },
  });
  expect(screen.getByRole("alert").textContent).toContain("The download was interrupted.");
});

it("Uninstall asks with the plan, then follows harness.uninstall's job back to Not installed", async () => {
  runCommand.mockImplementation(async (name: string, input: { keepData?: boolean }) =>
    name === "harness.uninstallPlan"
      ? {
          harnessId: "codex",
          methods: ["npm"],
          deletes: ["C:\\Users\\d\\AppData\\Roaming\\npm\\codex.cmd"],
          pathEntries: [],
          keepData: input.keepData ?? true,
          running: 0,
        }
      : { jobId: "job_u1" },
  );
  const onDone = await show(vi.fn(), { ...codex, installed: true, version: "0.9.1" });
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Uninstall" })));
  const ask = screen.getByRole("dialog", { name: /Uninstall Codex\?/ });
  expect(ask.textContent).toContain("codex.cmd");
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Uninstall" })));
  expect(runCommand).toHaveBeenLastCalledWith("harness.uninstall", {
    harnessId: "codex",
    keepData: true,
  });
  setTree({
    job_u1: {
      id: "job_u1",
      title: "Uninstall Codex",
      status: "running",
      steps: [{ label: "Uninstalling Codex", status: "running" }],
    },
  });
  expect(screen.getByText("Uninstalling Codex")).toBeTruthy();
  setTree({
    job_u1: {
      id: "job_u1",
      title: "Uninstall Codex",
      status: "done",
      steps: [{ label: "Uninstalling Codex", status: "done", note: "Codex is uninstalled." }],
    },
  });
  await waitFor(() => expect(onDone).toHaveBeenCalled());
});
