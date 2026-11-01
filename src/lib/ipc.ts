// Typed wrappers over Tauri's invoke/listen. Every Rust command and event the UI
// uses gets a function here, so the contract lives in one place.
// Outside Tauri (`bun run dev` in a browser) calls go to an in-memory mock instead, and so
// do builds made with VITE_GOND_MOCK=1: the real Windows app on fake data, for CI screenshots.
import { invoke, isTauri } from "@tauri-apps/api/core";
import generated from "./commands.generated.json";
import { listen, type EventCallback, type UnlistenFn } from "@tauri-apps/api/event";
import type { AgentInfo, Project, Settings } from "./types";

type None = Record<string, never>;

/** True when calls go to the in-memory mock rather than the Rust side. */
export const mockBackend = () => !isTauri() || import.meta.env.VITE_GOND_MOCK === "1";

/** Command name → [args, result]. Mirrors the #[tauri::command]s in src-tauri. */
export interface Commands {
  app_version: [None, string];
  project_add: [{ path: string }, Project];
  project_list: [None, Project[]];
  project_remove: [{ projectId: string }, null];
  project_touch: [{ projectId: string }, null];
  agent_detect: [None, AgentInfo[]];
  pty_open: [
    { threadId?: string; projectId?: string; cols: number; rows: number },
    { ptyId: string },
  ];
  pty_write: [{ ptyId: string; data: string }, null];
  pty_resize: [{ ptyId: string; cols: number; rows: number }, null];
  pty_close: [{ ptyId: string }, null];
  settings_get: [None, Settings];
  settings_set: [{ patch: Partial<Settings> }, Settings];
}

/** Event name → payload. Mirrors the events Rust emits. */
export interface Events {
  /** `end`: the terminal's output length after `data` (a pane that attached skips what its
   *  replay already drew). */
  "pty:data": { ptyId: string; data: string; end?: number };
  "pty:exit": { ptyId: string; code?: number };
}

/**
 * The registered name of each old per-feature command ("Was"). In the app,
 * `call` runs these through `command_run`, so every UI call goes through the registry.
 */
export const REGISTRY_NAME: Partial<Record<keyof Commands, string>> = {
  app_version: "app.version",
  project_add: "project.add",
  project_list: "project.list",
  project_remove: "project.remove",
  project_touch: "project.touch",
  agent_detect: "agent.detect",
  pty_open: "pty.open",
  pty_write: "pty.write",
  pty_resize: "pty.resize",
  pty_close: "pty.close",
  settings_get: "settings.get",
  settings_set: "settings.set",
};

const REGISTERED = new Set<string>(generated.map((c: { name: string }) => c.name));

export async function call<K extends keyof Commands>(
  cmd: K,
  args: Commands[K][0],
): Promise<Commands[K][1]> {
  const name = REGISTRY_NAME[cmd];
  if (!mockBackend() && name && REGISTERED.has(name))
    return invoke<Commands[K][1]>("command_run", { name, input: args ?? {} });
  if (!mockBackend()) return invoke<Commands[K][1]>(cmd, args);
  const { mockCall } = await import("./mock");
  return mockCall(cmd, args);
}

export async function on<K extends keyof Events>(
  event: K,
  cb: EventCallback<Events[K]>,
): Promise<UnlistenFn> {
  if (!mockBackend()) return listen<Events[K]>(event, cb);
  const { mockOn } = await import("./mock");
  return mockOn(event, cb);
}

/** The OS folder picker (tauri-plugin-dialog); null if cancelled. */
export async function pickFolder(): Promise<string | null> {
  if (mockBackend()) return (await import("./mock")).mockPickFolder();
  const { open } = await import("@tauri-apps/plugin-dialog");
  return open({ directory: true, title: "Open a project folder" });
}

/** Opens a link in the default browser (tauri-plugin-opener); a new tab outside Tauri. */
export async function openExternal(url: string) {
  if (!isTauri()) return void window.open(url, "_blank", "noopener");
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(url);
}

/** Files dragged over the window and dropped (`type` "over" while they move, then "drop" or
 *  "leave"), with their paths and where they are, in CSS pixels. */
export type FileDrag = { type: "over" | "drop" | "leave"; paths: string[]; x: number; y: number };

/** Tauri's own drag and drop: real paths, which a browser drop never has. Nothing in the mock. */
export async function onFileDrag(cb: (e: FileDrag) => void): Promise<UnlistenFn> {
  if (mockBackend()) return () => undefined;
  const { getCurrentWebview } = await import("@tauri-apps/api/webview");
  // The paths come with "enter"; "over" only moves.
  let paths: string[] = [];
  return getCurrentWebview().onDragDropEvent(({ payload: p }) => {
    if (p.type === "leave") return cb({ type: "leave", paths: [], x: 0, y: 0 });
    if (p.type === "enter" || p.type === "drop") paths = p.paths;
    const r = window.devicePixelRatio || 1;
    cb({
      type: p.type === "drop" ? "drop" : "over",
      paths,
      x: p.position.x / r,
      y: p.position.y / r,
    });
  });
}
