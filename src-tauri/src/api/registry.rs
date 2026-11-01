//! Command registry: every action is a named command with a JSON
//! Schema for input and output, a risk level and a handler.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AppError, AppResult};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum Risk {
    Read,
    WriteLocal,
    Outward,
    Destructive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ActorKind {
    Human,
    Agent,
    External,
}

/// Who is calling. Recorded on every event-log row and used by policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Actor {
    pub kind: ActorKind,
    /// "human" | "agent:<threadId>" | "external:<clientName>"
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

impl Actor {
    pub fn human() -> Self {
        Actor {
            kind: ActorKind::Human,
            id: "human".into(),
            label: "You".into(),
            thread_id: None,
            session_id: None,
        }
    }
}

/// A risk that is constant, or depends on the input (e.g. `thread.archive` with
/// `removeWorktree`).
#[derive(Clone, Copy)]
pub enum RiskOf {
    Fixed(Risk),
    Dynamic {
        note: &'static str,
        of: fn(&Value) -> Risk,
    },
}

/// The static half of a command: everything but the handler.
pub struct Def {
    pub name: &'static str,
    pub summary: &'static str,
    pub risk: RiskOf,
    pub human_only: bool,
    /// JSON Pointers masked in the event log.
    pub redact: &'static [&'static str],
}

impl Def {
    pub const fn new(name: &'static str, summary: &'static str, risk: Risk) -> Self {
        Def {
            name,
            summary,
            risk: RiskOf::Fixed(risk),
            human_only: false,
            redact: &[],
        }
    }
    pub const fn dynamic(mut self, note: &'static str, of: fn(&Value) -> Risk) -> Self {
        self.risk = RiskOf::Dynamic { note, of };
        self
    }
    pub const fn human_only(mut self) -> Self {
        self.human_only = true;
        self
    }
    pub const fn redact(mut self, pointers: &'static [&'static str]) -> Self {
        self.redact = pointers;
        self
    }
}

/// One call's context: who is calling, what the handler can reach, and the
/// registry itself (for `app.command.list`).
pub struct Call<'a, C> {
    pub actor: &'a Actor,
    pub ctx: &'a C,
    pub registry: &'a Registry<C>,
}

type Handler<C> = Box<dyn Fn(&Call<C>, Value) -> AppResult<Value> + Send + Sync>;

pub struct Command<C> {
    pub def: Def,
    pub input_schema: Value,
    pub output_schema: Value,
    handler: Handler<C>,
}

impl<C> Command<C> {
    pub fn risk(&self, input: &Value) -> Risk {
        match self.def.risk {
            RiskOf::Fixed(r) => r,
            RiskOf::Dynamic { of, .. } => of(input),
        }
    }

    pub fn info(&self) -> CommandInfo {
        let (risk, risk_note) = match self.def.risk {
            RiskOf::Fixed(r) => (serde_json::to_value(r).unwrap_or_default(), None),
            RiskOf::Dynamic { note, .. } => ("dynamic".into(), Some(note.to_string())),
        };
        CommandInfo {
            name: self.def.name.into(),
            summary: self.def.summary.into(),
            risk,
            risk_note,
            human_only: self.def.human_only,
            input_schema: self.input_schema.clone(),
            output_schema: self.output_schema.clone(),
        }
    }
}

/// What `app.command.list` returns for each command.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CommandInfo {
    pub name: String,
    pub summary: String,
    /// "read" | "write-local" | "outward" | "destructive" | "dynamic"
    pub risk: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_note: Option<String>,
    pub human_only: bool,
    pub input_schema: Value,
    pub output_schema: Value,
}

/// Input for commands that take none. Unknown fields are rejected so typos show.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoInput {}

pub struct Registry<C> {
    commands: BTreeMap<&'static str, Command<C>>,
}

impl<C> Default for Registry<C> {
    fn default() -> Self {
        Registry {
            commands: BTreeMap::new(),
        }
    }
}

/// `area.verb` in camelCase segments, at least two of them.
pub fn valid_name(name: &str) -> bool {
    let parts: Vec<&str> = name.split('.').collect();
    parts.len() >= 2
        && parts.iter().all(|p| {
            let mut c = p.chars();
            c.next().is_some_and(|f| f.is_ascii_lowercase())
                && c.all(|ch| ch.is_ascii_alphanumeric())
        })
}

/// Commands that open code for human editing (editor or Explorer panels, "open in editor"
/// buttons). The rule: agents edit, the human steers and approves. Never registered.
/// Terminals are not editor features. The one way into an editor is a `file:line` link a pane
/// printed, which the human clicks: `file.open`, human only.
pub fn human_editing(name: &str) -> bool {
    let last = name.rsplit('.').next().unwrap_or_default();
    name.starts_with("editor.")
        || name.starts_with("explorer.")
        || last.starts_with("openIn")
        || last == "reveal"
}

fn schema<T: JsonSchema>() -> Value {
    serde_json::to_value(schemars::schema_for!(T)).unwrap_or_default()
}

impl<C> Registry<C> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a typed command. Panics on a bad or duplicate name: that is a
    /// programming error the registry tests catch.
    pub fn add<I, O, F>(&mut self, def: Def, handler: F) -> &mut Self
    where
        I: DeserializeOwned + JsonSchema,
        O: Serialize + JsonSchema,
        F: Fn(&Call<C>, I) -> AppResult<O> + Send + Sync + 'static,
    {
        assert!(valid_name(def.name), "bad command name {:?}", def.name);
        assert!(
            !human_editing(def.name),
            "{:?} is a human-editing command",
            def.name
        );
        assert!(
            !self.commands.contains_key(def.name),
            "duplicate command {:?}",
            def.name
        );
        let handler: Handler<C> = Box::new(move |call, input| {
            let input: I = parse(input)?;
            let out = handler(call, input)?;
            serde_json::to_value(out).map_err(|e| AppError::Invalid(format!("bad output: {e}")))
        });
        let name = def.name;
        self.commands.insert(
            name,
            Command {
                def,
                input_schema: schema::<I>(),
                output_schema: schema::<O>(),
                handler,
            },
        );
        self
    }

    pub fn get(&self, name: &str) -> Option<&Command<C>> {
        self.commands.get(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.commands.keys().copied()
    }

    /// `app.command.list`: sorted by name, optionally filtered.
    pub fn list(&self, risk: Option<Risk>, prefix: Option<&str>) -> Vec<CommandInfo> {
        self.commands
            .values()
            .filter(|c| prefix.is_none_or(|p| c.def.name.starts_with(p)))
            .filter(|c| match (risk, c.def.risk) {
                (None, _) => true,
                (Some(r), RiskOf::Fixed(cr)) => r == cr,
                (Some(_), RiskOf::Dynamic { .. }) => true,
            })
            .map(Command::info)
            .collect()
    }

    /// Runs one command: lookup, human-only check, input validation, handler.
    /// Policy, approvals and the event log wrap this in the gateway.
    pub fn run(&self, ctx: &C, actor: &Actor, name: &str, input: Value) -> AppResult<Value> {
        let cmd = self
            .commands
            .get(name)
            .ok_or_else(|| AppError::UnknownCommand(name.into()))?;
        if cmd.def.human_only && actor.kind != ActorKind::Human {
            return Err(AppError::Forbidden(format!(
                "{name} can only be run by the human"
            )));
        }
        (cmd.handler)(
            &Call {
                actor,
                ctx,
                registry: self,
            },
            input,
        )
    }
}

/// Deserialises input, reporting the failing field as a JSON Pointer.
/// `null` counts as `{}` so commands without input can be called bare.
fn parse<I: DeserializeOwned>(input: Value) -> AppResult<I> {
    let input = if input.is_null() {
        Value::Object(Default::default())
    } else {
        input
    };
    serde_path_to_error::deserialize(input).map_err(|e| {
        let pointer = e
            .path()
            .iter()
            .filter_map(|seg| match seg {
                serde_path_to_error::Segment::Seq { index } => Some(index.to_string()),
                serde_path_to_error::Segment::Map { key } => Some(key.clone()),
                serde_path_to_error::Segment::Enum { variant } => Some(variant.clone()),
                serde_path_to_error::Segment::Unknown => None,
            })
            .fold(String::new(), |acc, s| format!("{acc}/{s}"));
        AppError::InvalidInput {
            message: format!(
                "invalid input at {}: {}",
                if pointer.is_empty() { "/" } else { &pointer },
                e.inner()
            ),
            pointer,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(Deserialize, JsonSchema)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Archive {
        thread_id: String,
        remove_worktree: bool,
    }

    fn reg() -> Registry<u32> {
        let mut r = Registry::new();
        r.add(
            Def::new("test.add", "Add ctx to n", Risk::Read),
            |call, n: u32| Ok(n + *call.ctx),
        );
        r.add(
            Def::new("thread.archive", "Archive", Risk::WriteLocal).dynamic(
                "destructive when removeWorktree",
                |v| {
                    if v["removeWorktree"] == true {
                        Risk::Destructive
                    } else {
                        Risk::WriteLocal
                    }
                },
            ),
            |_, a: Archive| Ok(format!("{}:{}", a.thread_id, a.remove_worktree)),
        );
        r.add(
            Def::new("policy.set", "Set policy", Risk::WriteLocal).human_only(),
            |_, _: NoInput| Ok(()),
        );
        r
    }

    fn agent() -> Actor {
        Actor {
            kind: ActorKind::Agent,
            id: "agent:t1".into(),
            label: "a".into(),
            thread_id: Some("t1".into()),
            session_id: None,
        }
    }

    #[test]
    fn runs_with_context() {
        assert_eq!(
            reg()
                .run(&2, &Actor::human(), "test.add", json!(40))
                .unwrap(),
            json!(42)
        );
    }

    #[test]
    fn unknown_command() {
        let e = reg()
            .run(&0, &Actor::human(), "nope.x", json!({}))
            .unwrap_err();
        assert_eq!(e.code(), "unknown_command");
    }

    #[test]
    fn invalid_input_has_pointer() {
        let e = reg()
            .run(
                &0,
                &Actor::human(),
                "thread.archive",
                json!({ "threadId": "t", "removeWorktree": "yes" }),
            )
            .unwrap_err();
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["code"], "invalid_input");
        assert_eq!(v["pointer"], "/removeWorktree");
        let e = reg()
            .run(
                &0,
                &Actor::human(),
                "thread.archive",
                json!({ "threadId": "t", "removeWorktree": true, "typo": 1 }),
            )
            .unwrap_err();
        assert_eq!(e.code(), "invalid_input");
    }

    #[test]
    fn human_only_is_forbidden_for_agents() {
        let r = reg();
        assert_eq!(
            r.run(&0, &agent(), "policy.set", Value::Null)
                .unwrap_err()
                .code(),
            "forbidden"
        );
        assert!(r
            .run(&0, &Actor::human(), "policy.set", Value::Null)
            .is_ok());
    }

    #[test]
    fn dynamic_risk() {
        let r = reg();
        let c = r.get("thread.archive").unwrap();
        assert_eq!(
            c.risk(&json!({ "removeWorktree": true })),
            Risk::Destructive
        );
        assert_eq!(
            c.risk(&json!({ "removeWorktree": false })),
            Risk::WriteLocal
        );
        assert_eq!(c.info().risk, json!("dynamic"));
    }

    #[test]
    fn list_has_schemas_and_filters() {
        let r = reg();
        let all = r.list(None, None);
        assert_eq!(
            all.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
            ["policy.set", "test.add", "thread.archive"]
        );
        let a = all.iter().find(|c| c.name == "thread.archive").unwrap();
        assert_eq!(
            a.input_schema["properties"]["removeWorktree"]["type"],
            "boolean"
        );
        assert_eq!(r.list(Some(Risk::Read), Some("test.")).len(), 1);
        assert_eq!(
            serde_json::to_value(Risk::WriteLocal).unwrap(),
            "write-local"
        );
    }

    #[test]
    fn names() {
        assert!(valid_name("agent.custom.save"));
        assert!(valid_name("security.rotateToken"));
        assert!(!valid_name("thread_create"));
        assert!(!valid_name("Thread.create"));
        assert!(!valid_name("thread."));
    }

    #[test]
    fn human_editing_names() {
        for n in [
            "editor.open",
            "explorer.open",
            "diff.openInEditor",
            "file.reveal",
        ] {
            assert!(human_editing(n), "{n}");
        }
        for n in [
            "pty.read",
            "pty.takeOver",
            "pty.release",
            "diff.file",
            "thread.open",
            "project.add",
        ] {
            assert!(!human_editing(n), "{n}");
        }
    }

    #[test]
    #[should_panic(expected = "human-editing")]
    fn human_editing_panics() {
        reg().add(
            Def::new("editor.open", "no", Risk::WriteLocal),
            |_, _: NoInput| Ok(()),
        );
    }

    #[test]
    #[should_panic(expected = "duplicate")]
    fn duplicate_panics() {
        let mut r = reg();
        r.add(
            Def::new("test.add", "again", Risk::Read),
            |_, _: NoInput| Ok(()),
        );
    }
}
