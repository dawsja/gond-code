import type { Terminal } from "@xterm/xterm";
import type { SerializeAddon } from "@xterm/addon-serialize";
import { runCommand } from "@/lib/appState";
import { promptFolder, promptRows } from "./prompt";

export function savePaneScreen(paneId: string, terminal: Terminal, serialize: SerializeAddon) {
  return runCommand("pane.screen.save", {
    paneId,
    screen: serialize.serialize({ scrollback: 2000 }),
    cwd: promptFolder(promptRows(terminal)),
  });
}
