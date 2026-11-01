import { beforeEach, expect, it, vi } from "vitest";

const invoke = vi.fn(async () => ({ sha: "abc1234" }));
const call = vi.fn(async () => ({ sha: "mock" }));
let mock = false;
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("./ipc", () => ({ call, mockBackend: () => mock }));

beforeEach(() => {
  invoke.mockClear();
  call.mockClear();
});

it("runs a registered command through command_run in the app", async () => {
  mock = false;
  const { callOrRun } = await import("./appState");
  await callOrRun("pty_close", "pty.close", { ptyId: "t1" });
  expect(invoke).toHaveBeenCalledWith("command_run", {
    name: "pty.close",
    input: { ptyId: "t1" },
  });
  expect(call).not.toHaveBeenCalled();
});

it("keeps the old IPC on the browser mock", async () => {
  mock = true;
  const { callOrRun } = await import("./appState");
  await callOrRun("pty_close", "pty.close", { ptyId: "t1" });
  expect(call).toHaveBeenCalledWith("pty_close", { ptyId: "t1" });
  expect(invoke).not.toHaveBeenCalled();
});
