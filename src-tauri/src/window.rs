//! The native window: Windows' own title bar, themed from the first frame (no custom or folded
//! caption), with the app's header row inside the
//! window below it; one instance; remembered size and position; Mica Alt on Windows 11
//! where the UI wants it.

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime, WebviewWindow};

pub const MAIN: &str = "main";

/// What the UI needs to know to draw its chrome.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    /// Mica is applied: the UI may leave its background transparent where it
    /// wants the material to show. Otherwise it must paint a solid background.
    pub mica: bool,
    /// Always `null`: the title bar is Windows' own (snap layouts, drag, double-click
    /// and the system menu come from Windows). Kept so old callers read "no caption".
    pub caption: (),
}

/// `WindowInfo` once `setup` has it. Managed before `setup` runs, because the page
/// can ask before then; `window_info` waits for it rather than failing, which the
/// UI took as "no caption" and so never made its bar the drag region.
#[derive(Default)]
pub struct InfoCell {
    info: std::sync::Mutex<Option<WindowInfo>>,
    ready: std::sync::Condvar,
}

impl InfoCell {
    pub fn set(&self, info: WindowInfo) {
        *self.info.lock().unwrap_or_else(|e| e.into_inner()) = Some(info);
        self.ready.notify_all();
    }

    /// Blocks until `set`, or `timeout` (then "no caption", as before).
    pub fn wait(&self, timeout: std::time::Duration) -> WindowInfo {
        let guard = self.info.lock().unwrap_or_else(|e| e.into_inner());
        let (guard, _) = self
            .ready
            .wait_timeout_while(guard, timeout, |i| i.is_none())
            .unwrap_or_else(|e| e.into_inner());
        guard.unwrap_or(WindowInfo {
            mica: false,
            caption: (),
        })
    }
}

/// Applies the material and shows the window. It starts hidden, so the restored
/// size and position appear without a flash of the default ones.
pub fn setup<R: Runtime>(app: &AppHandle<R>, store: &crate::store::Store) -> WindowInfo {
    let Some(window) = app.get_webview_window(MAIN) else {
        return WindowInfo {
            mica: false,
            caption: (),
        };
    };
    // Still hidden here (visible: false): use the saved preference before showing it.
    setup_theme(&window, store);
    #[cfg(windows)]
    answer_microphone(&window);
    let info = WindowInfo {
        mica: false,
        caption: (),
    };
    let _ = window.show();
    let _ = window.set_focus();
    info
}

/// The chat box's mic: Gond answers WebView2's microphone prompt itself, allowed while
/// speech to text is on (turning it on in Settings is the consent) and refused otherwise, so
/// there's no "tauri.localhost wants to use your microphones" box. Windows' own privacy
/// switch and mic-in-use indicator still apply.
#[cfg(windows)]
fn answer_microphone<R: Runtime>(window: &WebviewWindow<R>) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PERMISSION_KIND, COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
        COREWEBVIEW2_PERMISSION_STATE_ALLOW, COREWEBVIEW2_PERMISSION_STATE_DENY,
    };
    let _ = window.with_webview(|wv| unsafe {
        let Ok(core) = wv.controller().CoreWebView2() else {
            return;
        };
        let handler = webview2_com::PermissionRequestedEventHandler::create(Box::new(|_, args| {
            let Some(args) = args else {
                return Ok(());
            };
            let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
            args.PermissionKind(&mut kind)?;
            if kind == COREWEBVIEW2_PERMISSION_KIND_MICROPHONE {
                args.SetState(if crate::speech::on() {
                    COREWEBVIEW2_PERMISSION_STATE_ALLOW
                } else {
                    COREWEBVIEW2_PERMISSION_STATE_DENY
                })?;
            }
            Ok(())
        }));
        let mut token = 0i64;
        let _ = core.add_PermissionRequested(&handler, &mut token);
    });
}

/// Resolve a preference without letting an OS event override an explicit choice.
#[cfg(any(windows, test))]
fn resolved_theme(preference: crate::settings::Theme, system: tauri::Theme) -> tauri::Theme {
    match preference {
        crate::settings::Theme::System => system,
        crate::settings::Theme::Light => tauri::Theme::Light,
        crate::settings::Theme::Dark => tauri::Theme::Dark,
    }
}

/// Read Windows' app preference, not `window.theme()` (which may still be forced to the
/// previous choice when changing Dark/Light back to System). Missing registry value = Light.
#[cfg(windows)]
fn system_theme() -> tauri::Theme {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let key: Vec<u16> = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let name: Vec<u16> = "AppsUseLightTheme".encode_utf16().chain(Some(0)).collect();
    let mut light = 1u32;
    let mut bytes = std::mem::size_of_val(&light) as u32;
    let status = unsafe {
        // Safety: valid NUL-terminated strings and a writable DWORD-sized buffer.
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&mut light as *mut u32).cast(),
            &mut bytes,
        )
    };
    if status == ERROR_SUCCESS && light == 0 {
        tauri::Theme::Dark
    } else {
        tauri::Theme::Light
    }
}

#[cfg(windows)]
fn apply_theme<R: Runtime>(window: &WebviewWindow<R>, preference: crate::settings::Theme) {
    let forced = match preference {
        crate::settings::Theme::System => None,
        crate::settings::Theme::Light => Some(tauri::Theme::Light),
        crate::settings::Theme::Dark => Some(tauri::Theme::Dark),
    };
    // None lets Tauri keep delivering OS theme changes in System mode.
    let _ = window.set_theme(forced);
    paint_caption(window, resolved_theme(preference, system_theme()));
}

#[cfg(windows)]
fn setup_theme<R: Runtime>(window: &WebviewWindow<R>, store: &crate::store::Store) {
    apply_theme(
        window,
        crate::settings::get(store).unwrap_or_default().theme,
    );
    let (app, store) = (window.app_handle().clone(), store.clone());
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::ThemeChanged(_) = event {
            if let Some(window) = app.get_webview_window(MAIN) {
                let preference = crate::settings::get(&store).unwrap_or_default().theme;
                // Read the current OS preference rather than a queued event's old theme.
                // Do not set_theme here: doing so may itself send another ThemeChanged.
                paint_caption(&window, resolved_theme(preference, system_theme()));
            }
        }
    });
}

#[cfg(not(windows))]
fn setup_theme<R: Runtime>(_: &WebviewWindow<R>, _: &crate::store::Store) {}

/// The existing settings.set handler persists and publishes /settings/theme. Keep native
/// work here, outside the Tauri-free registry, and marshal it onto the window thread.
#[cfg(windows)]
pub fn watch_theme<R: Runtime>(app: &AppHandle<R>, services: &crate::api::Services) {
    let (app, store) = (app.clone(), services.store.clone());
    services.state.subscribe(move |patch| {
        if !patch
            .ops
            .iter()
            .any(|op| matches!(op.path(), "/settings" | "/settings/theme" | ""))
        {
            return;
        }
        let (handle, store) = (app.clone(), store.clone());
        let _ = app.run_on_main_thread(move || {
            if let Some(window) = handle.get_webview_window(MAIN) {
                // Read the latest saved preference so queued changes cannot apply stale values.
                apply_theme(
                    &window,
                    crate::settings::get(&store).unwrap_or_default().theme,
                );
            }
        });
    });
}

/// Windows' own title bar and the native/WebView background. Caption colours need
/// Windows 11; immersive dark mode alone covers Windows 10.
#[cfg(windows)]
fn paint_caption<R: Runtime>(window: &WebviewWindow<R>, theme: tauri::Theme) {
    use std::ffi::c_void;
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
    };
    let dark = theme == tauri::Theme::Dark;
    let ground = if dark {
        tauri::window::Color(0, 0, 0, 255)
    } else {
        tauri::window::Color(245, 243, 238, 255) // #f5f3ee, the light floor
    };
    let _ = window.set_background_color(Some(ground));
    let Ok(hwnd) = window.hwnd() else { return };
    let hwnd = hwnd.0 as windows_sys::Win32::Foundation::HWND;
    let set = |attr: i32, value: u32| unsafe {
        // Safety: our own live window; a 4-byte value as each attribute expects.
        DwmSetWindowAttribute(hwnd, attr as u32, &value as *const u32 as *const c_void, 4);
    };
    set(DWMWA_USE_IMMERSIVE_DARK_MODE, u32::from(dark));
    // COLORREF 0x00BBGGRR: match the neutral sidebar, with contrasting caption text.
    set(
        DWMWA_CAPTION_COLOR,
        if dark { 0x0003_0303 } else { 0x00FF_FFFF },
    );
    set(
        DWMWA_TEXT_COLOR,
        if dark { 0x00FF_FFFF } else { 0x0020_2020 },
    );
}

/// A second launch focuses the running window.
pub fn on_second_instance<R: Runtime>(app: &AppHandle<R>) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[cfg(test)]
mod info_tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Duration;

    #[test]
    fn only_system_follows_os_theme_changes() {
        use crate::settings::Theme;
        for os in [tauri::Theme::Light, tauri::Theme::Dark] {
            assert_eq!(resolved_theme(Theme::System, os), os);
            assert_eq!(resolved_theme(Theme::Light, os), tauri::Theme::Light);
            assert_eq!(resolved_theme(Theme::Dark, os), tauri::Theme::Dark);
        }
    }

    #[test]
    fn a_waiting_ask_gets_the_info_set_later() {
        let cell = Arc::new(InfoCell::default());
        let asker = {
            let cell = cell.clone();
            std::thread::spawn(move || cell.wait(Duration::from_secs(5)))
        };
        std::thread::sleep(Duration::from_millis(50));
        cell.set(WindowInfo {
            mica: true,
            caption: (),
        });
        let got = asker.join().unwrap();
        assert!(got.mica);
        assert_eq!(
            serde_json::to_value(got).unwrap()["caption"],
            serde_json::Value::Null
        );
        let empty = InfoCell::default().wait(Duration::from_millis(10));
        assert!(!empty.mica, "times out to no material");
    }
}
