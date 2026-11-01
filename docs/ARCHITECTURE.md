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
│  runner     builds each pane's agent CLI (program, args, env, folder)    │
│  pty        portable-pty sessions (ConPTY on Windows), output to the UI  │
│  agents     one driver per agent: detect, models, sign-in, sessions      │
│  harness    install / update / uninstall plans; install runs the jobs    │
│  updater    checks latest.json, downloads and verifies the installer     │
│  store      SQLite; vault: Windows Credential Manager                    │
└──────────────────────────────────────────────────────────────────────────┘
   spawns: the agents' CLIs, shells, git, gh, winget, bun — with the user's PATH
```

Local-first: no server. Every pane runs the agent's own interactive CLI; Gond runs nothing in
the background on an agent's behalf.

## 2. How a pane runs
1. The UI's layout (`src/features/workspace/layout.ts`) is a tree of right/down splits whose
   leaves are panes. Sessions (`useSessions.ts`) are saved with `sessions.set` to
   `sessions.json` beside the database.
2. An agent pane has a **thread** (`threads.rs`, `thread.create`): the agent, its account, model,
   thinking level, folder and the agent's own session id.
3. `pane.start` (`api/pty.rs`) settles the session id, then asks `AgentRunner::interactive`
   (`runner.rs`) for the command: the agent's CLI with its model, thinking and resume flags, and
   the account's env (its config home or API key, resolved from the vault into the child's env
   only). Gond passes no permission flags.
4. `pty/` runs it in a ConPTY and streams output as `pty:data`; `Terminal.tsx` draws it and
   reads Working / Needs you / Done from the screen (`status.ts`).
5. A plain shell pane uses `pty.open`: the pane's own shell, else the one picked in Settings
   (`shells.rs`), in the pane's folder or the home folder. The pane keeps the shell id it ran,
   so it reopens the same shell.

### Resume
Each agent resumes its conversation when its pane opens again (`api/pty.rs`,
`agents/sessions.rs`, `runner.rs`):

| Agent | First start | Opening again |
|---|---|---|
| Claude Code | `--session-id <new uuid>` | `--resume <id>` once its `.jsonl` exists, else a new id |
| Pi | `--session-id <new uuid>` (create or open) | the same |
| Grok Build | `-s <new uuid>` | `-r <id>` once Grok has saved it |
| Codex | nothing | finds the rollout file under `~/.codex/sessions` (or the account's `CODEX_HOME`) that this pane started in its folder, then `codex resume <id>` |
| Antigravity | nothing | the folder's entry in `last_conversations.json`, then `--conversation <id>` |
| opencode | nothing | `--continue` (the folder's latest session) |

For Codex, Antigravity and opencode, a pane counts as opening again when its thread is more than
15 seconds old. Custom agents start fresh.

### Restart to update
`src/lib/restart.ts` lets each part register a save; Restart to update waits for all of them
(4 seconds at most), then calls `app.update.install`. The sessions are saved, and each shell
pane saves its serialized screen and the folder read from its prompt (`prompt.ts`) with
`pane.screen.save`. When the pane opens again, `pane.screen.take` returns them once and the
shell starts in that folder.

## 3. Repo layout

```
src/                       React app
  main.tsx                 entry
  styles/theme.css         tokens (dark only, tints), see docs/THEME.md
  components/ui/           shadcn-style kit restyled to Gond
  components/brand/        agent and shell brand icons (@lobehub/icons, plus Pi's)
  features/deck/           Deck.tsx: the app shell (sidebar, workspace, settings)
  features/workspace/      sessions, panes, layout, New agent, status, dictation, KeyHelp
  features/terminal/       the sign-in fallback terminal
  features/settings/       Settings space, General, Updates
  features/usage/          Settings → Usage
  features/harness/        Settings → Agents (install, update, uninstall)
  features/accounts/       sign-in and API-key flows
  features/connections/    Settings → GitHub
  features/setup/          Git and GitHub CLI installs and the progress toast
  features/speech/         speech to text setting and recording
  features/shell/          workspace hook, the sidebar's update button
  lib/ipc.ts               the only place that calls invoke/listen
  lib/restart.ts           saves before Restart to update
  lib/commands.generated.json  the registry with schemas (checked by a test)
src-tauri/src/             Rust
  main.rs lib.rs           builder, plugins, `gond-code path|purge-data` subcommands
  api/                     registry.rs, state.rs and one file per area (accounts, harness,
                           layout, models, projects, pty, signin, speech, threads, usage, git,
                           github, builtin)
  commands/                thin #[tauri::command] wrappers (command_run, state_get, window)
  agents/                  driver.rs (trait), claude, codex, agy, grok, opencode, pi, custom,
                           detect.rs, models.rs, sessions.rs (finding an agent's session)
  runner.rs                a pane's agent command
  pty/                     PtyManager, scrollback
  threads.rs projects.rs   pane threads; folders
  accounts.rs vault/       accounts; secrets in Credential Manager
  harness.rs install.rs    install/update plans; the background install engine
  fallback.rs releases.rs  installs without winget; latest versions
  uninstall.rs             removing an agent (keep or delete its data)
  plan_usage.rs grok_usage.rs  plan limits for Settings → Usage
  endpoint_test.rs         "Test connection" for API-key accounts
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
  only the window may call them. The UI calls them through `command_run`. There are 55; the
  list is [AGENT-API.md](AGENT-API.md). `agent_api_tables_match_the_registry` and
  `commands_json_is_current` keep it and `src/lib/commands.generated.json` in step
  (`UPDATE_COMMANDS=1 cargo test --lib commands_json` regenerates the JSON).
- Live state is one versioned JSON tree (`api/state.rs`: projects, threads, accounts, installs,
  sign-ins, usage, speech, github, update, layout, settings). The UI reads it once with
  `state_get` and follows RFC 6902 patches in `state:patch` events. SQLite and the settings
  store are the source of truth; the tree is rebuilt from them at startup.
- Terminal output is a raw stream (`pty:data`, `pty:exit`), not part of the tree.
- `layout.focus` only switches between the workspace and a Settings section.

## 5. Conventions
- IDs: ULID strings. Times: RFC 3339 UTC strings.
- JSON over IPC is camelCase (`#[serde(rename_all = "camelCase")]`).
- Every command returns `Result<T, AppError>`; the UI gets `{ code, message }` (`error.rs`).
- Long work (installs, sign-ins, the speech download, an update download) never blocks a
  command: it reports progress into the tree. `install.cancel` stops a running install.
- Child processes never open a console window (`CREATE_NO_WINDOW`) and run in a job object the
  app owns (`proctree.rs`), so nothing is orphaned.

## 6. Where data lives
- `%APPDATA%\GondCode` (`appdata::roaming()`): `gond-code.db` (SQLite, WAL), `sessions.json`,
  and `screens\` (shell screens saved for an update, removed once read).
- `%LOCALAPPDATA%\GondCode` (`appdata::local()`): sign-in homes for accounts, the speech model.
- Secrets: Windows Credential Manager under `GondCode/` (`vault/`), never in SQLite, argv, logs
  or the tree.
- New data gets a folder under one of these roots, never a root of its own.
  `gond-code purge-data` (the uninstaller's "Delete the application data") deletes exactly
  these roots and credentials; it never touches a project folder or an agent's own config.
- Migrations are numbered SQL files in `store/migrations`, applied in order at startup.

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
