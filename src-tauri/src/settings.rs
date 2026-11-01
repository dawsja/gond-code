//! Typed app settings over the raw `settings` table. Keys are stored camelCase,
//! as the UI sends them. The app is dark only, so there is no theme setting.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::{AppError, AppResult};
use crate::store::{settings as repo, Store};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Tint {
    Blue,
    Green,
    Purple,
    Pink,
    Amber,
    Orange,
    Red,
}

/// The terminals' font: one of the Nerd Fonts the app ships, nothing custom.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum TerminalFont {
    #[default]
    JetbrainsMono,
    Hack,
    FiraCode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub tint: Tint,
    /// The window's X hides Gond Code to the tray, so its agents keep working; Quit is in
    /// the tray menu. Off: X quits.
    #[serde(default = "yes")]
    pub close_to_tray: bool,
    /// A plain terminal's shell (`shells::list` ids: `pwsh`, `cmd`, `wsl:Ubuntu`…); none =
    /// PowerShell 7, else Windows PowerShell, else Command Prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    /// The terminals' font (not the app's).
    #[serde(default)]
    pub terminal_font: TerminalFont,
}

/// What `settings.set` takes: every key typed, nothing else (the exported schema has
/// `additionalProperties: false`, so a stray key fails the UI's contract test too).
/// `null` clears an optional key.
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Patch {
    #[serde(default)]
    pub tint: Option<Tint>,
    #[serde(default)]
    pub close_to_tray: Option<bool>,
    #[serde(default, deserialize_with = "nullable")]
    pub shell: Option<Option<String>>,
    #[serde(default)]
    pub terminal_font: Option<TerminalFont>,
}

/// A present key, which may be `null` (clear it): `Some(None)`.
fn nullable<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Some(Option::deserialize(d)?))
}

impl Patch {
    /// The patch as the stored keys `set` merges.
    pub fn into_map(self) -> Map<String, Value> {
        let mut m = Map::new();
        if let Some(t) = self.tint {
            m.insert("tint".into(), serde_json::to_value(t).unwrap_or_default());
        }
        if let Some(on) = self.close_to_tray {
            m.insert("closeToTray".into(), Value::from(on));
        }
        if let Some(f) = self.terminal_font {
            m.insert(
                "terminalFont".into(),
                serde_json::to_value(f).unwrap_or_default(),
            );
        }
        if let Some(shell) = self.shell {
            m.insert("shell".into(), shell.map_or(Value::Null, Value::from));
        }
        m
    }
}

fn yes() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            tint: Tint::Blue,
            close_to_tray: true,
            shell: None,
            terminal_font: TerminalFont::default(),
        }
    }
}

/// Every key `Settings` has, as the UI spells it.
const KEYS: &[&str] = &["tint", "closeToTray", "shell", "terminalFont"];

fn as_object(s: &Settings) -> Map<String, Value> {
    match serde_json::to_value(s) {
        Ok(Value::Object(m)) => m,
        _ => unreachable!("Settings serialises to an object"),
    }
}

fn from_object(m: Map<String, Value>) -> Result<Settings, serde_json::Error> {
    serde_json::from_value(Value::Object(m))
}

/// Defaults overlaid with what is stored. Stored values that no longer fit are ignored.
pub fn get(store: &Store) -> AppResult<Settings> {
    let mut merged = as_object(&Settings::default());
    for (key, value) in store.with(repo::all)? {
        let mut candidate = merged.clone();
        candidate.insert(key, value);
        if from_object(candidate.clone()).is_ok() {
            merged = candidate;
        }
    }
    Ok(from_object(merged).unwrap_or_default())
}

/// Merges `patch` into the current settings. Unknown keys or bad values are
/// `invalid` and nothing is saved. `null` clears an optional key.
pub fn set(store: &Store, patch: Map<String, Value>) -> AppResult<Settings> {
    if let Some(key) = patch.keys().find(|k| !KEYS.contains(&k.as_str())) {
        return Err(AppError::Invalid(format!("settings: unknown key `{key}`")));
    }
    let mut merged = as_object(&get(store)?);
    for (key, value) in &patch {
        if value.is_null() {
            merged.remove(key);
        } else {
            merged.insert(key.clone(), value.clone());
        }
    }
    let next = from_object(merged).map_err(|e| AppError::Invalid(format!("settings: {e}")))?;
    store.with(|c| {
        let tx = c.unchecked_transaction()?;
        for (key, value) in &patch {
            repo::set(&tx, key, value)?;
        }
        tx.commit()?;
        Ok(())
    })?;
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn patch(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn defaults() {
        let store = Store::open_in_memory().unwrap();
        let s = get(&store).unwrap();
        assert_eq!(s, Settings::default());
        assert_eq!(
            serde_json::to_value(&s).unwrap(),
            json!({ "tint": "blue", "closeToTray": true, "terminalFont": "jetbrains-mono" })
        );
    }

    #[test]
    fn patch_merges_and_persists() {
        let store = Store::open_in_memory().unwrap();
        set(&store, patch(json!({ "shell": "cmd" }))).unwrap();
        let s = set(
            &store,
            patch(json!({ "tint": "green", "closeToTray": false, "terminalFont": "hack" })),
        )
        .unwrap();
        assert_eq!(s.terminal_font, TerminalFont::Hack);
        assert_eq!(s.shell.as_deref(), Some("cmd"));
        assert_eq!(s.tint, Tint::Green);
        assert!(!s.close_to_tray);
        assert_eq!(get(&store).unwrap(), s);

        let s = set(&store, patch(json!({ "shell": null }))).unwrap();
        assert_eq!(s.shell, None);
        assert_eq!(get(&store).unwrap().shell, None);
    }

    #[test]
    fn rejects_unknown_keys_and_bad_values_without_saving() {
        let store = Store::open_in_memory().unwrap();
        for bad in [
            json!({ "tint": "green", "fontSize": 14 }),
            json!({ "theme": "dark" }),
            json!({ "theme": null }),
            json!({ "tint": "teal" }),
            json!({ "tint": null }),
            json!({ "shell": 3 }),
            json!({ "terminalFont": "comic-sans" }),
        ] {
            let err = set(&store, patch(bad.clone())).unwrap_err();
            assert_eq!(err.code(), "invalid", "{bad}");
        }
        assert_eq!(get(&store).unwrap(), Settings::default());
    }

    #[test]
    fn keys_match_the_struct() {
        let mut all = as_object(&Settings {
            shell: Some(String::new()),
            ..Settings::default()
        })
        .keys()
        .cloned()
        .collect::<Vec<_>>();
        all.sort();
        let mut keys = KEYS.to_vec();
        keys.sort();
        assert_eq!(all, keys);
    }

    #[test]
    fn migration_drops_a_stored_theme() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gond-code.db");
        // A database as a pre-release build left it: schema 1 with a theme row.
        {
            let c = rusqlite::Connection::open(&path).unwrap();
            c.execute_batch(include_str!("store/migrations/0001_init.sql"))
                .unwrap();
            c.execute_batch(
                "CREATE TABLE schema_version (version INTEGER NOT NULL);
                 INSERT INTO schema_version VALUES (1);",
            )
            .unwrap();
            repo::set(&c, "theme", &json!("light")).unwrap();
            repo::set(&c, "tint", &json!("pink")).unwrap();
        }
        let store = Store::open(&path).unwrap();
        let keys: Vec<String> = store
            .with(repo::all)
            .unwrap()
            .into_iter()
            .map(|(k, _)| k)
            .collect();
        assert_eq!(keys, ["tint"]);
        assert_eq!(get(&store).unwrap().tint, Tint::Pink);
    }
}
