/**
 * Agent CLIs paste images themselves: on their image-paste key they read the picture off the
 * clipboard and attach it ("[Image #1]"). A paste whose clipboard has no text (a screenshot,
 * a copied image) sends that key instead, so it works as it does in Alacritty or Windows
 * Terminal. A shell has no such key: there, an image-only paste does nothing.
 */
export function imagePasteKey(agentId: string | undefined): string | undefined {
  if (!agentId) return;
  // Claude Code on Windows takes Alt+V (Ctrl+V is its key only under WSL).
  if (agentId === "claude") return "\x1bv";
  // Ctrl+V: Codex and the rest.
  return "\x16";
}
