// @vitest-environment happy-dom
import { Terminal } from "@xterm/xterm";
import { expect, it } from "vitest";
import { promptFolder, promptRows } from "./prompt";

it("prefers the live prompt over older prompts with real xterm scrollback", async () => {
  const terminal = new Terminal({ cols: 100, rows: 24, scrollback: 1000 });
  const write = (text: string) => new Promise<void>((resolve) => terminal.write(text, resolve));
  try {
    await write("history\r\n".repeat(80));
    await write("PS C:\\old> cd C:\\new\r\n" + "output\r\n".repeat(18) + "PS C:\\new> ");
    expect(terminal.buffer.active.baseY).toBeGreaterThan(0);
    const ordered = promptRows(terminal);
    const oldBottomFirst = [...ordered.slice(-14), ...ordered.slice(0, -14)];
    expect(promptFolder(oldBottomFirst)).toBe("C:\\old");
    expect(promptFolder(ordered)).toBe("C:\\new");
    expect(promptRows(terminal).filter((line) => line.startsWith("PS C:\\old>"))).toHaveLength(1);
    terminal.reset();
    await write("PS C:\\fresh> ");
    expect(promptFolder(promptRows(terminal))).toBe("C:\\fresh");
  } finally {
    terminal.dispose();
  }
});
