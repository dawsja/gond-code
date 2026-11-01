//! Git's user.name and user.email: read, and set globally when the human asks.
//! core.autocrlf and everything else in the user's config is left alone.

use std::path::Path;

use schemars::JsonSchema;
use serde::Serialize;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Default, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

/// `git config --global` against `global` (a file) or the user's own config.
fn config(global: Option<&Path>, args: &[&str]) -> std::process::Command {
    let mut cmd = super::command(&std::env::temp_dir());
    cmd.args(["config", "--global"]).args(args);
    if let Some(file) = global {
        cmd.env("GIT_CONFIG_GLOBAL", file);
    }
    cmd
}

fn get(global: Option<&Path>, key: &str) -> AppResult<Option<String>> {
    let out = config(global, &["--get", key])
        .output()
        .map_err(super::spawn_failed)?;
    let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
    // `--get` exits 1 when the key is unset.
    Ok((out.status.success() && !v.is_empty()).then_some(v))
}

pub fn read(global: Option<&Path>) -> AppResult<Identity> {
    Ok(Identity {
        name: get(global, "user.name")?,
        email: get(global, "user.email")?,
    })
}

pub fn set(global: Option<&Path>, name: &str, email: &str) -> AppResult<Identity> {
    let (name, email) = (name.trim(), email.trim());
    if name.is_empty() {
        return Err(AppError::Invalid("Add a name for your commits.".into()));
    }
    if !email.contains('@') || email.contains(char::is_whitespace) {
        return Err(AppError::Invalid(format!("{email} isn't an email address")));
    }
    for (key, value) in [("user.name", name), ("user.email", email)] {
        let out = config(global, &[key, value])
            .output()
            .map_err(super::spawn_failed)?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(AppError::GitFailed(format!("git config {key}: {err}")));
        }
    }
    read(global)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_sets_without_touching_autocrlf() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("gitconfig");
        std::fs::write(&file, "[core]\n\tautocrlf = input\n").unwrap();
        assert_eq!(read(Some(&file)).unwrap(), Identity::default());
        assert_eq!(
            set(Some(&file), " ", "a@b.c").unwrap_err().code(),
            "invalid"
        );
        assert_eq!(
            set(Some(&file), "Sam", "nope").unwrap_err().code(),
            "invalid"
        );
        let id = set(Some(&file), "Sam", "d@example.com").unwrap();
        assert_eq!(id.name.as_deref(), Some("Sam"));
        assert_eq!(id.email.as_deref(), Some("d@example.com"));
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("autocrlf = input"), "{text}");
    }

    #[test]
    fn missing_git_is_typed() {
        let e = crate::git::spawn_failed(std::io::ErrorKind::NotFound.into());
        assert_eq!(e.code(), "git_missing");
        let e = crate::git::spawn_failed(std::io::ErrorKind::PermissionDenied.into());
        assert_eq!(e.code(), "git_failed");
    }
}
