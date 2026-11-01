//! HTTP for Settings → Usage: the agents' own usage endpoints, called through curl with the
//! sign-in's token in an owner-only header file (deleted after), never on the command line.

use std::time::Duration;

/// What to call: the URL and every header, the token included.
#[derive(Debug, PartialEq)]
pub struct Request {
    pub url: String,
    pub headers: Vec<(String, String)>,
}

/// `(status, body)` from a GET, or a plain sentence when the host can't be reached.
pub type Fetch = fn(&Request, Duration) -> Result<(u16, String), String>;

/// The real fetch: curl with the headers in an owner-only file (`-H @file`), so the key is
/// never in argv, and no console window.
pub fn curl(req: &Request, timeout: Duration) -> Result<(u16, String), String> {
    let bin = crate::agents::detect::find("curl", crate::process::path())
        .ok_or("curl isn't installed, so Gond can't reach it.")?;
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
    let out =
        crate::process::when_free(|| cmd.output()).map_err(|_| "Couldn't run curl.".to_string())?;
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn curl_keeps_the_token_out_of_argv() {
        // A fake curl that records its argv, and finds the token only in the header file.
        use std::os::unix::fs::PermissionsExt;
        let bin = tempfile::tempdir().unwrap();
        let log = bin.path().join("argv");
        let fake = bin.path().join("curl");
        std::fs::write(
            &fake,
            format!(
                "#!/bin/sh
echo \"$@\" > {}
for a in \"$@\"; do case \"$a\" in @*) cat \"${{a#@}}\" >> {}.h;; esac; done
printf 'ok\n200'
",
                log.display(),
                log.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let req = Request {
            url: "https://api.example/usage".into(),
            headers: vec![("Authorization".into(), "Bearer sk-test".into())],
        };
        let (code, body) = curl_at(&fake, &req, None, Duration::from_secs(5)).unwrap();
        assert_eq!((code, body.as_str()), (200, "ok"));
        let argv = std::fs::read_to_string(&log).unwrap();
        assert!(!argv.contains("sk-test"), "{argv}");
        let headers = std::fs::read_to_string(format!("{}.h", log.display())).unwrap();
        assert!(headers.contains("Authorization: Bearer sk-test"));
    }
}
