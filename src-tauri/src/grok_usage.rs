//! Grok Build's plan usage (part 2). Grok's stream says nothing about limits, so
//! after a Grok turn Gond asks the CLI's own proxy, as the CLI does: `/billing` for the
//! month's credits and `/billing?format=credits` for the week's share, with the sign-in's
//! access token from `<GROK_HOME>/auth.json`. The token goes to curl through a 0600
//! header file (`endpoint_test::curl`), never argv or logs, and is never refreshed here:
//! an expired one just means no reading until Grok renews it on its next run.

use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use crate::agents::UsageWindow;
use crate::endpoint_test::{Fetch, Request};

const BASE: &str = "https://cli-chat-proxy.grok.com/v1";
/// The client the proxy expects (the CLI's own headers; the version we recorded).
const CLIENT_VERSION: &str = "1.0.70";
pub const TIMEOUT: Duration = Duration::from_secs(8);

/// The usable access token in `auth.json` (a map of sign-ins): the first one with a key
/// that hasn't expired.
pub fn token(home: &Path) -> Option<String> {
    let store: Value =
        serde_json::from_str(&std::fs::read_to_string(home.join("auth.json")).ok()?).ok()?;
    let now = time::OffsetDateTime::now_utc();
    store.as_object()?.values().find_map(|a| {
        let key = a["key"].as_str().filter(|k| !k.trim().is_empty())?;
        let live = match a["expires_at"].as_str() {
            Some(at) => {
                time::OffsetDateTime::parse(at, &time::format_description::well_known::Rfc3339)
                    .is_ok_and(|t| t > now)
            }
            None => true,
        };
        live.then(|| key.to_string())
    })
}

fn unix_of(v: &Value) -> Option<i64> {
    v.as_str()
        .and_then(|s| {
            time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).ok()
        })
        .map(|t| t.unix_timestamp())
}

/// The windows from the two answers: the week's share when the plan has one (it's left
/// out at a fresh week's start), and the month's credits when there's a limit.
pub fn windows(monthly: &Value, weekly: &Value) -> Vec<UsageWindow> {
    let mut out = vec![];
    let week = &weekly["config"];
    if week["currentPeriod"]["type"] == "USAGE_PERIOD_TYPE_WEEKLY" {
        let used = week["creditUsagePercent"].as_f64().unwrap_or(0.0) / 100.0;
        out.push(UsageWindow::from_used(
            "weekly",
            used,
            unix_of(&week["billingPeriodEnd"]),
            None,
        ));
    }
    let month = &monthly["config"];
    if let (Some(limit), Some(used)) = (
        month["monthlyLimit"]["val"].as_f64(),
        month["used"]["val"].as_f64(),
    ) {
        if limit > 0.0 {
            out.push(UsageWindow::from_used(
                "monthly",
                used / limit,
                unix_of(&month["billingPeriodEnd"]),
                None,
            ));
        }
    }
    out
}

/// Both readings with `token`; a failed or unexpected answer gives no windows.
pub fn read(token: &str, fetch: Fetch) -> Vec<UsageWindow> {
    let get = |path: &str| -> Value {
        let req = Request {
            url: format!("{BASE}/{path}"),
            headers: vec![
                ("Authorization".into(), format!("Bearer {token}")),
                ("Accept".into(), "application/json".into()),
                ("X-XAI-Token-Auth".into(), "xai-grok-cli".into()),
                ("x-grok-client-version".into(), CLIENT_VERSION.into()),
                ("x-grok-client-mode".into(), "interactive".into()),
            ],
        };
        match fetch(&req, TIMEOUT) {
            Ok((200, body)) => serde_json::from_str(&body).unwrap_or_default(),
            _ => Value::Null,
        }
    };
    windows(&get("billing"), &get("billing?format=credits"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_from_the_billing_answers() {
        let monthly = serde_json::json!({ "config": { "monthlyLimit": { "val": 200.0 }, "used": { "val": 50.0 }, "billingPeriodEnd": "2026-11-01T00:00:00Z" } });
        let weekly = serde_json::json!({ "config": { "currentPeriod": { "type": "USAGE_PERIOD_TYPE_WEEKLY" }, "creditUsagePercent": 12.5, "billingPeriodEnd": "2026-11-02T00:00:00Z" } });
        let w = windows(&monthly, &weekly);
        assert_eq!((w[0].name.as_str(), w[0].percent_left), ("weekly", 87.5));
        assert_eq!(w[0].resets_at.as_deref(), Some("2026-11-02T00:00:00Z"));
        assert_eq!((w[1].name.as_str(), w[1].percent_left), ("monthly", 75.0));
        // A fresh week leaves the share out; no limit, no monthly window.
        assert!(windows(&serde_json::json!({}), &serde_json::json!({ "config": {} })).is_empty());
    }

    #[test]
    fn the_token_is_a_live_key_and_goes_only_in_the_header() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(
            d.path().join("auth.json"),
            r#"{"old":{"key":"tok-old","expires_at":"2001-01-01T00:00:00Z"},"now":{"key":"tok-live","expires_at":"2999-01-01T00:00:00Z"}}"#,
        )
        .unwrap();
        assert_eq!(token(d.path()).as_deref(), Some("tok-live"));
        fn fetch(req: &Request, _: Duration) -> Result<(u16, String), String> {
            assert!(!req.url.contains("tok-live"));
            assert!(req
                .headers
                .contains(&("Authorization".into(), "Bearer tok-live".into())));
            Ok((
                200,
                if req.url.ends_with("format=credits") {
                    r#"{"config":{"currentPeriod":{"type":"USAGE_PERIOD_TYPE_WEEKLY"},"creditUsagePercent":40}}"#.into()
                } else {
                    r#"{"config":{"monthlyLimit":{"val":100},"used":{"val":10}}}"#.into()
                },
            ))
        }
        let w = read("tok-live", fetch);
        assert_eq!(
            w.iter()
                .map(|w| (w.name.as_str(), w.percent_left))
                .collect::<Vec<_>>(),
            [("weekly", 60.0), ("monthly", 90.0)]
        );
        fn down(_: &Request, _: Duration) -> Result<(u16, String), String> {
            Ok((401, "{}".into()))
        }
        assert!(read("x", down).is_empty());
        assert!(token(Path::new("/nonexistent")).is_none());
    }
}
