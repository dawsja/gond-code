// Brand marks for models and agents, from LobeHub's icon set (@lobehub/icons-static-svg, MIT).
// One table: provider ids (companies, for models), agent ids (CLIs Gond Code runs) and lobehub's
// own keys all resolve here, so Rust can send any of them (AgentInfo.provider).
//
// `color`: use the -color SVG. Mono SVGs fill with currentColor, so on our near-black they take
// the text colour; that is what logos that are black by default (OpenAI, xAI, Grok, Ollama,
// Cursor, opencode, Copilot, Anthropic, Moonshot) use.

export type Brand = { name: string; icon: string; color: boolean };

const b = (name: string, icon: string, color = false): Brand => ({ name, icon, color });

export const BRANDS: Record<string, Brand> = {
  // Providers (who makes the model).
  anthropic: b("Anthropic", "anthropic"),
  openai: b("OpenAI", "openai"),
  xai: b("xAI", "xai"),
  deepseek: b("DeepSeek", "deepseek", true),
  minimax: b("MiniMax", "minimax", true),
  google: b("Google", "google", true),
  qwen: b("Qwen", "qwen", true),
  moonshot: b("Moonshot", "moonshot"),
  mistral: b("Mistral", "mistral", true),
  meta: b("Meta", "meta", true),
  zhipu: b("Zhipu", "zhipu", true),
  openrouter: b("OpenRouter", "openrouter"),
  ollama: b("Ollama", "ollama"),
  github: b("GitHub Copilot", "githubcopilot"),
  // GitHub itself (Settings → Connections), not Copilot.
  gh: b("GitHub", "github"),
  // MCP servers people add (Settings → MCP servers).
  figma: b("Figma", "figma", true),
  notion: b("Notion", "notion"),

  // Model families (what the model picker shows).
  claude: b("Claude", "claude", true),
  chatgpt: b("ChatGPT", "openai"),
  gpt: b("GPT", "openai"),
  gemini: b("Gemini", "gemini", true),
  grok: b("Grok", "grok"),
  kimi: b("Kimi", "kimi", true),

  // Agents (the CLIs a thread runs).
  // Lumen: Claude Code wears the Claude starburst, like the model.
  "claude-code": b("Claude Code", "claude", true),
  codex: b("Codex", "codex", true),
  agy: b("Antigravity", "antigravity", true),
  antigravity: b("Antigravity", "antigravity", true),
  "grok-build": b("Grok Build", "grok"),
  "qwen-code": b("Qwen Code", "qwen", true),
  "kimi-cli": b("Kimi CLI", "kimi", true),
  "cursor-agent": b("Cursor", "cursor"),
  cursor: b("Cursor", "cursor"),
  "copilot-cli": b("GitHub Copilot", "githubcopilot"),
  copilot: b("GitHub Copilot", "githubcopilot"),
  opencode: b("opencode", "opencode"),
  pi: b("Pi", "pi", true),
  // Shells (shell.list ids; a WSL distribution by its family, see shellBrand).
  pwsh: b("PowerShell 7", "powershellShell", true),
  powershell: b("Windows PowerShell", "powershellShell", true),
  cmd: b("Command Prompt", "cmdShell", true),
  gitbash: b("Git Bash", "gitShell", true),
  ubuntu: b("Ubuntu", "ubuntuShell", true),
  debian: b("Debian", "debianShell", true),
  linux: b("Linux", "linuxShell", true),
};

/**
 * The brand id for a shell (shell.list's `pwsh`, `cmd`, `wsl:Ubuntu-24.04`…): a WSL distribution
 * Ubuntu and Debian by name, Tux for the rest; no id (Default) is PowerShell.
 */
export function shellBrand(id: string | undefined): string {
  if (!id) return "powershell";
  if (!id.startsWith("wsl:")) return id;
  const d = id.slice(4).toLowerCase();
  return ["ubuntu", "debian"].find((f) => d.includes(f)) ?? "linux";
}

/** The brand for an id, case-insensitively; undefined for ids we don't know. */
export const brandOf = (id: string | undefined) => (id ? BRANDS[id.toLowerCase()] : undefined);

/** Two letters for an unknown id, from its words ("my-agent" → "MA", "aider" → "AI"). */
export function initials(label: string): string {
  const words = label.split(/[\s_\-./]+/).filter(Boolean);
  const letters =
    words.length > 1 ? words[0][0] + words[1][0] : label.replace(/[^a-z0-9]/gi, "").slice(0, 2);
  return letters.toUpperCase() || "?";
}
