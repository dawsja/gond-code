# Gond Code: the agents (harnesses)

What Gond knows about each built-in agent: how it's found, installed, updated and uninstalled,
how it signs in, how an account reaches it, and which models it offers. Each one runs in a pane
as its own interactive CLI (`runner.rs`); Gond passes no permission flags. Code:
`src-tauri/src/agents/` (one driver each), `harness.rs` (install and update plans),
`uninstall.rs`, `accounts.rs`, `agents/models.rs`.

Facts about the vendors' CLIs move fast; re-check a row when a CLI changes.

## 1. Summary

| Agent | Command | Install (in order of preference) | Update | Gond accounts | Pane flags |
|---|---|---|---|---|---|
| Claude Code | `claude` | winget `Anthropic.ClaudeCode` · `irm https://claude.ai/install.ps1 \| iex` · `bun add -g @anthropic-ai/claude-code` | `claude update` | sign-in, API key, setup-token, endpoint | `--session-id` / `--resume <id>`, `--model`, `--effort` |
| Codex | `codex` | winget `OpenAI.Codex` · `irm https://chatgpt.com/codex/install.ps1 \| iex` · `bun add -g @openai/codex` | `codex update` | sign-in, API key, endpoint | `-m <model>`, `resume <id>` |
| Antigravity | `agy` | `irm https://antigravity.google/cli/install.ps1 \| iex` | re-run the installer | one Google sign-in, API key, gateway | `--model <model>-<level>`, `--conversation <id>` |
| Grok Build | `grok` | winget `xAI.GrokBuild` · `irm https://x.ai/cli/install.ps1 \| iex` | `grok update` | sign-in, API key | `-s` / `-r <id>`, `-m`, `--effort` |
| opencode | `opencode` | winget `SST.opencode` · `bun add -g opencode-ai` | `opencode upgrade` | none (signs in itself) | `--continue` |
| Pi | `pi` | `bun add -g @earendil-works/pi-coding-agent` (Node 22.19+) | Bun | none (signs in itself, `/login`) | `--session-id <id>`, `--model`, `--thinking` |
| GitHub Copilot | `copilot` | winget `GitHub.Copilot` · `bun add -g @github/copilot` (Node) | winget or Bun, the way it was installed | sign-in, GitHub token, model endpoint | `--session-id <id>`, `--model`, `--reasoning-effort` |
| Cursor | `cursor-agent` | `irm https://cursor.com/install?win32=true \| iex` | `cursor-agent update` | one sign-in per Windows user, API key | `--resume <chat>`, `--model <model>-<level>` |
| Custom | any | — | — | — | the saved command and args |

Every built-in agent resumes its conversation when its pane opens again (`pane.start` in
`api/pty.rs`, `agents/sessions.rs`):
- **Claude Code** gets a fresh `--session-id` up front, and `--resume <id>` once the session's
  `.jsonl` exists under its config's `projects` folder.
- **Pi** and **GitHub Copilot** get `--session-id <id>`, which creates the session or opens
  it (Copilot's is under `~\.copilot\session-state\<id>`).
- **Cursor** names its own chat, so on a pane's first start Gond asks for an empty one
  (`cursor-agent create-chat`, in the pane's folder, as its account) and always starts with
  `--resume <id>`. If Cursor can't make one (signed out, offline), the pane starts fresh.
- **Grok Build** gets `-s <id>` to create it, then `-r <id>` once its `summary.json` is saved
  under `~\.grok\sessions`.
- **Codex** names its own session: Gond finds the rollout file the pane started
  (`~\.codex\sessions\YYYY\MM\DD\rollout-….jsonl`, in the pane's folder, begun after the pane,
  not taken by another pane), then runs `codex resume <id>`.
- **Antigravity** names its own too: Gond reads the folder's entry in
  `~\.gemini\antigravity-cli\cache\last_conversations.json`, then passes `--conversation <id>`.
- **opencode** takes no id: opening again, it gets `--continue` (the folder's latest session).

Custom agents run their saved command and args and start fresh. Codex's thinking level is not
passed to its interactive CLI.

## 2. Detection
`agents/detect.rs` looks each command up on PATH plus the known install folders
(`install_dirs`: `~\.local\bin`, `%LOCALAPPDATA%\agy\bin`, Bun's global bin and the rest) and
reads `--version`. It never errors: a missing or broken CLI is `installed: false`. PATH is
re-read from the registry after an install, so a new agent shows without a restart.

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
  its settings and sign-ins and Gond's accounts for it. Refused while one of its panes is open.
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

## 4. Sign-in and accounts
- An account belongs to one agent (`accounts.rs`): `oauth` (a sign-in), `apiKey`, `token` or
  `gateway`. Secrets live in Windows Credential Manager (`GondCode/…`) and reach the agent's
  process env only at spawn.
- **Your own sign-in:** at startup and when Settings opens, Gond finds each agent's existing
  login (`claude auth status`, Codex's `auth.json`, Copilot's `config.json`, `cursor-agent
  status`) and lists it as the default. A pane on it runs exactly as the CLI does on its own.
- **Extra sign-ins** run the agent's own login in the background (`account.signIn`:
  `claude auth login`, `codex login`, `grok login`, `copilot login`) inside a Gond-owned home under
  `%LOCALAPPDATA%\GondCode`, passed by env:

| Agent | Sign-in home | API key | Custom endpoint |
|---|---|---|---|
| Claude Code | `CLAUDE_CONFIG_DIR` | `ANTHROPIC_API_KEY`, or `ANTHROPIC_AUTH_TOKEN` (bearer); setup-token as `CLAUDE_CODE_OAUTH_TOKEN` | `ANTHROPIC_BASE_URL`, `ANTHROPIC_CUSTOM_HEADERS`; Bedrock/Vertex through their env |
| Codex | `CODEX_HOME` | a per-run provider `-c model_provider='gond'` with the key in `GOND_CODEX_API_KEY` | `model_providers.gond.base_url` (`chat` wire API), headers, Azure `api-version` |
| Antigravity | none: one Google sign-in per Windows user (its token is in Credential Manager) | `GEMINI_API_KEY` | `AGY_LLM_GATEWAY_URL` + `AGY_LLM_GATEWAY_API_KEY` |
| Grok Build | `GROK_HOME` | `XAI_API_KEY` | not mapped |
| GitHub Copilot | `COPILOT_HOME` (the GitHub token variables are unset so the sign-in wins) | a fine-grained token with Copilot Requests as `COPILOT_GITHUB_TOKEN` | its own provider (BYOK): `COPILOT_PROVIDER_BASE_URL`, the key as `_API_KEY` or `_BEARER_TOKEN`, `_TYPE` (openai or azure), `_WIRE_API`, `_HEADERS`; a key needs its endpoint |
| Cursor | none: one sign-in per Windows user (`%APPDATA%\Cursor\auth.json`, which only `APPDATA` moves) | `CURSOR_API_KEY` | not mapped |

- opencode (`opencode auth login`) and Pi (`/login`) sign in per provider inside their own CLI;
  Gond keeps no accounts for them.
- **Test connection** (`account.test`, `endpoint_test.rs`) lists an API account's models from
  its endpoint; those models then show in the picker.
- **Known limit:** an API key reaches the agent through its environment, so commands the agent
  runs can read it. Sign-in accounts put no key in the environment.

## 5. Models and thinking levels
Static lists in `agents/models.rs`, shown by `model.list`; the picker only offers levels the
model accepts. Levels: `none · minimal · low · medium · high · xhigh · max · ultra`.

| Agent | Models | Thinking |
|---|---|---|
| Claude Code | `opus`, `sonnet`, `haiku` (5.5, default medium), `fable` (Fable 5.1, default high), `opus[1m]`, `sonnet[1m]` | `--effort low…max` |
| Codex | GPT-6.1 Sol, GPT-6.1 Terra, GPT-6 Astra (up to `ultra`), GPT-6 Luna (up to `max`) | `model_reasoning_effort` (not passed to a pane yet) |
| Antigravity | Gemini 4 Argon, Gemini 3.8 / 3.7 / 3.6 Flash (low/medium/high), Gemini 3.1 Pro (low/high) | part of the model id: `<model>-<level>` |
| Grok Build | Grok 4.7, Grok 4.7 Fast, Grok 4.6 (low…xhigh), Grok 4.5 (low…high); default high | `--effort` |
| GitHub Copilot | Auto, Claude Opus / Sonnet / Haiku 5.5 and Fable 5.1, GPT-6.1 Sol, GPT-6 Astra and Luna, Gemini 4 Argon, Grok 4.7, Kimi K3 (its `model` setting, 1.0.105) | `--reasoning-effort`, each family's own levels; Gemini and Kimi at Copilot's default |
| Cursor | Auto, Composer 2.5, Claude Opus / Sonnet / Haiku 5.5 and Fable 5.1, GPT-5.6 Sol / Terra / Luna, Gemini 4 Argon, Grok 4.7, Kimi K3 (`cursor-agent models`, 2026.10.22) | part of the model id: `<model>-<level>`; unset, the level Cursor's plain name stands for |
| opencode, Pi, custom | none listed: the agent picks | none (Pi takes `--thinking` when a level is set) |

Google models run through Antigravity only; Gemini CLI is not supported (`check-no-gemini-cli.sh`).

## 6. Plan usage (Settings → Usage)
`usage.refresh` (`plan_usage.rs`) reads each signed-in account's limits the way the agent's own
`/usage` does, without running a turn; Gond never counts spend.

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
