//! `github.*` behind the registry. Sign-in, switch and logout are the
//! human's alone; the token is redacted from the input and never reaches the tree.
//! A missing gh is `gh_missing`: the UI offers `prereq.install` for "gh".

use schemars::JsonSchema;
use serde::Deserialize;

use super::registry::{Call, Def, Registry, Risk};
use super::state::{Cause, StateTree};
use super::Services;
use crate::error::{AppError, AppResult};
use crate::github::{GithubAccount, GithubStatus, Pending, DEFAULT_HOST};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostInput {
    /// Defaults to github.com.
    #[serde(default)]
    pub host: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TokenInput {
    #[serde(default)]
    pub host: Option<String>,
    /// A personal access token (scopes repo, workflow, read:org). Piped to gh on stdin.
    pub token: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountInput {
    #[serde(default)]
    pub host: Option<String>,
    pub login: String,
}

fn host(h: &Option<String>) -> AppResult<String> {
    let h = h.as_deref().map(str::trim).unwrap_or(DEFAULT_HOST);
    let ok = !h.is_empty()
        && h.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    ok.then(|| h.to_string())
        .ok_or_else(|| AppError::Invalid(format!("{h} isn't a GitHub host name")))
}

fn publish(state: &StateTree, cause: Cause, status: &GithubStatus) {
    let value = serde_json::to_value(status).unwrap_or_default();
    state.commit(cause, |d| d["github"] = value);
}

/// Refreshes `/github` and returns what it now says.
fn refresh(call: &Call<Services>, command: &str) -> GithubStatus {
    let status = call.ctx.github.status();
    let cause = Cause::Command {
        event_id: None,
        command: command.into(),
        actor: call.actor.clone(),
    };
    publish(&call.ctx.state, cause, &status);
    status
}

pub fn register(r: &mut Registry<Services>) {
    r.add(
        Def::new(
            "github.status",
            "GitHub CLI: installed?, signed-in accounts and any sign-in in progress",
            Risk::Read,
        ),
        |call, _: HostInput| -> AppResult<GithubStatus> { Ok(refresh(call, "github.status")) },
    );
    r.add(
        Def::new(
            "github.login.web",
            "Sign in to GitHub with the browser: returns the one-time code to enter",
            Risk::Outward,
        )
        .human_only(),
        |call, i: HostInput| -> AppResult<Pending> {
            let host = host(&i.host)?;
            let state = call.ctx.state.clone();
            let pending = call.ctx.github.login_web(&host, move |status| {
                let cause = Cause::Runtime {
                    runtime: "github.login.web".into(),
                };
                publish(&state, cause, &status);
            })?;
            refresh(call, "github.login.web");
            Ok(pending)
        },
    );
    r.add(
        Def::new(
            "github.login.token",
            "Sign in to GitHub with a personal access token",
            Risk::Outward,
        )
        .human_only()
        .redact(&["/token"]),
        |call, i: TokenInput| -> AppResult<GithubAccount> {
            let host = host(&i.host)?;
            call.ctx.github.cli.login_token(&host, &i.token)?;
            refresh(call, "github.login.token")
                .accounts
                .into_iter()
                .find(|a| a.host == host && a.active)
                .ok_or_else(|| AppError::GithubFailed("GitHub CLI didn't keep the sign-in.".into()))
        },
    );
    r.add(
        Def::new(
            "github.switch",
            "Make another signed-in GitHub account the active one",
            Risk::WriteLocal,
        )
        .human_only(),
        |call, i: AccountInput| -> AppResult<GithubStatus> {
            call.ctx.github.cli.switch(&host(&i.host)?, &i.login)?;
            Ok(refresh(call, "github.switch"))
        },
    );
    r.add(
        Def::new(
            "github.logout",
            "Sign a GitHub account out of the GitHub CLI",
            Risk::WriteLocal,
        )
        .human_only(),
        |call, i: AccountInput| -> AppResult<GithubStatus> {
            call.ctx.github.cli.logout(&host(&i.host)?, &i.login)?;
            Ok(refresh(call, "github.logout"))
        },
    );
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::time::{Duration, Instant};

    use serde_json::{json, Value};

    use crate::api::{registry, Actor, ActorKind, Services};
    use crate::github::{GhCli, Github};

    /// A fake gh: logs argv to `gh.log` and stdin to `stdin.log`, and keeps its accounts
    /// as `<login> <active>` lines in `accounts`, printed as `gh auth status --json hosts`.
    const FAKE: &str = r#"#!/bin/sh
d=$(dirname "$0")
echo "$@" >> "$d/gh.log"
env | grep -qE '^(GH|GITHUB|GH_ENTERPRISE)_TOKEN=' && echo "token in env" >> "$d/gh.log"
user() { while [ $# -gt 0 ]; do [ "$1" = --user ] && echo "$2"; shift; done; }
add() { sed -i 's/ true$/ false/' "$d/accounts"; echo "$1 true" >> "$d/accounts"; }
case "$1 $2" in
  "--version ") echo "gh version 2.103.3 (2026-10-26)" ;;
  "auth status")
    printf '{"hosts":{"github.com":['; sep=
    while read -r l a; do printf '%s{"login":"%s","active":%s,"scopes":"repo, workflow"}' "$sep" "$l" "$a"; sep=,; done < "$d/accounts"
    printf ']}}\n' ;;
  "auth login")
    case "$*" in
      *--with-token*) read -r t; echo "$t" >> "$d/stdin.log"
        [ "$t" = ghp_good ] || { echo "error validating token: HTTP 401" >&2; exit 1; }
        add token-user ;;
      *--web*) echo "! First copy your one-time code: AB12-CD34" >&2
        echo "Open this URL to continue in your web browser: https://github.com/login/device" >&2
        [ -f "$d/web-fails" ] && { echo "authentication timed out" >&2; exit 1; }
        sleep 1; add web-user ;;
    esac ;;
  "auth switch") u=$(user "$@"); sed -i "s/ true$/ false/; s/^$u false$/$u true/" "$d/accounts" ;;
  "auth logout") u=$(user "$@"); sed -i "/^$u /d" "$d/accounts" ;;
esac
"#;

    fn services_with_fake(dir: &Path) -> Services {
        let gh = dir.join("gh");
        std::fs::write(&gh, FAKE).unwrap();
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(dir.join("accounts"), "").unwrap();
        with_path(format!("{}:/usr/bin:/bin", dir.display()))
    }

    fn with_path(path: impl Into<std::ffi::OsString>) -> Services {
        Services {
            github: Github::new(GhCli::with_path(path)),
            ..Services::default()
        }
    }

    fn log(dir: &Path, name: &str) -> String {
        std::fs::read_to_string(dir.join(name)).unwrap_or_default()
    }

    fn tree(s: &Services) -> Value {
        s.state.get("/github").unwrap().1
    }

    fn wait_for(s: &Services, f: impl Fn(&Value) -> bool) -> Value {
        let start = Instant::now();
        loop {
            let t = tree(s);
            if f(&t) || start.elapsed() > Duration::from_secs(10) {
                return t;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn missing_gh_reports_not_installed() {
        let dir = tempfile::tempdir().unwrap();
        let s = with_path(dir.path().as_os_str());
        let (r, h) = (registry(), Actor::human());
        let st = r.run(&s, &h, "github.status", json!({})).unwrap();
        assert_eq!(st, json!({ "ghInstalled": false, "accounts": [] }));
        let e = r.run(&s, &h, "github.login.web", json!({})).unwrap_err();
        assert_eq!(e.code(), "gh_missing");
        let e = r
            .run(&s, &h, "github.login.token", json!({ "token": "t" }))
            .unwrap_err();
        assert_eq!(e.code(), "gh_missing");
    }

    #[test]
    fn token_goes_over_stdin_then_git_is_set_up() {
        let dir = tempfile::tempdir().unwrap();
        let s = services_with_fake(dir.path());
        let (r, h) = (registry(), Actor::human());

        let e = r
            .run(&s, &h, "github.login.token", json!({ "token": "ghp_bad" }))
            .unwrap_err();
        assert_eq!(e.code(), "github_failed");
        assert!(e.to_string().contains("HTTP 401") && !e.to_string().contains("ghp_bad"));

        let a = r
            .run(&s, &h, "github.login.token", json!({ "token": "ghp_good" }))
            .unwrap();
        assert_eq!(
            a,
            json!({ "login": "token-user", "host": "github.com", "active": true, "scopes": ["repo", "workflow"] })
        );
        let argv = log(dir.path(), "gh.log");
        assert!(argv.contains("auth login --hostname github.com --git-protocol https --with-token"));
        assert!(argv.contains("auth setup-git --hostname github.com"));
        assert!(!argv.contains("ghp_"), "token in argv: {argv}");
        assert!(!argv.contains("token in env"), "{argv}");
        assert_eq!(log(dir.path(), "stdin.log"), "ghp_bad\nghp_good\n");

        let t = tree(&s);
        assert_eq!(t["ghInstalled"], true);
        assert_eq!(t["ghVersion"], "2.103.3");
        assert_eq!(t["accounts"][0]["login"], "token-user");
        assert!(!t.to_string().contains("ghp_"));
    }

    #[test]
    fn web_login_shows_the_code_and_finishes_in_the_background() {
        let dir = tempfile::tempdir().unwrap();
        let s = services_with_fake(dir.path());
        let (r, h) = (registry(), Actor::human());

        let p = r.run(&s, &h, "github.login.web", json!({})).unwrap();
        assert_eq!(p["userCode"], "AB12-CD34");
        assert_eq!(p["verificationUri"], "https://github.com/login/device");
        assert!(p["expiresAt"].as_str().unwrap().ends_with('Z'));
        assert_eq!(tree(&s)["pending"]["userCode"], "AB12-CD34");

        let t = wait_for(&s, |t| t.get("pending").is_none());
        assert!(t.get("pending").is_none(), "{t}");
        assert_eq!(t["accounts"][0]["login"], "web-user");
        assert!(log(dir.path(), "gh.log").contains("auth setup-git --hostname github.com"));

        std::fs::write(dir.path().join("web-fails"), "").unwrap();
        r.run(&s, &h, "github.login.web", json!({})).unwrap();
        let t = wait_for(&s, |t| t.get("loginError").is_some());
        assert_eq!(
            t["loginError"],
            "GitHub sign-in didn't finish: authentication timed out"
        );
    }

    #[test]
    fn switch_and_logout() {
        let dir = tempfile::tempdir().unwrap();
        let s = services_with_fake(dir.path());
        std::fs::write(dir.path().join("accounts"), "sam false\nsam-work true\n").unwrap();
        let (r, h) = (registry(), Actor::human());

        let st = r
            .run(&s, &h, "github.switch", json!({ "login": "sam" }))
            .unwrap();
        let active: Vec<_> = st["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["active"] == true)
            .map(|a| a["login"].clone())
            .collect();
        assert_eq!(active, [json!("sam")]);
        assert!(log(dir.path(), "gh.log").contains("auth switch --hostname github.com --user sam"));

        let st = r
            .run(&s, &h, "github.logout", json!({ "login": "sam-work" }))
            .unwrap();
        assert_eq!(st["accounts"].as_array().unwrap().len(), 1);
        assert_eq!(tree(&s)["accounts"][0]["login"], "sam");

        let e = r
            .run(
                &s,
                &h,
                "github.switch",
                json!({ "host": "evil host", "login": "x" }),
            )
            .unwrap_err();
        assert_eq!(e.code(), "invalid");
    }

    #[test]
    fn agents_can_read_status_but_never_sign_in() {
        let dir = tempfile::tempdir().unwrap();
        let s = services_with_fake(dir.path());
        let r = registry();
        let agent = Actor {
            kind: ActorKind::Agent,
            id: "agent:T1".into(),
            label: "a".into(),
            thread_id: Some("T1".into()),
            session_id: None,
        };
        assert!(r.run(&s, &agent, "github.status", json!({})).is_ok());
        for (cmd, input) in [
            ("github.login.web", json!({})),
            ("github.login.token", json!({ "token": "ghp_good" })),
            ("github.switch", json!({ "login": "x" })),
            ("github.logout", json!({ "login": "x" })),
        ] {
            assert_eq!(
                r.run(&s, &agent, cmd, input).unwrap_err().code(),
                "forbidden",
                "{cmd}"
            );
        }
        assert!(log(dir.path(), "stdin.log").is_empty());
        assert_eq!(r.get("github.login.token").unwrap().def.redact, ["/token"]);
    }
}
