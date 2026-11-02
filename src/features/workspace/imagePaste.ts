import { statusSpec } from "@/lib/agentStatus";

/**
 * Agent CLIs paste images themselves: on their image-paste key they read the picture off the
 * clipboard and attach it ("[Image #1]"). A paste whose clipboard has no text (a screenshot,
 * a copied image) sends that key instead, as a terminal that leaves image pastes to the
 * program does. A shell has no such key: there, an image-only paste does nothing.
 */
export function imagePasteKey(agentId: string | undefined): string | undefined {
  if (!agentId) return;
  // Its file's `[status] paste`: Alt+V for Claude Code (its Ctrl+V is for WSL); Ctrl+V for the
  // rest.
  return statusSpec(agentId).paste === "alt+v" ? "\x1bv" : "\x16";
}
