//! Agent CLIs: the built-in ones and your custom ones, how to find them (`detect`), and
//! which one runs in a terminal (`running`).

pub mod cwd;
pub mod detect;
pub mod resume;
pub mod running;
pub mod spec;

use serde::{Deserialize, Serialize};

/// What an agent is: a built-in one's id (its file in `agents/`), or `custom`.
pub type AgentKind = String;

/// `AgentInfo.kind` for a custom agent.
pub const CUSTOM_KIND: &str = "custom";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentInfo {
    pub id: String,
    pub kind: AgentKind,
    /// Brand icon key (@lobehub/icons), e.g. "claude", "codex"; unknown → neutral glyph.
    pub provider: String,
    pub name: String,
    pub command: String,
    pub installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Not installed, but what it left is still here (its settings, a PATH entry, an
    /// uninstall that stopped halfway): uninstalling clears it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub leftovers: bool,
    /// Gond reads its plan usage (Settings → Usage).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub plan: bool,
    /// How its status shows in a pane: its title's reader, its prompts' words, where it asks,
    /// its image-paste key (its file's `[status]`). None for a custom agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<spec::Status>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CustomAgent {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Brand icon key for the UI; none shows a neutral glyph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// How the prompt reaches the command.
    #[serde(default)]
    pub prompt_via: PromptVia,
}

/// `arg` appends the prompt as the last argument; `stdin` writes it to stdin and
/// closes it. Either way it is also in `GOND_CODE_PROMPT`.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum PromptVia {
    #[default]
    Arg,
    Stdin,
}

impl PromptVia {
    pub fn as_str(self) -> &'static str {
        match self {
            PromptVia::Arg => "arg",
            PromptVia::Stdin => "stdin",
        }
    }
}

/// AgentInfo.provider for a custom agent that didn't pick one.
pub const CUSTOM_PROVIDER: &str = "custom";

/// The agents Gond knows, from their files in `agents/` (`spec`): (id, display name, command).
pub fn built_in() -> impl Iterator<Item = (&'static str, &'static str, &'static str)> {
    spec::all()
        .iter()
        .map(|s| (s.id.as_str(), s.name.as_str(), s.command.as_str()))
}

/// One plan-usage window a harness reported: the 5-hour session, the week…
#[derive(Debug, Clone, PartialEq, serde::Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindow {
    /// "session" | "daily" | "weekly" | "monthly"
    pub name: String,
    /// A window the plan scopes, e.g. "Weekly · Fable" (one model's week); none for the plan's own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// 0–100, what's left in this window.
    pub percent_left: f64,
    /// RFC 3339, when the window resets.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
    /// The harness's own word, e.g. "allowed", "allowed_warning", "rejected".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

impl UsageWindow {
    /// From a used fraction (0–1) and a reset time in Unix seconds.
    pub fn from_used(
        name: &str,
        used: f64,
        resets_unix: Option<i64>,
        status: Option<&str>,
    ) -> Self {
        let resets_at = resets_unix
            .and_then(|s| time::OffsetDateTime::from_unix_timestamp(s).ok())
            .and_then(|t| {
                t.format(&time::format_description::well_known::Rfc3339)
                    .ok()
            });
        UsageWindow {
            name: name.into(),
            label: None,
            percent_left: ((1.0 - used.clamp(0.0, 1.0)) * 100.0 * 10.0).round() / 10.0,
            resets_at,
            status: status.map(String::from),
        }
    }
}

use crate::error::{AppError, AppResult};
use crate::store::{custom_agents, Store};

pub fn list_custom(store: &Store) -> AppResult<Vec<CustomAgent>> {
    store.with(custom_agents::list)
}

/// Probes every agent. Blocks for up to a few seconds: call it off the async runtime,
/// and never with the Store locked.
pub fn detect(customs: &[CustomAgent]) -> Vec<AgentInfo> {
    detect::detect_all(crate::process::path(), customs)
}

/// Saves a custom agent, giving it an id if it has none.
pub fn save_custom(store: &Store, mut agent: CustomAgent) -> AppResult<CustomAgent> {
    agent.name = agent.name.trim().to_string();
    agent.command = agent.command.trim().to_string();
    agent.provider = agent
        .provider
        .map(|p| p.trim().to_lowercase())
        .filter(|p| !p.is_empty());
    if agent.name.is_empty() || agent.command.is_empty() {
        return Err(AppError::Invalid(
            "a custom agent needs a name and a command".into(),
        ));
    }
    if built_in().any(|(id, ..)| id == agent.id) {
        return Err(AppError::Invalid(format!(
            "{} is a built-in agent",
            agent.id
        )));
    }
    if agent.id.is_empty() {
        agent.id = ulid::Ulid::generate().to_string();
    }
    store.with(|c| custom_agents::upsert(c, &agent))?;
    Ok(agent)
}

pub fn delete_custom(store: &Store, id: &str) -> AppResult<()> {
    match store.with(|c| custom_agents::delete(c, id))? {
        true => Ok(()),
        false => Err(AppError::NotFound(format!("custom agent {id} not found"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str, name: &str) -> CustomAgent {
        CustomAgent {
            id: id.into(),
            name: name.into(),
            command: "aider".into(),
            args: vec!["--yes".into()],
            provider: None,
            prompt_via: Default::default(),
        }
    }

    #[test]
    fn custom_agents_round_trip() {
        let store = Store::open_in_memory().unwrap();
        let saved = save_custom(&store, agent("", " Aider ")).unwrap();
        assert_eq!(saved.id.len(), 26);
        assert_eq!(saved.name, "Aider");

        let renamed = save_custom(
            &store,
            CustomAgent {
                name: "Aider 2".into(),
                ..saved.clone()
            },
        )
        .unwrap();
        let listed = store.with(custom_agents::list).unwrap();
        assert_eq!(listed, [renamed]);

        let infos = detect(&list_custom(&store).unwrap());
        let info = infos.iter().find(|a| a.id == saved.id).unwrap();
        assert_eq!(
            (info.kind.as_str(), info.provider.as_str()),
            (CUSTOM_KIND, CUSTOM_PROVIDER)
        );

        // A provider round-trips (normalised) and reaches AgentInfo.
        let branded = save_custom(
            &store,
            CustomAgent {
                provider: Some(" DeepSeek ".into()),
                ..saved.clone()
            },
        )
        .unwrap();
        assert_eq!(branded.provider.as_deref(), Some("deepseek"));
        assert_eq!(
            store.with(custom_agents::list).unwrap(),
            std::slice::from_ref(&branded)
        );
        let infos = detect(&list_custom(&store).unwrap());
        assert_eq!(
            infos.iter().find(|a| a.id == saved.id).unwrap().provider,
            "deepseek"
        );
        let cleared = save_custom(
            &store,
            CustomAgent {
                provider: Some("  ".into()),
                ..branded
            },
        )
        .unwrap();
        assert_eq!(cleared.provider, None);

        delete_custom(&store, &saved.id).unwrap();
        assert_eq!(
            delete_custom(&store, &saved.id).unwrap_err().code(),
            "not_found"
        );
    }

    #[test]
    fn rejects_bad_custom_agents() {
        let store = Store::open_in_memory().unwrap();
        for bad in [
            agent("", "  "),
            CustomAgent {
                command: "".into(),
                ..agent("", "x")
            },
            agent("claude", "x"),
        ] {
            assert_eq!(save_custom(&store, bad).unwrap_err().code(), "invalid");
        }
    }

    #[test]
    fn dto_json_shape() {
        let info = AgentInfo {
            id: "codex".into(),
            kind: "codex".into(),
            provider: "codex".into(),
            name: "Codex".into(),
            command: "codex".into(),
            installed: false,
            version: None,
            leftovers: false,
            plan: false,
            status: None,
        };
        assert_eq!(
            serde_json::to_value(&info).unwrap(),
            serde_json::json!({ "id": "codex", "kind": "codex", "provider": "codex", "name": "Codex", "command": "codex", "installed": false })
        );
    }
}
