//! Models and their thinking levels per harness. The picker
//! shows only what this says a model accepts, so a turn never asks for a level the
//! model would reject. Static for Claude Code and Codex, ;
//! harnesses that can list their own (agy `-p "/effort"`, Codex app-server) probe later.

use schemars::JsonSchema;
use serde::Serialize;

use super::driver::Level;
use crate::error::{AppError, AppResult};

/// How the harness takes the level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ThinkingVia {
    /// A command-line flag (Claude `--effort`, Codex `-c model_reasoning_effort`).
    Flag,
    /// Written into the account's Gond-owned config for the run (Kimi).
    Config,
    /// Part of the model id (Cursor).
    ModelSlug,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingSupport {
    pub levels: Vec<Level>,
    /// The harness's own default for this model, named in the picker; `None` = unknown.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Level>,
    pub via: ThinkingVia,
    /// A note the picker shows beside a level, e.g. Codex's `ultra`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    /// What goes to the harness (`--model` / `-m`).
    pub id: String,
    pub label: String,
    /// Brand icon key for the model family.
    pub provider: String,
    pub thinking: ThinkingSupport,
    /// The context window in tokens, where it's known; the Context meter shows
    /// tokens only without it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_limit: Option<u64>,
}

// Not a glob: `Level::None` would shadow `Option::None`.
use Level::{High, Low, Max, Medium, Ultra, Xhigh};

const FIVE: &[Level] = &[Low, Medium, High, Xhigh, Max];

fn model(id: &str, label: &str, provider: &str, thinking: ThinkingSupport) -> ModelInfo {
    ModelInfo {
        id: id.into(),
        label: label.into(),
        provider: provider.into(),
        thinking,
        context_limit: None,
    }
}

/// `model`, with a context window that's known for certain.
fn sized(m: ModelInfo, tokens: u64) -> ModelInfo {
    ModelInfo {
        context_limit: Some(tokens),
        ..m
    }
}

fn flag(levels: &[Level], default: Option<Level>) -> ThinkingSupport {
    ThinkingSupport {
        levels: levels.to_vec(),
        default,
        via: ThinkingVia::Flag,
        note: None,
    }
}

/// Claude Code's aliases: all five levels on the 5.x and Fable
/// models; `medium` is the default on 5.5, `high` on Fable.
pub fn claude() -> Vec<ModelInfo> {
    vec![
        model("opus", "Opus 5.5", "claude", flag(FIVE, Some(Medium))),
        model("sonnet", "Sonnet 5.5", "claude", flag(FIVE, Some(Medium))),
        model("haiku", "Haiku 5.5", "claude", flag(FIVE, Some(Medium))),
        model("fable", "Fable 5.1", "claude", flag(FIVE, Some(High))),
        sized(
            model(
                "opus[1m]",
                "Opus 5.5 (1M context)",
                "claude",
                flag(FIVE, Some(Medium)),
            ),
            1_000_000,
        ),
        sized(
            model(
                "sonnet[1m]",
                "Sonnet 5.5 (1M context)",
                "claude",
                flag(FIVE, Some(Medium)),
            ),
            1_000_000,
        ),
    ]
}

/// Codex's models: `low` to `max`, plus `ultra`, which splits the
/// work across sub-agents. Luna stops at `max`. Defaults aren't published.
pub fn codex() -> Vec<ModelInfo> {
    let ultra = || ThinkingSupport {
        note: Some("ultra splits the task across parallel sub-agents".into()),
        ..flag(&[Low, Medium, High, Xhigh, Max, Ultra], None)
    };
    vec![
        model("gpt-6.1-sol", "GPT-6.1 Sol", "openai", ultra()),
        model("gpt-6.1-terra", "GPT-6.1 Terra", "openai", ultra()),
        model("gpt-6-astra", "GPT-6 Astra", "openai", ultra()),
        model("gpt-6-luna", "GPT-6 Luna", "openai", flag(FIVE, None)),
    ]
}

/// agy's models, as `agy models` prints them (recorded on Windows, agy 1.4.10, API-key
/// mode): the effort is part of the id (`gemini-4-argon-high`), so the level goes in the
/// model slug. Argon and the 3.x Flash models take low/medium/high, 3.1 Pro low/high.
/// The Claude models agy serves to Google sign-ins don't show in API-key mode.
pub fn agy() -> Vec<ModelInfo> {
    let slug = |levels: &[Level], default| ThinkingSupport {
        via: ThinkingVia::ModelSlug,
        ..flag(levels, default)
    };
    let lmh = |default| slug(&[Low, Medium, High], default);
    vec![
        model(
            "gemini-4-argon",
            "Gemini 4 Argon",
            "gemini",
            lmh(Some(Medium)),
        ),
        model(
            "gemini-3.8-flash",
            "Gemini 3.8 Flash",
            "gemini",
            lmh(Some(Medium)),
        ),
        model(
            "gemini-3.7-flash",
            "Gemini 3.7 Flash",
            "gemini",
            lmh(Some(Medium)),
        ),
        model(
            "gemini-3.6-flash",
            "Gemini 3.6 Flash",
            "gemini",
            lmh(Some(Medium)),
        ),
        model(
            "gemini-3.1-pro",
            "Gemini 3.1 Pro",
            "gemini",
            slug(&[Low, High], Some(Low)),
        ),
    ]
}

/// The id agy takes for `model` at `level`: `<model>-<level>`, the model's default level
/// when none is picked. Unknown models pass through unchanged.
pub fn agy_slug(model: &str, level: Option<Level>) -> String {
    let default = agy()
        .into_iter()
        .find(|m| m.id == model)
        .and_then(|m| m.thinking.default);
    match level.or(default) {
        Some(l) => format!("{model}-{}", l.as_str()),
        None => model.to_string(),
    }
}

/// Grok Build's models. grok takes `--effort none…max`, but each
/// model's menu comes from xAI's `/v1/models` and isn't published, so no levels are offered
/// until a recorded run reads them: the harness picks.
/// What Grok's own session reports (`reasoningEfforts` in its models update over ACP, Grok
/// 1.0.70 with a grok.com sign-in): High is each one's default.
pub fn grok() -> Vec<ModelInfo> {
    const FOUR: &[Level] = &[Low, Medium, High, Xhigh];
    vec![
        model("grok-4.7", "Grok 4.7", "grok", flag(FOUR, Some(High))),
        model(
            "grok-4.7-build-fast",
            "Grok 4.7 Fast",
            "grok",
            flag(FOUR, Some(High)),
        ),
        model("grok-4.6", "Grok 4.6", "grok", flag(FOUR, Some(High))),
        model(
            "grok-4.5",
            "Grok 4.5",
            "grok",
            flag(&[Low, Medium, High], Some(High)),
        ),
    ]
}

/// GitHub Copilot's models, from its `model` setting (`copilot help config`, Copilot CLI
/// 1.0.105): the current one of each family it serves. `--reasoning-effort` takes the
/// levels the family's own harness offers; Gemini and Kimi run at Copilot's default.
pub fn copilot() -> Vec<ModelInfo> {
    const FOUR: &[Level] = &[Low, Medium, High, Xhigh];
    let none = || flag(&[], None);
    vec![
        model("auto", "Auto", "copilot", none()),
        model(
            "claude-opus-5.5",
            "Claude Opus 5.5",
            "claude",
            flag(FIVE, None),
        ),
        model(
            "claude-sonnet-5.5",
            "Claude Sonnet 5.5",
            "claude",
            flag(FIVE, None),
        ),
        model(
            "claude-fable-5.1",
            "Claude Fable 5.1",
            "claude",
            flag(FIVE, None),
        ),
        model(
            "claude-haiku-5.5",
            "Claude Haiku 5.5",
            "claude",
            flag(FIVE, None),
        ),
        model("gpt-6.1-sol", "GPT-6.1 Sol", "openai", flag(FIVE, None)),
        model("gpt-6-astra", "GPT-6 Astra", "openai", flag(FIVE, None)),
        model("gpt-6-luna", "GPT-6 Luna", "openai", flag(FIVE, None)),
        model("gemini-4-argon", "Gemini 4 Argon", "gemini", none()),
        model("grok-4.7", "Grok 4.7", "grok", flag(FOUR, None)),
        model("kimi-k3", "Kimi K3", "kimi", none()),
    ]
}

/// Cursor's models, as `cursor-agent models` lists them (recorded on Windows, Cursor CLI
/// 2026.10.22, a signed-in account): the current one of each family. The level is part of
/// the id (`claude-opus-5-5-high`), so it goes in the model slug; a model without one
/// runs at the level Cursor's own name marks as plain ("Claude Opus 5.5 1M" is medium).
pub fn cursor() -> Vec<ModelInfo> {
    let slug = |levels: &[Level], default| ThinkingSupport {
        via: ThinkingVia::ModelSlug,
        ..flag(levels, default)
    };
    const GPT: &[Level] = &[Level::None, Low, Medium, High, Xhigh, Max];
    vec![
        model("auto", "Auto", "cursor", flag(&[], None)),
        model("composer-2.5", "Composer 2.5", "cursor", flag(&[], None)),
        model(
            "claude-opus-5-5",
            "Claude Opus 5.5",
            "claude",
            slug(FIVE, Some(Medium)),
        ),
        model(
            "claude-sonnet-5-5",
            "Claude Sonnet 5.5",
            "claude",
            slug(FIVE, Some(Medium)),
        ),
        model(
            "claude-fable-5-1",
            "Claude Fable 5.1",
            "claude",
            slug(FIVE, Some(High)),
        ),
        model(
            "claude-haiku-5-5-thinking",
            "Claude Haiku 5.5",
            "claude",
            slug(FIVE, Some(Medium)),
        ),
        model(
            "gpt-5.6-sol",
            "GPT-5.6 Sol",
            "openai",
            slug(GPT, Some(Medium)),
        ),
        model(
            "gpt-5.6-terra",
            "GPT-5.6 Terra",
            "openai",
            slug(GPT, Some(Medium)),
        ),
        model(
            "gpt-5.6-luna",
            "GPT-5.6 Luna",
            "openai",
            slug(GPT, Some(Medium)),
        ),
        model(
            "gemini-4-argon",
            "Gemini 4 Argon",
            "gemini",
            slug(&[Low, Medium, High], Some(Medium)),
        ),
        model(
            "grok-4.7",
            "Grok 4.7",
            "grok",
            slug(&[Low, Medium, High, Xhigh], Some(Medium)),
        ),
        model(
            "kimi-k3",
            "Kimi K3",
            "kimi",
            slug(&[Low, High, Max], Some(Max)),
        ),
    ]
}

/// The id Cursor takes for `model` at `level`: `<model>-<level>`, the model's default level
/// when none is picked. Models without levels and unknown ones pass through unchanged.
pub fn cursor_slug(model: &str, level: Option<Level>) -> String {
    let default = cursor()
        .into_iter()
        .find(|m| m.id == model && !m.thinking.levels.is_empty())
        .map(|m| m.thinking.default);
    match default {
        Some(d) => match level.or(d) {
            Some(l) => format!("{model}-{}", l.as_str()),
            None => model.to_string(),
        },
        None => model.to_string(),
    }
}

/// Checks a turn's model and level against `models`. An unknown model id is allowed
/// (full ids and new releases), but then only with no level, since we can't vouch for it.
pub fn validate(models: &[ModelInfo], model: Option<&str>, level: Option<Level>) -> AppResult<()> {
    let Some(level) = level else { return Ok(()) };
    let Some(id) = model else {
        return Err(AppError::Invalid(
            "pick a model to set a thinking level; levels differ per model".into(),
        ));
    };
    let m = models.iter().find(|m| m.id == id).ok_or_else(|| {
        AppError::Invalid(format!(
            "Gond doesn't know which thinking levels {id} takes; leave it on the default"
        ))
    })?;
    if m.thinking.levels.is_empty() {
        return Err(AppError::Invalid(format!(
            "{} has no thinking levels Gond can set yet; leave it on the default",
            m.label
        )));
    }
    if m.thinking.levels.contains(&level) {
        Ok(())
    } else {
        let names: Vec<_> = m
            .thinking
            .levels
            .iter()
            .map(|l| {
                serde_json::to_value(l)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        Err(AppError::Invalid(format!(
            "{} takes {}, not {}",
            m.label,
            names.join(", "),
            serde_json::to_value(level).unwrap().as_str().unwrap()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_per_model() {
        let c = codex();
        let luna = c.iter().find(|m| m.id == "gpt-6-luna").unwrap();
        assert!(!luna.thinking.levels.contains(&Ultra));
        assert!(c[0].thinking.levels.contains(&Ultra) && c[0].thinking.note.is_some());
        assert!(claude().iter().all(|m| m.thinking.levels == FIVE));
        let json = serde_json::to_value(&claude()[0]).unwrap();
        assert_eq!(json["thinking"]["default"], "medium");
        assert_eq!(json["thinking"]["via"], "flag");
    }

    #[test]
    fn validation_explains_what_is_allowed() {
        assert!(validate(&codex(), Some("gpt-6-luna"), None).is_ok());
        assert!(validate(&codex(), Some("gpt-6.1-sol"), Some(Ultra)).is_ok());
        let e = validate(&codex(), Some("gpt-6-luna"), Some(Ultra)).unwrap_err();
        assert_eq!(
            e.to_string(),
            "GPT-6 Luna takes low, medium, high, xhigh, max, not ultra"
        );
        assert!(validate(&claude(), None, Some(High)).is_err());
        assert!(validate(&claude(), Some("claude-opus-5-5-20261001"), None).is_ok());
        assert!(validate(&claude(), Some("claude-opus-5-5-20261001"), Some(High)).is_err());
    }
}
