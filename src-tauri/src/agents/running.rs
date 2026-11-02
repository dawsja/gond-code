//! Which agent you started in a terminal: every pane is a shell, and typing `claude` or
//! `codex` there runs the agent's CLI under it. Found from the shell's process tree, by the
//! program's name, so the pane can show the agent and watch it (Working, Needs you, Done).
//! An agent run through a script runtime is recognised when the runtime runs from the agent's
//! own install folder (Cursor's `cursor-agent` is `…\cursor-agent\versions\…\node.exe`); a
//! plain `node.exe` from an npm install isn't.

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

/// Whether `p` is a script runtime (`node.exe`, `bun.exe`) running from agent `command`'s own
/// install folder: a folder named for the command on the runtime's path.
fn runs_from_install(p: &Proc, command: &str) -> bool {
    let exe = p.exe.to_lowercase();
    if !matches!(exe.as_str(), "node.exe" | "bun.exe" | "node" | "bun") {
        return false;
    }
    let name = std::path::Path::new(command)
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if name.is_empty() {
        return false;
    }
    let marks = markers(&name);
    let marked = |text: &str| {
        text.to_lowercase()
            .split(['\\', '/'])
            .any(|segment| segment == name || marks.iter().any(|m| m == segment))
    };
    if image_of(p.pid).is_some_and(|path| marked(&path.to_string_lossy())) {
        return true;
    }
    // This PC's own Node running the agent's script: its arguments say whose.
    !marks.is_empty() && super::cwd::args(p.pid).is_some_and(|args| args.iter().any(|a| marked(a)))
}

/// Folders that mark an agent whose command isn't its folder's name (its file's
/// `detect.markers`): MiniMax Code's `mcode` is Node running
/// `…\.minimax-code\releases\…\@minimax-ai\code\cli.js`.
fn markers(command: &str) -> &'static [String] {
    super::spec::all()
        .iter()
        .find(|s| s.command.eq_ignore_ascii_case(command))
        .map(|s| s.detect.markers.as_slice())
        .unwrap_or(&[])
}

/// Every running process whose program is `exe` (`cline.exe`), with its full path.
pub fn programs_named(exe: &str) -> Vec<(u32, std::path::PathBuf)> {
    snapshot()
        .into_iter()
        .filter(|p| p.exe.eq_ignore_ascii_case(exe))
        .filter_map(|p| image_of(p.pid).map(|i| (p.pid, i)))
        .collect()
}

/// The full path of process `pid`'s program.
fn image_of(pid: u32) -> Option<std::path::PathBuf> {
    #[cfg(windows)]
    return crate::pty::host::win::image_of(pid);
    #[cfg(target_os = "linux")]
    return std::fs::read_link(format!("/proc/{pid}/exe")).ok();
    #[allow(unreachable_code)]
    {
        let _ = pid;
        None
    }
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
            if let Some((id, _)) = agents
                .iter()
                .find(|(_, c)| is(&p.exe, c) || runs_from_install(p, c))
            {
                return Some((id.clone(), p.pid));
            }
            next.push(p.pid);
        }
        level = next;
    }
    None
}

/// Every process now. On Linux (WSL, an SSH machine) from `/proc`; empty elsewhere.
pub fn snapshot() -> Vec<Proc> {
    #[cfg(windows)]
    {
        win::snapshot()
    }
    #[cfg(target_os = "linux")]
    {
        linux::snapshot()
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        vec![]
    }
}

/// Script runtimes an agent CLI runs under on Linux (`node /usr/bin/claude`, a shell script).
const RUNTIMES: &[&str] = &[
    "node", "bun", "deno", "python", "python3", "sh", "bash", "dash", "zsh",
];

/// The name a Linux process goes by, from its command line: the program it was started as
/// (`claude`, a symlink to a versioned binary, is `claude`), or for a script runtime the
/// script it runs (`node /home/me/.npm/bin/pi` is `pi`). `comm` when there's no command line.
pub fn linux_name(cmdline: &[String], comm: &str) -> String {
    let base = |s: &str| {
        std::path::Path::new(s)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let Some(first) = cmdline.first().map(|a| base(a)).filter(|a| !a.is_empty()) else {
        return comm.to_string();
    };
    // `sh -c "…"` runs a command line, not a script named for an agent.
    if RUNTIMES.contains(&first.as_str()) && !cmdline.iter().any(|a| a == "-c") {
        if let Some(script) = cmdline.iter().skip(1).find(|a| !a.starts_with('-')) {
            let name = base(script);
            let stem = name
                .strip_suffix(".js")
                .or_else(|| name.strip_suffix(".mjs"))
                .or_else(|| name.strip_suffix(".py"))
                .unwrap_or(&name);
            if !stem.is_empty() && !matches!(stem, "cli" | "index" | "main") {
                return stem.to_string();
            }
        }
    }
    first
}

#[cfg(target_os = "linux")]
mod linux {
    use super::Proc;

    pub fn snapshot() -> Vec<Proc> {
        let Ok(dir) = std::fs::read_dir("/proc") else {
            return vec![];
        };
        dir.flatten()
            .filter_map(|e| {
                let pid: u32 = e.file_name().to_str()?.parse().ok()?;
                let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
                let (comm, parent) = parse_stat(&stat)?;
                let cmdline: Vec<String> = std::fs::read(format!("/proc/{pid}/cmdline"))
                    .map(|b| {
                        b.split(|&c| c == 0)
                            .filter(|a| !a.is_empty())
                            .map(|a| String::from_utf8_lossy(a).into_owned())
                            .collect()
                    })
                    .unwrap_or_default();
                Some(Proc {
                    pid,
                    parent,
                    exe: super::linux_name(&cmdline, &comm),
                })
            })
            .collect()
    }

    /// `(comm, ppid)` from `/proc/<pid>/stat`: `pid (comm) state ppid …`, where comm may hold
    /// spaces and parentheses itself.
    pub fn parse_stat(stat: &str) -> Option<(String, u32)> {
        let open = stat.find('(')?;
        let close = stat.rfind(')')?;
        let comm = stat.get(open + 1..close)?.to_string();
        let mut rest = stat.get(close + 1..)?.split_whitespace();
        let _state = rest.next()?;
        Some((comm, rest.next()?.parse().ok()?))
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn stat_lines_with_odd_names_parse() {
            assert_eq!(
                super::parse_stat("42 (a (b) c) S 7 42 42 0"),
                Some(("a (b) c".into(), 7))
            );
        }

        /// This very test process is in the snapshot, under its parent.
        #[test]
        fn the_snapshot_holds_this_process() {
            let me = std::process::id();
            let procs = super::snapshot();
            let p = procs.iter().find(|p| p.pid == me).expect("this process");
            assert_eq!(p.parent, std::os::unix::process::parent_id());
        }
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

    /// On Linux an agent is named by how it was started: a symlinked binary by the link, an
    /// npm one (`node …/bin/pi`) by its script, never by the runtime.
    #[test]
    fn linux_processes_are_named_as_they_were_started() {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(linux_name(&v(&["claude"]), "2.1.4"), "claude");
        assert_eq!(
            linux_name(&v(&["/usr/bin/node", "/home/me/.npm/bin/pi"]), "node"),
            "pi"
        );
        assert_eq!(
            linux_name(&v(&["node", "--no-warnings", "/usr/bin/codex"]), "node"),
            "codex"
        );
        assert_eq!(linux_name(&v(&["node", "/opt/app/cli.js"]), "node"), "node");
        assert_eq!(linux_name(&v(&["-bash"]), "bash"), "-bash");
        assert_eq!(linux_name(&v(&["bash", "-l"]), "bash"), "bash");
        assert_eq!(
            linux_name(&v(&["/bin/sh", "/home/me/bin/grok"]), "sh"),
            "grok"
        );
        assert_eq!(linux_name(&v(&["sh", "-c", "claude"]), "sh"), "sh");
        assert_eq!(linux_name(&[], "kworker/0:1"), "kworker/0:1");
        let procs = [p(10, 1, "bash"), p(11, 10, "pi")];
        assert_eq!(
            agent_under(10, &procs, &[("pi".into(), "pi".into())]).as_deref(),
            Some("pi")
        );
    }
}
