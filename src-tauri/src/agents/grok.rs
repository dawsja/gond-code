//! Grok Build (`grok`, xai-org/grok-build): its interactive CLI runs in a pane
//! (`runner::interactive`); this says where it is, which models it offers and how it signs in.

use super::driver::{AccountEnv, Driver};
use super::{detect, AgentInfo, AgentKind};
use crate::accounts::{Account, AccountKind};
use crate::error::{AppError, AppResult};

pub struct Grok;

impl Driver for Grok {
    fn id(&self) -> &str {
        "grok"
    }

    fn harness(&self) -> &'static str {
        "grok"
    }

    fn kind(&self) -> AgentKind {
        AgentKind::Grok
    }

    fn detect(&self) -> AgentInfo {
        let found = detect::find("grok", crate::process::path());
        AgentInfo {
            id: "grok".into(),
            kind: AgentKind::Grok,
            provider: "grok".into(),
            name: "Grok Build".into(),
            command: "grok".into(),
            installed: found.is_some(),
            version: found.as_deref().and_then(detect::version),
        }
    }

    fn models(&self) -> Vec<super::models::ModelInfo> {
        super::models::grok()
    }

    fn login_args(&self) -> Option<&'static [&'static str]> {
        Some(&["login"])
    }

    /// Sign-ins get their own `GROK_HOME`; keys go in `XAI_API_KEY`.
    fn account_env(&self, account: &Account) -> AppResult<AccountEnv> {
        let mut e = AccountEnv {
            remove: vec!["GROK_HOME", "XAI_API_KEY"],
            ..Default::default()
        };
        match account.kind {
            AccountKind::Oauth => e.home(account, "GROK_HOME"),
            AccountKind::ApiKey => e.secrets.push(AccountEnv::secret(account, "XAI_API_KEY")?),
            _ => {
                return Err(AppError::Invalid(
                    "Grok accounts are sign-ins or API keys".into(),
                ))
            }
        }
        Ok(e)
    }
}
