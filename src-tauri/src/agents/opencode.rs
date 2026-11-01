//! opencode: its interactive CLI runs in a pane (`runner::interactive`).
//! It signs in per provider itself (`opencode auth login`), so it runs as whatever it's set up
//! with.

use super::driver::Driver;
use super::{detect, AgentInfo, AgentKind};

pub struct Opencode;

impl Driver for Opencode {
    fn id(&self) -> &str {
        "opencode"
    }

    fn harness(&self) -> &'static str {
        "opencode"
    }

    fn kind(&self) -> AgentKind {
        AgentKind::Opencode
    }

    fn detect(&self) -> AgentInfo {
        let found = detect::find("opencode", crate::process::path());
        AgentInfo {
            id: "opencode".into(),
            kind: AgentKind::Opencode,
            provider: "opencode".into(),
            name: "opencode".into(),
            command: "opencode".into(),
            installed: found.is_some(),
            version: found.as_deref().and_then(detect::version),
        }
    }
}
