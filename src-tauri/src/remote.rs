//! Machines: where a terminal runs besides this PC. A WSL distribution (`wsl:<distro>`, found on
//! this PC) or another computer over SSH (`ssh:<id>`, added in Settings → Machines).
//!
//! Either way it's Linux, reached through a program's input and output (`wsl.exe`, Windows'
//! OpenSSH `ssh`): Gond copies its terminal host for Linux (`gond-remote`, `../remote`) to
//! `~/.gond/bin/<hash>/` there once and runs `gond-remote attach`, which starts that machine's
//! host if none runs and carries the app's protocol (`pty/proto.rs`). The terminals live in
//! the host over there: they keep running when the connection drops or Gond Code quits, and
//! the host answers what the app can't see from here, the agent under a shell
//! (`pty.agent`), how it would resume (`agent.resume`), which printed paths are files.
//!
//! - **One connection per machine,** all its panes over it (Windows' OpenSSH can't share one).
//! - **Nothing to sign in to here.** SSH uses your `~/.ssh/config`, keys and ssh-agent. When
//!   ssh asks for something (a passphrase, a password, a new host key) it asks Gond through
//!   `SSH_ASKPASS`: this exe, run as the asker, passes the question to the window and the
//!   answer back to ssh. Gond keeps no answer.
//! - **WSL stops a distribution** soon after its last `wsl.exe` ends, and everything in it. So
//!   while the host there runs, Gond's own terminal host (which outlives the app) keeps one
//!   `wsl.exe … gond-remote keepalive` running, which ends when that host does.
//! - A connection that drops is retried by itself, unless it failed signing in.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex, Weak};
use std::task::{Context, Poll};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader, ReadBuf};

use crate::error::{AppError, AppResult};
use crate::pty::client::{Dial, HostClient, OnDown, Stream};
use crate::pty::{PtySink, Spawn};

/// Where a machine is: what the id says.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Wsl {
        distro: String,
    },
    /// A `Host` in Gond's ssh config (`crate::sshconfig`).
    Ssh(crate::sshconfig::Entry),
}

/// Whether a pane's shell id is a machine's (`wsl:…`, `ssh:…`) rather than a shell on this PC.
pub fn is_remote(id: &str) -> bool {
    id.starts_with("wsl:") || id.starts_with("ssh:")
}

/// The machine `id` names.
pub fn target(id: &str) -> AppResult<Target> {
    if let Some(distro) = id.strip_prefix("wsl:") {
        return Ok(Target::Wsl {
            distro: distro.to_string(),
        });
    }
    if let Some(alias) = id.strip_prefix("ssh:") {
        return crate::sshconfig::find(alias)
            .map(Target::Ssh)
            .ok_or_else(|| AppError::NotFound(format!("no machine {id}")));
    }
    Err(AppError::Invalid(format!("{id} isn't a machine")))
}

/// A machine's name: the distribution's, or its `Host` in ssh's config.
pub fn name(id: &str) -> String {
    id.strip_prefix("wsl:")
        .or_else(|| id.strip_prefix("ssh:"))
        .unwrap_or(id)
        .to_string()
}

/// The data folder on the machine: `.gond`, or `.gond-dev` for Gond Code Dev, so the two never
/// share a host or its terminals.
fn data_name() -> &'static str {
    if crate::channel::is_dev() {
        ".gond-dev"
    } else {
        ".gond"
    }
}

/// The helper for `arch` (`uname -m`): shipped beside the app (`remote/<arch>/gond-remote`); a
/// debug build also takes the one `remote/` builds.
pub fn helper(arch: &str) -> Option<PathBuf> {
    let arch = match arch {
        "x86_64" | "amd64" => "x86_64",
        "aarch64" | "arm64" | "armv8l" => "aarch64",
        _ => return None,
    };
    let mut places = vec![];
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        places.push(dir.join("remote").join(arch).join("gond-remote"));
    }
    #[cfg(debug_assertions)]
    places.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("remote/target")
            .join(format!("{arch}-unknown-linux-musl"))
            .join("release/gond-remote"),
    );
    places.into_iter().find(|p| p.is_file())
}

/// A helper's name on the machine: the first 16 hex digits of its SHA-256, so a new one goes
/// beside the old, and a copy that arrived damaged is caught (`sha256sum` there).
pub fn hash(path: &Path) -> std::io::Result<String> {
    use sha2::Digest;
    let bytes = std::fs::read(path)?;
    let d = sha2::Sha256::digest(&bytes);
    Ok(d.iter().take(8).map(|b| format!("{b:02x}")).collect())
}

/// What runs on the machine (`sh -c`): says its OS, arch and home; takes the helper's hash; if
/// that helper isn't there, takes its size and bytes and checks them; then runs it. The app
/// sends nothing it isn't asked for, so nothing reads past what's meant for it. No single
/// quotes: ssh's remote command wraps it in them.
pub fn script(data: &str) -> String {
    [
        &format!("d=\"$HOME/{data}\""),
        "printf \"gond:arch %s %s %s\\n\" \"$(uname -s)\" \"$(uname -m)\" \"$HOME\"",
        "IFS= read -r want || exit 3",
        "[ \"$want\" = none ] && exit 3",
        "h=\"$d/bin/$want/gond-remote\"",
        "if [ ! -x \"$h\" ]; then printf \"gond:need\\n\"; IFS= read -r n || exit 3; \
         mkdir -p \"$d/bin/$want\" && chmod 700 \"$d\" && head -c \"$n\" > \"$h.part\" || exit 4; \
         if command -v sha256sum >/dev/null 2>&1; then s=$(sha256sum \"$h.part\" | cut -c1-16); \
         if [ \"$s\" != \"$want\" ]; then rm -f \"$h.part\"; printf \"gond:bad\\n\"; exit 4; fi; fi; \
         chmod 700 \"$h.part\" && mv \"$h.part\" \"$h\" || exit 4; fi",
        "printf \"gond:ok\\n\"",
        "exec \"$h\" --data \"$d\" attach",
    ]
    .join("; ")
}

/// What the machine said as it started: where the helper is there.
#[derive(Debug, Clone, Default)]
pub struct Boot {
    pub home: String,
    pub hash: String,
}

impl Boot {
    fn data(&self) -> String {
        format!("{}/{}", self.home, data_name())
    }
    fn helper(&self) -> String {
        format!("{}/bin/{}/gond-remote", self.data(), self.hash)
    }
}

/// The program carrying a connection (`wsl.exe`, `ssh`): its output to read, its input to
/// write, and the program itself, ended when the stream goes.
struct ChildStream {
    rd: BufReader<tokio::process::ChildStdout>,
    wr: tokio::process::ChildStdin,
    _child: tokio::process::Child,
}

impl AsyncRead for ChildStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.rd).poll_read(cx, buf)
    }
}

impl AsyncWrite for ChildStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.wr).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.wr).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.wr).poll_shutdown(cx)
    }
}

/// What a connection is doing, for the window (`/machines/<id>`).
type Report = Arc<dyn Fn(&str) + Send + Sync>;

fn io_err(msg: impl Into<String>) -> std::io::Error {
    std::io::Error::other(msg.into())
}

/// Windows' own OpenSSH first (not another on PATH, such as Git's), so ssh reads the same
/// config and keys, with the same rules, however Gond was started.
fn ssh_exe() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let own = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
            .join(r"System32\OpenSSH\ssh.exe");
        if own.is_file() {
            return Some(own);
        }
    }
    which::which("ssh").ok()
}

/// `ssh` to machine `alias` (its `Host` in Gond's ssh config) with `extra` options; ssh's
/// questions go to the window when there's an `askpass`. The remote command goes after.
fn ssh_command(
    alias: &str,
    askpass: Option<&str>,
    machine: &str,
    extra: &[&str],
) -> std::io::Result<std::process::Command> {
    let ssh = ssh_exe()
        .ok_or_else(|| io_err("OpenSSH isn't installed (Windows Settings → Optional features)"))?;
    let mut c = crate::process::command(ssh);
    c.args([
        "-T",
        "-o",
        "ServerAliveInterval=15",
        "-o",
        "ServerAliveCountMax=3",
        "-o",
        "ConnectTimeout=20",
    ]);
    // Extra options for ssh (a test's own known_hosts).
    if let Ok(extra) = std::env::var("GOND_SSH_OPTIONS") {
        c.args(extra.split_whitespace());
    }
    c.args(extra);
    // Its `Host` in Gond's ssh config, which ~/.ssh/config includes (put back if gone).
    let _ = crate::sshconfig::ensure_included();
    c.arg("--").arg(alias);
    if let (Some(ep), Ok(me)) = (askpass, std::env::current_exe()) {
        c.env("SSH_ASKPASS", me)
            .env("SSH_ASKPASS_REQUIRE", "force")
            .env("DISPLAY", ":0")
            .env(ASKPASS_ENV, ep)
            .env(ASKPASS_MACHINE_ENV, machine);
    }
    Ok(c)
}

/// Runs `c` to its end (`wait` at most): whether it went well, and the last thing it said on
/// stderr.
fn run_for(mut c: std::process::Command, wait: Duration) -> std::io::Result<(bool, String)> {
    use std::io::Read;
    c.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    let mut child = c.spawn()?;
    let mut err = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(e) = err.as_mut() {
            let _ = e.read_to_string(&mut s);
        }
        s
    });
    let end = std::time::Instant::now() + wait;
    let status = loop {
        if let Some(st) = child.try_wait()? {
            break Some(st);
        }
        if std::time::Instant::now() > end {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let said = reader.join().unwrap_or_default();
    let last = said
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .unwrap_or_default()
        .to_string();
    match status {
        Some(st) => Ok((st.success(), last)),
        None => Ok((false, "it didn't answer in time".into())),
    }
}

/// The command that carries a connection to `t`, and the name to say it by.
fn command(
    t: &Target,
    askpass: Option<&str>,
    machine: &str,
) -> std::io::Result<tokio::process::Command> {
    let script = script(data_name());
    let cmd = match t {
        Target::Wsl { distro } => {
            let wsl = which::which("wsl").map_err(|_| io_err("WSL isn't installed"))?;
            let mut c = crate::process::command(wsl);
            c.args(["-d", distro, "--exec", "/bin/sh", "-c", &script]);
            c
        }
        Target::Ssh(m) => {
            let mut c = ssh_command(&m.alias, askpass, machine, &[])?;
            c.arg(format!("exec /bin/sh -c '{script}'"));
            c
        }
    };
    // `process::command`: no console window, none of a parent agent's variables.
    let mut cmd = tokio::process::Command::from(cmd);
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    Ok(cmd)
}

/// Reads lines until one starts with `tag` (a login script may print first); its rest.
async fn tagged(
    rd: &mut BufReader<tokio::process::ChildStdout>,
    tag: &str,
    wait: Duration,
) -> std::io::Result<String> {
    let read = async {
        let mut line = String::new();
        for _ in 0..200 {
            line.clear();
            if rd.read_line(&mut line).await? == 0 {
                return Err(io_err("the connection closed"));
            }
            if let Some(rest) = line.trim_end().strip_prefix(tag) {
                return Ok(rest.to_string());
            }
        }
        Err(io_err("the machine didn't answer as Gond expects"))
    };
    tokio::time::timeout(wait, read)
        .await
        .map_err(|_| io_err("the machine didn't answer in time"))?
}

/// Starts the program for `t`, puts the helper there if it isn't, and returns the stream to
/// its host, with where the helper is.
async fn open(
    t: Target,
    machine: String,
    askpass: Option<String>,
    report: Report,
) -> std::io::Result<(Stream, Boot)> {
    report(match &t {
        Target::Wsl { .. } => "Starting WSL",
        Target::Ssh(_) => "Connecting",
    });
    let mut child = command(&t, askpass.as_deref(), &machine)?.spawn()?;
    let stdout = child.stdout.take().ok_or_else(|| io_err("no output"))?;
    let mut wr = child.stdin.take().ok_or_else(|| io_err("no input"))?;
    // What the program says when it fails (ssh: "Permission denied…").
    let said = Arc::new(Mutex::new(String::new()));
    if let Some(err) = child.stderr.take() {
        let said = said.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                let mut s = said.lock().unwrap_or_else(|e| e.into_inner());
                s.push_str(&l);
                s.push('\n');
                if s.len() > 4096 {
                    let cut = s.len() - 4096;
                    s.drain(..cut);
                }
            }
        });
    }
    let failed = |e: std::io::Error| {
        // Give stderr a moment to arrive: it says why far better than a closed pipe.
        std::thread::sleep(Duration::from_millis(300));
        let s = said
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .trim()
            .to_string();
        let last: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
        if last.is_empty() {
            e
        } else {
            io_err(last[last.len().saturating_sub(3)..].join(" "))
        }
    };
    let mut rd = BufReader::new(stdout);
    // Signing in can take as long as you take to answer.
    let arch = tagged(&mut rd, "gond:arch ", Duration::from_secs(300))
        .await
        .map_err(failed)?;
    let mut parts = arch.splitn(3, ' ');
    let (os, arch, home) = (
        parts.next().unwrap_or_default().to_string(),
        parts.next().unwrap_or_default().to_string(),
        parts.next().unwrap_or_default().to_string(),
    );
    let bin = (os == "Linux").then(|| helper(&arch)).flatten();
    let Some(bin) = bin else {
        let _ = wr.write_all(b"none\n").await;
        return Err(io_err(if os == "Linux" {
            format!("Gond's helper doesn't run on {arch} yet")
        } else {
            format!("Gond's helper runs on Linux; this machine is {os}")
        }));
    };
    let hash = hash(&bin)?;
    wr.write_all(format!("{hash}\n").as_bytes()).await?;
    wr.flush().await?;
    let mut said_next = tagged(&mut rd, "gond:", Duration::from_secs(60))
        .await
        .map_err(failed)?;
    if said_next == "need" {
        let bytes = std::fs::read(&bin)?;
        report(&format!(
            "Copying Gond's helper ({:.1} MB)",
            bytes.len() as f64 / 1_048_576.0
        ));
        wr.write_all(format!("{}\n", bytes.len()).as_bytes())
            .await?;
        wr.write_all(&bytes).await?;
        wr.flush().await?;
        said_next = tagged(&mut rd, "gond:", Duration::from_secs(300))
            .await
            .map_err(failed)?;
    }
    match said_next.as_str() {
        "ok" => {}
        "bad" => return Err(io_err("Gond's helper arrived damaged; try again")),
        other => return Err(io_err(format!("the machine said {other:?}"))),
    }
    report("Starting the terminal host");
    let stream: Stream = Box::new(ChildStream {
        rd,
        wr,
        _child: child,
    });
    Ok((stream, Boot { home, hash }))
}

/// A connection's state, as the window shows it.
#[derive(Debug, Clone, PartialEq)]
enum Link {
    Connecting(String),
    Up,
    /// Out of reach; `retry`: Gond is trying again by itself.
    Down {
        error: String,
        retry: bool,
    },
    /// Disconnected on purpose.
    Off,
}

/// Tells the window: `/machines/<id>` or `/askpass/<id>` set to a value (null removes it).
pub type Notify = Box<dyn Fn(&str, Value) + Send + Sync>;
/// Runs a WSL distribution's keepalive in Gond's own terminal host.
pub type Keepalive = Box<dyn Fn(&str, Spawn) + Send + Sync>;

/// Every machine's connection.
pub struct Hub {
    sink: Arc<dyn PtySink>,
    me: Weak<Hub>,
    clients: Mutex<HashMap<String, Arc<HostClient>>>,
    links: Mutex<HashMap<String, Link>>,
    /// How many times each machine has come up: a pane reattaches when it changes.
    ups: Mutex<HashMap<String, u64>>,
    boots: Mutex<HashMap<String, Boot>>,
    probes: Mutex<HashMap<String, Value>>,
    /// One connect at a time per machine.
    connecting: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    retrying: Mutex<std::collections::HashSet<String>>,
    /// When each machine's last connect failed, and why.
    failed: Mutex<HashMap<String, (std::time::Instant, String)>>,
    /// Machines that asked for a password while signing in: a key would spare you that.
    passwords: Mutex<std::collections::HashSet<String>>,
    /// Tells the window (`/machines/<id>`, `/askpass/<id>`).
    notify: Notify,
    /// Keeps a WSL distribution up from Gond's own host: (machine, the keepalive to run).
    keep: Mutex<Option<Keepalive>>,
    askpass: Askpass,
}

impl Hub {
    /// `notify(path, value)` sets `/machines/<id>` or `/askpass/<id>` in the state tree (null
    /// removes it).
    pub fn new(sink: Arc<dyn PtySink>, notify: Notify) -> Arc<Hub> {
        Arc::new_cyclic(|me| Hub {
            sink,
            me: me.clone(),
            clients: Mutex::default(),
            links: Mutex::default(),
            ups: Mutex::default(),
            boots: Mutex::default(),
            probes: Mutex::default(),
            connecting: Mutex::default(),
            retrying: Mutex::default(),
            failed: Mutex::default(),
            passwords: Mutex::default(),
            notify,
            keep: Mutex::new(None),
            askpass: Askpass::default(),
        })
    }

    /// How to keep a WSL distribution up: run this keepalive in Gond's own terminal host.
    pub fn set_keepalive(&self, keep: Keepalive) {
        *lock(&self.keep) = Some(keep);
    }

    fn set(&self, id: &str, link: Link) {
        lock(&self.links).insert(id.to_string(), link.clone());
        let (state, step, error, retry) = match &link {
            Link::Connecting(s) => ("connecting", Some(s.clone()), None, false),
            Link::Up => ("up", None, None, false),
            Link::Down { error, retry } => ("down", None, Some(error.clone()), *retry),
            Link::Off => ("off", None, None, false),
        };
        let kind = if id.starts_with("wsl:") { "wsl" } else { "ssh" };
        let mut v = json!({
            "id": id, "kind": kind, "name": name(id), "state": state,
            "up": lock(&self.ups).get(id).copied().unwrap_or(0),
        });
        if let Some(s) = step {
            v["step"] = json!(s);
        }
        if let Some(e) = error {
            v["error"] = json!(e);
        }
        if retry {
            v["retrying"] = json!(true);
        }
        if lock(&self.passwords).contains(id) {
            v["password"] = json!(true);
        }
        if let Some(p) = lock(&self.probes).get(id) {
            v["probe"] = p.clone();
        }
        (self.notify)(id, v);
    }

    /// Machine `id`'s client, made the first time.
    pub fn client(&self, id: &str) -> AppResult<Arc<HostClient>> {
        if let Some(c) = lock(&self.clients).get(id) {
            return Ok(c.clone());
        }
        let t = target(id)?;
        let me = self.me.clone();
        let (machine, boot_into) = (id.to_string(), self.me.clone());
        let dial: Dial = Arc::new(move || {
            let (t, machine, me, boot_into) =
                (t.clone(), machine.clone(), me.clone(), boot_into.clone());
            Box::pin(async move {
                let hub = me.upgrade().ok_or_else(|| io_err("closing"))?;
                // A new try: you're asked again, and it's yet to be seen how it signs in.
                lock(&hub.askpass.cancelled).remove(&machine);
                lock(&hub.passwords).remove(&machine);
                let askpass = match &t {
                    Target::Ssh(_) => Some(hub.askpass.endpoint(&hub.me)?),
                    Target::Wsl { .. } => None,
                };
                let report: Report = {
                    let (me, m) = (me.clone(), machine.clone());
                    Arc::new(move |step: &str| {
                        if let Some(h) = me.upgrade() {
                            h.set(&m, Link::Connecting(step.to_string()));
                        }
                    })
                };
                let (stream, boot) = open(t, machine.clone(), askpass, report).await?;
                if let Some(h) = boot_into.upgrade() {
                    lock(&h.boots).insert(machine, boot);
                }
                Ok(stream)
            })
        });
        let down: OnDown = {
            let (me, m) = (self.me.clone(), id.to_string());
            Arc::new(move || {
                if let Some(h) = me.upgrade() {
                    h.lost(&m);
                }
            })
        };
        let c = Arc::new(HostClient::remote(
            id.to_string(),
            dial,
            self.sink.clone(),
            down,
        ));
        lock(&self.clients).insert(id.to_string(), c.clone());
        Ok(c)
    }

    /// Puts a key's `line` in machine `id`'s `~/.ssh/authorized_keys`, over its connection
    /// (made now if need be: ssh asks for the password one last time). A host there from an
    /// older Gond doesn't know how: then a one-off `ssh` does it, which may ask again. → how.
    pub fn authorize(&self, id: &str, line: &str) -> AppResult<&'static str> {
        let c = self.ensure(id)?;
        match c.authorized_keys(Some(line), None) {
            Ok(_) => Ok("host"),
            Err(e) if e.to_string().contains("doesn't know that request") => {
                let Target::Ssh(m) = target(id)? else {
                    return Err(AppError::Invalid("only an SSH machine takes a key".into()));
                };
                // Plain characters only (`sshkeys::authorized_line`): safe in double quotes.
                let key = line.split_whitespace().nth(1).unwrap_or_default();
                let script = format!(
                    "umask 077; mkdir -p ~/.ssh && touch ~/.ssh/authorized_keys && \
                     {{ grep -qF \"{key}\" ~/.ssh/authorized_keys || \
                     {{ [ -z \"$(tail -c1 ~/.ssh/authorized_keys)\" ] || echo >> ~/.ssh/authorized_keys; \
                     echo \"{line}\" >> ~/.ssh/authorized_keys; }}; }}"
                );
                lock(&self.askpass.cancelled).remove(id);
                let ep = self.askpass.endpoint(&self.me)?;
                let mut cmd = ssh_command(&m.alias, Some(&ep), id, &[])?;
                cmd.arg(format!("exec /bin/sh -c '{script}'"));
                match run_for(cmd, Duration::from_secs(120))? {
                    (true, _) => Ok("ssh"),
                    (false, why) => Err(AppError::PtyFailed(format!(
                        "Couldn't add the key on {}: {why}",
                        name(id)
                    ))),
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Takes a key (its base64) out of machine `id`'s `~/.ssh/authorized_keys`.
    pub fn revoke(&self, id: &str, key: &str) -> AppResult<()> {
        let c = self.ensure(id)?;
        c.authorized_keys(None, Some(key)).map(|_| ()).map_err(|e| {
            if e.to_string().contains("doesn't know that request") {
                AppError::Invalid(format!(
                    "{}'s terminal host is from an older Gond: close all terminals there first",
                    name(id)
                ))
            } else {
                e
            }
        })
    }

    /// Signs in to machine `id` with the key at `path` alone, and nothing else (no password):
    /// whether the machine takes it. A key with a passphrase asks for it in the window.
    pub fn check_key(&self, id: &str, path: &Path, passphrase: bool) -> AppResult<()> {
        let Target::Ssh(m) = target(id)? else {
            return Err(AppError::Invalid("only an SSH machine takes a key".into()));
        };
        let file = path.to_string_lossy().into_owned();
        let mut opts = vec![
            "-o",
            "PreferredAuthentications=publickey",
            "-o",
            "IdentitiesOnly=yes",
            "-o",
            "PasswordAuthentication=no",
            "-o",
            "KbdInteractiveAuthentication=no",
            "-i",
            &file,
        ];
        let ep = if passphrase {
            lock(&self.askpass.cancelled).remove(id);
            Some(self.askpass.endpoint(&self.me)?)
        } else {
            opts.extend(["-o", "BatchMode=yes"]);
            None
        };
        let mut cmd = ssh_command(&m.alias, ep.as_deref(), id, &opts)?;
        cmd.arg("exit 0");
        match run_for(cmd, Duration::from_secs(60))? {
            (true, _) => {
                // Signed in with the key: no password to warn about any more.
                lock(&self.passwords).remove(id);
                // Read first: `set` takes the lock again.
                let now = lock(&self.links).get(id).cloned();
                if let Some(l) = now {
                    self.set(id, l);
                }
                Ok(())
            }
            (false, why) => Err(AppError::PtyFailed(format!(
                "{} didn't take the key: {why}",
                name(id)
            ))),
        }
    }

    /// Machine `id`'s client, connected (now, if it wasn't).
    pub fn ensure(&self, id: &str) -> AppResult<Arc<HostClient>> {
        let c = self.client(id)?;
        if c.connected() {
            return Ok(c);
        }
        let asked = std::time::Instant::now();
        let gate = lock(&self.connecting)
            .entry(id.to_string())
            .or_default()
            .clone();
        let _one = lock(&gate);
        if c.connected() {
            return Ok(c);
        }
        // A connect that failed while this one waited its turn is this one's answer too: it
        // doesn't dial (and ask you to sign in) again straight after.
        if let Some((when, why)) = lock(&self.failed).get(id).cloned() {
            if when > asked {
                return Err(AppError::PtyFailed(format!(
                    "Couldn't reach {}: {why}",
                    name(id)
                )));
            }
        }
        self.set(id, Link::Connecting("Connecting".into()));
        match c.connect() {
            Ok(()) => {
                let probe = c.probe().ok();
                if let Some(p) = &probe {
                    lock(&self.probes).insert(id.to_string(), json!(p));
                }
                *lock(&self.ups).entry(id.to_string()).or_default() += 1;
                self.set(id, Link::Up);
                self.keepalive(id);
                Ok(c)
            }
            Err(e) => {
                let msg = e.to_string();
                let msg = msg
                    .strip_prefix("unreachable: ")
                    .unwrap_or(&msg)
                    .to_string();
                // You said no: that's the reason, not what ssh made of it.
                let msg = if lock(&self.askpass.cancelled).contains(id) {
                    "Signing in was cancelled".to_string()
                } else {
                    msg
                };
                lock(&self.failed).insert(id.to_string(), (std::time::Instant::now(), msg.clone()));
                self.set(
                    id,
                    Link::Down {
                        error: msg.clone(),
                        retry: false,
                    },
                );
                Err(AppError::PtyFailed(format!(
                    "Couldn't reach {}: {msg}",
                    name(id)
                )))
            }
        }
    }

    /// For a WSL distribution: Gond's own host keeps it up while the host there runs.
    fn keepalive(&self, id: &str) {
        let Some(distro) = id.strip_prefix("wsl:") else {
            return;
        };
        let Some(boot) = lock(&self.boots).get(id).cloned() else {
            return;
        };
        let Ok(wsl) = which::which("wsl") else { return };
        let spawn = Spawn::new(wsl, std::env::temp_dir()).args([
            "-d".to_string(),
            distro.to_string(),
            "--exec".into(),
            boot.helper(),
            "--data".into(),
            boot.data(),
            "keepalive".into(),
        ]);
        if let Some(keep) = &*lock(&self.keep) {
            keep(id, spawn);
        }
    }

    /// The connection to `id` dropped: try again by itself, unless signing in failed.
    fn lost(&self, id: &str) {
        if matches!(lock(&self.links).get(id), Some(Link::Off)) {
            return;
        }
        self.set(
            id,
            Link::Down {
                error: "The connection was lost".into(),
                retry: true,
            },
        );
        self.retry(id);
    }

    fn retry(&self, id: &str) {
        if !lock(&self.retrying).insert(id.to_string()) {
            return;
        }
        let (me, id) = (self.me.clone(), id.to_string());
        std::thread::spawn(move || {
            for wait in [1u64, 2, 5, 10, 20, 30, 30, 60, 60, 120] {
                std::thread::sleep(Duration::from_secs(wait));
                let Some(h) = me.upgrade() else { return };
                if matches!(lock(&h.links).get(&id), Some(Link::Off) | Some(Link::Up)) {
                    break;
                }
                match h.ensure(&id) {
                    Ok(_) => break,
                    // Signing in failed or was cancelled: asking again by itself would nag.
                    Err(e) if signing_in(&e.to_string()) => break,
                    Err(_) => {
                        // Read, then let go of the lock: `set` takes it again.
                        let now = lock(&h.links).get(&id).cloned();
                        if let Some(Link::Down { error, .. }) = now {
                            h.set(&id, Link::Down { error, retry: true });
                        }
                    }
                }
            }
            if let Some(h) = me.upgrade() {
                lock(&h.retrying).remove(&id);
                let now = lock(&h.links).get(&id).cloned();
                if let Some(Link::Down { error, .. }) = now {
                    h.set(
                        &id,
                        Link::Down {
                            error,
                            retry: false,
                        },
                    );
                }
            }
        });
    }

    /// Connects `id` (Reconnect, Connect).
    pub fn connect(&self, id: &str) -> AppResult<()> {
        self.ensure(id).map(|_| ())
    }

    /// Drops `id`'s connection; its terminals keep running there.
    pub fn disconnect(&self, id: &str) {
        if let Some(c) = lock(&self.clients).get(id) {
            c.disconnect();
        }
        if lock(&self.links).contains_key(id) {
            self.set(id, Link::Off);
        }
    }

    /// Closes every terminal on `id` and ends its host.
    pub fn close_all(&self, id: &str) -> AppResult<()> {
        let c = self.ensure(id)?;
        // Off first: the host ending drops the connection, which isn't a lost one to retry.
        self.set(id, Link::Off);
        let _ = c.shutdown();
        c.disconnect();
        Ok(())
    }

    /// Forgets `id` (a machine removed).
    pub fn forget(&self, id: &str) {
        if let Some(c) = lock(&self.clients).remove(id) {
            c.disconnect();
        }
        lock(&self.links).remove(id);
        (self.notify)(id, Value::Null);
    }

    /// Every connected machine's client.
    pub fn connected(&self) -> Vec<Arc<HostClient>> {
        lock(&self.clients)
            .values()
            .filter(|c| c.connected())
            .cloned()
            .collect()
    }

    /// Drops every connection (the app is quitting); the terminals keep running there.
    pub fn detach(&self) {
        for c in lock(&self.clients).values() {
            c.disconnect();
        }
    }

    /// Answers ssh's question `id` (`None`: cancelled).
    pub fn answer(&self, id: &str, answer: Option<String>) -> AppResult<()> {
        self.askpass.answer(id, answer)
    }
}

/// Whether a failure is about signing in (wrong password, cancelled, a key refused).
fn signing_in(msg: &str) -> bool {
    let m = msg.to_lowercase();
    m.contains("permission denied")
        || m.contains("cancel")
        || m.contains("host key verification failed")
        || m.contains("authentication")
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ---- ssh's questions (SSH_ASKPASS) ----

/// Where ssh's asker (this exe) reaches the app.
pub const ASKPASS_ENV: &str = "GOND_ASKPASS";
/// The machine it's asking for.
pub const ASKPASS_MACHINE_ENV: &str = "GOND_ASKPASS_MACHINE";

/// The app's end: questions from the asker go to the window (`/askpass/<id>`) and wait there
/// for `machine.askpass.answer`.
#[derive(Default)]
struct Askpass {
    endpoint: Mutex<Option<String>>,
    waiting: Mutex<HashMap<String, std::sync::mpsc::Sender<Option<String>>>>,
    /// Which machine each waiting question is for.
    asked_for: Mutex<HashMap<String, String>>,
    /// Machines whose sign-in you cancelled: ssh's next questions in that try are cancelled too
    /// (it asks for a password up to three times). Cleared when the next connect starts.
    cancelled: Mutex<std::collections::HashSet<String>>,
    rt: Mutex<Option<tokio::runtime::Runtime>>,
}

#[derive(Serialize, Deserialize)]
struct Question {
    prompt: String,
    #[serde(default)]
    machine: String,
}

#[derive(Serialize, Deserialize)]
struct Reply {
    #[serde(default)]
    answer: Option<String>,
}

impl Askpass {
    /// Listens (once) and says where.
    fn endpoint(&self, hub: &Weak<Hub>) -> std::io::Result<String> {
        if let Some(ep) = &*lock(&self.endpoint) {
            return Ok(ep.clone());
        }
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .thread_name("askpass")
            .enable_all()
            .build()?;
        let nonce = ulid::Ulid::generate().to_string().to_lowercase();
        #[cfg(windows)]
        let ep = format!(
            r"\\.\pipe\{}-askpass-{nonce}",
            crate::channel::CHANNEL.identifier()
        );
        #[cfg(not(windows))]
        let ep = std::env::temp_dir()
            .join(format!("gond-askpass-{nonce}.sock"))
            .to_string_lossy()
            .into_owned();
        let (hub, at) = (hub.clone(), ep.clone());
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        rt.spawn(async move {
            let r = serve_askpass(&at, hub, ready_tx.clone()).await;
            if let Err(e) = r {
                let _ = ready_tx.send(Err(e));
            }
        });
        ready_rx
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| io_err("the sign-in prompt didn't start"))??;
        *lock(&self.rt) = Some(rt);
        *lock(&self.endpoint) = Some(ep.clone());
        Ok(ep)
    }

    fn answer(&self, id: &str, answer: Option<String>) -> AppResult<()> {
        let tx = lock(&self.waiting)
            .remove(id)
            .ok_or_else(|| AppError::NotFound(format!("no question {id}")))?;
        let machine = lock(&self.asked_for).remove(id);
        if let (None, Some(m)) = (&answer, machine) {
            lock(&self.cancelled).insert(m);
        }
        let _ = tx.send(answer);
        Ok(())
    }
}

/// A question from the asker: shown in the window until it's answered (5 minutes at most).
async fn ask_window(hub: &Weak<Hub>, q: Question) -> Option<String> {
    let h = hub.upgrade()?;
    if lock(&h.askpass.cancelled).contains(&q.machine) {
        return None;
    }
    let id = ulid::Ulid::generate().to_string();
    let (tx, rx) = std::sync::mpsc::channel();
    lock(&h.askpass.waiting).insert(id.clone(), tx);
    lock(&h.askpass.asked_for).insert(id.clone(), q.machine.clone());
    let lower = q.prompt.to_lowercase();
    let confirm = lower.contains("(yes/no");
    if !confirm && lower.contains("password") {
        lock(&h.passwords).insert(q.machine.clone());
    }
    (h.notify)(
        &format!("askpass:{id}"),
        json!({
            "id": id, "machine": q.machine, "machineName": name(&q.machine),
            "prompt": q.prompt.trim(), "kind": if confirm { "confirm" } else { "secret" },
        }),
    );
    let link = lock(&h.links).get(&q.machine).cloned();
    if matches!(link, Some(Link::Connecting(_))) {
        h.set(
            &q.machine,
            Link::Connecting("Waiting for you to sign in".into()),
        );
    }
    drop(h);
    let got = tokio::task::spawn_blocking(move || rx.recv_timeout(Duration::from_secs(300)).ok())
        .await
        .ok()
        .flatten()
        .flatten();
    if let Some(h) = hub.upgrade() {
        lock(&h.askpass.waiting).remove(&id);
        (h.notify)(&format!("askpass:{id}"), Value::Null);
        if lock(&h.links)
            .get(&q.machine)
            .is_some_and(|l| matches!(l, Link::Connecting(_)))
        {
            h.set(&q.machine, Link::Connecting("Signing in".into()));
        }
    }
    got
}

async fn handle_asker<S: AsyncRead + AsyncWrite + Unpin>(hub: Weak<Hub>, conn: S) {
    let (rd, mut wr) = tokio::io::split(conn);
    let mut lines = BufReader::new(rd).lines();
    let Ok(Some(text)) = lines.next_line().await else {
        return;
    };
    let Ok(q) = serde_json::from_str::<Question>(&text) else {
        return;
    };
    let answer = ask_window(&hub, q).await;
    let reply = serde_json::to_string(&Reply { answer }).unwrap_or_default() + "\n";
    let _ = wr.write_all(reply.as_bytes()).await;
    let _ = wr.flush().await;
}

#[cfg(windows)]
async fn serve_askpass(
    ep: &str,
    hub: Weak<Hub>,
    ready: std::sync::mpsc::Sender<std::io::Result<()>>,
) -> std::io::Result<()> {
    use tokio::net::windows::named_pipe::ServerOptions;
    let sid = crate::pty::host::win::user_sid().ok_or_else(|| io_err("no user SID"))?;
    let mut sec = crate::pty::host::win::Security::only(&sid)?;
    let create = |first: bool, sec: &mut crate::pty::host::win::Security| {
        // SAFETY: `sec` holds a valid SECURITY_ATTRIBUTES for the duration of the call.
        unsafe {
            ServerOptions::new()
                .first_pipe_instance(first)
                .reject_remote_clients(true)
                .create_with_security_attributes_raw(ep, sec.as_ptr())
        }
    };
    let mut server = create(true, &mut sec)?;
    let _ = ready.send(Ok(()));
    loop {
        server.connect().await?;
        let next = create(false, &mut sec)?;
        let conn = std::mem::replace(&mut server, next);
        tokio::spawn(handle_asker(hub.clone(), conn));
    }
}

#[cfg(not(windows))]
async fn serve_askpass(
    ep: &str,
    hub: Weak<Hub>,
    ready: std::sync::mpsc::Sender<std::io::Result<()>>,
) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let listener = tokio::net::UnixListener::bind(ep)?;
    std::fs::set_permissions(ep, std::fs::Permissions::from_mode(0o600))?;
    let _ = ready.send(Ok(()));
    loop {
        let (conn, _) = listener.accept().await?;
        tokio::spawn(handle_asker(hub.clone(), conn));
    }
}

/// `gond-code` run by ssh as its asker (`SSH_ASKPASS`): the question is our argument; it goes
/// to the app, and the answer to our output. Cancelled: exit 1, and ssh gives up.
pub fn askpass_main(ep: &str) -> i32 {
    let prompt = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    let machine = std::env::var(ASKPASS_MACHINE_ENV).unwrap_or_default();
    let ask = async {
        #[cfg(windows)]
        let conn = tokio::net::windows::named_pipe::ClientOptions::new().open(ep)?;
        #[cfg(not(windows))]
        let conn = tokio::net::UnixStream::connect(ep).await?;
        let (rd, mut wr) = tokio::io::split(conn);
        let q = serde_json::to_string(&Question { prompt, machine }).unwrap_or_default() + "\n";
        wr.write_all(q.as_bytes()).await?;
        wr.flush().await?;
        let mut line = String::new();
        BufReader::new(rd).read_line(&mut line).await?;
        Ok::<_, std::io::Error>(
            serde_json::from_str::<Reply>(&line)
                .ok()
                .and_then(|r| r.answer),
        )
    };
    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return 1;
    };
    match rt.block_on(ask) {
        Ok(Some(answer)) => {
            use std::io::Write;
            let mut out = std::io::stdout();
            let _ = writeln!(out, "{answer}");
            let _ = out.flush();
            0
        }
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The script ssh runs has no single quote (ssh's remote command wraps it in them), and
    /// ends by running the helper.
    #[test]
    fn the_start_script_fits_in_single_quotes() {
        let s = script(".gond-dev");
        assert!(!s.contains('\''), "{s}");
        assert!(s.ends_with("exec \"$h\" --data \"$d\" attach"));
        assert!(s.contains("$HOME/.gond-dev"));
    }

    /// The handshake against the real script: it asks for the helper, takes exactly its bytes,
    /// checks them, and runs it (here, a stand-in that answers one line).
    #[cfg(unix)]
    #[test]
    fn the_start_script_installs_and_runs_the_helper() {
        use std::io::{BufRead, Write};
        let home = tempfile::tempdir().unwrap();
        let helper = b"#!/bin/sh\necho helper-ran \"$@\"\n";
        let want: String = {
            use sha2::Digest;
            sha2::Sha256::digest(helper)
                .iter()
                .take(8)
                .map(|b| format!("{b:02x}"))
                .collect()
        };
        let mut sh = std::process::Command::new("sh")
            .args(["-c", &script(".gond-test")])
            .env("HOME", home.path())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = sh.stdin.take().unwrap();
        let mut out = std::io::BufReader::new(sh.stdout.take().unwrap());
        let mut line = String::new();
        out.read_line(&mut line).unwrap();
        assert!(line.starts_with("gond:arch "), "{line}");
        writeln!(input, "{want}").unwrap();
        line.clear();
        out.read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "gond:need");
        writeln!(input, "{}", helper.len()).unwrap();
        input.write_all(helper).unwrap();
        input.flush().unwrap();
        line.clear();
        out.read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "gond:ok");
        line.clear();
        out.read_line(&mut line).unwrap();
        assert!(line.starts_with("helper-ran --data"), "{line}");
        assert!(line.trim().ends_with("attach"), "{line}");
        let _ = sh.wait();
        assert!(home
            .path()
            .join(".gond-test/bin")
            .join(&want)
            .join("gond-remote")
            .is_file());
    }
}
