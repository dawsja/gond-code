//! The native window: Windows' own title bar, dark from the first frame (no custom or folded
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
pub fn setup<R: Runtime>(app: &AppHandle<R>) -> WindowInfo {
    let Some(window) = app.get_webview_window(MAIN) else {
        return WindowInfo {
            mica: false,
            caption: (),
        };
    };
    // Still hidden here (visible: false), so the first frame is already dark.
    paint_caption(&window);
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

/// Windows' own title bar, dark: dark mode, a #030303 caption (the sidebar's colour)
/// and white caption text. The colours need Windows 11; dark mode alone covers 10.
#[cfg(windows)]
fn paint_caption<R: Runtime>(window: &WebviewWindow<R>) {
    use std::ffi::c_void;
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
    };
    let Ok(hwnd) = window.hwnd() else { return };
    let hwnd = hwnd.0 as windows_sys::Win32::Foundation::HWND;
    let set = |attr: i32, value: u32| unsafe {
        // Safety: our own live window; a 4-byte value as each attribute expects.
        DwmSetWindowAttribute(hwnd, attr as u32, &value as *const u32 as *const c_void, 4);
    };
    set(DWMWA_USE_IMMERSIVE_DARK_MODE, 1);
    set(DWMWA_CAPTION_COLOR, 0x0003_0303); // COLORREF 0x00BBGGRR: #030303
    set(DWMWA_TEXT_COLOR, 0x00FF_FFFF); // white
}

#[cfg(not(windows))]
fn paint_caption<R: Runtime>(_: &WebviewWindow<R>) {}

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
