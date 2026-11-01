//! Plan limits for Accounts and Settings → Usage: `/usage/{key}` holds each account's windows
//! (key: the account's id, or `default:<agent>` for its own sign-in). They
//! come from turns (Claude's rate-limit events, Codex's notifications, Grok's billing) and from
//! `usage.refresh`, which asks each agent's own usage source (`crate::plan_usage`). The last
//! reading is kept (settings key `planUsage`), so it's there after a restart.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};

use super::registry::{Def, Registry, Risk};
use super::state::{Cause, StateTree};
use super::Services;
use crate::accounts::{Account, AccountKind};
use crate::agents::UsageWindow;
use crate::error::AppResult;
use crate::store::{settings as repo, Store};

const KEY: &str = "planUsage";

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoInput {}

/// One reading at a time.
static REFRESHING: AtomicBool = AtomicBool::new(false);

fn load(store: &Store) -> BTreeMap<String, Value> {
    store
        .with(repo::all)
        .ok()
        .and_then(|all| all.into_iter().find(|(k, _)| k == KEY))
        .and_then(|(_, v)| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

/// Puts `key`'s windows in the tree, beside its spend, and keeps them.
pub fn record(
    tree: &StateTree,
    store: &Store,
    key: &str,
    harness: &str,
    account_id: Option<&str>,
    windows: &[UsageWindow],
) {
    let value = json!({
        "harness": harness, "accountId": account_id,
        "windows": windows, "updatedAt": crate::store::now(),
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

/// The usage key an account's readings go under.
fn key_of(a: &Account) -> String {
    if a.is_own() {
        format!("default:{}", a.harness)
    } else {
        a.id.clone()
    }
}

/// Reads every signed-in account's plan limits, in the background.
pub fn refresh(s: &Services, fetch: crate::endpoint_test::Fetch, post: crate::endpoint_test::Post) {
    if REFRESHING.swap(true, Ordering::SeqCst) {
        return;
    }
    let (tree, store) = (s.state.clone(), s.store.clone());
    std::thread::spawn(move || {
        let accounts = crate::accounts::list(&store, None, None).unwrap_or_default();
        let path = crate::process::path();
        for a in accounts.iter().filter(|a| a.kind == AccountKind::Oauth) {
            let home = a.config_dir.as_deref().map(std::path::PathBuf::from);
            let windows = match a.harness.as_str() {
                "claude" => home
                    .or_else(|| std::env::var_os("CLAUDE_CONFIG_DIR").map(Into::into))
                    .or_else(|| dirs::home_dir().map(|h| h.join(".claude")))
                    .and_then(|c| crate::plan_usage::claude_token(&c))
                    .map(|t| crate::plan_usage::claude(&t, fetch))
                    .unwrap_or_default(),
                "codex" => crate::agents::detect::find("codex", path.clone())
                    .map(|p| crate::plan_usage::codex(&p, home.as_deref()))
                    .unwrap_or_default(),
                "grok" => home
                    .or_else(|| std::env::var_os("GROK_HOME").map(Into::into))
                    .or_else(|| dirs::home_dir().map(|h| h.join(".grok")))
                    .and_then(|h| crate::grok_usage::token(&h))
                    .map(|t| crate::grok_usage::read(&t, fetch))
                    .unwrap_or_default(),
                // One sign-in per Windows user (Credential Manager).
                "agy" => crate::plan_usage::agy_token()
                    .map(|t| crate::plan_usage::agy(&t, post))
                    .unwrap_or_default(),
                _ => vec![],
            };
            if !windows.is_empty() {
                record(&tree, &store, &key_of(a), &a.harness, Some(&a.id), &windows);
            }
        }
        REFRESHING.store(false, Ordering::SeqCst);
    });
}

pub fn register(r: &mut Registry<Services>) {
    r.add(
        Def::new(
            "usage.refresh",
            "Read every signed-in account's plan limits (percent used, when it resets) now",
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
                refresh(
                    call.ctx,
                    crate::endpoint_test::curl,
                    crate::endpoint_test::curl_post,
                );
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
        record(&s.state, &s.store, "default:claude", "claude", None, &[w]);
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
    }
}
