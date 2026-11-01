//! Files and folders dropped on a pane: the text to type for them, as that pane's shell spells
//! a path. A file that sits in the temp folder (a screenshot dragged from the Snipping Tool, a
//! file dragged out of a zip or a mail) is copied into `%LOCALAPPDATA%\GondCode\drops` first,
//! so it's still there when the agent reads it. Kept a week.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// How long dropped copies are kept.
const KEEP: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// The text for `paths` dropped on a pane running `shell` (a `shell.list` id; none = the
/// Windows default), each followed by a space.
pub fn text(paths: &[PathBuf], shell: Option<&str>, temp: &Path, drops: &Path) -> String {
    prune(drops);
    paths
        .iter()
        .map(|p| format!("{} ", spell(&keep(p, temp, drops), shell)))
        .collect()
}

/// `path`, or a lasting copy of it when it's a file in the temp folder.
fn keep(path: &Path, temp: &Path, drops: &Path) -> PathBuf {
    let in_temp = path
        .to_string_lossy()
        .to_lowercase()
        .starts_with(&temp.to_string_lossy().to_lowercase());
    if !in_temp || !path.is_file() {
        return path.to_path_buf();
    }
    let Some(name) = path.file_name() else {
        return path.to_path_buf();
    };
    let dir = drops.join(ulid::Ulid::generate().to_string().to_lowercase());
    let copy = dir.join(name);
    match std::fs::create_dir_all(&dir).and_then(|()| std::fs::copy(path, &copy)) {
        Ok(_) => copy,
        Err(_) => path.to_path_buf(),
    }
}

/// Drops copied more than a week ago.
fn prune(drops: &Path) {
    let Ok(entries) = std::fs::read_dir(drops) else {
        return;
    };
    let now = SystemTime::now();
    for e in entries.flatten() {
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > KEEP);
        if old {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// `path` as `shell` writes it: `/c/…` in Git Bash, `/mnt/c/…` in WSL, the Windows path
/// elsewhere; quoted when it has to be.
pub fn spell(path: &Path, shell: Option<&str>) -> String {
    let win = path.to_string_lossy().replace('/', "\\");
    let posix = |root: &str| -> Option<String> {
        let mut chars = win.chars();
        let drive = chars.next().filter(char::is_ascii_alphabetic)?;
        if chars.next() != Some(':') {
            return None;
        }
        let rest = win[2..].replace('\\', "/");
        Some(format!("{root}{}{rest}", drive.to_ascii_lowercase()))
    };
    let unix = match shell {
        Some("gitbash") => posix("/"),
        Some(s) if s.starts_with("wsl:") => posix("/mnt/"),
        _ => None,
    };
    match unix {
        Some(p) if p.chars().any(|c| " '\"$`\\!&()[]{};<>|*?#~".contains(c)) => {
            format!("'{}'", p.replace('\'', r"'\''"))
        }
        Some(p) => p,
        None if win.chars().any(|c| " &()[]{}^=;!'+,`~$@#%".contains(c)) => format!("\"{win}\""),
        None => win,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_as_each_shell_spells_them() {
        let p = Path::new(r"C:\Users\Dawson\Pictures\shot 1.png");
        assert_eq!(spell(p, None), r#""C:\Users\Dawson\Pictures\shot 1.png""#);
        assert_eq!(
            spell(p, Some("pwsh")),
            r#""C:\Users\Dawson\Pictures\shot 1.png""#
        );
        assert_eq!(
            spell(Path::new(r"C:\code\gond\src"), Some("cmd")),
            r"C:\code\gond\src"
        );
        assert_eq!(
            spell(p, Some("gitbash")),
            "'/c/Users/Dawson/Pictures/shot 1.png'"
        );
        assert_eq!(
            spell(Path::new(r"D:\it's\a.txt"), Some("wsl:Ubuntu")),
            r"'/mnt/d/it'\''s/a.txt'"
        );
        assert_eq!(
            spell(Path::new(r"C:\code\x.rs"), Some("wsl:Ubuntu")),
            "/mnt/c/code/x.rs"
        );
        // A network path stays a Windows path, even in Git Bash.
        assert_eq!(
            spell(Path::new(r"\\server\share\a b"), Some("gitbash")),
            r#""\\server\share\a b""#
        );
    }

    #[test]
    fn a_file_in_temp_is_kept_and_others_are_not_copied() {
        let temp = tempfile::tempdir().unwrap();
        let drops = tempfile::tempdir().unwrap();
        let shot = temp.path().join("Screenshot.png");
        std::fs::write(&shot, b"png").unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        let mine = elsewhere.path().join("notes.md");
        std::fs::write(&mine, b"md").unwrap();

        let t = text(
            &[shot.clone(), mine.clone()],
            Some("cmd"),
            temp.path(),
            drops.path(),
        );
        let kept = std::fs::read_dir(drops.path())
            .unwrap()
            .flatten()
            .next()
            .unwrap()
            .path()
            .join("Screenshot.png");
        assert!(kept.is_file());
        std::fs::remove_file(&shot).unwrap();
        assert!(kept.is_file(), "the copy outlives the temp file");
        assert!(t.contains(&spell(&kept, Some("cmd"))), "{t}");
        assert!(t.contains(&spell(&mine, Some("cmd"))), "{t}");
        assert!(t.ends_with(' '));
    }
}
