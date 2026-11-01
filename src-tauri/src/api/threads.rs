//! `thread.*` behind the registry: each pane's agent is a thread. Handlers
//! call `crate::threads`, then mirror `/threads` in the state tree: unarchived threads by id.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::Deserialize;

use super::registry::{Call, Def, Registry, Risk};
use super::state::Cause;
use super::Services;
use crate::error::AppResult;
use crate::store::threads::{self as repo, Thread};
use crate::threads::{self, NewThread};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateInput {
    pub project_id: String,
    pub agent_id: String,
    /// Defaults to the agent's name.
    pub title: Option<String>,
    /// Run as this account; omitted = the harness's own sign-in.
    pub account_id: Option<String>,
    /// A model from `model.list`; omitted = the harness default.
    pub model: Option<String>,
    /// A thinking level the model takes (`model.list`); needs `model`.
    pub thinking: Option<crate::agents::driver::Level>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThreadInput {
    pub thread_id: String,
}

fn cause(call: &Call<Services>, command: &str) -> Cause {
    Cause::Command {
        event_id: None,
        command: command.into(),
        actor: call.actor.clone(),
    }
}

/// Rewrites `/threads` from the store: every unarchived thread, by id.
pub fn sync_threads(services: &Services, cause: Cause) -> AppResult<()> {
    let all = services.store.with(repo::list_unarchived)?;
    let map: BTreeMap<String, Thread> = all.into_iter().map(|t| (t.id.clone(), t)).collect();
    let value = serde_json::to_value(map).unwrap_or_default();
    services.state.commit(cause, |d| d["threads"] = value);
    Ok(())
}

pub fn register(r: &mut Registry<Services>) {
    r.add(
        Def::new(
            "thread.create",
            "Save a pane's agent: which agent, its account, model and thinking level, in the project's folder",
            Risk::WriteLocal,
        ),
        |call, i: CreateInput| -> AppResult<Thread> {
            let t = threads::create(
                &call.ctx.store,
                NewThread {
                    project_id: i.project_id,
                    agent_id: i.agent_id,
                    title: i.title,
                    account_id: i.account_id,
                    model: i.model,
                    thinking: i.thinking,
                },
            )?;
            sync_threads(call.ctx, cause(call, "thread.create"))?;
            Ok(t)
        },
    );
    r.add(
        Def::new(
            "thread.archive",
            "Close a thread (its pane closed): its terminals end",
            Risk::WriteLocal,
        ),
        |call, i: ThreadInput| -> AppResult<()> {
            let s = call.ctx;
            s.ptys.close_thread(&i.thread_id);
            let owner = i.thread_id.clone();
            s.state.commit(cause(call, "thread.archive"), move |d| {
                if let Some(m) = d["terminals"].as_object_mut() {
                    m.retain(|_, t| t["owner"] != owner.as_str());
                }
            });
            threads::archive(&s.store, &i.thread_id)?;
            sync_threads(s, cause(call, "thread.archive"))
        },
    );
}
