//! Directory-only targets for File Explorer, independent of registered projects.

use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

fn expand(raw: &str, home: Option<&Path>, windows: bool) -> AppResult<PathBuf> {
    if raw == "~" {
        return home
            .map(Path::to_path_buf)
            .ok_or_else(|| AppError::Invalid("no home folder".into()));
    }
    if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        return home
            .map(|h| h.join(rest))
            .ok_or_else(|| AppError::Invalid("no home folder".into()));
    }
    if windows {
        let rest = raw.strip_prefix("/mnt/").or_else(|| raw.strip_prefix('/'));
        if let Some(rest) = rest {
            let mut chars = rest.chars();
            if let Some(drive) = chars.next().filter(char::is_ascii_alphabetic) {
                let tail = chars.as_str();
                if tail.is_empty() || tail.starts_with('/') {
                    return Ok(format!(
                        "{}:\\{}",
                        drive.to_ascii_uppercase(),
                        tail.trim_start_matches('/').replace('/', "\\")
                    )
                    .into());
                }
            }
        }
    }
    Ok(raw.into())
}

pub fn directory(raw: &str, home: Option<&Path>) -> AppResult<PathBuf> {
    let path = expand(raw, home, cfg!(windows))?;
    if !path.is_absolute() || !path.is_dir() {
        return Err(AppError::Invalid(
            "folder must be an existing directory".into(),
        ));
    }
    // Resolve before opening: a file must never reach the OS's default-app launcher.
    let path = std::fs::canonicalize(path).map(crate::git::dunce_path)?;
    if !path.is_dir() {
        return Err(AppError::Invalid(
            "folder must be an existing directory".into(),
        ));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_plain_directories_and_home_but_never_files_or_unresolved_ids() {
        let dir = tempfile::tempdir().unwrap();
        let child = dir.path().join("plain folder");
        std::fs::create_dir(&child).unwrap();
        let file = dir.path().join("program.exe");
        std::fs::write(&file, b"not a directory").unwrap();
        let expected = std::fs::canonicalize(&child)
            .map(crate::git::dunce_path)
            .unwrap();
        assert_eq!(directory(child.to_str().unwrap(), None).unwrap(), expected);
        assert_eq!(
            directory("~/plain folder", Some(dir.path())).unwrap(),
            expected
        );
        assert_eq!(directory("~", Some(&child)).unwrap(), expected);
        assert_eq!(
            directory(&format!("{}/", child.display()), None).unwrap(),
            expected
        );
        for raw in [
            "",
            "removed-project",
            "~",
            file.to_str().unwrap(),
            dir.path().join("missing").to_str().unwrap(),
        ] {
            assert_eq!(directory(raw, None).unwrap_err().code(), "invalid");
        }
    }

    #[test]
    fn preserves_windows_paths_and_converts_git_bash_drives() {
        for (raw, expected) in [
            (r"C:\Users\x", r"C:\Users\x"),
            ("c:/Users/x/", "c:/Users/x/"),
            ("/c/Users/x", r"C:\Users\x"),
            ("/mnt/d/src", r"D:\src"),
            ("/c/", "C:\\"),
            (r"\\server\share\", r"\\server\share\"),
            (r"\\wsl$\Ubuntu\home\x", r"\\wsl$\Ubuntu\home\x"),
        ] {
            assert_eq!(expand(raw, None, true).unwrap(), PathBuf::from(expected));
        }
    }
}

#[cfg(test)]
mod command_tests {
    use crate::api::{registry, Actor, Services};
    use serde_json::json;

    #[test]
    fn folder_command_rejects_files_and_old_project_input_before_opening() {
        let (registry, services, human) = (registry(), Services::default(), Actor::human());
        let file = tempfile::NamedTempFile::new().unwrap();
        for (input, code) in [
            (json!({ "path": file.path() }), "invalid"),
            (json!({ "path": "" }), "invalid"),
            (json!({ "projectId": "" }), "invalid_input"),
        ] {
            assert_eq!(
                registry
                    .run(&services, &human, "folder.open", input)
                    .unwrap_err()
                    .code(),
                code
            );
        }
    }
}
