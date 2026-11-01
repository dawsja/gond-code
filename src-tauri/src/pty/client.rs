//! The app's side of the terminal host (`host.rs`): it connects (starting a host if none
//! runs), sends requests and waits for their answers, and passes the host's output on to the
//! app's `PtySink` as if the terminals were its own.
//!
//! If the host goes away, every terminal it held has ended: each one the app knew of gets its
//! `exit`, and the next request starts a new host.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc as std_mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

use super::proto::{line, Envelope, Failure, FromHost, Request, PROTOCOL};
use super::{Attached, Opened, PtyRead, PtySink, Spawn};
use crate::error::{AppError, AppResult};

/// How long a request waits for its answer (a `read` waits its own timeout on top).
const ANSWER: Duration = Duration::from_secs(15);
/// How long a host just started has to come up.
const START: Duration = Duration::from_secs(8);

type Answer = Result<Value, Failure>;
type Launch = Box<dyn Fn() -> std::io::Result<()> + Send + Sync>;

struct Conn {
    tx: mpsc::UnboundedSender<String>,
    alive: Arc<AtomicBool>,
}

pub struct HostClient {
    endpoint: String,
    /// Starts a host when none answers; `None` only connects (tests, `stop`).
    launch: Option<Launch>,
    sink: Arc<dyn PtySink>,
    rt: tokio::runtime::Runtime,
    conn: Mutex<Option<Conn>>,
    pending: Arc<Mutex<HashMap<u64, std_mpsc::Sender<Answer>>>>,
    /// The terminals this app has opened or attached, still running.
    known: Arc<Mutex<HashSet<String>>>,
    next: AtomicU64,
}

impl HostClient {
    pub fn new(endpoint: String, launch: Option<Launch>, sink: Arc<dyn PtySink>) -> Self {
        HostClient {
            endpoint,
            launch,
            sink,
            rt: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .thread_name("pty-host-client")
                .enable_all()
                .build()
                .expect("a tokio runtime for the terminal host"),
            conn: Mutex::new(None),
            pending: Arc::default(),
            known: Arc::default(),
            next: AtomicU64::new(1),
        }
    }

    /// This user's host for this channel, started from a copy of this exe when needed.
    pub fn for_app(sink: Arc<dyn PtySink>) -> Self {
        HostClient::new(
            super::host::endpoint(),
            Some(Box::new(super::host::launch)),
            sink,
        )
    }

    /// The host this client talks to.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Connects now (starting a host if needed); a connection that's up is kept.
    pub fn connect(&self) -> AppResult<()> {
        self.sender().map(|_| ())
    }

    /// Whether a connection is up.
    pub fn connected(&self) -> bool {
        self.conn
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .is_some_and(|c| c.alive.load(Ordering::SeqCst))
    }

    /// Whether this app opened or attached `pty_id` through this host, and it still runs.
    pub fn knows(&self, pty_id: &str) -> bool {
        lock(&self.known).contains(pty_id)
    }

    /// Drops the connection; the host keeps every terminal running.
    pub fn disconnect(&self) {
        lock(&self.known).clear();
        self.conn.lock().unwrap_or_else(|e| e.into_inner()).take();
    }

    fn sender(&self) -> AppResult<mpsc::UnboundedSender<String>> {
        let mut conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(c) = conn.as_ref().filter(|c| c.alive.load(Ordering::SeqCst)) {
            return Ok(c.tx.clone());
        }
        let stream = self.open_stream()?;
        let c = self.start(stream);
        let tx = c.tx.clone();
        *conn = Some(c);
        drop(conn);
        let hello: Value = self.ask_on(
            &tx,
            Request::Hello {
                build: Some(super::host::build()),
            },
            ANSWER,
        )?;
        // Any host from protocol 1 on: requests are only ever added.
        if hello["protocol"].as_u64().is_none_or(|p| p < 1) {
            self.disconnect();
            return Err(AppError::PtyFailed(format!(
                "the terminal host speaks protocol {}, not {PROTOCOL}",
                hello["protocol"]
            )));
        }
        Ok(tx)
    }

    /// Connects, or starts a host and waits for it to come up.
    fn open_stream(&self) -> AppResult<Stream> {
        if let Ok(s) = self.dial() {
            return Ok(s);
        }
        let Some(launch) = &self.launch else {
            return Err(unreachable_host("no terminal host is running"));
        };
        launch()
            .map_err(|e| unreachable_host(&format!("couldn't start the terminal host: {e}")))?;
        let start = Instant::now();
        loop {
            match self.dial() {
                Ok(s) => return Ok(s),
                Err(e) if start.elapsed() > START => {
                    return Err(unreachable_host(&format!(
                        "the terminal host didn't start: {e}"
                    )))
                }
                Err(_) => std::thread::sleep(Duration::from_millis(50)),
            }
        }
    }

    /// One connection attempt, on a plain thread (a caller may be inside a runtime).
    fn dial(&self) -> std::io::Result<Stream> {
        let endpoint = self.endpoint.clone();
        let handle = self.rt.handle().clone();
        std::thread::spawn(move || handle.block_on(dial(&endpoint)))
            .join()
            .map_err(|_| std::io::Error::other("connect thread panicked"))?
    }

    /// Starts reading the host's messages and writing ours over `stream`.
    fn start(&self, stream: Stream) -> Conn {
        let (rd, mut wr) = tokio::io::split(stream);
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let alive = Arc::new(AtomicBool::new(true));
        self.rt.spawn(async move {
            while let Some(msg) = rx.recv().await {
                if wr.write_all(msg.as_bytes()).await.is_err() {
                    break;
                }
                if rx.is_empty() && wr.flush().await.is_err() {
                    break;
                }
            }
        });
        let (sink, pending, known, up) = (
            self.sink.clone(),
            self.pending.clone(),
            self.known.clone(),
            alive.clone(),
        );
        self.rt.spawn(async move {
            let mut lines = BufReader::new(rd).lines();
            while let Ok(Some(text)) = lines.next_line().await {
                match serde_json::from_str::<FromHost>(&text) {
                    Ok(FromHost::Reply { id, ok, err }) => {
                        if let Some(tx) = lock(&pending).remove(&id) {
                            let _ = tx.send(err.map_or(Ok(ok), Err));
                        }
                    }
                    Ok(FromHost::Data { pty_id, data, end }) => sink.data(&pty_id, data, end),
                    Ok(FromHost::Exit { pty_id, code }) => {
                        lock(&known).remove(&pty_id);
                        sink.exit(&pty_id, code);
                    }
                    Err(_) => {}
                }
            }
            // The host is gone: every terminal the app knew of ended with it.
            up.store(false, Ordering::SeqCst);
            for (_, tx) in lock(&pending).drain() {
                let _ = tx.send(Err(Failure {
                    code: "pty_failed".into(),
                    message: "the terminal host stopped".into(),
                }));
            }
            let gone: Vec<String> = lock(&known).drain().collect();
            for id in gone {
                sink.exit(&id, None);
            }
        });
        Conn { tx, alive }
    }

    /// Sends `req` and waits for its answer.
    fn ask<T: DeserializeOwned>(&self, req: Request, wait: Duration) -> AppResult<T> {
        let tx = self.sender()?;
        self.ask_on(&tx, req, wait)
    }

    fn ask_on<T: DeserializeOwned>(
        &self,
        tx: &mpsc::UnboundedSender<String>,
        req: Request,
        wait: Duration,
    ) -> AppResult<T> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (atx, arx) = std_mpsc::channel();
        lock(&self.pending).insert(id, atx);
        if tx.send(line(&Envelope { id, req })).is_err() {
            lock(&self.pending).remove(&id);
            return Err(AppError::PtyFailed("the terminal host stopped".into()));
        }
        match arx.recv_timeout(wait) {
            Ok(Ok(v)) => serde_json::from_value(v)
                .map_err(|e| AppError::PtyFailed(format!("terminal host: {e}"))),
            Ok(Err(f)) => Err(f.into()),
            Err(_) => {
                lock(&self.pending).remove(&id);
                Err(AppError::PtyFailed(
                    "the terminal host didn't answer".into(),
                ))
            }
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
        let o: Opened = self.ask(
            Request::Spawn {
                owner: owner.into(),
                key: key.map(str::to_string),
                spawn: spawn.clone(),
                cols,
                rows,
                headless,
            },
            ANSWER,
        )?;
        lock(&self.known).insert(o.pty_id.clone());
        Ok(o)
    }

    pub fn attach(&self, key: &str) -> AppResult<Option<Attached>> {
        let a: Option<Attached> = self.ask(Request::Attach { key: key.into() }, ANSWER)?;
        if let Some(a) = a.as_ref().filter(|a| !a.exited) {
            lock(&self.known).insert(a.pty_id.clone());
        }
        Ok(a)
    }

    pub fn write(&self, pty_id: &str, data: &str) -> AppResult<()> {
        self.ask::<Value>(
            Request::Write {
                pty_id: pty_id.into(),
                data: data.into(),
            },
            ANSWER,
        )
        .map(|_| ())
    }

    pub fn resize(&self, pty_id: &str, cols: u16, rows: u16) -> AppResult<()> {
        self.ask::<Value>(
            Request::Resize {
                pty_id: pty_id.into(),
                cols,
                rows,
            },
            ANSWER,
        )
        .map(|_| ())
    }

    pub fn read(
        &self,
        pty_id: &str,
        since: Option<u64>,
        wait_for: Option<&str>,
        timeout: Option<Duration>,
    ) -> AppResult<PtyRead> {
        let wait = timeout
            .unwrap_or(super::READ_TIMEOUT)
            .min(super::READ_TIMEOUT_MAX);
        self.ask(
            Request::Read {
                pty_id: pty_id.into(),
                since,
                wait_for: wait_for.map(str::to_string),
                timeout_ms: timeout.map(|t| t.as_millis() as u64),
            },
            wait + ANSWER,
        )
    }

    pub fn pid(&self, pty_id: &str) -> Option<u32> {
        self.ask(
            Request::Pid {
                pty_id: pty_id.into(),
            },
            ANSWER,
        )
        .ok()
        .flatten()
    }

    pub fn pids(&self) -> Vec<u32> {
        if !self.connected() {
            return vec![];
        }
        self.ask(Request::Pids, ANSWER).unwrap_or_default()
    }

    pub fn close(&self, pty_id: &str) -> AppResult<()> {
        lock(&self.known).remove(pty_id);
        self.ask::<Value>(
            Request::Close {
                pty_id: pty_id.into(),
            },
            ANSWER,
        )
        .map(|_| ())
    }

    pub fn close_all(&self) {
        if self.connected() {
            let _ = self.ask::<Value>(Request::CloseAll, ANSWER);
        }
        lock(&self.known).clear();
    }

    pub fn close_owner(&self, owner: &str) -> AppResult<()> {
        self.ask::<Value>(
            Request::CloseOwner {
                owner: owner.into(),
            },
            ANSWER,
        )
        .map(|_| ())
    }

    pub fn prune(&self, keep: &[String]) -> AppResult<usize> {
        self.ask(
            Request::Prune {
                keep: keep.to_vec(),
            },
            ANSWER,
        )
    }

    /// Asks the host to close everything and end.
    pub fn shutdown(&self) -> AppResult<()> {
        self.ask::<Value>(Request::Shutdown, ANSWER).map(|_| ())
    }
}

impl Drop for HostClient {
    fn drop(&mut self) {
        self.disconnect();
    }
}

/// The error for "no host to talk to": the app then runs terminals itself.
fn unreachable_host(msg: &str) -> AppError {
    AppError::PtyFailed(format!("unreachable: {msg}"))
}

/// Whether `e` means the host couldn't be reached at all (rather than a request failing).
pub fn is_unreachable(e: &AppError) -> bool {
    matches!(e, AppError::PtyFailed(m) if m.starts_with("unreachable: "))
}

/// `gond-code pty-host stop`: closes every terminal and ends the host, if one runs.
pub fn stop(endpoint: &str) -> AppResult<()> {
    struct Quiet;
    impl PtySink for Quiet {
        fn data(&self, _: &str, _: String, _: u64) {}
        fn exit(&self, _: &str, _: Option<u32>) {}
    }
    HostClient::new(endpoint.into(), None, Arc::new(Quiet)).shutdown()
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

trait Duplex: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> Duplex for T {}
type Stream = Box<dyn Duplex>;

#[cfg(windows)]
async fn dial(endpoint: &str) -> std::io::Result<Stream> {
    use std::os::windows::io::AsRawHandle;
    use tokio::net::windows::named_pipe::ClientOptions;
    let client = ClientOptions::new().open(endpoint)?;
    // Only our own host: a pipe of that name opened by anything else is refused.
    let mut pid = 0u32;
    // SAFETY: a valid pipe handle and an out-parameter we own.
    let ok = unsafe {
        windows_sys::Win32::System::Pipes::GetNamedPipeServerProcessId(
            client.as_raw_handle(),
            &mut pid,
        )
    };
    if ok == 0 || !super::host::is_ours(pid) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "the pipe isn't Gond Code's terminal host",
        ));
    }
    Ok(Box::new(client))
}

#[cfg(not(windows))]
async fn dial(endpoint: &str) -> std::io::Result<Stream> {
    Ok(Box::new(tokio::net::UnixStream::connect(endpoint).await?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pty::tests::{wait_for, Recorder};

    fn endpoint(dir: &std::path::Path) -> String {
        if cfg!(windows) {
            format!(r"\\.\pipe\gond-code-test-{}", ulid::Ulid::generate())
        } else {
            dir.join("t.sock").to_string_lossy().into_owned()
        }
    }

    /// The whole point: a terminal outlives the window. One client opens it and goes away;
    /// the next attaches by the pane's key, gets what it printed, and keeps using it.
    #[test]
    fn a_terminal_outlives_its_window() {
        let dir = tempfile::tempdir().unwrap();
        let ep = endpoint(dir.path());
        let server_ep = ep.clone();
        let server = std::thread::spawn(move || {
            crate::pty::host::serve(
                &server_ep,
                Duration::from_secs(60),
                &crate::pty::host::build(),
                None,
            )
            .unwrap()
        });
        let shell = crate::pty::test_shell(dir.path());

        let first_out = Arc::new(Recorder::default());
        let first = HostClient::new(ep.clone(), None, first_out.clone());
        let start = Instant::now();
        while first.connect().is_err() {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "host didn't come up"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        let o = first
            .spawn("home", Some("pane-1"), &shell, 80, 24, true)
            .unwrap();
        assert!(o.pid.is_some());
        first.write(&o.pty_id, "echo gond-before\r").unwrap();
        wait_for("output in the first window", || {
            first_out.out.lock().unwrap().matches("gond-before").count() >= 2
        });
        drop(first);

        let second_out = Arc::new(Recorder::default());
        let second = HostClient::new(ep.clone(), None, second_out.clone());
        let a = second.attach("pane-1").unwrap().expect("still there");
        assert_eq!(a.pty_id, o.pty_id);
        assert!(!a.exited);
        assert!(a.replay.contains("gond-before"), "{:?}", a.replay);
        second.write(&a.pty_id, "echo gond-after\r").unwrap();
        wait_for("output in the second window", || {
            second_out.out.lock().unwrap().contains("gond-after")
        });
        assert!(second_out.ends.lock().unwrap().iter().all(|e| *e > a.end));
        let read = second
            .read(
                &a.pty_id,
                None,
                Some("gond-after"),
                Some(Duration::from_secs(10)),
            )
            .unwrap();
        assert_eq!(read.matched, Some(true));
        assert!(second.pids().contains(&a.pid.unwrap()));
        assert_eq!(second.prune(&["pane-1".into()]).unwrap(), 0);

        second.shutdown().unwrap();
        server.join().unwrap();
        assert!(!second.connected() || second.pids().is_empty());
    }
}
