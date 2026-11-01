//! "Test connection" for API accounts (`account.test`): list the endpoint's models with the account's key and headers, in a
//! few seconds, with one plain sentence when it fails. The key goes to curl in a header file
//! (owner-only, deleted after), never on the command line.

use std::time::{Duration, Instant};

use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;

use crate::accounts::Cloud;

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl TestResult {
    fn fail(error: impl Into<String>) -> Self {
        TestResult {
            ok: false,
            models: None,
            ms: None,
            error: Some(error.into()),
        }
    }
}

/// What to call: the URL and every header, the key included.
#[derive(Debug, PartialEq)]
pub struct Request {
    pub url: String,
    pub headers: Vec<(String, String)>,
}

/// Everything a test needs, secrets already resolved (they live only in this call).
pub struct Target<'a> {
    /// `anthropic`, `openai`, `google` or `xai` (the harness's provider).
    pub provider: &'a str,
    pub endpoint: Option<&'a str>,
    pub key: Option<&'a str>,
    pub bearer: bool,
    pub organization: Option<&'a str>,
    pub project: Option<&'a str>,
    pub azure_api_version: Option<&'a str>,
    pub cloud: Option<&'a Cloud>,
    pub headers: Vec<(String, String)>,
}

/// The models request for a target, or why it can't be tested.
pub fn request(t: &Target) -> Result<Request, String> {
    if t.cloud.is_some() {
        return Err(
            "Testing Bedrock and Vertex accounts isn't supported yet; start an agent to check it."
                .into(),
        );
    }
    let key = t.key.ok_or("This account has no key to test with.")?;
    let base = |default: &str| {
        t.endpoint
            .unwrap_or(default)
            .trim_end_matches('/')
            .to_string()
    };
    let mut headers = vec![];
    let url = match t.provider {
        "anthropic" => {
            if t.bearer {
                headers.push(("Authorization".into(), format!("Bearer {key}")));
            } else {
                headers.push(("x-api-key".into(), key.into()));
            }
            headers.push(("anthropic-version".into(), "2023-06-01".into()));
            let b = base("https://api.anthropic.com");
            if b.ends_with("/v1") {
                format!("{b}/models")
            } else {
                format!("{b}/v1/models")
            }
        }
        "openai" => match t.azure_api_version {
            Some(v) => {
                headers.push(("api-key".into(), key.into()));
                format!(
                    "{}/models?api-version={v}",
                    base("https://api.openai.com/v1")
                )
            }
            None => {
                headers.push(("Authorization".into(), format!("Bearer {key}")));
                if let Some(o) = t.organization {
                    headers.push(("OpenAI-Organization".into(), o.into()));
                }
                if let Some(p) = t.project {
                    headers.push(("OpenAI-Project".into(), p.into()));
                }
                format!("{}/models", base("https://api.openai.com/v1"))
            }
        },
        "xai" => {
            headers.push(("Authorization".into(), format!("Bearer {key}")));
            format!("{}/models", base("https://api.x.ai/v1"))
        }
        "google" => match t.endpoint {
            // A gateway (agy's AGY_LLM_GATEWAY_URL) is OpenAI-shaped in practice; unverified.
            Some(e) => {
                headers.push(("Authorization".into(), format!("Bearer {key}")));
                format!("{}/models", e.trim_end_matches('/'))
            }
            None => {
                headers.push(("x-goog-api-key".into(), key.into()));
                "https://generativelanguage.googleapis.com/v1beta/models".into()
            }
        },
        other => return Err(format!("Gond can't test {other} accounts yet.")),
    };
    headers.extend(t.headers.iter().cloned());
    Ok(Request { url, headers })
}

/// Model ids from a models response: `data[].id` (OpenAI, Anthropic, xAI, most gateways)
/// or `models[].name` (Gemini, without its `models/` prefix).
pub fn parse_models(body: &str) -> Option<Vec<String>> {
    let v: Value = serde_json::from_str(body).ok()?;
    let ids: Vec<String> = if let Some(data) = v["data"].as_array() {
        data.iter()
            .filter_map(|m| m["id"].as_str().map(String::from))
            .collect()
    } else {
        v["models"]
            .as_array()?
            .iter()
            .filter_map(|m| m["name"].as_str())
            .map(|n| n.trim_start_matches("models/").to_string())
            .collect()
    };
    Some(ids)
}

/// One plain sentence for an HTTP status.
fn status_sentence(code: u16, url: &str) -> String {
    match code {
        401 | 403 => format!("The endpoint refused the key ({code})."),
        404 => format!("{url} has no models list (404); check the base URL."),
        429 => "The endpoint is rate-limiting this key (429); try again in a minute.".into(),
        500..=599 => format!("The endpoint had an error ({code}); try again later."),
        _ => format!("The endpoint answered {code} instead of a models list."),
    }
}

/// `(status, body)` from a GET, or a plain sentence when the host can't be reached.
pub type Fetch = fn(&Request, Duration) -> Result<(u16, String), String>;

pub fn run(t: &Target, fetch: Fetch) -> TestResult {
    let req = match request(t) {
        Ok(r) => r,
        Err(e) => return TestResult::fail(e),
    };
    let start = Instant::now();
    let (code, body) = match fetch(&req, Duration::from_secs(10)) {
        Ok(r) => r,
        Err(e) => return TestResult::fail(e),
    };
    let ms = start.elapsed().as_millis() as u64;
    if code != 200 {
        return TestResult {
            ms: Some(ms),
            ..TestResult::fail(status_sentence(code, &req.url))
        };
    }
    match parse_models(&body) {
        Some(models) => TestResult {
            ok: true,
            models: Some(models),
            ms: Some(ms),
            error: None,
        },
        None => TestResult {
            ms: Some(ms),
            ..TestResult::fail("The endpoint answered, but not with a models list.")
        },
    }
}

/// The real fetch: curl with the headers in an owner-only file (`-H @file`), so the key is
/// never in argv, and no console window.
pub fn curl(req: &Request, timeout: Duration) -> Result<(u16, String), String> {
    let bin = crate::agents::detect::find("curl", crate::process::path())
        .ok_or("curl isn't installed, so Gond can't test the connection.")?;
    curl_at(&bin, req, None, timeout)
}

/// `(status, body)` from a POST of `body` (JSON), or a plain sentence when the host can't be
/// reached.
pub type Post = fn(&Request, &str, Duration) -> Result<(u16, String), String>;

/// The real POST: as [`curl`], the body from an owner-only file too.
pub fn curl_post(req: &Request, body: &str, timeout: Duration) -> Result<(u16, String), String> {
    let bin = crate::agents::detect::find("curl", crate::process::path())
        .ok_or("curl isn't installed, so Gond can't reach it.")?;
    curl_at(&bin, req, Some(body), timeout)
}

fn curl_at(
    bin: &std::path::Path,
    req: &Request,
    post: Option<&str>,
    timeout: Duration,
) -> Result<(u16, String), String> {
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let file = dir.path().join("headers");
    let body: String = req
        .headers
        .iter()
        .map(|(k, v)| format!("{k}: {v}\n"))
        .collect();
    write_private(&file, &body).map_err(|e| e.to_string())?;
    let host = req.url.split('/').take(3).collect::<Vec<_>>().join("/");
    let mut cmd = crate::process::command(bin);
    cmd.args(["--silent", "--show-error", "--location", "--max-time"])
        .arg(timeout.as_secs().to_string())
        .args(["--write-out", "\n%{http_code}", "-H"])
        .arg(format!("@{}", file.display()));
    if let Some(data) = post {
        let data_file = dir.path().join("body");
        write_private(&data_file, data).map_err(|e| e.to_string())?;
        cmd.arg("--data-binary")
            .arg(format!("@{}", data_file.display()));
    }
    cmd.arg(&req.url).stdin(std::process::Stdio::null());
    let out = crate::process::when_free(|| cmd.output())
        .map_err(|_| "Couldn't run curl to test the connection.".to_string())?;
    if !out.status.success() {
        return Err(format!(
            "Couldn't reach {host} within {} s.",
            timeout.as_secs()
        ));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let (body, code) = text.rsplit_once('\n').unwrap_or(("", &text));
    let code = code.trim().parse().unwrap_or(0);
    Ok((code, body.to_string()))
}

fn write_private(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    use std::io::Write;
    opts.open(path)?.write_all(contents.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target<'a>(provider: &'a str, endpoint: Option<&'a str>) -> Target<'a> {
        Target {
            provider,
            endpoint,
            key: Some("sk-test"),
            bearer: false,
            organization: None,
            project: None,
            azure_api_version: None,
            cloud: None,
            headers: vec![],
        }
    }

    #[test]
    fn requests_per_provider() {
        let a = request(&target("anthropic", None)).unwrap();
        assert_eq!(a.url, "https://api.anthropic.com/v1/models");
        assert!(a.headers.contains(&("x-api-key".into(), "sk-test".into())));
        let gw = request(&Target {
            bearer: true,
            headers: vec![("X-Team".into(), "core".into())],
            ..target("anthropic", Some("https://litellm.example/v1/"))
        })
        .unwrap();
        assert_eq!(gw.url, "https://litellm.example/v1/models");
        assert!(gw
            .headers
            .contains(&("Authorization".into(), "Bearer sk-test".into())));
        assert!(gw.headers.contains(&("X-Team".into(), "core".into())));
        let o = request(&Target {
            organization: Some("org-1"),
            ..target("openai", Some("https://openrouter.ai/api/v1"))
        })
        .unwrap();
        assert_eq!(o.url, "https://openrouter.ai/api/v1/models");
        assert!(o
            .headers
            .contains(&("OpenAI-Organization".into(), "org-1".into())));
        let az = request(&Target {
            azure_api_version: Some("2025-04-01-preview"),
            ..target("openai", Some("https://acme.openai.azure.com/openai"))
        })
        .unwrap();
        assert_eq!(
            az.url,
            "https://acme.openai.azure.com/openai/models?api-version=2025-04-01-preview"
        );
        assert!(az.headers.contains(&("api-key".into(), "sk-test".into())));
        let g = request(&target("google", None)).unwrap();
        assert!(g
            .url
            .starts_with("https://generativelanguage.googleapis.com/"));
        assert!(request(&Target {
            key: None,
            ..target("openai", None)
        })
        .is_err());
        let bedrock = Cloud::Bedrock {
            region: "us-east-1".into(),
        };
        assert!(request(&Target {
            cloud: Some(&bedrock),
            ..target("anthropic", None)
        })
        .is_err());
    }

    #[test]
    fn parses_model_lists() {
        assert_eq!(
            parse_models(r#"{"data":[{"id":"claude-opus-5-5"},{"id":"claude-sonnet-5-5"}]}"#),
            Some(vec!["claude-opus-5-5".into(), "claude-sonnet-5-5".into()])
        );
        assert_eq!(
            parse_models(r#"{"models":[{"name":"models/gemini-4-argon"}]}"#),
            Some(vec!["gemini-4-argon".into()])
        );
        assert_eq!(parse_models("<html>"), None);
    }

    #[test]
    fn plain_sentences_for_failures() {
        let t = target("openai", Some("https://gw.example/v1"));
        let r = run(&t, |_, _| Ok((401, "{}".into())));
        assert_eq!(
            r.error.as_deref(),
            Some("The endpoint refused the key (401).")
        );
        let r = run(&t, |_, _| {
            Err("Couldn't reach https://gw.example within 10 s.".into())
        });
        assert!(!r.ok && r.error.unwrap().contains("Couldn't reach"));
        let r = run(&t, |_, _| {
            Ok((200, r#"{"data":[{"id":"gpt-6.1-sol"}]}"#.into()))
        });
        assert!(r.ok && r.models == Some(vec!["gpt-6.1-sol".into()]) && r.ms.is_some());
        let r = run(&t, |_, _| Ok((200, "not json".into())));
        assert!(!r.ok);
    }

    #[cfg(unix)]
    #[test]
    fn curl_keeps_the_key_out_of_argv() {
        // A fake curl that records its argv, and finds the key only in the header file.
        use std::os::unix::fs::PermissionsExt;
        let bin = tempfile::tempdir().unwrap();
        let log = bin.path().join("argv");
        let fake = bin.path().join("curl");
        std::fs::write(
            &fake,
            format!(
                "#!/bin/sh\necho \"$@\" > {}\nfor a in \"$@\"; do case \"$a\" in @*) cat \"${{a#@}}\" >> {}.h;; esac; done\nprintf '{{\"data\":[{{\"id\":\"m1\"}}]}}\\n200'\n",
                log.display(),
                log.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let req = request(&target("openai", Some("https://gw.example/v1"))).unwrap();
        let (code, body) = curl_at(&fake, &req, None, Duration::from_secs(5)).unwrap();
        assert_eq!(code, 200);
        assert_eq!(parse_models(&body), Some(vec!["m1".into()]));
        let argv = std::fs::read_to_string(&log).unwrap();
        assert!(!argv.contains("sk-test"), "{argv}");
        let headers = std::fs::read_to_string(format!("{}.h", log.display())).unwrap();
        assert!(headers.contains("Authorization: Bearer sk-test"));
    }
}
