// The SVG text of every brand icon we use, in one chunk loaded on first use (see ProviderIcon),
// so the ~30 logos never weigh on startup.
import anthropic from "@lobehub/icons-static-svg/icons/anthropic.svg?raw";
import claude from "@lobehub/icons-static-svg/icons/claude-color.svg?raw";
import codex from "@lobehub/icons-static-svg/icons/codex-color.svg?raw";
import cursor from "@lobehub/icons-static-svg/icons/cursor.svg?raw";
import deepseek from "@lobehub/icons-static-svg/icons/deepseek-color.svg?raw";
import gemini from "@lobehub/icons-static-svg/icons/gemini-color.svg?raw";
import antigravity from "@lobehub/icons-static-svg/icons/antigravity-color.svg?raw";
import antigravityMono from "@lobehub/icons-static-svg/icons/antigravity.svg?raw";
import figma from "@lobehub/icons-static-svg/icons/figma-color.svg?raw";
import figmaMono from "@lobehub/icons-static-svg/icons/figma.svg?raw";
import github from "@lobehub/icons-static-svg/icons/github.svg?raw";
import notion from "@lobehub/icons-static-svg/icons/notion.svg?raw";
import githubcopilot from "@lobehub/icons-static-svg/icons/githubcopilot.svg?raw";
import google from "@lobehub/icons-static-svg/icons/google-color.svg?raw";
import grok from "@lobehub/icons-static-svg/icons/grok.svg?raw";
import kimi from "@lobehub/icons-static-svg/icons/kimi-color.svg?raw";
import meta from "@lobehub/icons-static-svg/icons/meta-color.svg?raw";
import minimax from "@lobehub/icons-static-svg/icons/minimax-color.svg?raw";
import mistral from "@lobehub/icons-static-svg/icons/mistral-color.svg?raw";
import moonshot from "@lobehub/icons-static-svg/icons/moonshot.svg?raw";
import ollama from "@lobehub/icons-static-svg/icons/ollama.svg?raw";
import openai from "@lobehub/icons-static-svg/icons/openai.svg?raw";
// Pi (pi.dev): its own logo from pi.dev/logo-auto.svg; LobeHub's "Pi" is another product.
import pi from "./pi-color.svg?raw";
import piMono from "./pi.svg?raw";
import opencode from "@lobehub/icons-static-svg/icons/opencode.svg?raw";
import openrouter from "@lobehub/icons-static-svg/icons/openrouter.svg?raw";
import qwen from "@lobehub/icons-static-svg/icons/qwen-color.svg?raw";
import xai from "@lobehub/icons-static-svg/icons/xai.svg?raw";
import zhipu from "@lobehub/icons-static-svg/icons/zhipu-color.svg?raw";

// The shells a plain terminal runs (Settings → Default shell): PowerShell's own logo, a drawn
// Command Prompt, and simple-icons' shapes (CC0) in each brand's colour.
import powershellShell from "./shells/powershell.svg?raw";
import cmdShell from "./shells/cmd.svg?raw";
import gitShell from "./shells/git.svg?raw";
import ubuntuShell from "./shells/ubuntu.svg?raw";
import debianShell from "./shells/debian.svg?raw";
import linuxShell from "./shells/linux.svg?raw";

// Mono versions of the colour logos, for `variant="mono"`.
import claudeMono from "@lobehub/icons-static-svg/icons/claude.svg?raw";
import codexMono from "@lobehub/icons-static-svg/icons/codex.svg?raw";
import deepseekMono from "@lobehub/icons-static-svg/icons/deepseek.svg?raw";
import geminiMono from "@lobehub/icons-static-svg/icons/gemini.svg?raw";
import googleMono from "@lobehub/icons-static-svg/icons/google.svg?raw";
import kimiMono from "@lobehub/icons-static-svg/icons/kimi.svg?raw";
import metaMono from "@lobehub/icons-static-svg/icons/meta.svg?raw";
import minimaxMono from "@lobehub/icons-static-svg/icons/minimax.svg?raw";
import mistralMono from "@lobehub/icons-static-svg/icons/mistral.svg?raw";
import qwenMono from "@lobehub/icons-static-svg/icons/qwen.svg?raw";
import zhipuMono from "@lobehub/icons-static-svg/icons/zhipu.svg?raw";

export const COLOR: Record<string, string> = {
  powershellShell,
  cmdShell,
  gitShell,
  ubuntuShell,
  debianShell,
  linuxShell,
  pi,
  figma,
  antigravity,
  claude,
  codex,
  deepseek,
  gemini,
  google,
  kimi,
  meta,
  minimax,
  mistral,
  qwen,
  zhipu,
};

export const MONO: Record<string, string> = {
  // The shells have no mono logo: they draw in colour either way.
  powershellShell,
  cmdShell,
  gitShell,
  ubuntuShell,
  debianShell,
  linuxShell,
  anthropic,
  antigravity: antigravityMono,
  cursor,
  figma: figmaMono,
  notion,
  github,
  githubcopilot,
  grok,
  moonshot,
  ollama,
  openai,
  opencode,
  openrouter,
  xai,
  claude: claudeMono,
  pi: piMono,
  codex: codexMono,
  deepseek: deepseekMono,
  gemini: geminiMono,
  google: googleMono,
  kimi: kimiMono,
  meta: metaMono,
  minimax: minimaxMono,
  mistral: mistralMono,
  qwen: qwenMono,
  zhipu: zhipuMono,
};
