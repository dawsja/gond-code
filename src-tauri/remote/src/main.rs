//! `gond-remote`: Gond Code's terminal host, run on another machine (a WSL distribution, or a
//! computer over SSH). The app copies it to `~/.gond/bin/<hash>/` there and runs
//! `gond-remote attach` through `wsl.exe` or `ssh`: that starts the host if none runs (detached,
//! so it outlives the connection) and passes the app's line-JSON protocol between its own
//! input and output and the host's Unix socket. So the terminals, and the agents in them, keep
//! running when the connection drops or Gond Code quits, and the app picks each one back up.
//!
//! **One host per machine** (per data folder), whichever build started it: a newer helper
//! joins the host that runs rather than starting its own beside it, so an update never strands
//! a terminal over there (requests are only ever added; an older host answers a newer one it
//! doesn't know with an error). Once that host has no terminals and no window for 30 s it ends,
//! and the next connection starts the newer one.
//!
//! It's the app's own host code (`../src/pty`, `../src/agents`), built for Linux without the
//! app: the modules the app's code reaches for (`error`, `channel`, `appdata`…) are small
//! stand-ins here.

#![allow(dead_code, unused_imports)]

#[path = "../../src/pty/mod.rs"]
mod pty;

#[path = "../../src/process.rs"]
mod process;

#[path = "../../src/proctree.rs"]
mod proctree;

mod agents {
    #[path = "../../../src/agents/cwd.rs"]
    pub mod cwd;
    #[path = "../../../src/agents/resume.rs"]
    pub mod resume;
    #[path = "../../../src/agents/running.rs"]
    pub mod running;
    #[path = "../../../src/agents/spec.rs"]
    pub mod spec;
}

/// The app's errors, as far as the host code uses them.
mod error {
    #[derive(Debug, thiserror::Error)]
    pub enum AppError {
        #[error("{0}")]
        NotFound(String),
        #[error("{0}")]
        PtyFailed(String),
        #[error("{0}")]
        Invalid(String),
    }

    impl AppError {
        pub fn code(&self) -> &'static str {
            match self {
                AppError::NotFound(_) => "not_found",
                AppError::PtyFailed(_) => "pty_failed",
                AppError::Invalid(_) => "invalid",
            }
        }
    }

    impl From<std::io::Error> for AppError {
        fn from(e: std::io::Error) -> Self {
            AppError::PtyFailed(e.to_string())
        }
    }

    pub type AppResult<T> = Result<T, AppError>;
}

/// Gond Code or Gond Code Dev: the data folder says (`--data`), so a Dev build's host and
/// prod's never share terminals.
mod channel {
    pub struct Channel;
    pub const CHANNEL: Channel = Channel;
    impl Channel {
        pub const fn identifier(&self) -> &'static str {
            "app.gond.code.remote"
        }
    }
    pub fn is_dev() -> bool {
        crate::appdata::local().is_some_and(|d| d.to_string_lossy().ends_with("-dev"))
    }
}

/// `~/.gond` (Gond Code) or `~/.gond-dev` (Gond Code Dev): `--data`, else `~/.gond`.
mod appdata {
    use std::path::PathBuf;
    use std::sync::OnceLock;

    pub static DATA: OnceLock<PathBuf> = OnceLock::new();

    pub fn local() -> Option<PathBuf> {
        DATA.get()
            .cloned()
            .or_else(|| dirs::home_dir().map(|h| h.join(".gond")))
    }

    pub fn roaming() -> Option<PathBuf> {
        local()
    }
}

mod store {
    pub fn now() -> String {
        time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default()
    }
}

mod pathenv {
    pub fn registry_path() -> Option<String> {
        None
    }
}

use std::time::{Duration, Instant};

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // `--data <dir>` anywhere: the data folder; it's passed down to the host it starts.
    if let Some(i) = args.iter().position(|a| a == "--data") {
        if let Some(dir) = args.get(i + 1).cloned() {
            let _ = appdata::DATA.set(dir.clone().into());
            std::env::set_var("GOND_REMOTE_DATA", &dir);
            args.drain(i..=i + 1);
        }
    } else if let Some(dir) = std::env::var_os("GOND_REMOTE_DATA") {
        let _ = appdata::DATA.set(dir.into());
    }
    let code = match args.first().map(String::as_str) {
        Some("attach") => attach(),
        Some("keepalive") => keepalive(),
        Some("pty-host") if args.get(1).map(String::as_str) == Some("run") => {
            let build = pty::host::build();
            match pty::host::serve(
                &endpoint(),
                pty::host::IDLE,
                &build,
                pty::host::live_dir().as_deref(),
            ) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("gond-remote: {e}");
                    1
                }
            }
        }
        Some("version") => {
            println!(
                "gond-remote {} {}",
                env!("CARGO_PKG_VERSION"),
                pty::host::build()
            );
            0
        }
        _ => {
            eprintln!("usage: gond-remote [--data <dir>] attach | keepalive | version");
            2
        }
    };
    std::process::exit(code);
}

/// The host's socket: one per data folder, whichever build runs it.
fn endpoint() -> String {
    appdata::local()
        .unwrap_or_else(std::env::temp_dir)
        .join("pty.sock")
        .to_string_lossy()
        .into_owned()
}

/// Waits up to `limit` for the host to answer on its socket.
fn reach(limit: Duration) -> Option<std::os::unix::net::UnixStream> {
    let endpoint = endpoint();
    let start = Instant::now();
    loop {
        if let Ok(s) = std::os::unix::net::UnixStream::connect(&endpoint) {
            return Some(s);
        }
        if start.elapsed() > limit {
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Starts a host if none answers, then passes bytes between our input and output and its
/// socket until either side ends.
fn attach() -> i32 {
    let sock = match reach(Duration::ZERO) {
        Some(s) => s,
        None => {
            if let Err(e) = detach() {
                eprintln!("gond-remote: couldn't start the terminal host: {e}");
                return 1;
            }
            match reach(Duration::from_secs(8)) {
                Some(s) => s,
                None => {
                    eprintln!("gond-remote: the terminal host didn't start");
                    return 1;
                }
            }
        }
    };
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("gond-remote: {e}");
            return 1;
        }
    };
    let code = rt.block_on(async move {
        let _ = sock.set_nonblocking(true);
        let Ok(sock) = tokio::net::UnixStream::from_std(sock) else {
            return 1;
        };
        let (mut sr, mut sw) = sock.into_split();
        let mut stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        // Our input ending (the app closed it) closes the socket's sending half: the host
        // answers what it was asked, then hangs up, and `down` ends.
        let up = async {
            use tokio::io::AsyncWriteExt;
            let _ = tokio::io::copy(&mut stdin, &mut sw).await;
            let _ = sw.shutdown().await;
            std::future::pending::<()>().await
        };
        let down = async {
            let mut buf = vec![0u8; 64 * 1024];
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            loop {
                match sr.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if stdout.write_all(&buf[..n]).await.is_err()
                            || stdout.flush().await.is_err()
                        {
                            break;
                        }
                    }
                }
            }
        };
        tokio::select! {
            _ = up => {},
            _ = down => {},
        }
        0
    });
    // Don't wait on the blocked read of our input.
    rt.shutdown_background();
    code
}

/// Starts a host (this build's) in its own session, with no input or output: the connection
/// that started it ending (a hangup) never reaches it.
fn detach() -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;
    let exe = std::env::current_exe()?;
    let mut cmd = std::process::Command::new(exe);
    cmd.args(["pty-host", "run", &pty::host::build()])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if let Some(home) = dirs::home_dir() {
        cmd.current_dir(home);
    }
    // SAFETY: setsid is async-signal-safe and touches nothing of ours.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    cmd.spawn().map(|_| ())
}

/// Holds a WSL distribution up while the host runs there: WSL stops a distribution (and
/// everything in it) soon after its last `wsl.exe` ends, so Gond's own terminal host on Windows
/// runs this through `wsl.exe` for as long as the host here runs. It never connects to the host
/// (a connection is a window, and would take the app's output stream): it looks for the
/// host's socket and its process.
fn keepalive() -> i32 {
    let mut misses = 0;
    loop {
        std::thread::sleep(Duration::from_secs(5));
        let running = std::path::Path::new(&endpoint()).exists()
            && pty::host::live_dir().is_some_and(|d| !pty::host::running(&d).is_empty());
        if running {
            misses = 0;
        } else {
            misses += 1;
            if misses >= 3 {
                return 0;
            }
        }
    }
}
