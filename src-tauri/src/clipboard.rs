//! What's on the clipboard, for a paste into a terminal. Read here rather than through the
//! WebView's `navigator.clipboard`, which asks "wants to see text and images copied to the
//! clipboard" before every read it hasn't been allowed.

use schemars::JsonSchema;
use serde::Serialize;

/// The clipboard's text, if it holds any, and whether it holds a picture.
#[derive(Debug, Default, Serialize, JsonSchema)]
pub struct Clip {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub image: bool,
}

/// The clipboard now. Empty off Windows, or when another program holds it for over a second.
pub fn read() -> Clip {
    #[cfg(windows)]
    {
        win::read()
    }
    #[cfg(not(windows))]
    {
        Clip::default()
    }
}

#[cfg(windows)]
mod win {
    use super::Clip;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
        RegisterClipboardFormatW,
    };
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalUnlock};

    const CF_BITMAP: u32 = 2;
    const CF_DIB: u32 = 8;
    const CF_UNICODETEXT: u32 = 13;
    const CF_DIBV5: u32 = 17;

    pub fn read() -> Clip {
        // Another program can hold the clipboard for a moment (it's writing to it).
        let mut opened = false;
        for _ in 0..20 {
            if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
                opened = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        if !opened {
            return Clip::default();
        }
        let png: Vec<u16> = "PNG".encode_utf16().chain(Some(0)).collect();
        let png = unsafe { RegisterClipboardFormatW(png.as_ptr()) };
        let image = [CF_BITMAP, CF_DIB, CF_DIBV5, png]
            .iter()
            .any(|&f| f != 0 && unsafe { IsClipboardFormatAvailable(f) } != 0);
        let text = unsafe { text() }.filter(|t| !t.is_empty());
        unsafe { CloseClipboard() };
        Clip { text, image }
    }

    /// CF_UNICODETEXT, up to its NUL. Only with the clipboard open.
    unsafe fn text() -> Option<String> {
        if IsClipboardFormatAvailable(CF_UNICODETEXT) == 0 {
            return None;
        }
        let handle = GetClipboardData(CF_UNICODETEXT);
        if handle.is_null() {
            return None;
        }
        let ptr = GlobalLock(handle) as *const u16;
        if ptr.is_null() {
            return None;
        }
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
        GlobalUnlock(handle);
        Some(text)
    }
}
