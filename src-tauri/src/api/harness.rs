//! `harness.*` and `prereq.*` behind the registry.
//! Installs are outward (they download and run vendor code), so policy asks once per
//! job; the job then runs in the background and reports into `/installs/{jobId}`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::registry::{Def, NoInput, Registry, Risk};
use super::state::{Cause, StateTree};
use super::Services;
use crate::agents::detect;
use crate::error::{AppError, AppResult};
use crate::harness::{self};
use crate::install::{self, JobState, Step};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanInput {
    /// A catalog id: "claude", "codex", "agy", "grok", "opencode", "pi", "copilot" or "cursor".
    pub harness_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallInput {
    pub harness_id: String,
    /// Which of `harness.plan`'s install plans; default the first (best).
    #[serde(default)]
    pub plan_index: Option<usize>,
    /// Install winget steps for all users (one Windows prompt). Only after a per-user
    /// attempt reported `machineScopeAvailable` and the human chose it.
    #[serde(default)]
    pub machine_scope: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetupInput {
    /// Prerequisite ids from `prereq.list` ("git", "gh", "node", "bun"). Installed ones
    /// are skipped.
    pub prereqs: Vec<String>,
    /// As `harness.install`'s `machineScope`.
    #[serde(default)]
    pub machine_scope: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Prereq {
    pub id: String,
    pub name: String,
    pub installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Started {
    /// Follow it at `/installs/{jobId}`.
    pub job_id: String,
    pub steps: Vec<Step>,
}

/// Mirrors a job into `/installs/{id}`.
struct TreeProgress(StateTree);

impl install::Progress for TreeProgress {
    fn update(&self, job: &JobState) {
        let value = serde_json::to_value(job).unwrap_or_default();
        let cause = Cause::Runtime {
            runtime: "install".into(),
        };
        self.0
            .commit(cause, |d| d["installs"][job.id.as_str()] = value);
    }
}

/// The PATH jobs run with. Tests get an empty one, so a registry test can never start a
/// real install (Windows CI has winget).
fn job_path() -> Option<std::ffi::OsString> {
    if cfg!(test) {
        Some(
            std::env::temp_dir()
                .join("gond-code-no-tools")
                .into_os_string(),
        )
    } else {
        crate::process::path()
    }
}

/// The harnesses with an install, update or uninstall running now.
static BUSY: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// One job per harness at a time: a second click on Install while one runs would start a
/// second installer over the first (Cursor's script deletes its folder first, so one fails).
/// Held for the job's life; dropping it frees the harness.
struct Busy(String);

impl Busy {
    fn claim(h: &harness::Harness) -> AppResult<Busy> {
        let mut busy = BUSY.lock().unwrap_or_else(|e| e.into_inner());
        if busy.iter().any(|b| b == h.id) {
            return Err(AppError::Invalid(format!(
                "{} is already being installed, updated or removed. Wait for it to finish.",
                h.name
            )));
        }
        busy.push(h.id.to_string());
        Ok(Busy(h.id.to_string()))
    }
}

impl Drop for Busy {
    fn drop(&mut self) {
        BUSY.lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|b| *b != self.0);
    }
}

/// `start` for a harness's own job, once nothing else is running for it.
fn start_harness(
    services: &Services,
    id: &str,
    title: String,
    steps: Vec<Step>,
) -> AppResult<Started> {
    let busy = Busy::claim(find(id)?)?;
    Ok(start_holding(services, title, steps, Some(busy)))
}

/// Starts `steps` on a background thread and returns at once.
fn start(services: &Services, title: String, steps: Vec<Step>) -> Started {
    start_holding(services, title, steps, None)
}

fn start_holding(
    services: &Services,
    title: String,
    steps: Vec<Step>,
    busy: Option<Busy>,
) -> Started {
    let job_id = ulid::Ulid::generate().to_string();
    let progress = TreeProgress(services.state.clone());
    let (id, run_steps) = (job_id.clone(), steps.clone());
    std::thread::spawn(move || {
        let _busy = busy;
        install::run(
            id,
            title,
            run_steps,
            job_path(),
            install::refreshed_path,
            install::STEP_TIMEOUT,
            &progress,
        );
    });
    Started { job_id, steps }
}

fn find(id: &str) -> AppResult<&'static harness::Harness> {
    harness::get(id).ok_or_else(|| {
        AppError::NotFound(format!(
            "no harness {id}; try one of: claude, codex, agy, grok, opencode, pi, copilot, cursor"
        ))
    })
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CustomSaveInput {
    pub agent: crate::agents::CustomAgent,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CustomDeleteInput {
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobInput {
    pub job_id: String,
}

pub fn register(r: &mut Registry<Services>) {
    r.add(
        Def::new(
            "install.cancel",
            "Stop a running install job (its current step's whole process tree)",
            Risk::WriteLocal,
        ),
        |_, i: JobInput| -> AppResult<()> {
            if install::cancel(&i.job_id) {
                Ok(())
            } else {
                Err(AppError::NotFound(format!(
                    "no running install job {}; it may have finished",
                    i.job_id
                )))
            }
        },
    );
    r.add(
        Def::new(
            "agent.detect",
            "Which agent CLIs are installed, with their versions",
            Risk::Read,
        ),
        |call, _: super::registry::NoInput| -> AppResult<Vec<crate::agents::AgentInfo>> {
            let customs = crate::agents::list_custom(&call.ctx.store)?;
            Ok(crate::agents::detect(&customs))
        },
    );
    r.add(
        Def::new(
            "agent.custom.save",
            "Add or update a custom agent (a command that takes the prompt)",
            Risk::WriteLocal,
        ),
        |call, i: CustomSaveInput| crate::agents::save_custom(&call.ctx.store, i.agent),
    );
    r.add(
        Def::new(
            "agent.custom.delete",
            "Delete a custom agent",
            Risk::WriteLocal,
        ),
        |call, i: CustomDeleteInput| crate::agents::delete_custom(&call.ctx.store, &i.id),
    );
    r.add(
        Def::new(
            "harness.check",
            "Installed vs latest version per harness, with the changelog (offline: latest unknown)",
            Risk::Read,
        ),
        |_, _: NoInput| -> AppResult<Vec<crate::releases::Check>> {
            let list = harness::list(crate::process::path());
            Ok(std::thread::scope(|s| {
                let handles: Vec<_> = list
                    .into_iter()
                    .map(|h| {
                        s.spawn(move || {
                            crate::releases::check(
                                &h.id,
                                h.version,
                                &h.changelog,
                                crate::releases::fetch,
                            )
                        })
                    })
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            }))
        },
    );
    r.add(
        Def::new(
            "prereq.list",
            "Tools harnesses and projects need (Git, GitHub CLI, Node.js, Bun): installed?",
            Risk::Read,
        ),
        |_, _: NoInput| -> AppResult<Vec<Prereq>> {
            let path = crate::process::path();
            Ok(harness::PREREQS
                .iter()
                .map(|&(id, name, command, ..)| {
                    let bin = detect::find(command, path.clone());
                    Prereq {
                        id: id.into(),
                        name: name.into(),
                        installed: bin.is_some(),
                        version: bin.as_deref().and_then(detect::version),
                    }
                })
                .collect())
        },
    );
    r.add(
        Def::new(
            "prereq.install",
            "Install missing prerequisites in one background job",
            Risk::Outward,
        ),
        |call, i: SetupInput| -> AppResult<Started> {
            let (title, steps) = resolve_prereqs(&i.prereqs, i.machine_scope)?;
            Ok(start(call.ctx, title, steps))
        },
    );
    r.add(
        Def::new(
            "harness.install",
            "Install a harness in the background, with any prerequisites, in one job",
            Risk::Outward,
        ),
        |call, i: InstallInput| -> AppResult<Started> {
            let (title, steps) = resolve_install(&i.harness_id, i.plan_index, i.machine_scope)?;
            start_harness(call.ctx, &i.harness_id, title, steps)
        },
    );
    r.add(
        Def::new(
            "harness.uninstallPlan",
            "What uninstalling a harness would remove, for the confirm card",
            Risk::Read,
        ),
        |call, i: UninstallInput| preview_uninstall(call.ctx, &i.harness_id, !i.keep_data),
    );
    r.add(
        Def::new(
            "harness.uninstall",
            "Uninstall a harness: every install of it, its PATH entries, and optionally its settings",
            Risk::Destructive,
        )
        .human_only(),
        |call, i: UninstallInput| -> AppResult<Started> {
            start_uninstall(call.ctx, &i.harness_id, !i.keep_data)
        },
    );
    r.add(
        Def::new(
            "harness.update",
            "Update a harness the way it was installed, in the background",
            Risk::Outward,
        ),
        |call, i: PlanInput| -> AppResult<Started> {
            let (title, steps) = resolve_update(&i.harness_id)?;
            start_harness(call.ctx, &i.harness_id, title, steps)
        },
    );
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UninstallInput {
    pub harness_id: String,
    /// Keep its settings and sign-ins, and Gond's accounts for it (default true). `false`
    /// is "Remove everything".
    #[serde(default = "keep")]
    pub keep_data: bool,
}

fn keep() -> bool {
    true
}

/// `harness.uninstallPlan`: the confirm card's list, plus what blocks or adds to it.
#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UninstallPreview {
    #[serde(flatten)]
    pub plan: crate::uninstall::UninstallPlan,
    /// Agents of this harness running now; uninstall refuses until they stop.
    pub running: usize,
    /// Gond accounts for this harness ("Remove everything" deletes them).
    pub accounts: usize,
}

/// Whether a thread has a terminal open: its agent's CLI is running in a pane.
fn has_terminal(s: &Services, thread_id: &str) -> bool {
    s.state.get("/terminals").is_some_and(|(_, t)| {
        t.as_object()
            .is_some_and(|m| m.values().any(|x| x["owner"] == thread_id))
    })
}

#[cfg(test)]
pub static TEST_ROOTS: std::sync::Mutex<Option<crate::uninstall::Roots>> =
    std::sync::Mutex::new(None);

/// The PATH an uninstall looks a harness up on: the app's, except in
/// tests that point it at temporary folders, where a harness really installed on the
/// machine running them mustn't count.
fn still_installed_path() -> Option<std::ffi::OsString> {
    #[cfg(test)]
    if TEST_ROOTS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .is_some()
    {
        return Some(std::ffi::OsString::new());
    }
    crate::process::path()
}

fn uninstall_roots() -> AppResult<crate::uninstall::Roots> {
    #[cfg(test)]
    if let Some(r) = TEST_ROOTS.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        return Ok(r);
    }
    crate::uninstall::Roots::current()
        .ok_or_else(|| AppError::Invalid("couldn't find your user folders".into()))
}

pub fn preview_uninstall(s: &Services, id: &str, everything: bool) -> AppResult<UninstallPreview> {
    let h = find(id)?;
    let roots = uninstall_roots()?;
    let accounts = crate::accounts::list(&s.store, Some(id), None)?;
    let homes: Vec<std::path::PathBuf> = if everything {
        accounts
            .iter()
            .filter_map(|a| a.config_dir.as_ref().map(std::path::PathBuf::from))
            .collect()
    } else {
        vec![]
    };
    let running = s
        .store
        .with(crate::store::threads::list_unarchived)?
        .iter()
        // An open pane runs the agent's CLI, which holds its files open.
        .filter(|t| t.agent_id == id && has_terminal(s, &t.id))
        .count();
    Ok(UninstallPreview {
        plan: crate::uninstall::plan(
            h,
            &roots,
            &crate::pathenv::user_entries(),
            everything,
            &homes,
        ),
        running,
        accounts: accounts.len(),
    })
}

/// Starts the uninstall job; after it, Gond looks again so the card can't show a
/// harness as gone while it's still on PATH.
pub fn start_uninstall(s: &Services, id: &str, everything: bool) -> AppResult<Started> {
    let h = find(id)?;
    let p = preview_uninstall(s, id, everything)?;
    if p.running > 0 {
        return Err(AppError::Invalid(format!(
            "{} {} agent{} running. Stop {} first, then uninstall.",
            p.running,
            h.name,
            if p.running == 1 { " is" } else { "s are" },
            if p.running == 1 { "it" } else { "them" },
        )));
    }
    if p.plan.steps.is_empty() {
        return Err(AppError::Invalid(
            match detect::find(h.command, still_installed_path()) {
                Some(bin) => format!(
                "Gond can't tell how {} was installed at {}. Remove it the way you installed it.",
                h.name,
                bin.display()
            ),
                None => format!("{} isn't installed.", h.name),
            },
        ));
    }
    let busy = Busy::claim(h)?;
    let job_id = ulid::Ulid::generate().to_string();
    let steps = p.plan.steps.clone();
    let (svc, jid, name, command) = (s.clone(), job_id.clone(), h.name, h.command);
    let harness_id = id.to_string();
    std::thread::spawn(move || {
        let _busy = busy;
        let progress = TreeProgress(svc.state.clone());
        let mut job = install::run(
            jid,
            format!("Uninstall {name}"),
            steps,
            job_path(),
            install::refreshed_path,
            install::STEP_TIMEOUT,
            &progress,
        );
        if job.status != install::Status::Done {
            return;
        }
        if let Some(bin) = detect::find(command, still_installed_path()) {
            job.status = install::Status::Failed;
            job.error = Some(format!(
                "Gond removed what it installed, but {name} is still on this PC at {} (installed another way). Remove that copy the way it was installed.",
                bin.display()
            ));
        } else {
            if everything {
                if let Ok(accounts) = crate::accounts::list(&svc.store, Some(&harness_id), None) {
                    for a in accounts {
                        let _ = crate::accounts::remove(
                            &svc.store,
                            &*svc.vault,
                            &svc.accounts_root,
                            &a.id,
                        );
                    }
                }
                let _ = super::accounts::sync_accounts(
                    &svc,
                    Cause::Runtime {
                        runtime: "install".into(),
                    },
                );
            }
            if let Some(last) = job.steps.last_mut() {
                last.note = Some(format!("{name} is uninstalled."));
            }
        }
        install::Progress::update(&progress, &job);
    });
    Ok(Started {
        job_id,
        steps: p.plan.steps,
    })
}

/// The job that installs the missing prerequisites among `ids`.
/// Winget steps for all users when the human chose it; everything else unchanged.
fn scoped(steps: Vec<Step>, machine: bool) -> Vec<Step> {
    if !machine {
        return steps;
    }
    steps
        .into_iter()
        .map(|s| Step {
            plan: s.plan.machine_wide(),
            ..s
        })
        .collect()
}

pub fn resolve_prereqs(ids: &[String], machine: bool) -> AppResult<(String, Vec<Step>)> {
    let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
    if let Some(bad) = ids
        .iter()
        .find(|id| !harness::PREREQS.iter().any(|p| p.0 == **id))
    {
        return Err(AppError::NotFound(format!(
            "no prerequisite {bad}; try git, gh, node or bun"
        )));
    }
    let path = crate::process::path();
    let steps: Vec<Step> = harness::missing_prereqs(&ids, path.clone())
        .into_iter()
        .filter_map(|id| harness::prereq_step(id, path.clone()))
        .collect();
    if steps.is_empty() {
        return Err(AppError::Invalid("everything is already installed".into()));
    }
    Ok(("Set up".into(), scoped(steps, machine)))
}

/// The job that installs harness `id` with plan `plan_index`, prerequisites first.
pub fn resolve_install(
    id: &str,
    plan_index: Option<usize>,
    machine: bool,
) -> AppResult<(String, Vec<Step>)> {
    let h = find(id)?;
    let steps =
        harness::install_steps(h, plan_index, crate::process::path()).map_err(AppError::Invalid)?;
    Ok((format!("Install {}", h.name), scoped(steps, machine)))
}

/// The job that updates harness `id` the way it was installed.
pub fn resolve_update(id: &str) -> AppResult<(String, Vec<Step>)> {
    let h = find(id)?;
    let bin = detect::find(h.command, crate::process::path())
        .ok_or_else(|| AppError::Invalid(format!("{} isn't installed", h.name)))?;
    let plan = h.update_plan(harness::source_of(&bin)).ok_or_else(|| {
        AppError::Invalid(format!(
            "{} was installed some way Gond can't update; update it the way you installed it",
            h.name
        ))
    })?;
    let title = format!("Update {}", h.name);
    Ok((title.clone(), vec![Step { label: title, plan }]))
}

#[cfg(test)]
mod tests {
    use crate::api::{registry, Actor, Services};
    use serde_json::json;

    #[test]
    fn detect_and_install_plans() {
        let (r, s, h) = (registry(), Services::default(), Actor::human());
        // Every built-in agent is listed, installed or not, for the New agent picker.
        let found = r.run(&s, &h, "agent.detect", json!({})).unwrap();
        for id in ["claude", "codex", "agy", "grok", "opencode", "pi"] {
            assert!(
                found.as_array().unwrap().iter().any(|a| a["id"] == id),
                "{id}"
            );
        }
        // Grok installs with winget, else x.ai's script; an unknown id is not found.
        let p = serde_json::to_value(super::find("grok").unwrap().install_plans()).unwrap();
        assert_eq!(p[0]["program"], "winget");
        assert_eq!(p[1]["from"], "https://x.ai/cli/install.ps1");
        assert_eq!(super::find("gemini").err().unwrap().code(), "not_found");
    }

    #[test]
    fn installs_run_in_the_background_and_report_into_the_tree() {
        let (r, s, h) = (registry(), Services::default(), Actor::human());
        let started = r
            .run(&s, &h, "harness.install", json!({ "harnessId": "grok" }))
            .unwrap();
        let id = started["jobId"].as_str().unwrap().to_string();
        assert_eq!(started["steps"][0]["label"], "Install Grok Build");
        // Tests run jobs with an empty PATH: the job fails with a plain sentence.
        let begin = std::time::Instant::now();
        let job = loop {
            if let Some((_, j)) = s.state.get(&format!("/installs/{id}")) {
                if j["status"] == "failed" || j["status"] == "done" {
                    break j;
                }
            }
            assert!(begin.elapsed() < std::time::Duration::from_secs(10));
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        assert_eq!(job["status"], "failed");
        assert!(job["error"].as_str().unwrap().contains("isn't installed"));
    }

    #[test]
    fn cancelling_an_unknown_job_is_not_found() {
        let (r, s, h) = (registry(), Services::default(), Actor::human());
        let e = r
            .run(&s, &h, "install.cancel", json!({ "jobId": "nope" }))
            .unwrap_err();
        assert_eq!(e.code(), "not_found");
    }

    #[test]
    fn bad_setup_and_update_inputs() {
        let (r, s, h) = (registry(), Services::default(), Actor::human());
        let e = r
            .run(
                &s,
                &h,
                "prereq.install",
                json!({ "prereqs": ["git", "python"] }),
            )
            .unwrap_err();
        assert_eq!(e.code(), "not_found");
        let list = r.run(&s, &h, "prereq.list", json!({})).unwrap();
        // github.* answers gh_missing; this is what the UI offers next.
        assert!(r.get("prereq.install").is_some());
        assert!(
            list.as_array().unwrap().iter().any(|p| p["id"] == "gh"),
            "{list}"
        );
        assert_eq!(list.as_array().unwrap().len(), 4);
        let e = r
            .run(
                &s,
                &h,
                "harness.install",
                json!({ "harnessId": "agy", "planIndex": 5 }),
            )
            .unwrap_err();
        assert_eq!(e.code(), "invalid");
    }

    /// One job per harness: a second while one runs is refused plainly, and the harness is
    /// free again once the first ends.
    #[test]
    fn one_job_per_harness_at_a_time() {
        let pi = crate::harness::get("pi").unwrap();
        let first = super::Busy::claim(pi).unwrap();
        let e = super::Busy::claim(pi).err().unwrap();
        assert!(
            e.to_string().contains("Pi is already being installed"),
            "{e}"
        );
        // Another harness isn't held up.
        drop(super::Busy::claim(crate::harness::get("cursor").unwrap()).unwrap());
        drop(first);
        drop(super::Busy::claim(pi).unwrap());
    }

    #[test]
    fn machine_scope_only_changes_winget_steps() {
        let (_, user) = super::resolve_install("grok", Some(0), false).unwrap();
        let (_, all) = super::resolve_install("grok", Some(0), true).unwrap();
        assert!(user[0]
            .plan
            .args
            .windows(2)
            .any(|w| w == ["--scope", "user"]));
        assert!(all[0]
            .plan
            .args
            .windows(2)
            .any(|w| w == ["--scope", "machine"]));
        let (_, vendor) = super::resolve_install("grok", Some(1), true).unwrap();
        assert_eq!(vendor[0].plan.program, "powershell");
    }

    /// a real install on Windows through the engine the app runs, with the real
    /// PATH. Ignored by default; `.github/workflows/install-check.yml` runs it per
    /// harness and plan (GOND_REAL_HARNESS, GOND_REAL_PLAN) on a fresh runner.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn real_install() {
        use crate::install::{self, JobState, Progress};
        struct Print;
        impl Progress for Print {
            fn update(&self, j: &JobState) {
                let steps: Vec<_> = j
                    .steps
                    .iter()
                    .map(|s| format!("{}={:?}", s.label, s.status))
                    .collect();
                println!(
                    "progress {:?} {:?}% {}",
                    j.status,
                    j.percent,
                    steps.join(", ")
                );
            }
        }
        let id = std::env::var("GOND_REAL_HARNESS").expect("GOND_REAL_HARNESS");
        let plan = std::env::var("GOND_REAL_PLAN")
            .ok()
            .and_then(|p| p.parse().ok());
        let (title, steps) = super::resolve_install(&id, plan, false).unwrap();
        for s in &steps {
            println!(
                "step {}: {} {}",
                s.label,
                s.plan.program,
                s.plan.args.join(" ")
            );
        }
        let job = install::run(
            "real".into(),
            title,
            steps,
            crate::process::path(),
            install::refreshed_path,
            install::STEP_TIMEOUT,
            &Print,
        );
        println!("JOB {}", serde_json::to_string_pretty(&job).unwrap());
        let h = crate::harness::get(&id).unwrap();
        let bin = crate::agents::detect::find(h.command, crate::process::path());
        println!(
            "found {:?} version {:?}",
            bin,
            bin.as_deref().and_then(crate::agents::detect::version)
        );
        assert_eq!(job.status, install::Status::Done, "{:?}", job.error);
        assert!(
            bin.is_some(),
            "{} installed but not found on the refreshed PATH",
            h.name
        );
    }

    /// A real update the way the harness was installed (winget upgrade, its own updater, Bun),
    /// through the engine the app runs; it ends done and the agent is still found. Ignored
    /// by default: `GOND_REAL_HARNESS=opencode cargo test --lib real_update -- --ignored`.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn real_update() {
        use crate::install::{self, JobState, Progress};
        struct Quiet;
        impl Progress for Quiet {
            fn update(&self, _: &JobState) {}
        }
        let id = std::env::var("GOND_REAL_HARNESS").expect("GOND_REAL_HARNESS");
        let (title, steps) = super::resolve_update(&id).unwrap();
        for s in &steps {
            println!(
                "step {}: {} {}",
                s.label,
                s.plan.program,
                s.plan.args.join(" ")
            );
        }
        let job = install::run(
            "real-up".into(),
            title,
            steps,
            crate::process::path(),
            install::refreshed_path,
            install::STEP_TIMEOUT,
            &Quiet,
        );
        println!("JOB {}", serde_json::to_string_pretty(&job).unwrap());
        assert_eq!(job.status, install::Status::Done, "{:?}", job.error);
        let h = crate::harness::get(&id).unwrap();
        assert!(crate::agents::detect::find(h.command, install::refreshed_path()).is_some());
    }

    ///  through the registry: the card's plan, the job, the files gone, and a plain
    /// refusal when there's nothing to remove.
    #[test]
    fn uninstall_through_the_registry() {
        let (r, s, h) = (registry(), Services::default(), Actor::human());
        let t = tempfile::tempdir().unwrap();
        let roots = crate::uninstall::Roots {
            home: t.path().join("u"),
            local: t.path().join("u/AppData/Local"),
            roaming: t.path().join("u/AppData/Roaming"),
        };
        let exe = roots.local.join("agy/bin/agy.exe");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, "x").unwrap();
        let settings = roots.home.join(".gemini/antigravity-cli/settings.json");
        std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
        std::fs::write(&settings, "{}").unwrap();
        *super::TEST_ROOTS.lock().unwrap() = Some(roots.clone());

        let p = r
            .run(
                &s,
                &h,
                "harness.uninstallPlan",
                json!({ "harnessId": "agy" }),
            )
            .unwrap();
        assert_eq!(p["methods"], json!(["vendor"]));
        assert_eq!(
            p["deletes"],
            json!([roots.local.join("agy").to_string_lossy()])
        );
        assert_eq!(
            (p["running"].clone(), p["keepData"].clone()),
            (json!(0), json!(true))
        );
        assert_eq!(
            r.get("harness.uninstall").unwrap().risk(&json!({})),
            crate::api::Risk::Destructive
        );

        let started = r
            .run(&s, &h, "harness.uninstall", json!({ "harnessId": "agy" }))
            .unwrap();
        let id = started["jobId"].as_str().unwrap().to_string();
        let begin = std::time::Instant::now();
        let job = loop {
            if let Some((_, j)) = s.state.get(&format!("/installs/{id}")) {
                if (j["status"] == "done" && j["steps"][0]["note"].is_string())
                    || j["status"] == "failed"
                {
                    break j;
                }
            }
            // A loaded Windows runner can take well over 10 s to finish the job.
            assert!(
                begin.elapsed() < std::time::Duration::from_secs(90),
                "uninstall job didn't settle"
            );
            std::thread::sleep(std::time::Duration::from_millis(50));
        };
        assert_eq!(job["status"], "done", "{job}");
        assert_eq!(job["steps"][0]["note"], "Antigravity is uninstalled.");
        assert!(!roots.local.join("agy").exists());
        assert!(settings.exists(), "settings kept by default");

        let e = r
            .run(&s, &h, "harness.uninstall", json!({ "harnessId": "agy" }))
            .unwrap_err();
        assert_eq!(e.code(), "invalid");
        assert!(e.to_string().contains("isn't installed"), "{e}");
        *super::TEST_ROOTS.lock().unwrap() = None;
    }

    ///  on real Windows: after `real_install`, uninstall the same harness and check
    /// the program, its folders and its PATH entries are gone. GOND_REAL_EVERYTHING=1
    /// also removes settings. Run by the Install check workflow.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn real_uninstall() {
        use crate::install::{self, JobState, Progress};
        struct Print;
        impl Progress for Print {
            fn update(&self, j: &JobState) {
                let steps: Vec<_> = j
                    .steps
                    .iter()
                    .map(|s| format!("{}={:?}", s.label, s.status))
                    .collect();
                println!("progress {:?} {}", j.status, steps.join(", "));
            }
        }
        let id = std::env::var("GOND_REAL_HARNESS").expect("GOND_REAL_HARNESS");
        let everything = std::env::var("GOND_REAL_EVERYTHING").is_ok_and(|v| v == "1");
        let s = Services::default();
        let p = super::preview_uninstall(&s, &id, everything).unwrap();
        println!("PLAN {}", serde_json::to_string_pretty(&p).unwrap());
        assert!(!p.plan.steps.is_empty(), "nothing to uninstall");
        let job = install::run(
            "real-un".into(),
            "Uninstall".into(),
            p.plan.steps.clone(),
            crate::process::path(),
            install::refreshed_path,
            install::STEP_TIMEOUT,
            &Print,
        );
        println!("JOB {}", serde_json::to_string_pretty(&job).unwrap());
        assert_eq!(job.status, install::Status::Done, "{:?}", job.error);
        let h = crate::harness::get(&id).unwrap();
        let left = crate::agents::detect::find(h.command, install::refreshed_path());
        assert!(left.is_none(), "{} still found at {left:?}", h.command);
        for d in &p.plan.deletes {
            assert!(!std::path::Path::new(d).exists(), "{d} is still there");
        }
        let path = crate::pathenv::user_entries();
        for e in &p.plan.path_entries {
            assert!(
                !path.iter().any(|x| crate::pathenv::same_entry(x, e)),
                "{e} still on PATH"
            );
        }
        // No PATH entry may point at a folder this uninstall deleted.
        for x in &path {
            assert!(
                !p.plan
                    .deletes
                    .iter()
                    .any(|d| std::path::Path::new(x).starts_with(d)),
                "PATH entry {x} points into a deleted folder"
            );
        }
        let after = super::preview_uninstall(&s, &id, everything).unwrap();
        assert!(
            after.plan.steps.is_empty(),
            "left behind: {:?}",
            after.plan.deletes
        );
    }
}
