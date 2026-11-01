//! Finds agent CLIs on PATH and reads their versions. Never errors: an agent
//! that can't be found or run is `installed: false`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use super::{AgentInfo, AgentKind, CustomAgent, BUILT_IN, CUSTOM_PROVIDER};

const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

/// The probe's limit. Tests run many `cmd.exe` and shell spawns in parallel, and a loaded
/// Windows runner can take longer than 5 s to start a `.cmd` fake, so they wait longer
/// (the assertion stays exact).
fn version_timeout() -> Duration {
    if cfg!(test) {
        Duration::from_secs(30)
    } else {
        VERSION_TIMEOUT
    }
}

/// Built-ins first (with versions), then custom agents, all probed in parallel.
pub fn detect_all(path_var: Option<OsString>, customs: &[CustomAgent]) -> Vec<AgentInfo> {
    std::thread::scope(|s| {
        let built_in: Vec<_> = BUILT_IN
            .iter()
            .map(|&(id, kind, name, command)| {
                let path_var = path_var.clone();
                s.spawn(move || {
                    let found = find(command, path_var);
                    AgentInfo {
                        id: id.into(),
                        kind,
                        provider: provider(id).into(),
                        name: name.into(),
                        command: command.into(),
                        installed: found.is_some(),
                        version: found.as_deref().and_then(version),
                    }
                })
            })
            .collect();
        let mut out: Vec<AgentInfo> = built_in.into_iter().map(|h| h.join().unwrap()).collect();
        out.extend(customs.iter().map(|c| AgentInfo {
            id: c.id.clone(),
            kind: AgentKind::Custom,
            provider: c.provider.clone().unwrap_or_else(|| CUSTOM_PROVIDER.into()),
            name: c.name.clone(),
            command: c.command.clone(),
            installed: find(&c.command, path_var.clone()).is_some(),
            version: None,
        }));
        out
    })
}

/// A built-in's brand icon key: its id, except agy, which wears Antigravity's mark.
fn provider(id: &str) -> &str {
    match id {
        "agy" => "antigravity",
        other => other,
    }
}

/// Resolves `command` the way a shell would: a path as given, else a lookup on
/// `path_var` (on Windows, `which` tries PATHEXT, so `claude` finds `claude.cmd`).
pub fn find(command: &str, path_var: Option<OsString>) -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    if let Ok(found) = which::which_in(command, path_var, &cwd) {
        return Some(found);
    }
    // Not on PATH: where installers put programs without (yet) adding them, so a
    // winget, npm or vendor install is found before PATH catches up.
    if command.contains(['/', '\\']) {
        return None;
    }
    let dirs = install_dirs();
    if dirs.is_empty() {
        return None;
    }
    which::which_in(command, std::env::join_paths(&dirs).ok(), &cwd)
        .ok()
        .or_else(|| suffixed_in(command, &dirs))
}

/// A winget portable package keeps its program under the release's own name
/// (`codex-x86_64-pc-windows-msvc.exe`) and only links `codex` to it in WinGet\Links.
/// When that link is missing (winget falls back to PATH, or it was removed), the
/// program is still there: `<command>-<anything>.exe` in a package folder.
pub fn suffixed_in(command: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    let prefix = format!("{}-", command.to_lowercase());
    dirs.iter()
        .filter(|d| d.parent().is_some_and(|p| p.ends_with("Packages")))
        .flat_map(|d| std::fs::read_dir(d).into_iter().flatten().flatten())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(str::to_lowercase)
                .is_some_and(|n| {
                    // A Rust target triple, so `codex-windows-sandbox-setup.exe` isn't it.
                    n.starts_with(&prefix) && n.ends_with(".exe") && n.contains("-pc-windows-")
                })
        })
}

/// The folders installers put agent CLIs in, for this user. Empty off Windows.
fn install_dirs() -> Vec<PathBuf> {
    // Tests run with a fake PATH and must not find the runner's own tools (Windows CI has
    // Bun in ~\.bun\bin); the real-install checks opt back in.
    if !cfg!(windows) || (cfg!(test) && std::env::var_os("GOND_REAL_HARNESS").is_none()) {
        return vec![];
    }
    match (dirs::home_dir(), dirs::data_local_dir(), dirs::data_dir()) {
        (Some(home), Some(local), Some(roaming)) => install_dirs_under(&home, &local, &roaming),
        _ => vec![],
    }
}

/// `install_dirs` for a given profile: winget's Links and each package folder (portable
/// installs), npm's global prefix, Bun's, and the vendors' own install folders.
pub fn install_dirs_under(home: &Path, local: &Path, roaming: &Path) -> Vec<PathBuf> {
    let winget = local.join("Microsoft").join("WinGet");
    let mut out = vec![
        winget.join("Links"),
        roaming.join("npm"),
        home.join(".bun").join("bin"),
        home.join(".local").join("bin"),
        local
            .join("Programs")
            .join("OpenAI")
            .join("Codex")
            .join("bin"),
        local.join("agy").join("bin"),
        // Without winget: Git's per-user install, and the unpacked GitHub CLI and Node.
        local.join("Programs").join("Git").join("cmd"),
        local.join("Programs").join("gh").join("bin"),
        local.join("Programs").join("nodejs"),
        home.join(".grok").join("bin"),
        local.join("cursor-agent"),
    ];
    if let Ok(entries) = std::fs::read_dir(winget.join("Packages")) {
        let mut packages: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        packages.sort();
        out.extend(packages);
    }
    out
}

/// Runs `<bin> --version` and pulls out the first version-looking token.
pub fn version(bin: &Path) -> Option<String> {
    let mut cmd = crate::process::command(bin);
    cmd.arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    crate::process::no_window(&mut cmd);
    // One retry: a spawn can fail transiently when the machine is busy.
    let mut child = crate::process::when_free(|| cmd.spawn())
        .or_else(|_| {
            std::thread::sleep(Duration::from_millis(200));
            crate::process::when_free(|| cmd.spawn())
        })
        .ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait().ok()? {
            Some(_) => break,
            None if start.elapsed() > version_timeout() => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let out = child.wait_with_output().ok()?;
    parse_version(&String::from_utf8_lossy(&out.stdout))
}

/// "2.0.14 (Claude Code)" → "2.0.14"; "codex-cli 0.46.0" → "0.46.0";
/// "GitHub Copilot CLI 1.0.105." → "1.0.105".
pub fn parse_version(text: &str) -> Option<String> {
    text.split(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == ',')
        .map(|t| t.trim_start_matches('v').trim_end_matches('.'))
        .find(|t| {
            let mut parts = t.split('.');
            t.contains('.')
                && parts
                    .next()
                    .is_some_and(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        })
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Codex from winget: no `codex` link, but the package folder has the release's exe.
    #[test]
    fn finds_a_portable_package_by_its_release_name() {
        let root = tempfile::tempdir().unwrap();
        let (home, local, roaming) = (root.path(), root.path().join("L"), root.path().join("R"));
        let pkg = local
            .join("Microsoft")
            .join("WinGet")
            .join("Packages")
            .join("OpenAI.Codex_Microsoft.Winget.Source_8wekyb3d8bbwe");
        std::fs::create_dir_all(&pkg).unwrap();
        for f in [
            "codex-command-runner.exe",
            "codex-windows-sandbox-setup.exe",
        ] {
            std::fs::write(pkg.join(f), "").unwrap();
        }
        let dirs = install_dirs_under(home, &local, &roaming);
        assert_eq!(suffixed_in("codex", &dirs), None, "helpers aren't codex");
        std::fs::write(pkg.join("codex-x86_64-pc-windows-msvc.exe"), "").unwrap();
        assert_eq!(
            suffixed_in("codex", &dirs),
            Some(pkg.join("codex-x86_64-pc-windows-msvc.exe"))
        );
        assert_eq!(suffixed_in("claude", &dirs), None);
    }

    #[test]
    fn parses_versions() {
        assert_eq!(
            parse_version("2.0.14 (Claude Code)\n").as_deref(),
            Some("2.0.14")
        );
        assert_eq!(parse_version("codex-cli 0.46.0").as_deref(), Some("0.46.0"));
        assert_eq!(parse_version("tool v1.2-beta").as_deref(), Some("1.2-beta"));
        assert_eq!(parse_version("no version here"), None);
        // Copilot ends its line with a full stop; Cursor's version is a date and a commit.
        assert_eq!(
            parse_version(
                "GitHub Copilot CLI 1.0.105.
Run 'copilot update' to check for updates."
            )
            .as_deref(),
            Some("1.0.105")
        );
        assert_eq!(
            parse_version(
                "2026.10.22-27dcf5f
"
            )
            .as_deref(),
            Some("2026.10.22-27dcf5f")
        );
    }

    /// A fake `name` on a temp PATH that prints `output` for --version.
    fn fake_bin(dir: &Path, name: &str, output: &str) {
        #[cfg(windows)]
        std::fs::write(
            dir.join(format!("{name}.cmd")),
            format!("@echo {output}\r\n"),
        )
        .unwrap();
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            let p = dir.join(name);
            std::fs::write(&p, format!("#!/bin/sh\necho '{output}'\n")).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[test]
    fn detects_fake_claude_and_missing_codex() {
        let dir = tempfile::tempdir().unwrap();
        fake_bin(dir.path(), "claude", "2.0.14 (Claude Code)");
        fake_bin(dir.path(), "mytool", "mytool 9.9.9");
        let customs = [
            CustomAgent {
                id: "c1".into(),
                name: "Mine".into(),
                command: "mytool".into(),
                args: vec![],
                provider: Some("deepseek".into()),
                prompt_via: Default::default(),
            },
            CustomAgent {
                id: "c2".into(),
                name: "Gone".into(),
                command: "nope".into(),
                args: vec![],
                provider: None,
                prompt_via: Default::default(),
            },
        ];
        let found = detect_all(Some(dir.path().as_os_str().to_owned()), &customs);
        let by_id = |id: &str| found.iter().find(|a| a.id == id).unwrap();

        assert!(by_id("claude").installed);
        assert_eq!(by_id("claude").provider, "claude");
        assert_eq!(by_id("codex").provider, "codex");
        assert_eq!(by_id("c1").provider, "deepseek");
        assert_eq!(by_id("c2").provider, "custom");
        assert_eq!(by_id("claude").version.as_deref(), Some("2.0.14"));
        assert!(!by_id("codex").installed);
        assert_eq!(by_id("codex").version, None);
        assert!(by_id("c1").installed && by_id("c1").kind == AgentKind::Custom);
        assert!(!by_id("c2").installed);
        assert_eq!(by_id("agy").provider, "antigravity");
        assert!(!by_id("agy").installed);
        assert_eq!(by_id("grok").provider, "grok");
        assert_eq!(by_id("opencode").kind, AgentKind::Opencode);
        assert_eq!(by_id("pi").kind, AgentKind::Pi);
        assert_eq!(by_id("copilot").provider, "copilot");
        assert_eq!(by_id("cursor").command, "cursor-agent");
        assert_eq!(found.len(), 10);
    }

    #[test]
    fn install_dirs_cover_winget_npm_and_vendors() {
        let root = tempfile::tempdir().unwrap();
        let (home, local, roaming) = (
            root.path().join("u"),
            root.path().join("l"),
            root.path().join("r"),
        );
        let pkg = local.join("Microsoft/WinGet/Packages/xAI.GrokBuild_winget");
        std::fs::create_dir_all(&pkg).unwrap();
        let dirs = install_dirs_under(&home, &local, &roaming);
        for want in [
            local.join("Microsoft").join("WinGet").join("Links"),
            roaming.join("npm"),
            home.join(".local").join("bin"),
            pkg.clone(),
        ] {
            assert!(dirs.contains(&want), "{want:?} in {dirs:?}");
        }
    }
}
