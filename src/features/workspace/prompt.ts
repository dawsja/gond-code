import type { Terminal } from "@xterm/xterm";

/**
 * The folder a shell is in, read from its prompt on the last lines of its screen: PowerShell's
 * `PS C:\src>`, Command Prompt's `C:\src>`, Git Bash's `user@host MINGW64 /c/src`. Undefined when
 * no local drive prompt shows one (a program running, a custom prompt, WSL or UNC).
 */
export function promptFolder(lines: string[]): string | undefined {
  for (const line of lines
    .filter((l) => l.trim())
    .slice(-6)
    .reverse()) {
    const win = /^(?:PS )?([A-Za-z]:[\\/][^>]*?)>/.exec(line.trim());
    // A drive root keeps its backslash: "C:" alone means that drive's current folder.
    if (win) return /^[A-Za-z]:[\\/]$/.test(win[1]) ? win[1] : win[1].replace(/[\\/]$/, "");
    const bash = /\bMINGW(?:64|32) (\/[a-zA-Z](?:\/\S*)?)/.exec(line);
    if (bash) {
      const [, drive, rest = ""] = /^\/([a-zA-Z])(\/.*)?$/.exec(bash[1]) ?? [];
      if (drive) return `${drive.toUpperCase()}:${rest.replaceAll("/", "\\") || "\\"}`;
    }
  }
  return undefined;
}

/** Cursor and bottom regions, in buffer order, with overlapping rows read only once. */
export function promptRows(terminal: Terminal): string[] {
  const buffer = terminal.buffer.active;
  const bottom = Math.max(0, buffer.baseY + terminal.rows - 14);
  const cursorEnd = Math.min(buffer.baseY + buffer.cursorY + 2, bottom);
  const rows: string[] = [];
  for (let y = Math.max(0, cursorEnd - 14); y < cursorEnd; y++)
    rows.push(buffer.getLine(y)?.translateToString(true) ?? "");
  for (let y = bottom; y < buffer.baseY + terminal.rows; y++)
    rows.push(buffer.getLine(y)?.translateToString(true) ?? "");
  return rows;
}
