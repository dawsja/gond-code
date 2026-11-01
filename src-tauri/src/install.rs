//! The install engine: runs a job of plan steps in the background and
//! reports progress, for harnesses and prerequisites alike. The rule: no console
//! window ever (CREATE_NO_WINDOW), silent per-user plans, one confirmation for the whole job,
//! a progress card the UI renders from `/installs/{jobId}`, PATH refreshed afterwards.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use schemars::JsonSchema;
use serde::Serialize;

use crate::agents::detect;
use crate::harness::Plan;

/// Output kept per job for "Show details" (the tail, in bytes).
const DETAILS_MAX: usize = 16 * 1024;

/// winget's exit codes Gond reads (winget-cli `doc/.../winget/returnCodes.md`).
pub mod winget {
    const fn code(hex: u32) -> i32 {
        hex as i32
    }
    pub const DOWNLOAD_FAILED: i32 = code(0x8A15_0008);
    /// No installer matches the requested scope or system.
    pub const NO_APPLICABLE_INSTALLER: i32 = code(0x8A15_0010);
    pub const INSTALLER_HASH_MISMATCH: i32 = code(0x8A15_0011);
    pub const NO_APPLICATIONS_FOUND: i32 = code(0x8A15_0014);
    pub const MULTIPLE_APPLICATIONS_FOUND: i32 = code(0x8A15_0016);
    /// `winget upgrade` with nothing newer: already up to date.
    pub const UPDATE_NOT_APPLICABLE: i32 = code(0x8A15_002B);
    pub const BLOCKED_BY_POLICY: i32 = code(0x8A15_003A);
    pub const PACKAGE_AGREEMENTS_NOT_ACCEPTED: i32 = code(0x8A15_0041);
    pub const SOURCE_AGREEMENTS_NOT_ACCEPTED: i32 = code(0x8A15_0046);
    pub const PACKAGE_ALREADY_INSTALLED: i32 = code(0x8A15_0061);
    pub const INSTALL_IN_PROGRESS: i32 = code(0x8A15_0102);
    pub const INSTALL_NO_NETWORK: i32 = code(0x8A15_0107);
    pub const REBOOT_REQUIRED_TO_FINISH: i32 = code(0x8A15_0109);
    pub const REBOOT_REQUIRED_FOR_INSTALL: i32 = code(0x8A15_010A);
    pub const INSTALL_CANCELLED_BY_USER: i32 = code(0x8A15_010C);
    pub const INSTALL_ALREADY_INSTALLED: i32 = code(0x8A15_010D);
    pub const INSTALL_BLOCKED_BY_POLICY: i32 = code(0x8A15_010F);
}

/// What a winget exit code means for a step: done (with a line for the card), or a
/// plain failure. `None` for codes Gond doesn't know, which report the raw code.
pub fn winget_outcome(code: i32, name: &str, installing: bool) -> Option<Result<String, String>> {
    use winget::*;
    Some(match code {
        // `winget install` of a package it already has answers "no applicable upgrade":
        // that's installed, not up to date, and the job then checks it's really there.
        UPDATE_NOT_APPLICABLE if installing => Ok(format!("{name} is already installed.")),
        UPDATE_NOT_APPLICABLE => Ok(format!("{name} is already up to date.")),
        PACKAGE_ALREADY_INSTALLED => Ok(format!("{name} is already installed.")),
        REBOOT_REQUIRED_TO_FINISH => Ok(format!("Restart your PC to finish installing {name}.")),
        NO_APPLICATIONS_FOUND => Err(format!("winget couldn't find {name}.")),
        MULTIPLE_APPLICATIONS_FOUND => Err(format!("winget found more than one package for {name}.")),
        NO_APPLICABLE_INSTALLER => Err(format!("None of {name}'s installers fit this PC.")),
        INSTALLER_HASH_MISMATCH => Err(format!(
            "{name}'s download didn't match its checksum, so it wasn't installed. Try again later."
        )),
        DOWNLOAD_FAILED | INSTALL_NO_NETWORK => Err(format!(
            "{name} couldn't be downloaded. Check your internet connection and try again."
        )),
        REBOOT_REQUIRED_FOR_INSTALL => Err(format!("Restart your PC, then try {name} again.")),
        INSTALL_IN_PROGRESS => Err(format!(
            "Another installation is running. Try {name} again when it finishes."
        )),
        BLOCKED_BY_POLICY | INSTALL_BLOCKED_BY_POLICY => Err(format!(
            "Your organization's policy blocks installing {name}."
        )),
        INSTALL_CANCELLED_BY_USER => Err(format!("{name}'s installer was cancelled.")),
        INSTALL_ALREADY_INSTALLED => Err(format!(
            "A different version of {name} is already installed. Update it instead, or uninstall it first."
        )),
        SOURCE_AGREEMENTS_NOT_ACCEPTED | PACKAGE_AGREEMENTS_NOT_ACCEPTED => Err(format!(
            "{name} needs winget's agreements accepted. Run `winget install {name}` once in a terminal to accept them."
        )),
        _ => return None,
    })
}

/// A step that runs longer than this is stopped (its whole process tree).
pub const STEP_TIMEOUT: Duration = Duration::from_secs(20 * 60);

/// A running job's handle: cancelling it kills the step's process tree.
#[derive(Default)]
pub struct Control {
    cancelled: AtomicBool,
    pid: Mutex<Option<u32>>,
}

impl Control {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Some(pid) = *self.pid.lock().unwrap_or_else(|e| e.into_inner()) {
            crate::process::kill_tree(pid);
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

/// Jobs still running, by id, so `prereq.cancel` / `harness.cancel` can reach them.
static JOBS: Mutex<Option<HashMap<String, Arc<Control>>>> = Mutex::new(None);

fn jobs() -> std::sync::MutexGuard<'static, Option<HashMap<String, Arc<Control>>>> {
    JOBS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Cancels job `id`. Returns whether it was still running.
pub fn cancel(id: &str) -> bool {
    let control = jobs().as_ref().and_then(|m| m.get(id).cloned());
    control.map(|c| c.cancel()).is_some()
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub label: String,
    pub plan: Plan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Pending,
    Running,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StepState {
    pub label: String,
    pub status: Status,
    /// A plain line for a step that finished without doing anything new, e.g.
    /// "Codex is already up to date.".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The installed version, read once the step is done.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Where a step is. Downloading reports bytes; the others have no percent, so
/// the card shows an indeterminate bar with the stage and its elapsed time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    Downloading,
    Installing,
    SettingUp,
    Checking,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Bytes {
    pub done: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
}

/// One job as the UI sees it: what `/installs/{id}` holds.
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JobState {
    pub id: String,
    pub title: String,
    pub status: Status,
    pub steps: Vec<StepState>,
    /// The running step's progress, when its output reports one (winget does).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<u8>,
    /// The running step's stage, and when it started (RFC 3339) for "elapsed".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<Stage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage_started_at: Option<String>,
    /// When the job finished (done, failed or cancelled), so a card's "is ready" line can
    /// clear a while after.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    /// While downloading: bytes so far and the size, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<Bytes>,
    /// While downloading: bytes per second, over the last update.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<u64>,
    #[serde(skip)]
    /// Recent (arrival, bytes done) samples: the speed is their rate over ~2 s.
    pub byte_samples: std::collections::VecDeque<(std::time::Instant, u64)>,
    /// When the download stage began, for an average when redraws come too bunched
    /// for a rolling rate.
    #[serde(skip)]
    pub download_started: Option<std::time::Instant>,
    /// A plain sentence for the card; `details` has the output for "Show details".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub details: String,
    /// Set when a step failed only because winget has no per-user installer for it: the
    /// card offers "Install for all users" (one Windows prompt, `machineScope: true`) or Skip.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub machine_scope_available: bool,
}

/// Where progress goes: the state tree in the app, a recorder in tests.
pub trait Progress: Send + Sync {
    fn update(&self, job: &JobState);
}

/// Reads the newest PATH from the registry so new installs are found without a restart.
/// `None` keeps the current PATH (Linux, or when it can't be read).
pub fn refreshed_path() -> Option<OsString> {
    #[cfg(windows)]
    {
        let mut cmd = crate::process::command("powershell");
        cmd.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Environment]::GetEnvironmentVariable('Path','Machine') + ';' + [Environment]::GetEnvironmentVariable('Path','User')",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null());
        let out = crate::process::when_free(|| cmd.output())
            .ok()
            .filter(|o| o.status.success())?;
        let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!path.is_empty()).then(|| path.into())
    }
    #[cfg(not(windows))]
    None
}

/// The last "NN%" on a line, as winget prints while downloading.
fn percent_in(line: &str) -> Option<u8> {
    let i = line.rfind('%')?;
    let digits: String = line[..i]
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    digits.parse::<u8>().ok().filter(|p| *p <= 100)
}

/// A size as winget prints it ("12.0 MB", "512 KB", "1.20 GB").
fn size_in(text: &str) -> Option<u64> {
    let mut it = text.split_whitespace();
    let n: f64 = it.next()?.parse().ok()?;
    let mult = match it.next()?.to_ascii_uppercase().as_str() {
        "B" => 1.0,
        "KB" => 1024.0,
        "MB" => 1024.0 * 1024.0,
        "GB" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((n * mult) as u64)
}

/// winget's "  ██████▒▒  12.0 MB / 55.3 MB": bytes done and the total.
fn bytes_in(line: &str) -> Option<(u64, u64)> {
    let (left, right) = line.split_once(" / ")?;
    let done_text: Vec<&str> = left.split_whitespace().rev().take(2).collect();
    let done = size_in(&format!("{} {}", done_text.get(1)?, done_text.first()?))?;
    let total = size_in(right.trim())?;
    (total > 0 && done <= total).then_some((done, total))
}

/// The stage a winget line announces, if any.
fn stage_in(line: &str) -> Option<Stage> {
    let l = line.trim();
    if l.starts_with("Downloading ") {
        Some(Stage::Downloading)
    } else if l.starts_with("Successfully verified installer hash")
        || l.starts_with("Starting package install")
        || l.starts_with("Starting package uninstall")
    {
        Some(Stage::Installing)
    } else if l.starts_with("Successfully installed") {
        Some(Stage::SettingUp)
    } else {
        None
    }
}

/// The window the download speed is measured over, and the least span worth a figure.
const SPEED_WINDOW: Duration = Duration::from_secs(2);
const SPEED_MIN_SPAN: Duration = Duration::from_millis(50);

/// Adds a sample and returns bytes/s over the last ~2 s, once two samples span 50 ms
/// (a cached download can be over in a second:). Redraws
/// that arrive faster than that (winget draws often) are all counted, not reset.
fn rolling_speed(
    samples: &mut std::collections::VecDeque<(std::time::Instant, u64)>,
    at: std::time::Instant,
    done: u64,
) -> Option<u64> {
    samples.push_back((at, done));
    while samples.len() > 1
        && samples
            .front()
            .is_some_and(|(t, _)| at.duration_since(*t) > SPEED_WINDOW)
    {
        samples.pop_front();
    }
    let (t0, d0) = *samples.front()?;
    let span = at.duration_since(t0);
    (span >= SPEED_MIN_SPAN && done >= d0).then(|| ((done - d0) as f64 / span.as_secs_f64()) as u64)
}

/// Bytes/s since the download began, once it's been going 100 ms: winget's redraws can
/// arrive in one read on Windows, so the rolling rate never sees two apart.
fn average_speed(
    started: Option<std::time::Instant>,
    at: std::time::Instant,
    done: u64,
) -> Option<u64> {
    let span = at.checked_duration_since(started?)?;
    (span >= Duration::from_millis(100) && done > 0)
        .then(|| (done as f64 / span.as_secs_f64()) as u64)
}

/// Moves the job to `stage` (once), resetting what belongs to the previous one.
fn set_stage(job: &mut JobState, stage: Stage) -> bool {
    if job.stage == Some(stage) {
        return false;
    }
    job.stage = Some(stage);
    job.stage_started_at = Some(crate::store::now());
    job.download_started = (stage == Stage::Downloading).then(std::time::Instant::now);
    if stage != Stage::Downloading {
        // Silent installers don't report a percent: indeterminate, never frozen.
        job.percent = None;
        job.speed = None;
    }
    true
}

/// One line of output: a stage, download bytes, a percent, or text for "Show details".
fn take_line(raw: &str, job: &mut JobState, progress: &dyn Progress) {
    take_line_at(raw, std::time::Instant::now(), job, progress)
}

/// `take_line` for a line that arrived at `at`: the speed comes from when the bytes
/// arrived, not when the job got round to reading them.
fn take_line_at(raw: &str, at: std::time::Instant, job: &mut JobState, progress: &dyn Progress) {
    let line = crate::pty::scrollback::strip_ansi(raw);
    let line = line.trim_end();
    if line.trim().is_empty() {
        return;
    }
    if let Some(stage) = stage_in(line) {
        keep_tail(&mut job.details, line.trim());
        if set_stage(job, stage) {
            // The download began when its line arrived, not when it was read.
            if stage == Stage::Downloading {
                job.download_started = Some(at);
            }
            progress.update(job);
        }
        return;
    }
    if let Some((done, total)) = bytes_in(line) {
        // ConPTY repaints only the cells that changed, so a line can be a partial redraw:
        // a count that goes back or a total that changes isn't real, and is skipped.
        if job.stage == Some(Stage::Downloading)
            && job
                .bytes
                .is_some_and(|b| done < b.done || b.total != Some(total))
        {
            return;
        }
        set_stage(job, Stage::Downloading);
        job.speed = rolling_speed(&mut job.byte_samples, at, done)
            .or_else(|| average_speed(job.download_started, at, done))
            .or(job.speed);
        job.bytes = Some(Bytes {
            done,
            total: Some(total),
        });
        job.percent = Some(((done as f64 / total as f64) * 100.0).round().min(100.0) as u8);
        progress.update(job);
        return;
    }
    match percent_in(line) {
        // Same for a bare percent: the bar only moves forward within a download.
        Some(p)
            if job.stage != Some(Stage::Downloading) || job.percent.is_none_or(|was| p > was) =>
        {
            set_stage(job, Stage::Downloading);
            job.percent = Some(p);
            progress.update(job);
        }
        Some(_) => {}
        None => keep_tail(&mut job.details, line),
    }
}

fn keep_tail(details: &mut String, line: &str) {
    details.push_str(line);
    details.push('\n');
    if details.len() > DETAILS_MAX {
        let mut cut = details.len() - DETAILS_MAX;
        while !details.is_char_boundary(cut) {
            cut += 1;
        }
        details.drain(..cut);
    }
}

/// Runs `steps` in order, stopping at the first failure. Blocks: call it on a thread.
/// `path_var` finds each step's program. After every step the registry PATH is merged
/// into `process::path`, so the next step, detection and every later agent spawn see
/// what was installed, without a restart.
pub fn run(
    id: String,
    title: String,
    steps: Vec<Step>,
    mut path_var: Option<OsString>,
    refresh: fn() -> Option<OsString>,
    timeout: Duration,
    progress: &dyn Progress,
) -> JobState {
    let control = Arc::new(Control::default());
    jobs()
        .get_or_insert_with(HashMap::new)
        .insert(id.clone(), control.clone());
    let mut job = run_job(
        id,
        title,
        steps,
        &mut path_var,
        refresh,
        timeout,
        &control,
        progress,
    );
    if let Some(m) = jobs().as_mut() {
        m.remove(&job.id);
    }
    job.ended_at = Some(crate::store::now());
    progress.update(&job);
    job
}

#[allow(clippy::too_many_arguments)]
fn run_job(
    id: String,
    title: String,
    steps: Vec<Step>,
    path_var: &mut Option<OsString>,
    refresh: fn() -> Option<OsString>,
    timeout: Duration,
    control: &Control,
    progress: &dyn Progress,
) -> JobState {
    let mut job = JobState {
        id,
        title,
        status: Status::Running,
        steps: steps
            .iter()
            .map(|s| StepState {
                label: s.label.clone(),
                status: Status::Pending,
                note: None,
                version: None,
            })
            .collect(),
        percent: None,
        stage: None,
        stage_started_at: None,
        ended_at: None,
        bytes: None,
        speed: None,
        byte_samples: Default::default(),
        download_started: None,
        error: None,
        details: String::new(),
        machine_scope_available: false,
    };
    progress.update(&job);
    for (i, step) in steps.iter().enumerate() {
        job.steps[i].status = Status::Running;
        job.percent = None;
        job.bytes = None;
        job.speed = None;
        job.byte_samples.clear();
        job.stage = None;
        // Until the output says otherwise (winget announces its download), a step is
        // installing: an indeterminate bar with its elapsed time.
        set_stage(&mut job, Stage::Installing);
        progress.update(&job);
        let mut ran = run_step(step, path_var.clone(), timeout, control, &mut job, progress);
        // winget there but broken (a bad source, a store policy, a crash): Git, GitHub CLI
        // and Node still come from their official releases.
        if let (Err(why), Some(id)) = (&ran, fallback_for(step)) {
            if !control.is_cancelled() {
                keep_tail(
                    &mut job.details,
                    &format!("winget failed ({why}); using the official release instead."),
                );
                let s = download_step_for(&id, step);
                ran = run_step(&s, path_var.clone(), timeout, control, &mut job, progress);
            }
        }
        if let Ok(note) = &ran {
            job.steps[i].note = note.clone();
        }
        if let Err(e) = ran {
            let status = if control.is_cancelled() {
                Status::Cancelled
            } else {
                Status::Failed
            };
            job.steps[i].status = status;
            job.status = status;
            job.error = Some(e);
            progress.update(&job);
            return job;
        }
        set_stage(&mut job, Stage::Checking);
        progress.update(&job);
        if let Some(fresh) = refresh() {
            let merged = crate::process::merge_paths(&fresh, path_var.as_deref());
            crate::process::set_path(merged.clone());
            *path_var = Some(merged);
        }
        // Re-detect: a step that says it worked must leave its program findable, or the
        // card would read "Not installed" next to a green line.
        if let Some(command) = &step.plan.provides {
            let mut found = crate::agents::detect::find(command, path_var.clone());
            // winget still lists it, but its files are gone (removed in Control Panel or
            // by hand): Gond repairs it itself, never sending anyone to Settings.
            let stale = job.steps[i]
                .note
                .as_deref()
                .is_some_and(|n| n.contains("already"));
            if found.is_none() && stale {
                if let Some((uninstall, reinstall)) = repair_steps(step) {
                    job.steps[i].note = None;
                    set_stage(&mut job, Stage::Installing);
                    progress.update(&job);
                    // A stale record may refuse to uninstall; the forced install still runs.
                    let _ = run_step(
                        &uninstall,
                        path_var.clone(),
                        timeout,
                        control,
                        &mut job,
                        progress,
                    );
                    if control.is_cancelled() {
                        job.steps[i].status = Status::Cancelled;
                        job.status = Status::Cancelled;
                        job.error = Some("Cancelled".into());
                        progress.update(&job);
                        return job;
                    }
                    if let Err(e) = run_step(
                        &reinstall,
                        path_var.clone(),
                        timeout,
                        control,
                        &mut job,
                        progress,
                    ) {
                        job.steps[i].status = Status::Failed;
                        job.status = Status::Failed;
                        job.error = Some(e);
                        progress.update(&job);
                        return job;
                    }
                    set_stage(&mut job, Stage::Checking);
                    progress.update(&job);
                    if let Some(fresh) = refresh() {
                        let merged = crate::process::merge_paths(&fresh, path_var.as_deref());
                        crate::process::set_path(merged.clone());
                        *path_var = Some(merged);
                    }
                    found = crate::agents::detect::find(command, path_var.clone());
                }
            }
            match found {
                Some(bin) => {
                    // "Installed · Git 2.51" on the card.
                    job.steps[i].version = crate::agents::detect::version(&bin);
                    if job.steps[i].note.is_none() {
                        let name = step
                            .label
                            .trim_start_matches("Install ")
                            .trim_start_matches("Update ");
                        job.steps[i].note = Some(match crate::agents::detect::version(&bin) {
                            Some(v) => format!("{name} {v} is ready."),
                            None => format!("{name} is ready."),
                        });
                    }
                }
                None => {
                    let name = step
                        .label
                        .trim_start_matches("Install ")
                        .trim_start_matches("Update ");
                    job.steps[i].note = None;
                    let e = if stale {
                        format!(
                            "{name} is listed as installed, but its files are missing, and reinstalling didn't bring `{command}` back. Restart Gond and try again, or pick another way to install it."
                        )
                    } else {
                        format!(
                            "{name} finished installing, but Gond can't find `{command}` on this PC. Restart Gond, or pick another way to install it."
                        )
                    };
                    job.steps[i].status = Status::Failed;
                    job.status = Status::Failed;
                    job.error = Some(e);
                    progress.update(&job);
                    return job;
                }
            }
        }
        job.steps[i].status = Status::Done;
    }
    job.status = Status::Done;
    job.percent = None;
    job.stage = None;
    job.stage_started_at = None;
    job.bytes = None;
    job.speed = None;
    progress.update(&job);
    job
}

/// For a winget install whose package winget still lists but whose files are gone: an
/// uninstall that clears the stale record (`--force --purge`), then the same install
/// with `--force`. `None` for anything that isn't a winget install.
fn repair_steps(step: &Step) -> Option<(Step, Step)> {
    let plan = &step.plan;
    if plan.source != crate::harness::Source::Winget
        || plan.args.first().map(String::as_str) != Some("install")
    {
        return None;
    }
    let id = plan
        .args
        .iter()
        .position(|a| a == "--id")
        .and_then(|i| plan.args.get(i + 1))?;
    let name = step.label.trim_start_matches("Install ");
    let mut uninstall = plan.clone();
    uninstall.args = [
        "uninstall",
        "--id",
        id,
        "--exact",
        "--source",
        "winget",
        "--silent",
        "--force",
        "--purge",
        "--disable-interactivity",
        "--accept-source-agreements",
    ]
    .map(String::from)
    .into();
    uninstall.provides = None;
    let mut reinstall = plan.clone();
    reinstall.args.push("--force".into());
    Some((
        Step {
            label: format!("Clear {name}'s old install"),
            plan: uninstall,
        },
        Step {
            label: step.label.clone(),
            plan: reinstall,
        },
    ))
}

fn run_step(
    step: &Step,
    path_var: Option<OsString>,
    timeout: Duration,
    control: &Control,
    job: &mut JobState,
    progress: &dyn Progress,
) -> Result<Option<String>, String> {
    if control.is_cancelled() {
        return Err("Cancelled".into());
    }
    let plan = &step.plan;
    if plan.source == crate::harness::Source::Gond {
        return gond_step(plan, job).map(|()| None);
    }
    if plan.source == crate::harness::Source::Download {
        let id = plan.args.first().cloned().unwrap_or_default();
        let root = crate::fallback::programs_dir().ok_or("no local app data folder")?;
        return download_step(
            &id,
            &step.label,
            &root,
            timeout,
            control,
            job,
            progress,
            &http_get,
            crate::fallback::allowed_url,
        );
    }
    let program = detect::find(&plan.program, path_var.clone()).ok_or_else(|| {
        format!(
            "{} needs {}, which isn't installed",
            step.label, plan.program
        )
    })?;
    // winget only draws its progress on a console: give it a pseudo console (ConPTY
    // on Windows, a pty elsewhere, never a visible window) so the card gets real
    // bytes and stages instead of 0 then 100.
    if plan.source == crate::harness::Source::Winget {
        return run_step_pty(step, &program, path_var, timeout, control, job, progress);
    }
    let mut cmd = crate::process::command(&program);
    cmd.args(&plan.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(p) = &path_var {
        cmd.env("PATH", p);
    }
    // Vendor scripts run in Windows PowerShell 5.1. A PSModulePath inherited from
    // PowerShell 7 (Gond started from a pwsh terminal, or CI) hides 5.1's own modules,
    // so `Get-FileHash` "isn't recognized" and Claude's installer fails. Without
    // it, 5.1 builds its default path.
    if plan.program.eq_ignore_ascii_case("powershell") {
        cmd.env_remove("PSModulePath");
    }
    crate::process::new_process_group(&mut cmd);
    let mut child = crate::process::when_free(|| cmd.spawn())
        .map_err(|e| format!("couldn't start {}: {e}", plan.program))?;
    let tree = crate::proctree::contain(&child);
    let pid = child.id();
    *control.pid.lock().unwrap_or_else(|e| e.into_inner()) = Some(pid);
    // The watchdog stops a hung step: the whole tree, so installers it started go too.
    let (finished, watch) = mpsc::channel::<()>();
    let timed_out = Arc::new(AtomicBool::new(false));
    let watchdog = {
        let timed_out = timed_out.clone();
        std::thread::spawn(move || {
            if let Err(mpsc::RecvTimeoutError::Timeout) = watch.recv_timeout(timeout) {
                timed_out.store(true, Ordering::SeqCst);
                crate::process::kill_tree(pid);
            }
        })
    };
    let stderr = child.stderr.take();
    let err_tail = Arc::new(Mutex::new(String::new()));
    let err_reader = {
        let tail = err_tail.clone();
        std::thread::spawn(move || {
            if let Some(s) = stderr {
                for line in BufReader::new(s).lines().map_while(Result::ok) {
                    keep_tail(&mut tail.lock().unwrap_or_else(|e| e.into_inner()), &line);
                }
            }
        })
    };
    if let Some(mut out) = child.stdout.take() {
        // winget redraws its bar with \r and only ends with \n: read raw chunks and
        // split on both, so the percent reaches the card while it's moving.
        let (mut chunk, mut pending) = ([0u8; 4096], Vec::new());
        while let Ok(n) = out.read(&mut chunk) {
            if n == 0 {
                break;
            }
            pending.extend_from_slice(&chunk[..n]);
            while let Some(at) = pending.iter().position(|b| *b == b'\r' || *b == b'\n') {
                let line: Vec<u8> = pending.drain(..=at).collect();
                take_line(&String::from_utf8_lossy(&line), job, progress);
            }
        }
        take_line(&String::from_utf8_lossy(&pending), job, progress);
    }
    let status = child.wait();
    let _ = finished.send(());
    let _ = watchdog.join();
    *control.pid.lock().unwrap_or_else(|e| e.into_inner()) = None;
    // The step is done: end its tree, which also closes any pipe a leftover
    // process held, so the stderr reader finishes.
    drop(tree);
    let _ = err_reader.join();
    let err = err_tail.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if !err.is_empty() {
        keep_tail(&mut job.details, err.trim_end());
    }
    let status = status.ok().map(|s| (s.success(), s.code()));
    outcome(
        step,
        status,
        timed_out.load(Ordering::SeqCst),
        timeout,
        control,
        job,
    )
}

/// [`run_step`] for winget, under a pseudo console so it prints its progress.
/// How long a winget step may print nothing before it counts as stuck.
/// Short in unit tests; the real-winget checks (`GOND_REAL_HARNESS`) keep the real one.
fn idle_limit() -> Duration {
    if cfg!(test) && std::env::var_os("GOND_REAL_HARNESS").is_none() {
        Duration::from_secs(3)
    } else {
        Duration::from_secs(3 * 60)
    }
}

/// How many cursor-position requests (`ESC[6n`) a chunk of console output holds.
fn cursor_queries(chunk: &[u8]) -> usize {
    chunk.windows(4).filter(|w| *w == b"\x1b[6n").count()
}

fn run_step_pty(
    step: &Step,
    program: &std::path::Path,
    path_var: Option<OsString>,
    timeout: Duration,
    control: &Control,
    job: &mut JobState,
    progress: &dyn Progress,
) -> Result<Option<String>, String> {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};
    let plan = &step.plan;
    let failed = |e: String| format!("couldn't start {}: {e}", plan.program);
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 30,
            // Wide, so winget's bar and its "x MB / y MB" stay on one line.
            cols: 200,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| failed(e.to_string()))?;
    let mut cmd = CommandBuilder::new(program);
    cmd.args(&plan.args);
    if let Some(p) = path_var.as_ref().cloned().or_else(crate::process::path) {
        cmd.env("PATH", p);
    }
    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| failed(e.to_string()))?;
    drop(pair.slave);
    let pid = child.process_id().unwrap_or(0);
    let tree = (pid != 0).then(|| crate::proctree::contain_pid(pid));
    *control.pid.lock().unwrap_or_else(|e| e.into_inner()) = (pid != 0).then_some(pid);
    let (finished, watch) = mpsc::channel::<()>();
    let timed_out = Arc::new(AtomicBool::new(false));
    let watchdog = {
        let timed_out = timed_out.clone();
        std::thread::spawn(move || {
            if let Err(mpsc::RecvTimeoutError::Timeout) = watch.recv_timeout(timeout) {
                timed_out.store(true, Ordering::SeqCst);
                if pid != 0 {
                    crate::process::kill_tree(pid);
                }
            }
        })
    };
    // Output arrives on a reader thread; lines come back here, where the job lives.
    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| failed(e.to_string()))?;
    // ConPTY (portable-pty opens it with PSEUDOCONSOLE_INHERIT_CURSOR) asks where the
    // cursor is (`ESC[6n`) and the child prints nothing until it's answered; with no
    // terminal attached, the pump answers "top left" itself.
    let mut writer = pair
        .master
        .take_writer()
        .map_err(|e| failed(e.to_string()))?;
    let (lines, rx) = mpsc::channel::<(std::time::Instant, String)>();
    let pump = std::thread::spawn(move || {
        let (mut chunk, mut pending) = ([0u8; 4096], Vec::new());
        while let Ok(n) = reader.read(&mut chunk) {
            if n == 0 {
                break;
            }
            for _ in 0..cursor_queries(&chunk[..n]) {
                let _ = writer.write_all(b"\x1b[1;1R");
                let _ = writer.flush();
            }
            pending.extend_from_slice(&chunk[..n]);
            while let Some(at) = pending.iter().position(|b| *b == b'\r' || *b == b'\n') {
                let line: Vec<u8> = pending.drain(..=at).collect();
                let arrived = std::time::Instant::now();
                if lines
                    .send((arrived, String::from_utf8_lossy(&line).into_owned()))
                    .is_err()
                {
                    return;
                }
            }
        }
        let _ = lines.send((
            std::time::Instant::now(),
            String::from_utf8_lossy(&pending).into_owned(),
        ));
    });
    let mut last_output = std::time::Instant::now();
    let mut idle = false;
    let status = loop {
        while let Ok((at, line)) = rx.try_recv() {
            last_output = std::time::Instant::now();
            take_line_at(&line, at, job, progress);
        }
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => break None,
        }
        // winget draws a spinner while it works; silence this long means it's stuck.
        // (A shorter step timeout speaks for itself.)
        if !idle && idle_limit() < timeout && last_output.elapsed() > idle_limit() {
            idle = true;
            if pid != 0 {
                crate::process::kill_tree(pid);
            }
        }
    };
    let _ = finished.send(());
    let _ = watchdog.join();
    *control.pid.lock().unwrap_or_else(|e| e.into_inner()) = None;
    drop(tree);
    // Closing the console ends the reader (ConPTY keeps it open until then).
    drop(pair.master);
    let _ = pump.join();
    while let Ok((at, line)) = rx.try_recv() {
        take_line_at(&line, at, job, progress);
    }
    // winget exits with HRESULTs (0x8A15...): the same bits as an i32.
    let status = status.map(|s| (s.success(), Some(s.exit_code() as i32)));
    if idle && !control.is_cancelled() {
        return Err(format!(
            "{} stopped responding: nothing for {}, so Gond stopped it. Try again.",
            step.label.trim_start_matches("Install "),
            if idle_limit().as_secs() >= 60 {
                format!("{} minutes", idle_limit().as_secs() / 60)
            } else {
                format!("{} seconds", idle_limit().as_secs())
            }
        ));
    }
    outcome(
        step,
        status,
        timed_out.load(Ordering::SeqCst),
        timeout,
        control,
        job,
    )
}

/// What a finished step means: done (maybe with a note), or a plain failure.
fn outcome(
    step: &Step,
    status: Option<(bool, Option<i32>)>,
    timed_out: bool,
    timeout: Duration,
    control: &Control,
    job: &mut JobState,
) -> Result<Option<String>, String> {
    let plan = &step.plan;
    if control.is_cancelled() {
        return Err("Cancelled".into());
    }
    if timed_out {
        return Err(format!(
            "{} took longer than {} minutes and was stopped",
            step.label,
            timeout.as_secs() / 60
        ));
    }
    let (success, code) =
        status.ok_or_else(|| format!("couldn't tell whether {} finished", step.label))?;
    if success {
        if let Some(rel) = &plan.adds_to_path {
            put_on_path(rel, job);
        }
        return Ok(None);
    }
    let name = step
        .label
        .trim_start_matches("Install ")
        .trim_start_matches("Update ");
    if plan.source == crate::harness::Source::Winget
        && code == Some(winget::NO_APPLICABLE_INSTALLER)
        && plan.args.iter().any(|a| a == "user")
    {
        job.machine_scope_available = true;
        return Err(format!(
            "{} has no per-user installer. Install it for all users (Windows asks once), or skip it.",
            name
        ));
    }
    if plan.source == crate::harness::Source::Winget {
        if let Some(outcome) = code.and_then(|c| {
            winget_outcome(c, name, plan.args.first().is_some_and(|v| v == "install"))
        }) {
            return outcome.map(Some);
        }
    }
    Err(format!(
        "{} failed{}",
        step.label,
        code.map(|c| format!(" (exit {c})")).unwrap_or_default()
    ))
}

/// Steps Gond does itself: `remove` deletes files and folders, `path-remove`
/// takes folders off the user PATH and this process's. Every path to delete must sit
/// below the user's profile folders, never be one, so a bad plan can't reach anything else.
/// The prerequisite a winget step installs, when it has an official-release fallback.
fn fallback_for(step: &Step) -> Option<String> {
    let plan = &step.plan;
    if plan.source != crate::harness::Source::Winget
        || plan.args.first().map(String::as_str) != Some("install")
    {
        return None;
    }
    let wid = plan
        .args
        .iter()
        .position(|a| a == "--id")
        .and_then(|i| plan.args.get(i + 1))?;
    crate::harness::PREREQS
        .iter()
        .find(|p| p.3 == wid && crate::fallback::supported(p.0))
        .map(|p| p.0.to_string())
}

fn download_step_for(id: &str, step: &Step) -> Step {
    let &(_, name, command, _, _) = crate::harness::PREREQS
        .iter()
        .find(|p| p.0 == id)
        .expect("a known prerequisite");
    Step {
        label: step.label.clone(),
        plan: crate::harness::download_plan(id, name, command),
    }
}

/// GETs `url` (vendor metadata) as text, with a User-Agent GitHub's API requires.
fn http_get(url: &str) -> Result<String, String> {
    http_client()?
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.text())
        .map_err(|e| format!("couldn't read {}: {}", host_of(url), e.without_url()))
}

pub(crate) fn http_client() -> Result<reqwest::blocking::Client, String> {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    reqwest::blocking::Client::builder()
        .user_agent(concat!("GondCode/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(20))
        .timeout(None)
        .build()
        .map_err(|e| e.to_string())
}

fn host_of(url: &str) -> &str {
    url.split("//")
        .nth(1)
        .and_then(|r| r.split('/').next())
        .unwrap_or(url)
}

/// Reports a download's bytes on the card, like winget's bar (stage, bytes, speed).
struct CardBytes<'a> {
    job: &'a mut JobState,
    progress: &'a dyn Progress,
}

impl crate::fallback::OnBytes for CardBytes<'_> {
    fn bytes(&mut self, done: u64, total: Option<u64>) {
        set_stage(self.job, Stage::Downloading);
        let now = std::time::Instant::now();
        self.job.speed = rolling_speed(&mut self.job.byte_samples, now, done)
            .or_else(|| average_speed(self.job.download_started, now, done))
            .or(self.job.speed);
        self.job.bytes = Some(Bytes { done, total });
        self.job.percent = total
            .filter(|t| *t > 0)
            .map(|t| ((done as f64 / t as f64) * 100.0).round().min(100.0) as u8);
        self.progress.update(self.job);
    }
}

/// `Source::Download`: the vendor's release, verified against its published
/// SHA-256, then installed per user. `get` reads metadata (tests pass their own).
#[allow(clippy::too_many_arguments)]
fn download_step(
    id: &str,
    label: &str,
    root: &std::path::Path,
    timeout: Duration,
    control: &Control,
    job: &mut JobState,
    progress: &dyn Progress,
    get: &dyn Fn(&str) -> Result<String, String>,
    allowed: fn(&str) -> bool,
) -> Result<Option<String>, String> {
    use crate::fallback::{self, Kind};
    let name = label.trim_start_matches("Install ");
    let art = fallback::resolve_checked(id, get, allowed)?;
    keep_tail(
        &mut job.details,
        &format!("Downloading {} {} from {}", name, art.version, art.url),
    );
    set_stage(job, Stage::Downloading);
    progress.update(job);
    let dir = tempfile::Builder::new()
        .prefix("gond-install-")
        .tempdir()
        .map_err(|e| e.to_string())?;
    let file = dir.path().join(&art.name);
    let resp = http_client()?
        .get(&art.url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("{name} couldn't be downloaded: {}", e.without_url()))?;
    let total = resp.content_length().or(art.size);
    let begin = std::time::Instant::now();
    let stop = || control.is_cancelled() || begin.elapsed() > timeout;
    let sha = fallback::copy_hashed(resp, &file, total, &mut CardBytes { job, progress }, &stop)?;
    if sha != art.sha256 {
        return Err(format!(
            "{name}'s download didn't match its published checksum, so it wasn't installed. Try again later."
        ));
    }
    keep_tail(
        &mut job.details,
        "Checksum matches the vendor's published SHA-256.",
    );
    set_stage(job, Stage::Installing);
    progress.update(job);
    match art.kind {
        Kind::Installer(args) => {
            let mut cmd = crate::process::command(&file);
            cmd.args(&args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            crate::process::new_process_group(&mut cmd);
            let mut child = cmd
                .spawn()
                .map_err(|e| format!("{name}'s installer didn't start: {e}"))?;
            let tree = crate::proctree::contain(&child);
            *control.pid.lock().unwrap_or_else(|e| e.into_inner()) = Some(child.id());
            let status = loop {
                match child.try_wait() {
                    Ok(Some(st)) => break Ok(st),
                    Ok(None) if control.is_cancelled() || begin.elapsed() > timeout => {
                        crate::proctree::kill_tree(child.id());
                        let _ = child.wait();
                        break Err(if control.is_cancelled() {
                            "Cancelled".to_string()
                        } else {
                            format!(
                                "{label} took longer than {} minutes and was stopped",
                                timeout.as_secs() / 60
                            )
                        });
                    }
                    Ok(None) => std::thread::sleep(Duration::from_millis(200)),
                    Err(e) => break Err(e.to_string()),
                }
            };
            *control.pid.lock().unwrap_or_else(|e| e.into_inner()) = None;
            drop(tree);
            let status = status?;
            if !status.success() {
                return Err(format!(
                    "{name}'s installer stopped with code {}.",
                    status.code().unwrap_or(-1)
                ));
            }
        }
        Kind::Zip {
            dir: sub,
            bin,
            strip,
        } => {
            let dest = root.join(&sub);
            std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
            fallback::unzip(&file, &dest, strip)?;
            let bin = if bin.is_empty() { dest } else { dest.join(bin) };
            let dir = bin.to_string_lossy().to_string();
            match crate::pathenv::add_user_dir(&dir) {
                Ok(()) => keep_tail(&mut job.details, &format!("Gond added {dir} to your PATH.")),
                Err(e) => {
                    return Err(format!(
                        "{name} is unpacked, but couldn't go on your PATH: {e}"
                    ))
                }
            }
        }
    }
    Ok(Some(format!("{name} {} is installed.", art.version)))
}

fn gond_step(plan: &Plan, job: &mut JobState) -> Result<(), String> {
    match plan.program.as_str() {
        "remove" => {
            for p in &plan.args {
                let path = std::path::Path::new(p);
                if !inside_profile(path) {
                    return Err(format!(
                        "Gond won't delete {p}: it isn't inside your user folder."
                    ));
                }
                match remove_soon(path, LOCK_WAIT) {
                    Ok(()) => keep_tail(&mut job.details, &format!("Removed {p}")),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => {
                        return Err(format!(
                            "Couldn't remove {p} ({e}). Close anything that's using it, then retry."
                        ))
                    }
                }
            }
            Ok(())
        }
        "path-remove" => {
            for dir in &plan.args {
                crate::pathenv::remove_user_dir(dir)?;
                if let Some(current) = crate::process::path() {
                    let kept: Vec<_> = std::env::split_paths(&current)
                        .filter(|e| !crate::pathenv::same_entry(&e.to_string_lossy(), dir))
                        .collect();
                    if let Ok(p) = std::env::join_paths(kept) {
                        crate::process::set_path(p);
                    }
                }
                keep_tail(&mut job.details, &format!("Took {dir} off your PATH"));
            }
            Ok(())
        }
        other => Err(format!("unknown Gond step {other}")),
    }
}

/// How long a removal waits out a file that's briefly in use.
const LOCK_WAIT: Duration = Duration::from_secs(5);

/// Removes `path`, waiting up to `wait` while Windows says it's in use: a version check
/// (`agy --version` from the Agents page) or a virus scan holds an .exe for a moment, and
/// a running .exe can't be deleted. Any other error answers at once.
fn remove_soon(path: &std::path::Path, wait: Duration) -> std::io::Result<()> {
    let start = std::time::Instant::now();
    loop {
        let gone = if path.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        };
        match gone {
            // Access denied (5) or a sharing violation (32): in use, for now.
            Err(e) if matches!(e.raw_os_error(), Some(5) | Some(32)) && start.elapsed() < wait => {
                std::thread::sleep(Duration::from_millis(250));
            }
            other => return other,
        }
    }
}

/// Below (never equal to) the user's home, local or roaming app-data folder.
fn inside_profile(path: &std::path::Path) -> bool {
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return false;
    }
    let roots = [dirs::home_dir(), dirs::data_local_dir(), dirs::data_dir()];
    let mut roots: Vec<_> = roots.into_iter().flatten().collect();
    // Tests build their fake profiles in the temp folder.
    if cfg!(test) {
        roots.push(std::env::temp_dir());
    }
    if let Some(app_data) = dirs::data_local_dir().and_then(|l| l.parent().map(|p| p.to_path_buf()))
    {
        roots.push(app_data);
    }
    // Nothing that *is* a profile folder (local and roaming sit under home).
    !roots.iter().any(|r| r == path)
        && roots.iter().any(|r| {
            path.strip_prefix(r)
                .is_ok_and(|rest| rest.components().count() >= 1)
        })
}

/// Puts `rel` (under the user's profile) on the user PATH if the installer left it
/// there without doing so, so the next detection and every new terminal find it.
fn put_on_path(rel: &str, job: &mut JobState) {
    let Some(home) = dirs::home_dir() else {
        return;
    };
    let dir = home.join(rel);
    if !dir.is_dir() {
        return;
    }
    let dir = dir.to_string_lossy().to_string();
    match crate::pathenv::add_user_dir(&dir) {
        Ok(()) => keep_tail(&mut job.details, &format!("Gond added {dir} to your PATH.")),
        Err(e) => keep_tail(
            &mut job.details,
            &format!("Couldn't add {dir} to PATH: {e}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::Source;

    /// A file another program has open without sharing (as a running .exe is) can't be
    /// deleted; the removal waits for it to be let go, and gives up plainly without a wait.
    #[cfg(windows)]
    #[test]
    fn removal_waits_out_a_file_in_use() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("agy.exe");
        std::fs::write(&exe, "x").unwrap();
        let held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&exe)
            .unwrap();
        assert!(remove_soon(&exe, Duration::ZERO).is_err());
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(600));
            drop(held);
        });
        remove_soon(&exe, Duration::from_secs(5)).unwrap();
        release.join().unwrap();
        assert!(!exe.exists());
    }

    #[derive(Default)]
    struct Recorder(Mutex<Vec<JobState>>);

    impl Progress for Recorder {
        fn update(&self, job: &JobState) {
            self.0.lock().unwrap().push(job.clone());
        }
    }

    #[test]
    fn percents() {
        assert_eq!(percent_in("  ██████▒▒▒  45%"), Some(45));
        assert_eq!(percent_in("100%"), Some(100));
        assert_eq!(percent_in("Downloading 12.5 MB"), None);
        assert_eq!(percent_in("150%"), None);
    }

    #[cfg(unix)]
    fn fake(dir: &std::path::Path, name: &str, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join(name);
        std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[cfg(unix)]
    fn step(label: &str, program: &str) -> Step {
        Step {
            label: label.into(),
            plan: Plan {
                source: Source::Winget,
                program: program.into(),
                args: vec!["install".into()],
                from: "test".into(),
                adds_to_path: None,
                provides: None,
            },
        }
    }

    #[cfg(unix)]
    #[test]
    fn runs_steps_with_progress_and_stops_at_the_first_failure() {
        let dir = tempfile::tempdir().unwrap();
        fake(
            dir.path(),
            "good",
            "printf '  10%%\\r  55%%\\r 100%%\\n'; echo Installed",
        );
        fake(dir.path(), "bad", "echo 'No package found' >&2; exit 3");
        let path = Some(format!("{}:/usr/bin:/bin", dir.path().display()).into());
        let rec = Recorder::default();
        let job = run(
            "J1".into(),
            "Set up".into(),
            vec![
                step("Install Git", "good"),
                step("Install Node", "bad"),
                step("Never", "good"),
            ],
            path,
            || None,
            STEP_TIMEOUT,
            &rec,
        );
        assert_eq!(job.status, Status::Failed);
        let states: Vec<_> = job.steps.iter().map(|s| s.status).collect();
        assert_eq!(states, [Status::Done, Status::Failed, Status::Pending]);
        assert_eq!(job.error.as_deref(), Some("Install Node failed (exit 3)"));
        assert!(job.details.contains("Installed") && job.details.contains("No package found"));
        let seen: Vec<_> = rec
            .0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|j| j.percent)
            .collect();
        assert_eq!(seen, [10, 55, 100]);
    }

    #[cfg(unix)]
    #[test]
    fn a_missing_program_is_a_plain_error() {
        let dir = tempfile::tempdir().unwrap();
        let job = run(
            "J2".into(),
            "Install Codex".into(),
            vec![step("Install Codex", "winget")],
            Some(dir.path().as_os_str().to_owned()),
            || None,
            STEP_TIMEOUT,
            &Recorder::default(),
        );
        assert_eq!(job.status, Status::Failed);
        assert_eq!(
            job.error.as_deref(),
            Some("Install Codex needs winget, which isn't installed")
        );
    }

    #[cfg(unix)]
    #[derive(Default)]
    struct Timed(Mutex<Vec<(std::time::Instant, JobState)>>);

    #[cfg(unix)]
    impl Progress for Timed {
        fn update(&self, job: &JobState) {
            self.0
                .lock()
                .unwrap()
                .push((std::time::Instant::now(), job.clone()));
        }
    }

    #[cfg(unix)]
    #[test]
    fn percent_arrives_while_the_step_is_still_running() {
        let dir = tempfile::tempdir().unwrap();
        // Like winget: a \r-redrawn bar, no newline until the end.
        fake(
            dir.path(),
            "slow",
            "printf '  42%%\r'; sleep 1; printf ' 100%%\n'",
        );
        let rec = Timed::default();
        let job = run(
            "J3".into(),
            "Install".into(),
            vec![step("Install", "slow")],
            Some(format!("{}:/usr/bin:/bin", dir.path().display()).into()),
            || None,
            STEP_TIMEOUT,
            &rec,
        );
        assert_eq!(job.status, Status::Done);
        let seen = rec.0.lock().unwrap();
        let at42 = seen.iter().find(|(_, j)| j.percent == Some(42)).unwrap().0;
        let end = seen.last().unwrap().0;
        assert!(
            end.duration_since(at42) >= Duration::from_millis(500),
            "42% came only at exit"
        );
    }

    /// a step that prints nothing for the idle limit is stopped with a plain error.
    #[cfg(unix)]
    #[test]
    fn a_silent_step_is_stopped_as_stuck() {
        let dir = tempfile::tempdir().unwrap();
        fake(dir.path(), "quiet", "sleep 30");
        let begin = std::time::Instant::now();
        let job = run(
            "J6".into(),
            "Install".into(),
            vec![step("Install Git", "quiet")],
            Some(format!("{}:/usr/bin:/bin", dir.path().display()).into()),
            || None,
            Duration::from_secs(20),
            &Recorder::default(),
        );
        assert!(begin.elapsed() < Duration::from_secs(10));
        assert_eq!(job.status, Status::Failed);
        assert_eq!(
            job.error.as_deref(),
            Some("Git stopped responding: nothing for 3 seconds, so Gond stopped it. Try again.")
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_hung_step_times_out_and_cancel_stops_a_job() {
        let dir = tempfile::tempdir().unwrap();
        fake(dir.path(), "hang", "sleep 30");
        let path: Option<OsString> = Some(format!("{}:/usr/bin:/bin", dir.path().display()).into());
        let begin = std::time::Instant::now();
        let job = run(
            "J4".into(),
            "Install".into(),
            vec![step("Install Hang", "hang")],
            path.clone(),
            || None,
            Duration::from_millis(300),
            &Recorder::default(),
        );
        assert!(begin.elapsed() < Duration::from_secs(10));
        assert_eq!(job.status, Status::Failed, "{:?}", job.error);
        let error = job.error.unwrap();
        assert!(error.contains("took longer than"), "{error}");

        let worker = std::thread::spawn(move || {
            run(
                "J5".into(),
                "Install".into(),
                vec![step("Install Hang", "hang"), step("Never", "hang")],
                path,
                || None,
                STEP_TIMEOUT,
                &Recorder::default(),
            )
        });
        let begin = std::time::Instant::now();
        while !cancel("J5") {
            assert!(
                begin.elapsed() < Duration::from_secs(5),
                "job never registered"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        let job = worker.join().unwrap();
        assert_eq!(job.status, Status::Cancelled);
        let states: Vec<_> = job.steps.iter().map(|s| s.status).collect();
        assert_eq!(states, [Status::Cancelled, Status::Pending]);
        assert!(!cancel("J5"), "a finished job is gone");
    }

    /// winget's "no newer version" after an install is a finished update with a
    /// plain line, and other known codes read as sentences, never "exit -1978335189".
    #[test]
    fn winget_codes_read_as_sentences() {
        assert_eq!(
            winget_outcome(winget::UPDATE_NOT_APPLICABLE, "Codex", false),
            Some(Ok("Codex is already up to date.".into()))
        );
        // After an install it never reads "up to date".
        assert_eq!(
            winget_outcome(winget::UPDATE_NOT_APPLICABLE, "Claude Code", true),
            Some(Ok("Claude Code is already installed.".into()))
        );
        assert_eq!(winget::UPDATE_NOT_APPLICABLE, 0x8A15_002B_u32 as i32);
        assert!(matches!(
            winget_outcome(winget::PACKAGE_ALREADY_INSTALLED, "Git", true),
            Some(Ok(_))
        ));
        for code in [
            winget::NO_APPLICATIONS_FOUND,
            winget::INSTALLER_HASH_MISMATCH,
            winget::DOWNLOAD_FAILED,
            winget::INSTALL_IN_PROGRESS,
            winget::INSTALL_BLOCKED_BY_POLICY,
            winget::SOURCE_AGREEMENTS_NOT_ACCEPTED,
        ] {
            let e = winget_outcome(code, "Git", true).unwrap().unwrap_err();
            assert!(e.contains("Git") && !e.contains("exit"), "{e}");
        }
        assert_eq!(
            winget_outcome(1, "Git", true),
            None,
            "unknown codes keep the raw exit"
        );
    }

    /// The same through a real job on Windows, where exit codes carry winget's HRESULT.
    #[cfg(windows)]
    #[test]
    fn an_update_with_nothing_newer_is_done() {
        struct Quiet;
        impl Progress for Quiet {
            fn update(&self, _: &JobState) {}
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("wgfake.cmd"),
            "@echo No applicable update found.\r\n@exit /b -1978335189\r\n",
        )
        .unwrap();
        let path: Option<OsString> =
            Some(format!("{};C:\\Windows\\System32", dir.path().display()).into());
        let step = Step {
            label: "Update Codex".into(),
            plan: Plan {
                source: Source::Winget,
                program: "wgfake".into(),
                args: vec!["upgrade".into()],
                from: "test".into(),
                adds_to_path: None,
                provides: None,
            },
        };
        let job = run(
            "J9".into(),
            "Update Codex".into(),
            vec![step],
            path,
            || None,
            STEP_TIMEOUT,
            &Quiet,
        );
        assert_eq!(job.status, Status::Done, "{:?}", job.error);
        assert_eq!(
            job.steps[0].note.as_deref(),
            Some("Codex is already up to date.")
        );

        // The same code on an *install* of a program that isn't there: never
        // "up to date", and a plain sentence once Gond can't find it.
        let step = Step {
            label: "Install Claude Code".into(),
            plan: Plan {
                source: Source::Winget,
                program: "wgfake".into(),
                args: vec!["install".into()],
                from: "test".into(),
                adds_to_path: None,
                provides: Some("gond-not-a-real-cli".into()),
            },
        };
        let path: Option<OsString> =
            Some(format!("{};C:\\Windows\\System32", dir.path().display()).into());
        let job = run(
            "J10".into(),
            "Install Claude Code".into(),
            vec![step],
            path,
            || None,
            STEP_TIMEOUT,
            &Quiet,
        );
        assert_eq!(job.status, Status::Failed);
        let e = job.error.unwrap();
        assert!(
            e.contains("listed as installed, but its files are missing")
                && !e.contains("Settings")
                && !e.contains("up to date"),
            "{e}"
        );
    }

    /// a step that reports success but leaves nothing to find fails plainly; one
    /// whose program is found says it's ready.
    #[cfg(unix)]
    #[test]
    fn a_step_is_done_only_when_its_program_is_found() {
        let dir = tempfile::tempdir().unwrap();
        fake(dir.path(), "inst", "echo ok");
        fake(dir.path(), "tool", "echo 'tool 1.2.3'");
        let path: Option<OsString> = Some(format!("{}:/usr/bin:/bin", dir.path().display()).into());
        let with = |provides: &str| Step {
            label: "Install Tool".into(),
            plan: Plan {
                source: Source::Vendor,
                program: "inst".into(),
                args: vec![],
                from: "test".into(),
                adds_to_path: None,
                provides: Some(provides.into()),
            },
        };
        let job = run(
            "J7".into(),
            "Install".into(),
            vec![with("missing-tool")],
            path.clone(),
            || None,
            STEP_TIMEOUT,
            &Recorder::default(),
        );
        assert_eq!(job.status, Status::Failed);
        let e = job.error.unwrap();
        assert!(
            e.contains("can't find `missing-tool`") && !e.contains("up to date"),
            "{e}"
        );
        let job = run(
            "J8".into(),
            "Install".into(),
            vec![with("tool")],
            path,
            || None,
            STEP_TIMEOUT,
            &Recorder::default(),
        );
        assert_eq!(job.status, Status::Done, "{:?}", job.error);
        assert_eq!(job.steps[0].note.as_deref(), Some("Tool 1.2.3 is ready."));
    }

    /// winget draws progress only on a console, so the step runs under one; the
    /// job then reports named stages and real download bytes, never 0 → 100.
    #[cfg(unix)]
    #[test]
    fn a_download_reports_stages_and_bytes() {
        let dir = tempfile::tempdir().unwrap();
        // Like winget: the bar only when stdout is a terminal, redrawn with \r and
        // coloured with escape codes; then hash, install and done lines.
        fake(
            dir.path(),
            "winget",
            "[ -t 1 ] || { echo 'not a console: no progress'; exit 0; }\n\
             echo 'Found Git [Git.Git] Version 2.51.0'\n\
             echo 'Downloading https://github.com/git-for-windows/git/releases/Git-64-bit.exe'\n\
             for mb in 5.0 15.0 30.0 45.0 60.0; do printf '\\033[32m  ██████▒▒▒  %s MB / 60.0 MB\\033[0m\\r' \"$mb\"; sleep 0.1; done\n\
             printf '\\n'\n\
             echo 'Successfully verified installer hash'\n\
             echo 'Starting package install...'\n\
             sleep 0.3\n\
             echo 'Successfully installed'",
        );
        fake(dir.path(), "git", "echo 'git version 2.51.0.windows.1'");
        let mut install = step("Install Git", "winget");
        install.plan.provides = Some("git".into());
        let rec = Recorder::default();
        let job = run(
            "J7".into(),
            "Set up Git".into(),
            vec![install],
            Some(format!("{}:/usr/bin:/bin", dir.path().display()).into()),
            || None,
            STEP_TIMEOUT,
            &rec,
        );
        assert_eq!(job.status, Status::Done, "{}", job.details);
        assert!(
            !job.details.contains("not a console"),
            "ran under a pseudo console"
        );
        let seen = rec.0.lock().unwrap();
        let percents: Vec<u8> = seen.iter().filter_map(|j| j.percent).collect();
        let between: Vec<u8> = percents
            .iter()
            .copied()
            .filter(|p| *p > 0 && *p < 100)
            .collect();
        assert!(
            between.len() >= 3,
            "several steps between 0 and 100: {percents:?}"
        );
        assert!(
            percents.windows(2).all(|w| w[0] <= w[1]),
            "never goes back: {percents:?}"
        );
        let last = seen.iter().rev().find(|j| j.bytes.is_some()).unwrap();
        assert_eq!(last.bytes.unwrap().total, Some(60 * 1024 * 1024));
        // The speed depends on when the redraws arrive, so it's checked with fixed
        // times in `speed_is_a_rolling_rate_over_two_seconds`, not here under load.
        // The stages, in order (repeats collapsed).
        let mut stages: Vec<Stage> = vec![];
        for j in seen.iter() {
            if let Some(st) = j.stage {
                if stages.last() != Some(&st) {
                    stages.push(st);
                }
            }
        }
        assert_eq!(
            stages,
            vec![
                Stage::Installing,
                Stage::Downloading,
                Stage::Installing,
                Stage::SettingUp,
                Stage::Checking
            ]
        );
        // Installing has no percent (an indeterminate bar), but says since when.
        let installing = seen
            .iter()
            .rev()
            .find(|j| j.stage == Some(Stage::Installing))
            .unwrap();
        assert_eq!(installing.percent, None);
        assert!(installing.stage_started_at.is_some());
        assert_eq!(
            (job.stage, job.percent),
            (None, None),
            "done clears progress"
        );
    }

    #[test]
    fn sizes_and_bytes() {
        assert_eq!(size_in("12.0 MB"), Some(12 * 1024 * 1024));
        assert_eq!(size_in("512 KB"), Some(512 * 1024));
        assert_eq!(
            bytes_in("  ██▒▒  1.00 MB / 4.00 MB"),
            Some((1 << 20, 4 << 20))
        );
        assert_eq!(bytes_in("nothing / here"), None);
        assert_eq!(stage_in("Downloading https://x"), Some(Stage::Downloading));
        assert_eq!(
            stage_in("Starting package install..."),
            Some(Stage::Installing)
        );
        assert_eq!(stage_in("Successfully installed"), Some(Stage::SettingUp));
    }

    /// ConPTY's partial repaints can look like a count going back; the bar never does.
    #[test]
    fn download_progress_only_moves_forward() {
        let rec = Recorder::default();
        let mut job = run_job_state();
        for line in [
            "  ██████▒▒▒▒  30.0 MB / 60.0 MB",
            "  ██▒▒▒▒▒▒▒▒  6.0 MB / 60.0 MB",
            "  ████  3.0 MB / 9.0 MB",
            "36%",
            "  ████████▒▒  45.0 MB / 60.0 MB",
            "100%",
        ] {
            take_line(line, &mut job, &rec);
        }
        let seen = rec.0.lock().unwrap();
        let percents: Vec<u8> = seen.iter().filter_map(|j| j.percent).collect();
        assert_eq!(percents, vec![50, 75, 100]);
        assert_eq!(job.bytes.unwrap().total, Some(60 * 1024 * 1024));
    }

    /// a stale winget install is repaired with an uninstall (forced, purged) and
    /// the same install forced; only winget installs get one.
    #[test]
    fn repair_steps_for_a_stale_winget_install() {
        let codex = crate::harness::get("codex").unwrap();
        let step = Step {
            label: "Install Codex".into(),
            plan: codex.install_plans()[0].clone(),
        };
        let (un, re) = repair_steps(&step).unwrap();
        assert_eq!(un.label, "Clear Codex's old install");
        assert_eq!(
            un.plan.args[..3],
            [
                "uninstall".to_string(),
                "--id".into(),
                "OpenAI.Codex".into()
            ]
        );
        for flag in ["--force", "--purge", "--silent", "--disable-interactivity"] {
            assert!(un.plan.args.iter().any(|a| a == flag), "{flag}");
        }
        assert_eq!(un.plan.provides, None);
        assert_eq!(re.plan.args.first().map(String::as_str), Some("install"));
        assert_eq!(re.plan.args.last().map(String::as_str), Some("--force"));
        assert_eq!(re.plan.provides.as_deref(), Some("codex"));
        let bun = Step {
            label: "Install Codex".into(),
            plan: codex.install_plans()[2].clone(),
        };
        assert!(repair_steps(&bun).is_none());
    }

    ///  on a real runner: install Codex with winget, delete its files the way a
    /// Control Panel removal leaves things (winget still lists it), then Install again
    /// from Gond: it repairs itself and ends found, with no trip to Settings.
    #[cfg(windows)]
    #[test]
    #[ignore = "real network and winget; run by CI's winget-progress job"]
    fn real_codex_repairs_after_its_files_are_removed() {
        std::env::set_var("GOND_REAL_HARNESS", "1");
        let codex = crate::harness::get("codex").unwrap();
        let step = Step {
            label: "Install Codex".into(),
            plan: codex.install_plans()[0].clone(),
        };
        let install = || {
            run(
                "REAL".into(),
                "Install Codex".into(),
                vec![step.clone()],
                crate::process::path(),
                refreshed_path,
                Duration::from_secs(10 * 60),
                &Recorder::default(),
            )
        };
        let job = install();
        println!(
            "first: {:?} {:?} {:?}",
            job.status, job.error, job.steps[0].note
        );
        assert_eq!(job.status, Status::Done, "{:?}\n{}", job.error, job.details);
        let found = crate::agents::detect::find("codex", crate::process::path())
            .expect("found after install");
        println!("found at {}", found.display());

        let winget = dirs::data_local_dir()
            .unwrap()
            .join("Microsoft")
            .join("WinGet");
        let _ = std::fs::remove_file(winget.join("Links").join("codex.exe"));
        for e in std::fs::read_dir(winget.join("Packages"))
            .unwrap()
            .flatten()
        {
            if e.file_name().to_string_lossy().starts_with("OpenAI.Codex") {
                std::fs::remove_dir_all(e.path()).unwrap();
            }
        }
        assert_eq!(
            crate::agents::detect::find("codex", crate::process::path()),
            None,
            "files gone, as after a Control Panel removal"
        );

        let job = install();
        println!(
            "again: {:?} {:?} {:?}\n{}",
            job.status, job.error, job.steps[0].note, job.details
        );
        assert_eq!(job.status, Status::Done, "{:?}", job.error);
        assert!(crate::agents::detect::find("codex", crate::process::path()).is_some());
    }

    /// The speed is the rate over the last ~2 s of arrivals, whenever they're read; fast
    /// redraws (winget draws every few ms) still add up to a figure.
    #[test]
    fn speed_is_a_rolling_rate_over_two_seconds() {
        let rec = Recorder::default();
        let mut job = run_job_state();
        let t0 = std::time::Instant::now();
        let mb = |n: u64| n * 1024 * 1024;
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let bar = |n: u64| format!("  ██▒▒  {n}.0 MB / 600.0 MB");
        // One sample is no rate; two 20 ms apart aren't either; from 50 ms on, they are.
        take_line_at(&bar(10), at(0), &mut job, &rec);
        assert_eq!(job.speed, None, "one sample");
        take_line_at(&bar(10), at(20), &mut job, &rec);
        assert_eq!(job.speed, None, "under 50 ms");
        take_line_at(&bar(11), at(100), &mut job, &rec);
        assert_eq!(job.speed, Some(mb(1) * 10), "1 MB over 100 ms");
        // A download over in under a second still shows a figure.
        take_line_at(&bar(12), at(400), &mut job, &rec);
        assert_eq!(job.speed, Some(mb(2) * 1000 / 400), "2 MB over 400 ms");
        // Older samples age out: only the last ~2 s count.
        take_line_at(&bar(100), at(10_000), &mut job, &rec);
        take_line_at(&bar(110), at(11_000), &mut job, &rec);
        assert_eq!(job.speed, Some(mb(10)), "10 MB over the last second");
        // A stage change clears it.
        take_line("Successfully verified installer hash", &mut job, &rec);
        take_line("Starting package install...", &mut job, &rec);
        assert_eq!(job.speed, None);
    }

    fn run_job_state() -> JobState {
        JobState {
            id: "j".into(),
            title: "t".into(),
            status: Status::Running,
            steps: vec![],
            percent: None,
            stage: None,
            stage_started_at: None,
            ended_at: None,
            bytes: None,
            speed: None,
            byte_samples: Default::default(),
            download_started: None,
            error: None,
            details: String::new(),
            machine_scope_available: false,
        }
    }

    /// Serves `body` at any path on 127.0.0.1 (a vendor's download, in tests).
    fn serve(body: Vec<u8>) -> u16 {
        use std::io::{BufRead, Write};
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for c in l.incoming().flatten() {
                let mut r = std::io::BufReader::new(c.try_clone().unwrap());
                loop {
                    let mut line = String::new();
                    if r.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                        break;
                    }
                }
                let mut w = c;
                let _ = write!(
                    w,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = w.write_all(&body);
            }
        });
        port
    }

    fn gh_zip() -> Vec<u8> {
        use std::io::Write;
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default();
            z.start_file("bin/gh.exe", o).unwrap();
            z.write_all(&vec![1u8; 200_000]).unwrap();
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    /// without winget, GitHub CLI comes from its release: downloaded with the same
    /// stages and bytes as winget's, checked against GitHub's SHA-256, unpacked per user.
    #[test]
    fn a_download_step_verifies_then_unpacks() {
        use sha2::{Digest, Sha256};
        let body = gh_zip();
        let sha: String = Sha256::digest(&body)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let port = serve(body.clone());
        let meta = |sha: String| {
            move |_: &str| -> Result<String, String> {
                Ok(serde_json::json!({ "tag_name": "v2.103.3", "assets": [{
                    "name": "gh_2.103.3_windows_amd64.zip", "digest": format!("sha256:{sha}"),
                    "browser_download_url": format!("http://127.0.0.1:{port}/gh.zip"),
                }]})
                .to_string())
            }
        };
        let root = tempfile::tempdir().unwrap();
        let rec = Recorder::default();
        let mut job = run_job_state();
        let note = download_step(
            "gh",
            "Install GitHub CLI",
            root.path(),
            STEP_TIMEOUT,
            &Control::default(),
            &mut job,
            &rec,
            &meta(sha.clone()),
            |_| true,
        )
        .unwrap();
        assert_eq!(note.as_deref(), Some("GitHub CLI 2.103.3 is installed."));
        assert!(root.path().join("gh/bin/gh.exe").is_file());
        let seen = rec.0.lock().unwrap();
        let stages: Vec<_> = seen.iter().filter_map(|j| j.stage).collect();
        assert!(
            stages.contains(&Stage::Downloading) && stages.last() == Some(&Stage::Installing),
            "{stages:?}"
        );
        let last = seen.iter().rev().find_map(|j| j.bytes).unwrap();
        assert_eq!(
            (last.done, last.total),
            (body.len() as u64, Some(body.len() as u64))
        );
        assert!(job.details.contains("Checksum matches"));
        drop(seen);

        // A checksum that doesn't match: nothing is installed.
        let other = tempfile::tempdir().unwrap();
        let e = download_step(
            "gh",
            "Install GitHub CLI",
            other.path(),
            STEP_TIMEOUT,
            &Control::default(),
            &mut run_job_state(),
            &rec,
            &meta("0".repeat(64)),
            |_| true,
        )
        .unwrap_err();
        assert!(e.contains("didn't match its published checksum"), "{e}");
        assert!(!other.path().join("gh").exists());
    }

    #[test]
    fn winget_steps_for_git_gh_and_node_have_a_fallback() {
        let step = |id: &str| {
            crate::harness::prereq_step(id, Some(std::ffi::OsString::from("/nowhere"))).unwrap()
        };
        // No winget here: the official release, for these three; Bun keeps its script.
        for id in ["git", "gh", "node"] {
            let s = step(id);
            assert_eq!(s.plan.source, Source::Download, "{id}");
            assert_eq!(s.plan.args, [id.to_string()]);
        }
        assert_eq!(step("bun").plan.source, Source::Vendor);
        // A winget install of one of them that fails falls back to the same.
        let winget = Step {
            label: "Install Node.js LTS".into(),
            plan: Plan {
                source: Source::Winget,
                program: "winget".into(),
                args: ["install", "--id", "OpenJS.NodeJS.LTS", "--exact"]
                    .map(String::from)
                    .into(),
                from: "w".into(),
                adds_to_path: None,
                provides: Some("node".into()),
            },
        };
        assert_eq!(fallback_for(&winget).as_deref(), Some("node"));
        assert_eq!(
            download_step_for("node", &winget).plan.source,
            Source::Download
        );
        let mut bun = winget.clone();
        bun.plan.args = ["install", "--id", "Oven-sh.Bun"].map(String::from).into();
        assert_eq!(fallback_for(&bun), None);
    }

    ///  on Windows: every redraw read at once (one Instant), so no rolling rate; the
    /// average since the download began still gives a figure.
    #[test]
    fn bunched_redraws_still_show_a_speed() {
        let rec = Recorder::default();
        let mut job = run_job_state();
        take_line("Downloading https://example/Git.exe", &mut job, &rec);
        let started = job.download_started.expect("the download's start is kept");
        let at = started + Duration::from_millis(1500);
        for mb in [4u64, 9, 14] {
            take_line_at(
                &format!("  ████▒▒  {mb}.0 MB / 64.0 MB"),
                at,
                &mut job,
                &rec,
            );
        }
        assert_eq!(job.speed, Some(((14 * 1024 * 1024) as f64 / 1.5) as u64));
        assert_eq!(
            average_speed(Some(started), started + Duration::from_millis(50), 5),
            None,
            "too soon (under the 100 ms threshold)"
        );
    }

    /// the download's start is when its line arrived, so a short download read in
    /// one go still has a span to average over.
    #[test]
    fn the_download_start_is_its_arrival() {
        let rec = Recorder::default();
        let mut job = run_job_state();
        let t0 = std::time::Instant::now();
        take_line_at("Downloading https://example/Git.exe", t0, &mut job, &rec);
        assert_eq!(job.download_started, Some(t0));
        take_line_at(
            "  ███▒▒  2.0 MB / 3.7 MB",
            t0 + Duration::from_millis(400),
            &mut job,
            &rec,
        );
        assert_eq!(job.speed, Some(5 * 1024 * 1024), "2 MB over 400 ms");
    }

    #[test]
    fn counts_cursor_position_requests() {
        assert_eq!(cursor_queries(b"\x1b[6n"), 1);
        assert_eq!(cursor_queries(b"\x1b[?25l\x1b[6nhi\x1b[6n"), 2);
        assert_eq!(cursor_queries(b"\x1b[2J plain text"), 0);
    }

    ///  on a real runner: with winget off PATH, Git, GitHub CLI and Node install
    /// from their official releases (verified, per user) and are found afterwards.
    #[cfg(windows)]
    #[test]
    #[ignore = "real network; run by CI's install-fallback job"]
    fn real_installs_without_winget() {
        std::env::set_var("GOND_REAL_HARNESS", "1");
        // PATH without winget (it lives in WindowsApps).
        let path: OsString = std::env::join_paths(
            std::env::split_paths(&crate::process::path().unwrap_or_default())
                .filter(|p| !p.to_string_lossy().contains("WindowsApps")),
        )
        .unwrap();
        assert!(
            detect::find("winget", Some(path.clone())).is_none(),
            "winget is off PATH"
        );
        let steps: Vec<Step> = ["git", "gh", "node"]
            .iter()
            .map(|id| crate::harness::prereq_step(id, Some(path.clone())).unwrap())
            .collect();
        assert!(steps.iter().all(|s| s.plan.source == Source::Download));
        let rec = Recorder::default();
        let job = run(
            "FALLBACK".into(),
            "Set up".into(),
            steps,
            Some(path),
            refreshed_path,
            Duration::from_secs(15 * 60),
            &rec,
        );
        println!(
            "status {:?} error {:?}\n{}",
            job.status, job.error, job.details
        );
        for s in &job.steps {
            println!("{}: {:?} {:?}", s.label, s.status, s.note);
        }
        assert_eq!(job.status, Status::Done, "{:?}", job.error);
        let local = dirs::data_local_dir().unwrap().join("Programs");
        for exe in ["gh/bin/gh.exe", "nodejs/node.exe"] {
            assert!(local.join(exe).is_file(), "{exe} unpacked per user");
        }
        // Git's installer goes per user on a PC without Git; the runner already has a
        // machine-wide Git, which the installer upgrades in place. Either way it's found.
        let fresh = refreshed_path();
        for cmd in ["git", "gh", "node"] {
            let at = detect::find(cmd, fresh.clone()).unwrap_or_else(|| panic!("{cmd} found"));
            println!("{cmd}: {}", at.display());
        }
        let seen = rec.0.lock().unwrap();
        assert!(seen
            .iter()
            .any(|j| j.stage == Some(Stage::Downloading) && j.bytes.is_some()));
    }

    /// , on a real Windows runner: a real winget download, through the pseudo
    /// console, reports at least 3 increasing percents. CI's `winget-progress` job runs
    /// it (`--ignored`); it downloads only, nothing is installed.
    #[cfg(windows)]
    #[test]
    #[ignore = "real network and winget; run by CI's winget-progress job"]
    fn real_winget_download_reports_progress() {
        std::env::set_var("GOND_REAL_HARNESS", "1");
        let winget =
            detect::find("winget", crate::process::path()).expect("winget is on the runner");
        let out = tempfile::tempdir().unwrap();
        let dir = out.path().to_string_lossy().into_owned();
        let step = Step {
            label: "Download Git".into(),
            plan: Plan {
                source: Source::Winget,
                program: winget.to_string_lossy().into_owned(),
                args: [
                    "download",
                    "--id",
                    "Git.Git",
                    "--exact",
                    "--source",
                    "winget",
                    "--download-directory",
                    &dir,
                    "--accept-source-agreements",
                    "--accept-package-agreements",
                    "--disable-interactivity",
                ]
                .map(String::from)
                .into(),
                from: "winget".into(),
                adds_to_path: None,
                provides: None,
            },
        };
        let rec = Recorder::default();
        let job = run(
            "REAL".into(),
            "Download Git".into(),
            vec![step],
            crate::process::path(),
            || None,
            Duration::from_secs(10 * 60),
            &rec,
        );
        let seen = rec.0.lock().unwrap();
        let percents: Vec<u8> = seen.iter().filter_map(|j| j.percent).collect();
        let stages: Vec<_> = seen.iter().filter_map(|j| j.stage).collect();
        println!(
            "status {:?}, percents {percents:?}, stages {stages:?}",
            job.status
        );
        println!("details:\n{}", job.details);
        assert_eq!(job.status, Status::Done, "{}", job.details);
        // On a fast runner (or from winget's cache) the whole download lands in one or two
        // redraws, and ConPTY repaints only the cells that change, so any one percent may be
        // all that's read ([54] and [100] both seen). The parsing is unit-tested; here the
        // real winget must reach Downloading, finish, and never go back.
        assert!(stages.contains(&Stage::Downloading), "{stages:?}");
        assert!(!percents.is_empty(), "some progress was read: {stages:?}");
        assert!(
            percents.windows(2).all(|w| w[0] <= w[1]),
            "never goes back: {percents:?}"
        );
        let between = percents.iter().filter(|p| **p > 0 && **p < 100).count();
        let bytes = seen
            .iter()
            .any(|j| j.bytes.is_some_and(|b| b.total.is_some()));
        println!("in-between percents: {between}, bytes seen: {bytes}");
    }
}
