//! The Driver trait: one driver per agent harness: where its CLI
//! is, the models it offers, how it signs in and how an account reaches it. The runner
//! starts its interactive CLI in a pane.

use serde::{Deserialize, Serialize};

use super::{AgentInfo, AgentKind};
use crate::accounts::Account;
use crate::error::{AppError, AppResult};
use crate::vault::SecretRef;

pub trait Driver: Send + Sync {
    /// Per agent: a built-in is its harness id; each saved custom agent has its own id.
    fn id(&self) -> &str;
    /// The capability-matrix row this driver implements ("claude", "codex", "custom", …).
    fn harness(&self) -> &'static str;
    fn kind(&self) -> AgentKind;
    /// Find the CLI and its version. Never errors: `installed: false` instead.
    fn detect(&self) -> AgentInfo;
    /// The models this harness offers and the thinking levels each takes.
    /// Empty = the harness picks; the UI shows no picker.
    fn models(&self) -> Vec<super::models::ModelInfo> {
        vec![]
    }
    /// The harness's own sign-in, run in a terminal with a sign-in account's home
    /// (`account.signIn`). `None` = no per-account sign-in.
    fn login_args(&self) -> Option<&'static [&'static str]> {
        None
    }
    /// A command that exits 0 once the account's home is signed in, checked after the
    /// login ends (`account.signIn`). `None`: the login's own exit code decides.
    fn login_status_args(&self) -> Option<&'static [&'static str]> {
        None
    }
    /// How to run as `account`. The runner applies it to the child's
    /// env only; secrets stay references until then.
    fn account_env(&self, account: &Account) -> AppResult<AccountEnv> {
        Err(AppError::Invalid(format!(
            "{} can't run as an account yet",
            account.harness
        )))
    }
}

/// Env changes for one account: unset first (so the user's own keys can't win), then set.
#[derive(Debug, Default, PartialEq)]
pub struct AccountEnv {
    pub remove: Vec<&'static str>,
    pub env: Vec<(String, String)>,
    pub secrets: Vec<(String, SecretRef)>,
    /// Global options placed before the driver's arguments (Codex `-c` overrides).
    pub args: Vec<String>,
    /// Env values built from text and secrets (Claude's header block), resolved at spawn.
    pub composed: Vec<(String, Vec<Part>)>,
}

/// A piece of a composed env value.
#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Text(String),
    Secret(SecretRef),
}

impl AccountEnv {
    /// A sign-in account's harness home under `var`. The harness's own sign-in (no home of
    /// Gond's) keeps whatever `var` the user has, so it runs exactly as their CLI does.
    pub fn home(&mut self, account: &Account, var: &'static str) {
        match &account.config_dir {
            Some(dir) => self.env.push((var.into(), dir.clone())),
            None => self.remove.retain(|v| *v != var),
        }
    }

    /// The account's secret under `var`.
    pub fn secret(account: &Account, var: &str) -> AppResult<(String, SecretRef)> {
        let at = account.secret_ref.clone().ok_or_else(|| {
            AppError::Invalid(format!("account \"{}\" has no key", account.label))
        })?;
        Ok((var.into(), at))
    }
}

/// Thinking or reasoning effort, the union of every harness's vocabulary.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
    Ultra,
}

impl Level {
    /// The word the harnesses take ("xhigh", …), same as the JSON form.
    pub fn as_str(self) -> &'static str {
        match self {
            Level::None => "none",
            Level::Minimal => "minimal",
            Level::Low => "low",
            Level::Medium => "medium",
            Level::High => "high",
            Level::Xhigh => "xhigh",
            Level::Max => "max",
            Level::Ultra => "ultra",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::{claude::Claude, codex::Codex, custom::Custom, CustomAgent};

    /// The harness's own sign-in (no Gond home) runs as the user's CLI does: no home is
    /// set, and the user's own home variable isn't removed either.
    #[test]
    fn an_own_sign_in_keeps_the_users_cli_home() {
        use crate::accounts::{Account, AccountKind};
        let own = |harness: &str| Account {
            id: "A0".into(),
            harness: harness.into(),
            provider: "p".into(),
            label: "me@example.com".into(),
            kind: AccountKind::Oauth,
            secret_ref: None,
            config_dir: None,
            endpoint: None,
            created_at: String::new(),
            options: Default::default(),
            identity: None,
            is_default: true,
            own: true,
        };
        for (driver, var) in [
            (&Claude as &dyn Driver, "CLAUDE_CONFIG_DIR"),
            (&super::super::codex::Codex, "CODEX_HOME"),
            (&super::super::grok::Grok, "GROK_HOME"),
        ] {
            let e = driver.account_env(&own(driver.id())).unwrap();
            assert!(!e.env.iter().any(|(k, _)| k == var), "{var} not set");
            assert!(!e.remove.contains(&var), "{var} not removed");
        }
    }

    #[test]
    fn account_env_per_harness() {
        use crate::accounts::{Account, AccountKind};
        let acct = |harness: &str, kind| Account {
            id: "A1".into(),
            harness: harness.into(),
            provider: "p".into(),
            label: "work".into(),
            kind,
            secret_ref: (kind != AccountKind::Oauth).then(|| SecretRef::for_account(harness, "A1")),
            config_dir: (kind == AccountKind::Oauth).then(|| "/homes/A1".into()),
            endpoint: None,
            created_at: String::new(),
            options: Default::default(),
            identity: None,
            is_default: false,
            own: false,
        };
        let e = Claude
            .account_env(&acct("claude", AccountKind::Oauth))
            .unwrap();
        assert_eq!(
            e.env,
            [("CLAUDE_CONFIG_DIR".to_string(), "/homes/A1".to_string())]
        );
        assert!(e.remove.contains(&"ANTHROPIC_API_KEY") && e.secrets.is_empty());
        let e = Claude
            .account_env(&acct("claude", AccountKind::ApiKey))
            .unwrap();
        assert_eq!(e.secrets[0].0, "ANTHROPIC_API_KEY");
        assert!(e.env.is_empty());
        let e = Claude
            .account_env(&acct("claude", AccountKind::Token))
            .unwrap();
        assert_eq!(e.secrets[0].0, "CLAUDE_CODE_OAUTH_TOKEN");
        let e = Codex
            .account_env(&acct("codex", AccountKind::Oauth))
            .unwrap();
        assert_eq!(e.env, [("CODEX_HOME".to_string(), "/homes/A1".to_string())]);
        // An OpenAI key: a per-run provider at api.openai.com, the key in env only.
        let e = Codex
            .account_env(&acct("codex", AccountKind::ApiKey))
            .unwrap();
        let a = e.args.join(" ");
        assert!(a.starts_with("-c model_provider='gond'"), "{a}");
        assert!(a.contains("model_providers.gond.base_url='https://api.openai.com/v1'"));
        assert!(a.contains("model_providers.gond.env_key='GOND_CODEX_API_KEY'"));
        assert!(a.contains("model_providers.gond.wire_api='responses'"));
        assert_eq!(e.secrets[0].0, "GOND_CODEX_API_KEY");
        assert!(!a.contains("sk-"), "never the key itself");
        // Any OpenAI-compatible endpoint.
        let mut gw = acct("codex", AccountKind::Gateway);
        gw.endpoint = Some("https://llm.example.com/v1".into());
        let a = Codex.account_env(&gw).unwrap().args.join(" ");
        assert!(
            a.contains("base_url='https://llm.example.com/v1'") && a.contains("wire_api='chat'")
        );
        assert!(Codex
            .account_env(&acct("codex", AccountKind::Token))
            .is_err());
        // Claude through an Anthropic-API endpoint.
        let mut cgw = acct("claude", AccountKind::Gateway);
        cgw.endpoint = Some("https://gateway.example.com".into());
        let e = Claude.account_env(&cgw).unwrap();
        assert_eq!(
            e.env,
            [(
                "ANTHROPIC_BASE_URL".to_string(),
                "https://gateway.example.com".to_string()
            )]
        );
        assert_eq!(e.secrets[0].0, "ANTHROPIC_API_KEY");
        assert!(e.remove.contains(&"ANTHROPIC_BASE_URL"));
        let custom = Custom(CustomAgent {
            id: "C".into(),
            name: "c".into(),
            command: "c".into(),
            args: vec![],
            provider: None,
            prompt_via: Default::default(),
        });
        assert!(custom
            .account_env(&acct("custom", AccountKind::ApiKey))
            .is_err());
    }

    #[test]
    fn api_account_options_map_per_harness() {
        use crate::accounts::{
            Account, AccountKind, AccountOptions, Cloud, HeaderValue, SecretSlot,
        };
        let api = |harness: &str, endpoint: Option<&str>, options: AccountOptions| Account {
            id: "A1".into(),
            harness: harness.into(),
            provider: "p".into(),
            label: "proxy".into(),
            kind: AccountKind::ApiKey,
            secret_ref: Some(SecretRef::for_account(harness, "A1")),
            config_dir: None,
            endpoint: endpoint.map(String::from),
            created_at: String::new(),
            options,
            identity: None,
            is_default: false,
            own: false,
        };
        // Claude through a bearer gateway with a routing header.
        let e = Claude
            .account_env(&api(
                "claude",
                Some("https://litellm.local.test"),
                AccountOptions {
                    bearer: true,
                    headers: [
                        ("X-Team".to_string(), HeaderValue::Plain("core".into())),
                        (
                            "X-Gw-Token".to_string(),
                            HeaderValue::Secret {
                                secret: SecretSlot::Stored(true),
                            },
                        ),
                    ]
                    .into(),
                    ..Default::default()
                },
            ))
            .unwrap();
        assert!(e.env.contains(&(
            "ANTHROPIC_BASE_URL".into(),
            "https://litellm.local.test".into()
        )));
        // BTreeMap order: X-Gw-Token, then X-Team; the secret stays a vault reference.
        let (var, parts) = &e.composed[0];
        assert_eq!(var, "ANTHROPIC_CUSTOM_HEADERS");
        assert_eq!(parts[0], Part::Text("X-Gw-Token: ".into()));
        assert!(matches!(&parts[1], Part::Secret(r) if r.0.ends_with("/header/x-gw-token")));
        assert_eq!(parts[2], Part::Text("\nX-Team: ".into()));
        assert_eq!(parts[3], Part::Text("core".into()));
        assert!(e.env.iter().all(|(k, _)| k != "ANTHROPIC_CUSTOM_HEADERS"));
        assert_eq!(e.secrets[0].0, "ANTHROPIC_AUTH_TOKEN");
        // Claude on Bedrock and Vertex.
        let b = Claude
            .account_env(&api(
                "claude",
                None,
                AccountOptions {
                    cloud: Some(Cloud::Bedrock {
                        region: "us-east-1".into(),
                    }),
                    ..Default::default()
                },
            ))
            .unwrap();
        assert!(b
            .env
            .contains(&("CLAUDE_CODE_USE_BEDROCK".into(), "1".into())));
        assert_eq!(b.secrets[0].0, "AWS_BEARER_TOKEN_BEDROCK");
        let v = Claude
            .account_env(&api(
                "claude",
                None,
                AccountOptions {
                    cloud: Some(Cloud::Vertex {
                        region: "us-east5".into(),
                        project: "p1".into(),
                    }),
                    ..Default::default()
                },
            ))
            .unwrap();
        assert!(v
            .env
            .contains(&("ANTHROPIC_VERTEX_PROJECT_ID".into(), "p1".into())));
        assert!(
            v.secrets.is_empty(),
            "Vertex uses the machine's Google credentials"
        );
        // Codex on Azure OpenAI with an org.
        let a = Codex
            .account_env(&api(
                "codex",
                Some("https://acme.openai.azure.com/openai"),
                AccountOptions {
                    azure_api_version: Some("2025-04-01-preview".into()),
                    organization: Some("org-1".into()),
                    ..Default::default()
                },
            ))
            .unwrap()
            .args
            .join(" ");
        assert!(a.contains("wire_api='responses'"), "{a}");
        assert!(
            a.contains("query_params={ 'api-version' = '2025-04-01-preview' }"),
            "{a}"
        );
        assert!(
            a.contains("http_headers={ 'OpenAI-Organization' = 'org-1' }"),
            "{a}"
        );
        // A local OpenAI-compatible server speaks chat by default.
        let l = Codex
            .account_env(&api(
                "codex",
                Some("http://localhost:11434/v1"),
                Default::default(),
            ))
            .unwrap()
            .args
            .join(" ");
        assert!(
            l.contains("base_url='http://localhost:11434/v1'") && l.contains("wire_api='chat'")
        );
    }
}
