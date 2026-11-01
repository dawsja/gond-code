import { useEffect, useRef } from "react";
import { FitAddon } from "@xterm/addon-fit";
import { SerializeAddon } from "@xterm/addon-serialize";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { WebglAddon } from "@xterm/addon-webgl";
import { Terminal as XTerm, type ITheme } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { callOrRun, isRegistered, liveBackend, runCommand } from "@/lib/appState";
import { on, openExternal } from "@/lib/ipc";
import { onBeforeRestart } from "@/lib/restart";
import { errorText } from "@/lib/types";
import { imagePasteKey } from "./imagePaste";
import { setPaneFolder } from "./paneFolder";
import { promptFolder } from "./prompt";
import { asks, setStatus, type Status } from "./status";

/**
 * Colours in the spirit of Alacritty and Ghostty: a true black ground, soft but saturated ANSI colours in
 * Gond's tones, brights that stay readable, 24-bit colour passed straight through.
 */
const THEME: ITheme = {
  background: "#000000",
  foreground: "#e6e6e6",
  cursor: "#e6e6e6",
  cursorAccent: "#000000",
  selectionBackground: "#2a3b5c",
  selectionInactiveBackground: "#1d2638",
  black: "#1c1c1c",
  red: "#f28b82",
  green: "#a5d6a7",
  yellow: "#ffd479",
  blue: "#82b1ff",
  magenta: "#d7a6f5",
  cyan: "#80deea",
  white: "#d0d0d0",
  brightBlack: "#6b6b6b",
  brightRed: "#ffaba3",
  brightGreen: "#c5ecc6",
  brightYellow: "#ffe6a6",
  brightBlue: "#b0cbff",
  brightMagenta: "#e8c6fb",
  brightCyan: "#b4f0f7",
  brightWhite: "#ffffff",
};

/** What a pane's keys and menu ask of the layout around it. */
export type Shortcut = "new" | "right" | "down" | "zoom" | "close" | "dictate" | "keys";

/** A running terminal's actions, for its right-click menu. */
export type Handle = {
  copy: () => void;
  paste: () => void;
  selectAll: () => void;
  hasSelection: () => boolean;
  focus: () => void;
  /** Types `text` in as if pasted (dictation): it waits there for you to send it. */
  insert: (text: string) => void;
};

/** Each mounted pane's terminal, by pane id. */
export const handles = new Map<string, Handle>();

const shortcut = (e: KeyboardEvent): Shortcut | undefined => {
  if (e.type !== "keydown" || !e.ctrlKey || e.altKey) return;
  const k = e.key.toLowerCase();
  if (!e.shiftKey && k === "t") return "new";
  if (!e.shiftKey) return;
  if (e.code === "Space") return "dictate";
  if (k === "arrowright") return "right";
  if (k === "arrowdown") return "down";
  if (k === "z") return "zoom";
  if (k === "w") return "close";
  if (k === "p") return "keys";
};

type Props = {
  paneId: string;
  /** Starts the pane's process (pane.start, pty.open) and answers its pty; a shell coming back
   *  after an update starts in the folder it was in (`cwd`). */
  open: (size: { cols: number; rows: number; cwd?: string }) => Promise<{ ptyId: string }>;
  /** An agent's pane watches its screen for Working / Needs you / Done; a shell's doesn't. */
  watch: boolean;
  /** The pane's agent (none for a shell): an image paste goes to it on its own key. */
  agentId?: string;
  active: boolean;
  onFocus: () => void;
  onShortcut: (s: Shortcut) => void;
};

/** One terminal, drawn on the GPU (WebGL), on a pty the Rust side runs; it lives until unmounted. */
export function Terminal({ paneId, open, watch, agentId, active, onFocus, onShortcut }: Props) {
  const box = useRef<HTMLDivElement>(null);
  const term = useRef<XTerm>(null);
  const live = useRef({ open, onShortcut, onFocus, active, watch, agentId });
  useEffect(() => {
    live.current = { open, onShortcut, onFocus, active, watch, agentId };
  });

  useEffect(() => {
    const el = box.current!;
    let disposed = false;
    let ptyId: string | null = null;
    const early: { ptyId: string; data: string }[] = [];
    const t = new XTerm({
      fontFamily: '"JetBrains Mono Variable", "Cascadia Mono", Consolas, monospace',
      fontSize: 13,
      lineHeight: 1.2,
      fontWeight: 400,
      fontWeightBold: 650,
      cursorBlink: true,
      cursorStyle: "block",
      cursorInactiveStyle: "outline",
      drawBoldTextInBrightColors: false,
      minimumContrastRatio: 1,
      scrollback: 10000,
      allowProposedApi: true,
      theme: THEME,
    });
    const fit = new FitAddon();
    t.loadAddon(fit);
    const serialize = new SerializeAddon();
    t.loadAddon(serialize);
    t.loadAddon(new Unicode11Addon());
    t.unicode.activeVersion = "11";
    t.loadAddon(new WebLinksAddon((_, uri) => void openExternal(uri)));
    term.current = t;

    // Text is pasted (bracketed, when the program asks); a picture the agent reads off the
    // clipboard itself, on its image-paste key. The app reads the clipboard, not the WebView,
    // which would ask permission first.
    const clipboard = (): Promise<{ text?: string; image: boolean }> =>
      liveBackend() && isRegistered("clipboard.read")
        ? runCommand("clipboard.read", {})
        : (navigator.clipboard?.readText() ?? Promise.resolve("")).then((text) => ({
            text,
            image: !text,
          }));
    const paste = () =>
      void clipboard().then(
        ({ text, image }) => {
          if (text) return t.paste(text);
          const key = image && imagePasteKey(live.current.agentId);
          if (key) t.input(key);
        },
        () => undefined,
      );

    // Gond's keys leave the terminal; Ctrl+C copies when there's a selection, else it's the
    // program's (stop, cancel). Everything else is the agent's.
    t.attachCustomKeyEventHandler((e) => {
      // Ctrl+Shift+C copies and Ctrl+Shift+V pastes, as in other terminals; the program gets
      // neither key.
      if (e.ctrlKey && e.shiftKey && !e.altKey && ["c", "v"].includes(e.key.toLowerCase())) {
        if (e.type === "keydown") {
          e.preventDefault();
          if (e.key.toLowerCase() === "c") {
            if (t.hasSelection()) void navigator.clipboard?.writeText(t.getSelection());
          } else {
            paste();
          }
        }
        return false;
      }
      const s = shortcut(e);
      if (s) {
        e.preventDefault();
        live.current.onShortcut(s);
        return false;
      }
      if (e.type === "keydown" && e.ctrlKey && !e.shiftKey && e.key.toLowerCase() === "c") {
        if (t.hasSelection()) {
          void navigator.clipboard?.writeText(t.getSelection());
          t.clearSelection();
          return false;
        }
      }
      return true;
    });

    handles.set(paneId, {
      copy: () => void navigator.clipboard?.writeText(t.getSelection()),
      paste,
      selectAll: () => t.selectAll(),
      hasSelection: () => t.hasSelection(),
      focus: () => t.focus(),
      insert: (text) => t.paste(text),
    });

    // What the agent is doing, from its output: still printing (not just echoing your keys) is
    // working; a question on screen is needs-you; stopping while you're elsewhere is done.
    let lastOut = 0;
    let lastKey = 0;
    let busySince = 0;
    let state: Status = "idle";
    const set = (s: Status) => {
      state = s;
      setStatus(paneId, live.current.watch ? s : undefined);
    };
    const lines = () => {
      const b = t.buffer.active;
      const out: string[] = [];
      for (let y = Math.max(0, b.baseY + t.rows - 14); y < b.baseY + t.rows; y++)
        out.push(b.getLine(y)?.translateToString(true) ?? "");
      return out;
    };
    const screen = () => lines().join("\n");

    // Before a restart into an update, a shell keeps its screen (colours and scrollback) and the
    // folder its prompt shows, to come back as it was. An agent restores its own conversation.
    const shellsOnly = () =>
      !live.current.watch && liveBackend() && isRegistered("pane.screen.save");
    const stopSaving = onBeforeRestart(async () => {
      if (!ptyId || !shellsOnly()) return;
      await runCommand("pane.screen.save", {
        paneId,
        screen: serialize.serialize({ scrollback: 2000 }),
        cwd: promptFolder(lines()),
      });
    });
    const tick = window.setInterval(() => {
      if (state === "exited") return;
      // A shell can cd anywhere: the status bar follows the folder its prompt shows.
      if (!live.current.watch) {
        const folder = promptFolder(lines());
        if (folder) setPaneFolder(paneId, folder);
      }
      const now = Date.now();
      // A question on screen is the agent waiting for you, even while it redraws (Codex's
      // folder-trust prompt animates).
      if (asks(screen())) {
        busySince = 0;
        return set("needs");
      }
      const printing = now - lastOut < 1500;
      if (printing) {
        if (!busySince) busySince = now;
        if (state !== "working" && now - busySince > 700) set("working");
        return;
      }
      const wasBusy = busySince && now - busySince > 2500;
      busySince = 0;
      if (state === "working" && wasBusy && !live.current.active) return set("done");
      if (state === "done" && !live.current.active) return;
      set("idle");
    }, 500);

    const stopData = on("pty:data", ({ payload }) => {
      if (ptyId === null) return void early.push(payload);
      if (payload.ptyId !== ptyId) return;
      t.write(payload.data);
      // Your own typing comes back as output: that isn't the agent working.
      if (Date.now() - lastKey > 250) lastOut = Date.now();
    });
    const stopExit = on("pty:exit", ({ payload }) => {
      if (payload.ptyId !== ptyId) return;
      set("exited");
      t.write("\r\n\x1b[2m[The process ended. Restart it from the pane's menu.]\x1b[0m\r\n");
    });

    const resize = new ResizeObserver(() => {
      if (!el.clientWidth || !el.clientHeight) return;
      fit.fit();
      if (ptyId)
        callOrRun("pty_resize", "pty.resize", { ptyId, cols: t.cols, rows: t.rows }).catch(
          () => undefined,
        );
    });

    // Draws once the terminal font is in (or straight away where fonts can't be waited on).
    const font = document.fonts?.load('13px "JetBrains Mono Variable"') ?? Promise.resolve();
    font.finally(async () => {
      if (disposed) return;
      t.open(el);
      // The GPU renderer, as Alacritty draws: a glyph atlas on the GPU. If the GPU context is
      // lost it falls back to the DOM renderer rather than going blank.
      try {
        const gl = new WebglAddon();
        gl.onContextLoss(() => gl.dispose());
        t.loadAddon(gl);
      } catch {
        // No WebGL here: the DOM renderer draws it.
      }
      fit.fit();
      resize.observe(el);
      t.textarea?.addEventListener("focus", () => live.current.onFocus());
      // xterm's hidden input, named for agents and the legibility audit.
      t.textarea?.setAttribute("data-local", "pane.input");
      t.onData((data) => {
        lastKey = Date.now();
        if (state === "done" || state === "needs") set("idle");
        if (ptyId) callOrRun("pty_write", "pty.write", { ptyId, data }).catch(() => undefined);
      });
      try {
        const back = shellsOnly()
          ? await runCommand<{ screen: string; cwd?: string } | null>("pane.screen.take", {
              paneId,
            }).catch(() => null)
          : null;
        if (back) t.write(`${back.screen}\r\n\x1b[2m[Restored after the update]\x1b[0m\r\n`);
        ({ ptyId } = await live.current.open({ cols: t.cols, rows: t.rows, cwd: back?.cwd }));
        if (disposed) return void callOrRun("pty_close", "pty.close", { ptyId });
        early.filter((e) => e.ptyId === ptyId).forEach((e) => t.write(e.data));
        if (live.current.active) t.focus();
      } catch (e) {
        t.write(`\x1b[31mCouldn't start: ${errorText(e)}\x1b[0m\r\n`);
        set("exited");
      }
    });

    return () => {
      disposed = true;
      stopSaving();
      window.clearInterval(tick);
      setPaneFolder(paneId, undefined);
      resize.disconnect();
      stopData.then((stop) => stop());
      stopExit.then((stop) => stop());
      handles.delete(paneId);
      setStatus(paneId, undefined);
      if (ptyId) callOrRun("pty_close", "pty.close", { ptyId }).catch(() => undefined);
      t.dispose();
    };
  }, [paneId]);

  useEffect(() => {
    if (active) term.current?.focus();
  }, [active]);

  // The padding sits outside the box xterm fits to: its fit reads the box's height and width,
  // which would include padding and size the terminal a row and a few columns too big.
  return (
    <div
      data-selectable
      onMouseDown={() => live.current.onFocus()}
      className="flex min-h-0 min-w-0 flex-1 overflow-hidden bg-black py-1.5 pl-3"
    >
      <div ref={box} className="min-h-0 min-w-0 flex-1 overflow-hidden" />
    </div>
  );
}
