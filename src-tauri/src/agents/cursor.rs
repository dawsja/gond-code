//! Cursor's CLI (`cursor-agent`): its interactive CLI runs in a pane (`runner::interactive`);
//! this says where it is, which models it offers and how it signs in.

use super::driver::{AccountEnv, Driver};
use super::{detect, AgentInfo, AgentKind};
use crate::accounts::{Account, AccountKind};
use crate::error::{AppError, AppResult};

pub struct Cursor;

impl Driver for Cursor {
    fn id(&self) -> &str {
        "cursor"
    }

    fn harness(&self) -> &'static str {
        "cursor"
    }

    fn kind(&self) -> AgentKind {
        AgentKind::Cursor
    }

    fn detect(&self) -> AgentInfo {
        let found = detect::find("cursor-agent", crate::process::path());
        AgentInfo {
            id: "cursor".into(),
            kind: AgentKind::Cursor,
            provider: "cursor".into(),
            name: "Cursor".into(),
            command: "cursor-agent".into(),
            installed: found.is_some(),
            version: found.as_deref().and_then(detect::version),
        }
    }

    fn models(&self) -> Vec<super::models::ModelInfo> {
        super::models::cursor()
    }

    fn login_args(&self) -> Option<&'static [&'static str]> {
        Some(&["login"])
    }

    fn login_status_args(&self) -> Option<&'static [&'static str]> {
        Some(&["status"])
    }

    /// The sign-in is Cursor's own and one per Windows user (`%APPDATA%\Cursor\auth.json`,
    /// which only `APPDATA` moves): sign-in accounts change nothing. Keys go in env.
    fn account_env(&self, account: &Account) -> AppResult<AccountEnv> {
        let mut e = AccountEnv {
            remove: vec!["CURSOR_API_KEY", "CURSOR_AUTH_TOKEN"],
            ..Default::default()
        };
        match account.kind {
            AccountKind::Oauth => {}
            AccountKind::ApiKey => e
                .secrets
                .push(AccountEnv::secret(account, "CURSOR_API_KEY")?),
            _ => {
                return Err(AppError::Invalid(
                    "Cursor accounts are its sign-in or API keys".into(),
                ))
            }
        }
        Ok(e)
    }
}
