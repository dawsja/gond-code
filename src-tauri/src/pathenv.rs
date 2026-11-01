//! `gond-code path add|remove <dir>` (packaging): puts the CLI's folder on the
//! user's PATH, or takes it off, through the registry API in Rust. NSIS strings stop
//! at NSIS_MAX_STRLEN and would cut a long PATH; this has no such limit. The
//! installer hooks run it; it never opens a window.

/// Whether two PATH entries name the same folder (case-insensitive, trailing `\` or `/`
/// ignored, `%VAR%` expanded: installers like agy's write `%LOCALAPPDATA%\agy\bin`).
fn same_dir(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        expand(s.trim())
            .trim_end_matches(['\\', '/'])
            .to_lowercase()
    };
    !a.trim().is_empty() && norm(a) == norm(b)
}

/// `%NAME%` replaced by the variable's value; unknown names stay as written.
fn expand(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => {
                let name = &after[..end];
                match std::env::var(name) {
                    Ok(v) => out.push_str(&v),
                    Err(_) => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `path` with `dir` appended, or `None` when it's already there.
pub fn with_entry(path: &str, dir: &str) -> Option<String> {
    if path.split(';').any(|e| same_dir(e, dir)) {
        return None;
    }
    let trimmed = path.trim_end_matches(';');
    Some(if trimmed.is_empty() {
        dir.to_string()
    } else {
        format!("{trimmed};{dir}")
    })
}

/// `path` without any entry for `dir`, or `None` when it had none. Other entries and
/// their order are kept exactly.
pub fn without_entry(path: &str, dir: &str) -> Option<String> {
    let entries: Vec<&str> = path.split(';').collect();
    if !entries.iter().any(|e| same_dir(e, dir)) {
        return None;
    }
    Some(
        entries
            .into_iter()
            .filter(|e| !same_dir(e, dir))
            .collect::<Vec<_>>()
            .join(";"),
    )
}

#[cfg(windows)]
mod reg {
    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_EXPAND_SZ, REG_SZ, RRF_NOEXPAND,
        RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// HKCU\Environment\Path as stored (unexpanded) and its type; empty if unset.
    pub fn read() -> Result<(String, u32), String> {
        let (key, name) = (wide("Environment"), wide("Path"));
        let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;
        let (mut kind, mut bytes) = (0u32, 0u32);
        let rc = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                flags,
                &mut kind,
                std::ptr::null_mut(),
                &mut bytes,
            )
        };
        if rc == ERROR_FILE_NOT_FOUND {
            return Ok((String::new(), REG_EXPAND_SZ));
        }
        if rc != ERROR_SUCCESS {
            return Err(format!("reading the user PATH failed ({rc})"));
        }
        let mut buf = vec![0u16; (bytes as usize).div_ceil(2) + 1];
        let rc = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                flags,
                &mut kind,
                buf.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        if rc != ERROR_SUCCESS {
            return Err(format!("reading the user PATH failed ({rc})"));
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Ok((
            String::from_utf16_lossy(&buf[..len]),
            if kind == REG_SZ {
                REG_SZ
            } else {
                REG_EXPAND_SZ
            },
        ))
    }

    /// Writes PATH back with its original type (REG_EXPAND_SZ by default) and tells
    /// Explorer and new shells.
    pub fn write(path: &str, kind: u32) -> Result<(), String> {
        let (key, name, data) = (wide("Environment"), wide("Path"), wide(path));
        let rc = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                kind,
                data.as_ptr().cast(),
                (data.len() * 2) as u32,
            )
        };
        if rc != ERROR_SUCCESS {
            return Err(format!("writing the user PATH failed ({rc})"));
        }
        let env = wide("Environment");
        let mut result = 0usize;
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                env.as_ptr() as isize,
                SMTO_ABORTIFHUNG,
                5000,
                &mut result,
            )
        };
        Ok(())
    }
}

/// Adds `dir` to the user's PATH (HKCU) if it isn't there, and tells new shells.
/// Used after an installer that puts its program in a folder it doesn't add.
pub fn add_user_dir(dir: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        let (path, kind) = reg::read()?;
        match with_entry(&path, dir) {
            Some(p) => reg::write(&p, kind),
            None => Ok(()),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = dir;
        Ok(())
    }
}

/// Takes `dir` off the user's PATH (HKCU) if it's there, and tells new shells.
pub fn remove_user_dir(dir: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        let (path, kind) = reg::read()?;
        match without_entry(&path, dir) {
            Some(p) => reg::write(&p, kind),
            None => Ok(()),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = dir;
        Ok(())
    }
}

/// The user's PATH entries as stored (HKCU), empty off Windows.
pub fn user_entries() -> Vec<String> {
    #[cfg(windows)]
    {
        reg::read()
            .map(|(p, _)| {
                p.split(';')
                    .filter(|e| !e.trim().is_empty())
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default()
    }
    #[cfg(not(windows))]
    {
        vec![]
    }
}

/// Whether `entry` names `dir`, as Windows compares PATH entries.
pub fn same_entry(entry: &str, dir: &str) -> bool {
    same_dir(entry, dir)
}

/// A PATH entry with its `%VAR%`s expanded.
pub fn expanded(entry: &str) -> String {
    expand(entry)
}

/// `gond-code path add|remove <dir>`. Returns the exit code.
pub fn run(args: &[String]) -> i32 {
    let (Some(action), Some(dir)) = (args.first(), args.get(1)) else {
        eprintln!("usage: gond-code path add|remove <folder>");
        return 2;
    };
    if action != "add" && action != "remove" {
        eprintln!("usage: gond-code path add|remove <folder>");
        return 2;
    }
    #[cfg(windows)]
    {
        let outcome = reg::read().and_then(|(path, kind)| {
            let next = if action == "add" {
                with_entry(&path, dir)
            } else {
                without_entry(&path, dir)
            };
            match next {
                Some(p) => reg::write(&p, kind),
                None => Ok(()),
            }
        });
        match outcome {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("gond-code path: {e}");
                1
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = dir;
        eprintln!("gond-code path: only on Windows");
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BIN: &str = r"C:\Users\d\AppData\Local\Gond Code\bin";

    #[test]
    fn adds_once_and_keeps_everything_else() {
        assert_eq!(with_entry("", BIN).as_deref(), Some(BIN));
        assert_eq!(
            with_entry(r"C:\a;%USERPROFILE%\b", BIN).unwrap(),
            format!(r"C:\a;%USERPROFILE%\b;{BIN}")
        );
        assert_eq!(with_entry(r"C:\a;", BIN).unwrap(), format!(r"C:\a;{BIN}"));
        assert_eq!(
            with_entry(&format!(r"C:\a;{}\", BIN.to_uppercase()), BIN),
            None,
            "already there, any case or slash"
        );
        let long = vec![r"C:\some\very\long\tool\path"; 400].join(";");
        assert_eq!(
            with_entry(&long, BIN).unwrap().len(),
            long.len() + 1 + BIN.len(),
            "no length limit"
        );
    }

    #[test]
    fn removes_every_copy_and_nothing_else() {
        assert_eq!(without_entry(r"C:\a", BIN), None);
        assert_eq!(without_entry(BIN, BIN).as_deref(), Some(""));
        assert_eq!(
            without_entry(&format!(r"{BIN};C:\a"), BIN).as_deref(),
            Some(r"C:\a")
        );
        assert_eq!(
            without_entry(&format!(r"C:\a;{BIN};%X%\b;{BIN}\"), BIN).as_deref(),
            Some(r"C:\a;%X%\b")
        );
        assert_eq!(run(&["frob".into(), "x".into()]), 2);
        assert_eq!(run(&[]), 2);
    }

    #[test]
    fn unexpanded_entries_match_their_folder() {
        std::env::set_var("GOND_TEST_LOCAL", "C:\\Users\\d\\AppData\\Local");
        assert!(same_entry(
            "%GOND_TEST_LOCAL%\\agy\\bin",
            "C:\\Users\\D\\AppData\\Local\\agy\\bin\\"
        ));
        assert_eq!(
            without_entry(
                "C:\\a;%GOND_TEST_LOCAL%\\agy\\bin",
                "C:\\Users\\d\\AppData\\Local\\agy\\bin"
            )
            .as_deref(),
            Some("C:\\a")
        );
        assert!(!same_entry("%NOT_SET_GOND%\\x", "C:\\x"));
    }
}
