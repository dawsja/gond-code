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
| `app.update.check` | **outward**, **human only** | `{}` → `null`: looks for a newer version now; the answer lands in `/update` |
| `app.update.install` | **outward**, **human only** | `{}` → `null`: downloads the newer version with its progress in `/update`, verifies its signature, then restarts into it; the sessions and panes come back as saved |
| `app.update.status` | read | `{}` → `{ current, phase, checkedAt?, available?: { version, notes?, date? }, percent?, error? }`: this version, the newer one if any, and how a check or download is going (also at `/update`) |
| `app.version` | read | `{}` → `string` |
| `folder.open` | read | `{ projectId }` → `null`: opens the project's folder in File Explorer |
| `folder.git` | read | `{ path }` → `{ folder, repo?, branch? }`: the git repository and branch a folder is in (the status bar) |
| `layout.focus` | read, **human only** | `{ panel: "chat" \| "settings", entityId? }` → `null`: shows the workspace (`chat`) or a Settings section (`entityId`: general, usage, harnesses, connections, updates). It changes only what's on screen |
| `sessions.get` | read | `{}` → the saved sessions (`{ sessions: { id, name, layout }[], current? }`, each layout a tree of right/down splits whose leaves are panes), or `null` before any. A `spaces.json` from before the rename is read until the next save |
| `sessions.set` | write-local | `{ sessions }` → `null`: saves them to `sessions.json` beside the database, written whole then swapped in |
| `settings.get` | read | `{}` → `Settings` |
| `settings.set` | write-local | `{ patch }` → `Settings`: `patch` takes `tint`, `shell` (a `shell.list` id, `null` for the default) and `closeToTray`; nothing else |
| `shell.list` | read | `{}` → `{ id, label, program, args }[]`: the shells a plain terminal can run on this PC: PowerShell 7, Windows PowerShell, Command Prompt, Git Bash, each WSL distribution. Settings `shell` picks one by id; `pty.open` from the human starts it |

## Folders

| Command | Risk | Input → result |
|---|---|---|
| `project.add` | write-local | `{ path }` → `Project`: a folder agents work in (a Git repo's root when it's inside one); adding it again returns the same project. Nothing is written into the folder |
| `project.list` | read | `{}` → `Project[]` |
| `project.remove` | write-local; destructive with `archiveThreads` | `{ projectId, archiveThreads? }` → `null`; `invalid` while unarchived threads exist unless `archiveThreads: true` (stops and archives each) |
| `project.touch` | write-local | `{ projectId }` → `null` |

## Panes and terminals

| Command | Risk | Input → result |
|---|---|---|
| `pane.screen.save` | write-local, **human only** | `{ paneId, screen, cwd? }` → `null`: keeps a shell pane's screen (as xterm serialises it) and folder in `screens/` beside the database, before a restart into an update |
| `pane.screen.take` | write-local, **human only** | `{ paneId }` → `{ screen, cwd? } \| null`: the saved screen and folder, once (reading removes it) |
| `pane.start` | write-local | `{ threadId, cols?, rows? }` → `{ ptyId }`: a pane's agent, its own interactive CLI with the thread's account, model and thinking, in a terminal that runs until the pane closes. Opening again it resumes its conversation: Claude Code, Pi and Grok Build on the session id Gond chose, Codex and Antigravity on the session the pane started (found on disk), opencode on the folder's latest |
| `pty.close` | write-local | `{ ptyId }` → `null` |
| `pty.open` | write-local | `{ threadId?, projectId?, shell?, cwd?, cols?, rows? }` (default 120×32) → `{ ptyId, shell? }`: a plain terminal in the thread's or project's folder, else your home folder; `shell` is a `shell.list` id (else Settings' pick), `cwd` a folder to start in instead; the answer names the shell it ran |
| `pty.read` | read | `{ ptyId, since?: offset, waitFor?: regex, timeoutMs? }` → `{ text, offset, truncated, exited, exitCode?, matched? }` |
| `pty.resize` | write-local | `{ ptyId, cols, rows }` → `null` |
| `pty.write` | write-local | `{ ptyId, data }` → `null` |
| `thread.archive` | write-local | `{ threadId }` → `null`: the pane closed; its session record is kept, archived |
| `thread.create` | write-local | `{ projectId, agentId, title?, accountId?, model?, thinking? }` → `Thread`: an agent pane's session record (`model`/`thinking` checked against `model.list`) |

## Agents

| Command | Risk | Input → result |
|---|---|---|
| `agent.custom.delete` | write-local | `{ id }` → `null` |
| `agent.custom.save` | write-local | `{ agent }` → `CustomAgent` |
| `agent.detect` | read | `{}` → `AgentInfo[]`: every built-in agent and saved custom agent, with whether its CLI is installed and its version |
| `harness.check` | read (fetches public version metadata) | `{}` → `{ id, installed?, latest?, updateAvailable, changelog }[]`: latest from the npm registry (`claude`, `codex`) or GitHub releases (`agy`); Grok publishes none, and offline means `latest` is absent |
| `harness.install` | **outward** | `{ harnessId, planIndex?, machineScope? }` → `{ jobId, steps: Step[] }`; prerequisites batched in front, one approval; runs in the background (no console) and reports into `/installs/{jobId}` |
| `harness.uninstall` | **destructive**, **human only** | `{ harnessId, keepData? }` → `{ jobId, steps }` at `/installs/{jobId}`: winget uninstall, the npm/Bun globals, the vendor's files, then its user PATH entries (a shared folder only once empty), never anything outside that harness's fixed list under the user's profile. `keepData` (default true) keeps settings and sign-ins; `false` also deletes them and the harness's accounts and vault secrets. `invalid` while any of its agents run, or when nothing Gond knows is installed. Done only when the program is no longer found; the last step's note reads "X is uninstalled." |
| `harness.uninstallPlan` | read | `{ harnessId, keepData? }` → `{ harnessId, methods: ("winget" \| "vendor" \| "npm" \| "bun")[], steps, deletes: string[], pathEntries: string[], keepData, running, accounts }`: exactly what `harness.uninstall` would remove, for the confirm card. `deletes` lists every file and folder; `keepData: false` adds the settings and sign-in dirs and Gond's account homes for that harness |
| `harness.update` | **outward** | `{ harnessId }` → `{ jobId, steps }`: updates the way it was installed; `invalid` when Gond can't tell |
| `install.cancel` | write-local | `{ jobId }` → `null`: stops a running install job and its current step's process tree; the job ends `cancelled`. `not_found` once finished |
| `model.list` | read | `{ harness, accountId? }` → `ModelInfo[]`: `{ id, label, provider, thinking: { levels: Level[], default?, via: "flag" \| "config" \| "modelSlug", note? } }`, `Level = "none" \| "minimal" \| "low" \| "medium" \| "high" \| "xhigh" \| "max" \| "ultra"`. Empty = the harness picks. With `accountId`, once its Test connection passed: the endpoint's own models (cached at `/endpointModels/{accountId}`), with catalog levels where the id is known and none otherwise |
| `prereq.install` | **outward** | `{ prereqs: string[], machineScope? }` → `{ jobId, steps }`: installs the missing ones (winget, silent, per-user). Without winget, or when its install fails, Git, GitHub CLI and Node come from their official releases. Gond downloads them with the same stages and bytes, checks the vendor's published SHA-256, then runs Git's installer with `/CURRENTUSER` or unpacks the GitHub CLI and Node zips under `%LOCALAPPDATA%\Programs` and puts them on the user PATH (plan `source: "download"`) |
| `prereq.list` | read | `{}` → `{ id: "git" \| "gh" \| "node" \| "bun", name, installed, version? }[]` |

## Accounts and usage

| Command | Risk | Input → result |
|---|---|---|
| `account.add` | write-local, **human only**; `/secret` and `/options/headers` redacted | `{ harness, label, kind: "oauth" \| "apiKey" \| "token" \| "gateway", secret?, endpoint?, options? }` → `Account`; the secret goes straight into Credential Manager. `options`: `{ bearer?, headers?: Record<string, string \| { secret: string }>, organization?, project?, azureApiVersion?, wireApi?, cloud? }`; a `{ secret }` header goes to the vault and reads back as `{ secret: true }`; **outward** when `endpoint` is set |
| `account.discover` | write-local | `{}` → how many were added: the agents' own sign-ins, found again (one you removed isn't added back) |
| `account.kinds` | read | `{ harness }` → `{ provider, kinds: AccountKind[], maxOauth? }`; agy has `maxOauth: 1` (one sign-in per Windows user) |
| `account.signIn` | write-local, **human only** | `{ accountId, cols?, rows? }` → `{ jobId?, ptyId }`: runs the harness's own login (`claude auth login`, `codex login`, `grok login`) **in the background** with the account's home (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `GROK_HOME`), so the sign-in lands there. `/signins/{jobId}` is `{ id, accountId, harness, status: "starting" \| "waiting" \| "needsCode" \| "signedIn" \| "failed" \| "cancelled", url?, code?, message?, ptyId }`: the sign-in URL and any device code read from its output, `needsCode` when it asks for a code back, then the harness's own status check (`claude auth status`, `codex login status`) decides `signedIn`. Ten minutes, then `failed`. The hidden terminal (`/terminals/{ptyId}`, `background: true`) is only a fallback to reveal. `invalid` for key accounts. agy keeps one sign-in per Windows user: there's no job, and `ptyId` is `agy` itself, run as you, to sign in at its own prompt |
| `account.signIn.cancel` | write-local, **human only** | `{ jobId }` → `null`: stops a running sign-in (`cancelled`); `invalid` once it has finished |
| `account.signIn.code` | write-local, **human only**; `/code` redacted | `{ jobId, code }` → `null`: types the code the browser showed into a `needsCode` sign-in |
| `account.test` | read for `{ accountId }`, **outward** for a draft; **human only**; `/secret` and `/options/headers` redacted | `{ accountId }` or the draft `{ harness, kind, endpoint?, secret?, options? }` → `{ ok, models?: string[], ms?, error? }`; lists the endpoint's models (Anthropic `/v1/models`, OpenAI/Azure `/models`, Google, xAI) with a 10 s limit; `error` is one plain sentence; sign-in, Bedrock and Vertex accounts return `ok: false` with why. The key goes to curl through a 0600 header file, never argv |
| `usage.refresh` | read | `{}` → `null`; reads every signed-in Claude, Codex, Grok and Antigravity account's plan limits into `/usage/{key}/windows` (`{ name, label?, percentLeft, resetsAt?, status? }`), kept across restarts |

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
