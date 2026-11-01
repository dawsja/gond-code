//! GitHub through the GitHub CLI only: no OAuth App of our own.
//! Gond runs `gh auth` for the human and keeps nothing: tokens live in gh's store, go to
//! gh on stdin and never reach argv, logs, the state tree or an agent's env.

use std::ffi::OsString;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use schemars::JsonSchema;
use serde::Serialize;

use crate::agents::detect;
use crate::error::{AppError, AppResult};

pub const DEFAULT_HOST: &str = "github.com";
/// GitHub's device codes last 15 minutes; gh gives up then too.
const CODE_LIFETIME: Duration = Duration::from_secs(15 * 60);
/// How long gh gets to print the one-time code.
const CODE_WAIT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GithubAccount {
    pub login: String,
    pub host: String,
    /// The account gh (and so Git) uses for this host.
    pub active: bool,
    pub scopes: Vec<String>,
}

/// A browser sign-in waiting for the human to enter `user_code` at `verification_uri`.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    pub host: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GithubStatus {
    pub gh_installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gh_version: Option<String>,
    pub accounts: Vec<GithubAccount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending: Option<Pending>,
    /// Why the last browser sign-in didn't finish, in one sentence; cleared by the next one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_error: Option<String>,
}

/// Finds and runs gh. `path_var` pins the PATH searched (tests use a fake gh); `None`
/// reads the app's live PATH (`process::path`, refreshed after every install) at each
/// call, so a fresh gh install is picked up without a restart.
#[derive(Clone, Default)]
pub struct GhCli {
    path_var: Option<OsString>,
}

impl GhCli {
    pub fn with_path(path_var: impl Into<OsString>) -> Self {
        GhCli {
            path_var: Some(path_var.into()),
        }
    }

    fn path(&self) -> Option<OsString> {
        self.path_var.clone().or_else(crate::process::path)
    }

    fn find(&self) -> AppResult<PathBuf> {
        detect::find("gh", self.path())
            .ok_or_else(|| AppError::GhMissing("GitHub CLI (gh) isn't installed yet.".into()))
    }

    /// `gh <args>`: no console window, no prompts, and no token from Gond's own env.
    fn command(&self, args: &[&str]) -> AppResult<Command> {
        let mut cmd = crate::process::command(self.find()?);
        cmd.args(args)
            .env("GH_PROMPT_DISABLED", "1")
            .env("GH_NO_UPDATE_NOTIFIER", "1")
            .env("NO_COLOR", "1")
            .env_remove("GH_TOKEN")
            .env_remove("GITHUB_TOKEN")
            .env_remove("GH_ENTERPRISE_TOKEN")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(p) = self.path() {
            cmd.env("PATH", p);
        }
        Ok(cmd)
    }

    /// Runs `gh <args>`, optionally feeding `stdin`; a failure carries gh's last line.
    fn run(&self, args: &[&str], stdin: Option<&str>) -> AppResult<String> {
        let mut cmd = self.command(args)?;
        if stdin.is_some() {
            cmd.stdin(Stdio::piped());
        }
        let mut child = crate::process::when_free(|| cmd.spawn())?;
        if let (Some(text), Some(mut pipe)) = (stdin, child.stdin.take()) {
            // gh may exit before reading it all; its own error says why.
            let _ = pipe.write_all(text.as_bytes());
        }
        let out = child.wait_with_output()?;
        if out.status.success() {
            return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
        }
        let err = String::from_utf8_lossy(&out.stderr);
        Err(AppError::GithubFailed(last_line(&err, stdin)))
    }

    /// gh's accounts on every host (`gh auth status --json hosts`).
    pub fn status(&self) -> GithubStatus {
        let Ok(bin) = self.find() else {
            return GithubStatus::default();
        };
        let accounts = self
            .run(&["auth", "status", "--json", "hosts"], None)
            .map(|out| parse_accounts(&out))
            .unwrap_or_default();
        GithubStatus {
            gh_installed: true,
            gh_version: detect::version(&bin),
            accounts,
            ..Default::default()
        }
    }

    /// Signs in with a token piped to `gh auth login --with-token`, then sets up Git.
    pub fn login_token(&self, host: &str, token: &str) -> AppResult<()> {
        let token = token.trim();
        if token.is_empty() || token.contains(char::is_whitespace) {
            return Err(AppError::Invalid(
                "That doesn't look like a GitHub token.".into(),
            ));
        }
        let args = [
            "auth",
            "login",
            "--hostname",
            host,
            "--git-protocol",
            "https",
        ];
        self.run(
            &[&args[..], &["--with-token"]].concat(),
            Some(&format!("{token}\n")),
        )?;
        self.setup_git(host)
    }

    /// `gh auth setup-git`: Git's pushes and fetches use gh's credentials.
    pub fn setup_git(&self, host: &str) -> AppResult<()> {
        self.run(&["auth", "setup-git", "--hostname", host], None)
            .map(drop)
    }

    pub fn switch(&self, host: &str, login: &str) -> AppResult<()> {
        self.run(
            &["auth", "switch", "--hostname", host, "--user", login],
            None,
        )
        .map(drop)
    }

    /// The active account's profile (`gh api user`), for prefilling Git's identity.
    /// The email is the public one, else GitHub's noreply address for the account.
    pub fn user(&self, host: &str) -> Option<(String, String)> {
        let out = self.run(&["api", "--hostname", host, "user"], None).ok()?;
        let v: serde_json::Value = serde_json::from_str(&out).ok()?;
        let login = v["login"].as_str().filter(|l| !l.is_empty())?;
        let name = v["name"]
            .as_str()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or(login);
        let email = match v["email"].as_str().filter(|e| e.contains('@')) {
            Some(e) => e.to_string(),
            None => format!("{}+{login}@users.noreply.{host}", v["id"].as_u64()?),
        };
        Some((name.to_string(), email))
    }

    pub fn logout(&self, host: &str, login: &str) -> AppResult<()> {
        self.run(
            &["auth", "logout", "--hostname", host, "--user", login],
            None,
        )
        .map(drop)
    }
}

/// gh's last stderr line, with `secret` scrubbed out in case gh ever echoes it.
fn last_line(stderr: &str, secret: Option<&str>) -> String {
    let line = stderr
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .unwrap_or("gh exited with an error")
        .to_string();
    match secret.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => line.replace(s, "[token]"),
        None => line,
    }
}

/// Accounts from `gh auth status --json hosts`: `{ hosts: { host: [{ login, active, scopes }] } }`.
pub fn parse_accounts(json: &str) -> Vec<GithubAccount> {
    let v: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    let Some(hosts) = v["hosts"].as_object() else {
        return vec![];
    };
    let mut out = vec![];
    for (host, entries) in hosts {
        for e in entries.as_array().into_iter().flatten() {
            let Some(login) = e["login"].as_str().filter(|l| !l.is_empty()) else {
                continue;
            };
            out.push(GithubAccount {
                login: login.into(),
                host: e["host"].as_str().unwrap_or(host).into(),
                active: e["active"].as_bool().unwrap_or(false),
                scopes: e["scopes"]
                    .as_str()
                    .unwrap_or("")
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect(),
            });
        }
    }
    out
}

/// The one-time code and verification URL from a line of `gh auth login --web`.
fn device_code(line: &str) -> Option<String> {
    let re = regex::Regex::new(r"one-time code:\s*([A-Z0-9]{4}-[A-Z0-9]{4})").ok()?;
    re.captures(line).map(|c| c[1].to_string())
}

fn device_url(line: &str) -> Option<String> {
    let re = regex::Regex::new(r"https?://\S+/login/device").ok()?;
    re.find(line).map(|m| m.as_str().to_string())
}

struct Running {
    id: u64,
    child: Child,
}

/// gh plus the one browser sign-in in flight. Cheap to clone; clones share it.
#[derive(Clone, Default)]
pub struct Github {
    pub cli: GhCli,
    login: Arc<Mutex<Option<Running>>>,
    pending: Arc<Mutex<Option<Pending>>>,
    error: Arc<Mutex<Option<String>>>,
    next_id: Arc<Mutex<u64>>,
}

impl Github {
    pub fn new(cli: GhCli) -> Self {
        Github {
            cli,
            ..Default::default()
        }
    }

    /// gh's accounts plus the sign-in in flight, if any.
    pub fn status(&self) -> GithubStatus {
        GithubStatus {
            pending: self.pending.lock().unwrap().clone(),
            login_error: self.error.lock().unwrap().clone(),
            ..self.cli.status()
        }
    }

    /// Starts `gh auth login --web` and returns its one-time code as soon as gh prints it.
    /// The sign-in finishes in the background: on success Git is set up, and either way
    /// `done` gets the new status. A second call replaces the first.
    pub fn login_web(
        &self,
        host: &str,
        done: impl Fn(GithubStatus) + Send + 'static,
    ) -> AppResult<Pending> {
        self.cancel();
        let args = [
            "auth",
            "login",
            "--hostname",
            host,
            "--git-protocol",
            "https",
            "--web",
        ];
        let mut cmd = self.cli.command(&args)?;
        // gh asks for Enter before it opens a browser when it thinks it has a terminal.
        cmd.stdin(Stdio::piped()).stdout(Stdio::null());
        let mut child = crate::process::when_free(|| cmd.spawn())?;
        if let Some(mut pipe) = child.stdin.take() {
            let _ = pipe.write_all(b"\n");
        }
        let stderr = child.stderr.take().expect("piped stderr");
        let (tx, rx) = mpsc::channel::<String>();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });

        let (mut code, mut url, mut last) = (None, None, String::new());
        let start = Instant::now();
        while code.is_none() || url.is_none() {
            let Some(left) = CODE_WAIT.checked_sub(start.elapsed()) else {
                break;
            };
            match rx.recv_timeout(left) {
                Ok(line) => {
                    code = code.or_else(|| device_code(&line));
                    url = url.or_else(|| device_url(&line));
                    if !line.trim().is_empty() {
                        last = line;
                    }
                }
                // gh exited (or went quiet) before printing both; a code alone is enough.
                Err(_) => break,
            }
        }
        let Some(user_code) = code else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(AppError::GithubFailed(if last.is_empty() {
                "GitHub CLI didn't give a sign-in code.".into()
            } else {
                last_line(&last, None)
            }));
        };
        let expires = time::OffsetDateTime::now_utc() + CODE_LIFETIME;
        let pending = Pending {
            host: host.into(),
            user_code,
            verification_uri: url.unwrap_or_else(|| format!("https://{host}/login/device")),
            expires_at: expires
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
        };
        let id = {
            let mut n = self.next_id.lock().unwrap();
            *n += 1;
            *n
        };
        *self.login.lock().unwrap() = Some(Running { id, child });
        *self.pending.lock().unwrap() = Some(pending.clone());
        *self.error.lock().unwrap() = None;

        let me = self.clone();
        let host = host.to_string();
        std::thread::spawn(move || {
            let outcome = me.wait(id, rx);
            let error = match outcome {
                None => return, // replaced or cancelled: the newer sign-in reports
                Some(Ok(())) => me.cli.setup_git(&host).err().map(|e| e.to_string()),
                Some(Err(e)) => Some(e),
            };
            *me.pending.lock().unwrap() = None;
            *me.error.lock().unwrap() = error;
            done(me.status());
        });
        Ok(pending)
    }

    /// Waits for sign-in `id`: `None` if another call took it over, else whether gh succeeded.
    fn wait(&self, id: u64, rx: mpsc::Receiver<String>) -> Option<Result<(), String>> {
        let start = Instant::now();
        let mut last = String::new();
        loop {
            while let Ok(line) = rx.try_recv() {
                if !line.trim().is_empty() {
                    last = line;
                }
            }
            let mut slot = self.login.lock().unwrap();
            let running = slot.as_mut().filter(|r| r.id == id)?;
            if let Ok(Some(status)) = running.child.try_wait() {
                *slot = None;
                return Some(if status.success() {
                    Ok(())
                } else {
                    Err(format!(
                        "GitHub sign-in didn't finish: {}",
                        last_line(&last, None)
                    ))
                });
            }
            if start.elapsed() > CODE_LIFETIME + Duration::from_secs(30) {
                let _ = running.child.kill();
                let _ = running.child.wait();
                *slot = None;
                return Some(Err("The sign-in code expired. Start again.".into()));
            }
            drop(slot);
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    /// Stops a browser sign-in in flight, if any.
    pub fn cancel(&self) {
        if let Some(mut r) = self.login.lock().unwrap().take() {
            let _ = r.child.kill();
            let _ = r.child.wait();
        }
        *self.pending.lock().unwrap() = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gh_auth_status_json() {
        let json = r#"{"hosts":{"github.com":[
            {"state":"success","active":true,"host":"github.com","login":"sam","tokenSource":"keyring","scopes":"gist, read:org, repo, workflow","gitProtocol":"https"},
            {"state":"success","active":false,"host":"github.com","login":"sam-work","scopes":"repo"}]}}"#;
        let a = parse_accounts(json);
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].login, "sam");
        assert!(a[0].active && !a[1].active);
        assert_eq!(a[0].scopes, ["gist", "read:org", "repo", "workflow"]);
        assert!(parse_accounts(r#"{"hosts":{}}"#).is_empty());
        assert!(parse_accounts("not json").is_empty());
    }

    #[test]
    fn reads_the_device_code_line() {
        let line = "! First copy your one-time code: AB12-CD34";
        assert_eq!(device_code(line).as_deref(), Some("AB12-CD34"));
        assert_eq!(
            device_url(
                "Open this URL to continue in your web browser: https://github.com/login/device"
            )
            .as_deref(),
            Some("https://github.com/login/device")
        );
        assert_eq!(device_code("Press Enter to open github.com"), None);
    }

    #[test]
    fn errors_never_carry_the_token() {
        let msg = last_line(
            "error validating token ghp_SECRET123\n",
            Some("ghp_SECRET123\n"),
        );
        assert!(
            !msg.contains("ghp_SECRET123") && msg.contains("[token]"),
            "{msg}"
        );
    }

    /// After `prereq.install { prereqs: ["gh"] }` the install engine refreshes the live
    /// PATH; the default GhCli must see gh on the next call, without a restart.
    #[cfg(unix)]
    #[test]
    fn a_fresh_install_is_found_on_the_live_path() {
        use std::os::unix::fs::PermissionsExt;
        // Only ever prepend to the live PATH: other tests spawn git through it meanwhile.
        let before = crate::process::path();
        let dir = tempfile::tempdir().unwrap();
        let gh = dir.path().join("gh");
        std::fs::write(&gh, "#!/bin/sh\necho 'gh version 9.9.9 (2026-01-01)'\n").unwrap();
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        // What the install engine does after a step: merge the fresh PATH in front.
        crate::process::set_path(crate::process::merge_paths(
            dir.path().as_os_str(),
            before.as_deref(),
        ));
        let status = GhCli::default().status();
        crate::process::set_path(before.unwrap_or_default());
        assert!(status.gh_installed);
        assert_eq!(status.gh_version.as_deref(), Some("9.9.9"));
    }

    #[test]
    fn gh_is_an_installable_prerequisite() {
        let empty = tempfile::tempdir().unwrap();
        let path = Some(empty.path().as_os_str().to_owned());
        assert_eq!(
            crate::harness::missing_prereqs(&["gh"], path.clone()),
            ["gh"]
        );
        let step = crate::harness::prereq_step("gh", path).expect("gh has an install step");
        assert!(step.label.contains("GitHub CLI"), "{}", step.label);
    }
}
