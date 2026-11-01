//! Latest harness versions for `harness.check`: the npm registry's `latest` tag
//! or a GitHub repo's latest release. Fetched with the system `curl` (shipped with
//! Windows 10+), in the background like every other child (no console window).

use std::process::Stdio;

use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;

use crate::agents::detect;

/// Where a harness publishes its versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Latest {
    Npm(&'static str),
    GitHub(&'static str),
}

/// Per catalog harness. Grok Build publishes no GitHub releases and its changelog page
/// isn't machine-readable, so it has none: the UI shows "latest unknown".
pub fn source(harness_id: &str) -> Option<Latest> {
    match harness_id {
        "claude" => Some(Latest::Npm("@anthropic-ai/claude-code")),
        "codex" => Some(Latest::Npm("@openai/codex")),
        "agy" => Some(Latest::GitHub("google-antigravity/antigravity-cli")),
        _ => None,
    }
}

impl Latest {
    pub fn url(self) -> String {
        match self {
            Latest::Npm(pkg) => format!("https://registry.npmjs.org/{pkg}/latest"),
            Latest::GitHub(repo) => format!("https://api.github.com/repos/{repo}/releases/latest"),
        }
    }

    /// The version in the response body.
    pub fn parse(self, body: &str) -> Option<String> {
        let v: Value = serde_json::from_str(body).ok()?;
        let raw = match self {
            Latest::Npm(_) => v["version"].as_str()?,
            Latest::GitHub(_) => v["tag_name"].as_str()?,
        };
        detect::parse_version(&raw.replace(['-', '_'], " ").replacen("rust v", "", 1))
            .or_else(|| detect::parse_version(raw))
    }
}

/// Fetches `url` (10 s cap). `None` when offline or the server says no.
pub fn fetch(url: &str) -> Option<String> {
    let mut cmd = crate::process::command("curl");
    cmd.args([
        "--silent",
        "--fail",
        "--location",
        "--max-time",
        "10",
        "--header",
        "User-Agent: gond-code",
        url,
    ])
    .stdin(Stdio::null())
    .stderr(Stdio::null());
    let out = crate::process::when_free(|| cmd.output())
        .ok()
        .filter(|o| o.status.success())?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Whether `latest` is newer than `installed`, comparing dotted numbers ("2.1.312").
pub fn newer(latest: &str, installed: &str) -> bool {
    let nums = |v: &str| -> Vec<u64> {
        v.split(['.', '-', '+'])
            .map_while(|p| p.parse().ok())
            .collect()
    };
    nums(latest) > nums(installed)
}

/// What `harness.check` returns per harness.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed: Option<String>,
    /// `None` when the harness doesn't publish one, or the lookup failed (offline).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest: Option<String>,
    pub update_available: bool,
    pub changelog: String,
}

pub fn check(
    id: &str,
    installed: Option<String>,
    changelog: &str,
    get: fn(&str) -> Option<String>,
) -> Check {
    let latest = source(id).and_then(|s| get(&s.url()).and_then(|b| s.parse(&b)));
    let update_available = matches!((&latest, &installed), (Some(l), Some(i)) if newer(l, i));
    Check {
        id: id.into(),
        installed,
        latest,
        update_available,
        changelog: changelog.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_npm_and_github_bodies() {
        let npm = Latest::Npm("@openai/codex");
        assert_eq!(npm.url(), "https://registry.npmjs.org/@openai/codex/latest");
        assert_eq!(
            npm.parse(r#"{"name":"@openai/codex","version":"0.166.0"}"#)
                .as_deref(),
            Some("0.166.0")
        );
        let gh = Latest::GitHub("google-antigravity/antigravity-cli");
        assert_eq!(
            gh.parse(r#"{"tag_name":"v1.4.10"}"#).as_deref(),
            Some("1.4.10")
        );
        assert_eq!(
            gh.parse(r#"{"tag_name":"rust-v0.166.0"}"#).as_deref(),
            Some("0.166.0")
        );
        assert_eq!(gh.parse("not json"), None);
        assert_eq!(source("grok"), None);
    }

    #[test]
    fn compares_versions() {
        assert!(newer("2.1.312", "2.1.300"));
        assert!(newer("1.10.0", "1.9.9"));
        assert!(!newer("1.4.10", "1.4.10"));
        assert!(!newer("0.165.0", "0.166.0"));
    }

    #[test]
    fn check_uses_the_fetcher_and_degrades_offline() {
        let c = check("claude", Some("2.1.300".into()), "cl", |_| {
            Some(r#"{"version":"2.1.312"}"#.into())
        });
        assert_eq!(
            (c.latest.as_deref(), c.update_available),
            (Some("2.1.312"), true)
        );
        let offline = check("claude", Some("2.1.300".into()), "cl", |_| None);
        assert_eq!((offline.latest, offline.update_available), (None, false));
        let missing = check("codex", None, "cx", |_| {
            Some(r#"{"version":"0.166.0"}"#.into())
        });
        assert!(
            !missing.update_available,
            "not installed: install, don't update"
        );
    }
}
