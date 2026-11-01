// @vitest-environment happy-dom
import { act, renderHook, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";

const tree: { installs?: Record<string, unknown> } = {};
const runCommand = vi.fn(async (...[name]: [string, unknown?]) => {
  if (name === "prereq.list")
    return [
      { id: "git", name: "Git", installed: false },
      { id: "gh", name: "GitHub CLI", installed: true, version: "2.62.0" },
      { id: "node", name: "Node.js LTS", installed: false },
    ];
  if (name === "git.identity") return { current: {}, needed: true };
  if (name === "github.status") return { ghInstalled: true, accounts: [] };
  if (name === "prereq.install") return { jobId: "job_9" };
  throw new Error(name);
});
vi.mock("@/lib/appState", () => ({
  liveBackend: () => true,
  runCommand: (name: string, input: unknown) => runCommand(name, input),
  useAppState: (select: (t: typeof tree) => unknown) => select(tree),
}));

it("reads prereq.list live, keeps what first run needs, and follows the job it started", async () => {
  const { useSetup } = await import("./useSetup");
  const { result, rerender } = renderHook(() => useSetup());
  await waitFor(() => expect(result.current.loaded).toBe(true));
  expect(result.current.prereqs?.map((p) => p.id)).toEqual(["git", "gh"]);
  expect(result.current.missing).toBe(true);
  expect(result.current.identity?.needed).toBe(true);

  await act(() => result.current.install({ prereqs: ["git"] }));
  expect(runCommand).toHaveBeenCalledWith("prereq.install", { prereqs: ["git"] });
  tree.installs = {
    job_9: { id: "job_9", title: "Set up Git", status: "running", steps: [], percent: 40 },
  };
  rerender();
  expect(result.current.job?.percent).toBe(40);

  // When it ends, prereq.list is read again, so the card turns Ready by itself.
  const before = runCommand.mock.calls.filter(([n]) => n === "prereq.list").length;
  tree.installs = { job_9: { ...(tree.installs.job_9 as object), status: "done" } };
  rerender();
  await waitFor(() =>
    expect(runCommand.mock.calls.filter(([n]) => n === "prereq.list").length).toBe(before + 1),
  );
});
