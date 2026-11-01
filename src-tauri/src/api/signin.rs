//! Background sign-in (Lumen): `account.signIn` runs the harness's own login out of
//! sight and mirrors what the person needs at `/signins/{jobId}`: the sign-in URL and any
//! device code it prints, a code prompt (Claude's "Paste code"), then signed in or a plain
//! reason. The login runs in a pseudo-terminal, since some only print to a TTY; its
//! `ptyId` is kept as the fallback the card can reveal when a login needs typing.

use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::state::{Cause, StateTree};
use super::Services;
use crate::accounts;
use crate::error::{AppError, AppResult};

/// How long a login may take to show its link (or code) before Gond calls it stuck and
/// shows what it printed. Short in tests.
fn link_timeout() -> Duration {
    if cfg!(test) && std::env::var_os("GOND_REAL_HARNESS").is_none() {
        Duration::from_secs(2)
    } else {
        Duration::from_secs(20)
    }
}

/// What the details pane shows: the login's own output, the last of it.
const OUTPUT_TAIL: usize = 4000;

fn tail_of(text: &str) -> String {
    let start = text.len().saturating_sub(OUTPUT_TAIL);
    let start = (start..=text.len())
        .find(|i| text.is_char_boundary(*i))
        .unwrap_or(text.len());
    text[start..].to_string()
}

/// How long a login may wait for the person before Gond gives up on it.
pub const LOGIN_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// What the login's output has shown so far.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Seen {
    pub url: Option<String>,
    pub code: Option<String>,
    pub needs_code: bool,
}

/// Reads a login's plain-text output for the first sign-in URL, a device code (shown
/// near the word "code", like `ABCD-1234`) and a prompt to paste a code back.
pub fn scan(text: &str) -> Seen {
    let url = regex::Regex::new(r#"https://[^\s"'<>]+"#)
        .ok()
        .and_then(|re| {
            re.find(text)
                .map(|m| m.as_str().trim_end_matches(['.', ',', ')']).to_string())
        });
    // A device code is upper-case letters and digits (`ABCD-1234`), named as a code, and
    // never part of a URL or path (real Windows runs matched an OAuth client id and a
    // folder name before this).
    let plain = regex::Regex::new(r#"(https?://|[A-Za-z]:\\)[^\s"'<>]+"#)
        .map(|re| re.replace_all(text, " ").into_owned())
        .unwrap_or_else(|_| text.to_string());
    let code = regex::Regex::new(r"(?i:code)[^\n]{0,40}?\b([A-Z0-9]{4,}-[A-Z0-9]{4,})\b")
        .ok()
        .and_then(|re| re.captures(&plain).map(|c| c[1].to_string()));
    let lower = text.to_lowercase();
    Seen {
        url,
        code,
        needs_code: lower.contains("paste code") || lower.contains("paste the code"),
    }
}

fn last_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .unwrap_or("")
        .to_string()
}

fn commit(tree: &StateTree, job_id: &str, patch: Value) {
    let id = job_id.to_string();
    let cause = Cause::Runtime {
        runtime: "signin".into(),
    };
    tree.commit(cause, move |d| {
        if let (Some(entry), Some(p)) = (d["signins"][&id].as_object_mut(), patch.as_object()) {
            for (k, v) in p {
                entry.insert(k.clone(), v.clone());
            }
        }
    });
}

/// The pieces of a login the watcher needs once it has started.
pub struct Login {
    pub job_id: String,
    pub account_id: String,
    pub pty_id: String,
    pub name: String,
    /// The status check (program and args) with the account's env, if the harness has one.
    pub status: Option<std::process::Command>,
}

/// Follows the login until it ends, the person cancels, or `timeout` passes.
pub fn watch(s: Services, mut login: Login, timeout: Duration) {
    let (tree, ptys) = (s.state.clone(), s.ptys.clone());
    let begin = Instant::now();
    let (mut text, mut since, mut seen) = (String::new(), None, Seen::default());
    let (exited, code) = loop {
        let Ok(r) = ptys.read(&login.pty_id, since, None, Some(Duration::from_millis(400))) else {
            break (true, None);
        };
        if !r.text.is_empty() {
            text.push_str(&r.text);
            since = Some(r.offset);
            let now = scan(&text);
            // The raw output is always there, so the details pane is never blank.
            let mut patch = json!({ "output": tail_of(&text) });
            if now != seen {
                patch["status"] = (if now.needs_code {
                    "needsCode"
                } else {
                    "waiting"
                })
                .into();
                patch["url"] = now.url.clone().into();
                patch["code"] = now.code.clone().into();
                seen = now;
            }
            commit(&tree, &login.job_id, patch);
        }
        let shown = seen.url.is_some() || seen.code.is_some() || seen.needs_code;
        if !shown && !r.exited && begin.elapsed() > link_timeout() {
            let _ = ptys.close(&login.pty_id);
            let said = last_line(&text);
            commit(
                &tree,
                &login.job_id,
                json!({
                    "status": "failed", "output": tail_of(&text),
                    "message": if said.is_empty() {
                        format!("{} didn't start its sign-in: it printed nothing. Try again.", login.name)
                    } else {
                        format!("{}'s sign-in didn't show a link. It said: {said}", login.name)
                    },
                }),
            );
            return;
        }
        if r.exited {
            break (true, r.exit_code);
        }
        let cancelled = tree
            .get(&format!("/signins/{}/status", login.job_id))
            .is_some_and(|(_, v)| v == "cancelled");
        if cancelled {
            let _ = ptys.close(&login.pty_id);
            return;
        }
        if begin.elapsed() > timeout {
            let _ = ptys.close(&login.pty_id);
            break (false, None);
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let outcome = if !exited {
        json!({ "status": "failed", "message": format!("{} sign-in timed out. Try again.", login.name) })
    } else if code != Some(0) {
        let tail = last_line(&text);
        json!({ "status": "failed", "message": if tail.is_empty() {
            format!("{} sign-in didn't finish.", login.name)
        } else {
            format!("{} sign-in didn't finish: {tail}", login.name)
        }})
    } else {
        let status = login.status.as_mut().map(|cmd| cmd.output());
        let ok = status
            .as_ref()
            .is_none_or(|o| o.as_ref().is_ok_and(|o| o.status.success()));
        if ok {
            let said = status
                .and_then(Result::ok)
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned());
            signed_in(&s, &login, said.as_deref())
        } else {
            json!({ "status": "failed", "message": format!("{}'s login ended, but it isn't signed in. Try again.", login.name) })
        }
    };
    commit(&tree, &login.job_id, outcome);
    let _ = ptys.close(&login.pty_id);
}

/// The outcome of a finished login: who it signed in as, and, when an account already
/// was that person, the merge into it, so signing in twice never makes two rows.
fn signed_in(s: &Services, login: &Login, status: Option<&str>) -> Value {
    let done = |message: String, id: &str| json!({ "status": "signedIn", "message": message, "accountId": id });
    let Ok(Some(a)) = s
        .store
        .with(|c| crate::store::accounts::get(c, &login.account_id))
    else {
        return done(format!("Signed in to {}.", login.name), &login.account_id);
    };
    let who = status
        .and_then(|t| accounts::identity_from_status(&a.provider, t))
        .or_else(|| {
            a.config_dir
                .as_deref()
                .filter(|_| a.harness == "codex")
                .and_then(|h| accounts::identity_from_codex_home(std::path::Path::new(h)))
        });
    let Some(who) = who else {
        return done(format!("Signed in to {}.", login.name), &a.id);
    };
    let cause = Cause::Runtime {
        runtime: "signin".into(),
    };
    let out = match accounts::adopt(&s.store, &s.accounts_root, &a.id, &who) {
        Ok(Some(kept)) => done(
            format!(
                "Already signed in as {}, so I updated \"{}\".",
                who.label, kept.label
            ),
            &kept.id,
        ),
        Ok(None) => done(
            format!("Signed in to {} as {}.", login.name, who.label),
            &a.id,
        ),
        Err(_) => done(format!("Signed in to {}.", login.name), &a.id),
    };
    let _ = super::accounts::sync_accounts(s, cause);
    out
}

fn pty_of(s: &Services, job_id: &str) -> AppResult<String> {
    let (_, v) = s
        .state
        .get(&format!("/signins/{job_id}"))
        .ok_or_else(|| AppError::NotFound(format!("sign-in {job_id} not found")))?;
    let live = matches!(
        v["status"].as_str(),
        Some("starting" | "waiting" | "needsCode")
    );
    if !live {
        return Err(AppError::Invalid(
            "that sign-in has already finished".into(),
        ));
    }
    v["ptyId"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| AppError::NotFound("sign-in has no terminal".into()))
}

/// `account.signIn.code`: the code the person copied from the browser, typed into the login.
pub fn send_code(s: &Services, job_id: &str, code: &str) -> AppResult<()> {
    let pty = pty_of(s, job_id)?;
    let code = code.trim();
    if code.is_empty() || code.contains(['\r', '\n']) {
        return Err(AppError::Invalid("paste the code on one line".into()));
    }
    s.ptys.write(&pty, &format!("{code}\r"))?;
    commit(&s.state, job_id, json!({ "status": "waiting" }));
    Ok(())
}

/// `account.signIn.cancel`: stops the login; the watcher sees it and closes the terminal.
pub fn cancel(s: &Services, job_id: &str) -> AppResult<()> {
    pty_of(s, job_id)?;
    commit(
        &s.state,
        job_id,
        json!({ "status": "cancelled", "message": "Sign-in cancelled." }),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_urls_codes_and_prompts() {
        let claude = "Opening browser to sign in…\nIf the browser didn't open, visit: https://claude.ai/oauth/authorize?code=true&client_id=x.\n\nPaste code here if prompted >";
        let s = scan(claude);
        assert_eq!(
            s.url.as_deref(),
            Some("https://claude.ai/oauth/authorize?code=true&client_id=x")
        );
        assert!(s.needs_code && s.code.is_none());
        let device = "To sign in, open https://auth.x.ai/device and enter the code: WXYZ-4821\n";
        let s = scan(device);
        assert_eq!(
            (s.url.as_deref(), s.code.as_deref()),
            (Some("https://auth.x.ai/device"), Some("WXYZ-4821"))
        );
        assert!(!s.needs_code);
        assert_eq!(
            scan("Starting local login server on http://localhost:1455."),
            Seen::default()
        );
        // Real Windows runs: a client id inside the URL, a folder name, are not codes.
        let claude = "Browser didn't open? Use the url below to sign in (c to copy)\nhttps://claude.com/cai/oauth/authorize?code=true&client_id=9d1c250a-e61b-44d9-88ed\nPaste code here if prompted >";
        assert_eq!(scan(claude).code, None);
        let codex = r#"WARNING: (codex_home: AbsolutePathBuf("C:\Users\r\AppData\Local\Temp\gond-code-test-accounts-01M\codex"))"#;
        assert_eq!(scan(codex), Seen::default());
    }
}
