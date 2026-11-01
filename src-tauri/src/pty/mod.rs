//! Terminal sessions over portable-pty (ConPTY on Windows). Tauri-free: output
//! goes to a `PtySink`, so the manager is tested on Linux without a window.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};

use crate::error::{AppError, AppResult};

pub mod scrollback;
pub use scrollback::{PtyRead, Scrollback};

/// `pty.read` timeout: default and ceiling.
pub const READ_TIMEOUT: Duration = Duration::from_secs(10);
pub const READ_TIMEOUT_MAX: Duration = Duration::from_secs(120);

/// How often buffered output is flushed to the UI.
const BATCH: Duration = Duration::from_millis(16);
/// Flush early once this much output is waiting.
const MAX_BATCH_BYTES: usize = 64 * 1024;

/// Where session output goes (Tauri events in the app, a buffer in tests).
pub trait PtySink: Send + Sync + 'static {
    fn data(&self, pty_id: &str, data: String);
    fn exit(&self, pty_id: &str, code: Option<u32>);
}

struct Session {
    thread_id: String,
    master: Box<dyn MasterPty + Send>,
    writer: Writer,
    killer: Box<dyn ChildKiller + Send + Sync>,
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
    /// Gond answers every one; nobody ever looks (background sign-in).
    Always,
}

/// How many of `queries` new cursor queries Gond answers, `answered` having been so far.
fn replies(cursor: Cursor, answered: usize, queries: usize) -> usize {
    match cursor {
        Cursor::Terminal => 0,
        Cursor::Startup => queries.min(1usize.saturating_sub(answered)),
        Cursor::Always => queries,
    }
}

/// By ptyId: (threadId, scrollback).
type Scrolls = Arc<Mutex<HashMap<String, (String, Arc<Scrollback>)>>>;

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
        self.open_shell(thread_id, cwd, cols, rows, Cursor::Terminal)
    }

    /// `open` for a caller with no xterm.js attached at the start (an agent over MCP,
    /// ): Gond answers ConPTY's startup cursor query, or the shell's output never
    /// comes. Later queries are left to whatever terminal the human attaches.
    pub fn open_headless(
        &self,
        thread_id: &str,
        cwd: &Path,
        cols: u16,
        rows: u16,
    ) -> AppResult<String> {
        self.open_shell(thread_id, cwd, cols, rows, Cursor::Startup)
    }

    fn open_shell(
        &self,
        thread_id: &str,
        cwd: &Path,
        cols: u16,
        rows: u16,
        cursor: Cursor,
    ) -> AppResult<String> {
        let mut cmd = CommandBuilder::new(default_shell());
        cmd.cwd(cwd);
        cmd.env("TERM", "xterm-256color");
        // The registry's PATH as it is now, so tools installed since startup work in new terminals.
        if let Some(path) = crate::process::fresh_path() {
            cmd.env("PATH", path);
        }
        self.open_inner(thread_id, cmd, cols, rows, cursor)
    }

    pub fn open_command(
        &self,
        thread_id: &str,
        cmd: CommandBuilder,
        cols: u16,
        rows: u16,
    ) -> AppResult<String> {
        self.open_inner(thread_id, cmd, cols, rows, Cursor::Terminal)
    }

    /// A terminal nobody is looking at (a background sign-in): Gond answers the
    /// console's cursor queries itself, as xterm.js would, or ConPTY never lets the
    /// child's output through.
    pub fn open_background(
        &self,
        thread_id: &str,
        cmd: CommandBuilder,
        cols: u16,
        rows: u16,
    ) -> AppResult<String> {
        self.open_inner(thread_id, cmd, cols, rows, Cursor::Always)
    }

    fn open_inner(
        &self,
        thread_id: &str,
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
        self.scrolls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.clone(), (thread_id.to_string(), scroll.clone()));
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
                sink.data(&rid, chunk)
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
            .scrolls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(pty_id)
            .map(|(_, s)| s.clone())
            .ok_or_else(|| not_found(pty_id))?;
        let timeout = timeout.unwrap_or(READ_TIMEOUT).min(READ_TIMEOUT_MAX);
        Ok(scroll.read(since, re.as_ref(), timeout))
    }

    /// Whether the terminal's program is still running.
    pub fn is_live(&self, pty_id: &str) -> bool {
        self.lock().contains_key(pty_id)
    }

    /// Kills the shell; `pty:exit` follows from its waiter.
    pub fn close(&self, pty_id: &str) -> AppResult<()> {
        let scroll = self
            .scrolls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(pty_id);
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
        self.scrolls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, (t, _)| t != thread_id);
        self.close_where(|s| s.thread_id == thread_id);
    }

    /// Closes everything (on app exit).
    pub fn close_all(&self) {
        self.scrolls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
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

#[cfg(test)]
mod tests {
    use super::*;

    // The PTY tests drive /bin/sh, so they and their helpers are Unix only.
    #[cfg(unix)]
    #[derive(Default)]
    struct Recorder {
        out: Mutex<String>,
        exits: Mutex<Vec<(String, Option<u32>)>>,
    }

    #[cfg(unix)]
    impl PtySink for Recorder {
        fn data(&self, _: &str, data: String) {
            self.out.lock().unwrap().push_str(&data);
        }
        fn exit(&self, id: &str, code: Option<u32>) {
            self.exits.lock().unwrap().push((id.into(), code));
        }
    }

    #[cfg(unix)]
    fn wait_for(what: &str, mut f: impl FnMut() -> bool) {
        let start = Instant::now();
        while !f() {
            assert!(
                start.elapsed() < Duration::from_secs(10),
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

    /// a background terminal answers the console's cursor query itself, so a
    /// program that waits for the answer (as ConPTY does on Windows) carries on.
    #[cfg(unix)]
    #[test]
    fn a_background_terminal_answers_cursor_queries() {
        let mgr = PtyManager::new(Arc::new(Recorder::default()));
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.args(["-c", "stty raw -echo; printf '\\033[6n'; dd bs=1 count=6 2>/dev/null | od -c | head -1; echo; echo done"]);
        let id = mgr.open_background("t1", cmd, 80, 24).unwrap();
        let out = mgr
            .read(&id, None, Some("done"), Some(Duration::from_secs(10)))
            .unwrap();
        assert_eq!(out.matched, Some(true), "{}", out.text);
        assert!(out.text.contains("1   ;   1   R"), "{}", out.text);
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
        assert_eq!(replies(Cursor::Always, 3, 2), 2);
    }

    #[cfg(unix)]
    #[test]
    fn an_agents_terminal_gets_its_startup_query_answered() {
        let mgr = PtyManager::new(Arc::new(Recorder::default()));
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.args(["-c", "stty raw -echo; printf '\\033[6n'; dd bs=1 count=6 2>/dev/null | od -c | head -1; echo; echo done"]);
        let id = mgr.open_inner("t1", cmd, 80, 24, Cursor::Startup).unwrap();
        let out = mgr
            .read(&id, None, Some("done"), Some(Duration::from_secs(10)))
            .unwrap();
        assert_eq!(out.matched, Some(true), "{}", out.text);
        assert!(out.text.contains("1   ;   1   R"), "{}", out.text);
    }
}
