// @vitest-environment happy-dom
import { Terminal } from "@xterm/xterm";
import { SerializeAddon } from "@xterm/addon-serialize";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { expect, it } from "vitest";
import { promptFolder, promptRows } from "./prompt";
import { savePaneScreen } from "./paneScreen";

it("never persists a forged UNC prompt as a restart cwd", async () => {
  const terminal = new Terminal({ cols: 100, rows: 24 });
  const serialize = new SerializeAddon();
  terminal.loadAddon(serialize);
  const saved: Record<string, unknown>[] = [];
  // Only the native IPC boundary is substituted; parser, buffer and serializer are real.
  mockIPC((command, payload) => {
    expect(command).toBe("command_run");
    if (!payload || !("name" in payload) || !("input" in payload))
      throw new Error("expected command_run arguments");
    expect(payload.name).toBe("pane.screen.save");
    saved.push(JSON.parse(JSON.stringify(payload.input)));
    return null;
  });
  try {
    for (const path of ["\\\\evil\\share", "\\\\wsl$\\Ubuntu\\home", "//evil/share"]) {
      terminal.reset();
      await new Promise<void>((resolve) => terminal.write(`PS ${path}> `, resolve));
      expect(promptFolder(promptRows(terminal))).toBeUndefined();
      await savePaneScreen("pane", terminal, serialize);
      expect(saved.at(-1)).not.toHaveProperty("cwd");
      expect(saved.at(-1)?.screen).toContain(path);
    }
    terminal.reset();
    await new Promise<void>((resolve) => terminal.write("PS C:\\local> ", resolve));
    await savePaneScreen("pane", terminal, serialize);
    expect(saved.at(-1)?.cwd).toBe("C:\\local");
  } finally {
    clearMocks();
    terminal.dispose();
  }
});
