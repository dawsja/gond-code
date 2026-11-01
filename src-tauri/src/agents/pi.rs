//! Pi (pi.dev, `@earendil-works/pi-coding-agent`): its interactive CLI runs
//! in a pane (`runner::interactive`). It signs in per provider itself (`/login`), so it runs as
//! whatever it's set up with. Its settings, sign-ins and sessions live in `~\.pi\agent`.

use super::driver::Driver;
use super::{detect, AgentInfo, AgentKind};

pub struct Pi;

impl Driver for Pi {
    fn id(&self) -> &str {
        "pi"
    }

    fn harness(&self) -> &'static str {
        "pi"
    }

    fn kind(&self) -> AgentKind {
        AgentKind::Pi
    }

    fn detect(&self) -> AgentInfo {
        let found = detect::find("pi", crate::process::path());
        AgentInfo {
            id: "pi".into(),
            kind: AgentKind::Pi,
            provider: "pi".into(),
            name: "Pi".into(),
            command: "pi".into(),
            installed: found.is_some(),
            version: found.as_deref().and_then(detect::version),
        }
    }
}
