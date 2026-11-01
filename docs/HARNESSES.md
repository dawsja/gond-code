# Gond Code: the agents (harnesses)

What Gond knows about each built-in agent: how it's found, installed, updated and uninstalled,
how Gond sees it running, and where its plan usage comes from. Gond never starts an agent
itself: every pane is a shell, and you start the agent by typing its command there, signed in
however the CLI already is. Code: `src-tauri/src/agents/` (`detect.rs`, `running.rs`),
`harness.rs` (install and update plans), `uninstall.rs`, `plan_usage.rs`.

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
own install folder (Cursor's `cursor-agent` is `…\cursor-agent\versions\…\node.exe`); a plain
`node.exe` from an npm install isn't.

## 3. Install, update, uninstall
- **Plans are data** (`harness.rs`): winget first where it exists, then the vendor's script,
  then the npm package through **`bun add -g`** (never npm). Packages that run on Node
  (`bun_needs_node`) get Node as a prerequisite in the same job.
- **Jobs** (`install.rs`): `harness.install` / `harness.update` run in the background with no
  console window, with progress (stages and bytes) at `/installs/{jobId}`, then PATH is
  refreshed. After a vendor script or a Bun install, Gond puts the program's folder on the user
  PATH (`~\.bun\bin`, `~\.local\bin`, `%LOCALAPPDATA%\cursor-agent`…) if it isn't already, so
  a terminal can run the agent too; winget does that itself. A running install shows Cancel (`install.cancel`). Without winget, Git, GitHub CLI
  and Node come from their vendors' releases (`fallback.rs`).
- **Update checks** (`harness.check`, `releases.rs`): the npm registry's `latest` tag, the
  vendor's GitHub releases, or the version Cursor's install script names, at start and every
  6 hours. An update installs only from the Update
  button, the way the agent was installed.
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

The Claude, Antigravity, Copilot and Cursor sources aren't documented; if they change, the bars keep their last
reading. An expired token means no reading until the agent renews it.
