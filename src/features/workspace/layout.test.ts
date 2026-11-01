import { describe, expect, it } from "vitest";
import { one, panes, remove, resize, split, type Node, type Pane } from "./layout";

const pane = (id: string): Pane => ({ kind: "pane", id, projectId: "p" });

describe("a folder's panes", () => {
  it("split right and down beside the pane you're in, and close back to the sibling", () => {
    let root: Node | null = split(null, undefined, "right", pane("claude"));
    expect(root).toEqual(pane("claude"));
    root = split(root, "claude", "right", pane("codex"));
    root = split(root, "codex", "down", pane("grok"));
    // Claude on the left; Codex above Grok on the right.
    expect(panes(root).map((p) => p.id)).toEqual(["claude", "codex", "grok"]);
    expect(root).toMatchObject({ dir: "right", a: { id: "claude" }, b: { dir: "down" } });

    root = remove(root, "codex");
    expect(root).toMatchObject({ dir: "right", a: { id: "claude" }, b: { id: "grok" } });
    root = remove(root, "claude");
    expect(root).toEqual(pane("grok"));
    expect(remove(root, "grok")).toBeNull();
  });

  it("opens a layout saved per folder as one workspace, nothing missing", () => {
    const merged = one({
      a: { root: pane("claude") },
      b: { root: null },
      c: { root: pane("codex") },
    });
    expect(panes(merged.root).map((p) => p.id)).toEqual(["claude", "codex"]);
    expect(one({ all: { root: pane("grok"), focus: "grok" } })).toEqual({
      root: pane("grok"),
      focus: "grok",
    });
  });

  it("keeps a resized split from squeezing either side away", () => {
    const root = split(pane("a"), "a", "right", pane("b"));
    const id = root.kind === "split" ? root.id : "";
    expect(resize(root, id, 0.99)).toMatchObject({ ratio: 0.85 });
    expect(resize(root, id, 0.01)).toMatchObject({ ratio: 0.15 });
    expect(resize(root, id, 0.4)).toMatchObject({ ratio: 0.4 });
  });
});

describe("an agent waiting on you", () => {
  it("is seen in its own prompts: approvals, yes/no, folder trust", async () => {
    const { asks } = await import("./status");
    expect(asks("Do you want to proceed?\n❯ 1. Yes")).toBe(true);
    expect(asks("Trust this folder? Codex can read, edit…\n› 1. Trust and continue")).toBe(true);
    expect(asks("Allow edit k8s/web.yaml?  [y] yes  [n] no")).toBe(true);
    expect(asks("✻ Animating… (38s · esc to interrupt)")).toBe(false);
  });
});
