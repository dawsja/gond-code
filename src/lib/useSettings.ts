import { useEffect, useLayoutEffect, useState } from "react";
import { callOrRun, useAppState } from "./appState";
import { applyTheme, applyTint, normalizeTheme, readStoredTheme, watchSystemTheme } from "./theme";
import { setTerminalFont } from "./terminalFont";
import { notify } from "./notify";
import type { Settings } from "./types";

/** Rust's settings subtree is authoritative, including changes from other command callers.
 * The cache is used only until settings.get / the initial state snapshot arrives.
 */
export function useSettings() {
  const [settings, setSettings] = useState<Settings>(() => ({
    tint: "blue",
    theme: readStoredTheme(),
  }));
  const saved = useAppState((tree) => tree.settings as Settings | undefined);
  const current = saved ?? settings;
  const [version, setVersion] = useState<string>();
  useEffect(() => {
    let active = true;
    callOrRun("settings_get", "settings.get", {}).then(
      (s) => {
        if (active) setSettings(s);
      },
      () => undefined,
    );
    callOrRun("app_version", "app.version", {}).then(
      (v) => {
        if (active) setVersion(v);
      },
      () => undefined,
    );
    return () => {
      active = false;
    };
  }, []);
  const theme = normalizeTheme(current.theme);
  useLayoutEffect(() => {
    applyTheme(theme);
    return watchSystemTheme(theme);
  }, [theme]);
  useEffect(() => applyTint(current.tint), [current.tint]);
  useEffect(() => setTerminalFont(current.terminalFont), [current.terminalFont]);
  const change = (patch: Partial<Settings>) => {
    // Don't mirror a preference that failed to save. Native chrome and the page take the
    // successful command result/state patch, not an independent optimistic theme store.
    callOrRun("settings_set", "settings.set", { patch }).then(setSettings, () => {
      notify.error("Couldn't save settings.");
    });
  };
  return { settings: { ...current, theme }, version, change };
}
