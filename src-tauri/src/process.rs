//! Shared helpers for spawning child processes.

use std::ffi::{OsStr, OsString};
use std::process::Command;
use std::sync::RwLock;

/// What a Claude Code session puts in its children's environment. Gond started from inside
/// one (a terminal in Claude Code) would hand them on to every agent it launches, and Claude
/// then takes itself for that session's child: it stops saving transcripts, so nothing can be
/// read back into the chat, and talks to the other session's socket.
const PARENT_AGENT_VARS: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_BRIDGE_SESSION_ID",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_PID",
    "CLAUDE_EFFORT",
];

/// The variables to leave out of every child's environment ([`command`], terminals).
pub fn parent_agent_vars() -> &'static [&'static str] {
    PARENT_AGENT_VARS
}

/// The one way to start a child process: a `Command` that never opens a console window
/// on Windows. `scripts/check-no-console.sh` fails CI on any other `Command::new`.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut cmd = Command::new(program);
    no_window(&mut cmd);
    for var in PARENT_AGENT_VARS {
        cmd.env_remove(var);
    }
    cmd
}

/// Keeps a console window from flashing up on Windows. No-op elsewhere.
pub fn no_window(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// The PATH every child is spawned with, refreshed after installs so new
/// tools are found without restarting. `None` until set: the process's own PATH.
static PATH: RwLock<Option<OsString>> = RwLock::new(None);

/// The PATH to spawn children with. Read it on every spawn, never once at startup.
pub fn path() -> Option<OsString> {
    PATH.read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .or_else(|| std::env::var_os("PATH"))
}

/// The PATH for a new terminal or agent: the registry's as it is now (what a program started
/// from Explorer gets), then anything else Gond's own PATH has. Gond's own is whatever launched
/// it handed down, kept across restarts into updates, so it can be days out of date.
pub fn fresh_path() -> Option<OsString> {
    match crate::pathenv::registry_path() {
        Some(fresh) => Some(merge_paths(OsStr::new(&fresh), path().as_deref())),
        None => path(),
    }
}

pub fn set_path(path: OsString) {
    *PATH.write().unwrap_or_else(|e| e.into_inner()) = Some(path);
}

/// `fresh` entries first (the registry's Machine + User PATH), then any inherited entry
/// not already there, so nothing the app was started with is lost.
pub fn merge_paths(fresh: &OsStr, inherited: Option<&OsStr>) -> OsString {
    let key = |p: &std::path::Path| {
        let s = p
            .to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .to_string();
        if cfg!(windows) {
            s.to_lowercase()
        } else {
            s
        }
    };
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    let all =
        std::env::split_paths(fresh).chain(inherited.into_iter().flat_map(std::env::split_paths));
    for p in all {
        if !p.as_os_str().is_empty() && seen.insert(key(&p)) {
            out.push(p);
        }
    }
    std::env::join_paths(out).unwrap_or_else(|_| fresh.to_owned())
}

/// Runs a spawn, retrying briefly while the program is "text file busy": on Unix a
/// program written a moment ago can still be open in a process another thread is
/// forking (the Linux dev path and parallel tests hit this; Windows doesn't).
pub fn when_free<T>(mut spawn: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    const ETXTBSY: i32 = 26;
    let mut tries = 0;
    loop {
        match spawn() {
            Err(e) if cfg!(unix) && e.raw_os_error() == Some(ETXTBSY) && tries < 20 => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            other => return other,
        }
    }
}

/// Readies a child for its own tree: its own process group on Unix; on Windows
/// it starts suspended (and windowless) so `proctree::contain` can put it in its job
/// before it runs a single instruction. Always follow the spawn with `contain`,
/// which resumes it.
pub(crate) fn new_process_group(cmd: &mut std::process::Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const CREATE_SUSPENDED: u32 = 0x0000_0004;
        cmd.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
    }
    #[cfg(not(any(unix, windows)))]
    let _ = cmd;
}

/// Kills `pid` and its descendants: its job object on Windows, its process group on
/// Unix. `pid` must have been put in a tree with `proctree::contain`.
pub(crate) fn kill_tree(pid: u32) {
    crate::proctree::kill_tree(pid);
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn a_busy_program_is_retried_other_errors_are_not() {
        let busy = || std::io::Error::from_raw_os_error(26);
        let mut n = 0;
        assert_eq!(
            when_free(|| {
                n += 1;
                if n < 3 {
                    Err(busy())
                } else {
                    Ok(n)
                }
            })
            .unwrap(),
            3
        );
        let mut m = 0;
        let e = when_free(|| -> std::io::Result<()> {
            m += 1;
            Err(std::io::Error::from(std::io::ErrorKind::NotFound))
        });
        assert!(e.is_err() && m == 1);
    }

    #[test]
    fn merges_fresh_first_without_duplicates() {
        let sep = if cfg!(windows) { ";" } else { ":" };
        let fresh = OsString::from(["/new/bin", "/usr/bin"].join(sep));
        let old = OsString::from(["/usr/bin/", "/app/only", ""].join(sep));
        let merged = merge_paths(&fresh, Some(&old));
        let parts: Vec<_> = std::env::split_paths(&merged).collect();
        assert_eq!(
            parts,
            ["/new/bin", "/usr/bin", "/app/only"].map(std::path::PathBuf::from)
        );
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    extern "system" {
        fn FreeConsole() -> i32;
    }

    /// What the child sees as its console window: 0 means none (CREATE_NO_WINDOW worked).
    fn console_window_of(mut cmd: Command) -> u64 {
        let out = cmd
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Add-Type -Name K -Namespace W -MemberDefinition '[DllImport(\"kernel32.dll\")] public static extern System.IntPtr GetConsoleWindow();'; [W.K]::GetConsoleWindow().ToInt64()",
            ])
            .output()
            .expect("powershell runs");
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .expect("a handle")
    }

    /// One test, in order: the app is a GUI process with no console, so the test lets go of
    /// its own (the CI runner's has no window either, which would hide the difference). Then a
    /// child started without the flag must get a window of its own, proving the probe can see
    /// one, and a child from `command` must not.
    #[test]
    fn command_never_gets_a_console_window() {
        unsafe { FreeConsole() };
        assert_ne!(
            console_window_of(Command::new("powershell")),
            0,
            "a deliberately windowed child should get a console window"
        );
        assert_eq!(console_window_of(command("powershell")), 0);
    }
}
