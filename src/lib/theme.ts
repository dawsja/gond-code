import type { Tint } from "./types";

export const TINTS: readonly Tint[] = ["blue", "purple", "pink", "red", "orange", "amber", "green"];

/** Gond Code is dark only: <html> always carries `dark`, and the tint goes in `data-tint`. */
export function applyTint(tint: Tint = "blue", root = document.documentElement) {
  root.classList.add("dark");
  root.dataset.tint = tint;
}
