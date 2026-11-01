// @vitest-environment happy-dom
import { act, renderHook } from "@testing-library/react";
import { expect, it } from "vitest";
import { useWorkspace } from "@/features/shell/useWorkspace";
import { isRegistered, setAppStateStore, useAppState } from "./appState";
import { createStateStore } from "./state";

const project = (id: string, lastOpenedAt: string) => ({
  id,
  name: id,
  path: `C:\\code\\${id}`,
  defaultBranch: "main",
  addedAt: lastOpenedAt,
  lastOpenedAt,
  threadIds: [],
});

it("follows the state tree, patch by patch, keeping untouched subtrees", () => {
  const store = createStateStore({
    epoch: "e1",
    version: 1,
    value: { projects: { p1: project("p1", "2026-10-30T01:00:00Z") }, threads: {} },
  });
  setAppStateStore(store);
  const threads = renderHook(() => useAppState((v) => v.threads));
  const before = threads.result.current;
  const ws = renderHook(() => useWorkspace());
  expect(ws.result.current.projects.map((p) => p.id)).toEqual(["p1"]);

  // An agent adds a project and opens it: the tree changes, so the UI does.
  act(() => {
    store.receive({
      epoch: "e1",
      version: 2,
      ops: [{ op: "add", path: "/projects/p2", value: project("p2", "2026-10-30T02:00:00Z") }],
      cause: null,
    });
  });
  expect(ws.result.current.projects.map((p) => p.id)).toEqual(["p2", "p1"]);
  expect(threads.result.current).toBe(before);
});

it("knows which commands go through the registry", () => {
  expect(isRegistered("pty.open")).toBe(true);
  expect(isRegistered("no.such.command")).toBe(false);
});
