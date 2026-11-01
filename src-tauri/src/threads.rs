//! Threads behind the `thread.*` commands: each pane's agent is one. A
//! thread records the agent, its account, model and thinking level, the folder it works in and
//! its session, so the pane can start the agent's CLI and resume it after a restart.

use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::projects;
use crate::runner::driver_for;
use crate::store::threads::{self as repo, Thread, ThreadStatus};
use crate::store::{now, Store};

pub struct NewThread {
    pub project_id: String,
    pub agent_id: String,
    pub title: Option<String>,
    /// Run as this account; `None` = the harness's own sign-in.
    pub account_id: Option<String>,
    /// Model and thinking level, checked against the harness's `models`.
    pub model: Option<String>,
    pub thinking: Option<crate::agents::driver::Level>,
}

/// Saves a thread for `new.agent_id` in the project's folder, its model and account checked.
pub fn create(store: &Store, new: NewThread) -> AppResult<Thread> {
    let driver = driver_for(store, &new.agent_id)?;
    let model = new
        .model
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty());
    crate::agents::models::validate(&driver.models(), model, new.thinking)?;
    if let Some(id) = &new.account_id {
        let account = store
            .with(|c| crate::store::accounts::get(c, id))?
            .ok_or_else(|| AppError::NotFound(format!("account {id} not found")))?;
        if account.harness != driver.harness() {
            return Err(AppError::Invalid(format!(
                "account \"{}\" is for {}, not {}",
                account.label,
                account.harness,
                driver.harness()
            )));
        }
    }
    let project = projects::get(store, &new.project_id)?;
    let title = new
        .title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| driver.detect().name);
    let folder = Path::new(&project.path);
    let branch = crate::git::run(folder, &["rev-parse", "--abbrev-ref", "HEAD"])
        .map(|b| b.trim().to_string())
        .unwrap_or_default();
    let at = now();
    let thread = Thread {
        id: ulid::Ulid::generate().to_string(),
        project_id: project.id,
        agent_id: new.agent_id,
        slug: String::new(),
        title,
        branch: branch.clone(),
        base_branch: branch,
        worktree_path: project.path.clone(),
        status: ThreadStatus::Waiting,
        agent_session_id: None,
        pr_url: None,
        created_at: at.clone(),
        updated_at: at,
        archived_at: None,
        account_id: new.account_id,
        model: model.map(String::from),
        thinking: new.thinking,
        handle: None,
        plan: None,
        parent_thread_id: None,
        group_id: None,
        task_id: None,
        role: Some(repo::Role::Chat),
    };
    store.with(|c| repo::insert(c, &thread))?;
    get(store, &thread.id)
}

pub fn get(store: &Store, id: &str) -> AppResult<Thread> {
    store
        .with(|c| repo::get(c, id))?
        .ok_or_else(|| AppError::NotFound(format!("thread {id} not found")))
}

pub fn list(store: &Store, project_id: &str) -> AppResult<Vec<Thread>> {
    store.with(|c| repo::list(c, project_id))
}

/// Closes a thread: its pane was closed. Its agent's session stays the agent's own.
pub fn archive(store: &Store, id: &str) -> AppResult<()> {
    get(store, id)?;
    store.with(|c| repo::archive(c, id))
}

/// Where a terminal opens and who owns it: a thread's folder (owner = the thread id), a
/// project's folder (owner = `project:<id>`), or with neither your home folder (owner = `home`):
/// a new session's shell before you've added any folder.
pub fn terminal_target(
    store: &Store,
    thread_id: Option<&str>,
    project_id: Option<&str>,
) -> AppResult<(String, PathBuf)> {
    match (thread_id, project_id) {
        (Some(t), None) => Ok((t.to_string(), PathBuf::from(get(store, t)?.worktree_path))),
        (None, Some(p)) => Ok((
            project_owner(p),
            PathBuf::from(projects::get(store, p)?.path),
        )),
        (None, None) => Ok((
            "home".into(),
            dirs::home_dir().ok_or_else(|| AppError::Invalid("no home folder".into()))?,
        )),
        _ => Err(AppError::Invalid(
            "give threadId or projectId, not both".into(),
        )),
    }
}

/// The PTY owner key for a project's root terminals.
pub fn project_owner(project_id: &str) -> String {
    format!("project:{project_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pane's agent works in the project's own folder, as it would in a terminal there,
    /// and checks its account is for that agent.
    #[test]
    fn a_thread_works_in_the_project_folder() {
        let store = Store::open_in_memory().unwrap();
        let repo_dir = crate::git::tests::temp_repo();
        let p = projects::add(&store, repo_dir.path()).unwrap();
        let t = create(
            &store,
            NewThread {
                project_id: p.id.clone(),
                agent_id: "claude".into(),
                title: None,
                account_id: None,
                model: None,
                thinking: None,
            },
        )
        .unwrap();
        assert_eq!(t.worktree_path, p.path);
        assert_eq!(t.title, "Claude Code");
        assert_eq!(list(&store, &p.id).unwrap().len(), 1);
        archive(&store, &t.id).unwrap();
        assert!(list(&store, &p.id).unwrap().is_empty());
        let wrong = create(
            &store,
            NewThread {
                project_id: p.id,
                agent_id: "claude".into(),
                title: None,
                account_id: Some("nope".into()),
                model: None,
                thinking: None,
            },
        );
        assert!(wrong.is_err());
    }

    /// A new session's shell before any folder is added opens in your home folder.
    #[test]
    fn a_terminal_with_no_folder_opens_at_home() {
        let store = Store::open_in_memory().unwrap();
        let (owner, cwd) = terminal_target(&store, None, None).unwrap();
        assert_eq!(owner, "home");
        assert_eq!(Some(cwd), dirs::home_dir());
        assert!(terminal_target(&store, Some("t"), Some("p")).is_err());
    }
}
