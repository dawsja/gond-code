//! Where Gond Code keeps its own data: every file the app writes lives under
//! one of these two roots, and every secret under the `GondCode/` prefix in Windows
//! Credential Manager. A new kind of data gets a folder under a root (`local.join(..)`),
//! never a root of its own, so `gond-code purge-data` (the uninstaller's "Delete the
//! application data") removes it too.
//!
//! - `roaming`: `%APPDATA%\GondCode`, the database, `sessions.json` and saved screens.
//! - `local`: `%LOCALAPPDATA%\GondCode`, sign-in homes (accounts) and the speech model.

use std::path::PathBuf;

/// The folder name under the OS data dirs.
pub const DIR: &str = if cfg!(windows) {
    "GondCode"
} else {
    "gond-code"
};

/// Credential Manager target names Gond Code owns (`GondCode/<harness>/<account>`,
/// `GondCode/mcp/<name>`).
pub const CREDENTIAL_PREFIX: &str = "GondCode/";

/// `%APPDATA%\GondCode` (the XDG data dir elsewhere).
pub fn roaming() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join(DIR))
}

/// `%LOCALAPPDATA%\GondCode` (the same as `roaming` on Linux).
pub fn local() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join(DIR))
}

/// Every root, once each.
pub fn roots() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = vec![];
    for r in [roaming(), local()].into_iter().flatten() {
        if !out.contains(&r) {
            out.push(r);
        }
    }
    out
}
