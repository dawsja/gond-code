// @vitest-environment happy-dom
import { act, renderHook } from "@testing-library/react";
import { beforeEach, expect, it } from "vitest";
import type { Pane } from "./layout";
import { useSessions } from "./useSessions";
import { setPaneFolder } from "./paneFolder";
import { mockCall } from "@/lib/mock";

beforeEach(() => localStorage.clear());

// Closing a session's last terminal gave you a terminal in Settings' shell (Git Bash), not the
// one you closed.
it("reopens the same shell when a session's last terminal is closed", () => {
  const pane: Pane = { kind: "pane", id: "p", projectId: "", shell: "powershell" };
  localStorage.setItem(
    "gond.sessions",
    JSON.stringify({
      sessions: [{ id: "s", name: "Main", layout: { root: pane, focus: "p" } }],
      current: "s",
    }),
  );
  const { result } = renderHook(() => useSessions({ projects: [] }));

  act(() => result.current.actions.shortcut("p", "close"));

  const root = result.current.sessions[0].layout.root as Pane;
  expect(root.kind).toBe("pane");
  expect(root.id).not.toBe("p");
  expect(root.shell).toBe("powershell");
});

// Splitting opened the New agent picker; it opens a terminal in the default shell, in the same
// folder, beside the pane.
it("splits a pane into a new terminal in the default shell", () => {
  const pane: Pane = { kind: "pane", id: "p", projectId: "proj", shell: "gitbash" };
  localStorage.setItem(
    "gond.sessions",
    JSON.stringify({
      sessions: [{ id: "s", name: "Main", layout: { root: pane, focus: "p" } }],
      current: "s",
    }),
  );
  const { result } = renderHook(() => useSessions({ projects: [] }));

  act(() => result.current.actions.shortcut("p", "down"));

  const layout = result.current.sessions[0].layout;
  expect(layout.root).toMatchObject({ kind: "split", dir: "down", a: { id: "p" } });
  const added = layout.root?.kind === "split" ? layout.root.b : undefined;
  expect(added).toEqual({ kind: "pane", id: layout.focus, projectId: "proj" });
});

it("opens a projectless pane through the mock and guards stale project ids on reopen", async () => {
  const pane: Pane = { kind: "pane", id: "plain", projectId: "removed" };
  localStorage.setItem(
    "gond.sessions",
    JSON.stringify({
      sessions: [{ id: "s", name: "Main", layout: { root: pane, focus: pane.id } }],
      current: "s",
    }),
  );
  const { result } = renderHook(() => useSessions({ projects: [] }));
  setPaneFolder(pane.id, "C:\\plain");
  await act(async () => {
    expect(await result.current.actions.openFolder(pane.id)).toBeNull();
  });
  const opened = await result.current.opener(pane)({ cols: 80, rows: 24 });
  expect(opened.ptyId).toBeTruthy();
  await mockCall("pty_close", { ptyId: opened.ptyId });
  setPaneFolder(pane.id, undefined);
});
