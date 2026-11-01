import type { ILink, ILinkProvider, Terminal } from "@xterm/xterm";
import { isRegistered, liveBackend, runCommand } from "@/lib/appState";
import { notify } from "@/lib/notify";
import { errorText } from "@/lib/types";

// `file:line` in a pane opens in your editor: compiler errors, stack traces, an agent's "edited
// src/foo.ts:12". Only paths that are real files (file.resolve, from the pane's folder) become
// links; a click runs file.open at the line.

/** A path a pane printed, with the line and column after it, if any. */
export type FileRef = {
  index: number;
  length: number;
  path: string;
  line?: number;
  column?: number;
};

// A path: an optional root (C:\, ~/, ./, ../, /), folders, then a name with an extension that
// starts with a letter (so versions like 2.1.316 aren't paths); then :line[:col] or (line,col).
const PATH =
  /(?:[A-Za-z]:[\\/]|~[\\/]|\.{1,2}[\\/]|[\\/])?(?:[\w.@+-]+[\\/])*[\w@+-][\w.@+-]*\.[A-Za-z]\w{0,9}(?::(\d+)(?::(\d+))?|\((\d+)(?:,(\d+))?\))?/g;

/**
 * The paths in a line of terminal text that could be files: ones with a folder in them or a
 * line number after them (a bare "e.g." or "Node.js" is neither). Not the inside of a URL.
 */
export function findFileRefs(text: string): FileRef[] {
  const out: FileRef[] = [];
  for (const m of text.matchAll(PATH)) {
    const index = m.index ?? 0;
    const whole = m[0];
    // https://host/a/b.ts: the web-links addon's.
    if (/[\w+-]:\/?\/?$/.test(text.slice(Math.max(0, index - 12), index)) || whole.startsWith("//"))
      continue;
    const line = m[1] ?? m[3];
    const column = m[2] ?? m[4];
    const path = line ? whole.replace(/(?::\d+(?::\d+)?|\(\d+(?:,\d+)?\))$/, "") : whole;
    if (!line && !/[\\/]/.test(path)) continue;
    out.push({
      index,
      length: whole.length,
      path,
      ...(line ? { line: Number(line) } : {}),
      ...(column ? { column: Number(column) } : {}),
    });
  }
  return out;
}

// What file.resolve said, by folder and path, for a while (files come and go).
const known = new Map<string, Promise<string | null>>();
let cleared = Date.now();
function resolve(paths: string[], cwd: string | undefined): Promise<(string | null)[]> {
  if (Date.now() - cleared > 30_000) {
    known.clear();
    cleared = Date.now();
  }
  const key = (p: string) => `${cwd ?? ""}\n${p}`;
  const missing = paths.filter((p) => !known.has(key(p)));
  if (missing.length) {
    const asked = runCommand<(string | null)[]>("file.resolve", {
      paths: missing,
      ...(cwd ? { cwd } : {}),
    }).catch(() => missing.map(() => null));
    missing.forEach((p, i) =>
      known.set(
        key(p),
        asked.then((r) => r[i] ?? null),
      ),
    );
  }
  return Promise.all(paths.map((p) => known.get(key(p))!));
}

/** The link provider for a pane's terminal; `cwd` is the pane's folder now. */
export function fileLinks(t: Terminal, cwd: () => string | undefined): ILinkProvider {
  return {
    provideLinks(y, callback) {
      if (!liveBackend() || !isRegistered("file.resolve")) return callback(undefined);
      const text = t.buffer.active.getLine(y - 1)?.translateToString(true) ?? "";
      const refs = findFileRefs(text);
      if (!refs.length) return callback(undefined);
      void resolve(
        refs.map((r) => r.path),
        cwd(),
      ).then((files) => {
        const links: ILink[] = [];
        refs.forEach((r, i) => {
          const file = files[i];
          if (!file) return;
          links.push({
            range: { start: { x: r.index + 1, y }, end: { x: r.index + r.length, y } },
            text: text.slice(r.index, r.index + r.length),
            decorations: { underline: true, pointerCursor: true },
            activate: () =>
              void runCommand("file.open", {
                path: file,
                ...(r.line ? { line: r.line } : {}),
                ...(r.column ? { column: r.column } : {}),
              }).catch((e) =>
                notify.error("Couldn't open the file", { description: errorText(e) }),
              ),
          });
        });
        callback(links.length ? links : undefined);
      });
    },
  };
}
