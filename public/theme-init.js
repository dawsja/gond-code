// First-paint mirror only; Rust settings win once loaded. Kept in step by the theme structure guard.
(function () {
  const root = document.documentElement;
  let theme = "system";
  let tint = "blue";
  try {
    theme = localStorage.getItem("gond-code-theme");
    tint = localStorage.getItem("gond-code-tint");
  } catch {
    /* Storage can be disabled. */
  }
  if (["system", "light", "dark"].indexOf(theme) < 0) theme = "system";
  const dark =
    theme === "dark" || (theme === "system" && matchMedia("(prefers-color-scheme: dark)").matches);
  root.classList.toggle("dark", dark);
  root.dataset.theme = theme;
  root.style.colorScheme = dark ? "dark" : "light";
  if (["blue", "purple", "pink", "red", "orange", "amber", "green"].indexOf(tint) >= 0)
    root.dataset.tint = tint;
})();
