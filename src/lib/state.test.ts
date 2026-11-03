import { describe, expect, it, vi } from "vitest";
import { applyOps, connectState, createStateStore, type Patch } from "./state";

const invoke = vi.fn();
const listen = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  isTauri: () => true,
  invoke: (...args: unknown[]) => invoke(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: (...args: unknown[]) => listen(...args) }));

describe("applyOps", () => {
  it("matches the Rust patches and shares untouched subtrees", () => {
    const doc = {
      threads: { a: { status: "running" } },
      settings: { tint: "blue" },
      x: { "a/b": 1 },
    };
    const out = applyOps(doc, [
      { op: "replace", path: "/threads/a/status", value: "waiting" },
      { op: "add", path: "/threads/b", value: { status: "starting" } },
      { op: "remove", path: "/x/a~1b" },
    ]);
    expect(out).toEqual({
      threads: { a: { status: "waiting" }, b: { status: "starting" } },
      settings: { tint: "blue" },
      x: {},
    });
    expect(out.settings).toBe(doc.settings);
    expect(doc.threads.a.status).toBe("running");
  });
});

describe("createStateStore", () => {
  it("applies patches in order and asks for a resync on a gap or new epoch", () => {
    const s = createStateStore({ epoch: "e1", version: 0, value: { n: 0 } });
    let calls = 0;
    s.subscribe(() => calls++);
    expect(
      s.receive({
        epoch: "e1",
        version: 1,
        ops: [{ op: "replace", path: "/n", value: 1 }],
        cause: {},
      }),
    ).toBe(true);
    expect(s.get()).toEqual({ epoch: "e1", version: 1, value: { n: 1 } });
    expect(s.receive({ epoch: "e1", version: 1, ops: [], cause: {} })).toBe(true); // duplicate: ignored
    expect(s.receive({ epoch: "e1", version: 3, ops: [], cause: {} })).toBe(false);
    expect(s.receive({ epoch: "e2", version: 1, ops: [], cause: {} })).toBe(false);
    expect(calls).toBe(1);
  });
});

it("does not lose a saved theme patch while the initial snapshot is in flight", async () => {
  let emit: (event: { payload: Patch }) => void;
  listen.mockImplementationOnce(async (_name, callback) => {
    emit = callback;
    return () => {};
  });
  invoke.mockImplementationOnce(async () => {
    // This snapshot was taken before settings.set, but its reply reaches the UI later.
    emit({
      payload: {
        epoch: "theme",
        version: 1,
        cause: {},
        ops: [{ op: "replace", path: "/settings/theme", value: "light" }],
      },
    });
    return { epoch: "theme", version: 0, value: { settings: { theme: "dark" } } };
  });
  const store = await connectState();
  expect(store.get().value.settings).toEqual({ theme: "light" });
  expect(store.get().version).toBe(1);
});
