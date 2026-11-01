//! Terminal sessions over portable-pty (ConPTY on Windows). Tauri-free: output
//! goes to a `PtySink`, so the manager is tested on Linux without a window.
//!
//! In the app the terminals live in the terminal host (`host.rs`), a background process the
//! window connects to, so they outlive the app; `Ptys` (`hub.rs`) is what the app holds. A
//! terminal opened for a pane carries the pane's id as its key, so a pane coming back after
//! the app restarts finds its terminal (`attach`) and replays what it last printed.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

pub mod client;
pub mod host;
pub mod hub;
pub mod proto;
pub mod scrollback;
pub use hub::Ptys;
pub use scrollback::{PtyRead, Scrollback};

/// `pty.read` timeout: default and ceiling.
pub const READ_TIMEOUT: Duration = Duration::from_secs(10);
pub const READ_TIMEOUT_MAX: Duration = Duration::from_secs(120);

/// How often buffered output is flushed to the UI.
const BATCH: Duration = Duration::from_millis(16);
/// Flush early once this much output is waiting.
const MAX_BATCH_BYTES: usize = 64 * 1024;
/// How much of a terminal's raw output is kept to replay into a pane that attaches later.
const REPLAY_CAP: usize = 2 * 1024 * 1024;

/// Where session output goes (Tauri events in the app, the connected window from the
/// terminal host, a buffer in tests). `end` is the terminal's output length in bytes after
/// `data`: a pane that attaches drops what its replay already holds.
pub trait PtySink: Send + Sync + 'static {
    fn data(&self, pty_id: &str, data: String, end: u64);
    fn exit(&self, pty_id: &str, code: Option<u32>);
}

/// What a terminal runs: the program, its arguments, its folder and its whole environment.
/// The app builds it, so a terminal the host starts gets the app's environment of the moment
/// (today's PATH included), not the host's from whenever it started.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Spawn {
    pub program: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: PathBuf,
    /// The full environment; empty = the environment of whoever runs it.
    #[serde(default)]
    pub env: Vec<(String, String)>,
}

impl Spawn {
    /// `program` in `cwd`, with this process's environment and the registry's PATH as it is now.
    pub fn new(program: impl Into<PathBuf>, cwd: impl Into<PathBuf>) -> Self {
        let mut env: Vec<(String, String)> = std::env::vars()
            .filter(|(k, _)| !k.eq_ignore_ascii_case("PATH"))
            .collect();
        if let Some(path) = crate::process::fresh_path().or_else(|| std::env::var_os("PATH")) {
            env.push(("PATH".into(), path.to_string_lossy().into_owned()));
        }
        Spawn {
            program: program.into(),
            args: vec![],
            cwd: cwd.into(),
            env,
        }
    }

    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    fn command(&self) -> CommandBuilder {
        let mut cmd = CommandBuilder::new(&self.program);
        cmd.args(&self.args);
        cmd.cwd(&self.cwd);
        if !self.env.is_empty() {
            cmd.env_clear();
            for (k, v) in &self.env {
                cmd.env(k, v);
            }
        }
        cmd
    }
}

/// A terminal just started.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Opened {
    pub pty_id: String,
    pub pid: Option<u32>,
}

/// A pane's terminal found again, with what it printed (its last 2 MiB, raw) to draw before
/// what comes next.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Attached {
    pub pty_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    pub replay: String,
    /// The output length (bytes) the replay goes up to; `pty:data` past it is new.
    pub end: u64,
    /// The program has ended; its pane says so.
    pub exited: bool,
}

/// A terminal's raw output: the last `REPLAY_CAP` bytes, starting on a new line where it can.
#[derive(Default)]
struct Replay {
    buf: String,
    end: u64,
}

impl Replay {
    fn push(&mut self, chunk: &str) {
        self.buf.push_str(chunk);
        self.end += chunk.len() as u64;
        if self.buf.len() > REPLAY_CAP {
            let over = self.buf.len() - REPLAY_CAP;
            // A new line, so the replay doesn't start inside an escape sequence.
            let cut = self.buf[ceil_boundary(&self.buf, over)..]
                .find('\n')
                .filter(|i| *i < 64 * 1024)
                .map(|i| ceil_boundary(&self.buf, over) + i + 1)
                .unwrap_or_else(|| ceil_boundary(&self.buf, over));
            self.buf.drain(..cut);
        }
    }
}

fn ceil_boundary(s: &str, i: usize) -> usize {
    (i..=s.len())
        .find(|i| s.is_char_boundary(*i))
        .unwrap_or(s.len())
}

struct Session {
    thread_id: String,
    master: Box<dyn MasterPty + Send>,
    writer: Writer,
    killer: Box<dyn ChildKiller + Send + Sync>,
    /// The program's process id (the shell's, for a plain terminal).
    pid: Option<u32>,
    /// Its process tree (a job on Windows): dropping the session ends everything the program
    /// started, and if the host itself dies, Windows closes the job and ends them too. No
    /// agent or dev server is left running with no pane.
    _tree: Option<crate::proctree::Guard>,
}

type Writer = Arc<Mutex<Box<dyn Write + Send>>>;
type Sessions = Arc<Mutex<HashMap<String, Session>>>;

/// What a console asks when it wants the cursor position (DSR); ConPTY sends it at start
/// and holds back the child's output until someone answers.
const CURSOR_QUERY: &str = "\x1b[6n";
/// The answer xterm.js would give: row 1, column 1.
const CURSOR_REPLY: &[u8] = b"\x1b[1;1R";

/// How many cursor queries `chunk` completes, given the end of the previous chunk
/// (`tail`, updated here) so a query split across reads still counts once.
fn cursor_queries(tail: &mut String, chunk: &str) -> usize {
    let joined = format!("{tail}{chunk}");
    let before = tail.matches(CURSOR_QUERY).count();
    let n = joined.matches(CURSOR_QUERY).count() - before;
    let keep = joined.len().saturating_sub(CURSOR_QUERY.len() - 1);
    let keep = (keep..=joined.len())
        .find(|i| joined.is_char_boundary(*i))
        .unwrap_or(joined.len());
    *tail = joined[keep..].to_string();
    n
}
/// Who answers the console's cursor-position queries (ConPTY holds output until one is).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Cursor {
    /// xterm.js in the UI, attached from the start.
    Terminal,
    /// Gond answers the startup query only; an agent's terminal.
    Startup,
}

/// How many of `queries` new cursor queries Gond answers, `answered` having been so far.
fn replies(cursor: Cursor, answered: usize, queries: usize) -> usize {
    match cursor {
        Cursor::Terminal => 0,
        Cursor::Startup => queries.min(1usize.saturating_sub(answered)),
    }
}

/// What's kept of a terminal until it's closed, running or not.
struct Kept {
    owner: String,
    key: Option<String>,
    scroll: Arc<Scrollback>,
    replay: Arc<Mutex<Replay>>,
    pid: Option<u32>,
}

/// By ptyId.
type Scrolls = Arc<Mutex<HashMap<String, Kept>>>;

#[derive(Clone)]
pub struct PtyManager {
    sessions: Sessions,
    /// Kept after a terminal exits so its last output can still be read; dropped on close.
    scrolls: Scrolls,
    sink: Arc<dyn PtySink>,
}

fn failed(e: impl std::fmt::Display) -> AppError {
    AppError::PtyFailed(e.to_string())
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows: rows.max(1),
        cols: cols.max(1),
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// The user's shell: pwsh → powershell → cmd on Windows, $SHELL (or /bin/sh) elsewhere.
pub fn default_shell() -> PathBuf {
    #[cfg(windows)]
    {
        ["pwsh", "powershell", "cmd"]
            .iter()
            .find_map(|s| which::which(s).ok())
            .unwrap_or_else(|| PathBuf::from("cmd.exe"))
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("SHELL")
            .map(PathBuf::from)
            .unwrap_or_else(|| "/bin/sh".into())
    }
}

impl PtyManager {
    pub fn new(sink: Arc<dyn PtySink>) -> Self {
        PtyManager {
            sessions: Arc::default(),
            scrolls: Arc::default(),
            sink,
        }
    }

    /// Opens the user's shell in `cwd`. Returns the new ptyId.
    pub fn open(&self, thread_id: &str, cwd: &Path, cols: u16, rows: u16) -> AppResult<String> {
        let spawn = Spawn::new(default_shell(), cwd);
        Ok(self
            .spawn(thread_id, None, &spawn, cols, rows, false)?
            .pty_id)
    }

    pub fn open_command(
        &self,
        thread_id: &str,
        cmd: CommandBuilder,
        cols: u16,
        rows: u16,
    ) -> AppResult<String> {
        self.open_inner(thread_id, None, cmd, cols, rows, Cursor::Terminal)
    }

    /// Runs `spawn` for `owner`. With a `key` (a pane's id) the terminal can be found again
    /// with `attach`; one already under that key is closed first (its pane restarted).
    /// `headless`: no xterm.js listens from the start (an agent over the API), so Gond
    /// answers ConPTY's startup cursor query, or the shell's output never comes.
    pub fn spawn(
        &self,
        owner: &str,
        key: Option<&str>,
        spawn: &Spawn,
        cols: u16,
        rows: u16,
        headless: bool,
    ) -> AppResult<Opened> {
        if let Some(old) = key.and_then(|k| self.keyed(k)) {
            let _ = self.close(&old);
        }
        let cursor = if headless {
            Cursor::Startup
        } else {
            Cursor::Terminal
        };
        let id = self.open_inner(owner, key, spawn.command(), cols, rows, cursor)?;
        Ok(Opened {
            pid: self.pid(&id),
            pty_id: id,
        })
    }

    /// The terminal kept under `key`, running or ended.
    fn keyed(&self, key: &str) -> Option<String> {
        self.scrolls()
            .iter()
            .find(|(_, k)| k.key.as_deref() == Some(key))
            .map(|(id, _)| id.clone())
    }

    /// The terminal under `key` (a pane's id) with its output so far, if there is one.
    pub fn attach(&self, key: &str) -> Option<Attached> {
        let scrolls = self.scrolls();
        let (id, kept) = scrolls
            .iter()
            .find(|(_, k)| k.key.as_deref() == Some(key))?;
        // Under the replay's lock: output can't slip between the snapshot and `end`.
        let r = kept.replay.lock().unwrap_or_else(|e| e.into_inner());
        Some(Attached {
            pty_id: id.clone(),
            pid: kept.pid,
            replay: r.buf.clone(),
            end: r.end,
            exited: !self.is_live(id),
        })
    }

    /// Closes every keyed terminal whose key isn't in `keep`: panes that are gone (closed
    /// while the app couldn't save them). Returns how many.
    pub fn prune(&self, keep: &[String]) -> usize {
        let gone: Vec<String> = self
            .scrolls()
            .iter()
            .filter(|(_, k)| k.key.as_ref().is_some_and(|key| !keep.contains(key)))
            .map(|(id, _)| id.clone())
            .collect();
        for id in &gone {
            let _ = self.close(id);
        }
        gone.len()
    }

    fn open_inner(
        &self,
        thread_id: &str,
        key: Option<&str>,
        mut cmd: CommandBuilder,
        cols: u16,
        rows: u16,
        cursor: Cursor,
    ) -> AppResult<String> {
        for var in crate::process::parent_agent_vars() {
            cmd.env_remove(var);
        }
        // Every pane is a full-colour terminal (WebGL, 24-bit). Say so, and don't pass on a NO_COLOR the app
        // only inherited from whatever launched it: the agents would draw in black and white.
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env_remove("NO_COLOR");
        let pair = native_pty_system()
            .openpty(size(cols, rows))
            .map_err(failed)?;
        let mut child = pair.slave.spawn_command(cmd).map_err(failed)?;
        // The child holds the slave now; ours must go or the reader never sees EOF.
        drop(pair.slave);
        let reader = pair.master.try_clone_reader().map_err(failed)?;
        let writer: Writer = Arc::new(Mutex::new(pair.master.take_writer().map_err(failed)?));
        let id = ulid::Ulid::generate().to_string();

        let (done_tx, done_rx) = mpsc::channel();
        let scroll = Arc::new(Scrollback::default());
        let replay = Arc::new(Mutex::new(Replay::default()));
        self.scrolls().insert(
            id.clone(),
            Kept {
                owner: thread_id.to_string(),
                key: key.map(str::to_string),
                scroll: scroll.clone(),
                replay: replay.clone(),
                pid: child.process_id(),
            },
        );
        let (sink, rid, rscroll) = (self.sink.clone(), id.clone(), scroll.clone());
        let reply = (cursor != Cursor::Terminal).then(|| writer.clone());
        std::thread::spawn(move || {
            let mut tail = String::new();
            let mut answered = 0usize;
            pump(reader, |chunk| {
                if let Some(w) = &reply {
                    let n = replies(cursor, answered, cursor_queries(&mut tail, &chunk));
                    answered += n;
                    for _ in 0..n {
                        let mut w = w.lock().unwrap_or_else(|e| e.into_inner());
                        let _ = w.write_all(CURSOR_REPLY).and_then(|()| w.flush());
                    }
                }
                rscroll.push(&chunk);
                // Sent under the replay's lock, so `attach` sees each chunk in exactly one of
                // its replay and what follows it.
                let mut r = replay.lock().unwrap_or_else(|e| e.into_inner());
                r.push(&chunk);
                sink.data(&rid, chunk, r.end)
            });
            let _ = done_tx.send(());
        });

        self.lock().insert(
            id.clone(),
            Session {
                thread_id: thread_id.into(),
                master: pair.master,
                writer,
                killer: child.clone_killer(),
                pid: child.process_id(),
                _tree: child.process_id().map(crate::proctree::contain_pid),
            },
        );

        let (sink, sessions, wid) = (self.sink.clone(), self.sessions.clone(), id.clone());
        std::thread::spawn(move || {
            let code = child.wait().ok().map(|s| s.exit_code());
            // Dropping the master closes ConPTY, which ends the reader on Windows.
            sessions
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&wid);
            // Let the last output through before saying it exited; don't hang on a
            // grandchild that keeps the terminal open.
            let _ = done_rx.recv_timeout(Duration::from_secs(1));
            scroll.exit(code);
            sink.exit(&wid, code);
        });
        Ok(id)
    }

    pub fn write(&self, pty_id: &str, data: &str) -> AppResult<()> {
        let mut sessions = self.lock();
        let s = sessions.get_mut(pty_id).ok_or_else(|| not_found(pty_id))?;
        let mut w = s.writer.lock().unwrap_or_else(|e| e.into_inner());
        w.write_all(data.as_bytes()).map_err(failed)?;
        w.flush().map_err(failed)
    }

    pub fn resize(&self, pty_id: &str, cols: u16, rows: u16) -> AppResult<()> {
        let sessions = self.lock();
        let s = sessions.get(pty_id).ok_or_else(|| not_found(pty_id))?;
        s.master.resize(size(cols, rows)).map_err(failed)
    }

    /// Plain-text output after `since`; with `wait_for`, blocks until it matches,
    /// the terminal exits, or `timeout` (capped at 120 s) runs out.
    pub fn read(
        &self,
        pty_id: &str,
        since: Option<u64>,
        wait_for: Option<&str>,
        timeout: Option<Duration>,
    ) -> AppResult<PtyRead> {
        let re = wait_for
            .map(regex::Regex::new)
            .transpose()
            .map_err(|e| AppError::Invalid(format!("waitFor: {e}")))?;
        let scroll = self
            .scrolls()
            .get(pty_id)
            .map(|k| k.scroll.clone())
            .ok_or_else(|| not_found(pty_id))?;
        let timeout = timeout.unwrap_or(READ_TIMEOUT).min(READ_TIMEOUT_MAX);
        Ok(scroll.read(since, re.as_ref(), timeout))
    }

    /// Every running terminal's program's process id.
    pub fn pids(&self) -> Vec<u32> {
        self.lock().values().filter_map(|s| s.pid).collect()
    }

    /// The terminal's program's process id, while it runs.
    pub fn pid(&self, pty_id: &str) -> Option<u32> {
        self.lock().get(pty_id).and_then(|s| s.pid)
    }

    /// Whether the terminal's program is still running.
    pub fn is_live(&self, pty_id: &str) -> bool {
        self.lock().contains_key(pty_id)
    }

    /// Whether this manager holds `pty_id`, running or ended.
    pub fn holds(&self, pty_id: &str) -> bool {
        self.scrolls().contains_key(pty_id)
    }

    /// Kills the shell; `pty:exit` follows from its waiter.
    pub fn close(&self, pty_id: &str) -> AppResult<()> {
        let scroll = self.scrolls().remove(pty_id);
        match self.lock().remove(pty_id) {
            Some(mut s) => {
                let _ = s.killer.kill();
                Ok(())
            }
            // Already exited: closing just drops its scrollback.
            None if scroll.is_some() => Ok(()),
            None => Err(not_found(pty_id)),
        }
    }

    /// Closes every session of a thread (on archive).
    pub fn close_thread(&self, thread_id: &str) {
        self.scrolls().retain(|_, k| k.owner != thread_id);
        self.close_where(|s| s.thread_id == thread_id);
    }

    /// Closes everything (on app exit).
    pub fn close_all(&self) {
        self.scrolls().clear();
        self.close_where(|_| true);
    }

    fn close_where(&self, pred: impl Fn(&Session) -> bool) {
        let mut sessions = self.lock();
        let ids: Vec<_> = sessions
            .iter()
            .filter(|(_, s)| pred(s))
            .map(|(k, _)| k.clone())
            .collect();
        for id in ids {
            if let Some(mut s) = sessions.remove(&id) {
                let _ = s.killer.kill();
            }
        }
    }

    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Session>> {
        self.sessions.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn scrolls(&self) -> std::sync::MutexGuard<'_, HashMap<String, Kept>> {
        self.scrolls.lock().unwrap_or_else(|e| e.into_inner())
    }
}

fn not_found(pty_id: &str) -> AppError {
    AppError::NotFound(format!("terminal {pty_id} not found"))
}

/// Reads until EOF, emitting UTF-8 text in batches of at most ~16 ms. A
/// multi-byte character split across reads is held back until it is whole.
fn pump(mut reader: Box<dyn Read + Send>, mut emit: impl FnMut(String)) {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });
    let mut pending: Vec<u8> = Vec::new();
    let mut since = Instant::now();
    loop {
        let open = match rx.recv_timeout(BATCH) {
            Ok(bytes) => {
                if pending.is_empty() {
                    since = Instant::now();
                }
                pending.extend(bytes);
                true
            }
            Err(RecvTimeoutError::Timeout) => true,
            Err(RecvTimeoutError::Disconnected) => false,
        };
        if !pending.is_empty()
            && (!open || since.elapsed() >= BATCH || pending.len() >= MAX_BATCH_BYTES)
        {
            let cut = if open {
                utf8_boundary(&pending)
            } else {
                pending.len()
            };
            if cut > 0 {
                let rest = pending.split_off(cut);
                emit(String::from_utf8_lossy(&pending).into_owned());
                pending = rest;
            }
            since = Instant::now();
        }
        if !open {
            break;
        }
    }
}

/// Length of the longest prefix that doesn't end inside a UTF-8 sequence.
fn utf8_boundary(b: &[u8]) -> usize {
    let start = b.len().saturating_sub(3);
    for i in (start..b.len()).rev() {
        let byte = b[i];
        if byte & 0b1100_0000 != 0b1000_0000 {
            let need = match byte {
                0x00..=0x7f => 1,
                0xc0..=0xdf => 2,
                0xe0..=0xef => 3,
                _ => 4,
            };
            return if b.len() - i >= need { b.len() } else { i };
        }
    }
    b.len()
}

/// A shell for tests on either OS: `sh` on Unix, `cmd` on Windows; `script` runs and the
/// shell stays open.
#[cfg(test)]
pub(crate) fn test_shell(cwd: &Path) -> Spawn {
    if cfg!(windows) {
        Spawn::new("cmd.exe", cwd).args(["/q", "/k"])
    } else {
        Spawn::new("/bin/sh", cwd)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[derive(Default)]
    pub(crate) struct Recorder {
        pub out: Mutex<String>,
        pub ends: Mutex<Vec<u64>>,
        pub exits: Mutex<Vec<(String, Option<u32>)>>,
    }

    impl PtySink for Recorder {
        fn data(&self, _: &str, data: String, end: u64) {
            self.out.lock().unwrap().push_str(&data);
            self.ends.lock().unwrap().push(end);
        }
        fn exit(&self, id: &str, code: Option<u32>) {
            self.exits.lock().unwrap().push((id.into(), code));
        }
    }

    pub(crate) fn wait_for(what: &str, mut f: impl FnMut() -> bool) {
        let start = Instant::now();
        while !f() {
            assert!(
                start.elapsed() < Duration::from_secs(20),
                "timed out waiting for {what}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn utf8_boundaries() {
        assert_eq!(utf8_boundary(b"abc"), 3);
        let e = "é".as_bytes(); // 2 bytes
        assert_eq!(utf8_boundary(&[b'a', e[0]]), 1);
        assert_eq!(utf8_boundary(&[b'a', e[0], e[1]]), 3);
        let snow = "☃".as_bytes(); // 3 bytes
        assert_eq!(utf8_boundary(&snow[..2]), 0);
    }

    #[test]
    fn replay_keeps_the_end_and_starts_on_a_line() {
        let mut r = Replay::default();
        let line = format!("{}\n", "x".repeat(1023));
        for _ in 0..(REPLAY_CAP / 1024 + 10) {
            r.push(&line);
        }
        assert!(r.buf.len() <= REPLAY_CAP);
        assert!(r.buf.starts_with('x') && r.buf.ends_with('\n'));
        assert_eq!(r.end, (REPLAY_CAP / 1024 + 10) as u64 * 1024);
    }

    #[cfg(unix)]
    #[test]
    fn echoes_through_a_pty_and_reports_exit() {
        let rec = Arc::new(Recorder::default());
        let mgr = PtyManager::new(rec.clone());
        let dir = tempfile::tempdir().unwrap();
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.cwd(dir.path());
        let id = mgr.open_command("t1", cmd, 80, 24).unwrap();
        assert_eq!(mgr.len(), 1);

        mgr.resize(&id, 120, 40).unwrap();
        mgr.write(&id, "echo $((40+2)) ☃; pwd\n").unwrap();
        let cwd = dir.path().canonicalize().unwrap().display().to_string();
        wait_for("echo output", || {
            let out = rec.out.lock().unwrap();
            out.contains("42 ☃") && out.contains(&cwd)
        });

        let r = mgr
            .read(&id, None, Some(r"\b42 ☃"), Some(Duration::from_secs(10)))
            .unwrap();
        assert_eq!(r.matched, Some(true));
        assert!(!r.text.contains('\x1b'), "ANSI stripped: {:?}", r.text);
        let since = r.offset;

        mgr.write(&id, "exit 3\n").unwrap();
        wait_for("exit", || !rec.exits.lock().unwrap().is_empty());
        assert_eq!(rec.exits.lock().unwrap()[0], (id.clone(), Some(3)));
        assert!(mgr.is_empty());
        assert_eq!(mgr.write(&id, "x").unwrap_err().code(), "not_found");
        // Output stays readable after exit, until the terminal is closed.
        let r = mgr.read(&id, Some(since), None, None).unwrap();
        assert_eq!((r.exited, r.exit_code), (true, Some(3)));
        assert_eq!(
            mgr.read(&id, None, Some("("), None).unwrap_err().code(),
            "invalid"
        );
        mgr.close(&id).unwrap();
        assert_eq!(
            mgr.read(&id, None, None, None).unwrap_err().code(),
            "not_found"
        );
    }

    /// A pane coming back finds its terminal by key with what it printed; output after the
    /// replay's `end` is what's new; a restart replaces it; prune closes the orphans.
    #[test]
    fn keyed_terminals_attach_replay_and_prune() {
        let rec = Arc::new(Recorder::default());
        let mgr = PtyManager::new(rec.clone());
        let dir = tempfile::tempdir().unwrap();
        let shell = test_shell(dir.path());
        let a = mgr
            .spawn("home", Some("pane-a"), &shell, 80, 24, true)
            .unwrap();
        let b = mgr
            .spawn("home", Some("pane-b"), &shell, 80, 24, true)
            .unwrap();
        assert!(a.pid.is_some());
        mgr.write(&a.pty_id, "echo gond-attach\r").unwrap();
        wait_for("echo", || {
            mgr.attach("pane-a")
                .is_some_and(|x| x.replay.matches("gond-attach").count() >= 2)
        });
        let at = mgr.attach("pane-a").unwrap();
        assert_eq!(at.pty_id, a.pty_id);
        assert!(!at.exited);
        assert!(at.end >= at.replay.len() as u64);
        assert!(mgr.attach("pane-z").is_none());

        // The pane restarted: a new terminal under the same key, the old one closed.
        let a2 = mgr
            .spawn("home", Some("pane-a"), &shell, 80, 24, true)
            .unwrap();
        assert_ne!(a2.pty_id, a.pty_id);
        assert!(!mgr.holds(&a.pty_id));
        assert_eq!(mgr.attach("pane-a").unwrap().pty_id, a2.pty_id);

        assert_eq!(mgr.prune(&["pane-a".into()]), 1);
        assert!(!mgr.holds(&b.pty_id));
        assert!(mgr.holds(&a2.pty_id));
        mgr.close_all();
    }

    /// Closing a pane ends what its shell started, not just the shell: no dev server or agent
    /// is left running with nowhere to see it.
    #[test]
    fn closing_a_terminal_ends_what_it_started() {
        let mgr = PtyManager::new(Arc::new(Recorder::default()));
        let dir = tempfile::tempdir().unwrap();
        let o = mgr
            .spawn("home", None, &test_shell(dir.path()), 120, 24, true)
            .unwrap();
        let start = if cfg!(windows) {
            "powershell -NoProfile -Command \"$p = Start-Process ping -ArgumentList '-n','120','127.0.0.1' -PassThru -WindowStyle Hidden; 'PID=' + $p.Id\"\r"
        } else {
            "sleep 120 & echo PID=$!\n"
        };
        mgr.write(&o.pty_id, start).unwrap();
        let r = mgr
            .read(
                &o.pty_id,
                None,
                Some(r"PID=\d+"),
                Some(Duration::from_secs(60)),
            )
            .unwrap();
        assert_eq!(r.matched, Some(true), "{}", r.text);
        let pid: u32 = regex::Regex::new(r"PID=(\d+)")
            .unwrap()
            .captures_iter(&r.text)
            .last()
            .and_then(|c| c[1].parse().ok())
            .unwrap();
        assert!(crate::proctree::tests::alive(pid), "the grandchild runs");
        mgr.close(&o.pty_id).unwrap();
        assert!(
            crate::proctree::tests::gone_soon(pid),
            "closing the pane ended it"
        );
    }

    #[cfg(unix)]
    #[test]
    fn close_thread_and_close_all_kill_sessions() {
        let rec = Arc::new(Recorder::default());
        let mgr = PtyManager::new(rec.clone());
        let open = |t| {
            mgr.open_command(t, CommandBuilder::new("/bin/sh"), 80, 24)
                .unwrap()
        };
        let (a1, _a2, _b) = (open("a"), open("a"), open("b"));
        mgr.close_thread("a");
        assert_eq!(mgr.len(), 1);
        mgr.close_all();
        assert!(mgr.is_empty());
        wait_for("three exits", || rec.exits.lock().unwrap().len() == 3);
        assert_eq!(mgr.close(&a1).unwrap_err().code(), "not_found");
    }

    #[test]
    fn cursor_queries_are_counted_once_even_when_split() {
        let mut tail = String::new();
        assert_eq!(cursor_queries(&mut tail, "hello\x1b[6n"), 1);
        assert_eq!(cursor_queries(&mut tail, "more"), 0);
        assert_eq!(cursor_queries(&mut tail, "x\x1b["), 0);
        assert_eq!(
            cursor_queries(&mut tail, "6nY\x1b[6n"),
            2,
            "split across reads, then another"
        );
        assert_eq!(cursor_queries(&mut tail, "é\x1b[6"), 0);
        assert_eq!(cursor_queries(&mut tail, "n"), 1);
    }

    /// an agent's terminal gets the startup query answered (or ConPTY would hold
    /// every byte back), and only that one, so a human taking over later isn't
    /// double-answered.
    #[test]
    fn only_the_startup_query_is_answered_for_an_agent() {
        assert_eq!(replies(Cursor::Startup, 0, 1), 1);
        assert_eq!(replies(Cursor::Startup, 0, 2), 1, "two in one chunk");
        assert_eq!(replies(Cursor::Startup, 1, 1), 0, "later ones");
        assert_eq!(replies(Cursor::Terminal, 0, 1), 0, "xterm.js answers");
    }

    #[cfg(unix)]
    #[test]
    fn an_agents_terminal_gets_its_startup_query_answered() {
        let mgr = PtyManager::new(Arc::new(Recorder::default()));
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.args(["-c", "stty raw -echo; printf '\\033[6n'; dd bs=1 count=6 2>/dev/null | od -c | head -1; echo; echo done"]);
        let id = mgr
            .open_inner("t1", None, cmd, 80, 24, Cursor::Startup)
            .unwrap();
        let out = mgr
            .read(&id, None, Some("done"), Some(Duration::from_secs(10)))
            .unwrap();
        assert_eq!(out.matched, Some(true), "{}", out.text);
        assert!(out.text.contains("1   ;   1   R"), "{}", out.text);
    }
}
