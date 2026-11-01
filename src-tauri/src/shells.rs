//! The shells a plain terminal can run: the ones on this PC, found each time they're listed,
//! and the one Settings picked. An agent's pane runs the agent's CLI, never these.

use std::path::PathBuf;

use serde::Serialize;

/// One shell: its id in Settings (`pwsh`, `wsl:Ubuntu`…), its name, and how it starts.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Shell {
    pub id: String,
    pub label: String,
    pub program: String,
    pub args: Vec<String>,
}

fn found(id: &str, label: &str, program: PathBuf, args: &[&str]) -> Shell {
    Shell {
        id: id.into(),
        label: label.into(),
        program: program.to_string_lossy().into_owned(),
        args: args.iter().map(|a| a.to_string()).collect(),
    }
}

/// Git for Windows' bash, beside the `git` on PATH or where its installer puts it.
#[cfg(windows)]
fn git_bash() -> Option<PathBuf> {
    let beside = which::which("git").ok().and_then(|git| {
        // …\Git\cmd\git.exe → …\Git\bin\bash.exe
        let root = git.parent()?.parent()?.to_path_buf();
        Some(root.join("bin").join("bash.exe"))
    });
    let installed = std::env::var_os("ProgramFiles")
        .map(|p| PathBuf::from(p).join("Git").join("bin").join("bash.exe"));
    [beside, installed]
        .into_iter()
        .flatten()
        .find(|p| p.is_file())
}

/// The WSL distributions installed (`wsl -l -q`, which answers in UTF-16), less Docker's own.
#[cfg(windows)]
fn wsl_distros() -> Vec<String> {
    let Ok(wsl) = which::which("wsl") else {
        return vec![];
    };
    let Ok(out) = crate::process::command(&wsl).args(["-l", "-q"]).output() else {
        return vec![];
    };
    if !out.status.success() {
        return vec![];
    }
    let units: Vec<u16> = out
        .stdout
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&units)
        .lines()
        .map(|l| {
            l.trim_matches(|c: char| c.is_whitespace() || c == '\0')
                .to_string()
        })
        .filter(|d| !d.is_empty() && !d.starts_with("docker-desktop"))
        .collect()
}

/// Every shell on this PC, the usual first.
pub fn list() -> Vec<Shell> {
    #[cfg(windows)]
    {
        let mut out = vec![];
        if let Ok(p) = which::which("pwsh") {
            out.push(found("pwsh", "PowerShell 7", p, &["-NoLogo"]));
        }
        if let Ok(p) = which::which("powershell") {
            out.push(found("powershell", "Windows PowerShell", p, &["-NoLogo"]));
        }
        let cmd = which::which("cmd").unwrap_or_else(|_| PathBuf::from("cmd.exe"));
        out.push(found("cmd", "Command Prompt", cmd, &[]));
        if let Some(p) = git_bash() {
            out.push(found("gitbash", "Git Bash", p, &["--login", "-i"]));
        }
        if let Ok(wsl) = which::which("wsl") {
            for d in wsl_distros() {
                out.push(Shell {
                    id: format!("wsl:{d}"),
                    label: format!("{d} (WSL)"),
                    program: wsl.to_string_lossy().into_owned(),
                    args: vec!["-d".into(), d],
                });
            }
        }
        out
    }
    #[cfg(not(windows))]
    {
        vec![found(
            "default",
            "Default shell",
            crate::pty::default_shell(),
            &[],
        )]
    }
}

/// The shell Settings picked, if it's still on this PC.
pub fn chosen(id: Option<&str>) -> Option<Shell> {
    let id = id?;
    list().into_iter().find(|s| s.id == id)
}

#[cfg(test)]
mod tests {
    /// There's always a shell to pick, and ids are unique.
    #[test]
    fn this_pc_has_a_shell_to_pick() {
        let all = super::list();
        assert!(!all.is_empty());
        let mut ids: Vec<_> = all.iter().map(|s| s.id.clone()).collect();
        ids.dedup();
        assert_eq!(ids.len(), all.len());
        assert_eq!(super::chosen(Some(&all[0].id)), Some(all[0].clone()));
        assert_eq!(super::chosen(Some("nope")), None);
    }
}
