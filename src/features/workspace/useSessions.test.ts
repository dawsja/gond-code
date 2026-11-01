// @vitest-environment happy-dom
import { act, renderHook } from "@testing-library/react";
import { beforeEach, expect, it } from "vitest";
import type { Pane } from "./layout";
import { useSessions } from "./useSessions";

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
  const { result } = renderHook(() =>
    useSessions({ agents: [], models: {}, accounts: [], projects: [] }),
  );

  act(() => result.current.actions.shortcut("p", "close"));

  const root = result.current.sessions[0].layout.root as Pane;
  expect(root.kind).toBe("pane");
  expect(root.id).not.toBe("p");
  expect(root.shell).toBe("powershell");
  expect(root.threadId).toBeUndefined();
});
