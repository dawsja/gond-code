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
   by program name in the shell's process tree). While one does, the pane shows it, reads Working / Needs you / Done
   (`status.ts`: the agent's OSC 7501 report, its title, its own working line, then the
   screen), and the sidebar lists
   it. Panes run with `TERM_PROGRAM=gond-code`: without it Devin takes ConPTY for the old
   console host and sends no OSC 7501.

### A pane's folder
`pty.open` starts in its registered project's folder, else home (or the supplied `cwd`).
Saved project IDs are sent even before the UI project list loads; Rust resolves them and falls
back to home for empty or removed IDs, without swallowing other lookup errors.
For a local shell, `Terminal.tsx` reads the folder its prompt shows, including a prompt near
the cursor above the bottom of the screen. The reader preserves buffer order with bottom rows
last, so older prompts cannot override the live prompt after scrollback. Prompt-derived UNC
paths (`\\host\share`, including `\\wsl$`) are rejected inside `promptFolder` for every
consumer, including restart snapshot cwd; the pane-folder store also filters them defensively. Terminal
output is untrusted and must not cause a network probe. The resolver falls back to saved resume
cwd, registered start folder or home. Rust still validates explicit paths as directories.
It keeps the last accepted folder while an agent runs;
it does not ask the agent. WSL and custom prompts may not show a readable folder.
Open folder resolves the pane's last prompt folder, else its saved resume cwd, else its
registered start folder, else home. `folder.open` accepts a path, not a project id, and
validates an existing directory before handing it to File Explorer. The menu entry is hidden
for WSL/SSH panes: those paths belong to another machine, not local Explorer.

### The terminal host
The terminals live in a background process so they outlive the app (`pty/host.rs`,
`pty/client.rs`, `pty/hub.rs`, `pty/proto.rs`).
- It's this exe run as `gond-code pty-host run <build>`, from a copy in
  `%LOCALAPPDATA%\GondCode\host\<build>` named `gond-code-host.exe` (Dev:
  `gond-code-dev-host.exe`); `<build>` is the exe's size and modified time. The copy lets the
  installer replace `gond-code.exe` and a dev build rebuild while it runs; its own name keeps
  the installer's "close Gond Code" from matching it.
- A short-lived launcher (`pty-host detach`) starts it, so it's nobody's child and a
  process-tree kill passes it by. The launcher hands it nothing but NUL for its input and output
  (an explicit handle list, not the standard spawn's every inheritable handle) and breaks it
  away from any job that allows it. A job that doesn't (`cargo run`, so `tauri dev`) keeps it,
  and the host's log says so: there its terminals last as long as that job. The host turns
  Ctrl+C handling back on, which its shells inherit.
- **One host per build.** Its pipe is named for the channel, the user's SID and the build
  (`\\.\pipe\app.gond.code-pty-<sid>-<build>`), and it writes `host\live\<build>.json` (its
  endpoint, build, pid and protocol) while it runs. The app starts new terminals in its own
  build's host. A pane that finds no terminal there looks in the older hosts `live` lists
  (`Ptys::attach`), and every request for that terminal then goes to the host that has it.
  An older host a newer window has reached (its `hello` names another build) ends once its last
  terminal has. So neither an update nor a change to this code ends or strands a terminal.
  Records of hosts that are gone are removed when read.
- Only this Windows user and SYSTEM can open the pipe, and only from this integrity level up
  (a low-integrity process can't drive it, nor an unelevated one an elevated Gond's). The app
  checks the pipe's server is a host exe from that folder before it trusts it. Elsewhere it's
  a Unix socket in the data folder.
- One JSON message per line: requests with ids and their replies, and the output of every
  terminal (`data` with its byte offset `end`, `exit`). Requests and fields are only ever added,
  never changed or removed, so a newer app always talks to an older host; a host answers one it
  doesn't know with an error. Bump `proto::PROTOCOL` when you add one.
- Each terminal's program runs in its own job (`proctree::contain_pid`): closing the pane ends
  everything it started, and if the host dies Windows closes the jobs and ends them too. No
  agent or dev server is left running without a pane.
- Each terminal's output goes through a terminal parser in the host (`pty/screen.rs`, the
  `vt100` crate), so the host knows its screen as it is: up to 4000 lines of history, the
  visible screen (or a full-screen app's own screen), and the modes the program switched on
  (bracketed paste, keypad and cursor keys, mouse mode and encoding, focus reporting, cursor
  style). `pty.attach` returns escape codes that draw all of it into a fresh xterm.js, with its
  `end`; the pane draws them, skips `pty:data` up to that `end`, and resizes a row smaller and
  back so a full-screen agent redraws too. Output while no window is connected only goes to the
  parser.
- `pty.prune` (after the sessions load) closes terminals whose pane no longer exists.
- With no terminals and no window for 30 s, the host ends. `gond-code pty-host stop` closes
  everything in every build's host and ends them; the uninstaller runs it (never an update).
- If the host can't be reached, `Ptys` runs terminals in the app as before, and `/ptyHost` is
  `false`. It logs to `host\host.log`.
- Quitting detaches (Settings `keepTerminals`, default on) or closes them all.

### Machines: WSL and SSH
A pane's `shell` can be a machine (`wsl:<distro>`, `ssh:<id>`; a session's `machine` is where
its new panes open). Its terminal then runs in a terminal host on that machine (`remote.rs`,
`remote/`).
- `gond-remote` is the host's own code (`pty/`, `agents/running.rs`, `cwd.rs`, `resume.rs`, `spec.rs` with `agents/`),
  built for Linux (x86_64, aarch64; static musl, linked by rust-lld) without the app: its
  `main.rs` includes those files and stands in for the modules they reach for. It's built
  with `--cfg gond_remote` (no SQLite, so the conversations agents keep in SQLite, opencode's,
  Antigravity's and the like, aren't read there). `bun run remote` builds it; `bun run dist:win` bundles it as `remote/<arch>/` beside
  the app, and a debug build takes it from `remote/target`.
- `remote::open` runs `wsl.exe -d <distro> --exec /bin/sh -c <script>` or Windows' OpenSSH
  `ssh -T … <host> "exec /bin/sh -c '<script>'"`. The script says the machine's OS, arch and
  home; the app answers the helper's hash; a missing helper is sent over the same stream (size,
  bytes, checked with `sha256sum`) to `~/.gond/bin/<hash>/` (`.gond-dev` for Dev); then it runs
  `gond-remote attach`, which starts the host if none runs (in its own session, so a hangup
  passes it by) and carries the protocol between its stdio and the host's socket. One
  connection per machine carries all its panes.
- One host per machine, whichever build started it: a newer helper joins the running host
  (requests are only ever added), which ends once it has no terminals and no window for 30 s.
- Protocol 2 adds what the app can't read from here: `agent` (the agent under a terminal's
  shell, by `/proc`: a program by the name it was started as, a script runtime (`node`, `bun`,
  `python`, `sh`) by its script), `resume`, `probe`, `files` and `upload` (`pty/here.rs`).
- `HostClient::remote` dials through `remote::open` and connects only when told (`Hub::ensure`):
  a request while it's down fails at once. A connection that drops doesn't end its terminals:
  `/machines/<id>` goes `down` and `Hub` retries (1 s … 2 min), unless signing in failed. Each
  time a machine comes up its `up` count grows and its panes attach again (`pty.attach` with
  their `shell`), redrawn from the host's screen.
- WSL stops a distribution ~15 s after its last `wsl.exe` ends. So after connecting, the app
  starts `wsl.exe … gond-remote keepalive` in its own terminal host on Windows (owner
  `keepalive:<machine>`), which outlives the app and ends when the host there does. It looks at
  the host's socket and live record, never connects (a connection is a window and would take the
  output stream).
- SSH questions: `ssh` runs with `SSH_ASKPASS` = this exe and `SSH_ASKPASS_REQUIRE=force`;
  `lib::subcommand` sees `GOND_ASKPASS` and passes the question over a user-only named pipe to
  the app, which shows it at `/askpass/<id>` until `machine.askpass.answer`. Nothing is kept.
- SSH keys (`sshkeys.rs`): the key pairs in `~/.ssh`. A key made here is OpenSSH's own format
  (the `ssh-key` crate: ED25519, or RSA 4096), a passphrase encrypting it as ssh-keygen does,
  the private file created readable by you alone (Windows' OpenSSH refuses a key others can
  read). A machine signs in with a key through its `IdentityFile` in Gond's ssh config.
  Putting a key on a machine (`Hub::authorize`) goes over the connection: protocol 3's
  `authorizedKeys` request has the host there add the line to `~/.ssh/authorized_keys` (a new
  `.ssh` 700, a new file 600; other lines untouched). A host from an older Gond answers that it
  doesn't know the request; then a one-off `ssh` with a short `sh` script adds it. Then
  `Hub::check_key` signs in with that key alone (`IdentitiesOnly`, no password). A connect
  where ssh asked for a password sets `password` on `/machines/<id>`. Gond runs Windows' own
  OpenSSH (`System32\OpenSSH\ssh.exe`) before any other `ssh` on PATH.
- SSH machines are `Host`s in Gond's own ssh config, `~/.ssh/gond/config` (Dev:
  `config-dev`), which `~/.ssh/config` includes from its first line (`sshconfig.rs`): `ssh
  <name>` works in any terminal, and the list moves with `~/.ssh`. Adding a machine writes its
  `Host` (and the `Include`, if it isn't there); removing it takes the `Host` out, and the
  `Include` stays. A host your ssh config already sets up gets an empty `Host`. Connecting runs
  `ssh <name>` and puts the `Include` back if it's gone. `GOND_SSH_OPTIONS` adds ssh options
  (a test's own known_hosts).

### Resume after a restart of the PC
While an agent runs in a pane, the pane asks `agent.resume` when it starts and every 20 s:
Rust finds the agent under the pane's shell, reads the folder it really runs in from its process
(`agents/cwd.rs`: the PEB on Windows, `/proc` on Linux), and its conversation (`agents/resume.rs`).
An agent started on one (`--resume <id>`, `--session <id>`…) names it on its command line, read
the same way. Otherwise it's the newest in the agent's own store, in its folder, changed since it
started, that another pane doesn't have: Claude Code's `projects/<folder>/<id>.jsonl`, Codex's
`sessions/…/rollout-…-<id>.jsonl`, Grok's `sessions/<folder>/<id>/summary.json`, opencode's
`opencode.db` (`session` table), Pi's `sessions/--<folder>--/<time>_<id>.jsonl`, Antigravity's
`conversation_summaries.db` and Devin's `%APPDATA%\devin\cli\sessions.db`. Each format was read
off a real install. The pane keeps
`{ agent, session, command, cwd }` as `resume` in `sessions.json`. A pane that opens without its
terminal (`pty.attach` finds none) starts its shell in `cwd` and types `command` once the prompt
shows. Quitting the agent, or Restart, clears it.

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
  features/machines/       the machine switcher, Add SSH machine, ssh's questions, Settings →
                           Machines
  features/shell/          workspace hook, the sidebar's update button
  lib/ipc.ts               the only place that calls invoke/listen
  lib/restart.ts           saves before Restart to update
  lib/commands.generated.json  the registry with schemas (checked by a test)
src-tauri/remote/          gond-remote: the terminal host for WSL and SSH machines (Linux)
src-tauri/src/             Rust
  main.rs lib.rs           builder, plugins, `gond-code path|purge-data` subcommands
  channel.rs appdata.rs    Gond Code or Gond Code Dev; where each keeps its data
  api/                     registry.rs, state.rs and one file per area (harness, layout,
                           projects, pty, speech, usage, git, github, builtin)
  commands/                thin #[tauri::command] wrappers (command_run, state_get, window)
  agents/                  spec.rs (the agents, from the repo's agents/*.toml, built in),
                           detect.rs (finding agent CLIs), running.rs (the agent in a pane),
                           resume.rs (its conversation)
  pty/                     PtyManager, scrollback; here.rs answers about this machine
  remote.rs api/machines.rs  WSL and SSH machines: connecting, the helper, ssh's questions
  projects.rs              folders
  clipboard.rs             the clipboard for a paste into a pane
  editors.rs api/files.rs  `file:line` links: which printed paths are files (`file.resolve`),
                           opening one at its line (`file.open`), the editors (`file.editors`);
                           `fileLinks.ts` finds them in a pane, `PaneSearch.tsx` searches
                           every pane's xterm buffer (`scrollbackSearch.ts`)
  drop.rs                  files dropped on a pane: paths as its shell writes them, temp
                           files copied to `drops/` (`pane.drop`; `FileDrop.tsx` listens)
  harness.rs install.rs    install/update plans; the background install engine
  fallback.rs releases.rs  installs without winget; latest versions
  uninstall.rs             removing an agent (keep or delete its data)
  plan_usage.rs grok_usage.rs  plan limits for Settings → Usage and the sidebar; api/usage.rs
                           reads them (`usage::watch`) when an agent with a plan starts in a
                           terminal and every 5 minutes while one runs
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
- `~/.ssh/gond/config` (Dev: `config-dev`): the SSH machines you added, as `Host`s;
  `~/.ssh/config` gets an `Include` of it.
- On a WSL or SSH machine: `~/.gond` (Dev: `~/.gond-dev`): the helper in `bin/<hash>/`, the
  host's socket `pty.sock`, its log and live record in `host/`, and dropped files in `drops/`.
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
- `bun run dist:win` → `scripts/build-remote.ts` (gond-remote for both Linux arches), then
  `tauri build --bundles nsis --config src-tauri/tauri.remote.conf.json`, which bundles them. The installer is per user. Its hooks
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
