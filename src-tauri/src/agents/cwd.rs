//! What a process is running with: the folder it works in (where an agent in a pane really
//! runs, whatever the shell's prompt last showed: `cd x; claude` on one line never shows `x`)
//! and its command line (an agent started with `--resume <id>` says which conversation it has).
//!
//! On Windows both are read from the process's own parameters block (its PEB), the way Process
//! Explorer shows them: this works for 64-bit processes of this user. On Linux they're
//! `/proc/<pid>/cwd` and `/proc/<pid>/cmdline`.

use std::path::PathBuf;

/// `pid`'s working folder, if it can be read.
pub fn of(pid: u32) -> Option<PathBuf> {
    #[cfg(all(windows, target_pointer_width = "64"))]
    return win::read_parameter(pid, win::CURRENT_DIRECTORY).map(|s| {
        // "C:\code\gond\" → "C:\code\gond"; a drive's root keeps its backslash.
        PathBuf::from(if s.len() > 3 {
            s.trim_end_matches('\\')
        } else {
            &s
        })
    });
    #[cfg(target_os = "linux")]
    return std::fs::read_link(format!("/proc/{pid}/cwd")).ok();
    #[allow(unreachable_code)]
    {
        let _ = pid;
        None
    }
}

/// `pid`'s arguments, its program first, if they can be read.
pub fn args(pid: u32) -> Option<Vec<String>> {
    #[cfg(all(windows, target_pointer_width = "64"))]
    return win::read_parameter(pid, win::COMMAND_LINE).map(|line| win::split(&line));
    #[cfg(target_os = "linux")]
    return std::fs::read(format!("/proc/{pid}/cmdline")).ok().map(|b| {
        b.split(|c| *c == 0)
            .filter(|a| !a.is_empty())
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect()
    });
    #[allow(unreachable_code)]
    {
        let _ = pid;
        None
    }
}

#[cfg(all(windows, target_pointer_width = "64"))]
mod win {
    use std::ffi::c_void;

    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };
    use windows_sys::Win32::UI::Shell::CommandLineToArgvW;

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
    /// current directory's and the command line's `UNICODE_STRING`s (each its length, then at
    /// +8 its buffer).
    const PEB_PARAMETERS: usize = 0x20;
    pub const CURRENT_DIRECTORY: usize = 0x38;
    pub const COMMAND_LINE: usize = 0x70;

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

    /// The string at `offset` in process `pid`'s parameters block.
    pub fn read_parameter(pid: u32, offset: usize) -> Option<String> {
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
                let len: u16 = read(h, params + offset)?;
                let buffer: usize = read(h, params + offset + 8)?;
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
                (ok != 0).then(|| String::from_utf16_lossy(&wide))
            })();
            CloseHandle(h);
            out
        }
    }

    /// A Windows command line split into arguments as programs see them.
    pub fn split(line: &str) -> Vec<String> {
        let wide: Vec<u16> = line.encode_utf16().chain(Some(0)).collect();
        let mut n = 0i32;
        // SAFETY: a NUL-terminated string; Windows allocates the array, freed with LocalFree.
        unsafe {
            let argv = CommandLineToArgvW(wide.as_ptr(), &mut n);
            if argv.is_null() {
                return vec![];
            }
            let out = (0..n as usize)
                .map(|i| {
                    let p = *argv.add(i);
                    let len = (0..).take_while(|&j| *p.add(j) != 0).count();
                    String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
                })
                .collect();
            LocalFree(argv.cast());
            out
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_child_process_folder_and_arguments_are_read() {
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
        let (cwd, args) = (super::of(child.id()), super::args(child.id()));
        let _ = child.kill();
        let _ = child.wait();
        if cfg!(any(windows, target_os = "linux")) {
            assert_eq!(
                cwd.expect("read the working folder")
                    .canonicalize()
                    .unwrap(),
                dir.path().canonicalize().unwrap()
            );
            let args = args.expect("read the arguments");
            assert_eq!(
                args.last().map(String::as_str),
                Some(if cfg!(windows) { "127.0.0.1" } else { "30" })
            );
        }
    }
}
