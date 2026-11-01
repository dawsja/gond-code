//! Windows Credential Manager through CredWriteW / CredReadW / CredDeleteW: generic
//! credentials, persisted for this Windows user on this machine.

use windows_sys::Win32::Foundation::{GetLastError, ERROR_NOT_FOUND};
use windows_sys::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
    CRED_TYPE_GENERIC,
};

use super::{Secret, SecretRef, Vault};
use crate::error::{AppError, AppResult};

/// CRED_MAX_CREDENTIAL_BLOB_SIZE: 5 × 512 bytes.
const MAX_BLOB: usize = 2560;

#[derive(Default)]
pub struct CredentialManager;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn failed(what: &str, at: &SecretRef) -> AppError {
    // SAFETY: reads the calling thread's last-error value.
    let code = unsafe { GetLastError() };
    AppError::Invalid(format!(
        "Credential Manager {what} failed for {} (error {code})",
        at.0
    ))
}

impl Vault for CredentialManager {
    fn put(&self, at: &SecretRef, secret: &Secret) -> AppResult<()> {
        let blob = secret.expose().as_bytes();
        if blob.len() > MAX_BLOB {
            return Err(AppError::Invalid(format!(
                "a secret can be at most {MAX_BLOB} bytes"
            )));
        }
        let mut target = wide(&at.0);
        let mut user = wide("Gond Code");
        let cred = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: target.as_mut_ptr(),
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_ptr() as *mut u8,
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            UserName: user.as_mut_ptr(),
            ..Default::default()
        };
        // SAFETY: every pointer in `cred` outlives the call; CredWriteW copies the blob.
        if unsafe { CredWriteW(&cred, 0) } == 0 {
            return Err(failed("write", at));
        }
        Ok(())
    }

    fn get(&self, at: &SecretRef) -> AppResult<Option<Secret>> {
        let target = wide(&at.0);
        let mut out: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: `target` is NUL-terminated; on success `out` is freed with CredFree below.
        if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut out) } == 0 {
            // SAFETY: reads the calling thread's last-error value.
            if unsafe { GetLastError() } == ERROR_NOT_FOUND {
                return Ok(None);
            }
            return Err(failed("read", at));
        }
        // SAFETY: CredReadW succeeded, so `out` points at a valid CREDENTIALW whose blob
        // is `CredentialBlobSize` bytes long.
        let value = unsafe {
            let c = &*out;
            let bytes = std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize);
            let value = String::from_utf8_lossy(bytes).into_owned();
            CredFree(out as *const _);
            value
        };
        Ok(Some(Secret::new(value)))
    }

    fn delete(&self, at: &SecretRef) -> AppResult<()> {
        let target = wide(&at.0);
        // SAFETY: `target` is NUL-terminated.
        if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0 {
            // SAFETY: reads the calling thread's last-error value.
            if unsafe { GetLastError() } != ERROR_NOT_FOUND {
                return Err(failed("delete", at));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::SecretResolver;

    /// Deletes the test credential even when an assert fails.
    struct Cleanup(SecretRef);

    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = CredentialManager.delete(&self.0);
        }
    }

    /// Runs on the Windows CI job against the real Credential Manager.
    #[test]
    fn credential_manager_round_trip() {
        let v = CredentialManager;
        let at = SecretRef(format!("GondCode/test/{}", ulid::Ulid::generate()));
        let _cleanup = Cleanup(at.clone());
        assert!(v.get(&at).unwrap().is_none());
        v.put(&at, &Secret::new("sk-test-ünïcode")).unwrap();
        assert_eq!(v.resolve(&at).unwrap().expose(), "sk-test-ünïcode");
        v.put(&at, &Secret::new("rotated")).unwrap();
        assert_eq!(v.resolve(&at).unwrap().expose(), "rotated");
        v.delete(&at).unwrap();
        v.delete(&at).unwrap();
        assert!(v.get(&at).unwrap().is_none());
        assert!(v.put(&at, &Secret::new("x".repeat(MAX_BLOB + 1))).is_err());
    }
}
