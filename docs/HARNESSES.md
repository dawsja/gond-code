# Gond Code: the agents (harnesses)

What Gond knows about each built-in agent: how it's found, installed, updated and uninstalled,
how Gond sees it running, and where its plan usage comes from. Gond never starts an agent
itself: every pane is a shell, and you start the agent by typing its command there, signed in
however the CLI already is.

Each agent is a file in [`agents/`](../agents/README.md) (`agents/claude.toml`): its install
ways, update source, folders, uninstall footprint, resume store, prompts and paste key. When a
CLI changes, its file changes; this page describes what the files say. The machinery reading
them: `src-tauri/src/agents/` (`spec.rs`, `detect.rs`, `running.rs`, `resume.rs`), `harness.rs`
(install and update plans), `releases.rs`, `uninstall.rs`, `plan_usage.rs`.

Facts about the vendors' CLIs move fast; re-check a row when a CLI changes.

## 1. Summary

| Agent | Command | Install (in order of preference) | Update |
|---|---|---|---|
| Claude Code | `claude` | winget `Anthropic.ClaudeCode` · `irm https://claude.ai/install.ps1 \| iex` · `bun add -g @anthropic-ai/claude-code` | `claude update` |
| Codex | `codex` | winget `OpenAI.Codex` · `irm https://chatgpt.com/codex/install.ps1 \| iex` · `bun add -g @openai/codex` | `codex update` |
| Antigravity | `agy` | `irm https://antigravity.google/cli/install.ps1 \| iex` | re-run the installer |
| Grok Build | `grok` | winget `xAI.GrokBuild` · `irm https://x.ai/cli/install.ps1 \| iex` | `grok update` |
| opencode | `opencode` | winget `SST.opencode` · `bun add -g opencode-ai` | `opencode upgrade` |
| Pi | `pi` | `bun add -g @earendil-works/pi-coding-agent` (Node 22.19+) | Bun |
| Devin | `devin` | winget `CognitionAI.DevinCLI` · `bun add -g devin` (Node); needs Git | winget or Bun |
| Custom | any | — | — |

Google models run through Antigravity only; Gemini CLI is not supported (`check-no-gemini-cli.sh`).

Gond passes no flags: an agent runs exactly as it does when you type it in any terminal. To pick
a conversation back up after a restart, use the agent's own resume (`claude --continue`,
`codex resume`, …).

## 2. Detection
`agents/detect.rs` looks each command up on PATH plus the known install folders
(`install_dirs`: `~\.local\bin`, `%LOCALAPPDATA%\agy\bin`, Bun's global bin and the rest) and
reads `--version`. It never errors: a missing or broken CLI is `installed: false`. PATH is
re-read from the registry after an install, so a new agent shows without a restart.

**Running in a pane** (`agents/running.rs`, `pty.agent`): every 2 s a pane asks which agent runs
under its shell, found in the shell's process tree by the program's name (`claude.exe`, or a
winget package's `codex-x86_64-pc-windows-msvc.exe`). The pane then shows the agent and watches
its status, the sidebar lists it, and Ctrl+Shift+V with a picture sends the agent its own
image-paste key. An agent that runs as a script is seen when its runtime runs from the agent's
own install folder (`…\<command>\…\node.exe`); a plain `node.exe` from an npm install isn't. A
Bun install's version is read from its `package.json`.

**Status** (`status.ts`, the agent's `[status]`): Working, Needs you, Idle, from the agent's own
word first.

| Agent | How its status is read | Image paste |
|---|---|---|
| Claude Code | its title (a spinner while working, `✳` at its input); its prompts at the bottom of the pane | Alt+V |
| Codex | its title (a spinner, "Action Required"); its prompts at the bottom of the pane | Ctrl+V |
| Antigravity | its OSC 7501 report (`AGY_CLI_OSC7501=on` in every pane: its own support check can't be answered through Windows' console host) | Ctrl+V |
| Grok Build | its title (a spinner, "⚠ Action Required", `grok` when idle) | Alt+V |
| opencode | its footer's "esc interrupt" while working; its "△ Permission required" dialog | Ctrl+V |
| Pi | its spinner line ("⠋ Working") while working; it asks for no approvals | Alt+V |
| Devin | its OSC 7501 report (it needs `TERM_PROGRAM`, which every pane sets) | Ctrl+V |

## 3. Install, update, uninstall
- **Plans are data** (the agent's `[install]`, `harness.rs`): winget first where it exists, then
  the vendor's script, then the npm package through **`bun add -g`** (never npm). Packages that
  run on Node (`npm_needs_node`) get Node as a prerequisite in the same job.
- **Jobs** (`install.rs`): `harness.install` / `harness.update` run in the background with no
  console window, with progress (stages and bytes) at `/installs/{jobId}`, then PATH is
  refreshed. After a vendor script or a Bun install, Gond puts the program's folder on the user
  PATH (`~\.bun\bin`, `~\.local\bin`…) if it isn't already, so a terminal can run the agent too;
  winget does that itself. A running install shows Cancel (`install.cancel`). Without winget,
  Git, GitHub CLI and Node come from their vendors' releases (`fallback.rs`).
- **Update checks** (`harness.check`, `releases.rs`): the npm registry's `latest` tag, the
  vendor's GitHub releases, or a plain-text file holding the version (Grok's
  `x.ai/cli/stable`), as the agent's `[update] latest` says, at start and every 6 hours, for the
  agents that are installed (one that isn't is never looked up). An update installs only from
  the Update button, the way the agent was installed.
- A vendor script runs with Windows' own `System32` first on PATH, so its `tar.exe` and
  `curl.exe` are Windows' (Git's GNU tar takes `C:\…` for a host). Devin's script isn't used: it
  ends in `devin setup`, which waits for you.
- An npm package that ships its program as a per-platform package (Devin) has that package
  removed with it.
- **Uninstall** (`uninstall.rs`, `harness.uninstall`, human only): every way it was installed
  (winget, npm/Bun globals, the vendor's files), its PATH entries, and with "Remove everything"
  its settings and sign-ins. Refused while it runs in a pane.
  What each removes:

| Agent | Program files | Settings and sign-ins (Remove everything) |
|---|---|---|
| Claude Code | `~\.local\bin\claude.exe`, `~\.local\share\claude`, `~\.local\state\claude` | `~\.claude`, `~\.claude.json` |
| Codex | `%LOCALAPPDATA%\Programs\OpenAI\Codex`, `~\.codex\packages`, `~\.codex\tmp` | `~\.codex` |
| Antigravity | `%LOCALAPPDATA%\agy` | `~\.gemini\antigravity-cli` |
| Grok Build | `~\.grok\{bin,completions,docs,downloads}`, `%LOCALAPPDATA%\grok` | `~\.grok` |
| opencode | removed by winget or Bun | `~\.config\opencode`, `~\.local\share\opencode`, `~\.local\state\opencode`, `~\.cache\opencode` |
| Pi | removed by Bun | `~\.pi\agent` |
| Devin | removed by winget or Bun | `%APPDATA%\devin`, `%LOCALAPPDATA%\devin` |

## 4. Plan usage (Settings → Usage)
`usage.refresh` (`plan_usage.rs`) reads each agent's limits from its own sign-in, the way the
agent's own `/usage` does, without running a turn; Gond never counts spend. Settings → Usage
shows one card per installed agent.

| Agent | Source |
|---|---|
| Claude Code | `GET api.anthropic.com/api/oauth/usage` with the sign-in's token (5-hour, weekly, weekly per model) |
| Codex | its app server's `account/rateLimits/read` |
| Grok Build | the CLI proxy's `/billing` (monthly credits, weekly share), `grok_usage.rs` |
| Antigravity | Google's Cloud Code API (`retrieveUserQuotaSummary`, `retrieveUserQuota`) with agy's sign-in |
| Devin | Windsurf's `SeatManagementService/GetUserStatus` on the sign-in's `api_server_url` (`server.codeium.com` by default) with `%APPDATA%\devin\credentials.toml`'s `windsurf_api_key`, as JSON, naming itself `devin-cli` with the installed version (without one the server answers 400): the day and the week, unless the plan hides them |

opencode and Pi have no plan of their own to read: no card for them.

The Claude, Antigravity and Devin sources aren't documented; if they change, the bars keep their
last reading. An expired token means no reading until the agent renews it.
