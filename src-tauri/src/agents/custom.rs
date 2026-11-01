//! Custom agents: a command you saved, run in a pane as its own CLI.

use super::driver::Driver;
use super::{AgentInfo, AgentKind, CustomAgent, CUSTOM_PROVIDER};

pub struct Custom(pub CustomAgent);

impl Driver for Custom {
    fn id(&self) -> &str {
        &self.0.id
    }

    fn harness(&self) -> &'static str {
        "custom"
    }

    fn kind(&self) -> AgentKind {
        AgentKind::Custom
    }

    fn detect(&self) -> AgentInfo {
        AgentInfo {
            id: self.0.id.clone(),
            kind: AgentKind::Custom,
            provider: self
                .0
                .provider
                .clone()
                .unwrap_or_else(|| CUSTOM_PROVIDER.into()),
            name: self.0.name.clone(),
            command: self.0.command.clone(),
            installed: super::detect::find(&self.0.command, crate::process::path()).is_some(),
            version: None,
        }
    }
}
