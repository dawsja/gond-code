# Gond Code's commands

Every action the app takes goes through one command registry (`src-tauri/src/api`),
called by the UI through `command_run` and mirrored into the state tree it watches
(`state_get`, `state:patch` events). Each pane runs an agent's own CLI, so agents don't
call these; the window does. `src/lib/commands.generated.json` is this list with its schemas, checked by
`commands_json_is_current`; `agent_api_tables_match_the_registry` checks this file.

Risk: `read` changes nothing, `write-local` changes this PC only, `outward` reaches the
network (installs, sign-ins), `destructive` can lose data. **human only** commands
refuse any caller but the window.

## App

| Command | Risk | Input → result |
|---|---|---|
| `app.update.check` | **outward**, **human only** | `{}` → `null`: looks for a newer version now; the answer lands in `/update` (`invalid` in Gond Code Dev) |
| `app.update.install` | **outward**, **human only** | `{}` → `null`: downloads the newer version with its progress in `/update`, verifies its signature, then restarts into it; the sessions and panes come back as saved (`invalid` in Gond Code Dev) |
| `app.update.status` | read | `{}` → `{ current, phase, channel, checkedAt?, available?: { version, notes?, date? }, percent?, error? }`: this version (`channel` is `prod`, or `dev` for Gond Code Dev, which never updates), the newer one if any, and how a check or download is going (also at `/update`) |
| `app.version` | read | `{}` → `string` |
| `folder.open` | read, **human only** | `{ path }` → `null`: opens an existing local directory in File Explorer; `~` means home, `/c/…` and `/mnt/c/…` are Windows drives. Rejects files and missing directories. The pane menu uses its last prompt folder, else resume cwd, else registered start folder, else home; hidden on WSL/SSH panes. Prompt-derived UNC (`\\host\share`, including `\\wsl$`) is rejected by `promptFolder` before any consumer, including folder tracking and restart snapshots; falls back to resume cwd, registered start folder or home without probing the network |
| `folder.git` | read | `{ path }` → `{ folder, repo?, branch? }`: the git repository and branch a folder is in (the status bar) |
| `layout.focus` | read, **human only** | `{ panel: "chat" \| "settings", entityId? }` → `null`: shows the workspace (`chat`) or a Settings section (`entityId`: general, usage, harnesses, connections, updates). It changes only what's on screen |
| `sessions.get` | read | `{}` → the saved sessions (`{ sessions: { id, name, layout }[], current? }`, each layout a tree of right/down splits whose leaves are panes), or `null` before any. A `spaces.json` from before the rename is read until the next save |
| `sessions.set` | write-local | `{ sessions }` → `null`: saves them to `sessions.json` beside the database, written whole then swapped in |
| `settings.get` | read | `{}` → `Settings` |
| `settings.set` | write-local | `{ patch }` → `Settings`: `patch` takes `tint`, `shell` (a `shell.list` id, `null` for the default), `editor` (a `file.editors` id, `system` for the default app, `null` for the first editor found), `closeToTray`, `keepTerminals` (quitting leaves the terminals running in the terminal host; default `true`) `terminalFont` (`jetbrains-mono`, `hack` or `fira-code`) and `sounds` (the cues when an agent is done or needs you; default `true`); nothing else |
| `shell.list` | read | `{}` → `{ id, label, program, args }[]`: the shells a plain terminal can run on this PC: PowerShell 7, Windows PowerShell, Command Prompt, Git Bash, each WSL distribution. Settings `shell` picks one by id; `pty.open` from the human starts it |

## Folders

| Command | Risk | Input → result |
|---|---|---|
| `project.add` | write-local | `{ path }` → `Project`: a folder agents work in (a Git repo's root when it's inside one); adding it again returns the same project. Nothing is written into the folder |
| `project.list` | read | `{}` → `Project[]` |
| `project.remove` | write-local | `{ projectId }` → `null`: forgets the project (its folder stays on disk) and closes its terminals |
| `project.touch` | write-local | `{ projectId }` → `null` |

## Panes and terminals

| Command | Risk | Input → result |
|---|---|---|
| `clipboard.read` | read, **human only** | `{}` → `{ text?, image }`: what Ctrl+Shift+V pastes into a pane: the clipboard's text, and whether it holds a picture (an agent's pane then gets its image-paste key) |
| `agent.resume` | read | `{ ptyId, since, cwd?, taken? }` → `{ agent, session, command, cwd } \| null`: how to pick the conversation of the agent running in that terminal back up after a restart. The agent, the folder it really runs in and its arguments are read from its process (`cwd` only if the folder can't be). An agent started on a conversation (`--resume <id>`, `--session <id>`, `--conversation <id>`) names it; else it's the newest in the agent's own store, in that folder, since `since` (ms since 1970; 0 for any time), not in `taken`. `command` resumes it with the agent's own command (`claude --resume <id>`, `codex resume <id>`, `grok --resume <id>`, `opencode --session <id>`, `pi --session <id>`, `agy --conversation <id>`, `devin --resume <id>`). `null` with no conversation yet, or for a custom agent |
| `file.editors` | read | `{}` → `{ id, label }[]`: the code editors on this PC a `file:line` link can open in (VS Code, Cursor, Windsurf, Zed; whichever are installed) |
| `file.open` | write-local, **human only** | `{ path, line?, column? }` → `null`: opens a file at its line in the editor picked in Settings (`editor`), else the first one found, else the file's default app. The only way into an editor: a `file:line` link a pane printed |
| `file.resolve` | read | `{ paths, cwd? }` → `(string \| null)[]`: each path a pane printed, as the file it points at if it is one: relative to `cwd` (the pane's folder; else your home folder), `~` your home folder, `/c/…` and `/mnt/c/…` the Windows drive |
| `pane.drop` | write-local, **human only** | `{ paths, shell? }` → `string`: the text to type for files and folders dropped on a pane, each path as `shell` writes it (`/c/…` in Git Bash, `/mnt/c/…` in WSL, quoted when needed) and followed by a space; a file in the temp folder is first copied to `drops/` under the local data folder (kept a week) |
| `pane.screen.save` | write-local, **human only** | `{ paneId, screen, cwd? }` → `null`: keeps a shell pane's screen (as xterm serialises it) and folder (prompt-derived UNC cwd is omitted) in `screens/` beside the database, before a restart into an update |
| `pane.screen.take` | write-local, **human only** | `{ paneId }` → `{ screen, cwd? } \| null`: the saved screen and folder, once (reading removes it) |
| `pty.attach` | read | `{ key, shell? }` → `{ ptyId, pid?, replay, end, exited } \| null`: the terminal opened for pane `key` that is still in the terminal host (Gond Code restarted, updated or crashed; with a machine's `shell`, the host on that machine, connecting to it first), with escape codes in `replay` that draw it exactly as it is: its history (up to 4000 lines), screen, modes and cursor; `pty:data` with `end` at or below this `end` is already in it |
| `pty.agent` | read | `{ ptyId }` → `string \| null`: the agent running in the terminal, one you typed into its shell (`claude`, `codex`…), by its program under the shell (on another machine, its host looks there); `null` for none |
| `pty.close` | write-local | `{ ptyId }` → `null` |
| `pty.open` | write-local | `{ key?, projectId?, shell?, cwd?, cols?, rows? }` (default 120×32) → `{ ptyId, shell? }`: a terminal in the project's folder, else your home folder (also for an empty or removed project ID; other lookup errors propagate); `shell` is a `shell.list` id (else Settings' pick) or a machine's id (`wsl:<distro>`, `ssh:<id>`: its user's login shell in that machine's terminal host, in the project's folder as WSL sees it or the home folder), `cwd` a folder to start in instead; the answer names the shell it ran. `key` is the pane's id: the terminal runs in the terminal host, outlives the app and is found again with `pty.attach`; one already under that key is closed first |
| `pty.prune` | write-local | `{ keep: paneId[] }` → `number`: closes the terminals of panes not in `keep` (gone while the app couldn't save them) |
| `pty.read` | read | `{ ptyId, since?: offset, waitFor?: regex, timeoutMs? }` → `{ text, offset, truncated, exited, exitCode?, matched? }` |
| `pty.resize` | write-local | `{ ptyId, cols, rows }` → `null` |
| `pty.write` | write-local | `{ ptyId, data }` → `null` |
| `sound.play` | write-local | `{ cue: "done" \| "needs" }` → `boolean`: queues the chime for an agent that finished while you were elsewhere, or the alert for one that needs you; cues play one after another, and one already waiting isn't queued twice. `false` when Sounds is off (`settings.set` `sounds`) or it was already waiting |

## Machines

Where a terminal runs besides this PC: a WSL distribution or an SSH machine. Gond copies its terminal host for Linux there and talks to it through `wsl.exe` or Windows' OpenSSH; the terminals live there, outliving the connection. Each connection's state is at `/machines/<id>` (`state`: `connecting` with its `step`, `up`, `down` with its `error` and whether it's `retrying`, `off`; `up` counts the times it came up; `password` when it signed in with a password); a question ssh asks while signing in waits at `/askpass/<id>`. SSH keys are the key pairs in `~/.ssh`; a machine signs in with one through its `IdentityFile` in Gond's ssh config.

| Command | Risk | Input → result |
|---|---|---|
| `machine.askpass.answer` | outward, **human only**, `redact: ["/answer"]` | `{ id, answer? }` → `null`: what to tell ssh for question `id` (a passphrase, a password, `yes` to a new host key); no `answer` cancels signing in. Passed to ssh and kept nowhere |
| `machine.add` | write-local, **human only** | `{ host, port?, name? }` → `{ id, kind, name, address? }`: adds an SSH machine: a `Host` from your ssh config, a host name or address, or `user@host`. It's a `Host` (`name`, else the host's) in Gond's ssh config, `~/.ssh/gond/config`, which `~/.ssh/config` includes; a host your ssh config sets up keeps its own name and settings |
| `machine.closeAll` | destructive, **human only** | `{ id }` → `null`: closes every terminal on the machine and stops Gond's terminal host there |
| `machine.connect` | outward | `{ id }` → `null`: connects to the machine: `ssh` signs in as you would (your config, keys and agent; anything it asks comes to the window), Gond's helper is copied to `~/.gond/bin/<hash>/` there if it isn't, and its terminal host starts if it isn't running |
| `machine.edit` | write-local, **human only** | `{ id, host, port? }` → `{ id, kind, name, address?, key? }`: changes where an SSH machine is (`user@host` and port: its `HostName`, `User` and `Port` in Gond's ssh config). Its name stays. A connection to it starts over; its terminals keep running where they were. Not for a host your own ssh config sets up |
| `machine.disconnect` | write-local | `{ id }` → `null`: drops the connection; the machine's terminals keep running there |
| `machine.key.authorize` | outward, **human only** | `{ id, key }` → `{ via }`: puts key `key` on SSH machine `id` (its `~/.ssh/authorized_keys`, unless it's there) over Gond's connection, connecting first if need be, and signs in with it from then on (its `IdentityFile`). A terminal host there from an older Gond doesn't know how: then a one-off `ssh` adds it (`via: "ssh"`), which may ask for the password again |
| `machine.key.check` | outward | `{ id }` → `null`: signs in to SSH machine `id` with its key alone (no password, no other key); fails with ssh's reason if the machine doesn't take it. A key with a passphrase asks for it in the window |
| `machine.key.create` | write-local, **human only**, `redact: ["/passphrase"]` | `{ name, type?, passphrase? }` → its name: makes a key pair `~/.ssh/<name>` and `<name>.pub` (`type`: `ed25519`, the default, or `rsa`, 4096 bits), the private key readable by you alone and encrypted with `passphrase` if given. Never over a file that's there |
| `machine.key.delete` | destructive, **human only** | `{ name }` → `null`: deletes a key pair (to the Recycle Bin); machines that signed in with it go back to ssh's own choice |
| `machine.key.list` | read | `{}` → `{ keys: { name, path, kind, fingerprint, comment, passphrase, publicKey, default, usedBy }[] }`: the key pairs in `~/.ssh` (a public key alone, or PuTTY's `.ppk`, isn't one), with the SSH machines that sign in with each |
| `machine.key.revoke` | destructive, **human only** | `{ id, key }` → `null`: takes key `key` out of SSH machine `id`'s `~/.ssh/authorized_keys`; if the machine signed in with it, ssh picks again |
| `machine.key.use` | write-local | `{ id, key? }` → `null`: sets the key SSH machine `id` signs in with (its `IdentityFile`); no `key` leaves it to ssh |
| `machine.list` | read | `{}` → `{ machines: { id, kind, name, address?, key? }[], sshConfig: string[], configFile }`: each WSL distribution (`wsl:<distro>`), the SSH machines added (`ssh:<host>`, the `Host`s in Gond's ssh config `configFile`), and the `Host`s in your ssh config (and the files it includes) not added yet |
| `machine.remove` | destructive, **human only** | `{ id }` → `null`: removes an SSH machine from Gond: its `Host` leaves Gond's ssh config. Its terminals there are left running |

## Agents

| Command | Risk | Input → result |
|---|---|---|
| `agent.custom.delete` | write-local | `{ id }` → `null` |
| `agent.custom.save` | write-local | `{ agent }` → `CustomAgent` |
| `agent.detect` | read | `{}` → `AgentInfo[]`: every built-in agent and saved custom agent, with whether its CLI is installed and its version |
| `harness.check` | read (fetches public version metadata) | `{}` → `{ id, installed?, latest?, updateAvailable, changelog }[]`: for each installed agent (one that isn't is never looked up), latest from the npm registry, GitHub releases (`agy`) or a plain-text file (`grok`), as each agent's file says; offline means `latest` is absent |
| `harness.install` | **outward** | `{ harnessId, planIndex?, machineScope? }` → `{ jobId, steps: Step[] }`; prerequisites batched in front, one approval; runs in the background (no console) and reports into `/installs/{jobId}` |
| `harness.uninstall` | **destructive**, **human only** | `{ harnessId, keepData? }` → `{ jobId, steps }` at `/installs/{jobId}`: winget uninstall, the npm/Bun globals, the vendor's files, then its user PATH entries (a shared folder only once empty), never anything outside that harness's fixed list under the user's profile. `keepData` (default true) keeps settings and sign-ins; `false` also deletes them and the harness's accounts and vault secrets. `invalid` while any of its agents run, or when nothing Gond knows is installed. Done only when the program is no longer found; the last step's note reads "X is uninstalled." |
| `harness.uninstallPlan` | read | `{ harnessId, keepData? }` → `{ harnessId, methods: ("winget" \| "vendor" \| "npm" \| "bun")[], steps, deletes: string[], pathEntries: string[], keepData, running }`: exactly what `harness.uninstall` would remove, for the confirm card. `deletes` lists every file and folder; `keepData: false` adds its settings and sign-in dirs. `running` counts the terminals running it now (uninstall refuses until they stop) |
| `harness.update` | **outward** | `{ harnessId }` → `{ jobId, steps }`: updates the way it was installed; `invalid` when Gond can't tell |
| `install.cancel` | write-local | `{ jobId }` → `null`: stops a running install job and its current step's process tree; the job ends `cancelled`. `not_found` once finished |
| `prereq.install` | **outward** | `{ prereqs: string[], machineScope? }` → `{ jobId, steps }`: installs the missing ones (winget, silent, per-user). Without winget, or when its install fails, Git, GitHub CLI and Node come from their official releases. Gond downloads them with the same stages and bytes, checks the vendor's published SHA-256, then runs Git's installer with `/CURRENTUSER` or unpacks the GitHub CLI and Node zips under `%LOCALAPPDATA%\Programs` and puts them on the user PATH (plan `source: "download"`) |
| `prereq.list` | read | `{}` → `{ id: "git" \| "gh" \| "node" \| "bun", name, installed, version? }[]` |

## Usage

| Command | Risk | Input → result |
|---|---|---|
| `usage.refresh` | read | `{}` → `null`; reads the plan limits of each agent's own sign-in (Claude Code, Codex, Grok Build, Antigravity, Devin) into `/usage/default:<agent>/windows` (`{ name, label?, percentLeft, resetsAt?, status? }`), kept across restarts |

## GitHub and Git

| Command | Risk | Input → result |
|---|---|---|
| `git.identity` | read | Git's user.name and user.email, and a suggestion from GitHub if unset |
| `git.identity.set` | write-local, **human only** | Set Git's global user.name and user.email |
| `github.login.token` | outward, **human only**, `redact: ["/token"]` | `{ host?, token }` → `GithubAccount`. The token is piped to `gh auth login --with-token` on stdin, never argv |
| `github.login.web` | outward, **human only** | `{ host? }` → `{ userCode, verificationUri, expiresAt }`. It runs `gh auth login --web`, returns the one-time code at once for the UI to show, and completes in the background, updating `state.github` when done |
| `github.logout` | write-local, **human only** | `{ host?, login }` → `GithubStatus` (`gh auth logout`) |
| `github.status` | read | `{}` → `GithubStatus` (from `gh auth status`) |
| `github.switch` | write-local, **human only** | `{ host?, login }` → `GithubStatus` (`gh auth switch`) |

## Speech to text

| Command | Risk | Input → result |
|---|---|---|
| `speech.disable` | write-local | `{}` → `null`; stops a download and deletes the model |
| `speech.enable` | write-local | `{}` → `null`; downloads the speech model (731 MB), progress at `/speech`; `invalid` on a CPU without AVX2, FMA, F16C or BMI2 (`/speech` is then `unsupported`, with why) |
| `speech.transcribe` | read | `{ audio }` (16 kHz mono f32, base64) → the text |
