import { describe, expect, it } from "vitest";
import {
  fromSaved,
  moveBeside,
  one,
  panes,
  remove,
  resize,
  split,
  swap,
  type Node,
  type Pane,
} from "./layout";

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

  it("drags a pane onto another: swapped in the middle, beside it at an edge", () => {
    // Claude | (Codex over Grok)
    let root: Node | null = split(pane("claude"), "claude", "right", pane("codex"));
    root = split(root, "codex", "down", pane("grok"));
    const swapped = swap(root, "claude", "grok");
    expect(swapped).toMatchObject({
      a: { id: "grok" },
      b: { a: { id: "codex" }, b: { id: "claude" } },
    });
    expect(swap(root, "claude", "claude")).toBe(root);

    // Grok dropped on Claude's left edge: Grok | Claude on the left, Codex alone on the right.
    const moved = moveBeside(root, "grok", "claude", "left");
    expect(moved).toMatchObject({
      dir: "right",
      a: { dir: "right", a: { id: "grok" }, b: { id: "claude" } },
      b: { id: "codex" },
    });
    // Dropped on its own: nothing moves.
    expect(moveBeside(root, "grok", "grok", "up")).toBe(root);
    // Below the only other pane.
    const two = split(pane("a"), "a", "right", pane("b"));
    expect(moveBeside(two, "a", "b", "down")).toMatchObject({
      dir: "down",
      a: { id: "b" },
      b: { id: "a" },
    });
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

  it("opens a pane saved when it ran an agent as a shell in its folder", () => {
    const saved = fromSaved({
      sessions: [
        {
          id: "s",
          name: "Main",
          layout: {
            root: {
              ...split(pane("a"), "a", "right", pane("b")),
              a: {
                ...pane("a"),
                threadId: "t1",
                agentId: "claude",
                detail: "Opus 5.5",
                title: "Fix",
              },
            },
          },
        },
      ],
    });
    expect(panes(saved.sessions[0].layout.root)).toEqual([
      { ...pane("a"), title: "Fix" },
      pane("b"),
    ]);
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
    expect(
      asks(
        "Accessing workspace…\n❯ No, exit\n  Yes, I trust this folder\nEnter to confirm · Esc to cancel",
      ),
    ).toBe(true);
  });

  it("isn't the agent's own reply, idle at its input", async () => {
    const { asks } = await import("./status");
    const idle =
      "● Would you like to approve the next step?\n\n✻ Churned for 2s\n────\n❯ \n────\n⏵⏵ auto mode on";
    expect(asks(idle)).toBe(false);
  });

  it("isn't a choice in the conversation above Claude Code's waiting input box", async () => {
    const { atInput } = await import("./status");
    const rule = "─".repeat(40);
    const reply = ["● ok", "  ❯ 1. Yes [y/n]", "", rule, "❯ ", rule, "  ⏸ manual mode on"];
    expect(atInput(reply)).toBe(true);
    // A permission prompt replaces the box, with its own footer.
    const asking = [
      rule,
      " Do you want to create see.txt?",
      " ❯ 1. Yes",
      "   3. No",
      "",
      " Esc to cancel",
    ];
    expect(atInput(asking)).toBe(false);
    expect(atInput([...reply, "Enter to confirm · Esc to cancel"])).toBe(false);
  });

  it("is read first from the title Claude Code and Codex set", async () => {
    const { fromTitle } = await import("./status");
    expect(fromTitle("claude", "◐ Bye.txt file creation")).toBe("working");
    expect(fromTitle("claude", "⠋ Fixing tests")).toBe("working");
    expect(fromTitle("claude", "✳ Bye.txt file creation")).toBe("idle");
    expect(fromTitle("claude", "Claude Code")).toBeUndefined();
    expect(fromTitle("codex", "⠹ gond-code")).toBe("working");
    expect(fromTitle("codex", "Action Required · gond-code")).toBe("needs");
    expect(fromTitle("codex", "gond-code")).toBe("idle");
    expect(fromTitle("opencode", "⠋ busy")).toBeUndefined();
  });

  it("isn't moved by the terminal's own focus and cursor reports", async () => {
    const { typed } = await import("./status");
    for (const report of [
      "\x1b[I",
      "\x1b[O",
      "\x1b[O\x1b[I",
      "\x1b[12;40R",
      "\x1b[?1;2c",
      "\x1b]11;rgb:0000/0000/0000\x1b\\",
    ])
      expect(typed(report)).toBe(false);
    for (const key of ["y", "\r", "\x1b", "\x1b[A", "1", "hello"]) expect(typed(key)).toBe(true);
  });
});
