# Gond Code: the agents (harnesses)

What Gond knows about each built-in agent: how it's found, installed, updated and uninstalled,
how Gond sees it running, and where its plan usage comes from. Gond never starts an agent
itself: every pane is a shell, and you start the agent by typing its command there, signed in
however the CLI already is.

Each agent is a file in [`agents/`](../agents/README.md) (`agents/kimi.toml`): its install
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
| GitHub Copilot | `copilot` | winget `GitHub.Copilot` · `bun add -g @github/copilot` (Node) | winget or Bun, the way it was installed |
| Cursor | `cursor-agent` | `irm https://cursor.com/install?win32=true \| iex` | `cursor-agent update` |
| Droid (Factory) | `droid` | `irm https://app.factory.ai/cli/windows \| iex` · `bun add -g droid` (Node) | `droid update`, or Bun |
| Kimi Code | `kimi` | `irm https://code.kimi.ai/kimi-code/install.ps1 \| iex` · winget `MoonshotAI.KimiCodeCLI` · `bun add -g @moonshot-ai/kimi-code` (Node 22.19+); needs Git (its shell is Git Bash) | `kimi upgrade --yes`, or winget or Bun |
| MiniMax Code | `mcode` | `irm https://filecdn.minimax.chat/public/install.ps1 \| iex` (it brings its own Node when this PC's won't do) | `mcode update` |
| Hermes Agent | `hermes` | `& ([scriptblock]::Create((irm https://hermes-agent.nousresearch.com/install.ps1))) -NonInteractive` | `hermes update` |
| Kilo Code | `kilo` | winget `KiloCode.Kilo` · `bun add -g @kilocode/cli` (Node) | winget or Bun |
| Devin | `devin` | winget `CognitionAI.DevinCLI` · `bun add -g devin` (Node); needs Git | winget or Bun |
| Cline | `cline` | `bun add -g cline` (Node) | Bun, after its background hub is stopped |
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
own install folder (Cursor's `cursor-agent` is `…\cursor-agent\versions\…\node.exe`), or runs the
agent's script (MiniMax Code's `mcode` is this PC's Node on `…\.minimax-code\…\@minimax-ai\code`);
a plain `node.exe` from an npm install isn't. A Bun install's version is read from its
`package.json` and Hermes' from its install stamp: `cline --version` takes 8 s.

## 3. Install, update, uninstall
- **Plans are data** (the agent's `[install]`, `harness.rs`): winget first where it exists, then the vendor's script,
  then the npm package through **`bun add -g`** (never npm). Packages that run on Node
  (`npm_needs_node`) get Node as a prerequisite in the same job.
- **Jobs** (`install.rs`): `harness.install` / `harness.update` run in the background with no
  console window, with progress (stages and bytes) at `/installs/{jobId}`, then PATH is
  refreshed. After a vendor script or a Bun install, Gond puts the program's folder on the user
  PATH (`~\.bun\bin`, `~\.local\bin`, `%LOCALAPPDATA%\cursor-agent`…) if it isn't already, so
  a terminal can run the agent too; winget does that itself. A running install shows Cancel (`install.cancel`). Without winget, Git, GitHub CLI
  and Node come from their vendors' releases (`fallback.rs`).
- **Update checks** (`harness.check`, `releases.rs`): the npm registry's `latest` tag, the
  vendor's GitHub releases, or the version Cursor's install script names, at start and every
  6 hours, for the agents that are installed (one that isn't is never looked up). An update
  installs only from the Update button, the way the agent was installed. Droid's and Kimi
  Code's latest version is a plain-text file of theirs (`downloads.factory.ai/factory-cli/LATEST`,
  `code.kimi.ai/kimi-code/latest`); Hermes' its GitHub release; the rest the npm registry.
- A vendor script runs with Windows' own `System32` first on PATH, so its `tar.exe` and
  `curl.exe` are Windows' (Git's GNU tar takes `C:\…` for a host). Hermes' script runs with
  `-NonInteractive`, so it doesn't stop at its setup wizard. Devin's script isn't used: it ends
  in `devin setup`, which waits for you. Kimi Code's script comes before winget: winget's package
  downloads from `code.kimi.com`, which many networks can't reach.
- Cline keeps a hub running in the background (`cline.exe --cline-hub-daemon`), which holds its
  files: an update or uninstall stops it first. An npm package that ships its program as a
  per-platform package (Droid, Kilo Code, Devin, Cline) has that package removed with it.
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
| GitHub Copilot | removed by winget or Bun | `~\.copilot` |
| Cursor | `%LOCALAPPDATA%\cursor-agent`, `%LOCALAPPDATA%\cursor-compile-cache` | `~\.cursor\cli-config.json`, `%APPDATA%\Cursor\auth.json` (both folders are shared with the Cursor editor, so only the CLI's own files go) |
| Droid | `~\bin\droid.exe` (`~\bin` leaves PATH only once nothing else is in it), or Bun's | `~\.factory` |
| Kimi Code | `~\.kimi-code\bin`, or winget's or Bun's | `~\.kimi-code` |
| MiniMax Code | `~\.minimax-code` | `~\.minimax` (shared with MiniMax's desktop app) |
| Hermes Agent | `%LOCALAPPDATA%\hermes\{bin,hermes-agent,tools,installs}` | `%LOCALAPPDATA%\hermes` |
| Kilo Code | removed by winget or Bun | `~\.config\kilo`, `~\.local\share\kilo`, `~\.local\state\kilo`, `~\.cache\kilo` |
| Devin | removed by winget or Bun | `%APPDATA%\devin`, `%LOCALAPPDATA%\devin` |
| Cline | removed by Bun | `~\.cline` |

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
| GitHub Copilot | `GET api.github.com/copilot_internal/user` with the sign-in's GitHub token (Credential Manager, `<host>:<login>.copilot-cli`, written as UTF-16): the month's AI credits, and chat and completions where the plan limits them |
| Cursor | `DashboardService/GetCurrentPeriodUsage` on `api2.cursor.sh` with the sign-in's access token: the billing cycle's included usage and its Auto and API shares, weekly or monthly as the cycle is |
| Kimi Code | `GET api.kimi.ai/coding/v1/usages` (`api.kimi.com` for mainland China, by `~\.kimi-code\region`) with `~\.kimi-code\credentials\kimi-code*.json`'s token: the 5 hours, the week and the month the plan has |
| MiniMax Code | `GET platform.minimax.io/v1/api/openplatform/coding_plan/remains` with `~\.minimax\auth\prod\en\mcode-public\auth.json`'s token: the 5-hour interval and the week |
| Hermes Agent | `GET portal.nousresearch.com/api/oauth/account` with the Nous Portal sign-in in `%LOCALAPPDATA%\hermes\auth.json`: the subscription's gauges (none without one) |

Droid's sign-in is encrypted with a key in Credential Manager, Devin's has no plan endpoint, and
Kilo Code's and Cline's report a credit balance rather than a plan's limits: no card for them.

The Claude, Antigravity, Copilot and Cursor sources aren't documented; if they change, the bars keep their last
reading. An expired token means no reading until the agent renews it.
