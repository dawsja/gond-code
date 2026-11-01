//! Codex: its interactive CLI runs in a pane (`runner::interactive`); this says where it is,
//! which models it offers, how it signs in and how an account reaches it (an API key through
//! a `-c` provider override, a sign-in through its own `CODEX_HOME`).

use super::driver::{AccountEnv, Driver};
use super::{detect, AgentInfo, AgentKind};
use crate::accounts::HeaderValue;
use crate::accounts::{Account, AccountKind};
use crate::error::{AppError, AppResult};

/// A TOML string for a `-c key=value` override: single-quoted when it can be, else escaped.
fn toml_string(s: &str) -> String {
    if !s.contains('\'') && !s.contains('\n') && !s.contains('\r') {
        return format!("'{s}'");
    }
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub struct Codex;

/// The model provider Gond defines per run for key and endpoint accounts.
const PROVIDER: &str = "gond";
/// Where that provider's key is, in the child's env only.
const KEY_VAR: &str = "GOND_CODEX_API_KEY";
const OPENAI_API: &str = "https://api.openai.com/v1";

/// `{ "k" = 'v', … }` as an inline TOML table for a `-c` override.
fn toml_table(pairs: &[(String, String)]) -> String {
    let body: Vec<String> = pairs
        .iter()
        .map(|(k, v)| format!("{} = {}", toml_string(k), toml_string(v)))
        .collect();
    format!("{{ {} }}", body.join(", "))
}

impl Driver for Codex {
    fn id(&self) -> &str {
        "codex"
    }

    fn harness(&self) -> &'static str {
        "codex"
    }

    fn kind(&self) -> AgentKind {
        AgentKind::Codex
    }

    fn detect(&self) -> AgentInfo {
        let found = detect::find("codex", crate::process::path());
        AgentInfo {
            id: "codex".into(),
            kind: AgentKind::Codex,
            provider: "codex".into(),
            name: "Codex".into(),
            command: "codex".into(),
            installed: found.is_some(),
            version: found.as_deref().and_then(detect::version),
        }
    }

    fn models(&self) -> Vec<super::models::ModelInfo> {
        super::models::codex()
    }

    fn login_status_args(&self) -> Option<&'static [&'static str]> {
        Some(&["login", "status"])
    }

    fn login_args(&self) -> Option<&'static [&'static str]> {
        Some(&["login"])
    }

    /// Sign-ins get their own `CODEX_HOME`.
    fn account_env(&self, account: &Account) -> AppResult<AccountEnv> {
        match account.kind {
            AccountKind::Oauth => {
                let mut e = AccountEnv {
                    remove: vec!["CODEX_HOME"],
                    ..Default::default()
                };
                e.home(account, "CODEX_HOME");
                Ok(e)
            }
            // An OpenAI API key, or any OpenAI-compatible endpoint: a Gond model provider
            // set per run with `-c`, its key read from env.
            AccountKind::ApiKey | AccountKind::Gateway => {
                let o = &account.options;
                let base = match (&account.endpoint, account.kind) {
                    (Some(url), _) => url.as_str(),
                    (None, AccountKind::Gateway) => {
                        return Err(AppError::Invalid(format!(
                            "account \"{}\" has no endpoint",
                            account.label
                        )))
                    }
                    (None, _) => OPENAI_API,
                };
                let official = base.trim_end_matches('/') == OPENAI_API;
                let wire = o.wire_api.clone().unwrap_or_else(|| {
                    if official || o.azure_api_version.is_some() {
                        "responses".into()
                    } else {
                        "chat".into()
                    }
                });
                let set = |k: &str, v: &str| ["-c".to_string(), format!("{k}={}", toml_string(v))];
                let mut args = set("model_provider", PROVIDER).to_vec();
                for (k, v) in [
                    ("name", account.label.as_str()),
                    ("base_url", base),
                    ("env_key", KEY_VAR),
                    ("wire_api", wire.as_str()),
                ] {
                    args.extend(set(&format!("model_providers.{PROVIDER}.{k}"), v));
                }
                // Headers (org, project, the gateway's own) and Azure's api-version.
                // Plain headers go in http_headers; secret ones only by env var name
                // (env_http_headers), their values in the child's env.
                let mut headers: Vec<(String, String)> = vec![];
                let mut env_headers: Vec<(String, String)> = vec![];
                let mut secrets = vec![AccountEnv::secret(account, KEY_VAR)?];
                for (i, (name, value)) in o.headers.iter().enumerate() {
                    match value {
                        HeaderValue::Plain(v) => headers.push((name.clone(), v.clone())),
                        HeaderValue::Secret { .. } => {
                            let var = format!("GOND_CODEX_HEADER_{i}");
                            env_headers.push((name.clone(), var.clone()));
                            secrets.push((var, crate::accounts::header_ref(account, name)));
                        }
                    }
                }
                if !env_headers.is_empty() {
                    args.extend([
                        "-c".to_string(),
                        format!(
                            "model_providers.{PROVIDER}.env_http_headers={}",
                            toml_table(&env_headers)
                        ),
                    ]);
                }
                if let Some(org) = &o.organization {
                    headers.push(("OpenAI-Organization".into(), org.clone()));
                }
                if let Some(p) = &o.project {
                    headers.push(("OpenAI-Project".into(), p.clone()));
                }
                if !headers.is_empty() {
                    args.extend([
                        "-c".to_string(),
                        format!(
                            "model_providers.{PROVIDER}.http_headers={}",
                            toml_table(&headers)
                        ),
                    ]);
                }
                if let Some(v) = &o.azure_api_version {
                    args.extend([
                        "-c".to_string(),
                        format!(
                            "model_providers.{PROVIDER}.query_params={}",
                            toml_table(&[("api-version".into(), v.clone())])
                        ),
                    ]);
                }
                Ok(AccountEnv {
                    remove: vec![KEY_VAR],
                    secrets,
                    args,
                    ..Default::default()
                })
            }
            AccountKind::Token => Err(AppError::Invalid(
                "Codex takes sign-ins, API keys and OpenAI-compatible endpoints".into(),
            )),
        }
    }
}
