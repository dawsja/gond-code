//! Antigravity CLI (`agy`): its interactive CLI runs in a pane (`runner::interactive`);
//! this says where it is, which models it offers (Gemini 4 Argon) and how it signs in.

use super::driver::{AccountEnv, Driver};
use super::{detect, AgentInfo, AgentKind};
use crate::accounts::{Account, AccountKind};
use crate::error::{AppError, AppResult};

pub struct Agy;

impl Driver for Agy {
    fn id(&self) -> &str {
        "agy"
    }

    fn harness(&self) -> &'static str {
        "agy"
    }

    fn kind(&self) -> AgentKind {
        AgentKind::Agy
    }

    fn detect(&self) -> AgentInfo {
        let found = detect::find("agy", crate::process::path());
        AgentInfo {
            id: "agy".into(),
            kind: AgentKind::Agy,
            provider: "antigravity".into(),
            name: "Antigravity".into(),
            command: "agy".into(),
            installed: found.is_some(),
            version: found.as_deref().and_then(detect::version),
        }
    }

    fn models(&self) -> Vec<super::models::ModelInfo> {
        super::models::agy()
    }

    /// The Google sign-in is agy's own and one per Windows user:
    /// sign-in accounts change nothing. Keys and gateways go in env.
    fn account_env(&self, account: &Account) -> AppResult<AccountEnv> {
        let mut e = AccountEnv {
            remove: vec![
                "GEMINI_API_KEY",
                "AGY_LLM_GATEWAY_URL",
                "AGY_LLM_GATEWAY_API_KEY",
            ],
            ..Default::default()
        };
        match account.kind {
            AccountKind::Oauth => {}
            AccountKind::ApiKey => e
                .secrets
                .push(AccountEnv::secret(account, "GEMINI_API_KEY")?),
            AccountKind::Gateway => {
                let url = account.endpoint.clone().ok_or_else(|| {
                    AppError::Invalid(format!(
                        "account \"{}\" has no gateway endpoint",
                        account.label
                    ))
                })?;
                e.env.push(("AGY_LLM_GATEWAY_URL".into(), url));
                e.secrets
                    .push(AccountEnv::secret(account, "AGY_LLM_GATEWAY_API_KEY")?);
            }
            AccountKind::Token => {
                return Err(AppError::Invalid("agy takes no token accounts".into()))
            }
        }
        Ok(e)
    }
}
