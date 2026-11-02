//! The app's terminals: in the terminal host (`host.rs`) when it can be reached, so they
//! outlive the app; otherwise in the app itself, as before, so a pane always opens. Tests and
//! the browser-free API tests use the in-app manager only.
//!
//! New terminals go to this build's host. A pane whose terminal is still running in an older
//! build's host (from before an update) finds it there: `attach` looks in the hosts
//! `host\live` lists, and every request for that terminal then goes to the host that has it.
//!
//! A terminal on another machine (WSL, SSH: `crate::remote`) goes to that machine's host, and
//! every request for it after that.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::client::{is_unreachable, HostClient};
use super::{Attached, Opened, PtyManager, PtyRead, PtySink, Spawn};
use crate::error::{AppError, AppResult};

/// The endpoints of other builds' running hosts.
type FindOlder = Box<dyn Fn() -> Vec<String> + Send + Sync>;

pub struct Ptys {
    /// Other machines' hosts (WSL, SSH), and which machine each of their terminals is on.
    #[cfg(not(gond_remote))]
    remote: std::sync::OnceLock<Arc<crate::remote::Hub>>,
    #[cfg(not(gond_remote))]
    remote_ptys: Mutex<std::collections::HashMap<String, String>>,
    local: PtyManager,
    host: Option<HostClient>,
    /// Older builds' hosts this app has reached, for its panes still running there.
    older: Mutex<Vec<Arc<HostClient>>>,
    find_older: Option<FindOlder>,
    sink: Arc<dyn PtySink>,
    /// The host couldn't be reached: new terminals run in the app.
    fell_back: AtomicBool,
}

/// Where a terminal is.
enum Route<'a> {
    Local,
    Host(&'a HostClient),
    Older(Arc<HostClient>),
}

impl Ptys {
    /// Terminals in this process only.
    pub fn local(sink: Arc<dyn PtySink>) -> Self {
        Ptys {
            #[cfg(not(gond_remote))]
            remote: Default::default(),
            #[cfg(not(gond_remote))]
            remote_ptys: Default::default(),
            local: PtyManager::new(sink.clone()),
            host: None,
            older: Mutex::default(),
            find_older: None,
            sink,
            fell_back: AtomicBool::new(false),
        }
    }

    /// Terminals in this build's host, through `client`, and in older builds' hosts that are
    /// still running; in this process if no host can be reached.
    pub fn with_host(sink: Arc<dyn PtySink>, client: HostClient) -> Self {
        Self::with_hosts(sink, client, Box::new(super::host::older_endpoints))
    }

    /// `with_host`, with older hosts found by `find_older`.
    pub fn with_hosts(sink: Arc<dyn PtySink>, client: HostClient, find_older: FindOlder) -> Self {
        Ptys {
            #[cfg(not(gond_remote))]
            remote: Default::default(),
            #[cfg(not(gond_remote))]
            remote_ptys: Default::default(),
            local: PtyManager::new(sink.clone()),
            host: Some(client),
            older: Mutex::default(),
            find_older: Some(find_older),
            sink,
            fell_back: AtomicBool::new(false),
        }
    }

    fn host(&self) -> Option<&HostClient> {
        self.host
            .as_ref()
            .filter(|_| !self.fell_back.load(Ordering::SeqCst))
    }

    /// The older hosts reached so far.
    fn reached(&self) -> Vec<Arc<HostClient>> {
        self.older.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// The older hosts, with any that have started saying they run since the last look.
    fn older(&self) -> Vec<Arc<HostClient>> {
        let Some(find) = &self.find_older else {
            return vec![];
        };
        let mut older = self.older.lock().unwrap_or_else(|e| e.into_inner());
        let own = self.host.as_ref().map(|h| h.endpoint().to_string());
        for ep in find() {
            if Some(&ep) != own.as_ref() && !older.iter().any(|h| h.endpoint() == ep) {
                older.push(Arc::new(HostClient::new(ep, None, self.sink.clone())));
            }
        }
        older.clone()
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

    /// Where `pty_id` is: the app's own, an older build's host that gave it to this app, else
    /// this build's host.
    fn route(&self, pty_id: &str) -> Route<'_> {
        #[cfg(not(gond_remote))]
        if let Some(c) = self.remote_of(pty_id) {
            return Route::Older(c);
        }
        if self.local.holds(pty_id) {
            return Route::Local;
        }
        if let Some(h) = self.host.as_ref().filter(|h| h.knows(pty_id)) {
            return Route::Host(h);
        }
        if let Some(o) = self.reached().into_iter().find(|o| o.knows(pty_id)) {
            return Route::Older(o);
        }
        match &self.host {
            Some(h) => Route::Host(h),
            None => Route::Local,
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

    /// A pane's terminal still running (or ended) in this build's host or an older one's,
    /// with what it printed.
    pub fn attach(&self, key: &str) -> AppResult<Option<Attached>> {
        if let Some(found) = self.either(|h| h.attach(key), |l| Ok(l.attach(key)))? {
            return Ok(Some(found));
        }
        for o in self.older() {
            if let Ok(Some(found)) = o.attach(key) {
                return Ok(Some(found));
            }
        }
        Ok(None)
    }

    pub fn write(&self, pty_id: &str, data: &str) -> AppResult<()> {
        match self.route(pty_id) {
            Route::Local => self.local.write(pty_id, data),
            Route::Host(h) => h.write(pty_id, data),
            Route::Older(h) => h.write(pty_id, data),
        }
    }

    pub fn resize(&self, pty_id: &str, cols: u16, rows: u16) -> AppResult<()> {
        match self.route(pty_id) {
            Route::Local => self.local.resize(pty_id, cols, rows),
            Route::Host(h) => h.resize(pty_id, cols, rows),
            Route::Older(h) => h.resize(pty_id, cols, rows),
        }
    }

    pub fn read(
        &self,
        pty_id: &str,
        since: Option<u64>,
        wait_for: Option<&str>,
        timeout: Option<Duration>,
    ) -> AppResult<PtyRead> {
        match self.route(pty_id) {
            Route::Local => self.local.read(pty_id, since, wait_for, timeout),
            Route::Host(h) => h.read(pty_id, since, wait_for, timeout),
            Route::Older(h) => h.read(pty_id, since, wait_for, timeout),
        }
    }

    /// The terminal's program's pid on this PC; none for one on another machine.
    pub fn pid(&self, pty_id: &str) -> Option<u32> {
        #[cfg(not(gond_remote))]
        if self.machine_of(pty_id).is_some() {
            return None;
        }
        match self.route(pty_id) {
            Route::Local => self.local.pid(pty_id),
            Route::Host(h) => h.pid(pty_id),
            Route::Older(h) => h.pid(pty_id),
        }
    }

    /// Every running terminal's program's pid, in every host and here.
    pub fn pids(&self) -> Vec<u32> {
        let mut all = self.local.pids();
        if let Some(h) = &self.host {
            all.extend(h.pids());
        }
        for o in self.reached() {
            all.extend(o.pids());
        }
        all
    }

    pub fn close(&self, pty_id: &str) -> AppResult<()> {
        match self.route(pty_id) {
            Route::Local => self.local.close(pty_id),
            Route::Host(h) => h.close(pty_id),
            Route::Older(h) => h.close(pty_id),
        }
    }

    /// Closes every terminal `owner` opened (a project removed).
    pub fn close_thread(&self, owner: &str) {
        #[cfg(not(gond_remote))]
        for c in self.remotes() {
            let _ = c.close_owner(owner);
        }
        self.local.close_thread(owner);
        if let Some(h) = self.host() {
            let _ = h.close_owner(owner);
        }
        for o in self.reached() {
            let _ = o.close_owner(owner);
        }
    }

    /// Closes the keyed terminals of panes that no longer exist, in every host.
    pub fn prune(&self, keep: &[String]) -> usize {
        let mut n = self.local.prune(keep);
        #[cfg(not(gond_remote))]
        for c in self.remotes() {
            n += c.prune(keep).unwrap_or(0);
        }
        if let Some(h) = self.host() {
            n += h.prune(keep).unwrap_or(0);
        }
        for o in self.older() {
            n += o.prune(keep).unwrap_or(0);
        }
        n
    }

    /// Closes every terminal, in every host too ("Quit and close terminals").
    pub fn close_all(&self) {
        #[cfg(not(gond_remote))]
        for c in self.remotes() {
            c.close_all();
        }
        self.local.close_all();
        if let Some(h) = &self.host {
            h.close_all();
        }
        for o in self.older() {
            o.close_all();
        }
    }

    /// The app is going: its own terminals end with it, the hosts' keep running.
    pub fn detach(&self) {
        #[cfg(not(gond_remote))]
        if let Some(h) = self.remote.get() {
            h.detach();
        }
        self.local.close_all();
        if let Some(h) = &self.host {
            h.disconnect();
        }
        for o in self.reached() {
            o.disconnect();
        }
    }
}

/// Terminals on other machines (WSL, SSH).
#[cfg(not(gond_remote))]
impl Ptys {
    /// Gives this the machines' hub; a WSL distribution's keepalive runs in this PC's host.
    pub fn set_remote(self: &Arc<Self>, hub: Arc<crate::remote::Hub>) {
        let me = Arc::downgrade(self);
        hub.set_keepalive(Box::new(move |machine, spawn| {
            let Some(p) = me.upgrade() else { return };
            let owner = format!("keepalive:{machine}");
            p.close_thread_here(&owner);
            if let Err(e) = p.spawn(&owner, None, &spawn, 80, 24, true) {
                eprintln!("keepalive for {machine}: {e}");
            }
        }));
        let _ = self.remote.set(hub);
    }

    pub fn hub(&self) -> AppResult<&Arc<crate::remote::Hub>> {
        self.remote
            .get()
            .ok_or_else(|| AppError::Invalid("no other machines here".into()))
    }

    /// Closes `owner`'s terminals on this PC only (a keepalive replaced).
    fn close_thread_here(&self, owner: &str) {
        self.local.close_thread(owner);
        if let Some(h) = self.host() {
            let _ = h.close_owner(owner);
        }
    }

    fn remotes(&self) -> Vec<Arc<HostClient>> {
        self.remote.get().map(|h| h.connected()).unwrap_or_default()
    }

    /// The machine `pty_id` runs on, if not this PC.
    pub fn machine_of(&self, pty_id: &str) -> Option<String> {
        self.remote_ptys
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(pty_id)
            .cloned()
    }

    /// The client of the machine `pty_id` runs on, if not this PC.
    pub fn remote_of(&self, pty_id: &str) -> Option<Arc<HostClient>> {
        let m = self.machine_of(pty_id)?;
        self.remote.get()?.client(&m).ok()
    }

    fn mark(&self, pty_id: &str, machine: &str) {
        self.remote_ptys
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(pty_id.to_string(), machine.to_string());
    }

    /// Opens a terminal on `machine`, connecting to it first if need be.
    pub fn spawn_on(
        &self,
        machine: &str,
        owner: &str,
        key: Option<&str>,
        spawn: &Spawn,
        cols: u16,
        rows: u16,
    ) -> AppResult<Opened> {
        let c = self.hub()?.ensure(machine)?;
        let o = c.spawn(owner, key, spawn, cols, rows, false)?;
        self.mark(&o.pty_id, machine);
        Ok(o)
    }

    /// A pane's terminal still running on `machine`, connecting to it first if need be.
    pub fn attach_on(&self, machine: &str, key: &str) -> AppResult<Option<Attached>> {
        let c = self.hub()?.ensure(machine)?;
        let found = c.attach(key)?;
        if let Some(a) = &found {
            self.mark(&a.pty_id, machine);
        }
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pty::tests::{wait_for, Recorder};
    use std::time::Instant;

    fn endpoint(dir: &std::path::Path, name: &str) -> String {
        if cfg!(windows) {
            format!(r"\\.\pipe\gond-code-test-{name}-{}", ulid::Ulid::generate())
        } else {
            dir.join(format!("{name}.sock"))
                .to_string_lossy()
                .into_owned()
        }
    }

    fn connected(c: &HostClient) {
        let start = Instant::now();
        while c.connect().is_err() {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "host didn't come up"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

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
        let ptys = Ptys::with_hosts(rec, dead, Box::new(Vec::new));
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

    /// An update: a pane's terminal runs in the old build's host. The new app finds it there
    /// and drives it, starts new terminals in its own host, and the old host ends once its
    /// last terminal has.
    #[test]
    fn a_newer_build_keeps_the_older_hosts_terminals() {
        let dir = tempfile::tempdir().unwrap();
        let (old_ep, new_ep) = (endpoint(dir.path(), "old"), endpoint(dir.path(), "new"));
        let idle = Duration::from_millis(600);
        let ep = old_ep.clone();
        let old = std::thread::spawn(move || {
            crate::pty::host::serve(&ep, idle, "old-build", None).unwrap()
        });
        let ep = new_ep.clone();
        let new = std::thread::spawn(move || {
            crate::pty::host::serve(
                &ep,
                Duration::from_secs(60),
                &crate::pty::host::build(),
                None,
            )
            .unwrap()
        });
        let shell = crate::pty::test_shell(dir.path());

        // Before the update: the old app opened a pane's terminal in the old host.
        let old_app = HostClient::new(old_ep.clone(), None, Arc::new(Recorder::default()));
        connected(&old_app);
        let before = old_app
            .spawn("home", Some("pane-old"), &shell, 80, 24, true)
            .unwrap();
        drop(old_app);

        // After it: the new app.
        let out = Arc::new(Recorder::default());
        let current = HostClient::new(new_ep.clone(), None, out.clone());
        connected(&current);
        let found = old_ep.clone();
        let ptys = Ptys::with_hosts(out.clone(), current, Box::new(move || vec![found.clone()]));
        let a = ptys
            .attach("pane-old")
            .unwrap()
            .expect("found in the old host");
        assert_eq!(a.pty_id, before.pty_id);
        ptys.write(&a.pty_id, "echo gond-still-here\r").unwrap();
        wait_for("output from the old host", || {
            out.out.lock().unwrap().contains("gond-still-here")
        });

        // A new pane's terminal goes to the new host.
        let fresh = ptys
            .spawn("home", Some("pane-new"), &shell, 80, 24, true)
            .unwrap();
        let check = HostClient::new(new_ep.clone(), None, Arc::new(Recorder::default()));
        assert_eq!(
            check.attach("pane-new").unwrap().map(|x| x.pty_id),
            Some(fresh.pty_id.clone())
        );
        drop(check);

        // Its last terminal closed, the old host ends by itself.
        ptys.close(&a.pty_id).unwrap();
        let start = Instant::now();
        while !old.is_finished() {
            assert!(
                start.elapsed() < Duration::from_secs(15),
                "the old host didn't end"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        old.join().unwrap();

        ptys.close_all();
        crate::pty::client::stop(&new_ep).unwrap();
        new.join().unwrap();
    }
}
