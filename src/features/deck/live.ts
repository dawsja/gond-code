/** Dev only, stripped from release builds: `?demo` shows sample data. */
export const DEMO = import.meta.env.DEV && new URLSearchParams(window.location.search).has("demo");
