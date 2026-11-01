//! The app's terminals: in the terminal host (`host.rs`) when it can be reached, so they
//! outlive the app; otherwise in the app itself, as before, so a pane always opens. Tests and
//! the browser-free API tests use the in-app manager only.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::client::{is_unreachable, HostClient};
use super::{Attached, Opened, PtyManager, PtyRead, PtySink, Spawn};
use crate::error::{AppError, AppResult};

pub struct Ptys {
    local: PtyManager,
    host: Option<HostClient>,
    /// The host couldn't be reached: new terminals run in the app.
    fell_back: AtomicBool,
}

impl Ptys {
    /// Terminals in this process only.
    pub fn local(sink: Arc<dyn PtySink>) -> Self {
        Ptys {
            local: PtyManager::new(sink),
            host: None,
            fell_back: AtomicBool::new(false),
        }
    }

    /// Terminals in the host, through `client`; in this process if it can't be reached.
    pub fn with_host(sink: Arc<dyn PtySink>, client: HostClient) -> Self {
        Ptys {
            local: PtyManager::new(sink),
            host: Some(client),
            fell_back: AtomicBool::new(false),
        }
    }

    fn host(&self) -> Option<&HostClient> {
        self.host
            .as_ref()
            .filter(|_| !self.fell_back.load(Ordering::SeqCst))
    }

    /// Connects to the host now (starting it if needed). False: terminals run in the app.
    pub fn warm(&self) -> bool {
        match self.host() {
            Some(h) => match h.connect() {
                Ok(()) => true,
                Err(e) => {
                    self.fall_back(&e);
                    false
                }
            },
            None => false,
        }
    }

    /// Whether terminals outlive the app (they're in the host).
    pub fn in_host(&self) -> bool {
        self.host().is_some()
    }

    fn fall_back(&self, e: &AppError) {
        eprintln!("terminal host: {e}; terminals run in the app");
        self.fell_back.store(true, Ordering::SeqCst);
    }

    /// Runs `f` on the host; if the host can't be reached at all, falls back and runs `g` here.
    fn either<T>(
        &self,
        f: impl FnOnce(&HostClient) -> AppResult<T>,
        g: impl FnOnce(&PtyManager) -> AppResult<T>,
    ) -> AppResult<T> {
        if let Some(h) = self.host() {
            match f(h) {
                Err(e) if is_unreachable(&e) => self.fall_back(&e),
                r => return r,
            }
        }
        g(&self.local)
    }

    /// Where `pty_id` lives: the app's own terminals are asked first.
    fn owner(&self, pty_id: &str) -> Option<&HostClient> {
        if self.local.holds(pty_id) {
            None
        } else {
            self.host.as_ref()
        }
    }

    pub fn spawn(
        &self,
        owner: &str,
        key: Option<&str>,
        spawn: &Spawn,
        cols: u16,
        rows: u16,
        headless: bool,
    ) -> AppResult<Opened> {
        self.either(
            |h| h.spawn(owner, key, spawn, cols, rows, headless),
            |l| l.spawn(owner, key, spawn, cols, rows, headless),
        )
    }

    /// A pane's terminal still running (or ended) in the host, with what it printed.
    pub fn attach(&self, key: &str) -> AppResult<Option<Attached>> {
        self.either(|h| h.attach(key), |l| Ok(l.attach(key)))
    }

    pub fn write(&self, pty_id: &str, data: &str) -> AppResult<()> {
        match self.owner(pty_id) {
            Some(h) => h.write(pty_id, data),
            None => self.local.write(pty_id, data),
        }
    }

    pub fn resize(&self, pty_id: &str, cols: u16, rows: u16) -> AppResult<()> {
        match self.owner(pty_id) {
            Some(h) => h.resize(pty_id, cols, rows),
            None => self.local.resize(pty_id, cols, rows),
        }
    }

    pub fn read(
        &self,
        pty_id: &str,
        since: Option<u64>,
        wait_for: Option<&str>,
        timeout: Option<Duration>,
    ) -> AppResult<PtyRead> {
        match self.owner(pty_id) {
            Some(h) => h.read(pty_id, since, wait_for, timeout),
            None => self.local.read(pty_id, since, wait_for, timeout),
        }
    }

    pub fn pid(&self, pty_id: &str) -> Option<u32> {
        match self.owner(pty_id) {
            Some(h) => h.pid(pty_id),
            None => self.local.pid(pty_id),
        }
    }

    /// Every running terminal's program's pid, in the host and here.
    pub fn pids(&self) -> Vec<u32> {
        let mut all = self.local.pids();
        if let Some(h) = &self.host {
            all.extend(h.pids());
        }
        all
    }

    pub fn close(&self, pty_id: &str) -> AppResult<()> {
        match self.owner(pty_id) {
            Some(h) => h.close(pty_id),
            None => self.local.close(pty_id),
        }
    }

    /// Closes every terminal `owner` opened (a project removed).
    pub fn close_thread(&self, owner: &str) {
        self.local.close_thread(owner);
        if let Some(h) = self.host() {
            let _ = h.close_owner(owner);
        }
    }

    /// Closes the keyed terminals of panes that no longer exist.
    pub fn prune(&self, keep: &[String]) -> usize {
        let mut n = self.local.prune(keep);
        if let Some(h) = self.host() {
            n += h.prune(keep).unwrap_or(0);
        }
        n
    }

    /// Closes every terminal, in the host too ("Quit and close terminals").
    pub fn close_all(&self) {
        self.local.close_all();
        if let Some(h) = &self.host {
            h.close_all();
        }
    }

    /// The app is going: its own terminals end with it, the host's keep running.
    pub fn detach(&self) {
        self.local.close_all();
        if let Some(h) = &self.host {
            h.disconnect();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pty::tests::Recorder;

    #[test]
    fn without_a_host_terminals_run_in_the_app() {
        let rec = Arc::new(Recorder::default());
        let dead = HostClient::new(
            if cfg!(windows) {
                r"\\.\pipe\gond-code-test-nobody-here".into()
            } else {
                "/nonexistent/gond.sock".into()
            },
            None,
            rec.clone(),
        );
        let ptys = Ptys::with_host(rec, dead);
        assert!(!ptys.warm());
        assert!(!ptys.in_host());
        let dir = tempfile::tempdir().unwrap();
        let o = ptys
            .spawn(
                "home",
                Some("p"),
                &crate::pty::test_shell(dir.path()),
                80,
                24,
                true,
            )
            .unwrap();
        assert_eq!(ptys.attach("p").unwrap().unwrap().pty_id, o.pty_id);
        ptys.close(&o.pty_id).unwrap();
    }
}
