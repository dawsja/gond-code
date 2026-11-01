//! The folder a process is working in: where an agent in a pane really runs, whatever the
//! shell's prompt last showed (`cd x; claude` on one line never shows `x`).
//!
//! On Windows it's read from the process's own parameters block (its PEB), the way Process
//! Explorer shows it: this works for 64-bit processes of this user. On Linux it's
//! `/proc/<pid>/cwd`.

use std::path::PathBuf;

/// `pid`'s working folder, if it can be read.
pub fn of(pid: u32) -> Option<PathBuf> {
    #[cfg(all(windows, target_pointer_width = "64"))]
    return win::cwd(pid);
    #[cfg(target_os = "linux")]
    return std::fs::read_link(format!("/proc/{pid}/cwd")).ok();
    #[allow(unreachable_code)]
    {
        let _ = pid;
        None
    }
}

#[cfg(all(windows, target_pointer_width = "64"))]
mod win {
    use std::ffi::c_void;
    use std::path::PathBuf;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };

    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn NtQueryInformationProcess(
            process: HANDLE,
            class: u32,
            info: *mut c_void,
            len: u32,
            returned: *mut u32,
        ) -> i32;
    }
    unsafe extern "system" {
        fn ReadProcessMemory(
            process: HANDLE,
            base: *const c_void,
            buffer: *mut c_void,
            size: usize,
            read: *mut usize,
        ) -> i32;
    }

    /// `PROCESS_BASIC_INFORMATION` on 64-bit Windows: the PEB's address is its second field.
    #[repr(C)]
    struct BasicInfo {
        exit_status: isize,
        peb: usize,
        affinity: usize,
        priority: isize,
        pid: usize,
        parent: usize,
    }

    /// Where, on 64-bit Windows, the PEB keeps its process parameters, and where those keep the
    /// current directory's `UNICODE_STRING` (its length, then at +8 its buffer).
    const PEB_PARAMETERS: usize = 0x20;
    const PARAMETERS_CURRENT_DIRECTORY: usize = 0x38;

    /// Reads a `T` at `at` in `h`'s memory.
    unsafe fn read<T: Default>(h: HANDLE, at: usize) -> Option<T> {
        let mut v = T::default();
        let mut n = 0usize;
        let ok = ReadProcessMemory(
            h,
            at as *const c_void,
            (&raw mut v).cast(),
            std::mem::size_of::<T>(),
            &mut n,
        );
        (ok != 0 && n == std::mem::size_of::<T>()).then_some(v)
    }

    pub fn cwd(pid: u32) -> Option<PathBuf> {
        // SAFETY: a process handle we close; reads into buffers we own, of the sizes we pass,
        // at addresses the target's own PEB gives; nothing is written to it.
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
            if h.is_null() {
                return None;
            }
            let out = (|| {
                let mut info: BasicInfo = std::mem::zeroed();
                let status = NtQueryInformationProcess(
                    h,
                    0, // ProcessBasicInformation
                    (&raw mut info).cast(),
                    std::mem::size_of::<BasicInfo>() as u32,
                    std::ptr::null_mut(),
                );
                if status != 0 || info.peb == 0 {
                    return None;
                }
                let params: usize = read(h, info.peb + PEB_PARAMETERS)?;
                let len: u16 = read(h, params + PARAMETERS_CURRENT_DIRECTORY)?;
                let buffer: usize = read(h, params + PARAMETERS_CURRENT_DIRECTORY + 8)?;
                if len == 0 || buffer == 0 {
                    return None;
                }
                let mut wide = vec![0u16; len as usize / 2];
                let mut n = 0usize;
                let ok = ReadProcessMemory(
                    h,
                    buffer as *const c_void,
                    wide.as_mut_ptr().cast(),
                    len as usize,
                    &mut n,
                );
                if ok == 0 {
                    return None;
                }
                let s = String::from_utf16_lossy(&wide);
                // "C:\code\gond\" → "C:\code\gond"; a drive's root keeps its backslash.
                let s = if s.len() > 3 {
                    s.trim_end_matches('\\')
                } else {
                    &s
                };
                Some(PathBuf::from(s))
            })();
            CloseHandle(h);
            out
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_child_process_working_folder_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let mut child = if cfg!(windows) {
            let mut c = crate::process::command("ping");
            c.args(["-n", "30", "127.0.0.1"]);
            c
        } else {
            let mut c = crate::process::command("sleep");
            c.arg("30");
            c
        };
        let mut child = child
            .current_dir(dir.path())
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(300));
        let got = super::of(child.id());
        let _ = child.kill();
        let _ = child.wait();
        if cfg!(any(windows, target_os = "linux")) {
            let got = got.expect("read the working folder");
            assert_eq!(
                got.canonicalize().unwrap(),
                dir.path().canonicalize().unwrap()
            );
        }
    }
}
