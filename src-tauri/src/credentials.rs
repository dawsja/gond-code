//! Reads a generic credential from Windows Credential Manager: where agy and GitHub Copilot keep
//! their own sign-ins, which Settings → Usage reads plan limits with. Gond writes nothing there.

/// The credential stored under `target`, as text; `None` when there is none (or off Windows).
pub fn read(target: &str) -> Option<String> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Security::Credentials::{
            CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC,
        };
        let target: Vec<u16> = target.encode_utf16().chain(std::iter::once(0)).collect();
        let mut out: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: `target` is NUL-terminated; on success `out` is freed with CredFree below.
        if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut out) } == 0 {
            return None;
        }
        // SAFETY: CredReadW succeeded, so `out` points at a valid CREDENTIALW whose blob is
        // `CredentialBlobSize` bytes long.
        unsafe {
            let c = &*out;
            let bytes = std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize);
            let value = String::from_utf8_lossy(bytes).into_owned();
            CredFree(out as *const _);
            Some(value)
        }
    }
    #[cfg(not(windows))]
    {
        let _ = target;
        None
    }
}
