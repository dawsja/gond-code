//! The terminal host: a background process that owns the terminals, so they outlive the app.
//! Closing, quitting, crashing or updating Gond Code leaves its shells and the agents in them
//! running; the next Gond Code connects again and each pane finds its terminal by its id.
//!
//! - It is `gond-code pty-host run <build>`, run from a copy of the exe in
//!   `%LOCALAPPDATA%\GondCode\host\<build>` (`gond-code-host.exe`; Gond Code Dev's is
//!   `gond-code-dev-host.exe`), so the installer can replace `gond-code.exe` while it runs, and
//!   a dev build can be rebuilt. A launcher (`pty-host detach`) starts it with nothing but NUL
//!   for its input and output, and ends: the host is nobody's child.
//! - **One host per build.** It listens on a named pipe only this Windows user can open (and
//!   only at its integrity level or above), named for the channel, the user and the build (a
//!   Unix socket in the data folder elsewhere), and says where it is in `host\live\<build>.json`.
//!   A newer Gond Code starts its new terminals in its own build's host and finds the panes
//!   still running in older builds' hosts there; an older host that a newer window has reached
//!   ends once its last terminal has. So an update, or a change to this code, never ends a
//!   terminal, and never strands one either.
//! - One window at a time: a new connection takes over the output stream.
//! - With no terminals and no window for 30 seconds it ends.
//! - `gond-code pty-host stop` closes every terminal in every build's host and ends them (the
//!   uninstaller does this).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, Notify};

use super::proto::{line, Envelope, Failure, FromHost, Request, PROTOCOL};
use super::{PtyManager, PtySink};
use crate::error::AppError;

/// How long the host waits, with no terminals and no window, before it ends.
pub const IDLE: Duration = Duration::from_secs(30);

/// The host's exe name: its own, so the installer's "close Gond Code" never matches it.
pub fn exe_name() -> &'static str {
    match (crate::channel::is_dev(), cfg!(windows)) {
        (false, true) => "gond-code-host.exe",
        (true, true) => "gond-code-dev-host.exe",
        (false, false) => "gond-code-host",
        (true, false) => "gond-code-dev-host",
    }
}

/// Where host copies live: `%LOCALAPPDATA%\GondCode\host`.
pub fn root() -> Option<PathBuf> {
    crate::appdata::local().map(|d| d.join("host"))
}

/// This user's host for this channel and `build`.
pub fn endpoint_for(build: &str) -> String {
    #[cfg(windows)]
    {
        let sid = win::user_sid().unwrap_or_else(|| "user".into());
        format!(
            r"\\.\pipe\{}-pty-{sid}-{build}",
            crate::channel::CHANNEL.identifier()
        )
    }
    #[cfg(not(windows))]
    {
        crate::appdata::local()
            .unwrap_or_else(std::env::temp_dir)
            .join(format!("pty-{build}.sock"))
            .to_string_lossy()
            .into_owned()
    }
}

/// This build's host.
pub fn endpoint() -> String {
    endpoint_for(&build())
}

/// Where each running host says where it is: `host\live\<build>.json`.
pub fn live_dir() -> Option<PathBuf> {
    root().map(|r| r.join("live"))
}

/// A running host, as it describes itself in `live`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Live {
    pub endpoint: String,
    pub build: String,
    pub pid: u32,
    pub protocol: u32,
}

/// The hosts of every build that says it's running, from `dir` (`live_dir`). A record whose
/// host isn't running any more (it was killed outright) is removed.
pub fn running(dir: &std::path::Path) -> Vec<Live> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut out = vec![];
    for e in entries.flatten() {
        let Some(l) = std::fs::read(e.path())
            .ok()
            .and_then(|b| serde_json::from_slice::<Live>(&b).ok())
        else {
            continue;
        };
        if alive(l.pid) {
            out.push(l);
        } else {
            let _ = std::fs::remove_file(e.path());
        }
    }
    out
}

/// Whether host process `pid` still runs.
fn alive(pid: u32) -> bool {
    #[cfg(windows)]
    return is_ours(pid);
    #[cfg(not(windows))]
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

/// The endpoints of running hosts of other builds than this one: the app reaches its panes
/// still running there.
pub fn older_endpoints() -> Vec<String> {
    let own = build();
    live_dir()
        .map(|d| running(&d))
        .unwrap_or_default()
        .into_iter()
        .filter(|l| l.build != own)
        .map(|l| l.endpoint)
        .collect()
}

/// `gond-code pty-host [run <build> | detach <build> | stop]`.
pub fn main(args: &[String]) -> i32 {
    let build_arg = || args.get(1).cloned().unwrap_or_else(build);
    match args.first().map(String::as_str) {
        // The app's launcher: starts the host and ends, so the host is nobody's child and a
        // "close this app and its children" (an installer, `tauri dev`) passes it by.
        Some("detach") => match start(&["pty-host", "run", &build_arg()]) {
            Ok(()) => 0,
            Err(e) => {
                log(&format!("couldn't start the host: {e}"));
                1
            }
        },
        // Every build's host, then whatever answers at this build's name.
        Some("stop") => {
            let mut ends: Vec<String> = live_dir()
                .map(|d| running(&d))
                .unwrap_or_default()
                .into_iter()
                .map(|l| l.endpoint)
                .collect();
            ends.push(endpoint());
            ends.dedup();
            for e in ends {
                let _ = super::client::stop(&e);
            }
            0
        }
        _ => {
            let build = build_arg();
            match serve(&endpoint_for(&build), IDLE, &build, live_dir().as_deref()) {
                Ok(()) => 0,
                Err(e) => {
                    log(&format!("host ended: {e}"));
                    1
                }
            }
        }
    }
}

/// Appends a line to `host\host.log`, for finding out why a host went away.
fn log(msg: &str) {
    static ONE: Mutex<()> = Mutex::new(());
    let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
    let Some(dir) = root() else { return };
    let path = dir.join("host.log");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 256 * 1024) {
        let _ = std::fs::remove_file(&path);
    }
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        use std::io::Write;
        let _ = writeln!(f, "{} [{}] {msg}", crate::store::now(), std::process::id());
    }
}

/// Where the terminals' output goes: the window connected now, if any.
#[derive(Default)]
struct Outbox {
    to: Mutex<Option<(u64, mpsc::UnboundedSender<String>)>>,
    /// When the last window went away (or the host started).
    alone_since: Mutex<Option<Instant>>,
}

impl Outbox {
    fn send(&self, msg: &FromHost) {
        if let Some((_, tx)) = &*self.to.lock().unwrap_or_else(|e| e.into_inner()) {
            let _ = tx.send(line(msg));
        }
    }
}

impl PtySink for Outbox {
    fn data(&self, pty_id: &str, data: String, end: u64) {
        self.send(&FromHost::Data {
            pty_id: pty_id.into(),
            data,
            end,
        });
    }
    fn exit(&self, pty_id: &str, code: Option<u32>) {
        self.send(&FromHost::Exit {
            pty_id: pty_id.into(),
            code,
        });
    }
}

struct Host {
    mgr: PtyManager,
    out: Arc<Outbox>,
    next_conn: AtomicU64,
    stop: Notify,
    /// The build this host runs.
    build: String,
    /// A window of another (newer) build has connected: this host takes nothing new from it
    /// and ends once its last terminal has.
    replaced: AtomicBool,
}

/// Runs a host of `build` on `endpoint` until it's told to stop, or has been idle for
/// `idle`; says where it is in `live` (`host\live`) while it runs.
pub fn serve(
    endpoint: &str,
    idle: Duration,
    build: &str,
    live: Option<&std::path::Path>,
) -> std::io::Result<()> {
    // Ctrl+C on, whatever started us: the shells inherit it, and Ctrl+C in a pane has to stop
    // what runs there.
    #[cfg(windows)]
    // SAFETY: a plain call; a null handler with FALSE restores normal Ctrl+C processing.
    unsafe {
        windows_sys::Win32::System::Console::SetConsoleCtrlHandler(None, 0);
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let out = Arc::new(Outbox::default());
    *out.alone_since.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
    let host = Arc::new(Host {
        mgr: PtyManager::new(out.clone()),
        out,
        next_conn: AtomicU64::new(1),
        stop: Notify::new(),
        build: build.to_string(),
        replaced: AtomicBool::new(false),
    });
    #[cfg(windows)]
    let job = win::job_note();
    #[cfg(not(windows))]
    let job = "";
    log(&format!("host started on {endpoint}{job}"));
    let here = live.map(|d| d.join(format!("{build}.json")));
    if let Some(file) = &here {
        let me = Live {
            endpoint: endpoint.to_string(),
            build: build.to_string(),
            pid: std::process::id(),
            protocol: PROTOCOL,
        };
        let _ = std::fs::create_dir_all(file.parent().unwrap_or(file));
        let _ = std::fs::write(file, serde_json::to_vec(&me).unwrap_or_default());
    }
    let result = rt.block_on(listen(endpoint, host.clone(), idle));
    host.mgr.close_all();
    if let Some(file) = &here {
        let _ = std::fs::remove_file(file);
    }
    log("host stopped");
    result
}

async fn listen(endpoint: &str, host: Arc<Host>, idle: Duration) -> std::io::Result<()> {
    let watch = host.clone();
    let idler = tokio::spawn(async move {
        let mut empty_since: Option<Instant> = None;
        loop {
            tokio::time::sleep(Duration::from_millis(500).min(idle)).await;
            let alone = *watch
                .out
                .alone_since
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            empty_since = if watch.mgr.is_empty() {
                empty_since.or(Some(Instant::now()))
            } else {
                None
            };
            // No terminals for a while, and nobody here (or only a newer build, which starts
            // its terminals in its own host).
            let done = empty_since.is_some_and(|t| t.elapsed() >= idle)
                && (alone.is_some_and(|t| t.elapsed() >= idle)
                    || watch.replaced.load(Ordering::SeqCst));
            if done {
                watch.stop.notify_one();
                return;
            }
        }
    });
    let r = accept_loop(endpoint, host).await;
    idler.abort();
    r
}

#[cfg(windows)]
async fn accept_loop(endpoint: &str, host: Arc<Host>) -> std::io::Result<()> {
    use tokio::net::windows::named_pipe::ServerOptions;
    let sid = win::user_sid().ok_or_else(|| std::io::Error::other("no user SID"))?;
    let mut sec = win::Security::only(&sid)?;
    let create = |first: bool, sec: &mut win::Security| {
        // SAFETY: `sec` holds a valid SECURITY_ATTRIBUTES for the duration of the call.
        unsafe {
            ServerOptions::new()
                .first_pipe_instance(first)
                .reject_remote_clients(true)
                .create_with_security_attributes_raw(endpoint, sec.as_ptr())
        }
    };
    // The first instance: if another process already has this name, this fails, and nothing
    // can sit in front of a running host.
    let mut server = create(true, &mut sec)?;
    loop {
        tokio::select! {
            r = server.connect() => {
                r?;
                let next = create(false, &mut sec)?;
                let conn = std::mem::replace(&mut server, next);
                tokio::spawn(serve_conn(host.clone(), conn));
            }
            _ = host.stop.notified() => return Ok(()),
        }
    }
}

#[cfg(not(windows))]
async fn accept_loop(endpoint: &str, host: Arc<Host>) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    use tokio::net::{UnixListener, UnixStream};
    if UnixStream::connect(endpoint).await.is_ok() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AddrInUse,
            "a terminal host is already running",
        ));
    }
    let _ = std::fs::remove_file(endpoint);
    if let Some(dir) = std::path::Path::new(endpoint).parent() {
        std::fs::create_dir_all(dir)?;
    }
    let listener = UnixListener::bind(endpoint)?;
    std::fs::set_permissions(endpoint, std::fs::Permissions::from_mode(0o600))?;
    loop {
        tokio::select! {
            r = listener.accept() => {
                let (conn, _) = r?;
                tokio::spawn(serve_conn(host.clone(), conn));
            }
            _ = host.stop.notified() => {
                let _ = std::fs::remove_file(endpoint);
                return Ok(());
            }
        }
    }
}

/// One window's connection: it takes over the output stream, and each request it sends is
/// answered in order (a slow `read` answers when it can).
async fn serve_conn<S>(host: Arc<Host>, conn: S)
where
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    let (rd, mut wr) = tokio::io::split(conn);
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let me = host.next_conn.fetch_add(1, Ordering::Relaxed);
    *host.out.to.lock().unwrap_or_else(|e| e.into_inner()) = Some((me, tx.clone()));
    *host
        .out
        .alone_since
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = None;
    log(&format!("window {me} connected"));

    let writer = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if wr.write_all(msg.as_bytes()).await.is_err() {
                break;
            }
            if rx.is_empty() && wr.flush().await.is_err() {
                break;
            }
        }
    });

    let mut lines = BufReader::new(rd).lines();
    while let Ok(Some(text)) = lines.next_line().await {
        let env = match serde_json::from_str::<Envelope>(&text) {
            Ok(env) => env,
            // A newer app's request this host doesn't know: say so, so it isn't left waiting.
            Err(e) => {
                if let Some(id) = serde_json::from_str::<Value>(&text)
                    .ok()
                    .and_then(|v| v["id"].as_u64())
                {
                    let err = Failure::from(&AppError::Invalid(format!(
                        "this terminal host doesn't know that request: {e}"
                    )));
                    let _ = tx.send(line(&FromHost::Reply {
                        id,
                        ok: Value::Null,
                        err: Some(err),
                    }));
                }
                continue;
            }
        };
        let reply = |r: Result<Value, AppError>| {
            let (ok, err) = match r {
                Ok(v) => (v, None),
                Err(e) => (Value::Null, Some(Failure::from(&e))),
            };
            let _ = tx.send(line(&FromHost::Reply {
                id: env.id,
                ok,
                err,
            }));
        };
        match env.req {
            // A read can wait up to two minutes: it answers on its own.
            Request::Read {
                pty_id,
                since,
                wait_for,
                timeout_ms,
            } => {
                let (mgr, tx, id) = (host.mgr.clone(), tx.clone(), env.id);
                tokio::task::spawn_blocking(move || {
                    let r = mgr
                        .read(
                            &pty_id,
                            since,
                            wait_for.as_deref(),
                            timeout_ms.map(Duration::from_millis),
                        )
                        .map(|r| serde_json::to_value(r).unwrap_or_default());
                    let (ok, err) = match r {
                        Ok(v) => (v, None),
                        Err(e) => (Value::Null, Some(Failure::from(&e))),
                    };
                    let _ = tx.send(line(&FromHost::Reply { id, ok, err }));
                });
            }
            Request::Shutdown => {
                reply(Ok(Value::Null));
                host.mgr.close_all();
                // Let the answer out before the host goes.
                tokio::time::sleep(Duration::from_millis(100)).await;
                host.stop.notify_one();
            }
            Request::Hello { build } => {
                if build.as_deref().is_some_and(|b| b != host.build) {
                    host.replaced.store(true, Ordering::SeqCst);
                    log(&format!("a window of build {build:?} took over"));
                }
                reply(Ok(json!({
                    "protocol": PROTOCOL,
                    "build": host.build,
                    "pid": std::process::id(),
                })));
            }
            req => reply(handle(&host.mgr, req)),
        }
    }

    // This window is gone; if it was the one streaming, nobody is now.
    {
        let mut to = host.out.to.lock().unwrap_or_else(|e| e.into_inner());
        if to.as_ref().is_some_and(|(id, _)| *id == me) {
            *to = None;
            *host
                .out
                .alone_since
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
        }
    }
    drop(tx);
    let _ = writer.await;
    log(&format!("window {me} went away"));
}

fn handle(mgr: &PtyManager, req: Request) -> Result<Value, AppError> {
    fn v(x: impl serde::Serialize) -> Value {
        serde_json::to_value(x).unwrap_or_default()
    }
    Ok(match req {
        Request::Spawn {
            owner,
            key,
            spawn,
            cols,
            rows,
            headless,
        } => v(mgr.spawn(&owner, key.as_deref(), &spawn, cols, rows, headless)?),
        Request::Attach { key } => v(mgr.attach(&key)),
        Request::Write { pty_id, data } => {
            mgr.write(&pty_id, &data)?;
            Value::Null
        }
        Request::Resize { pty_id, cols, rows } => {
            mgr.resize(&pty_id, cols, rows)?;
            Value::Null
        }
        Request::Pid { pty_id } => json!(mgr.pid(&pty_id)),
        Request::Pids => json!(mgr.pids()),
        Request::Close { pty_id } => {
            mgr.close(&pty_id)?;
            Value::Null
        }
        Request::CloseAll => {
            mgr.close_all();
            Value::Null
        }
        Request::CloseOwner { owner } => {
            mgr.close_thread(&owner);
            Value::Null
        }
        Request::Prune { keep } => json!(mgr.prune(&keep)),
        Request::Hello { .. } | Request::Read { .. } | Request::Shutdown => Value::Null,
    })
}

/// Which build this exe is: its size and modified time.
pub fn build() -> String {
    std::env::current_exe()
        .and_then(std::fs::metadata)
        .map(|m| stamp(&m))
        .unwrap_or_default()
}

fn stamp(m: &std::fs::Metadata) -> String {
    let secs = m
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or_default();
    format!("{}-{secs}", m.len())
}

/// Starts a host: copies this exe into `host\<build>\` (once per build) and runs the copy,
/// detached, out of any job the app is in and through a launcher that ends at once, so it
/// outlives the app.
pub fn launch() -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let root = root().ok_or_else(|| std::io::Error::other("no app data folder"))?;
    let build = stamp(&std::fs::metadata(&exe)?);
    let dir = root.join(&build);
    let target = dir.join(exe_name());
    if !target.exists() {
        std::fs::create_dir_all(&dir)?;
        let part = dir.join(format!("{}.part", exe_name()));
        std::fs::copy(&exe, &part)?;
        std::fs::rename(&part, &target)?;
    }
    // Older builds' copies, unless a host still runs from one.
    let alive: Vec<String> = live_dir()
        .map(|d| running(&d))
        .unwrap_or_default()
        .into_iter()
        .map(|l| l.build)
        .collect();
    if let Ok(entries) = std::fs::read_dir(&root) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if e.path().is_dir() && name != build && name != "live" && !alive.contains(&name) {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    let status = command(&target, &["pty-host", "detach", &build])?.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "the host's launcher failed ({status})"
        )))
    }
}

/// Runs this exe with `args`, detached (the launcher's half of `launch`). On Windows it hands
/// down no handle but NUL for its input and output: whatever the app (or a terminal that
/// started it) had open isn't kept open by the host for good.
fn start(args: &[&str]) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    #[cfg(windows)]
    return win::spawn_detached(&exe, args);
    #[cfg(not(windows))]
    command(&exe, args)?.spawn().map(|_| ())
}

/// `exe args`, detached from the console and from any job that lets it go; in the exe's
/// folder, with no input or output.
fn command(exe: &std::path::Path, args: &[&str]) -> std::io::Result<std::process::Command> {
    let mut cmd = crate::process::command(exe);
    cmd.args(args)
        .current_dir(exe.parent().unwrap_or(std::path::Path::new(".")))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Not CREATE_NEW_PROCESS_GROUP: it turns Ctrl+C off for the host and every shell it
        // starts, and Ctrl+C in a pane would stop nothing.
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        let base = DETACHED_PROCESS;
        // A job that doesn't allow breaking away refuses; then the host lives in it.
        let can_break_away = {
            let mut probe = crate::process::command("cmd.exe");
            probe
                .args(["/c", "exit"])
                .creation_flags(base | CREATE_BREAKAWAY_FROM_JOB)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            probe.status().is_ok()
        };
        cmd.creation_flags(if can_break_away {
            base | CREATE_BREAKAWAY_FROM_JOB
        } else {
            base
        });
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    Ok(cmd)
}

/// Whether process `pid` is a terminal host from our own folder (or this very process, in
/// tests): a pipe that answers under our name but isn't ours is never trusted.
#[cfg(windows)]
pub fn is_ours(pid: u32) -> bool {
    if pid == std::process::id() {
        return true;
    }
    let (Some(image), Some(root)) = (win::image_of(pid), root()) else {
        return false;
    };
    let image = image.to_string_lossy().to_lowercase();
    image.starts_with(&root.to_string_lossy().to_lowercase())
        && image.ends_with(&exe_name().to_lowercase())
}

#[cfg(windows)]
pub(crate) mod win {
    use std::path::PathBuf;
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE};
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenUser, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY,
        TOKEN_USER,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW,
        PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    /// What the host's log says about the job it runs in: a job that ends its processes when
    /// it closes ends the terminals with it (a launcher that kept it from leaving).
    pub fn job_note() -> &'static str {
        use windows_sys::Win32::System::JobObjects::{
            IsProcessInJob, JobObjectExtendedLimitInformation, QueryInformationJobObject,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        // SAFETY: plain queries about this process's own job, into structs we own.
        unsafe {
            let mut inside = 0;
            if IsProcessInJob(GetCurrentProcess(), null_mut(), &mut inside) == 0 || inside == 0 {
                return "";
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            let ok = QueryInformationJobObject(
                null_mut(),
                JobObjectExtendedLimitInformation,
                (&raw mut info).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                null_mut(),
            );
            if ok != 0
                && info.BasicLimitInformation.LimitFlags & JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE != 0
            {
                " (in a job that ends it when it closes: its terminals last as long as that job)"
            } else {
                " (in a job)"
            }
        }
    }

    /// Whether this process runs elevated (as administrator).
    fn elevated() -> bool {
        use windows_sys::Win32::Security::{TokenElevation, TOKEN_ELEVATION};
        // SAFETY: a token handle we close, and an out-struct of the size we pass.
        unsafe {
            let mut token: HANDLE = null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return false;
            }
            let mut e: TOKEN_ELEVATION = std::mem::zeroed();
            let mut n = 0u32;
            let ok = GetTokenInformation(
                token,
                TokenElevation,
                (&raw mut e).cast(),
                std::mem::size_of::<TOKEN_ELEVATION>() as u32,
                &mut n,
            );
            CloseHandle(token);
            ok != 0 && e.TokenIsElevated != 0
        }
    }

    /// Starts `exe args` detached from any console, out of any job that lets it go, with NUL
    /// for its input and output and no other handle of ours: the standard spawn would hand
    /// down every inheritable handle this process has, and the host would hold them open for
    /// good.
    pub fn spawn_detached(exe: &std::path::Path, args: &[&str]) -> std::io::Result<()> {
        use std::os::windows::ffi::OsStrExt;
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Foundation::{
            SetHandleInformation, ERROR_ACCESS_DENIED, HANDLE_FLAG_INHERIT,
        };
        use windows_sys::Win32::System::Threading::{
            CreateProcessW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList,
            UpdateProcThreadAttribute, CREATE_BREAKAWAY_FROM_JOB, DETACHED_PROCESS,
            EXTENDED_STARTUPINFO_PRESENT, LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST, STARTF_USESTDHANDLES, STARTUPINFOEXW,
        };

        let quote = |s: &str| {
            if s.is_empty() || s.contains([' ', '\t', '"']) {
                format!("\"{}\"", s.replace('"', "\\\""))
            } else {
                s.to_string()
            }
        };
        let line = std::iter::once(quote(&exe.to_string_lossy()))
            .chain(args.iter().map(|a| quote(a)))
            .collect::<Vec<_>>()
            .join(" ");
        let mut line: Vec<u16> = std::ffi::OsStr::new(&line)
            .encode_wide()
            .chain(Some(0))
            .collect();
        let dir: Vec<u16> = exe
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let nul = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("NUL")?;
        let h = nul.as_raw_handle() as HANDLE;
        // SAFETY: an attribute list sized by its first call and freed below; a handle list of
        // one live handle that outlives CreateProcessW; strings NUL-terminated and alive.
        unsafe {
            if SetHandleInformation(h, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) == 0 {
                return Err(std::io::Error::last_os_error());
            }
            let mut size = 0usize;
            InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut size);
            let mut attrs = vec![0u8; size];
            let list = attrs.as_mut_ptr() as LPPROC_THREAD_ATTRIBUTE_LIST;
            if InitializeProcThreadAttributeList(list, 1, 0, &mut size) == 0 {
                return Err(std::io::Error::last_os_error());
            }
            let handles = [h];
            let ok = UpdateProcThreadAttribute(
                list,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_ptr().cast(),
                std::mem::size_of_val(&handles),
                null_mut(),
                std::ptr::null(),
            );
            if ok == 0 {
                DeleteProcThreadAttributeList(list);
                return Err(std::io::Error::last_os_error());
            }
            let mut si: STARTUPINFOEXW = std::mem::zeroed();
            si.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
            si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
            si.StartupInfo.hStdInput = h;
            si.StartupInfo.hStdOutput = h;
            si.StartupInfo.hStdError = h;
            si.lpAttributeList = list;
            let mut pi: PROCESS_INFORMATION = std::mem::zeroed();
            let mut run = |flags: u32| {
                CreateProcessW(
                    std::ptr::null(),
                    line.as_mut_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    1,
                    flags,
                    std::ptr::null(),
                    dir.as_ptr(),
                    &si.StartupInfo,
                    &mut pi,
                ) != 0
            };
            let base = EXTENDED_STARTUPINFO_PRESENT | DETACHED_PROCESS;
            // A job that doesn't let it go refuses with access denied: then it lives in it.
            let mut started = run(base | CREATE_BREAKAWAY_FROM_JOB);
            if !started
                && std::io::Error::last_os_error().raw_os_error()
                    == Some(ERROR_ACCESS_DENIED as i32)
            {
                super::log("the job the app runs in won't let the host leave it");
                started = run(base);
            }
            let err = std::io::Error::last_os_error();
            DeleteProcThreadAttributeList(list);
            if !started {
                return Err(err);
            }
            CloseHandle(pi.hThread);
            CloseHandle(pi.hProcess);
        }
        Ok(())
    }

    /// This process's user, as a SID string (`S-1-5-21-…`).
    pub fn user_sid() -> Option<String> {
        // SAFETY: a token handle we close, a buffer sized by the first call, and a string
        // Windows allocates that we free with LocalFree.
        unsafe {
            let mut token: HANDLE = null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
                return None;
            }
            let mut len = 0u32;
            GetTokenInformation(token, TokenUser, null_mut(), 0, &mut len);
            let mut buf = vec![0u64; (len as usize).div_ceil(8)];
            let ok = GetTokenInformation(token, TokenUser, buf.as_mut_ptr().cast(), len, &mut len);
            CloseHandle(token);
            if ok == 0 {
                return None;
            }
            let user = &*(buf.as_ptr() as *const TOKEN_USER);
            let mut s: *mut u16 = null_mut();
            if ConvertSidToStringSidW(user.User.Sid, &mut s) == 0 || s.is_null() {
                return None;
            }
            let n = (0..).take_while(|&i| *s.add(i) != 0).count();
            let out = String::from_utf16_lossy(std::slice::from_raw_parts(s, n));
            LocalFree(s.cast());
            Some(out)
        }
    }

    /// A security descriptor that lets only `sid` (and SYSTEM) open the pipe.
    pub struct Security {
        sd: PSECURITY_DESCRIPTOR,
        attrs: SECURITY_ATTRIBUTES,
    }
    // The descriptor is only read by the kernel calls that take it.
    unsafe impl Send for Security {}
    unsafe impl Sync for Security {}

    impl Security {
        pub fn only(sid: &str) -> std::io::Result<Security> {
            // This user and SYSTEM only; and nothing below our integrity level, so a program
            // running at low integrity (a sandboxed browser's) can't drive the terminals, nor
            // an unelevated one an elevated Gond's.
            let level = if elevated() { "HI" } else { "ME" };
            let sddl: Vec<u16> = format!("D:P(A;;GA;;;{sid})(A;;GA;;;SY)S:(ML;;NRNW;;;{level})")
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let mut sd: PSECURITY_DESCRIPTOR = null_mut();
            // SAFETY: a NUL-terminated string; Windows allocates `sd`, freed in Drop.
            let ok = unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    SDDL_REVISION_1,
                    &mut sd,
                    null_mut(),
                )
            };
            if ok == 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(Security {
                sd,
                attrs: SECURITY_ATTRIBUTES {
                    nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                    lpSecurityDescriptor: sd,
                    bInheritHandle: 0,
                },
            })
        }

        pub fn as_ptr(&mut self) -> *mut std::ffi::c_void {
            self.attrs.lpSecurityDescriptor = self.sd;
            (&mut self.attrs as *mut SECURITY_ATTRIBUTES).cast()
        }
    }

    impl Drop for Security {
        fn drop(&mut self) {
            // SAFETY: allocated by ConvertStringSecurityDescriptorToSecurityDescriptorW.
            unsafe { LocalFree(self.sd) };
        }
    }

    /// The full path of process `pid`'s exe.
    pub fn image_of(pid: u32) -> Option<PathBuf> {
        // SAFETY: a process handle we close and a buffer whose size we pass.
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return None;
            }
            let mut buf = vec![0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
            CloseHandle(h);
            (ok != 0).then(|| PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
        }
    }
}
