//! Which agent you started in a terminal: every pane is a shell, and typing `claude` or
//! `codex` there runs the agent's CLI under it. Found from the shell's process tree, by the
//! program's name, so the pane can show the agent and watch it (Working, Needs you, Done).
//! An agent run through a script runtime (`node.exe` for an npm install) isn't recognised.

/// A running process: its id, its parent's, and its program's file name (`codex.exe`).
#[derive(Debug, Clone)]
pub struct Proc {
    pub pid: u32,
    pub parent: u32,
    pub exe: String,
}

/// Whether program file `exe` is agent command `command`: `claude.exe`, or a winget package's
/// release-named program (`codex-x86_64-pc-windows-msvc.exe`).
fn is(exe: &str, command: &str) -> bool {
    let exe = exe.to_lowercase();
    let stem = exe.strip_suffix(".exe").unwrap_or(&exe);
    // A custom agent's command can be a path; its program's name is what runs.
    let command = std::path::Path::new(command)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if command.is_empty() {
        return false;
    }
    stem == command || (stem.starts_with(&format!("{command}-")) && stem.contains("-pc-windows-"))
}

/// The first agent (nearest the shell) among `root`'s descendants, as its id from
/// `agents` (id, command). An agent's own helpers below it don't count over it.
pub fn agent_under(root: u32, procs: &[Proc], agents: &[(String, String)]) -> Option<String> {
    agent_process_under(root, procs, agents).map(|(id, _)| id)
}

/// `agent_under`, with the agent's process id.
pub fn agent_process_under(
    root: u32,
    procs: &[Proc],
    agents: &[(String, String)],
) -> Option<(String, u32)> {
    let mut level = vec![root];
    let mut seen = std::collections::HashSet::from([root]);
    while !level.is_empty() {
        let mut next = Vec::new();
        for p in procs.iter().filter(|p| level.contains(&p.parent)) {
            if !seen.insert(p.pid) {
                continue;
            }
            if let Some((id, _)) = agents.iter().find(|(_, c)| is(&p.exe, c)) {
                return Some((id.clone(), p.pid));
            }
            next.push(p.pid);
        }
        level = next;
    }
    None
}

/// Every process now. Empty off Windows.
pub fn snapshot() -> Vec<Proc> {
    #[cfg(windows)]
    {
        win::snapshot()
    }
    #[cfg(not(windows))]
    {
        vec![]
    }
}

#[cfg(windows)]
mod win {
    use super::Proc;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    pub fn snapshot() -> Vec<Proc> {
        let mut out = Vec::new();
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            if snap == INVALID_HANDLE_VALUE {
                return out;
            }
            let mut e: PROCESSENTRY32W = std::mem::zeroed();
            e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut ok = Process32FirstW(snap, &mut e) != 0;
            while ok {
                let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
                out.push(Proc {
                    pid: e.th32ProcessID,
                    parent: e.th32ParentProcessID,
                    exe: String::from_utf16_lossy(&e.szExeFile[..len]),
                });
                ok = Process32NextW(snap, &mut e) != 0;
            }
            CloseHandle(snap);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, parent: u32, exe: &str) -> Proc {
        Proc {
            pid,
            parent,
            exe: exe.into(),
        }
    }

    fn agents() -> Vec<(String, String)> {
        [
            ("claude", "claude"),
            ("codex", "codex"),
            ("cursor", "cursor-agent"),
        ]
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .into()
    }

    /// The agent typed into the shell is found below it, however it's named, and only below
    /// that shell; an agent's own children (a shell it runs a tool in) don't hide it.
    #[test]
    fn finds_the_agent_under_its_shell() {
        let procs = [
            p(10, 1, "pwsh.exe"),
            p(11, 10, "Claude.exe"),
            p(12, 11, "pwsh.exe"),
            p(13, 12, "codex.exe"),
            p(20, 1, "pwsh.exe"),
            p(21, 20, "cmd.exe"),
            p(22, 21, "codex-x86_64-pc-windows-msvc.exe"),
            p(30, 1, "pwsh.exe"),
            p(31, 30, "git.exe"),
            p(40, 1, "pwsh.exe"),
            p(41, 40, "cursor-agent.exe"),
        ];
        assert_eq!(
            agent_under(10, &procs, &agents()).as_deref(),
            Some("claude")
        );
        assert_eq!(agent_under(20, &procs, &agents()).as_deref(), Some("codex"));
        assert_eq!(agent_under(30, &procs, &agents()), None);
        assert_eq!(
            agent_under(40, &procs, &agents()).as_deref(),
            Some("cursor")
        );
        // Not a sandbox helper that only starts with the name.
        assert!(!is("codex-windows-sandbox-setup.exe", "codex"));
    }
}
