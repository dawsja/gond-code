import { useEffect, useRef } from "react";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal, type ITheme } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { on } from "@/lib/ipc";
import { callOrRun, runCommand } from "@/lib/appState";
import { errorText } from "@/lib/types";

/** The terminal's colours, read from the theme so they follow the tint and stay in palette. */
function themeFromCss(): ITheme {
  const css = getComputedStyle(document.documentElement);
  const v = (name: string) => css.getPropertyValue(name).trim();
  return {
    background: v("--background"),
    foreground: v("--foreground"),
    cursor: v("--foreground"),
    cursorAccent: v("--background"),
    selectionBackground: v("--tint-wash"),
    black: v("--secondary"),
    red: v("--ember"),
    green: v("--mint"),
    yellow: v("--amber"),
    blue: v("--sky"),
    magenta: v("--magenta"),
    cyan: v("--grape"),
    white: v("--muted-foreground"),
    brightBlack: v("--ash"),
    brightWhite: v("--foreground"),
  };
}

type Props = {
  /** The worktree shell's thread (pty.open). */
  threadId?: string;
  /** Or another command that starts a pty and answers its id, e.g. account.signIn. */
  open?: (size: { cols: number; rows: number }) => Promise<{ ptyId: string }>;
  /** Or a pty that already runs (an agent's, a background sign-in): shown with its scrollback
   *  (pty.read), and never closed or resized from here, since it isn't ours. */
  attach?: string;
  /** An attached pty this pane owns after all (agy's sign-in): sized to fit the pane. */
  fitAttached?: boolean;
  /** Watch only: keystrokes go nowhere (an agent's terminal until you take it over). */
  readOnly?: boolean;
  active: boolean;
  onFocus: () => void;
  onExit: () => void;
};

/** One xterm.js terminal on its own pty (pty_*), fitted to its box. */
export default function XtermPane({
  threadId,
  open,
  attach,
  fitAttached,
  readOnly,
  active,
  onFocus,
  onExit,
}: Props) {
  const box = useRef<HTMLDivElement>(null);
  const term = useRef<Terminal>(null);
  const exitRef = useRef(onExit);
  const openRef = useRef(open);
  const readOnlyRef = useRef(readOnly);
  useEffect(() => {
    exitRef.current = onExit;
    openRef.current = open;
    readOnlyRef.current = readOnly;
    if (term.current) term.current.options.disableStdin = !!readOnly;
  });

  useEffect(() => {
    const el = box.current!;
    let disposed = false;
    let ptyId: string | null = null;
    const early: { ptyId: string; data: string }[] = [];
    const t = new Terminal({
      fontFamily: '"JetBrains Mono Variable", "Cascadia Mono", monospace',
      fontSize: 12.5,
      lineHeight: 1.25,
      cursorBlink: !readOnly,
      disableStdin: !!readOnly,
      allowProposedApi: false,
      theme: themeFromCss(),
    });
    // Only Ctrl+J (hide the panel) leaves the terminal; Ctrl+B, Ctrl+N and the rest stay with the
    // shell, where they edit the line and walk the history.
    t.attachCustomKeyEventHandler(
      (e) => !(e.ctrlKey && !e.shiftKey && e.key.toLowerCase() === "j"),
    );
    const fit = new FitAddon();
    t.loadAddon(fit);
    term.current = t;

    // Data can arrive before pty_open answers, so hold it until we know our id.
    const stopData = on("pty:data", ({ payload }) => {
      if (ptyId === null) early.push(payload);
      else if (payload.ptyId === ptyId) t.write(payload.data);
    });
    const stopExit = on("pty:exit", ({ payload }) => {
      if (payload.ptyId === ptyId) exitRef.current();
    });

    const resize = new ResizeObserver(() => {
      if (!el.clientWidth || !el.clientHeight) return;
      fit.fit();
      if (ptyId && (!attach || fitAttached))
        callOrRun("pty_resize", "pty.resize", { ptyId, cols: t.cols, rows: t.rows }).catch(
          () => undefined,
        );
    });
    // Re-theme when the tint changes.
    const retheme = new MutationObserver(() => (t.options.theme = themeFromCss()));
    retheme.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-tint"],
    });

    document.fonts.load('12px "JetBrains Mono Variable"').finally(async () => {
      if (disposed) return;
      t.open(el);
      fit.fit();
      resize.observe(el);
      t.onData(
        (data) =>
          ptyId &&
          !readOnlyRef.current &&
          callOrRun("pty_write", "pty.write", { ptyId, data }).catch(
            (e) => attach && t.write(`\r\n\x1b[31mCouldn't type here: ${errorText(e)}\x1b[0m\r\n`),
          ),
      );
      if (attach) {
        // Someone else's pty: its scrollback first, then live data (held in `early` meanwhile).
        t.write("\x1b[2mAttaching…\x1b[0m\r\n");
        try {
          const r = await runCommand<{ text: string; offset: number }>("pty.read", {
            ptyId: attach,
          });
          if (disposed) return;
          // pty.read is plain text with bare \n: xterm needs \r\n to start each line.
          t.write(r.text.replace(/\r?\n/g, "\r\n"));
          // One dim line saying what was replayed, so a quiet terminal can't look broken.
          if (!r.text) t.write("\x1b[2m(no output yet)\x1b[0m\r\n");
        } catch (e) {
          // Say why, in the pane: a silent blank terminal can't be told from an idle one.
          t.write(`\x1b[31mCouldn't read this terminal: ${errorText(e)}\x1b[0m\r\n`);
        }
        // The scrollback already holds what arrived while it was read.
        ptyId = attach;
        // Ours to size: the program redraws at the pane's width.
        if (fitAttached)
          callOrRun("pty_resize", "pty.resize", { ptyId, cols: t.cols, rows: t.rows }).catch(
            () => undefined,
          );
        return;
      }
      try {
        ({ ptyId } = openRef.current
          ? await openRef.current({ cols: t.cols, rows: t.rows })
          : await callOrRun("pty_open", "pty.open", {
              threadId: threadId!,
              cols: t.cols,
              rows: t.rows,
            }));
        if (disposed) return void callOrRun("pty_close", "pty.close", { ptyId });
        early.filter((e) => e.ptyId === ptyId).forEach((e) => t.write(e.data));
      } catch (e) {
        t.write(
          `\x1b[31m${openRef.current ? "Couldn't start" : "Couldn't open a shell"}: ${String((e as { message?: string }).message ?? e)}\x1b[0m\r\n`,
        );
      }
    });

    return () => {
      disposed = true;
      resize.disconnect();
      retheme.disconnect();
      stopData.then((stop) => stop());
      stopExit.then((stop) => stop());
      if (ptyId && !attach) callOrRun("pty_close", "pty.close", { ptyId }).catch(() => undefined);
      t.dispose();
    };
  }, [threadId, attach, fitAttached]);

  useEffect(() => {
    if (active) term.current?.focus();
  }, [active]);

  return (
    <div ref={box} onFocus={onFocus} className="min-h-0 min-w-0 flex-1 overflow-hidden px-3 py-2" />
  );
}
