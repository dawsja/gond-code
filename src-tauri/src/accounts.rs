//! Harness accounts: as many as you like per harness, each a
//! sign-in in its own Gond-owned config dir or a key held in the vault. A secret never
//! leaves the vault except into a child's env at spawn.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::store::{accounts as rows, now, Store};
use crate::vault::{Secret, SecretRef, Vault};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum AccountKind {
    /// The harness's own sign-in (subscription), run in a Gond-owned config dir.
    Oauth,
    ApiKey,
    /// A long-lived token from the harness (e.g. `claude setup-token`).
    Token,
    /// An enterprise gateway: `endpoint` plus a key.
    Gateway,
}

impl AccountKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AccountKind::Oauth => "oauth",
            AccountKind::ApiKey => "apiKey",
            AccountKind::Token => "token",
            AccountKind::Gateway => "gateway",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "apiKey" => AccountKind::ApiKey,
            "token" => AccountKind::Token,
            "gateway" => AccountKind::Gateway,
            _ => AccountKind::Oauth,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub harness: String,
    pub provider: String,
    pub label: String,
    pub kind: AccountKind,
    /// Where the key lives in the vault. Internal: never sent to the UI or agents.
    #[serde(skip)]
    pub secret_ref: Option<SecretRef>,
    /// The harness home for this sign-in. Internal, like `secret_ref`.
    #[serde(skip)]
    pub config_dir: Option<String>,
    /// The base URL, for API accounts on a custom endpoint and gateways.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "AccountOptions::is_empty")]
    pub options: AccountOptions,
    /// Who it signs in as: "sam@example.com · Claude Max" for a sign-in, or
    /// "anthropic key …a1b2 at api.example.com" for a key. Same harness and identity =
    /// the same account, so signing in twice updates instead of duplicating.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    /// The account new threads on this harness use when none is picked.
    #[serde(default)]
    pub is_default: bool,
    /// The harness's own sign-in: a sign-in with no Gond-owned home, so it runs as the
    /// CLI does for you (`~/.claude`, `~/.codex`, …). Worked out from the row, never stored.
    #[serde(default, skip_deserializing)]
    pub own: bool,
}

impl Account {
    /// Whether this is the harness's own sign-in (see `own`).
    pub fn is_own(&self) -> bool {
        self.kind == AccountKind::Oauth && self.config_dir.is_none()
    }
}

/// How an API account talks to its endpoint (OpenAI and Anthropic API URLs, or any
/// compatible one). No secrets here: only the key is secret, and it lives in the vault.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountOptions {
    /// Send the key as `Authorization: Bearer` instead of the provider's own header
    /// (Anthropic `x-api-key`): most proxies and gateways (LiteLLM, OpenRouter) want this.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bearer: bool,
    /// Extra request headers (same shape as MCP servers): a plain value, or
    /// `{ secret }`, whose value goes to Credential Manager and reads back as `{ secret: true }`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, HeaderValue>,
    /// OpenAI organisation and project ids (`OpenAI-Organization`, `OpenAI-Project`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// Azure OpenAI's `api-version` query parameter; set means the endpoint is Azure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub azure_api_version: Option<String>,
    /// For Codex: `responses` or `chat`. Default: `responses` on api.openai.com and Azure,
    /// `chat` on any other OpenAI-compatible endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wire_api: Option<String>,
    /// Claude through a cloud: AWS Bedrock or Google Vertex AI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud: Option<Cloud>,
    /// What this account pays, for harnesses that don't report a cost. Gond
    /// never guesses prices: without it, those threads show tokens only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing: Option<Pricing>,
}

/// US dollars per million tokens.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pricing {
    pub input_per_mtok: f64,
    pub output_per_mtok: f64,
}

impl Pricing {
    /// The cost of one turn's tokens.
    pub fn cost(&self, input: u64, output: u64) -> f64 {
        (input as f64 * self.input_per_mtok + output as f64 * self.output_per_mtok) / 1e6
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum HeaderValue {
    Plain(String),
    Secret { secret: SecretSlot },
}

/// A header's secret: the value on the way in, `true` once it's in the vault.
#[derive(JsonSchema)]
#[serde(untagged)]
pub enum SecretSlot {
    Stored(bool),
    Given(#[schemars(with = "String")] Secret),
}

impl SecretSlot {
    fn given(&self) -> Option<&Secret> {
        match self {
            SecretSlot::Given(s) => Some(s),
            SecretSlot::Stored(_) => None,
        }
    }
}

impl Clone for SecretSlot {
    fn clone(&self) -> Self {
        match self {
            SecretSlot::Stored(b) => SecretSlot::Stored(*b),
            SecretSlot::Given(s) => SecretSlot::Given(Secret::new(s.expose())),
        }
    }
}

impl PartialEq for SecretSlot {
    fn eq(&self, other: &Self) -> bool {
        matches!((self, other), (SecretSlot::Stored(a), SecretSlot::Stored(b)) if a == b)
    }
}

impl std::fmt::Debug for SecretSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretSlot(***)")
    }
}

/// Never the value: a secret always serializes as `true`.
impl Serialize for SecretSlot {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bool(true)
    }
}

impl<'de> Deserialize<'de> for SecretSlot {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match serde_json::Value::deserialize(d)? {
            serde_json::Value::Bool(b) => Ok(SecretSlot::Stored(b)),
            serde_json::Value::String(s) => Ok(SecretSlot::Given(Secret::new(s))),
            _ => Err(serde::de::Error::custom("a header secret is a string")),
        }
    }
}

/// Where a header's secret lives in the vault (derived, so nothing stores the ref).
pub fn header_ref(account: &Account, name: &str) -> SecretRef {
    SecretRef::for_account(
        &account.harness,
        &format!("{}/header/{}", account.id, name.to_lowercase()),
    )
}

/// Header names that carry credentials: their value must be `{ secret }`.
fn auth_header(name: &str) -> bool {
    let n = name.to_lowercase();
    n == "authorization"
        || n == "x-api-key"
        || n == "api-key"
        || n.contains("token")
        || n.contains("secret")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Cloud {
    /// The key is a Bedrock API key (`AWS_BEARER_TOKEN_BEDROCK`).
    Bedrock { region: String },
    /// Vertex uses the machine's Google credentials (ADC); no key in Gond.
    Vertex { region: String, project: String },
}

impl AccountOptions {
    pub fn is_empty(&self) -> bool {
        *self == AccountOptions::default()
    }
}

/// What `account.add` takes. The secret goes straight into the vault.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewAccount {
    pub harness: String,
    pub label: String,
    pub kind: AccountKind,
    /// A `Secret`, so `{:?}` never prints it.
    #[serde(default)]
    #[schemars(with = "Option<String>")]
    pub secret: Option<Secret>,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub options: AccountOptions,
}

/// Which account kinds a harness supports, and how many sign-ins it can hold side by side.
pub struct HarnessAccounts {
    pub provider: &'static str,
    pub kinds: &'static [AccountKind],
    /// `Some(1)` where the harness has no config-dir override (agy, HARNESSES.md finding 3).
    pub max_oauth: Option<usize>,
}

use AccountKind::*;

/// , for the v0.1 drivers. Unknown harnesses take no accounts.
pub fn rules(harness: &str) -> Option<HarnessAccounts> {
    let (provider, kinds, max_oauth): (_, &'static [AccountKind], _) = match harness {
        // Gateways: any Anthropic- or OpenAI-compatible endpoint (endpoint + key).
        "claude" => ("anthropic", &[Oauth, ApiKey, Token, Gateway], None),
        "codex" => ("openai", &[Oauth, ApiKey, Gateway], None),
        "agy" => ("google", &[Oauth, ApiKey, Gateway], Some(1)),
        "grok" => ("xai", &[Oauth, ApiKey], None),
        // Copilot: GitHub sign-ins (COPILOT_HOME), fine-grained tokens, or a model endpoint.
        "copilot" => ("github", &[Oauth, Token, ApiKey, Gateway], None),
        // Cursor keeps one sign-in per Windows user (`%APPDATA%\Cursor\auth.json`).
        "cursor" => ("cursor", &[Oauth, ApiKey], Some(1)),
        _ => return None,
    };
    Some(HarnessAccounts {
        provider,
        kinds,
        max_oauth,
    })
}

/// Where Gond keeps sign-in homes: `%LOCALAPPDATA%\GondCode\accounts` on Windows,
/// `$XDG_DATA_HOME/gond-code/accounts` on the Linux dev path (like the database).
pub fn default_root() -> Option<PathBuf> {
    crate::appdata::local().map(|d| d.join("accounts"))
}

pub fn list(
    store: &Store,
    harness: Option<&str>,
    provider: Option<&str>,
) -> AppResult<Vec<Account>> {
    Ok(store
        .with(rows::list)?
        .into_iter()
        .filter(|a| harness.is_none_or(|h| a.harness == h))
        .filter(|a| provider.is_none_or(|p| a.provider == p))
        .collect())
}

pub fn add(store: &Store, vault: &dyn Vault, root: &Path, new: NewAccount) -> AppResult<Account> {
    let harness = new.harness.trim().to_lowercase();
    let label = new.label.trim().to_string();
    let rules = rules(&harness)
        .ok_or_else(|| AppError::Invalid(format!("{harness} doesn't support accounts yet")))?;
    if label.is_empty() {
        return Err(AppError::Invalid("an account needs a label".into()));
    }
    if !rules.kinds.contains(&new.kind) {
        return Err(AppError::Invalid(format!(
            "{harness} doesn't take {} accounts",
            new.kind.as_str()
        )));
    }
    let secret = new.secret.filter(|s| !s.expose().trim().is_empty());
    match (new.kind, &secret) {
        (Oauth, Some(_)) => {
            return Err(AppError::Invalid(
                "a sign-in account takes no secret: sign in through the harness".into(),
            ))
        }
        (Oauth, None) => {}
        (_, None) => {
            return Err(AppError::Invalid(format!(
                "a {} account needs its key",
                new.kind.as_str()
            )))
        }
        _ => {}
    }
    let endpoint = new
        .endpoint
        .map(|e| e.trim().to_string())
        .filter(|e| !e.is_empty());
    // A gateway needs an endpoint; an API key may have one (a custom base URL: a proxy,
    // Azure, OpenRouter, LiteLLM, a local server). Sign-ins and tokens never do.
    if new.kind == Gateway && endpoint.is_none() {
        return Err(AppError::Invalid(
            "a gateway account needs an endpoint".into(),
        ));
    }
    // Copilot has no key of its own: its keys are for a model endpoint (BYOK).
    if harness == "copilot" && new.kind == ApiKey && endpoint.is_none() {
        return Err(AppError::Invalid(
            "a Copilot API key is for a model endpoint: add its base URL".into(),
        ));
    }
    if matches!(new.kind, Oauth | Token) && endpoint.is_some() {
        return Err(AppError::Invalid(format!(
            "a {} account takes no endpoint",
            new.kind.as_str()
        )));
    }
    if let Some(url) = &endpoint {
        if !(url.starts_with("https://")
            || url.starts_with("http://localhost")
            || url.starts_with("http://127.0.0.1"))
        {
            return Err(AppError::Invalid(
                "an endpoint must be https:// (or http:// on localhost for a local server)".into(),
            ));
        }
    }
    if let Some((name, _)) = new
        .options
        .headers
        .iter()
        .find(|(n, v)| auth_header(n) && matches!(v, HeaderValue::Plain(_)))
    {
        return Err(AppError::Invalid(format!(
            "the {name} header carries a credential: give it as a secret"
        )));
    }
    if new.kind == Oauth {
        if let Some(max) = rules.max_oauth {
            let have = list(store, Some(&harness), None)?
                .iter()
                .filter(|a| a.kind == Oauth)
                .count();
            if have >= max {
                return Err(AppError::Invalid(format!(
                    "{harness} allows one sign-in per Windows user; add more as API-key or gateway accounts"
                )));
            }
        }
    }

    // The same key to the same place is the same account: no second row.
    let identity = secret
        .as_ref()
        .map(|k| key_identity(rules.provider, k.expose(), endpoint.as_deref()));
    if let Some(who) = &identity {
        if let Some(twin) = list(store, Some(&harness), None)?
            .into_iter()
            .find(|a| a.identity.as_ref() == Some(who))
        {
            return Err(AppError::Invalid(format!(
                "that key is already saved as \"{}\"",
                twin.label
            )));
        }
    }

    let id = ulid::Ulid::generate().to_string();
    // A harness without config-dir isolation uses its own default sign-in.
    let config_dir = (new.kind == Oauth && rules.max_oauth.is_none())
        .then(|| root.join(&harness).join(&id).to_string_lossy().into_owned());
    let secret_ref = secret
        .as_ref()
        .map(|_| SecretRef::for_account(&harness, &id));
    if let (Some(at), Some(s)) = (&secret_ref, secret) {
        vault.put(at, &s)?;
    }
    let mut account = Account {
        id,
        harness,
        provider: rules.provider.into(),
        label,
        kind: new.kind,
        secret_ref,
        config_dir,
        endpoint,
        created_at: now(),
        options: new.options,
        identity,
        is_default: false,
        own: false,
    };
    account.own = account.is_own();
    // Secret header values go to the vault; the stored options keep only `{ secret: true }`.
    let given: Vec<(String, Secret)> = account
        .options
        .headers
        .iter()
        .filter_map(|(n, v)| match v {
            HeaderValue::Secret { secret } => {
                secret.given().map(|s| (n.clone(), Secret::new(s.expose())))
            }
            HeaderValue::Plain(_) => None,
        })
        .collect();
    for (name, value) in &given {
        vault.put(&header_ref(&account, name), value)?;
    }
    for v in account.options.headers.values_mut() {
        if let HeaderValue::Secret { secret } = v {
            *secret = SecretSlot::Stored(true);
        }
    }
    if let Err(e) = store.with(|c| rows::insert(c, &account)) {
        if let Some(at) = &account.secret_ref {
            let _ = vault.delete(at);
        }
        return Err(e);
    }
    Ok(account)
}

/// Adds the harness's own sign-in (the CLI's login, found at startup) as an account: no
/// home of Gond's, so it runs exactly as the CLI does for you. The default for its harness
/// when nothing else is.
pub fn add_own(store: &Store, harness: &str, who: &Identity) -> AppResult<Account> {
    let rules =
        rules(harness).ok_or_else(|| AppError::Invalid(format!("{harness} takes no accounts")))?;
    let others = list(store, Some(harness), None)?;
    let account = Account {
        id: ulid::Ulid::generate().to_string(),
        harness: harness.into(),
        provider: rules.provider.into(),
        label: who.label.clone(),
        kind: Oauth,
        secret_ref: None,
        config_dir: None,
        endpoint: None,
        created_at: now(),
        options: AccountOptions::default(),
        identity: Some(who.key.clone()),
        is_default: !others.iter().any(|a| a.is_default),
        own: true,
    };
    store.with(|c| rows::insert(c, &account))?;
    Ok(account)
}

/// Removes the account, its secret and its sign-in home. The home is only deleted when it
/// really sits under `root`, so a bad row can never point it elsewhere. Refused while an
/// agent is running as the account.
pub fn remove(store: &Store, vault: &dyn Vault, root: &Path, id: &str) -> AppResult<()> {
    let account = store
        .with(|c| rows::get(c, id))?
        .ok_or_else(|| AppError::NotFound(format!("account {id} not found")))?;
    if store.with(|c| crate::store::threads::account_in_use(c, id))? {
        return Err(AppError::Invalid(format!(
            "an agent is running as \"{}\"; stop it first",
            account.label
        )));
    }
    if let Some(at) = &account.secret_ref {
        vault.delete(at)?;
    }
    for (name, v) in &account.options.headers {
        if matches!(v, HeaderValue::Secret { .. }) {
            vault.delete(&header_ref(&account, name))?;
        }
    }
    store.with(|c| rows::delete(c, id))?;
    if let Some(dir) = &account.config_dir {
        remove_home(root, dir);
    }
    Ok(())
}

/// Who a sign-in is signed in as: `key` decides sameness, `label` names the row.
#[derive(Debug, Clone, PartialEq)]
pub struct Identity {
    pub key: String,
    pub label: String,
}

fn plan_name(provider: &str, plan: &str) -> String {
    let plan = plan.trim();
    let mut c = plan.chars();
    let titled = c
        .next()
        .map(|f| f.to_uppercase().chain(c).collect::<String>())
        .unwrap_or_default();
    match provider {
        "anthropic" => format!("Claude {titled}"),
        "openai" => format!("ChatGPT {titled}"),
        _ => titled,
    }
}

fn identity(provider: &str, email: &str, org: Option<&str>, plan: Option<&str>) -> Identity {
    let email = email.trim().to_lowercase();
    let key = match org.map(str::trim).filter(|o| !o.is_empty()) {
        Some(o) => format!("{email}/{o}"),
        None => email.clone(),
    };
    let label = match plan.map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) => format!("{email} · {}", plan_name(provider, p)),
        None => email,
    };
    Identity { key, label }
}

/// Reads a harness's status output (`claude auth status` prints JSON) for the email,
/// organisation and plan; plain text falls back to the first email address in it.
pub fn identity_from_status(provider: &str, text: &str) -> Option<Identity> {
    let json = text
        .find('{')
        .and_then(|at| serde_json::from_str::<serde_json::Value>(text[at..].trim()).ok());
    if let Some(v) = json {
        let pick = |paths: &[&str]| {
            paths
                .iter()
                .find_map(|p| v.pointer(p).and_then(|x| x.as_str()).map(String::from))
        };
        let email = pick(&[
            "/email",
            "/emailAddress",
            "/account/email",
            "/account/emailAddress",
        ]);
        if let Some(email) = email.filter(|e| e.contains('@')) {
            let org = pick(&["/orgId", "/organizationId", "/account/organizationUuid"]);
            let plan = pick(&["/subscriptionType", "/plan", "/account/subscriptionType"]);
            return Some(identity(provider, &email, org.as_deref(), plan.as_deref()));
        }
    }
    let re = regex::Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}").ok()?;
    re.find(text)
        .map(|m| identity(provider, m.as_str(), None, None))
}

fn base64url(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for b in s.bytes().take_while(|&b| b != b'=') {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// Codex's `login status` names no one, so read the email and plan from the ID token
/// in its home's `auth.json` (the claims only; the token itself is never kept).
pub fn identity_from_codex_home(home: &Path) -> Option<Identity> {
    let auth: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(home.join("auth.json")).ok()?).ok()?;
    let token = auth.pointer("/tokens/id_token")?.as_str()?;
    let claims: serde_json::Value =
        serde_json::from_slice(&base64url(token.split('.').nth(1)?)?).ok()?;
    let email = claims["email"].as_str()?;
    let openai = &claims["https://api.openai.com/auth"];
    Some(identity(
        "openai",
        email,
        openai["chatgpt_account_id"].as_str(),
        openai["chatgpt_plan_type"].as_str(),
    ))
}

/// Who a Copilot home is signed in as: its config's last signed-in GitHub user. A user on
/// another host than github.com (GitHub Enterprise) is named with it.
pub fn identity_from_copilot_home(home: &Path) -> Option<Identity> {
    let (host, login) = copilot_user(home)?;
    let host = host.trim_start_matches("https://").trim_end_matches('/');
    let name = match host {
        "" | "github.com" => login,
        h => format!("{login}@{h}"),
    };
    Some(identity("github", &name, None, None))
}

/// The host and login of the user Copilot last signed in as in `home` (`config.json`'s
/// `lastLoggedInUser`; the file starts with `//` comment lines).
pub fn copilot_user(home: &Path) -> Option<(String, String)> {
    let text = std::fs::read_to_string(home.join("config.json")).ok()?;
    let json: String = text
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join(
            "
",
        );
    let config: serde_json::Value = serde_json::from_str(&json).ok()?;
    let user = &config["lastLoggedInUser"];
    let login = user["login"].as_str().filter(|l| !l.trim().is_empty())?;
    let host = user["host"].as_str().unwrap_or("https://github.com");
    Some((host.to_string(), login.to_string()))
}

/// A key account's identity: its provider, where it goes, and a fingerprint of the key
/// (a hash prefix, so the key can't be read back from it).
pub fn key_identity(provider: &str, key: &str, endpoint: Option<&str>) -> String {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(key.trim().as_bytes());
    let print: String = hash[..6].iter().map(|b| format!("{b:02x}")).collect();
    let host = endpoint
        .map(|e| {
            e.trim_start_matches("https://")
                .trim_start_matches("http://")
                .split('/')
                .next()
                .unwrap_or(e)
                .to_lowercase()
        })
        .unwrap_or_else(|| "default".into());
    format!("{provider} key {print} at {host}")
}

/// Whether `label` is still the name Gond gave the row, so a real one may replace it.
fn generated_label(a: &Account) -> bool {
    let name = crate::harness::get(&a.harness).map_or(a.harness.as_str(), |h| h.name);
    a.label == format!("{name} sign-in") || a.label == a.harness || a.label.is_empty()
}

/// After `fresh` signs in as `who`: if another account on its harness already is `who`,
/// that one takes over `fresh`'s home and `fresh` goes (returned: the account kept).
/// Otherwise `fresh` keeps its identity and, if it still has Gond's name, `who`'s label.
pub fn adopt(
    store: &Store,
    root: &Path,
    fresh: &str,
    who: &Identity,
) -> AppResult<Option<Account>> {
    let a = store
        .with(|c| rows::get(c, fresh))?
        .ok_or_else(|| AppError::NotFound(format!("account {fresh} not found")))?;
    let twin = list(store, Some(&a.harness), None)?
        .into_iter()
        .filter(|o| o.id != a.id && o.identity.as_deref() == Some(who.key.as_str()))
        .min_by(|x, y| (&x.created_at, &x.id).cmp(&(&y.created_at, &y.id)));
    let Some(keep) = twin else {
        store.with(|c| {
            rows::set_identity(c, &a.id, Some(&who.key))?;
            if generated_label(&a) {
                rows::set_label(c, &a.id, &who.label)?;
            }
            Ok(())
        })?;
        return Ok(None);
    };
    // The fresh home is the newest sign-in; the old one's is dropped.
    let old_home = keep.config_dir.clone();
    store.with(|c| {
        rows::set_config_dir(c, &keep.id, a.config_dir.as_deref())?;
        rows::repoint_threads(c, &a.id, &keep.id)?;
        if a.is_default {
            rows::make_default(c, &keep.id)?;
        }
        rows::delete(c, &a.id).map(|_| ())
    })?;
    if let Some(dir) = old_home.filter(|d| Some(d) != a.config_dir.as_ref()) {
        remove_home(root, &dir);
    }
    store.with(|c| rows::get(c, &keep.id))
}

/// Startup: merges accounts on one harness with the same identity into the oldest,
/// moving their threads over. Returns how many went.
pub fn merge_duplicates(store: &Store, vault: &dyn Vault, root: &Path) -> AppResult<usize> {
    let mut all = list(store, None, None)?;
    all.sort_by(|x, y| (&x.created_at, &x.id).cmp(&(&y.created_at, &y.id)));
    let mut kept: BTreeMap<(String, String), Account> = BTreeMap::new();
    let mut gone = 0;
    for a in all {
        let Some(who) = a.identity.clone() else {
            continue;
        };
        let Some(keep) = kept.get(&(a.harness.clone(), who.clone())) else {
            kept.insert((a.harness.clone(), who), a);
            continue;
        };
        store.with(|c| {
            rows::repoint_threads(c, &a.id, &keep.id)?;
            if a.is_default {
                rows::make_default(c, &keep.id)?;
            }
            Ok(())
        })?;
        remove(store, vault, root, &a.id)?;
        gone += 1;
    }
    Ok(gone)
}

/// Fills in identities that can be worked out without running anything: key accounts
/// (from the vault) and Codex sign-ins (from their home). Returns the ids still unknown.
pub fn backfill_identities(store: &Store, vault: &dyn Vault) -> AppResult<Vec<String>> {
    let mut unknown = Vec::new();
    for a in list(store, None, None)?
        .into_iter()
        .filter(|a| a.identity.is_none())
    {
        let found = match (&a.secret_ref, &a.config_dir) {
            (Some(at), _) => vault
                .get(at)?
                .map(|k| key_identity(&a.provider, k.expose(), a.endpoint.as_deref())),
            (None, Some(home)) if a.harness == "codex" => identity_from_codex_home(Path::new(home))
                .map(|i| {
                    if generated_label(&a) {
                        let _ = store.with(|c| rows::set_label(c, &a.id, &i.label));
                    }
                    i.key
                }),
            _ => None,
        };
        match found {
            Some(who) => store.with(|c| rows::set_identity(c, &a.id, Some(&who)))?,
            None => unknown.push(a.id),
        }
    }
    Ok(unknown)
}

fn remove_home(root: &Path, dir: &str) {
    if let (Ok(dir), Ok(root)) = (Path::new(dir).canonicalize(), root.canonicalize()) {
        if dir.starts_with(&root) && dir != root {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::vault::{MemoryVault, SecretResolver};

    /// The CLI's own login, added as an account: no Gond home, its identity and label,
    /// the default while nothing else is, and `own` both when made and when read back.
    #[test]
    fn the_clis_own_sign_in_becomes_an_account() {
        let store = Store::open_in_memory().unwrap();
        let who = Identity {
            key: "me@example.com".into(),
            label: "me@example.com · Claude Max".into(),
        };
        let a = add_own(&store, "claude", &who).unwrap();
        assert!(a.own && a.is_default && a.config_dir.is_none());
        assert_eq!(
            (a.kind, a.provider.as_str(), a.identity.as_deref()),
            (AccountKind::Oauth, "anthropic", Some("me@example.com"))
        );
        let back = list(&store, Some("claude"), None).unwrap();
        assert_eq!(back.len(), 1);
        assert!(back[0].own);
        assert_eq!(back[0].label, "me@example.com · Claude Max");
        let v = serde_json::to_value(&back[0]).unwrap();
        assert_eq!(v["own"], true, "the UI sees it");
        // A second own sign-in on another harness is that harness's default too.
        let b = add_own(&store, "codex", &who).unwrap();
        assert!(b.is_default && b.provider == "openai");
    }

    fn new(harness: &str, kind: AccountKind, secret: Option<&str>) -> NewAccount {
        NewAccount {
            harness: harness.into(),
            label: format!("{harness} {}", kind.as_str()),
            kind,
            secret: secret.map(Secret::new),
            endpoint: (kind == Gateway).then(|| "https://gw.example".into()),
            options: Default::default(),
        }
    }

    /// A thread on `account` (foreign keys off: no project needed for these tests).
    pub(crate) fn thread_on(store: &Store, id: &str, account: &str, status: &str) {
        store
            .with(|c| {
                c.execute_batch("PRAGMA foreign_keys = OFF")?;
                c.execute(
                    "INSERT INTO threads (id, project_id, agent_id, title, slug, branch, base_branch,
                     worktree_path, status, created_at, updated_at, account_id)
                     VALUES (?1, 'p', 'claude', 't', ?1, ?1, 'main', '/w', ?2, '0', '0', ?3)",
                    rusqlite::params![id, status, account],
                )?;
                Ok(())
            })
            .unwrap();
    }

    fn account_of(store: &Store, thread: &str) -> Option<String> {
        store
            .with(|c| {
                Ok(c.query_row(
                    "SELECT account_id FROM threads WHERE id = ?1",
                    [thread],
                    |r| r.get(0),
                )?)
            })
            .unwrap()
    }

    #[test]
    fn identities_from_status_and_codex_home() {
        let claude = r#"{"loggedIn":true,"authMethod":"claude.ai","email":"Sam@Example.com","orgId":"org-1","orgName":"Example Org","subscriptionType":"max"}"#;
        let who = identity_from_status("anthropic", claude).unwrap();
        assert_eq!(who.key, "sam@example.com/org-1");
        assert_eq!(who.label, "sam@example.com · Claude Max");
        let text = identity_from_status("anthropic", "Logged in as ana@x.io\n").unwrap();
        assert_eq!(
            (text.key.as_str(), text.label.as_str()),
            ("ana@x.io", "ana@x.io")
        );
        assert_eq!(
            identity_from_status("openai", "Logged in using ChatGPT"),
            None
        );

        let home = tempfile::tempdir().unwrap();
        // header.claims.signature, claims base64url without padding.
        let claims = r#"{"email":"dev@corp.com","https://api.openai.com/auth":{"chatgpt_plan_type":"pro","chatgpt_account_id":"acc-9"}}"#;
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let enc: String = claims
            .as_bytes()
            .chunks(3)
            .flat_map(|ch| {
                let n =
                    ch.iter().fold(0u32, |n, &b| (n << 8) | u32::from(b)) << (8 * (3 - ch.len()));
                (0..=ch.len()).map(move |i| A[((n >> (18 - 6 * i)) & 63) as usize] as char)
            })
            .collect();
        std::fs::write(
            home.path().join("auth.json"),
            format!(r#"{{"tokens":{{"id_token":"e30.{enc}.sig"}}}}"#),
        )
        .unwrap();
        let who = identity_from_codex_home(home.path()).unwrap();
        assert_eq!(who.key, "dev@corp.com/acc-9");
        assert_eq!(who.label, "dev@corp.com · ChatGPT Pro");
    }

    #[test]
    fn the_same_key_twice_is_one_account() {
        let (store, vault) = (Store::open_in_memory().unwrap(), MemoryVault::default());
        let root = Path::new("/gond/accounts");
        let k = add(
            &store,
            &vault,
            root,
            new("claude", ApiKey, Some("sk-ant-1")),
        )
        .unwrap();
        let who = k.identity.clone().unwrap();
        assert!(who.starts_with("anthropic key ") && who.ends_with(" at default"));
        assert!(!who.contains("sk-ant-1"), "a fingerprint, not the key");
        let e = add(
            &store,
            &vault,
            root,
            new("claude", ApiKey, Some(" sk-ant-1 ")),
        )
        .unwrap_err();
        assert!(e.to_string().contains("already saved as"), "{e}");
        // Another key, or the same key to another endpoint, is another account.
        add(
            &store,
            &vault,
            root,
            new("claude", ApiKey, Some("sk-ant-2")),
        )
        .unwrap();
        let mut gw = new("claude", Gateway, Some("sk-ant-1"));
        gw.endpoint = Some("https://gw.example/v1".into());
        assert!(add(&store, &vault, root, gw)
            .unwrap()
            .identity
            .unwrap()
            .ends_with(" at gw.example"));
        assert_eq!(list(&store, Some("claude"), None).unwrap().len(), 3);
    }

    #[test]
    fn signing_in_as_someone_already_here_merges_into_them() {
        let (store, vault) = (Store::open_in_memory().unwrap(), MemoryVault::default());
        let root = tempfile::tempdir().unwrap();
        let mut first = new("claude", Oauth, None);
        first.label = "Claude Code sign-in".into();
        let a = add(&store, &vault, root.path(), first).unwrap();
        let b = add(&store, &vault, root.path(), new("claude", Oauth, None)).unwrap();
        for h in [&a, &b] {
            std::fs::create_dir_all(h.config_dir.as_deref().unwrap()).unwrap();
        }
        thread_on(&store, "t1", &b.id, "idle");
        let who = Identity {
            key: "d@x.com".into(),
            label: "d@x.com · Claude Max".into(),
        };
        // The first sign-in takes the identity, and the real name over Gond's.
        assert_eq!(adopt(&store, root.path(), &a.id, &who).unwrap(), None);
        let a1 = store.with(|c| rows::get(c, &a.id)).unwrap().unwrap();
        assert_eq!(
            (a1.identity.as_deref(), a1.label.as_str()),
            (Some("d@x.com"), "d@x.com · Claude Max")
        );
        // The second, as the same person, folds into the first: one account, the newest
        // home, its threads moved, the old home gone.
        let kept = adopt(&store, root.path(), &b.id, &who).unwrap().unwrap();
        assert_eq!(kept.id, a.id);
        assert_eq!(kept.config_dir, b.config_dir);
        assert_eq!(list(&store, Some("claude"), None).unwrap().len(), 1);
        assert_eq!(account_of(&store, "t1").as_deref(), Some(a.id.as_str()));
        assert!(!Path::new(a.config_dir.as_deref().unwrap()).exists());
        assert!(Path::new(b.config_dir.as_deref().unwrap()).exists());
    }

    #[test]
    fn startup_merges_duplicates_into_the_oldest() {
        let (store, vault) = (Store::open_in_memory().unwrap(), MemoryVault::default());
        let root = tempfile::tempdir().unwrap();
        let a = add(&store, &vault, root.path(), new("claude", Oauth, None)).unwrap();
        let b = add(&store, &vault, root.path(), new("claude", Oauth, None)).unwrap();
        let c = add(&store, &vault, root.path(), new("claude", Oauth, None)).unwrap();
        let other = add(&store, &vault, root.path(), new("codex", Oauth, None)).unwrap();
        store
            .with(|db| {
                for id in [&a.id, &b.id, &c.id, &other.id] {
                    rows::set_identity(db, id, Some("d@x.com"))?;
                }
                rows::make_default(db, &c.id)
            })
            .unwrap();
        thread_on(&store, "t1", &b.id, "idle");
        thread_on(&store, "t2", &c.id, "done");
        assert_eq!(merge_duplicates(&store, &vault, root.path()).unwrap(), 2);
        let left = list(&store, None, None).unwrap();
        assert_eq!(left.len(), 2, "codex's account is another harness: kept");
        let kept = left.iter().find(|x| x.harness == "claude").unwrap();
        assert_eq!(kept.id, a.id);
        assert!(
            kept.is_default,
            "a duplicate's default moves to the one kept"
        );
        assert_eq!(account_of(&store, "t1").as_deref(), Some(a.id.as_str()));
        assert_eq!(account_of(&store, "t2").as_deref(), Some(a.id.as_str()));
        assert_eq!(merge_duplicates(&store, &vault, root.path()).unwrap(), 0);
    }

    #[test]
    fn backfill_finds_key_identities_from_the_vault() {
        let (store, vault) = (Store::open_in_memory().unwrap(), MemoryVault::default());
        let root = Path::new("/gond/accounts");
        let k = add(
            &store,
            &vault,
            root,
            new("claude", ApiKey, Some("sk-ant-1")),
        )
        .unwrap();
        let o = add(&store, &vault, root, new("claude", Oauth, None)).unwrap();
        store.with(|c| rows::set_identity(c, &k.id, None)).unwrap();
        assert_eq!(backfill_identities(&store, &vault).unwrap(), vec![o.id]);
        let k1 = store.with(|c| rows::get(c, &k.id)).unwrap().unwrap();
        assert_eq!(k1.identity, k.identity);
    }

    #[test]
    fn many_accounts_per_harness_each_isolated() {
        let (store, vault) = (Store::open_in_memory().unwrap(), MemoryVault::default());
        let root = Path::new("/gond/accounts");
        let a = add(&store, &vault, root, new("claude", Oauth, None)).unwrap();
        let b = add(&store, &vault, root, new("claude", Oauth, None)).unwrap();
        let k = add(&store, &vault, root, new("claude", ApiKey, Some("sk-one"))).unwrap();
        assert_ne!(a.config_dir, b.config_dir);
        assert!(a.config_dir.as_deref().unwrap().contains(&a.id));
        assert_eq!(k.config_dir, None);
        assert_eq!(
            vault
                .resolve(k.secret_ref.as_ref().unwrap())
                .unwrap()
                .expose(),
            "sk-one"
        );
        assert_eq!(list(&store, Some("claude"), None).unwrap().len(), 3);
        assert_eq!(list(&store, None, Some("anthropic")).unwrap().len(), 3);
        assert!(list(&store, Some("codex"), None).unwrap().is_empty());

        remove(&store, &vault, root, &k.id).unwrap();
        assert!(vault.get(k.secret_ref.as_ref().unwrap()).unwrap().is_none());
        assert_eq!(
            remove(&store, &vault, root, &k.id).unwrap_err().code(),
            "not_found"
        );
    }

    #[test]
    fn remove_only_deletes_homes_under_the_root() {
        let (store, vault) = (Store::open_in_memory().unwrap(), MemoryVault::default());
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("accounts");
        let outside = tmp.path().join("precious");
        std::fs::create_dir_all(&outside).unwrap();
        let a = add(&store, &vault, &root, new("claude", Oauth, None)).unwrap();
        std::fs::create_dir_all(a.config_dir.as_ref().unwrap()).unwrap();
        remove(&store, &vault, &root, &a.id).unwrap();
        assert!(!Path::new(a.config_dir.as_ref().unwrap()).exists());

        // A row pointing outside the root never deletes that directory.
        let b = add(&store, &vault, &root, new("claude", Oauth, None)).unwrap();
        store
            .with(|c| {
                c.execute(
                    "UPDATE accounts SET config_dir = ?1 WHERE id = ?2",
                    rusqlite::params![outside.to_string_lossy(), b.id],
                )?;
                Ok(())
            })
            .unwrap();
        remove(&store, &vault, &root, &b.id).unwrap();
        assert!(outside.exists());
    }

    #[test]
    fn new_account_debug_hides_the_secret() {
        let n: NewAccount = serde_json::from_value(serde_json::json!({
            "harness": "claude", "label": "work", "kind": "apiKey", "secret": "sk-ant-XYZ"
        }))
        .unwrap();
        assert!(!format!("{n:?}").contains("sk-ant-XYZ"));
    }

    #[test]
    fn agy_allows_one_sign_in_then_keys_or_gateways() {
        let (store, vault) = (Store::open_in_memory().unwrap(), MemoryVault::default());
        let root = Path::new("/gond/accounts");
        let first = add(&store, &vault, root, new("agy", Oauth, None)).unwrap();
        assert_eq!(first.config_dir, None, "agy uses its own sign-in");
        let err = add(&store, &vault, root, new("agy", Oauth, None)).unwrap_err();
        assert!(err.to_string().contains("one sign-in per Windows user"));
        add(&store, &vault, root, new("agy", ApiKey, Some("AIza-1"))).unwrap();
        add(&store, &vault, root, new("agy", ApiKey, Some("AIza-2"))).unwrap();
        add(&store, &vault, root, new("agy", Gateway, Some("gw-key"))).unwrap();
        assert_eq!(list(&store, Some("agy"), None).unwrap().len(), 4);
    }

    /// Copilot holds several sign-ins (each its own home), tokens, and keys only with their
    /// endpoint. Cursor holds one sign-in (Windows-wide) and keys.
    #[test]
    fn copilot_and_cursor_accounts() {
        let (store, vault) = (Store::open_in_memory().unwrap(), MemoryVault::default());
        let root = Path::new("/gond/accounts");
        let a = add(&store, &vault, root, new("copilot", Oauth, None)).unwrap();
        let b = add(&store, &vault, root, new("copilot", Oauth, None)).unwrap();
        assert!(a.config_dir.is_some() && a.config_dir != b.config_dir);
        assert_eq!(a.provider, "github");
        add(
            &store,
            &vault,
            root,
            new("copilot", Token, Some("github_pat_1")),
        )
        .unwrap();
        let err = add(&store, &vault, root, new("copilot", ApiKey, Some("sk-1"))).unwrap_err();
        assert!(err.to_string().contains("base URL"), "{err}");
        let mut byok = new("copilot", ApiKey, Some("sk-1"));
        byok.endpoint = Some("https://llm.example.com/v1".into());
        add(&store, &vault, root, byok).unwrap();

        let own = add(&store, &vault, root, new("cursor", Oauth, None)).unwrap();
        assert_eq!(own.config_dir, None, "Cursor uses its own sign-in");
        assert!(add(&store, &vault, root, new("cursor", Oauth, None)).is_err());
        add(&store, &vault, root, new("cursor", ApiKey, Some("key_1"))).unwrap();
        assert!(add(&store, &vault, root, new("cursor", Token, Some("t"))).is_err());
    }

    /// Who Copilot is signed in as, from its config (comment lines first, as it writes it).
    #[test]
    fn copilot_identity_from_its_config() {
        let home = tempfile::tempdir().unwrap();
        std::fs::write(
            home.path().join("config.json"),
            "// User settings belong in settings.json.\n// This file is managed automatically.\n{\n  \"loggedInUsers\": [{ \"host\": \"https://github.com\", \"login\": \"dawsja\" }],\n  \"lastLoggedInUser\": { \"host\": \"https://github.com\", \"login\": \"dawsja\" }\n}\n",
        )
        .unwrap();
        let who = identity_from_copilot_home(home.path()).unwrap();
        assert_eq!((who.key.as_str(), who.label.as_str()), ("dawsja", "dawsja"));
        std::fs::write(
            home.path().join("config.json"),
            r#"{ "lastLoggedInUser": { "host": "https://acme.ghe.com", "login": "dev" } }"#,
        )
        .unwrap();
        assert_eq!(
            identity_from_copilot_home(home.path()).unwrap().label,
            "dev@acme.ghe.com"
        );
        assert!(identity_from_copilot_home(Path::new("/nonexistent")).is_none());
    }

    #[test]
    fn api_keys_take_a_base_url_but_not_auth_headers() {
        let (store, vault) = (Store::open_in_memory().unwrap(), MemoryVault::default());
        let root = Path::new("/r");
        let with = |endpoint: &str, options: AccountOptions| NewAccount {
            endpoint: Some(endpoint.into()),
            options,
            ..new("codex", ApiKey, Some("sk-x"))
        };
        let ok = add(
            &store,
            &vault,
            root,
            with("https://openrouter.ai/api/v1", Default::default()),
        );
        assert_eq!(
            ok.unwrap().endpoint.as_deref(),
            Some("https://openrouter.ai/api/v1")
        );
        add(
            &store,
            &vault,
            root,
            with("http://localhost:4000", Default::default()),
        )
        .unwrap();
        for bad in [
            with("http://gateway.example.com", Default::default()),
            with(
                "https://gw.example",
                AccountOptions {
                    headers: [(
                        "Authorization".to_string(),
                        HeaderValue::Plain("Bearer x".into()),
                    )]
                    .into(),
                    ..Default::default()
                },
            ),
            NewAccount {
                endpoint: Some("https://x".into()),
                ..new("claude", Oauth, None)
            },
        ] {
            assert_eq!(
                add(&store, &vault, root, bad).unwrap_err().code(),
                "invalid"
            );
        }
        // Options round-trip through the store.
        let a = add(
            &store,
            &vault,
            root,
            with(
                "https://gw.example",
                AccountOptions {
                    bearer: true,
                    ..Default::default()
                },
            ),
        )
        .unwrap();
        let back = list(&store, None, None)
            .unwrap()
            .into_iter()
            .find(|x| x.id == a.id)
            .unwrap();
        assert!(back.options.bearer);
    }

    #[test]
    fn secret_headers_live_in_the_vault_and_read_back_as_true() {
        let (store, vault) = (Store::open_in_memory().unwrap(), MemoryVault::default());
        let n: NewAccount = serde_json::from_value(serde_json::json!({
            "harness": "claude", "label": "gw", "kind": "apiKey", "secret": "sk-k",
            "endpoint": "https://gw.example",
            "options": { "headers": { "X-Team": "core", "X-Gateway-Token": { "secret": "gw-SECRET" } } }
        }))
        .unwrap();
        assert!(!format!("{n:?}").contains("gw-SECRET"));
        let a = add(&store, &vault, Path::new("/r"), n).unwrap();
        let json = serde_json::to_string(&list(&store, None, None).unwrap()).unwrap();
        assert!(!json.contains("gw-SECRET"), "{json}");
        // Nor in the database row itself.
        let raw: String = store
            .with(|c| Ok(c.query_row("SELECT options FROM accounts", [], |r| r.get(0))?))
            .unwrap();
        assert!(
            !raw.contains("gw-SECRET") && raw.contains("X-Gateway-Token"),
            "{raw}"
        );
        assert!(
            json.contains("\"X-Gateway-Token\":{\"secret\":true}"),
            "{json}"
        );
        let at = header_ref(&a, "X-Gateway-Token");
        assert_eq!(vault.resolve(&at).unwrap().expose(), "gw-SECRET");
        remove(&store, &vault, Path::new("/r"), &a.id).unwrap();
        assert!(
            vault.get(&at).unwrap().is_none(),
            "removed with the account"
        );
    }

    #[test]
    fn rejects_bad_accounts() {
        let (store, vault) = (Store::open_in_memory().unwrap(), MemoryVault::default());
        let root = Path::new("/r");
        for bad in [
            new("aider", ApiKey, Some("k")),
            new("claude", Oauth, Some("k")),
            new("claude", ApiKey, None),
            new("claude", ApiKey, Some("  ")),
            new("codex", Token, Some("k")),
            new("grok", Gateway, Some("k")),
            NewAccount {
                label: " ".into(),
                ..new("claude", Oauth, None)
            },
        ] {
            assert_eq!(
                add(&store, &vault, root, bad).unwrap_err().code(),
                "invalid"
            );
        }
    }

    #[test]
    fn secrets_never_reach_the_db_or_json() {
        let (store, vault) = (Store::open_in_memory().unwrap(), MemoryVault::default());
        let a = add(
            &store,
            &vault,
            Path::new("/r"),
            new("claude", ApiKey, Some("sk-SECRET-42")),
        )
        .unwrap();
        let json = serde_json::to_string(&list(&store, None, None).unwrap()).unwrap();
        assert!(!json.contains("sk-SECRET-42") && !json.contains("GondCode/"));
        assert!(!format!("{a:?}").contains("sk-SECRET-42"));
        let dump: Vec<String> = store
            .with(|c| {
                let mut s = c.prepare("SELECT * FROM accounts")?;
                let n = s.column_count();
                let rows = s.query_map([], |r| {
                    Ok((0..n)
                        .map(|i| {
                            r.get::<_, Option<String>>(i)
                                .unwrap_or_default()
                                .unwrap_or_default()
                        })
                        .collect::<Vec<_>>()
                        .join("|"))
                })?;
                Ok(rows.collect::<Result<_, _>>()?)
            })
            .unwrap();
        assert!(!dump.concat().contains("sk-SECRET-42"));
    }
}
