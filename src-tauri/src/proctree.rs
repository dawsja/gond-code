//! Process trees: every agent, proof and install process runs in a tree the
//! app owns, so stopping it, a timeout or the app going away never leaves orphans.
//!
//! - **Windows:** a job object with `KILL_ON_JOB_CLOSE`. `kill` terminates the job.
//!   Closing its handle kills whatever is left, whether the guard dropped or the app
//!   crashed (the OS closes our handles). The child is spawned suspended
//!   (`runner::new_process_group`) and only resumed once it's in the job, so nothing
//!   it starts can escape.
//! - **Unix:** the process group `runner::new_process_group` gives the child.
//!
//! [`contain`] adopts a spawned child and returns a guard; hold it until the child
//! has been waited on. [`kill_tree`] finds the tree by the child's pid.

use std::collections::HashMap;
use std::process::Child;
use std::sync::{Arc, Mutex, OnceLock};

#[cfg(windows)]
mod job {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
    };
    use windows_sys::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};

    /// Resumes every thread of the suspended process `pid` (a fresh one has just its
    /// main thread). True if one was resumed.
    pub fn resume(pid: u32) -> bool {
        // SAFETY: a snapshot we close, entries sized as Toolhelp requires, and thread
        // handles opened and closed here.
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
            if snap == INVALID_HANDLE_VALUE || snap.is_null() {
                return false;
            }
            let mut e: THREADENTRY32 = std::mem::zeroed();
            e.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
            let mut resumed = false;
            let mut more = Thread32First(snap, &mut e) != 0;
            while more {
                if e.th32OwnerProcessID == pid {
                    let t = OpenThread(THREAD_SUSPEND_RESUME, 0, e.th32ThreadID);
                    if !t.is_null() {
                        resumed |= ResumeThread(t) != u32::MAX;
                        CloseHandle(t);
                    }
                }
                more = Thread32Next(snap, &mut e) != 0;
            }
            CloseHandle(snap);
            resumed
        }
    }
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    /// A job object handle; closing it kills the processes still in it.
    pub struct Job(HANDLE);
    // The handle is only used through the kernel calls below, which are thread-safe.
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    impl Job {
        /// A new kill-on-close job holding `process`, or `None` if Windows refused.
        pub fn holding(process: HANDLE) -> Option<Job> {
            // SAFETY: plain Win32 calls on handles we own; `info` outlives the call.
            unsafe {
                let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if job.is_null() {
                    return None;
                }
                let job = Job(job);
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let ok = SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const core::ffi::c_void,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                if ok == 0 || AssignProcessToJobObject(job.0, process) == 0 {
                    return None;
                }
                Some(job)
            }
        }

        /// Whether process `pid` runs in this job.
        pub fn holds(&self, pid: u32) -> bool {
            use windows_sys::Win32::System::JobObjects::IsProcessInJob;
            use windows_sys::Win32::System::Threading::{
                OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            };
            // SAFETY: a process handle we open and close here; `self.0` is live.
            unsafe {
                let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
                if h.is_null() {
                    return false;
                }
                let mut inside = 0;
                let ok = IsProcessInJob(h, self.0, &mut inside);
                CloseHandle(h);
                ok != 0 && inside != 0
            }
        }

        pub fn kill(&self) {
            // SAFETY: `self.0` is a live job handle until drop.
            unsafe {
                TerminateJobObject(self.0, 1);
            }
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: closed once, here; KILL_ON_JOB_CLOSE ends what's left.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

/// One child and everything it starts.
struct Tree {
    #[cfg_attr(windows, allow(dead_code))]
    pid: u32,
    #[cfg(windows)]
    job: Option<job::Job>,
}

impl Tree {
    fn kill(&self) {
        #[cfg(windows)]
        if let Some(j) = &self.job {
            j.kill();
        }
        #[cfg(unix)]
        {
            let quiet = |c: &mut std::process::Command| {
                c.stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null());
            };
            let mut c = crate::process::command("kill");
            c.args(["-s", "KILL", "--", &format!("-{}", self.pid)]);
            quiet(&mut c);
            let _ = c.status();
            // A terminal's shell leads its own session, and with job control on (it runs on a
            // terminal) each `cmd &` gets a process group of its own: the session holds them all.
            // For a child that isn't a session leader, no session has its pid: nothing matches.
            let mut c = crate::process::command("pkill");
            c.args(["-KILL", "-s", &self.pid.to_string()]);
            quiet(&mut c);
            let _ = c.status();
        }
        #[cfg(not(any(windows, unix)))]
        let _ = self.pid;
    }
}

fn trees() -> &'static Mutex<HashMap<u32, Arc<Tree>>> {
    static T: OnceLock<Mutex<HashMap<u32, Arc<Tree>>>> = OnceLock::new();
    T.get_or_init(Default::default)
}

/// Held while a contained child runs. Dropping it ends the tree: anything the child
/// left running is killed.
pub struct Guard {
    pid: u32,
}

impl Guard {
    pub fn pid(&self) -> u32 {
        self.pid
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let tree = trees()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.pid);
        if let Some(t) = tree {
            // Leftovers (a dev server the agent started, a stuck grandchild) go too.
            t.kill();
        }
    }
}

/// Puts a just-spawned child (and so its future descendants) in its own tree.
/// The child must have been spawned with `runner::new_process_group`: on Windows it
/// is suspended until this puts it in its job and resumes it.
pub fn contain(child: &Child) -> Guard {
    let pid = child.id();
    #[cfg(windows)]
    let tree = {
        use std::os::windows::io::AsRawHandle;
        let job = job::Job::holding(child.as_raw_handle() as _);
        // In its job (or not, if Windows refused): now let it run.
        job::resume(pid);
        Tree { pid, job }
    };
    #[cfg(not(windows))]
    let tree = {
        let _ = child;
        Tree { pid }
    };
    trees()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(pid, Arc::new(tree));
    Guard { pid }
}

/// [`contain`] for a process spawned by other means (a pseudo console), by pid.
/// On Unix that process leads its own session already, so its group is its tree.
pub fn contain_pid(pid: u32) -> Guard {
    #[cfg(windows)]
    let tree = {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
        };
        // SAFETY: a handle we open and close here; the job keeps its own reference.
        let job = unsafe {
            let h = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
            if h.is_null() {
                None
            } else {
                let j = job::Job::holding(h);
                CloseHandle(h);
                j
            }
        };
        Tree { pid, job }
    };
    #[cfg(not(windows))]
    let tree = Tree { pid };
    trees()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(pid, Arc::new(tree));
    Guard { pid }
}

/// Whether `pid` is the child `root` itself or runs in its tree (its job, on Windows):
/// how Gond's listeners know a caller is the agent it launched or something that agent
/// started. False for a root that isn't (or is no longer) contained, and off Windows
/// for anything but `root` itself.
pub fn holds(root: u32, pid: u32) -> bool {
    if root == pid {
        return true;
    }
    let tree = trees()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&root)
        .cloned();
    #[cfg(windows)]
    return tree.is_some_and(|t| t.job.as_ref().is_some_and(|j| j.holds(pid)));
    #[cfg(not(windows))]
    {
        let _ = tree;
        false
    }
}

/// Kills the tree of the contained child `pid`: the child and all its descendants.
/// A no-op for a pid that isn't (or is no longer) contained.
pub fn kill_tree(pid: u32) {
    let tree = trees()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&pid)
        .cloned();
    if let Some(t) = tree {
        t.kill();
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    /// A parent that starts a long-lived grandchild and prints the grandchild's pid.
    fn parent(exit_at_once: bool) -> std::process::Command {
        #[cfg(windows)]
        let mut c = {
            let wait = if exit_at_once { "" } else { "; Start-Sleep 60" };
            let mut c = crate::process::command("powershell");
            c.args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &format!(
                    "$p = Start-Process ping -ArgumentList '-n','120','127.0.0.1' -PassThru -WindowStyle Hidden; $p.Id{wait}"
                ),
            ]);
            c
        };
        #[cfg(not(windows))]
        let mut c = {
            let wait = if exit_at_once { "" } else { "; wait" };
            let mut c = crate::process::command("sh");
            c.args(["-c", &format!("sleep 120 & echo $!{wait}")]);
            c
        };
        c.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        crate::process::new_process_group(&mut c);
        c
    }

    fn grandchild(child: &mut Child) -> u32 {
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        line.trim()
            .parse()
            .unwrap_or_else(|_| panic!("no pid in {line:?}"))
    }

    pub(crate) fn alive(pid: u32) -> bool {
        #[cfg(windows)]
        {
            let out = crate::process::command("tasklist")
                .args(["/FI", &format!("PID eq {pid}"), "/NH"])
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout).contains(&pid.to_string())
        }
        #[cfg(not(windows))]
        {
            // Gone, or a zombie nobody has reaped yet.
            match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
                Ok(s) => !s
                    .rsplit(')')
                    .next()
                    .is_some_and(|rest| rest.trim_start().starts_with('Z')),
                Err(_) => false,
            }
        }
    }

    pub(crate) fn gone_soon(pid: u32) -> bool {
        let until = Instant::now() + Duration::from_secs(15);
        while Instant::now() < until {
            if !alive(pid) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        false
    }

    #[test]
    fn killing_the_tree_kills_the_grandchild() {
        let mut child = parent(false).spawn().unwrap();
        let guard = contain(&child);
        let gc = grandchild(&mut child);
        assert!(alive(gc), "the grandchild runs");
        kill_tree(guard.pid());
        let _ = child.wait();
        assert!(gone_soon(gc), "the grandchild {gc} died with its parent");
        drop(guard);
    }

    #[test]
    fn a_grandchild_left_behind_dies_with_the_guard() {
        let mut child = parent(true).spawn().unwrap();
        let guard = contain(&child);
        let gc = grandchild(&mut child);
        let _ = child.wait();
        assert!(alive(gc), "the parent left it running");
        drop(guard);
        assert!(gone_soon(gc), "no orphan: {gc} died when the tree closed");
        // Unknown pids are a no-op.
        kill_tree(gc);
    }
}
