# Gond Code — architecture

If the code and this file disagree, the code wins; fix this file.

## 1. Shape

```
┌──────────────────────── Tauri window (WebView2) ─────────────────────────┐
│ React 19 UI                                                              │
│  sidebar (Sessions, Agents) · panes (xterm.js, WebGL) · settings         │
│  src/lib/ipc.ts ── command_run(name, input) ──►   ◄── state:patch, pty:* │
└───────────────────────────────┬──────────────────────────────────────────┘
                                │ Tauri commands / events (JSON, camelCase)
┌───────────────────────────────▼──────────────────────────────────────────┐
│ Rust core (src-tauri)                                                    │
│  api/       the command registry and the state tree                      │
│  pty        portable-pty sessions (ConPTY on Windows), output to the UI  │
│  agents     finds agent CLIs, and the one running in a pane's shell      │
│  harness    install / update / uninstall plans; install runs the jobs    │
│  updater    checks latest.json, downloads and verifies the installer     │
│  store      SQLite                                                       │
└──────────────────────────────────────────────────────────────────────────┘
   spawns: the agents' CLIs, shells, git, gh, winget, bun — with the user's PATH
```

Local-first: no server. Every pane is a shell; you start an agent by typing it there, and Gond
runs nothing on an agent's behalf.

## 2. How a pane runs
1. The UI's layout (`src/features/workspace/layout.ts`) is a tree of right/down splits whose
   leaves are panes. Sessions (`useSessions.ts`) are saved with `sessions.set` to
   `sessions.json` beside the database.
2. Every pane opens with `pty.open`: the pane's own shell, else the one picked in Settings
   (`shells.rs`), in the pane's folder or the home folder. The pane keeps the shell id it ran,
   so it reopens the same shell.
3. `pty/` runs it in a ConPTY, in the terminal host (below), and streams output as `pty:data`;
   `Terminal.tsx` draws it. The pane's id is the terminal's key: a pane first asks
   `pty.attach` for a terminal still running under its id, and only opens a new one if there
   is none.
4. Every 2 s the pane asks `pty.agent` which agent runs under its shell (`agents/running.rs`,
   by program name in the shell's process tree). While one does, the pane shows it, reads
   Working / Needs you / Done from the screen (`status.ts`), and the sidebar lists it.

### The terminal host
The terminals live in a background process so they outlive the app (`pty/host.rs`,
`pty/client.rs`, `pty/hub.rs`, `pty/proto.rs`).
- It's this exe run as `gond-code pty-host`, from a copy in `%LOCALAPPDATA%\GondCode\host\<build>`
  named `gond-code-host.exe` (Dev: `gond-code-dev-host.exe`). The copy lets the installer
  replace `gond-code.exe` and a dev build rebuild while it runs; its own name keeps the
  installer's "close Gond Code" from matching it. A short-lived launcher (`pty-host detach`)
  starts it, so it's nobody's child and a process-tree kill passes it by; it breaks away from
  any job that allows it. It turns Ctrl+C handling back on, which its shells inherit.
- It listens on a named pipe only this Windows user (and SYSTEM) can open, named for the
  channel, the user's SID and the protocol version
  (`\\.\pipe\app.gond.code-pty-<sid>-v1`); the app checks the pipe's server is the host
  exe from that folder before it trusts it. Elsewhere it's a Unix socket in the data folder.
- One JSON message per line: requests with ids and their replies, and the output of every
  terminal (`data` with its byte offset `end`, `exit`). A newer app with a different protocol
  starts its own host; bump `proto::PROTOCOL` for any change an older host wouldn't understand.
- Each terminal keeps its last 2 MiB of raw output. `pty.attach` returns it with its `end`; the
  pane draws it and skips `pty:data` up to that `end`. Output while no window is connected only
  goes to that buffer.
- `pty.prune` (after the sessions load) closes terminals whose pane no longer exists.
- With no terminals and no window for 30 s, the host ends. `gond-code pty-host stop` closes
  everything and ends it; the uninstaller runs it (never an update).
- If the host can't be reached, `Ptys` runs terminals in the app as before, and `/ptyHost` is
  `false`. It logs to `host\host.log`.
- Quitting detaches (Settings `keepTerminals`, default on) or closes them all.

### Restart to update
`src/lib/restart.ts` lets each part register a save; Restart to update waits for all of them
(4 seconds at most), then calls `app.update.install`. The sessions are saved, and each shell
pane saves its serialized screen and the folder read from its prompt (`prompt.ts`) with
`pane.screen.save`. With the terminal host the panes simply attach again, agents still
working; the saved screen is the fallback for a host that's gone. Then `pane.screen.take`
returns it once and the shell starts in that folder.

## 3. Repo layout

```
src/                       React app
  main.tsx                 entry
  styles/theme.css         tokens (dark only, tints), see docs/THEME.md
  components/ui/           shadcn-style kit restyled to Gond
  components/brand/        agent and shell brand icons (@lobehub/icons, plus Pi's)
  features/deck/           Deck.tsx: the app shell (sidebar, workspace, settings)
  features/workspace/      sessions, panes, layout, agent detection, status, dictation, KeyHelp
  features/settings/       Settings space, General, Updates
  features/usage/          Settings → Usage
  features/harness/        Settings → Agents (install, update, uninstall)
  features/connections/    Settings → Plugins (GitHub)
  features/setup/          Git and GitHub CLI installs and the progress toast
  features/speech/         speech to text setting and recording
  features/shell/          workspace hook, the sidebar's update button
  lib/ipc.ts               the only place that calls invoke/listen
  lib/restart.ts           saves before Restart to update
  lib/commands.generated.json  the registry with schemas (checked by a test)
src-tauri/src/             Rust
  main.rs lib.rs           builder, plugins, `gond-code path|purge-data` subcommands
  channel.rs appdata.rs    Gond Code or Gond Code Dev; where each keeps its data
  api/                     registry.rs, state.rs and one file per area (harness, layout,
                           projects, pty, speech, usage, git, github, builtin)
  commands/                thin #[tauri::command] wrappers (command_run, state_get, window)
  agents/                  detect.rs (finding agent CLIs), running.rs (the agent in a pane)
  pty/                     PtyManager, scrollback
  projects.rs              folders
  clipboard.rs             the clipboard for a paste into a pane
  harness.rs install.rs    install/update plans; the background install engine
  fallback.rs releases.rs  installs without winget; latest versions
  uninstall.rs             removing an agent (keep or delete its data)
  plan_usage.rs grok_usage.rs  plan limits for Settings → Usage
  http.rs credentials.rs   curl for the usage sources; reading agents' own credentials
  github.rs git/           GitHub through `gh`; git CLI and identity
  shells.rs speech.rs      plain shells; speech to text (transcribe.cpp)
  settings.rs store/       typed settings; SQLite and migrations
  window.rs tray.rs app_id.rs  window, tray, the AppUserModelID
  updater.rs               in-app updates
  appdata.rs purge.rs pathenv.rs proctree.rs process.rs  data roots, uninstall purge, PATH,
                           job objects, spawning
```

Rules: Windows-only code behind `cfg(windows)`; everything else compiles and is unit-tested on
Linux. Logic lives outside `commands/` so it is tested without Tauri. The UI never shells out.

## 4. The command registry and state tree
- Every action is a registered command (`api/registry.rs`) with a dotted name, JSON schemas, a
  risk (`read`, `write-local`, `outward`, `destructive`) and a handler. Some are human only:
  only the window may call them. The UI calls them through `command_run`. There are 47; the
  list is [AGENT-API.md](AGENT-API.md). `agent_api_tables_match_the_registry` and
  `commands_json_is_current` keep it and `src/lib/commands.generated.json` in step
  (`UPDATE_COMMANDS=1 cargo test --lib commands_json` regenerates the JSON).
- Live state is one versioned JSON tree (`api/state.rs`: projects, installs, terminals, usage,
  speech, github, update, layout, settings). The UI reads it once with
  `state_get` and follows RFC 6902 patches in `state:patch` events. SQLite and the settings
  store are the source of truth; the tree is rebuilt from them at startup.
- Terminal output is a raw stream (`pty:data`, `pty:exit`), not part of the tree.
- `layout.focus` only switches between the workspace and a Settings section.

## 5. Conventions
- IDs: ULID strings. Times: RFC 3339 UTC strings.
- JSON over IPC is camelCase (`#[serde(rename_all = "camelCase")]`).
- Every command returns `Result<T, AppError>`; the UI gets `{ code, message }` (`error.rs`).
- Long work (installs, the speech download, an update download) never blocks a
  command: it reports progress into the tree. `install.cancel` stops a running install.
- Child processes never open a console window (`CREATE_NO_WINDOW`) and run in a job object the
  app owns (`proctree.rs`), so nothing is orphaned.

## 6. Where data lives
- `%APPDATA%\GondCode` (`appdata::roaming()`): `gond-code.db` (SQLite, WAL), `sessions.json`,
  and `screens\` (shell screens saved for an update, removed once read).
- `%LOCALAPPDATA%\GondCode` (`appdata::local()`): the speech model.
- Gond keeps no secrets. What older versions' accounts left (keys under `GondCode/` in
  Credential Manager, sign-in homes in `%LOCALAPPDATA%\GondCode\accounts`) is removed at
  startup (`purge::forget_accounts`); migration 21 dropped their tables.
- New data gets a folder under one of these roots, never a root of its own.
  `gond-code purge-data` (the uninstaller's "Delete the application data") deletes exactly
  these roots and credentials; it never touches a project folder or an agent's own config.
- Migrations are numbered SQL files in `store/migrations`, applied in order at startup.
- Gond Code Dev (every debug build; `channel.rs`) keeps all of this under `GondCodeDev`
  instead, so it never reads or writes prod's data.

### Dev and prod
`channel.rs` fixes the channel at compile time: a debug build (or `--features dev`) is Gond Code
Dev, a release build is Gond Code. Before the app is built, `channel::apply` gives Dev its own
identifier (`app.gond.code.dev`). The single-instance lock, the AppUserModelID, the WebView2
profile and the saved window state all key on that identifier. Dev also gets its own product
name and window title. `appdata::DIR` gives Dev its own data roots. The window and tray icons
get an amber dot (`channel::badge`). `updater.rs` neither checks nor installs in Dev, and
`/update.channel` tells the UI which app it is (the status bar and Settings → Updates show
"Gond Code Dev"). Prod's identity is tauri.conf.json's and must never change.

## 7. Packaging and updates
- `bun run dist:win` → `tauri build --bundles nsis`. The installer is per user. Its hooks
  (`windows/hooks.nsh`) remove the `bin` folder and PATH entry older builds added, delete the
  speech model on uninstall, and run `purge-data` when "Delete the application data" is ticked
  (never on an update).
- `updater.rs` checks at start, every 6 hours and on `app.update.check`. It reads `latest.json`
  from the latest published release of `dawsja/gond-code` and mirrors what it finds to
  `/update`. `app.update.install` downloads the installer with progress, verifies its minisign
  signature against the public key in `tauri.conf.json`, and runs it (NSIS, passive), which
  restarts the app.
- A `v*` tag runs `.github/workflows/release.yml`, which builds the installer, drafts the
  release and adds `latest.json` (`scripts/publish-release.sh`). See CONTRIBUTING.md.
