//! The terminal host: a background process that owns the terminals, so they outlive the app.
//! Closing, quitting, crashing or updating Gond Code leaves its shells and the agents in them
//! running; the next Gond Code connects again and each pane finds its terminal by its id.
//!
//! - It is `gond-code pty-host`, run from a copy of the exe in `%LOCALAPPDATA%\GondCode\host`
//!   (`gond-code-host.exe`; Gond Code Dev's is `gond-code-dev-host.exe`), so the installer can
//!   replace `gond-code.exe` while it runs, and a dev build can be rebuilt.
//! - It listens on a named pipe only this Windows user can open (a Unix socket in the data
//!   folder elsewhere), named for the channel, the user and the protocol version.
//! - One window at a time: a new connection takes over the output stream.
//! - With no terminals and no window for 30 seconds it ends.
//! - `gond-code pty-host stop` closes every terminal and ends it (the uninstaller does this).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
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

/// This user's host for this channel and protocol.
pub fn endpoint() -> String {
    #[cfg(windows)]
    {
        let sid = win::user_sid().unwrap_or_else(|| "user".into());
        format!(
            r"\\.\pipe\{}-pty-{sid}-v{PROTOCOL}",
            crate::channel::CHANNEL.identifier()
        )
    }
    #[cfg(not(windows))]
    {
        crate::appdata::local()
            .unwrap_or_else(std::env::temp_dir)
            .join(format!("pty-v{PROTOCOL}.sock"))
            .to_string_lossy()
            .into_owned()
    }
}

/// `gond-code pty-host [stop|detach]`.
pub fn main(args: &[String]) -> i32 {
    match args.first().map(String::as_str) {
        // The app's launcher: starts the host and ends, so the host is nobody's child and a
        // "close this app and its children" (an installer, `tauri dev`) passes it by.
        Some("detach") => match start(&["pty-host"]) {
            Ok(()) => 0,
            Err(e) => {
                log(&format!("couldn't start the host: {e}"));
                1
            }
        },
        Some("stop") => match super::client::stop(&endpoint()) {
            Ok(()) => 0,
            Err(_) => 0, // none running: nothing to stop
        },
        _ => match serve(&endpoint(), IDLE) {
            Ok(()) => 0,
            Err(e) => {
                log(&format!("host ended: {e}"));
                1
            }
        },
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
}

/// Runs a host on `endpoint` until it's told to stop, or has been idle for `idle`.
pub fn serve(endpoint: &str, idle: Duration) -> std::io::Result<()> {
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
    });
    log(&format!("host started on {endpoint}"));
    let result = rt.block_on(listen(endpoint, host.clone(), idle));
    host.mgr.close_all();
    log("host stopped");
    result
}

async fn listen(endpoint: &str, host: Arc<Host>, idle: Duration) -> std::io::Result<()> {
    let watch = host.clone();
    let idler = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(500).min(idle)).await;
            let alone = *watch
                .out
                .alone_since
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if watch.mgr.is_empty() && alone.is_some_and(|t| t.elapsed() >= idle) {
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
        let Ok(env) = serde_json::from_str::<Envelope>(&text) else {
            continue;
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
        Request::Hello => json!({
            "protocol": PROTOCOL,
            "build": build(),
            "pid": std::process::id(),
        }),
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
        Request::Read { .. } | Request::Shutdown => Value::Null,
    })
}

/// Which build of the exe this host runs: its size and modified time.
fn build() -> String {
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
    if let Ok(entries) = std::fs::read_dir(&root) {
        for e in entries.flatten() {
            if e.path().is_dir() && e.file_name().to_string_lossy() != build {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    let status = command(&target, &["pty-host", "detach"])?.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "the host's launcher failed ({status})"
        )))
    }
}

/// Runs this exe with `args`, detached (the launcher's half of `launch`).
fn start(args: &[&str]) -> std::io::Result<()> {
    command(&std::env::current_exe()?, args)?
        .spawn()
        .map(|_| ())
}

/// `exe args`, detached from the console and from any job that lets it go; in the exe's
/// folder, with no input or output.
fn command(exe: &std::path::Path, args: &[&str]) -> std::io::Result<std::process::Command> {
    let mut cmd = std::process::Command::new(exe);
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
            let mut probe = std::process::Command::new("cmd.exe");
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
            let sddl: Vec<u16> = format!("D:P(A;;GA;;;{sid})(A;;GA;;;SY)")
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
