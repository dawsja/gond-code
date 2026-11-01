//! Runs the git CLI with the user's PATH.

pub mod identity;

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{AppError, AppResult};

/// A `git` command in `cwd` that never pops a console window on Windows.
pub fn command(cwd: &Path) -> Command {
    let mut cmd = crate::process::command("git");
    cmd.current_dir(cwd);
    // The live PATH, so a Git installed by `prereq.install` is found without a restart.
    if let Some(path) = crate::process::path() {
        cmd.env("PATH", path);
    }
    // Deep project trees: don't stop at MAX_PATH.
    #[cfg(windows)]
    cmd.args(["-c", "core.longpaths=true"]);
    crate::process::no_window(&mut cmd);
    cmd
}

/// Why git couldn't start: `GitMissing` when it isn't on PATH.
pub fn spawn_failed(e: std::io::Error) -> AppError {
    if e.kind() == std::io::ErrorKind::NotFound {
        AppError::GitMissing("Git isn't installed yet.".into())
    } else {
        AppError::GitFailed(format!("could not run git: {e}"))
    }
}

/// Runs git and returns trimmed stdout, or `GitFailed` with stderr.
pub fn run(cwd: &Path, args: &[&str]) -> AppResult<String> {
    let out = command(cwd).args(args).output().map_err(spawn_failed)?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(AppError::GitFailed(format!(
            "git {}: {err}",
            args.join(" ")
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The top level of the work tree containing `path`, or `NotAGitRepo`.
pub fn repo_root(path: &Path) -> AppResult<PathBuf> {
    let not_repo = || AppError::NotAGitRepo(format!("{} is not a git repository", path.display()));
    if !path.is_dir() {
        return Err(not_repo());
    }
    let top = run(path, &["rev-parse", "--show-toplevel"]).map_err(|e| match e {
        AppError::GitMissing(_) => e,
        _ => not_repo(),
    })?;
    Ok(dunce(PathBuf::from(top)))
}

/// origin's HEAD branch if there is one, else the checked-out branch, else "main".
pub fn default_branch(repo: &Path) -> String {
    if let Ok(r) = run(
        repo,
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
    ) {
        if let Some(b) = r.strip_prefix("origin/") {
            return b.to_string();
        }
    }
    run(repo, &["symbolic-ref", "--short", "HEAD"])
        .ok()
        .filter(|b| !b.is_empty())
        .unwrap_or_else(|| "main".into())
}

/// git prints `C:/x/y` on Windows; store paths in the native form.
/// `p` with Windows separators (and no `\\?\` prefix) for display and storage.
pub fn dunce_path(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy().into_owned();
    dunce(PathBuf::from(
        s.strip_prefix(r"\\?\").unwrap_or(&s).to_string(),
    ))
}

fn dunce(p: PathBuf) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(p.to_string_lossy().replace('/', "\\"))
    } else {
        p
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A fresh repo on branch `main` with one commit.
    pub fn temp_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        run(p, &["init", "-q", "-b", "main"]).unwrap();
        for (k, v) in [
            ("user.name", "t"),
            ("user.email", "t@t"),
            ("commit.gpgsign", "false"),
        ] {
            run(p, &["config", k, v]).unwrap();
        }
        run(p, &["commit", "-q", "--allow-empty", "-m", "init"]).unwrap();
        dir
    }

    #[test]
    fn plain_folder_is_not_a_repo() {
        let dir = tempfile::tempdir().unwrap();
        let err = repo_root(dir.path()).unwrap_err();
        assert_eq!(err.code(), "not_a_git_repo");
        assert_eq!(
            repo_root(&dir.path().join("missing")).unwrap_err().code(),
            "not_a_git_repo"
        );
    }

    #[test]
    fn root_and_default_branch() {
        let repo = temp_repo();
        let sub = repo.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        let root = repo_root(&sub).unwrap();
        assert_eq!(
            root.canonicalize().unwrap(),
            repo.path().canonicalize().unwrap()
        );
        assert_eq!(default_branch(repo.path()), "main");
    }
}
