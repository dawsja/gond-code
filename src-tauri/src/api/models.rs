//! `model.list`: a harness's models and the thinking levels each takes, for the
//! model and thinking pickers.

use schemars::JsonSchema;
use serde::Deserialize;

use super::registry::{Def, Registry, Risk};
use super::Services;
use crate::agents::models::{ModelInfo, ThinkingSupport, ThinkingVia};
use crate::error::AppResult;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HarnessInput {
    /// An agent id: "claude", "codex", or a custom agent's id.
    pub harness: String,
    /// An API account whose Test connection listed its endpoint's models: those come
    /// back instead, with the catalog's thinking levels where the id is known.
    pub account_id: Option<String>,
}

pub fn register(r: &mut Registry<Services>) {
    r.add(
        Def::new(
            "model.list",
            "A harness's models and the thinking levels each one takes",
            Risk::Read,
        ),
        |call, i: HarnessInput| -> AppResult<Vec<ModelInfo>> {
            let catalog = crate::runner::driver_for(&call.ctx.store, &i.harness)?.models();
            let served = i.account_id.and_then(|id| {
                let (_, v) = call.ctx.state.get(&format!("/endpointModels/{id}"))?;
                serde_json::from_value::<Vec<String>>(v).ok()
            });
            let Some(served) = served else {
                return Ok(catalog);
            };
            let provider = crate::accounts::rules(&i.harness).map_or("custom", |r| r.provider);
            Ok(served
                .into_iter()
                .map(|id| {
                    catalog
                        .iter()
                        .find(|m| m.id == id)
                        .cloned()
                        .unwrap_or(ModelInfo {
                            label: id.clone(),
                            id,
                            provider: provider.into(),
                            thinking: ThinkingSupport {
                                levels: vec![],
                                default: None,
                                via: ThinkingVia::Flag,
                                note: None,
                            },
                            context_limit: None,
                        })
                })
                .collect())
        },
    );
}

#[cfg(test)]
mod tests {
    use crate::api::{registry, Actor, Services};
    use serde_json::json;

    #[test]
    fn lists_models_with_levels() {
        let (r, s, h) = (registry(), Services::default(), Actor::human());
        let m = r
            .run(&s, &h, "model.list", json!({ "harness": "codex" }))
            .unwrap();
        let luna = m
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == "gpt-6-luna")
            .unwrap();
        assert_eq!(
            luna["thinking"]["levels"],
            json!(["low", "medium", "high", "xhigh", "max"])
        );
        let e = r
            .run(&s, &h, "model.list", json!({ "harness": "nope" }))
            .unwrap_err();
        assert_eq!(e.code(), "agent_not_found");
    }
}
