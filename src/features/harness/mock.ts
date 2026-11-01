import type { AccountKinds } from "@/features/accounts/AddAccount";
// The mock build's agents, models and accounts (Google models run through agy only).
import type { Account, Harness, ModelInfo, ThinkingSupport } from "@/lib/harnesses";

const t = (levels: ThinkingSupport["levels"], d: ThinkingSupport["default"]): ThinkingSupport => ({
  levels,
  default: d,
  how: "flag",
});

export const MOCK_HARNESSES: Harness[] = [
  {
    id: "claude",
    label: "Claude Code",
    brand: "claude-code",
    installed: true,
    version: "2.1.312",
    source: { kind: "npm", name: "@anthropic-ai/claude-code", verified: true },
    latest: "2.1.334",
    pinnable: true,
    changelog: [
      "Faster resume of long sessions",
      "New permission-prompt tool fields (Gond reads them)",
      "Fixes a Windows path bug with spaces",
    ],
    providers: ["anthropic"],
  },
  {
    id: "codex",
    label: "Codex",
    brand: "codex",
    installed: true,
    version: "0.166.0",
    source: { kind: "npm", name: "@openai/codex", verified: true },
    latest: "0.166.0",
    pinnable: true,
    providers: ["openai"],
  },
  {
    id: "grok",
    label: "Grok Build",
    brand: "grok",
    installed: true,
    version: "1.0.70",
    source: { kind: "vendor", name: "signed .exe", verified: true },
    latest: "1.0.74",
    pinnable: true,
    changelog: [
      "--agents for sub-agents",
      "Streaming JSON keeps tool results",
      "Lower memory on big repos",
    ],
    providers: ["xai"],
  },
  {
    id: "agy",
    label: "Antigravity",
    brand: "agy",
    installed: true,
    version: "1.4.10",
    source: { kind: "winget", name: "Google.Antigravity", verified: true },
    selfUpdating: true,
    providers: ["google"],
  },
];

export const MOCK_MODELS: Record<string, ModelInfo[]> = {
  claude: [
    {
      id: "opus-5.5",
      label: "Opus 5.5",
      provider: "claude",
      goodFor: "Best for hard, long tasks",
      isDefault: true,
      thinking: t(["none", "low", "medium", "high", "max"], "medium"),
    },
    {
      id: "sonnet-5.5",
      label: "Sonnet 5.5",
      provider: "claude",
      goodFor: "Fast, strong all-rounder",
      thinking: t(["none", "low", "medium", "high"], "medium"),
    },
    {
      id: "haiku-5.5",
      label: "Haiku 5.5",
      provider: "claude",
      goodFor: "Quick edits and reviews",
      thinking: null,
    },
  ],
  codex: [
    {
      id: "gpt-5.5-codex",
      label: "GPT-5.5 Codex",
      provider: "openai",
      goodFor: "Long agentic coding",
      isDefault: true,
      thinking: t(["minimal", "low", "medium", "high", "xhigh", "ultra"], "medium"),
    },
    {
      id: "gpt-5.5-mini",
      label: "GPT-5.5 mini",
      provider: "openai",
      goodFor: "Cheap, quick fixes",
      thinking: t(["minimal", "low", "medium"], "low"),
    },
  ],
  grok: [
    {
      id: "grok-5",
      label: "Grok 5",
      provider: "grok",
      goodFor: "Big refactors, fast",
      isDefault: true,
      thinking: t(["none", "low", "medium", "high", "max"], "low"),
    },
  ],
  agy: [
    {
      id: "gemini-4-argon",
      label: "Gemini 4 Argon",
      provider: "gemini",
      goodFor: "Huge context, UI work",
      isDefault: true,
      thinking: t(["low", "medium", "high", "max"], "high"),
    },
  ],
};

export const MOCK_ACCOUNTS: Account[] = [
  {
    id: "acc_a1",
    harness: "claude",
    provider: "anthropic",
    label: "Work",
    plan: "Max plan",
    identity: "d•••@studio",
    kind: "oauth",
    isDefault: true,
    usage: { kind: "unknown" },
  },
  {
    id: "acc_a3",
    harness: "claude",
    provider: "anthropic",
    label: "Personal",
    plan: "Pro plan",
    identity: "d•••@gmail",
    kind: "oauth",
    usage: { kind: "unknown" },
    limitedUntil: "22:15",
    seen: "Hit a limit at 17:15 · Gond retries after 22:15",
  },
  {
    id: "acc_a2",
    provider: "anthropic",
    label: "Team API",
    identity: "sk-ant-•••• 4f2c · org gond",
    kind: "apiKey",
    usage: { kind: "unknown" },
  },
  {
    id: "acc_o1",
    harness: "codex",
    provider: "openai",
    label: "ChatGPT Pro",
    identity: "d•••@gmail",
    kind: "oauth",
    isDefault: true,
    usage: { kind: "weekly", usedPercent: 41, resetsAt: "Mon" },
  },
  {
    id: "acc_o2",
    provider: "openai",
    label: "API",
    identity: "sk-•••• 9a1e",
    kind: "apiKey",
    usage: { kind: "unknown" },
  },
  {
    id: "acc_x1",
    provider: "xai",
    label: "Grok API",
    identity: "xai-•••• 77d0",
    kind: "apiKey",
    isDefault: true,
    usage: { kind: "unknown" },
  },
  {
    id: "acc_g1",
    harness: "agy",
    provider: "google",
    label: "Google account",
    identity: "d•••@gmail",
    kind: "oauth",
    isDefault: true,
    usage: { kind: "unknown" },
    expired: true,
  },
];

/** account.kinds per harness, until it's read live. */
export const MOCK_KINDS: Record<string, AccountKinds> = {
  claude: { provider: "anthropic", kinds: ["oauth", "apiKey", "token", "gateway"] },
  codex: { provider: "openai", kinds: ["oauth", "apiKey", "gateway"] },
  agy: { provider: "google", kinds: ["oauth"], maxOauth: 1 },
};

/** ?demo's Test connection: an endpoint that answers with its models, after a moment. */
export const mockAccountTest = (input: Record<string, unknown>) =>
  new Promise<{ ok: boolean; models?: string[]; ms?: number; error?: string }>((done) =>
    setTimeout(
      () =>
        done(
          String(input.secret ?? "").startsWith("bad")
            ? { ok: false, error: "The endpoint refused the key (401)." }
            : {
                ok: true,
                ms: 340,
                models:
                  input.harness === "codex"
                    ? ["gpt-5.5-codex", "gpt-5.5-mini", "o5"]
                    : ["claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5"],
              },
        ),
      400,
    ),
  );
