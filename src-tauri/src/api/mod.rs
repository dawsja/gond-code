//! The app's commands (docs/AGENT-API.md): one registry the UI calls through
//! `crate::commands::api`, and the state tree it watches. Tauri-free so it is unit-tested
//! on Linux.

pub mod builtin;
pub mod files;
pub mod git;
pub mod github;
pub mod harness;
pub mod layout;
pub mod projects;
pub mod pty;
pub mod registry;
pub mod speech;
pub mod state;
pub mod usage;

pub use registry::{Actor, ActorKind, CommandInfo, Def, Registry, Risk};
pub use state::StateTree;

/// What command handlers can reach. Grows as commands move into the registry.
/// The runtime managers are cheap clones sharing state with the ones Tauri manages.
#[derive(Clone)]
pub struct Services {
    pub state: StateTree,
    pub store: crate::store::Store,
    /// The terminals: in the terminal host when the app runs (they outlive it), in-process
    /// in tests.
    pub ptys: std::sync::Arc<crate::pty::Ptys>,
    /// The GitHub CLI and the browser sign-in in flight.
    pub github: crate::github::Github,
    /// Git's global config file; `None` is the user's own (tests point it at a temp file).
    pub git_global: Option<std::path::PathBuf>,
}

/// A terminal sink that drops everything: the runtime before `with_runtime`, and in tests.
pub struct NullSink;
impl crate::pty::PtySink for NullSink {
    fn data(&self, _: &str, _: String, _: u64) {}
    fn exit(&self, _: &str, _: Option<u32>) {}
}

impl Services {
    /// Services over `store`, with the tree loaded from it. The terminals start on a null
    /// sink; `with_runtime` attaches the app's own.
    pub fn new(store: crate::store::Store) -> crate::error::AppResult<Self> {
        let state = StateTree::new(initial_state());
        let s = Services {
            state,
            ptys: std::sync::Arc::new(crate::pty::Ptys::local(std::sync::Arc::new(NullSink))),
            github: crate::github::Github::default(),
            git_global: None,
            store,
        };
        projects::load(&s)?;
        usage::sync(&s);
        speech::sync(&s);
        Ok(s)
    }
}

impl Services {
    /// Uses the app's real terminals (the ones Tauri manages).
    pub fn with_runtime(mut self, ptys: std::sync::Arc<crate::pty::Ptys>) -> Self {
        self.ptys = ptys;
        self
    }
}

impl Default for Services {
    /// An in-memory store, for tests.
    fn default() -> Self {
        Services::new(crate::store::Store::open_in_memory().expect("in-memory store"))
            .expect("load services")
    }
}

/// The empty tree. Areas fill in as their commands move in.
pub fn initial_state() -> serde_json::Value {
    serde_json::json!({
        "app": {
            "version": env!("CARGO_PKG_VERSION"),
            "os": if cfg!(windows) { "windows" } else { "linux" },
            "eventSeq": 0,
        },
        "settings": {},
        "agents": {},
        "projects": {},
        "github": { "ghInstalled": false, "accounts": [] },
        "installs": {},
        "terminals": {},
        "usage": {},
        "speech": { "state": "off" },
        "layout": { "focus": { "panel": "chat" } },
    })
}

/// The app's registry: every command that has landed.
pub fn registry() -> Registry<Services> {
    let mut r = Registry::new();
    builtin::register(&mut r);
    files::register(&mut r);
    projects::register(&mut r);
    github::register(&mut r);
    git::register(&mut r);
    harness::register(&mut r);
    pty::register(&mut r);
    speech::register(&mut r);
    usage::register(&mut r);
    layout::register(&mut r);
    crate::updater::register(&mut r);
    r
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    /// `src/lib/commands.generated.json` is every command and its schemas, checked in, so
    /// the UI and the browser mock have them before the app runs.
    /// Regenerate with `UPDATE_COMMANDS=1 cargo test commands_json`.
    #[test]
    fn commands_json_is_current() {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../src/lib/commands.generated.json");
        let list = super::registry().list(None, None);
        let want = serde_json::to_string_pretty(&list).unwrap() + "\n";
        if std::env::var_os("UPDATE_COMMANDS").is_some() {
            std::fs::write(&path, &want).unwrap();
            return;
        }
        let have = std::fs::read_to_string(&path)
            .unwrap_or_default()
            .replace("\r\n", "\n");
        assert!(
            have == want,
            "src/lib/commands.generated.json is stale: run `UPDATE_COMMANDS=1 cargo test commands_json` in src-tauri and commit it"
        );
    }

    /// The contract can't drift from the registry (Atlas): every registered command
    /// has a row in an AGENT-API command table with the same risk, and every row is
    /// a registered command, unless its name cell says *(planned)* or *(MCP tool)*.
    #[test]
    fn agent_api_tables_match_the_registry() {
        let doc = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../docs/AGENT-API.md"),
        )
        .unwrap()
        .replace("\r\n", "\n");
        let cells = |l: &str| -> Vec<String> {
            l.trim()
                .trim_matches('|')
                .split(" | ")
                .map(|c| c.trim().to_string())
                .collect()
        };
        const RISKS: [&str; 4] = ["read", "write-local", "outward", "destructive"];
        // name -> (risk cell, planned)
        let mut rows: std::collections::BTreeMap<String, (String, bool)> = Default::default();
        let lines: Vec<&str> = doc.lines().collect();
        let mut risk_col: Option<usize> = None;
        for (i, l) in lines.iter().enumerate() {
            if !l.starts_with('|') {
                risk_col = None;
                continue;
            }
            let is_header = lines
                .get(i + 1)
                .is_some_and(|n| n.starts_with("|") && n.chars().all(|c| "|-: ".contains(c)));
            if is_header {
                let h = cells(l);
                risk_col = (h.first().map(String::as_str) == Some("Command"))
                    .then(|| h.iter().position(|c| c == "Risk"))
                    .flatten();
                continue;
            }
            let Some(rc) = risk_col else { continue };
            let c = cells(l);
            let Some(first) = c.first() else { continue };
            let Some(name) = first.strip_prefix('`').and_then(|r| r.split('`').next()) else {
                continue;
            };
            let planned = first.contains("*(planned)*") || first.contains("*(MCP tool)*");
            rows.entry(name.to_string())
                .or_insert((c.get(rc).cloned().unwrap_or_default(), planned));
        }
        let mut problems = vec![];
        for info in super::registry().list(None, None) {
            let Some((cell, planned)) = rows.get(&info.name) else {
                problems.push(format!(
                    "{}: registered but has no AGENT-API row",
                    info.name
                ));
                continue;
            };
            if *planned {
                problems.push(format!(
                    "{}: registered, so drop its *(planned)* mark",
                    info.name
                ));
            }
            // The first whole risk word in the cell ("already" isn't "read").
            let doc_risk = cell
                .split(|c: char| !(c.is_ascii_alphabetic() || c == '-'))
                .find_map(|w| RISKS.iter().copied().find(|r| *r == w));
            let problem = match info.risk.as_str() {
                Some("dynamic") => doc_risk
                    .is_none()
                    .then(|| format!("{}: dynamic risk, but the row names none", info.name)),
                Some(r) => (doc_risk != Some(r))
                    .then(|| format!("{}: registry says {r}, AGENT-API says {cell:?}", info.name)),
                None => None,
            };
            problems.extend(problem);
        }
        let names: std::collections::BTreeSet<String> = super::registry()
            .list(None, None)
            .into_iter()
            .map(|c| c.name)
            .collect();
        for (name, (_, planned)) in &rows {
            if !planned && !names.contains(name) {
                problems.push(format!(
                    "{name}: AGENT-API row with no registered command (mark it *(planned)*)"
                ));
            }
        }
        assert!(
            problems.is_empty(),
            "AGENT-API drifted from the registry:\n{}",
            problems.join("\n")
        );
    }
}
