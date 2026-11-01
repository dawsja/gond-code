import "@/styles/terminal-fonts.css";
import type { TerminalFont } from "./types";

// The terminals' font (Settings → General), separate from the app's. Deck sets it from the
// settings; every mounted terminal follows it.

export const TERMINAL_FONTS: { id: TerminalFont; label: string; family: string }[] = [
  { id: "jetbrains-mono", label: "JetBrains Mono Nerd Font", family: "JetBrainsMono Nerd Font" },
  { id: "hack", label: "Hack Nerd Font", family: "Hack Nerd Font" },
  { id: "fira-code", label: "Fira Code Nerd Font", family: "FiraCode Nerd Font" },
];

const DEFAULT: TerminalFont = "jetbrains-mono";

/** `font`'s CSS family, with fallbacks for a file that didn't load. */
export function fontStack(font: TerminalFont = DEFAULT) {
  const f = TERMINAL_FONTS.find((t) => t.id === font) ?? TERMINAL_FONTS[0];
  return `"${f.family}", "Cascadia Mono", Consolas, monospace`;
}

let current: TerminalFont = DEFAULT;
const listeners = new Set<() => void>();

export function setTerminalFont(font: TerminalFont = DEFAULT) {
  if (font === current) return;
  current = font;
  listeners.forEach((l) => l());
}

export const terminalFont = () => current;

export function onTerminalFont(l: () => void) {
  listeners.add(l);
  return () => void listeners.delete(l);
}

/** Resolves once `font`'s regular and bold faces are in (or straight away where fonts can't be
 *  waited on); a terminal measures its cells from them. */
export function loadTerminalFont(font: TerminalFont) {
  const family = TERMINAL_FONTS.find((t) => t.id === font)?.family ?? TERMINAL_FONTS[0].family;
  if (!document.fonts) return Promise.resolve();
  return Promise.all([
    document.fonts.load(`13px "${family}"`),
    document.fonts.load(`bold 13px "${family}"`),
  ]).then(
    () => undefined,
    () => undefined,
  );
}
