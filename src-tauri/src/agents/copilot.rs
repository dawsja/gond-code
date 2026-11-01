//! GitHub Copilot CLI (`copilot`, github/copilot-cli): its interactive CLI runs in a pane
//! (`runner::interactive`); this says where it is, which models it offers and how it signs in.

use super::driver::{AccountEnv, Driver, Part};
use super::{detect, AgentInfo, AgentKind};
use crate::accounts::{Account, AccountKind, HeaderValue};
use crate::error::{AppError, AppResult};

pub struct Copilot;

/// The tokens Copilot reads before its stored sign-in, in its order of precedence.
const TOKEN_VARS: [&str; 3] = ["COPILOT_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"];

impl Driver for Copilot {
    fn id(&self) -> &str {
        "copilot"
    }

    fn harness(&self) -> &'static str {
        "copilot"
    }

    fn kind(&self) -> AgentKind {
        AgentKind::Copilot
    }

    fn detect(&self) -> AgentInfo {
        let found = detect::find("copilot", crate::process::path());
        AgentInfo {
            id: "copilot".into(),
            kind: AgentKind::Copilot,
            provider: "copilot".into(),
            name: "GitHub Copilot".into(),
            command: "copilot".into(),
            installed: found.is_some(),
            version: found.as_deref().and_then(detect::version),
        }
    }

    fn models(&self) -> Vec<super::models::ModelInfo> {
        super::models::copilot()
    }

    fn login_args(&self) -> Option<&'static [&'static str]> {
        Some(&["login"])
    }

    /// Sign-ins get their own `COPILOT_HOME`, with the GitHub tokens Copilot would read
    /// first unset so the sign-in is the one used. A fine-grained token goes in
    /// `COPILOT_GITHUB_TOKEN`; a custom endpoint is Copilot's own provider (BYOK) env.
    fn account_env(&self, account: &Account) -> AppResult<AccountEnv> {
        let mut e = AccountEnv {
            remove: vec![
                "COPILOT_HOME",
                "COPILOT_PROVIDER_BASE_URL",
                "COPILOT_PROVIDER_TYPE",
                "COPILOT_PROVIDER_API_KEY",
                "COPILOT_PROVIDER_BEARER_TOKEN",
                "COPILOT_PROVIDER_HEADERS",
                "COPILOT_PROVIDER_WIRE_API",
                "COPILOT_PROVIDER_AZURE_API_VERSION",
            ],
            ..Default::default()
        };
        let o = &account.options;
        match account.kind {
            AccountKind::Oauth => {
                if account.config_dir.is_some() {
                    e.remove.extend(TOKEN_VARS);
                }
                e.home(account, "COPILOT_HOME");
            }
            AccountKind::Token => e
                .secrets
                .push(AccountEnv::secret(account, "COPILOT_GITHUB_TOKEN")?),
            // Copilot has no key of its own: a key is always for a model endpoint (BYOK).
            AccountKind::ApiKey | AccountKind::Gateway => {
                let url = account.endpoint.clone().ok_or_else(|| {
                    AppError::Invalid(format!(
                        "account \"{}\" needs the model endpoint's base URL",
                        account.label
                    ))
                })?;
                e.env.push(("COPILOT_PROVIDER_BASE_URL".into(), url));
                match &o.azure_api_version {
                    Some(v) => {
                        e.env.push(("COPILOT_PROVIDER_TYPE".into(), "azure".into()));
                        e.env
                            .push(("COPILOT_PROVIDER_AZURE_API_VERSION".into(), v.clone()));
                    }
                    None => e
                        .env
                        .push(("COPILOT_PROVIDER_TYPE".into(), "openai".into())),
                }
                if let Some(api) = &o.wire_api {
                    let wire = if api == "chat" {
                        "completions"
                    } else {
                        "responses"
                    };
                    e.env
                        .push(("COPILOT_PROVIDER_WIRE_API".into(), wire.into()));
                }
                let var = if o.bearer {
                    "COPILOT_PROVIDER_BEARER_TOKEN"
                } else {
                    "COPILOT_PROVIDER_API_KEY"
                };
                e.secrets.push(AccountEnv::secret(account, var)?);
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
                    e.composed.push(("COPILOT_PROVIDER_HEADERS".into(), parts));
                }
            }
        }
        Ok(e)
    }
}
