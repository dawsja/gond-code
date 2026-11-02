//! What a terminal host answers about the machine it runs on (protocol 2): the agent under a
//! terminal's shell, how it would be resumed, which printed paths are files, a dropped file
//! written here. On WSL and SSH the app can't see that machine's processes or files, so the
//! host there (`gond-remote`) answers for it; the app's own host answers the same way.

use std::path::{Path, PathBuf};

use base64::Engine;
use serde_json::{json, Value};

use super::proto::Probe;
use super::PtyManager;
use crate::error::AppError;

/// The agent (id) under `pty_id`'s shell.
pub fn agent(mgr: &PtyManager, pty_id: &str, agents: &[(String, String)]) -> Value {
    let Some(pid) = mgr.pid(pty_id) else {
        return Value::Null;
    };
    let procs = crate::agents::running::snapshot();
    json!(crate::agents::running::agent_under(pid, &procs, agents))
}

/// How `pty_id`'s agent would be resumed: read from the agent's own store on this machine.
pub fn resume(
    mgr: &PtyManager,
    pty_id: &str,
    agents: &[(String, String)],
    since_ms: u64,
    cwd: Option<&str>,
    taken: &[String],
) -> Value {
    let Some(shell) = mgr.pid(pty_id) else {
        return Value::Null;
    };
    let procs = crate::agents::running::snapshot();
    let Some((agent, pid)) = crate::agents::running::agent_process_under(shell, &procs, agents)
    else {
        return Value::Null;
    };
    let Some(cwd) =
        crate::agents::resume::folder_of(&agent, pid, &procs).or(cwd.map(PathBuf::from))
    else {
        return Value::Null;
    };
    let since =
        std::time::UNIX_EPOCH + std::time::Duration::from_millis(since_ms.saturating_sub(10_000));
    let args = crate::agents::cwd::args(pid).unwrap_or_default();
    json!(crate::agents::resume::find(
        &agent,
        Some(pid),
        &args,
        &cwd,
        since,
        taken,
        &crate::agents::resume::Homes::user(),
    ))
}

/// This machine.
pub fn probe() -> Probe {
    let var = |k: &str| std::env::var(k).unwrap_or_default();
    let home = dirs::home_dir()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_default();
    let distro = std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|t| {
            t.lines()
                .find_map(|l| l.strip_prefix("PRETTY_NAME="))
                .map(|v| v.trim_matches('"').to_string())
        })
        .unwrap_or_default();
    let hostname = std::fs::read_to_string("/etc/hostname")
        .map(|h| h.trim().to_string())
        .unwrap_or_else(|_| var("COMPUTERNAME"));
    Probe {
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        user: if cfg!(windows) {
            var("USERNAME")
        } else {
            var("USER")
        },
        home,
        shell: super::default_shell().to_string_lossy().into_owned(),
        hostname,
        distro,
    }
}

/// Each of `paths` that is a file here, absolute: `~/x` from home, relative from `cwd`.
pub fn files(cwd: &str, paths: &[String]) -> Value {
    let home = dirs::home_dir();
    let out: Vec<Value> = paths
        .iter()
        .map(|p| {
            let full = if let Some(rest) = p.strip_prefix("~/") {
                home.as_ref().map(|h| h.join(rest))
            } else if Path::new(p).is_absolute() {
                Some(PathBuf::from(p))
            } else {
                Some(Path::new(cwd).join(p))
            };
            match full.filter(|f| f.is_file()) {
                Some(f) => json!(f.to_string_lossy()),
                None => Value::Null,
            }
        })
        .collect();
    Value::Array(out)
}

/// Writes a dropped file into `drops/<time>/<name>` under the host's data folder.
pub fn upload(name: &str, b64: &str) -> Result<Value, AppError> {
    let name = Path::new(name)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .filter(|n| !n.is_empty() && n != "." && n != "..")
        .ok_or_else(|| AppError::Invalid("a file name".into()))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| AppError::Invalid(format!("base64: {e}")))?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    let dir = crate::appdata::local()
        .ok_or_else(|| AppError::Invalid("no data folder".into()))?
        .join("drops")
        .join(stamp.to_string());
    std::fs::create_dir_all(&dir).map_err(|e| AppError::Invalid(e.to_string()))?;
    let file = dir.join(name);
    std::fs::write(&file, bytes).map_err(|e| AppError::Invalid(e.to_string()))?;
    Ok(json!(file.to_string_lossy()))
}

/// A line of `authorized_keys`: its options (if any), type, base64 key and comment.
fn key_of(line: &str) -> Option<(String, String, String)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let words: Vec<&str> = line.split_whitespace().collect();
    // Options come first (`from="…",no-pty`); the type is the word before a base64 key.
    let i = words
        .iter()
        .position(|w| w.starts_with("ssh-") || w.starts_with("ecdsa-") || w.starts_with("sk-"))?;
    let key = words.get(i + 1)?;
    Some((
        words[i].to_string(),
        key.to_string(),
        words[i + 2..].join(" "),
    ))
}

/// `~/.ssh/authorized_keys` in `home`: `add` a key's line unless that key is there; `remove`
/// every line of a key (by its base64). The other lines stay as they are. A new `.ssh` is
/// 700 and a new file 600, as sshd wants them. → the keys there.
pub fn authorized_keys_in(
    home: &Path,
    add: Option<&str>,
    remove: Option<&str>,
) -> Result<Value, AppError> {
    let io = |e: std::io::Error| AppError::Invalid(format!("authorized_keys: {e}"));
    let dir = home.join(".ssh");
    let file = dir.join("authorized_keys");
    let mut text = std::fs::read_to_string(&file).unwrap_or_default();
    if let Some(line) = add {
        let line = line.trim();
        let Some((_, key, _)) = key_of(line).filter(|_| !line.contains(['\n', '\r'])) else {
            return Err(AppError::Invalid("not a public key's line".into()));
        };
        if !text
            .lines()
            .any(|l| key_of(l).is_some_and(|(_, k, _)| k == key))
        {
            if !dir.exists() {
                std::fs::create_dir_all(&dir).map_err(io)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
                }
            }
            let mut f = std::fs::OpenOptions::new();
            f.create(true).append(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                f.mode(0o600);
            }
            let mut f = f.open(&file).map_err(io)?;
            let gap = if text.is_empty() || text.ends_with('\n') {
                ""
            } else {
                "\n"
            };
            use std::io::Write;
            f.write_all(format!("{gap}{line}\n").as_bytes())
                .map_err(io)?;
            text = format!("{text}{gap}{line}\n");
        }
    }
    if let Some(key) = remove {
        let kept: Vec<&str> = text
            .lines()
            .filter(|l| !key_of(l).is_some_and(|(_, k, _)| k == key))
            .collect();
        if kept.len() != text.lines().count() {
            let mut out = kept.join("\n");
            if !out.is_empty() {
                out.push('\n');
            }
            // A new file beside it, with the old one's permissions, then over it.
            let tmp = dir.join(".authorized_keys.gond");
            std::fs::write(&tmp, &out).map_err(io)?;
            if let Ok(m) = std::fs::metadata(&file) {
                let _ = std::fs::set_permissions(&tmp, m.permissions());
            }
            std::fs::rename(&tmp, &file).map_err(io)?;
            text = out;
        }
    }
    Ok(Value::Array(
        text.lines()
            .filter_map(key_of)
            .map(|(t, k, c)| json!({ "type": t, "key": k, "comment": c }))
            .collect(),
    ))
}

/// `~/.ssh/authorized_keys` here (`authorized_keys_in`).
pub fn authorized_keys(add: Option<&str>, remove: Option<&str>) -> Result<Value, AppError> {
    let home = dirs::home_dir().ok_or_else(|| AppError::Invalid("no home folder".into()))?;
    authorized_keys_in(&home, add, remove)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorized_keys_gain_and_lose_one_key_and_keep_the_rest() {
        let home = tempfile::tempdir().unwrap();
        let ssh = home.path().join(".ssh");
        let file = ssh.join("authorized_keys");
        // No .ssh yet: made, with the key in it.
        let r = authorized_keys_in(home.path(), Some("ssh-ed25519 AAAAnew me@pc"), None).unwrap();
        assert_eq!(r[0]["key"], "AAAAnew");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!((mode(&ssh), mode(&file)), (0o700, 0o600));
        }
        // Someone's own lines, one with options and no newline at the end.
        std::fs::write(
            &file,
            "# mine\nfrom=\"10.0.0.0/8\",no-pty ssh-rsa AAAAold work laptop\nssh-ed25519 AAAAnew me@pc",
        )
        .unwrap();
        // Adding a key that's there (another comment) changes nothing.
        authorized_keys_in(home.path(), Some("ssh-ed25519 AAAAnew other"), None).unwrap();
        let r = authorized_keys_in(home.path(), Some("ssh-ed25519 AAAAsecond"), None).unwrap();
        assert_eq!(r.as_array().unwrap().len(), 3);
        assert_eq!(r[0]["comment"], "work laptop");
        assert!(std::fs::read_to_string(&file)
            .unwrap()
            .ends_with("ssh-ed25519 AAAAnew me@pc\nssh-ed25519 AAAAsecond\n"));
        // Removing takes that key's line alone.
        let r = authorized_keys_in(home.path(), None, Some("AAAAnew")).unwrap();
        assert_eq!(r.as_array().unwrap().len(), 2);
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "# mine\nfrom=\"10.0.0.0/8\",no-pty ssh-rsa AAAAold work laptop\nssh-ed25519 AAAAsecond\n"
        );
        assert!(authorized_keys_in(home.path(), Some("not a key"), None).is_err());
        assert!(
            authorized_keys_in(home.path(), Some("ssh-ed25519 AAAA\nssh-rsa B"), None).is_err()
        );
    }

    /// Only paths that are files come back, made absolute; the rest are null.
    #[test]
    fn files_are_found_from_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "x").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        let cwd = dir.path().to_string_lossy();
        let got = files(&cwd, &["a.rs".into(), "sub".into(), "nope.rs".into()]);
        assert_eq!(got[0], json!(dir.path().join("a.rs").to_string_lossy()));
        assert_eq!(got[1], Value::Null);
        assert_eq!(got[2], Value::Null);
    }
}
