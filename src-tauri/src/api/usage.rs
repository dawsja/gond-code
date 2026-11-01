//! Plan limits for Settings → Usage: `/usage/default:<agent>` holds each agent's windows, read
//! by `usage.refresh` from the agent's own sign-in through its own usage source
//! (`crate::plan_usage`). The last reading is kept (settings key `planUsage`), so it's there
//! after a restart.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use super::registry::{Def, Registry, Risk};
use super::state::{Cause, StateTree};
use super::Services;
use crate::agents::UsageWindow;
use crate::error::AppResult;
use crate::store::{settings as repo, Store};

const KEY: &str = "planUsage";

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoInput {}

/// One reading at a time.
static REFRESHING: AtomicBool = AtomicBool::new(false);

/// The kept readings, one per agent; ones an account kept (by its id) are dropped.
fn load(store: &Store) -> BTreeMap<String, Value> {
    let all: BTreeMap<String, Value> = store
        .with(repo::all)
        .ok()
        .and_then(|all| all.into_iter().find(|(k, _)| k == KEY))
        .and_then(|(_, v)| serde_json::from_value(v).ok())
        .unwrap_or_default();
    all.into_iter()
        .filter(|(k, _)| k.starts_with("default:"))
        .collect()
}

/// Puts `key`'s windows in the tree, beside its spend, and keeps them.
pub fn record(tree: &StateTree, store: &Store, key: &str, harness: &str, windows: &[UsageWindow]) {
    let value = json!({
        "harness": harness, "windows": windows, "updatedAt": crate::store::now(),
    });
    let mut all = load(store);
    all.insert(key.to_string(), value.clone());
    let _ = store.with(|c| repo::set(c, KEY, &serde_json::to_value(&all).unwrap_or_default()));
    put(tree, key, value);
}

fn put(tree: &StateTree, key: &str, value: Value) {
    let key = key.to_string();
    tree.commit(
        Cause::Runtime {
            runtime: "usage".into(),
        },
        // Merged: the account's spend lives beside its windows (`api::spend`).
        move |d| {
            if !d["usage"][&key].is_object() {
                d["usage"][&key] = json!({});
            }
            for (k, v) in value.as_object().into_iter().flatten() {
                d["usage"][&key][k] = v.clone();
            }
        },
    );
}

/// Startup: the last readings, back in the tree.
pub fn sync(s: &Services) {
    for (key, value) in load(&s.store) {
        put(&s.state, &key, value);
    }
}

/// Reads each agent's plan limits from its own sign-in, in the background.
pub fn refresh(s: &Services, fetch: crate::http::Fetch, post: crate::http::Post) {
    if REFRESHING.swap(true, Ordering::SeqCst) {
        return;
    }
    let (tree, store) = (s.state.clone(), s.store.clone());
    std::thread::spawn(move || {
        let path = crate::process::path();
        let home = |var: &str, dir: &str| {
            std::env::var_os(var)
                .map(std::path::PathBuf::from)
                .or_else(|| dirs::home_dir().map(|h| h.join(dir)))
        };
        for agent in ["claude", "codex", "grok", "agy", "copilot", "cursor"] {
            let windows = match agent {
                "claude" => home("CLAUDE_CONFIG_DIR", ".claude")
                    .and_then(|c| crate::plan_usage::claude_token(&c))
                    .map(|t| crate::plan_usage::claude(&t, fetch))
                    .unwrap_or_default(),
                "codex" => crate::agents::detect::find("codex", path.clone())
                    .map(|p| crate::plan_usage::codex(&p, None))
                    .unwrap_or_default(),
                "grok" => home("GROK_HOME", ".grok")
                    .and_then(|h| crate::grok_usage::token(&h))
                    .map(|t| crate::grok_usage::read(&t, fetch))
                    .unwrap_or_default(),
                // One sign-in per Windows user (Credential Manager).
                "agy" => crate::plan_usage::agy_token()
                    .map(|t| crate::plan_usage::agy(&t, post))
                    .unwrap_or_default(),
                "copilot" => home("COPILOT_HOME", ".copilot")
                    .and_then(|h| crate::plan_usage::copilot_token(&h))
                    .map(|t| crate::plan_usage::copilot(&t, fetch))
                    .unwrap_or_default(),
                // One sign-in per Windows user (%APPDATA%\Cursor).
                "cursor" => dirs::data_dir()
                    .and_then(|r| crate::plan_usage::cursor_token(&r))
                    .map(|t| crate::plan_usage::cursor(&t, post))
                    .unwrap_or_default(),
                _ => vec![],
            };
            if !windows.is_empty() {
                record(&tree, &store, &format!("default:{agent}"), agent, &windows);
            }
        }
        REFRESHING.store(false, Ordering::SeqCst);
    });
}

pub fn register(r: &mut Registry<Services>) {
    r.add(
        Def::new(
            "usage.refresh",
            "Read each agent's plan limits (percent used, when it resets) from its own sign-in now",
            Risk::Read,
        )
        .human_only(),
        |call, _: NoInput| -> AppResult<()> {
            if cfg!(test) {
                refresh(
                    call.ctx,
                    |_, _| Err("no network in tests".into()),
                    |_, _, _| Err("no network in tests".into()),
                );
            } else {
                refresh(call.ctx, crate::http::curl, crate::http::curl_post);
            }
            Ok(())
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reading is kept: after a restart it's back in the tree.
    #[test]
    fn readings_survive_a_restart() {
        let s = Services::default();
        let w = UsageWindow::from_used("weekly", 0.08, Some(1_793_696_400), None);
        // One an account kept, by its id, from before accounts went.
        let old = json!({ "01OLD": { "harness": "claude", "windows": [] } });
        let mut all: BTreeMap<String, Value> = serde_json::from_value(old).unwrap();
        all.insert(
            "default:codex".into(),
            json!({ "harness": "codex", "windows": [] }),
        );
        s.store
            .with(|c| repo::set(c, KEY, &serde_json::to_value(&all).unwrap()))
            .unwrap();
        record(&s.state, &s.store, "default:claude", "claude", &[w]);
        let later = Services {
            state: StateTree::new(serde_json::json!({})),
            ..s.clone()
        };
        sync(&later);
        let v = later
            .state
            .get("/usage/default:claude/windows/0/percentLeft")
            .unwrap()
            .1;
        assert_eq!(v, 92.0);
        assert!(later.state.get("/usage/default:codex").is_some());
        assert!(
            later.state.get("/usage/01OLD").is_none(),
            "an account's reading is gone"
        );
    }
}
