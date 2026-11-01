//! No console windows in the setup flows (stage 2), on Windows.
//!
//! The app is a GUI process with no console, so any child started without CREATE_NO_WINDOW
//! gets a console window of its own. Here fake `gh.exe` and `winget.exe` (a tiny probe built
//! with rustc) record whether they got one, while the real GitHub sign-in and install flows
//! drive them. Every child must report no window. A child started without the flag must
//! report one, which proves the probe can see it.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::github::{GhCli, Github};
use crate::harness::{Plan, Source};
use crate::install::{self, JobState, Progress, Step};

extern "system" {
    fn FreeConsole() -> i32;
}

/// Logs `<name> hwnd=<console window> args=<argv>` next to itself, and answers just enough
/// of gh and winget for the flows to run.
const PROBE: &str = r#"
use std::io::{Read, Write};
extern "system" { fn GetConsoleWindow() -> isize; }
#[link(name = "user32")]
extern "system" { fn GetClassNameW(hwnd: isize, name: *mut u16, len: i32) -> i32; }
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let exe = std::env::current_exe().unwrap();
    let name = exe.file_stem().unwrap().to_string_lossy().to_string();
    let hwnd = unsafe { GetConsoleWindow() };
    // A pseudo console (ConPTY, how winget runs for its progress) has a window that is
    // never shown: its class is PseudoConsoleWindow.
    let mut buf = [0u16; 128];
    let n = if hwnd == 0 { 0 } else { unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), 128) } };
    let class = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
    // One record file per run: concurrent probes (gh in the background, winget) never share a file.
    let dir = exe.parent().unwrap().join("records");
    let _ = std::fs::create_dir_all(&dir);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let file = dir.join(format!("{}-{}-{}.log", name, std::process::id(), nanos));
    std::fs::File::create(file).unwrap()
        .write_all(format!("{} hwnd={} class={} args={}\n", name, hwnd, class, args.join(" ")).as_bytes()).unwrap();
    let a: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    match a.as_slice() {
        ["--version", ..] => println!("gh version 2.103.3 (2026-10-26)"),
        ["auth", "status", ..] => println!("{{\"hosts\":{{}}}}"),
        ["auth", "login", rest @ ..] if rest.contains(&"--with-token") => {
            let mut s = String::new();
            let _ = std::io::stdin().read_to_string(&mut s);
        }
        ["auth", "login", ..] => {
            eprintln!("! First copy your one-time code: AB12-CD34");
            eprintln!("Open this URL to continue in your web browser: https://github.com/login/device");
        }
        ["install", ..] => println!("100%"),
        _ => {}
    }
}
"#;

fn build_probes(dir: &Path) -> PathBuf {
    let src = dir.join("probe.rs");
    std::fs::write(&src, PROBE).unwrap();
    let gh = dir.join("gh.exe");
    let built = Command::new("rustc")
        .args(["--edition", "2021", "-O"])
        .arg(&src)
        .arg("-o")
        .arg(&gh)
        .status()
        .expect("rustc runs");
    assert!(built.success(), "the probe compiles");
    std::fs::copy(&gh, dir.join("winget.exe")).unwrap();
    gh
}

/// Every probe run's record, one file each (`records/<name>-<pid>-<nanos>.log`).
fn log(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir.join("records")) else {
        return Vec::new();
    };
    let mut lines: Vec<String> = entries
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .flat_map(|t| t.lines().map(String::from).collect::<Vec<_>>())
        .collect();
    lines.sort();
    lines
}

/// Waits until the probe log has a line matching `pred`, or `ms` runs out (a condition, not a delay).
fn wait_for_line(dir: &Path, ms: u64, pred: impl Fn(&str) -> bool) -> bool {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        if log(dir).iter().any(|l| pred(l)) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

struct Quiet;
impl Progress for Quiet {
    fn update(&self, _: &JobState) {}
}

#[test]
fn setup_flows_open_no_console_windows() {
    // Like the app: no console of our own (the CI runner's has no window, which would hide one).
    unsafe { FreeConsole() };
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let gh_exe = build_probes(dir);

    // The probe can see a window: a child without the flag gets one.
    Command::new(&gh_exe).arg("--version").output().unwrap();
    let windowed = log(dir);
    assert!(
        windowed.iter().any(|l| !l.contains("hwnd=0 ")),
        "a deliberately windowed child should report a console window: {windowed:?}"
    );
    std::fs::remove_dir_all(dir.join("records")).unwrap();

    let path = format!("{};C:\\Windows\\System32", dir.display());

    // GitHub sign-in: status, token, Git setup, switch, sign out, and the web flow.
    let cli = GhCli::with_path(path.clone());
    let _ = cli.status();
    let _ = cli.login_token("github.com", "ghp_fake");
    let _ = cli.setup_git("github.com");
    let _ = cli.switch("github.com", "someone");
    let _ = cli.logout("github.com", "someone");
    let github = Github::new(GhCli::with_path(path.clone()));
    // The web sign-in finishes in the background (it runs `gh auth setup-git` on success) and
    // reports through `done`: wait for that, so no gh is still running when winget starts.
    let (finished, done) = std::sync::mpsc::channel();
    let _ = github.login_web("github.com", move |_| {
        let _ = finished.send(());
    });
    assert!(
        wait_for_line(dir, 10_000, |l| l.contains("--web")),
        "the web sign-in ran gh"
    );
    let _ = done.recv_timeout(Duration::from_secs(20));

    // Installs: one winget step through the real install runner.
    let job = install::run(
        "e2e-no-console".into(),
        "Install Git".into(),
        vec![Step {
            label: "Install Git".into(),
            plan: Plan {
                source: Source::Winget,
                program: "winget".into(),
                args: ["install", "--id", "Git.Git", "--scope", "user", "--silent"]
                    .map(String::from)
                    .into(),
                from: "winget".into(),
                adds_to_path: None,
                provides: None,
            },
        }],
        Some(path.into()),
        || None,
        Duration::from_secs(60),
        &Quiet,
    );
    github.cancel();

    let lines = log(dir);
    assert!(
        lines.iter().any(|l| l.starts_with("gh ")),
        "gh ran: {lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.starts_with("winget ")),
        "winget ran (job: {:?}): {lines:?}",
        job.status
    );
    // A pseudo console's window is never shown, so it isn't a console window to anyone.
    let windowed: Vec<_> = lines
        .iter()
        .filter(|l| !l.contains("hwnd=0 ") && !l.contains("class=PseudoConsoleWindow "))
        .collect();
    assert!(
        windowed.is_empty(),
        "children with a console window: {windowed:?} (all: {lines:?})"
    );
}
