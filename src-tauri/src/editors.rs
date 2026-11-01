//! `file:line` in a pane opens in your editor: which editors are on this PC, what a path printed
//! in a pane points at, and opening a file at its line (Settings → General → Open files in).

use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::error::{AppError, AppResult};

/// How an editor takes a file at a line.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Goto {
    /// `code -g file:line:col` (VS Code and its forks).
    Flag,
    /// `zed file:line:col`.
    Suffix,
}

/// The editors Gond looks for, in the order it prefers them: (id, name, command, how).
const KNOWN: [(&str, &str, &str, Goto); 4] = [
    ("code", "VS Code", "code", Goto::Flag),
    ("cursor", "Cursor", "cursor", Goto::Flag),
    ("windsurf", "Windsurf", "windsurf", Goto::Flag),
    ("zed", "Zed", "zed", Goto::Suffix),
];

/// The Settings value for "open in whatever Windows opens the file with".
pub const SYSTEM: &str = "system";

#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Editor {
    pub id: String,
    pub label: String,
}

/// An editor's command: on PATH, else where its installer puts it (VS Code and Cursor often
/// aren't on PATH).
fn find(command: &str) -> Option<PathBuf> {
    crate::agents::detect::find(command, crate::process::path()).or_else(|| {
        let local = dirs::data_local_dir()?.join("Programs");
        let program_files = std::env::var_os("ProgramFiles").map(PathBuf::from);
        let places: Vec<PathBuf> = match command {
            "code" => vec![
                local.join(r"Microsoft VS Code\bin\code.cmd"),
                program_files?.join(r"Microsoft VS Code\bin\code.cmd"),
            ],
            "cursor" => vec![local.join(r"cursor\resources\app\bin\cursor.cmd")],
            "windsurf" => vec![local.join(r"Windsurf\bin\windsurf.cmd")],
            "zed" => vec![local.join(r"Zed\bin\zed.exe")],
            _ => vec![],
        };
        places.into_iter().find(|p| cfg!(windows) && p.is_file())
    })
}

/// The editors on this PC.
pub fn list() -> Vec<Editor> {
    KNOWN
        .iter()
        .filter(|(_, _, cmd, _)| find(cmd).is_some())
        .map(|(id, label, _, _)| Editor {
            id: id.to_string(),
            label: label.to_string(),
        })
        .collect()
}

/// Opens `path` at `line` (and `column`) in `choice` (an editor id, `system`, or none for the
/// first editor found); in the default app when there's no editor.
pub fn open(
    path: &Path,
    line: Option<u32>,
    column: Option<u32>,
    choice: Option<&str>,
) -> AppResult<()> {
    if !path.is_file() {
        return Err(AppError::NotFound(format!(
            "{} isn't a file",
            path.display()
        )));
    }
    let editor = match choice {
        Some(SYSTEM) => None,
        Some(id) => KNOWN
            .iter()
            .find(|k| k.0 == id)
            .and_then(|k| find(k.2).map(|p| (p, k.3)))
            .or_else(first),
        None => first(),
    };
    let Some((program, goto)) = editor else {
        return tauri_plugin_opener::open_path(path, None::<&str>)
            .map_err(|e| AppError::Invalid(e.to_string()));
    };
    let at = target(path, line, column);
    let mut cmd = crate::process::command(program);
    match goto {
        Goto::Flag => cmd.args(["-g", &at]),
        Goto::Suffix => cmd.arg(&at),
    };
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| AppError::Invalid(format!("couldn't start the editor: {e}")))
}

fn first() -> Option<(PathBuf, Goto)> {
    KNOWN.iter().find_map(|k| find(k.2).map(|p| (p, k.3)))
}

/// `file:line:col` as the editors take it.
fn target(path: &Path, line: Option<u32>, column: Option<u32>) -> String {
    match (line, column) {
        (Some(l), Some(c)) => format!("{}:{l}:{c}", path.display()),
        (Some(l), None) => format!("{}:{l}", path.display()),
        _ => path.display().to_string(),
    }
}

/// The file a path printed in a pane points at, if it exists: relative to the pane's folder
/// (else your home folder); `~` is your home folder; Git Bash's `/c/…` and WSL's `/mnt/c/…`
/// are the Windows drive.
pub fn resolve(raw: &str, cwd: Option<&Path>, home: Option<&Path>) -> Option<PathBuf> {
    let raw = raw.trim().trim_matches(['"', '\'']);
    if raw.is_empty() {
        return None;
    }
    let path = if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        home?.join(rest)
    } else if let Some(win) = cfg!(windows).then(|| drive(raw)).flatten() {
        win
    } else {
        let p = PathBuf::from(raw);
        if p.is_absolute() {
            p
        } else {
            cwd.or(home)?.join(p)
        }
    };
    let path = tidy(&path);
    path.is_file().then_some(path)
}

/// `/c/x` or `/mnt/c/x` as `C:\x`.
fn drive(raw: &str) -> Option<PathBuf> {
    let rest = raw
        .strip_prefix("/mnt/")
        .or_else(|| raw.strip_prefix('/'))?;
    let mut chars = rest.chars();
    let letter = chars.next().filter(char::is_ascii_alphabetic)?;
    let after = chars.as_str();
    if !(after.is_empty() || after.starts_with('/')) {
        return None;
    }
    Some(PathBuf::from(format!(
        "{}:\\{}",
        letter.to_ascii_uppercase(),
        after.trim_start_matches('/').replace('/', "\\")
    )))
}

/// `a/./b/../c` as `a/c`, without touching the disk.
fn tidy(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_printed_path_resolves_against_the_panes_folder() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src/pty")).unwrap();
        std::fs::write(dir.path().join("src/pty/mod.rs"), "").unwrap();
        let file = dir.path().join("src").join("pty").join("mod.rs");
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join("notes.md"), "").unwrap();

        let here = Some(dir.path());
        assert_eq!(resolve("src/pty/mod.rs", here, None), Some(tidy(&file)));
        assert_eq!(
            resolve("./src/../src/pty/mod.rs", here, None),
            Some(tidy(&file))
        );
        assert_eq!(
            resolve(&file.to_string_lossy(), None, None),
            Some(tidy(&file))
        );
        assert_eq!(resolve("src/pty/nope.rs", here, None), None);
        assert_eq!(
            resolve("src/pty", here, None),
            None,
            "a folder isn't a file"
        );
        assert!(resolve("~/notes.md", here, Some(home.path())).is_some());
        assert!(resolve("notes.md", None, Some(home.path())).is_some());
    }

    #[test]
    fn git_bash_and_wsl_paths_are_the_windows_drive() {
        assert_eq!(
            drive("/c/Users/x/a.rs"),
            Some(PathBuf::from(r"C:\Users\x\a.rs"))
        );
        assert_eq!(
            drive("/mnt/d/code/b.ts"),
            Some(PathBuf::from(r"D:\code\b.ts"))
        );
        assert_eq!(drive("/usr/bin/env"), None);
        assert_eq!(drive("relative/x"), None);
    }

    #[test]
    fn the_editor_gets_file_line_and_column() {
        let p = Path::new("a.rs");
        assert_eq!(target(p, Some(12), Some(5)), "a.rs:12:5");
        assert_eq!(target(p, Some(12), None), "a.rs:12");
        assert_eq!(target(p, None, None), "a.rs");
    }
}
