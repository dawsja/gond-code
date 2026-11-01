//! Plan limits on demand:
//! what each agent's own `/usage` shows, read without running a turn, for Accounts and Usage.
//! - Claude Code: `GET api.anthropic.com/api/oauth/usage` with the sign-in's access token from
//!   `<config>/.credentials.json`, as Claude Code's `/usage` does: each limit's percent used
//!   and reset (the 5-hour session, the week, and the week per model).
//! - Codex: its app server's `account/rateLimits/read`, asked once (`codex app-server`).
//! - Grok: `grok_usage` (its billing).
//! - Antigravity: Google's Cloud Code API, as agy's `/usage` does, with agy's own sign-in from Credential Manager (`gemini:antigravity`):
//!   `loadCodeAssist` for the project, then `retrieveUserQuotaSummary` (each model group's
//!   5-hour and weekly limits) and `retrieveUserQuota` (the Gemini models' share).
//!
//! Tokens are never refreshed here: an expired one means no reading until the agent renews
//! it.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};

use crate::agents::UsageWindow;
use crate::endpoint_test::{Fetch, Post, Request};

const CLAUDE_USAGE: &str = "https://api.anthropic.com/api/oauth/usage";
const TIMEOUT: Duration = Duration::from_secs(8);
/// How long Codex's app server gets to answer.
const CODEX_WAIT: Duration = Duration::from_secs(20);

/// Claude's sign-in access token in `<config>/.credentials.json`, unless it has expired.
pub fn claude_token(config: &Path) -> Option<String> {
    let v: Value =
        serde_json::from_str(&std::fs::read_to_string(config.join(".credentials.json")).ok()?)
            .ok()?;
    let o = &v["claudeAiOauth"];
    let token = o["accessToken"].as_str().filter(|t| !t.trim().is_empty())?;
    let now_ms = time::OffsetDateTime::now_utc().unix_timestamp() * 1000;
    let live = o["expiresAt"].as_i64().is_none_or(|at| at > now_ms);
    live.then(|| token.to_string())
}

/// The plan's windows, from Claude's usage endpoint.
pub fn claude(token: &str, fetch: Fetch) -> Vec<UsageWindow> {
    let req = Request {
        url: CLAUDE_USAGE.into(),
        headers: vec![
            ("Authorization".into(), format!("Bearer {token}")),
            ("anthropic-beta".into(), "oauth-2025-04-20".into()),
            ("Accept".into(), "application/json".into()),
        ],
    };
    match fetch(&req, TIMEOUT) {
        Ok((200, body)) => claude_windows(&serde_json::from_str(&body).unwrap_or_default()),
        _ => vec![],
    }
}

fn window(
    name: &str,
    label: Option<String>,
    used_percent: f64,
    resets: Option<&str>,
) -> UsageWindow {
    let used = used_percent.clamp(0.0, 100.0);
    UsageWindow {
        name: name.into(),
        label,
        percent_left: ((100.0 - used) * 10.0).round() / 10.0,
        resets_at: resets.map(String::from),
        status: (used >= 100.0).then(|| "rejected".to_string()),
    }
}

/// `limits` (each with a kind, group, percent and reset; a scoped one names its model) when
/// the answer has them, else the older `five_hour` / `seven_day[_opus|_sonnet]` fields.
pub fn claude_windows(v: &Value) -> Vec<UsageWindow> {
    let limits = v["limits"].as_array().cloned().unwrap_or_default();
    if !limits.is_empty() {
        return limits
            .iter()
            .filter(|l| {
                l["is_active"] != false || l["kind"].as_str().is_some_and(|k| k.ends_with("_all"))
            })
            .filter_map(|l| {
                let name = match l["group"].as_str()? {
                    "weekly" => "weekly",
                    "monthly" => "monthly",
                    _ => "session",
                };
                let model = l
                    .pointer("/scope/model/display_name")
                    .and_then(Value::as_str);
                let label = model.map(|m| {
                    format!(
                        "{} · {m}",
                        if name == "weekly" {
                            "Weekly"
                        } else {
                            "Session"
                        }
                    )
                });
                Some(window(
                    name,
                    label,
                    l["percent"].as_f64()?,
                    l["resets_at"].as_str(),
                ))
            })
            .collect();
    }
    [
        ("five_hour", "session", None),
        ("seven_day", "weekly", None),
        ("seven_day_opus", "weekly", Some("Weekly · Opus")),
        ("seven_day_sonnet", "weekly", Some("Weekly · Sonnet")),
    ]
    .into_iter()
    .filter_map(|(field, name, label)| {
        let w = &v[field];
        Some(window(
            name,
            label.map(String::from),
            w["utilization"].as_f64()?,
            w["resets_at"].as_str(),
        ))
    })
    .collect()
}

const CLOUD_CODE: &str = "https://cloudcode-pa.googleapis.com/v1internal";

/// agy's sign-in access token (Credential Manager, `gemini:antigravity`), unless it expired.
pub fn agy_token() -> Option<String> {
    use crate::vault::{SecretRef, SystemVault, Vault};
    let raw = SystemVault::default()
        .get(&SecretRef("gemini:antigravity".into()))
        .ok()??;
    agy_token_from(raw.expose())
}

fn agy_token_from(raw: &str) -> Option<String> {
    let v: Value = serde_json::from_str(raw).ok()?;
    let token = v["token"]["access_token"]
        .as_str()
        .filter(|t| !t.trim().is_empty())?;
    let live = match v["token"]["expiry"].as_str() {
        Some(at) => time::OffsetDateTime::parse(at, &time::format_description::well_known::Rfc3339)
            .is_ok_and(|t| t > time::OffsetDateTime::now_utc()),
        None => true,
    };
    live.then(|| token.to_string())
}

/// Antigravity's limits: each model group's 5-hour and weekly windows, and the Gemini models'.
pub fn agy(token: &str, post: Post) -> Vec<UsageWindow> {
    let call = |method: &str, body: Value| -> Value {
        let req = Request {
            url: format!("{CLOUD_CODE}:{method}"),
            headers: vec![
                ("Authorization".into(), format!("Bearer {token}")),
                ("Content-Type".into(), "application/json".into()),
                ("User-Agent".into(), "antigravity".into()),
            ],
        };
        match post(&req, &body.to_string(), TIMEOUT) {
            Ok((200, b)) => serde_json::from_str(&b).unwrap_or_default(),
            _ => Value::Null,
        }
    };
    let assist = call(
        "loadCodeAssist",
        json!({ "metadata": { "ideType": "ANTIGRAVITY", "platform": "WINDOWS_AMD64", "pluginType": "GEMINI" } }),
    );
    let project = match &assist["cloudaicompanionProject"] {
        Value::String(p) => json!(p),
        Value::Object(o) => o.get("id").cloned().unwrap_or(Value::Null),
        _ => Value::Null,
    };
    let body = if project.is_null() {
        json!({})
    } else {
        json!({ "project": project })
    };
    agy_windows(
        &call("retrieveUserQuotaSummary", body.clone()),
        &call("retrieveUserQuota", body),
    )
}

/// `summary`'s groups (each bucket a window, labelled with its group) and the Gemini models'
/// share from `quota` (the lowest of theirs).
pub fn agy_windows(summary: &Value, quota: &Value) -> Vec<UsageWindow> {
    let mut out = vec![];
    for g in summary["groups"].as_array().into_iter().flatten() {
        let group = g["displayName"].as_str().unwrap_or("Models");
        for b in g["buckets"].as_array().into_iter().flatten() {
            let Some(left) = b["remainingFraction"].as_f64() else {
                continue;
            };
            let (name, span) = match b["window"].as_str() {
                Some("weekly") => ("weekly", "Weekly"),
                _ => ("session", "5-hour"),
            };
            out.push(window(
                name,
                Some(format!("{span} · {group}")),
                (1.0 - left) * 100.0,
                b["resetTime"].as_str(),
            ));
        }
    }
    let gemini = quota["buckets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| {
            b["modelId"]
                .as_str()
                .is_some_and(|m| m.starts_with("gemini"))
        })
        .filter_map(|b| Some((b["remainingFraction"].as_f64()?, b["resetTime"].as_str())))
        .min_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((left, resets)) = gemini {
        out.push(window(
            "session",
            Some("Gemini models".into()),
            (1.0 - left) * 100.0,
            resets,
        ));
    }
    out
}

/// Codex's plan windows: its app server, started for one `account/rateLimits/read`.
/// `home` is the account's `CODEX_HOME` (none: the person's own sign-in).
pub fn codex(program: &Path, home: Option<&Path>) -> Vec<UsageWindow> {
    let mut cmd = crate::process::command(program);
    cmd.arg("app-server")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    if let Some(h) = home {
        cmd.env("CODEX_HOME", h);
    }
    let Ok(mut child) = cmd.spawn() else {
        return vec![];
    };
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        return vec![];
    };
    let (tx, rx) = std::sync::mpsc::channel::<Value>();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                if tx.send(v).is_err() {
                    return;
                }
            }
        }
    });
    let send = |stdin: &mut std::process::ChildStdin, v: Value| {
        let _ = writeln!(stdin, "{v}");
        let _ = stdin.flush();
    };
    send(
        &mut stdin,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "clientInfo": { "name": "gond-code", "title": "Gond Code", "version": env!("CARGO_PKG_VERSION") },
            "capabilities": null,
        }}),
    );
    let deadline = std::time::Instant::now() + CODEX_WAIT;
    let mut out = vec![];
    while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
        let Ok(v) = rx.recv_timeout(left) else {
            break;
        };
        match v["id"].as_i64() {
            Some(1) => {
                send(
                    &mut stdin,
                    json!({ "jsonrpc": "2.0", "method": "initialized" }),
                );
                send(
                    &mut stdin,
                    json!({ "jsonrpc": "2.0", "id": 2, "method": "account/rateLimits/read", "params": null }),
                );
            }
            Some(2) => {
                out = codex_limits(&v["result"]["rateLimits"]);
                break;
            }
            _ => {}
        }
    }
    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
    out
}

/// Codex's plan usage (`account/rateLimits/read`) as usage windows: a window of five hours or
/// less is the session, a week the weekly one.
fn codex_limits(r: &serde_json::Value) -> Vec<crate::agents::UsageWindow> {
    let mut out = vec![];
    for w in [&r["primary"], &r["secondary"]] {
        let (Some(used), Some(mins)) =
            (w["usedPercent"].as_f64(), w["windowDurationMins"].as_i64())
        else {
            continue;
        };
        let name = match mins {
            ..=300 => "session",
            301..=10_080 => "weekly",
            _ => "monthly",
        };
        if out
            .iter()
            .any(|x: &crate::agents::UsageWindow| x.name == name)
        {
            continue;
        }
        let status = (r["rateLimitReachedType"].is_string()).then_some("rejected");
        out.push(crate::agents::UsageWindow::from_used(
            name,
            used / 100.0,
            w["resetsAt"].as_i64(),
            status,
        ));
    }
    out.sort_by_key(|w| if w.name == "session" { 0 } else { 1 });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The answer Claude's usage endpoint gave on 2026-10-31 (Claude Max), trimmed.
    #[test]
    fn claude_limits_become_windows() {
        let v: Value = serde_json::json!({
            "five_hour": null,
            "seven_day": { "utilization": 8.0, "resets_at": "2026-11-03T08:59:59Z" },
            "limits": [
                { "kind": "weekly_all", "group": "weekly", "percent": 8, "resets_at": "2026-11-03T09:00:00Z", "scope": null, "is_active": true },
                { "kind": "weekly_scoped", "group": "weekly", "percent": 0, "resets_at": "2026-11-03T09:00:00Z",
                  "scope": { "model": { "id": null, "display_name": "Fable" } }, "is_active": true },
                { "kind": "weekly_scoped", "group": "weekly", "percent": 0, "resets_at": null,
                  "scope": { "model": { "display_name": "Opus" } }, "is_active": false },
            ]
        });
        let w = claude_windows(&v);
        assert_eq!(w.len(), 2, "an inactive scoped limit is left out");
        assert_eq!(
            (w[0].name.as_str(), w[0].percent_left, w[0].label.as_deref()),
            ("weekly", 92.0, None)
        );
        assert_eq!(w[0].resets_at.as_deref(), Some("2026-11-03T09:00:00Z"));
        assert_eq!(w[1].label.as_deref(), Some("Weekly · Fable"));
        // An older answer without `limits`.
        let old = serde_json::json!({
            "five_hour": { "utilization": 100.0, "resets_at": "2026-10-31T23:00:00Z" },
            "seven_day": { "utilization": 40.0, "resets_at": "2026-11-03T09:00:00Z" },
        });
        let w = claude_windows(&old);
        assert_eq!((w[0].name.as_str(), w[0].percent_left), ("session", 0.0));
        assert_eq!(w[0].status.as_deref(), Some("rejected"));
        assert_eq!((w[1].name.as_str(), w[1].percent_left), ("weekly", 60.0));
    }

    /// On a real PC (ignored): each signed-in agent's own limits, read for real.
    #[test]
    #[ignore]
    fn real_plan_usage() {
        let home = dirs::home_dir().unwrap();
        if let Some(t) = claude_token(&home.join(".claude")) {
            eprintln!("claude: {:?}", claude(&t, crate::endpoint_test::curl));
        }
        if let Some(p) = crate::agents::detect::find("codex", crate::process::path()) {
            eprintln!("codex: {:?}", codex(&p, None));
        }
        let grok = std::env::var_os("GROK_HOME").map_or_else(|| home.join(".grok"), Into::into);
        if let Some(t) = agy_token() {
            eprintln!("agy: {:?}", agy(&t, crate::endpoint_test::curl_post));
        }
        if let Some(t) = crate::grok_usage::token(&grok) {
            eprintln!(
                "grok: {:?}",
                crate::grok_usage::read(&t, crate::endpoint_test::curl)
            );
        }
    }

    /// Antigravity's answers on 2026-10-31 (free tier), trimmed.
    #[test]
    fn agy_quota_becomes_windows() {
        let summary = json!({ "groups": [ { "displayName": "Claude and GPT models", "buckets": [
            { "bucketId": "3p-weekly", "window": "weekly", "resetTime": "2026-11-07T21:46:23Z", "remainingFraction": 1 },
            { "bucketId": "3p-5h", "window": "5h", "resetTime": "2026-11-01T02:46:23Z", "remainingFraction": 0.25 },
        ] } ] });
        let quota = json!({ "buckets": [
            { "modelId": "claude-opus-5-5-high", "remainingFraction": 1 },
            { "modelId": "gemini-2.5-pro", "remainingFraction": 0.9997216, "resetTime": "2026-11-01T01:20:48Z" },
            { "modelId": "gemini-3-flash", "remainingFraction": 0.5, "resetTime": "2026-11-01T01:20:48Z" },
        ] });
        let w = agy_windows(&summary, &quota);
        let shown: Vec<_> = w
            .iter()
            .map(|w| (w.name.as_str(), w.label.as_deref().unwrap(), w.percent_left))
            .collect();
        assert_eq!(
            shown,
            [
                ("weekly", "Weekly · Claude and GPT models", 100.0),
                ("session", "5-hour · Claude and GPT models", 25.0),
                ("session", "Gemini models", 50.0),
            ]
        );
        assert_eq!(
            agy_token_from(r#"{"token":{"access_token":"t","expiry":"2001-01-01T00:00:00Z"}}"#),
            None
        );
        assert_eq!(
            agy_token_from(
                r#"{"token":{"access_token":"t","expiry":"2999-01-01T00:00:00-05:00"}}"#
            )
            .as_deref(),
            Some("t")
        );
    }

    #[test]
    fn an_expired_claude_token_is_not_used() {
        let d = tempfile::tempdir().unwrap();
        let write = |at: i64| {
            std::fs::write(
                d.path().join(".credentials.json"),
                json!({ "claudeAiOauth": { "accessToken": "tok", "expiresAt": at } }).to_string(),
            )
            .unwrap()
        };
        write(1);
        assert_eq!(claude_token(d.path()), None);
        write(i64::MAX);
        assert_eq!(claude_token(d.path()).as_deref(), Some("tok"));
    }
}
