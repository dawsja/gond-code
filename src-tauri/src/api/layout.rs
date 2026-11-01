//! The app's view: the sessions and their panes (`sessions.*`), and whether the workspace or
//! Settings is showing (`layout.focus`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::registry::{Def, Registry, Risk};
use super::state::Cause;
use super::Services;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Panel {
    /// The sessions' panes. Saved as `"chat"`, the name it had before the terminal workspace.
    Chat,
    Settings,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FocusInput {
    pub panel: Panel,
    /// Settings' section ("general", "usage", "harnesses", "connections").
    #[serde(default)]
    pub entity_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionsInput {
    /// Every session (a named group of panes) and the one you're in, as the app keeps them.
    pub sessions: Value,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScreenInput {
    pub pane_id: String,
    /// The terminal's screen and scrollback, as xterm serialises it (colours kept).
    pub screen: String,
    /// The folder its shell was in, read from its prompt.
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PaneInput {
    pub pane_id: String,
}

/// A shell pane's screen and folder, saved before a restart into an update.
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Screen {
    pub screen: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

/// Where a pane's saved screen is kept: `screens/<paneId>.json` beside the database.
fn screen_file(s: &Services, pane_id: &str) -> AppResult<Option<std::path::PathBuf>> {
    if pane_id.is_empty()
        || !pane_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(AppError::Invalid("a pane id is letters and digits".into()));
    }
    Ok(s.store
        .dir()
        .map(|d| d.join("screens").join(format!("{pane_id}.json"))))
}

/// Where the sessions are kept: beside the database (none for an in-memory store).
fn sessions_file(s: &Services) -> Option<std::path::PathBuf> {
    s.store.dir().map(|d| d.join("sessions.json"))
}

pub fn register(r: &mut Registry<Services>) {
    // The sessions and their panes, kept by the app itself rather than
    // the WebView's storage, which a crash or a forced close can lose.
    r.add(
        Def::new(
            "sessions.get",
            "Every session and its panes, as saved",
            Risk::Read,
        )
        .human_only(),
        |call, _: super::registry::NoInput| -> AppResult<Value> {
            let Some(f) = sessions_file(call.ctx) else {
                return Ok(Value::Null);
            };
            // Saved before sessions had their name: spaces.json, read until the next save.
            let old = f.with_file_name("spaces.json");
            Ok([f, old]
                .iter()
                .find_map(|f| std::fs::read_to_string(f).ok())
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or(Value::Null))
        },
    );
    r.add(
        Def::new(
            "sessions.set",
            "Save every session and its panes",
            Risk::WriteLocal,
        )
        .human_only(),
        |call, i: SessionsInput| -> AppResult<()> {
            if !i.sessions.is_object() {
                return Err(AppError::Invalid("sessions is an object".into()));
            }
            let Some(f) = sessions_file(call.ctx) else {
                return Ok(());
            };
            // Written whole, then swapped in, so a crash mid-write keeps the last good copy.
            let tmp = f.with_extension("json.tmp");
            std::fs::write(&tmp, i.sessions.to_string())?;
            std::fs::rename(&tmp, &f)?;
            let _ = std::fs::remove_file(f.with_file_name("spaces.json"));
            Ok(())
        },
    );
    // A shell pane comes back after an update showing what it showed, in the folder it was in.
    r.add(
        Def::new(
            "pane.screen.save",
            "Keep a shell pane's screen and folder until it opens again",
            Risk::WriteLocal,
        )
        .human_only(),
        |call, i: ScreenInput| -> AppResult<()> {
            let Some(f) = screen_file(call.ctx, &i.pane_id)? else {
                return Ok(());
            };
            if let Some(dir) = f.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let saved = Screen {
                screen: i.screen,
                cwd: i.cwd,
            };
            std::fs::write(f, serde_json::to_vec(&saved).unwrap_or_default())?;
            Ok(())
        },
    );
    r.add(
        Def::new(
            "pane.screen.take",
            "A shell pane's saved screen and folder, once: reading it removes it",
            Risk::WriteLocal,
        )
        .human_only(),
        |call, i: PaneInput| -> AppResult<Option<Screen>> {
            let Some(f) = screen_file(call.ctx, &i.pane_id)? else {
                return Ok(None);
            };
            let Ok(bytes) = std::fs::read(&f) else {
                return Ok(None);
            };
            let _ = std::fs::remove_file(&f);
            Ok(serde_json::from_slice(&bytes).ok())
        },
    );
    r.add(
        Def::new(
            "layout.focus",
            "Show the workspace, or a section of Settings",
            // It changes only what's on screen, never the work, so it reads as a read.
            Risk::Read,
        )
        .human_only(),
        |call, i: FocusInput| -> AppResult<()> {
            let focus = strip_null(json!({ "panel": i.panel, "entityId": i.entity_id }));
            let cause = Cause::Command {
                event_id: None,
                command: "layout.focus".into(),
                actor: call.actor.clone(),
            };
            call.ctx
                .state
                .commit(cause, |d| d["layout"]["focus"] = focus.clone());
            Ok(())
        },
    );
}

fn strip_null(mut v: Value) -> Value {
    if let Some(o) = v.as_object_mut() {
        o.retain(|_, x| !x.is_null());
    }
    v
}

#[cfg(test)]
mod tests {
    use crate::api::{registry, Actor, ActorKind, Services};
    use serde_json::json;

    fn agent() -> Actor {
        Actor {
            kind: ActorKind::Agent,
            id: "agent:T1".into(),
            label: "@api".into(),
            thread_id: Some("T1".into()),
            session_id: None,
        }
    }

    /// The sessions and their panes come back as saved, from beside the database; the agents
    /// can't touch them.
    #[test]
    fn sessions_are_kept_by_the_app() {
        let dir = tempfile::tempdir().unwrap();
        let s = Services {
            store: crate::store::Store::open(&dir.path().join("gond-code.db")).unwrap(),
            ..Services::default()
        };
        let (r, h) = (registry(), Actor::human());
        assert_eq!(
            r.run(&s, &h, "sessions.get", json!({})).unwrap(),
            json!(null)
        );
        // What the build that called them spaces saved still opens.
        let old = json!({ "groups": [{ "id": "g0", "name": "Old", "space": { "root": null } }] });
        std::fs::write(dir.path().join("spaces.json"), old.to_string()).unwrap();
        assert_eq!(r.run(&s, &h, "sessions.get", json!({})).unwrap(), old);
        let sessions =
            json!({ "sessions": [{ "id": "g1", "name": "Main", "layout": { "root": null } }] });
        r.run(&s, &h, "sessions.set", json!({ "sessions": sessions }))
            .unwrap();
        assert_eq!(r.run(&s, &h, "sessions.get", json!({})).unwrap(), sessions);
        assert!(r
            .run(&s, &h, "sessions.set", json!({ "sessions": [] }))
            .is_err());
        assert!(r.run(&s, &agent(), "sessions.get", json!({})).is_err());
    }

    #[test]
    fn focus_shows_the_workspace_or_settings() {
        let (r, s) = (registry(), Services::default());
        r.run(
            &s,
            &Actor::human(),
            "layout.focus",
            json!({ "panel": "settings", "entityId": "usage" }),
        )
        .unwrap();
        assert_eq!(
            s.state.get("/layout/focus").unwrap().1,
            json!({ "panel": "settings", "entityId": "usage" })
        );
        r.run(
            &s,
            &Actor::human(),
            "layout.focus",
            json!({ "panel": "chat" }),
        )
        .unwrap();
        assert_eq!(
            s.state.get("/layout/focus").unwrap().1,
            json!({ "panel": "chat" })
        );
        assert_eq!(
            r.run(
                &s,
                &Actor::human(),
                "layout.focus",
                json!({ "panel": "diff" })
            )
            .unwrap_err()
            .code(),
            "invalid_input"
        );
        assert!(r
            .run(&s, &agent(), "layout.focus", json!({ "panel": "chat" }))
            .is_err());
    }

    /// A shell's screen comes back once, after the restart, and never again.
    #[test]
    fn a_saved_screen_is_taken_once() {
        let dir = tempfile::tempdir().unwrap();
        let s = Services {
            store: crate::store::Store::open(&dir.path().join("gond-code.db")).unwrap(),
            ..Services::default()
        };
        let (r, h) = (registry(), Actor::human());
        let saved = json!({ "paneId": "p1", "screen": "\u{1b}[32mok\u{1b}[0m", "cwd": "C:\\src" });
        r.run(&s, &h, "pane.screen.save", saved).unwrap();
        let back = r
            .run(&s, &h, "pane.screen.take", json!({ "paneId": "p1" }))
            .unwrap();
        assert_eq!(back["screen"], "\u{1b}[32mok\u{1b}[0m");
        assert_eq!(back["cwd"], "C:\\src");
        let again = r
            .run(&s, &h, "pane.screen.take", json!({ "paneId": "p1" }))
            .unwrap();
        assert!(again.is_null());
        assert!(r
            .run(&s, &h, "pane.screen.take", json!({ "paneId": "../x" }))
            .is_err());
    }
}
