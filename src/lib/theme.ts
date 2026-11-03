import type { Theme, Tint } from "./types";

export const THEMES = ["system", "light", "dark"] as const;
export const DARK_QUERY = "(prefers-color-scheme: dark)";
export const THEME_STORAGE_KEY = "gond-code-theme";
export const TINT_STORAGE_KEY = "gond-code-tint";
export const TINTS: readonly Tint[] = ["blue", "purple", "pink", "red", "orange", "amber", "green"];

export function isTheme(value: unknown): value is Theme {
  return (THEMES as readonly unknown[]).includes(value);
}

export const normalizeTheme = (value: unknown): Theme => (isTheme(value) ? value : "system");

/** Only a first-paint mirror. Rust settings replace this as soon as they load. */
export function readStoredTheme(): Theme {
  try {
    return normalizeTheme(localStorage.getItem(THEME_STORAGE_KEY));
  } catch {
    return "system";
  }
}

export function resolveTheme(
  theme: Theme,
  dark = window.matchMedia(DARK_QUERY).matches,
): "light" | "dark" {
  return theme === "system" ? (dark ? "dark" : "light") : theme;
}

export function applyTheme(theme: Theme, root = document.documentElement) {
  const resolved = resolveTheme(theme);
  root.classList.toggle("dark", resolved === "dark");
  root.dataset.theme = theme;
  root.style.colorScheme = resolved;
  try {
    localStorage.setItem(THEME_STORAGE_KEY, theme);
  } catch {
    /* Storage can be disabled; the saved Rust setting still works. */
  }
  window.dispatchEvent(new Event("gond:theme"));
}

/** One listener per settings owner; explicit choices ignore device changes. */
export function watchSystemTheme(theme: Theme) {
  const media = window.matchMedia(DARK_QUERY);
  const changed = () => {
    if (theme === "system") applyTheme(theme);
  };
  media.addEventListener("change", changed);
  return () => media.removeEventListener("change", changed);
}

export function applyTint(tint: Tint = "blue", root = document.documentElement) {
  root.dataset.tint = tint;
  try {
    localStorage.setItem(TINT_STORAGE_KEY, tint);
  } catch {
    /* First-paint caching is optional. */
  }
}
