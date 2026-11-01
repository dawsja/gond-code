//! The app's AppUserModelID on Windows.

use windows::core::HSTRING;

/// Gives this process the app's AppUserModelID (the bundle identifier), so the taskbar groups
/// its window under the same id the installer's shortcut and notifications use, however it was
/// started: the installed shortcut, the exe directly or a dev build. Must run before the first
/// window is created.
pub fn set_app_id(app_id: &str) {
    // Safety: a plain call with a NUL-terminated string that outlives it.
    let _ = unsafe {
        windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(app_id))
    };
}
