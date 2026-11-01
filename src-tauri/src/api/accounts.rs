//! `account.*` behind the registry. Secrets go straight into the vault: the
//! input's `/secret` and `/options/headers` (a header may carry a `{ secret }`) are
//! redacted before the event log, and the tree's `accounts`
//! mirror never carries a secret or where it lives.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::registry::{Call, Def, Registry, Risk};
use super::state::Cause;
use super::Services;
use crate::accounts::{self, Account, AccountKind, NewAccount};
use crate::error::{AppError, AppResult};
use crate::vault::SecretResolver;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HarnessInput {
    pub harness: String,
}

/// What a harness accepts, for the account manager.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Kinds {
    pub provider: String,
    pub kinds: Vec<AccountKind>,
    /// How many sign-ins it can hold side by side; `1` for agy (one per Windows user).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_oauth: Option<usize>,
}

fn cause(call: &Call<Services>, command: &str) -> Cause {
    Cause::Command {
        event_id: None,
        command: command.into(),
        actor: call.actor.clone(),
    }
}

/// Rewrites `/accounts` from the store, without secrets.
pub fn sync_accounts(services: &Services, cause: Cause) -> AppResult<()> {
    let list = accounts::list(&services.store, None, None)?;
    let map: BTreeMap<String, Account> = list.into_iter().map(|a| (a.id.clone(), a)).collect();
    let value = serde_json::to_value(map).unwrap_or_default();
    services.state.commit(cause, |d| d["accounts"] = value);
    Ok(())
}

/// A custom base URL sends prompts and code to a new host, so it's outward: it asks.
fn add_risk(input: &serde_json::Value) -> Risk {
    match input["endpoint"].as_str() {
        Some(e) if !e.trim().is_empty() => Risk::Outward,
        _ => Risk::WriteLocal,
    }
}

/// `account.test`: a saved account, or the Add dialog's draft fields.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum TestInput {
    Saved {
        #[serde(rename = "accountId")]
        account_id: String,
    },
    Draft(Box<accounts::NewAccount>),
}

/// Testing a saved account reads (its endpoint was approved when it was added); a draft
/// sends a key to a host nobody approved yet, so it's outward.
fn test_risk(input: &serde_json::Value) -> Risk {
    if input.get("accountId").is_some() {
        Risk::Read
    } else {
        Risk::Outward
    }
}

/// Lists the endpoint's models with the account's key (resolved here, never stored).
pub fn test_account(
    services: &Services,
    cause: Cause,
    input: TestInput,
    fetch: crate::endpoint_test::Fetch,
) -> AppResult<crate::endpoint_test::TestResult> {
    use crate::accounts::HeaderValue;
    use crate::endpoint_test::{run, Target, TestResult};
    let fail = |e: &str| {
        Ok(TestResult {
            ok: false,
            models: None,
            ms: None,
            error: Some(e.into()),
        })
    };
    // (harness, kind, endpoint, key, options, resolved headers)
    let saved = match &input {
        TestInput::Saved { account_id } => Some(account_id.clone()),
        TestInput::Draft(_) => None,
    };
    let (harness, kind, endpoint, key, options, headers) = match input {
        TestInput::Saved { account_id } => {
            let a = services
                .store
                .with(|c| crate::store::accounts::get(c, &account_id))?
                .ok_or_else(|| AppError::NotFound(format!("account {account_id} not found")))?;
            let key = match &a.secret_ref {
                Some(at) => Some(services.vault.resolve(at)?.expose().to_string()),
                None => None,
            };
            let mut headers = vec![];
            for (name, v) in &a.options.headers {
                let value = match v {
                    HeaderValue::Plain(p) => p.clone(),
                    HeaderValue::Secret { .. } => services
                        .vault
                        .resolve(&accounts::header_ref(&a, name))?
                        .expose()
                        .to_string(),
                };
                headers.push((name.clone(), value));
            }
            (a.harness, a.kind, a.endpoint, key, a.options, headers)
        }
        TestInput::Draft(n) => {
            let n = *n;
            let headers = n
                .options
                .headers
                .iter()
                .filter_map(|(name, v)| match v {
                    HeaderValue::Plain(p) => Some((name.clone(), p.clone())),
                    HeaderValue::Secret { secret } => {
                        secret_value(secret).map(|s| (name.clone(), s))
                    }
                })
                .collect();
            let key = n.secret.as_ref().map(|s| s.expose().to_string());
            (n.harness, n.kind, n.endpoint, key, n.options, headers)
        }
    };
    if kind == AccountKind::Oauth || kind == AccountKind::Token {
        return fail("Sign-in accounts are tested by signing in.");
    }
    let Some(rules) = accounts::rules(&harness) else {
        return fail(&format!("{harness} doesn't support accounts yet."));
    };
    let target = Target {
        provider: rules.provider,
        endpoint: endpoint.as_deref().filter(|e| !e.trim().is_empty()),
        key: key.as_deref(),
        bearer: options.bearer,
        organization: options.organization.as_deref(),
        project: options.project.as_deref(),
        azure_api_version: options.azure_api_version.as_deref(),
        cloud: options.cloud.as_ref(),
        headers,
    };
    let result = run(&target, fetch);
    // A saved account's models feed its model picker (`model.list { accountId }`).
    if let (Some(id), Some(models)) = (saved, &result.models) {
        let models = serde_json::json!(models);
        services
            .state
            .commit(cause, move |d| d["endpointModels"][&id] = models);
    }
    Ok(result)
}

/// A draft header's secret value, if it was given.
fn secret_value(slot: &accounts::SecretSlot) -> Option<String> {
    match slot {
        accounts::SecretSlot::Given(s) => Some(s.expose().to_string()),
        accounts::SecretSlot::Stored(_) => None,
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignInInput {
    pub account_id: String,
    /// Default 100.
    pub cols: Option<u16>,
    /// Default 30.
    pub rows: Option<u16>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignedIn {
    /// Follow it at `/signins/{jobId}`: `{ status, url?, code?, message?, ptyId }`. None for
    /// an agent with one sign-in per Windows user (agy): its own CLI is in the terminal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    /// The login's hidden terminal, for a card that has to fall back to showing it.
    pub pty_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignInCodeInput {
    pub job_id: String,
    /// The code the browser showed after signing in.
    pub code: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignInJobInput {
    pub job_id: String,
}

/// `account.signIn`: the harness's own login, in the background, with the
/// account's home in its env so the sign-in lands there and nowhere else. Its URL, code
/// and outcome appear at `/signins/{jobId}` (`api::signin`). `path_var` overrides PATH in
/// tests.
pub fn sign_in(
    services: &Services,
    cause: Cause,
    input: SignInInput,
    path_var: Option<std::ffi::OsString>,
) -> AppResult<SignedIn> {
    let a = services
        .store
        .with(|c| crate::store::accounts::get(c, &input.account_id))?
        .ok_or_else(|| AppError::NotFound(format!("account {} not found", input.account_id)))?;
    if a.kind != AccountKind::Oauth {
        return Err(AppError::Invalid(format!(
            "\"{}\" uses a key, not a sign-in; use Test connection instead",
            a.label
        )));
    }
    let driver = crate::runner::driver_for(&services.store, &a.harness)?;
    let path_var = path_var.or_else(crate::process::path);
    let command = command_of(&a.harness);
    let program = crate::agents::detect::find(command, path_var.clone()).ok_or_else(|| {
        AppError::AgentNotFound(format!("{command} isn't installed or isn't on PATH"))
    })?;
    // One sign-in per Windows user (agy keeps its token in Credential Manager, Cursor in
    // %APPDATA%), or your own sign-in again: the agent's own login, as you, in the terminal
    // here; you sign in the way it asks.
    let (Some(args), Some(home)) = (driver.login_args(), a.config_dir.as_deref()) else {
        let mut cmd = command_for(&program);
        if let Some(args) = driver.login_args() {
            cmd.args(args);
        }
        cmd.cwd(dirs::home_dir().unwrap_or_else(std::env::temp_dir));
        cmd.env("TERM", "xterm-256color");
        if let Some(p) = &path_var {
            cmd.env("PATH", p);
        }
        let (cols, rows) = (input.cols.unwrap_or(100), input.rows.unwrap_or(30));
        let owner = format!("account:{}", a.id);
        let pty_id = services.ptys.open_background(&owner, cmd, cols, rows)?;
        let entry = serde_json::json!({
            "id": pty_id, "owner": owner, "cols": cols, "rows": rows,
            "status": "open", "openedBy": "human", "background": true,
        });
        let tid = pty_id.clone();
        services
            .state
            .commit(cause, move |d| d["terminals"][&tid] = entry);
        return Ok(SignedIn {
            job_id: None,
            pty_id,
        });
    };
    let env = driver.account_env(&a)?;
    std::fs::create_dir_all(home)?;
    let mut cmd = command_for(&program);
    cmd.args(args);
    cmd.cwd(home);
    cmd.env("TERM", "xterm-256color");
    if let Some(p) = &path_var {
        cmd.env("PATH", p);
    }
    for var in &env.remove {
        cmd.env_remove(var);
    }
    for (k, v) in &env.env {
        cmd.env(k, v);
    }
    let (cols, rows) = (input.cols.unwrap_or(100), input.rows.unwrap_or(30));
    let owner = format!("account:{}", a.id);
    let pty_id = services.ptys.open_background(&owner, cmd, cols, rows)?;
    let job_id = ulid::Ulid::generate().to_string();
    let name = crate::harness::get(&a.harness).map_or(a.harness.clone(), |h| h.name.to_string());
    let entry = serde_json::json!({
        "id": pty_id, "owner": owner, "cols": cols, "rows": rows,
        "status": "open", "openedBy": "human", "background": true,
    });
    let signin = serde_json::json!({
        "id": job_id, "accountId": a.id, "harness": a.harness,
        "status": "starting", "ptyId": pty_id,
    });
    let (tid, jid) = (pty_id.clone(), job_id.clone());
    services.state.commit(cause, move |d| {
        d["terminals"][&tid] = entry;
        d["signins"][&jid] = signin;
    });
    // The check that the home really is signed in once the login ends.
    let status = status_command(&*driver, &program, home, &env, path_var.as_ref());
    let login = super::signin::Login {
        job_id: job_id.clone(),
        account_id: a.id.clone(),
        pty_id: pty_id.clone(),
        name,
        status,
    };
    let svc = services.clone();
    std::thread::spawn(move || super::signin::watch(svc, login, super::signin::LOGIN_TIMEOUT));
    Ok(SignedIn {
        job_id: Some(job_id),
        pty_id,
    })
}

/// The command a harness runs as (`cursor` is `cursor-agent`).
fn command_of(harness: &str) -> &str {
    crate::harness::get(harness).map_or(harness, |h| h.command)
}

/// The agent's CLI for a terminal. npm installs harnesses as `.cmd` shims, which
/// CreateProcess can't start on its own.
fn command_for(program: &std::path::Path) -> portable_pty::CommandBuilder {
    let shim = program
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"));
    if shim {
        let mut c = portable_pty::CommandBuilder::new("cmd.exe");
        c.args(["/d", "/c"]);
        c.arg(program);
        c
    } else {
        portable_pty::CommandBuilder::new(program)
    }
}

/// The harness's status check for an account's home (`claude auth status`), with the
/// account's env, its output captured for the identity.
fn status_command(
    driver: &dyn crate::agents::driver::Driver,
    program: &std::path::Path,
    home: &str,
    env: &crate::agents::driver::AccountEnv,
    path_var: Option<&std::ffi::OsString>,
) -> Option<std::process::Command> {
    let args = driver.login_status_args()?;
    let mut c = crate::process::command(program);
    c.args(args)
        .current_dir(home)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    if let Some(p) = path_var {
        c.env("PATH", p);
    }
    for var in &env.remove {
        c.env_remove(var);
    }
    for (k, v) in &env.env {
        c.env(k, v);
    }
    Some(c)
}

/// Startup: the sign-ins the CLIs already have. For each installed harness that takes accounts, who its own login
/// is; one that's in no account yet is added as the harness's own sign-in, and an own
/// sign-in whose CLI now logs in as someone else follows it. Returns how many were added.
pub fn discover_own(s: &Services) -> AppResult<usize> {
    let path_var = crate::process::path();
    let dismissed = dismissed_own(&s.store);
    let mut added = 0;
    for h in crate::harness::CATALOG {
        let Some(rules) = accounts::rules(h.id) else {
            continue;
        };
        let Some(program) = crate::agents::detect::find(h.command, path_var.clone()) else {
            continue;
        };
        let Some(who) = own_identity(s, h.id, rules.provider, &program, path_var.as_ref()) else {
            continue;
        };
        // One you removed stays removed; add it again by hand.
        if dismissed.contains(&who.key) {
            continue;
        }
        let all = accounts::list(&s.store, Some(h.id), None)?;
        if all
            .iter()
            .any(|a| a.identity.as_deref() == Some(who.key.as_str()))
        {
            continue;
        }
        if let Some(own) = all.iter().find(|a| a.is_own()) {
            s.store.with(|c| {
                crate::store::accounts::set_identity(c, &own.id, Some(&who.key))?;
                crate::store::accounts::set_label(c, &own.id, &who.label)
            })?;
            continue;
        }
        accounts::add_own(&s.store, h.id, &who)?;
        added += 1;
    }
    Ok(added)
}

/// The own sign-ins you removed, by identity: found again, they aren't added back.
const DISMISSED: &str = "dismissedOwnSignIns";

fn dismissed_own(store: &crate::store::Store) -> Vec<String> {
    store
        .with(crate::store::settings::all)
        .ok()
        .and_then(|all| all.into_iter().find(|(k, _)| k == DISMISSED))
        .and_then(|(_, v)| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

/// Who a harness's own login is, as the CLI runs for the user: Codex from its home's
/// `auth.json`, the rest from their login status command. None when it isn't signed in,
/// or says so without naming anyone.
fn own_identity(
    s: &Services,
    harness: &str,
    provider: &str,
    program: &std::path::Path,
    path_var: Option<&std::ffi::OsString>,
) -> Option<accounts::Identity> {
    if harness == "codex" {
        let home = std::env::var_os("CODEX_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| dirs::home_dir().map(|h| h.join(".codex")))?;
        return accounts::identity_from_codex_home(&home);
    }
    // Copilot has no status command: its config names who's signed in.
    if harness == "copilot" {
        let home = std::env::var_os("COPILOT_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| dirs::home_dir().map(|h| h.join(".copilot")))?;
        return accounts::identity_from_copilot_home(&home);
    }
    let driver = crate::runner::driver_for(&s.store, harness).ok()?;
    let home = dirs::home_dir()?;
    let mut cmd = status_command(
        &*driver,
        program,
        &home.to_string_lossy(),
        &Default::default(),
        path_var,
    )?;
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    accounts::identity_from_status(provider, &String::from_utf8_lossy(&out.stdout))
}

/// Startup: works out who the accounts made before identities are, then merges
/// the duplicates into the oldest of each, threads and all. Runs off the main thread,
/// since a sign-in's identity comes from running its harness's status check.
pub fn tidy(s: &Services) -> AppResult<usize> {
    let unknown = accounts::backfill_identities(&s.store, &*s.vault)?;
    let mut merged = 0;
    let path_var = crate::process::path();
    for id in unknown {
        let Some(a) = s.store.with(|c| crate::store::accounts::get(c, &id))? else {
            continue;
        };
        let (AccountKind::Oauth, Some(home)) = (a.kind, a.config_dir.as_deref()) else {
            continue;
        };
        let Ok(driver) = crate::runner::driver_for(&s.store, &a.harness) else {
            continue;
        };
        let (Ok(env), Some(program)) = (
            driver.account_env(&a),
            crate::agents::detect::find(command_of(&a.harness), path_var.clone()),
        ) else {
            continue;
        };
        let Some(mut cmd) = status_command(&*driver, &program, home, &env, path_var.as_ref())
        else {
            continue;
        };
        let Ok(out) = cmd.output() else { continue };
        let text = String::from_utf8_lossy(&out.stdout);
        if !out.status.success() {
            continue;
        }
        if let Some(who) = accounts::identity_from_status(&a.provider, &text) {
            if accounts::adopt(&s.store, &s.accounts_root, &a.id, &who)?.is_some() {
                merged += 1;
            }
        }
    }
    discover_own(s)?;
    merged += accounts::merge_duplicates(&s.store, &*s.vault, &s.accounts_root)?;
    sync_accounts(
        s,
        Cause::Runtime {
            runtime: "startup".into(),
        },
    )?;
    Ok(merged)
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoverInput {}

pub fn register(r: &mut Registry<Services>) {
    r.add(
        Def::new(
            "account.kinds",
            "Which account kinds a harness takes, and how many sign-ins",
            Risk::Read,
        ),
        |_, i: HarnessInput| -> AppResult<Kinds> {
            let r = accounts::rules(&i.harness).ok_or_else(|| {
                AppError::Invalid(format!("{} doesn't support accounts yet", i.harness))
            })?;
            Ok(Kinds {
                provider: r.provider.into(),
                kinds: r.kinds.to_vec(),
                max_oauth: r.max_oauth,
            })
        },
    );
    r.add(
        Def::new(
            "account.add",
            "Add an account (a sign-in, an API key, a token or a gateway)",
            Risk::WriteLocal,
        )
        .human_only()
        .redact(&["/secret", "/options/headers"])
        .dynamic(
            "outward when it sets a custom endpoint (base URL)",
            add_risk,
        ),
        |call, i: NewAccount| -> AppResult<Account> {
            let s = call.ctx;
            let a = accounts::add(&s.store, &*s.vault, &s.accounts_root, i)?;
            sync_accounts(s, cause(call, "account.add"))?;
            Ok(a)
        },
    );
    r.add(
        Def::new(
            "account.test",
            "Test connection: list an API account's models from its endpoint",
            Risk::Read,
        )
        .human_only()
        .redact(&["/secret", "/options/headers"])
        .dynamic(
            "outward for a draft (its key goes to a host not yet approved)",
            test_risk,
        ),
        |call, i: TestInput| {
            test_account(
                call.ctx,
                cause(call, "account.test"),
                i,
                crate::endpoint_test::curl,
            )
        },
    );
    r.add(
        Def::new(
            "account.signIn",
            "Sign a sign-in account in: the harness's own login, in the background",
            Risk::WriteLocal,
        )
        .human_only(),
        |call, i: SignInInput| sign_in(call.ctx, cause(call, "account.signIn"), i, None),
    );
    r.add(
        Def::new(
            "account.signIn.code",
            "Give a background sign-in the code the browser showed",
            Risk::WriteLocal,
        )
        .human_only()
        .redact(&["/code"]),
        |call, i: SignInCodeInput| super::signin::send_code(call.ctx, &i.job_id, &i.code),
    );
    r.add(
        Def::new(
            "account.signIn.cancel",
            "Stop a background sign-in",
            Risk::WriteLocal,
        )
        .human_only(),
        |call, i: SignInJobInput| super::signin::cancel(call.ctx, &i.job_id),
    );
    r.add(
        Def::new(
            "account.discover",
            "Look for the agents' own sign-ins again (a login made since Gond Code started)",
            Risk::WriteLocal,
        )
        .human_only(),
        |call, _: DiscoverInput| -> AppResult<usize> {
            let added = discover_own(call.ctx)?;
            if added > 0 {
                sync_accounts(call.ctx, cause(call, "account.discover"))?;
            }
            Ok(added)
        },
    );
}

#[cfg(test)]
mod tests {
    use crate::api::{registry, Actor, ActorKind, Services};
    use serde_json::json;

    /// On a real PC (ignored): the CLIs' own logins show up as accounts, once each.
    #[test]
    #[ignore]
    fn real_own_sign_ins_are_found() {
        let s = Services::default();
        let first = super::discover_own(&s).unwrap();
        let all = crate::accounts::list(&s.store, None, None).unwrap();
        for a in &all {
            eprintln!(
                "{} {} own={} default={} {:?}",
                a.harness, a.label, a.own, a.is_default, a.identity
            );
        }
        assert_eq!(first, all.len());
        assert!(all.iter().all(|a| a.own));
        assert_eq!(super::discover_own(&s).unwrap(), 0, "found once");
    }

    #[test]
    fn add_and_tree_without_secrets() {
        let (r, s, h) = (registry(), Services::default(), Actor::human());
        let a = r
            .run(
                &s,
                &h,
                "account.add",
                json!({ "harness": "claude", "label": "Work key", "kind": "apiKey", "secret": "sk-ant-TREE" }),
            )
            .unwrap();
        let id = a["id"].as_str().unwrap().to_string();
        assert_eq!(a["kind"], "apiKey");
        let (_, tree) = s.state.get("/accounts").unwrap();
        assert_eq!(tree[&id]["label"], "Work key");
        let everything = format!("{a} {tree}");
        assert!(!everything.contains("sk-ant-TREE") && !everything.contains("GondCode/"));

        let k = r
            .run(&s, &h, "account.kinds", json!({ "harness": "agy" }))
            .unwrap();
        assert_eq!(k["maxOauth"], 1);
        assert_eq!(k["kinds"], json!(["oauth", "apiKey", "gateway"]));
    }

    #[test]
    fn agents_cannot_add_accounts() {
        let (r, s) = (registry(), Services::default());
        let agent = Actor {
            kind: ActorKind::Agent,
            id: "agent:T1".into(),
            label: "a".into(),
            thread_id: Some("T1".into()),
            session_id: None,
        };
        let e = r
            .run(
                &s,
                &agent,
                "account.add",
                json!({ "harness": "claude", "label": "x", "kind": "apiKey", "secret": "k" }),
            )
            .unwrap_err();
        assert_eq!(e.code(), "forbidden");
    }

    #[test]
    fn a_custom_endpoint_is_outward() {
        assert_eq!(
            super::add_risk(&json!({ "endpoint": "https://openrouter.ai/api/v1" })),
            crate::api::Risk::Outward
        );
        assert_eq!(super::add_risk(&json!({})), crate::api::Risk::WriteLocal);
    }

    #[test]
    fn test_connection_saved_and_draft() {
        use super::{test_account, test_risk, TestInput};
        let tc = || crate::api::state::Cause::Command {
            event_id: None,
            command: "account.test".into(),
            actor: Actor::human(),
        };
        let (r, s, h) = (registry(), Services::default(), Actor::human());
        let a = r
            .run(
                &s,
                &h,
                "account.add",
                json!({ "harness": "codex", "label": "router", "kind": "apiKey", "secret": "sk-k",
                        "endpoint": "https://openrouter.ai/api/v1",
                        "options": { "headers": { "X-Title": "Gond", "X-Gw-Token": { "secret": "gw-1" } } } }),
            )
            .unwrap();
        let id = a["id"].as_str().unwrap().to_string();
        // The fetch sees the resolved key and secret header; nothing else does.
        fn fetch(
            req: &crate::endpoint_test::Request,
            _: std::time::Duration,
        ) -> Result<(u16, String), String> {
            assert_eq!(req.url, "https://openrouter.ai/api/v1/models");
            assert!(req
                .headers
                .contains(&("Authorization".into(), "Bearer sk-k".into())));
            assert!(req.headers.contains(&("X-Gw-Token".into(), "gw-1".into())));
            Ok((
                200,
                r#"{"data":[{"id":"anthropic/claude-opus-5.5"}]}"#.into(),
            ))
        }
        let saved = test_account(
            &s,
            tc(),
            TestInput::Saved {
                account_id: id.clone(),
            },
            fetch,
        )
        .unwrap();
        assert!(saved.ok);
        // The picker for this account now offers what the endpoint serves.
        let m = r
            .run(
                &s,
                &h,
                "model.list",
                json!({ "harness": "codex", "accountId": id }),
            )
            .unwrap();
        assert_eq!(m[0]["id"], "anthropic/claude-opus-5.5");
        assert_eq!(m[0]["thinking"]["levels"], json!([]));
        assert_eq!(saved.models.unwrap(), ["anthropic/claude-opus-5.5"]);
        let draft: TestInput = serde_json::from_value(json!({
            "harness": "codex", "label": "x", "kind": "apiKey", "secret": "sk-k",
            "endpoint": "https://openrouter.ai/api/v1",
            "options": { "headers": { "X-Gw-Token": { "secret": "gw-1" } } }
        }))
        .unwrap();
        assert!(test_account(&s, tc(), draft, fetch).unwrap().ok);
        assert_eq!(
            test_risk(&json!({ "accountId": id })),
            crate::api::Risk::Read
        );
        assert_eq!(
            test_risk(&json!({ "harness": "codex" })),
            crate::api::Risk::Outward
        );
        let oauth: TestInput = serde_json::from_value(json!({
            "harness": "claude", "label": "x", "kind": "oauth"
        }))
        .unwrap();
        let r = test_account(&s, tc(), oauth, fetch).unwrap();
        assert_eq!(
            r.error.as_deref(),
            Some("Sign-in accounts are tested by signing in.")
        );
    }

    #[cfg(unix)]
    #[test]
    fn sign_in_runs_the_harness_login_in_the_accounts_home() {
        use super::{sign_in, SignInInput};
        use std::os::unix::fs::PermissionsExt;
        let (r, s, h) = (registry(), Services::default(), Actor::human());
        let bin = tempfile::tempdir().unwrap();
        let fake = bin.path().join("claude");
        // `auth login` prints Claude's URL, asks for the code and records what it got;
        // `auth status` succeeds once that happened, in the same home.
        let script = r#"#!/bin/sh
if [ "$1 $2" = "auth status" ]; then [ -f "$CLAUDE_CONFIG_DIR/.signed" ]; exit $?; fi
echo "login:$*:$CLAUDE_CONFIG_DIR"
echo "If the browser didn't open, visit: https://claude.ai/oauth/authorize?code=true&x=1"
printf 'Paste code here if prompted > '
read code
echo "$code" > "$CLAUDE_CONFIG_DIR/.signed"
echo done
"#;
        std::fs::write(&fake, script).unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let a = r
            .run(
                &s,
                &h,
                "account.add",
                json!({ "harness": "claude", "label": "Max", "kind": "oauth" }),
            )
            .unwrap();
        let id = a["id"].as_str().unwrap().to_string();
        let home = s
            .store
            .with(|c| crate::store::accounts::get(c, &id))
            .unwrap()
            .unwrap()
            .config_dir
            .unwrap();
        let cause = crate::api::state::Cause::Command {
            event_id: None,
            command: "account.signIn".into(),
            actor: h.clone(),
        };
        let input = SignInInput {
            account_id: a["id"].as_str().unwrap().into(),
            cols: Some(250),
            rows: None,
        };
        let out = sign_in(&s, cause, input, Some(bin.path().as_os_str().to_owned())).unwrap();
        let job = format!("/signins/{}", out.job_id.as_deref().unwrap());
        let wait = |want: &str| {
            let begin = std::time::Instant::now();
            loop {
                let v = s.state.get(&job).unwrap().1;
                if v["status"] == want {
                    return v;
                }
                assert!(begin.elapsed() < std::time::Duration::from_secs(10), "{v}");
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        };
        // In the background, with the URL and the code prompt in the tree.
        let v = wait("needsCode");
        assert_eq!(v["url"], "https://claude.ai/oauth/authorize?code=true&x=1");
        assert_eq!(v["ptyId"], out.pty_id.as_str());
        assert_eq!(
            s.state
                .get(&format!("/terminals/{}/background", out.pty_id))
                .unwrap()
                .1,
            true
        );
        r.run(
            &s,
            &h,
            "account.signIn.code",
            json!({ "jobId": out.job_id, "code": "abc123" }),
        )
        .unwrap();
        let v = wait("signedIn");
        assert_eq!(v["message"], "Signed in to Claude Code.");
        // The login ran with the account's own home, and got the code.
        assert_eq!(
            std::fs::read_to_string(format!("{home}/.signed"))
                .unwrap()
                .trim(),
            "abc123"
        );
        let e = r
            .run(
                &s,
                &h,
                "account.signIn.cancel",
                json!({ "jobId": out.job_id }),
            )
            .unwrap_err();
        assert_eq!(e.code(), "invalid", "a finished sign-in can't be cancelled");

        // A key isn't signed in. agy's one sign-in isn't per account: its own CLI opens, as
        // you, in a terminal (no background job), and you sign in there.
        let key = r
            .run(
                &s,
                &h,
                "account.add",
                json!({ "harness": "claude", "label": "k", "kind": "apiKey", "secret": "sk" }),
            )
            .unwrap();
        let e = r
            .run(&s, &h, "account.signIn", json!({ "accountId": key["id"] }))
            .unwrap_err();
        assert_eq!(e.code(), "invalid");
        let agy = r
            .run(
                &s,
                &h,
                "account.add",
                json!({ "harness": "agy", "label": "g", "kind": "oauth" }),
            )
            .unwrap();
        let fake_agy = bin.path().join("agy");
        std::fs::write(
            &fake_agy,
            "#!/bin/sh
echo \"agy as you in $PWD\"
sleep 5
",
        )
        .unwrap();
        std::fs::set_permissions(&fake_agy, std::fs::Permissions::from_mode(0o755)).unwrap();
        let cause = crate::api::state::Cause::Command {
            event_id: None,
            command: "account.signIn".into(),
            actor: h.clone(),
        };
        let input = SignInInput {
            account_id: agy["id"].as_str().unwrap().into(),
            cols: None,
            rows: None,
        };
        let out = sign_in(&s, cause, input, Some(bin.path().as_os_str().to_owned())).unwrap();
        assert_eq!(out.job_id, None, "no background login: the terminal is it");
        assert!(s.state.get(&format!("/terminals/{}", out.pty_id)).is_some());
        s.ptys.close(&out.pty_id).ok();
    }

    /// Two sign-ins as the same person give one account.
    #[cfg(unix)]
    #[test]
    fn signing_in_twice_as_the_same_person_keeps_one_account() {
        use super::{sign_in, SignInInput};
        use std::os::unix::fs::PermissionsExt;
        let (r, s, h) = (registry(), Services::default(), Actor::human());
        let bin = tempfile::tempdir().unwrap();
        let fake = bin.path().join("claude");
        let script = r#"#!/bin/sh
if [ "$1 $2" = "auth status" ]; then
  echo '{"loggedIn":true,"email":"sam@example.com","subscriptionType":"max"}'; exit 0
fi
echo "Login successful."
"#;
        std::fs::write(&fake, script).unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let sign_in_new = || {
            let a = r
                .run(
                    &s,
                    &h,
                    "account.add",
                    json!({ "harness": "claude", "label": "Claude Code sign-in", "kind": "oauth" }),
                )
                .unwrap();
            let cause = crate::api::state::Cause::Runtime {
                runtime: "test".into(),
            };
            let input = SignInInput {
                account_id: a["id"].as_str().unwrap().into(),
                cols: None,
                rows: None,
            };
            let out = sign_in(&s, cause, input, Some(bin.path().as_os_str().to_owned())).unwrap();
            let begin = std::time::Instant::now();
            loop {
                let v = s
                    .state
                    .get(&format!("/signins/{}", out.job_id.as_deref().unwrap()))
                    .unwrap()
                    .1;
                if v["status"] == "signedIn" {
                    return (a["id"].as_str().unwrap().to_string(), v);
                }
                assert!(begin.elapsed() < std::time::Duration::from_secs(10), "{v}");
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        };
        let (first, v) = sign_in_new();
        assert_eq!(
            v["message"],
            "Signed in to Claude Code as sam@example.com · Claude Max."
        );
        let (second, v) = sign_in_new();
        assert_ne!(first, second);
        assert_eq!(
            v["message"],
            "Already signed in as sam@example.com · Claude Max, so I updated \"sam@example.com · Claude Max\"."
        );
        assert_eq!(v["accountId"], first.as_str());
        let all = crate::accounts::list(&s.store, Some("claude"), None).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].label, "sam@example.com · Claude Max");
        assert!(s.state.get(&format!("/accounts/{second}")).is_none());
    }

    /// Header values may be secrets, and header names vary, so the log drops them all.
    #[test]
    fn the_log_never_sees_keys_or_headers() {
        let r = registry();
        for c in ["account.add", "account.test"] {
            assert_eq!(
                r.get(c).unwrap().def.redact,
                ["/secret", "/options/headers"],
                "{c}"
            );
        }
    }

    /// dev.1591: a login that never shows a link fails within seconds with what it
    /// printed, instead of "Starting the sign-in…" forever; its output is always shown.
    #[cfg(unix)]
    #[test]
    fn a_sign_in_that_shows_no_link_fails_plainly() {
        use super::{sign_in, SignInInput};
        use std::os::unix::fs::PermissionsExt;
        let (r, s, h) = (registry(), Services::default(), Actor::human());
        for (script, says) in [
            ("/bin/sleep 30", "printed nothing"),
            (
                "echo 'Choose the text style that looks best'; /bin/sleep 30",
                "It said: Choose the text style that looks best",
            ),
        ] {
            let bin = tempfile::tempdir().unwrap();
            let fake = bin.path().join("claude");
            std::fs::write(&fake, format!("#!/bin/sh\n{script}\n")).unwrap();
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
            let a = r
                .run(
                    &s,
                    &h,
                    "account.add",
                    json!({ "harness": "claude", "label": "M", "kind": "oauth" }),
                )
                .unwrap();
            let cause = crate::api::state::Cause::Command {
                event_id: None,
                command: "account.signIn".into(),
                actor: h.clone(),
            };
            let input = SignInInput {
                account_id: a["id"].as_str().unwrap().into(),
                cols: None,
                rows: None,
            };
            let out = sign_in(&s, cause, input, Some(bin.path().as_os_str().to_owned())).unwrap();
            let begin = std::time::Instant::now();
            let v = loop {
                let v = s
                    .state
                    .get(&format!("/signins/{}", out.job_id.as_deref().unwrap()))
                    .unwrap()
                    .1;
                if v["status"] == "failed" {
                    break v;
                }
                assert!(begin.elapsed() < std::time::Duration::from_secs(8), "{v}");
                std::thread::sleep(std::time::Duration::from_millis(50));
            };
            assert!(v["message"].as_str().unwrap().contains(says), "{v}");
            if says.starts_with("It said") {
                assert!(
                    v["output"]
                        .as_str()
                        .unwrap()
                        .contains("Choose the text style"),
                    "{v}"
                );
            }
        }
    }

    /// dev.1591 on real Windows: the installed harness's own login, in Gond's hidden
    /// terminal, reaches its sign-in link (it can't finish OAuth in CI). Run by the
    /// Sign-in check workflow with GOND_REAL_HARNESS=claude|codex.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn real_signin_reaches_the_link() {
        use super::{sign_in, SignInInput};
        let id = std::env::var("GOND_REAL_HARNESS").expect("GOND_REAL_HARNESS");
        let (r, s, h) = (registry(), Services::default(), Actor::human());
        let a = r
            .run(
                &s,
                &h,
                "account.add",
                json!({ "harness": id, "label": "CI", "kind": "oauth" }),
            )
            .unwrap();
        let cause = crate::api::state::Cause::Command {
            event_id: None,
            command: "account.signIn".into(),
            actor: h.clone(),
        };
        let input = SignInInput {
            account_id: a["id"].as_str().unwrap().into(),
            cols: None,
            rows: None,
        };
        let out = sign_in(&s, cause, input, None).unwrap();
        let begin = std::time::Instant::now();
        let v = loop {
            let v = s
                .state
                .get(&format!("/signins/{}", out.job_id.as_deref().unwrap()))
                .unwrap()
                .1;
            if v["url"].is_string() || v["status"] == "failed" || v["status"] == "signedIn" {
                break v;
            }
            assert!(begin.elapsed() < std::time::Duration::from_secs(60), "{v}");
            std::thread::sleep(std::time::Duration::from_millis(200));
        };
        println!("SIGNIN {}", serde_json::to_string_pretty(&v).unwrap());
        let _ = crate::api::signin::cancel(&s, out.job_id.as_deref().unwrap());
        assert!(
            v["url"].is_string() || v["code"].is_string(),
            "no link: {v}"
        );
        println!("reached the link in {:?}", begin.elapsed());
    }
}
