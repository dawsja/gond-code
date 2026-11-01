//! `gond-code purge-data [--dry-run]`: the uninstaller's "Delete the application
//! data". Deletes exactly Gond Code's own data roots (`appdata::roots`) and its
//! Credential Manager entries (`GondCode/…`), nothing else: never a project folder, never
//! a harness's own config (`.claude`, `.codex`, …). Each root must be the `GondCode` folder
//! directly inside the OS data dir, and a real folder (not a link), or it's refused.

use std::path::{Path, PathBuf};

use crate::appdata;

/// Why a root may not be deleted, or `None` when it's exactly ours.
pub fn refuse(root: &Path, data_dirs: &[PathBuf]) -> Option<String> {
    if root.file_name().and_then(|n| n.to_str()) != Some(appdata::DIR) {
        return Some(format!(
            "{} isn't a {} folder",
            root.display(),
            appdata::DIR
        ));
    }
    let parent = root.parent()?;
    if !data_dirs.iter().any(|d| d == parent) {
        return Some(format!(
            "{} isn't directly inside the user's app data folder",
            root.display()
        ));
    }
    match std::fs::symlink_metadata(root) {
        Ok(m) if m.file_type().is_symlink() => {
            Some(format!("{} is a link; not following it", root.display()))
        }
        Ok(m) if !m.is_dir() => Some(format!("{} isn't a folder", root.display())),
        _ => None,
    }
}

#[cfg(windows)]
fn credentials() -> Vec<String> {
    use windows_sys::Win32::Security::Credentials::{CredEnumerateW, CredFree, CREDENTIALW};
    let filter: Vec<u16> = format!("{}*", appdata::CREDENTIAL_PREFIX)
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut count = 0u32;
    let mut list: *mut *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: `filter` is NUL-terminated; on success `list` holds `count` credentials and
    // is freed with CredFree below.
    if unsafe { CredEnumerateW(filter.as_ptr(), 0, &mut count, &mut list) } == 0 {
        return vec![];
    }
    let mut names = vec![];
    for i in 0..count as usize {
        // SAFETY: `list` has `count` valid entries, each with a NUL-terminated TargetName.
        let name = unsafe {
            let c = &**list.add(i);
            let mut n = 0;
            while *c.TargetName.add(n) != 0 {
                n += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(c.TargetName, n))
        };
        names.push(name);
    }
    // SAFETY: `list` came from CredEnumerateW.
    unsafe { CredFree(list as *const _) };
    names
}

#[cfg(not(windows))]
fn credentials() -> Vec<String> {
    vec![]
}

#[cfg(windows)]
fn delete_credential(name: &str) -> bool {
    use windows_sys::Win32::Security::Credentials::{CredDeleteW, CRED_TYPE_GENERIC};
    let target: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `target` is NUL-terminated.
    unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) != 0 }
}

#[cfg(not(windows))]
fn delete_credential(_: &str) -> bool {
    true
}

/// Removes what Gond's accounts left behind, now that agents use their own sign-ins: every
/// `GondCode/` credential (account keys and tokens) and the sign-in homes under
/// `%LOCALAPPDATA%\GondCodeccounts`. Run at startup; nothing to do once they're gone.
pub fn forget_accounts() {
    for name in credentials() {
        if name.starts_with(appdata::CREDENTIAL_PREFIX) {
            delete_credential(&name);
        }
    }
    if let Some(homes) = appdata::local().map(|d| d.join("accounts")) {
        accounts_folder_gone(&homes);
    }
}

/// Deletes `homes` when it's a real folder (never through a link).
fn accounts_folder_gone(homes: &Path) {
    if std::fs::symlink_metadata(homes).is_ok_and(|m| m.is_dir()) {
        let _ = std::fs::remove_dir_all(homes);
    }
}

pub fn run(args: &[String]) -> i32 {
    let dry = args.iter().any(|a| a == "--dry-run");
    let data_dirs: Vec<PathBuf> = [dirs::data_dir(), dirs::data_local_dir()]
        .into_iter()
        .flatten()
        .collect();
    let mut failed = false;

    for root in appdata::roots() {
        if !root.exists() {
            continue;
        }
        if let Some(why) = refuse(&root, &data_dirs) {
            eprintln!("purge-data: refusing {}: {why}", root.display());
            failed = true;
            continue;
        }
        if dry {
            println!("would delete {}", root.display());
            continue;
        }
        match std::fs::remove_dir_all(&root) {
            Ok(()) => println!("deleted {}", root.display()),
            Err(e) => {
                eprintln!("purge-data: couldn't delete {}: {e}", root.display());
                failed = true;
            }
        }
    }

    for name in credentials() {
        if !name.starts_with(appdata::CREDENTIAL_PREFIX) {
            continue;
        }
        if dry {
            println!("would delete credential {name}");
        } else if delete_credential(&name) {
            println!("deleted credential {name}");
        } else {
            eprintln!("purge-data: couldn't delete credential {name}");
            failed = true;
        }
    }
    i32::from(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_gond_code_folder_right_inside_a_data_dir() {
        let data = tempfile::tempdir().unwrap();
        let dirs = vec![data.path().to_path_buf()];
        let ours = data.path().join(appdata::DIR);
        std::fs::create_dir_all(&ours).unwrap();
        assert_eq!(refuse(&ours, &dirs), None);

        // Anything else is refused: the data dir itself, a deeper folder, a sibling.
        assert!(refuse(data.path(), &dirs).is_some());
        let deeper = ours.join(appdata::DIR);
        std::fs::create_dir_all(&deeper).unwrap();
        assert!(refuse(&deeper, &dirs).is_some());
        assert!(refuse(&data.path().join("Code"), &dirs).is_some());
        let elsewhere = tempfile::tempdir().unwrap().path().join(appdata::DIR);
        assert!(refuse(&elsewhere, &dirs).is_some());
    }

    #[cfg(unix)]
    #[test]
    fn a_link_is_refused_not_followed() {
        let data = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let link = data.path().join(appdata::DIR);
        std::os::unix::fs::symlink(target.path(), &link).unwrap();
        assert!(refuse(&link, &[data.path().to_path_buf()]).is_some());
    }

    /// The old sign-in homes go, and a second run has nothing to do.
    #[test]
    fn the_old_accounts_folder_goes() {
        let local = tempfile::tempdir().unwrap();
        let homes = local.path().join("accounts");
        std::fs::create_dir_all(homes.join("claude/A1")).unwrap();
        std::fs::write(homes.join("claude/A1/.credentials.json"), "{}").unwrap();
        accounts_folder_gone(&homes);
        assert!(!homes.exists());
        accounts_folder_gone(&homes); // already gone: nothing to do
    }

    #[test]
    fn every_data_path_is_under_a_root() {
        let roots = appdata::roots();
        for p in [crate::store::default_db_path()].into_iter().flatten() {
            assert!(roots.iter().any(|r| p.starts_with(r)), "{}", p.display());
        }
    }
}
