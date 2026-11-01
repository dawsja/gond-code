//! Claude Code: its interactive CLI runs in a pane (`runner::interactive`); this says where it
//! is, which models it offers, how it signs in and how an account's key or home reaches it.

use super::driver::{AccountEnv, Driver, Part};
use super::{detect, AgentInfo, AgentKind};
use crate::accounts::HeaderValue;
use crate::accounts::{Account, AccountKind};
use crate::error::{AppError, AppResult};

pub struct Claude;

impl Driver for Claude {
    fn id(&self) -> &str {
        "claude"
    }

    fn harness(&self) -> &'static str {
        "claude"
    }

    fn kind(&self) -> AgentKind {
        AgentKind::Claude
    }

    fn detect(&self) -> AgentInfo {
        let found = detect::find("claude", crate::process::path());
        AgentInfo {
            id: "claude".into(),
            kind: AgentKind::Claude,
            provider: "claude".into(),
            name: "Claude Code".into(),
            command: "claude".into(),
            installed: found.is_some(),
            version: found.as_deref().and_then(detect::version),
        }
    }

    fn models(&self) -> Vec<super::models::ModelInfo> {
        super::models::claude()
    }

    fn login_status_args(&self) -> Option<&'static [&'static str]> {
        Some(&["auth", "status"])
    }

    fn login_args(&self) -> Option<&'static [&'static str]> {
        Some(&["auth", "login"])
    }

    /// Sign-ins get their own `CLAUDE_CONFIG_DIR`; keys and setup-tokens go in env
    /// . The user's own Claude env is unset so it can't win.
    /// . API accounts: `ANTHROPIC_BASE_URL` for a custom endpoint, the
    /// key as `ANTHROPIC_API_KEY` (`x-api-key`) or `ANTHROPIC_AUTH_TOKEN` (bearer, what most
    /// gateways want), extra headers as `ANTHROPIC_CUSTOM_HEADERS`, and the Bedrock or
    /// Vertex switches. The user's own Claude env is unset first so it can't win.
    fn account_env(&self, account: &Account) -> AppResult<AccountEnv> {
        let mut e = AccountEnv {
            remove: vec![
                "CLAUDE_CONFIG_DIR",
                "ANTHROPIC_API_KEY",
                "ANTHROPIC_AUTH_TOKEN",
                "CLAUDE_CODE_OAUTH_TOKEN",
                "ANTHROPIC_BASE_URL",
                "ANTHROPIC_CUSTOM_HEADERS",
                "CLAUDE_CODE_USE_BEDROCK",
                "CLAUDE_CODE_USE_VERTEX",
                "AWS_BEARER_TOKEN_BEDROCK",
                "AWS_REGION",
                "CLOUD_ML_REGION",
                "ANTHROPIC_VERTEX_PROJECT_ID",
            ],
            ..Default::default()
        };
        let o = &account.options;
        match account.kind {
            AccountKind::Oauth => e.home(account, "CLAUDE_CONFIG_DIR"),
            AccountKind::Token => e
                .secrets
                .push(AccountEnv::secret(account, "CLAUDE_CODE_OAUTH_TOKEN")?),
            AccountKind::ApiKey | AccountKind::Gateway => {
                match &o.cloud {
                    Some(crate::accounts::Cloud::Bedrock { region }) => {
                        e.env.push(("CLAUDE_CODE_USE_BEDROCK".into(), "1".into()));
                        e.env.push(("AWS_REGION".into(), region.clone()));
                        e.secrets
                            .push(AccountEnv::secret(account, "AWS_BEARER_TOKEN_BEDROCK")?);
                    }
                    Some(crate::accounts::Cloud::Vertex { region, project }) => {
                        // Vertex authenticates with the machine's Google credentials.
                        e.env.push(("CLAUDE_CODE_USE_VERTEX".into(), "1".into()));
                        e.env.push(("CLOUD_ML_REGION".into(), region.clone()));
                        e.env
                            .push(("ANTHROPIC_VERTEX_PROJECT_ID".into(), project.clone()));
                    }
                    None => {
                        if let Some(url) = &account.endpoint {
                            e.env.push(("ANTHROPIC_BASE_URL".into(), url.clone()));
                        } else if account.kind == AccountKind::Gateway {
                            return Err(AppError::Invalid(format!(
                                "account \"{}\" has no endpoint",
                                account.label
                            )));
                        }
                        let var = if o.bearer {
                            "ANTHROPIC_AUTH_TOKEN"
                        } else {
                            "ANTHROPIC_API_KEY"
                        };
                        e.secrets.push(AccountEnv::secret(account, var)?);
                    }
                }
                // One `Name: value` per line; secret values are filled in at spawn.
                if !o.headers.is_empty() {
                    let mut parts = vec![];
                    for (i, (name, value)) in o.headers.iter().enumerate() {
                        let sep = if i == 0 { "" } else { "\n" };
                        parts.push(Part::Text(format!("{sep}{name}: ")));
                        parts.push(match value {
                            HeaderValue::Plain(v) => Part::Text(v.clone()),
                            HeaderValue::Secret { .. } => {
                                Part::Secret(crate::accounts::header_ref(account, name))
                            }
                        });
                    }
                    e.composed.push(("ANTHROPIC_CUSTOM_HEADERS".into(), parts));
                }
            }
        }
        Ok(e)
    }
}
