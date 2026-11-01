// The mock build's agents, and the plan windows their own sign-ins report.
import type { Harness } from "@/lib/harnesses";
import type { PlanUsage } from "@/lib/usage";

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
  },
  {
    id: "agy",
    label: "Antigravity",
    brand: "agy",
    installed: true,
    version: "1.4.10",
    source: { kind: "winget", name: "Google.Antigravity", verified: true },
    selfUpdating: true,
  },
];

/** `/usage` on the mock build: what Settings → Usage shows there. */
export const MOCK_USAGE: Record<string, PlanUsage> = {
  "default:claude": {
    harness: "claude",
    windows: [
      { name: "session", percentLeft: 62, resetsAt: "2026-11-01T17:40:00Z" },
      { name: "weekly", percentLeft: 81, resetsAt: "2026-11-05T09:00:00Z" },
    ],
    updatedAt: "2026-11-01T12:00:00Z",
  },
  "default:codex": {
    harness: "codex",
    windows: [{ name: "weekly", percentLeft: 44, resetsAt: "2026-11-04T09:00:00Z" }],
    updatedAt: "2026-11-01T12:00:00Z",
  },
};
