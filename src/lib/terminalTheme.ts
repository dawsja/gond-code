import type { ITheme } from "@xterm/xterm";

/** A single palette for workspace and sign-in terminals. Truecolour escapes pass through.
 * Keep the original dark palette unchanged; light ANSI colours use darker ink on linen.
 */
export const TERMINAL_THEMES = {
  dark: {
    background: "#000000",
    foreground: "#e6e6e6",
    cursor: "#e6e6e6",
    cursorAccent: "#000000",
    selectionBackground: "#2a3b5c",
    selectionInactiveBackground: "#1d2638",
    black: "#1c1c1c",
    red: "#f28b82",
    green: "#a5d6a7",
    yellow: "#ffd479",
    blue: "#82b1ff",
    magenta: "#d7a6f5",
    cyan: "#80deea",
    white: "#d0d0d0",
    brightBlack: "#6b6b6b",
    brightRed: "#ffaba3",
    brightGreen: "#c5ecc6",
    brightYellow: "#ffe6a6",
    brightBlue: "#b0cbff",
    brightMagenta: "#e8c6fb",
    brightCyan: "#b4f0f7",
    brightWhite: "#ffffff",
  },
  light: {
    background: "#f5f3ee",
    foreground: "#1b1a18",
    cursor: "#1b1a18",
    cursorAccent: "#f5f3ee",
    selectionBackground: "#c8d7ed",
    selectionInactiveBackground: "#dddcd7",
    black: "#1b1a18",
    red: "#c2301a",
    green: "#1a7d38",
    yellow: "#8a5a00",
    blue: "#2266c4",
    magenta: "#923eaf",
    cyan: "#147580",
    white: "#6b6863",
    brightBlack: "#524f4a",
    brightRed: "#b53535",
    brightGreen: "#19863f",
    brightYellow: "#9e670d",
    brightBlue: "#2f6fd6",
    brightMagenta: "#b8307f",
    brightCyan: "#20838c",
    brightWhite: "#77736c",
  },
} satisfies Record<"dark" | "light", ITheme>;

/** applyTheme resolves System into the html dark class; data-theme also supports explicit modes. */
export function terminalTheme(root = document.documentElement): ITheme {
  return TERMINAL_THEMES[
    root.classList.contains("dark") || root.dataset.theme === "dark" ? "dark" : "light"
  ];
}

/** Update xterm's options in place: never remount the terminal or disturb its PTY/scrollback.
 * The event is dispatched by applyTheme; observing html also covers pre-paint/external changes.
 */
export function onTerminalTheme(change: (theme: ITheme) => void, root = document.documentElement) {
  let previous: ITheme | undefined;
  const update = () => {
    const theme = terminalTheme(root);
    if (theme !== previous) {
      previous = theme;
      change(theme);
    }
  };
  const observer = new MutationObserver(update);
  observer.observe(root, { attributes: true, attributeFilter: ["class", "data-theme"] });
  window.addEventListener("gond:theme", update);
  update();
  return () => {
    observer.disconnect();
    window.removeEventListener("gond:theme", update);
  };
}
