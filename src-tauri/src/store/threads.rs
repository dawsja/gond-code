use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::error::AppResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ThreadStatus {
    Starting,
    Running,
    Waiting,
    Done,
    Error,
    Stopped,
}

impl ThreadStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ThreadStatus::Starting => "starting",
            ThreadStatus::Running => "running",
            ThreadStatus::Waiting => "waiting",
            ThreadStatus::Done => "done",
            ThreadStatus::Error => "error",
            ThreadStatus::Stopped => "stopped",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "starting" => ThreadStatus::Starting,
            "running" => ThreadStatus::Running,
            "waiting" => ThreadStatus::Waiting,
            "done" => ThreadStatus::Done,
            "error" => ThreadStatus::Error,
            _ => ThreadStatus::Stopped,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub id: String,
    pub project_id: String,
    pub agent_id: String,
    pub title: String,
    pub slug: String,
    pub branch: String,
    pub base_branch: String,
    pub worktree_path: String,
    pub status: ThreadStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_url: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
    /// The account the agent runs as; `None` = the harness's own sign-in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    /// Model and thinking level for every turn; `None` = the harness default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<crate::agents::driver::Level>,
    /// The agent's name everywhere: cards, Talk, @mentions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
    /// The agent's own plan (`thread.plan.set`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<Plan>,
    /// The thread whose agent created this one; `None` for the human's threads.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_thread_id: Option<String>,
    /// The crew it was started with (`thread.createMany`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<String>,
    /// The task that crew works on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// The agent's role in its crew; `None` = builder. Stored only: what a role
    /// may do is enforced at spawn.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Builder,
    Reviewer,
    Researcher,
    Judge,
    /// Gond in the Chat space: talks with the human and starts a crew for the work.
    Chat,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Builder => "builder",
            Role::Reviewer => "reviewer",
            Role::Researcher => "researcher",
            Role::Judge => "judge",
            Role::Chat => "chat",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum StepState {
    Done,
    Now,
    Next,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanStep {
    pub id: String,
    pub text: String,
    pub state: StepState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub steps: Vec<PlanStep>,
    pub updated_at: String,
}

const COLS: &str = "id, project_id, agent_id, title, slug, branch, base_branch, worktree_path, \
    status, agent_session_id, pr_url, created_at, updated_at, archived_at, account_id, model, thinking, handle, plan, parent_thread_id, group_id, task_id, role";

fn from_row(r: &Row) -> rusqlite::Result<Thread> {
    Ok(Thread {
        id: r.get(0)?,
        project_id: r.get(1)?,
        agent_id: r.get(2)?,
        title: r.get(3)?,
        slug: r.get(4)?,
        branch: r.get(5)?,
        base_branch: r.get(6)?,
        worktree_path: r.get(7)?,
        status: ThreadStatus::parse(&r.get::<_, String>(8)?),
        agent_session_id: r.get(9)?,
        pr_url: r.get(10)?,
        created_at: r.get(11)?,
        updated_at: r.get(12)?,
        archived_at: r.get(13)?,
        account_id: r.get(14)?,
        model: r.get(15)?,
        thinking: r
            .get::<_, Option<String>>(16)?
            .and_then(|t| serde_json::from_value(serde_json::Value::String(t)).ok()),
        handle: r.get(17)?,
        plan: r
            .get::<_, Option<String>>(18)?
            .and_then(|p| serde_json::from_str(&p).ok()),
        parent_thread_id: r.get(19)?,
        group_id: r.get(20)?,
        task_id: r.get(21)?,
        role: r
            .get::<_, Option<String>>(22)?
            .and_then(|t| serde_json::from_value(serde_json::Value::String(t)).ok()),
    })
}

pub fn insert(c: &Connection, t: &Thread) -> AppResult<()> {
    c.execute(
        &format!("INSERT INTO threads ({COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)"),
        params![
            t.id, t.project_id, t.agent_id, t.title, t.slug, t.branch, t.base_branch,
            t.worktree_path, t.status.as_str(), t.agent_session_id, t.pr_url, t.created_at,
            t.updated_at, t.archived_at, t.account_id, t.model,
            t.thinking.map(|l| l.as_str()),
            t.handle,
            t.plan.as_ref().and_then(|p| serde_json::to_string(p).ok()),
            t.parent_thread_id,
            t.group_id,
            t.task_id,
            t.role.map(|r| r.as_str())
        ],
    )?;
    Ok(())
}

pub fn get(c: &Connection, id: &str) -> AppResult<Option<Thread>> {
    let sql = format!("SELECT {COLS} FROM threads WHERE id = ?1");
    Ok(c.query_row(&sql, [id], from_row).optional()?)
}

/// A project's threads that aren't archived, most recently updated first.
pub fn list(c: &Connection, project_id: &str) -> AppResult<Vec<Thread>> {
    let mut stmt = c.prepare(&format!(
        "SELECT {COLS} FROM threads WHERE project_id = ?1 AND archived_at IS NULL
         ORDER BY updated_at DESC, created_at DESC"
    ))?;
    let rows = stmt.query_map([project_id], from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Every unarchived thread, across projects (the state tree's `/threads`).
pub fn list_unarchived(c: &Connection) -> AppResult<Vec<Thread>> {
    let mut stmt = c.prepare(&format!(
        "SELECT {COLS} FROM threads WHERE archived_at IS NULL ORDER BY updated_at DESC"
    ))?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn set_session(c: &Connection, id: &str, session_id: &str) -> AppResult<()> {
    c.execute(
        "UPDATE threads SET agent_session_id = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, session_id, super::now()],
    )?;
    Ok(())
}

pub fn archive(c: &Connection, id: &str) -> AppResult<()> {
    let at = super::now();
    c.execute(
        "UPDATE threads SET archived_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![id, at],
    )?;
    Ok(())
}

/// On startup, threads left `running`/`starting` lost their process with the app.
pub fn stop_orphans(c: &Connection) -> AppResult<usize> {
    Ok(c.execute(
        "UPDATE threads SET status = 'stopped', updated_at = ?1
         WHERE status IN ('running', 'starting')",
        params![super::now()],
    )?)
}

/// Whether an agent is running as `account_id` right now.
pub fn account_in_use(c: &Connection, account_id: &str) -> AppResult<bool> {
    Ok(c.query_row(
        "SELECT EXISTS(SELECT 1 FROM threads WHERE account_id = ?1 AND status IN ('starting', 'running'))",
        [account_id],
        |r| r.get(0),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{projects, Store};

    #[test]
    fn startup_sweep_stops_live_threads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gond-code.db");
        let s = Store::open(&path).unwrap();
        s.with(|c| {
            projects::insert(
                c,
                &projects::tests::sample("p", "/p", "2026-10-29T00:00:00Z"),
            )?;
            for (id, status) in [("t1", "running"), ("t2", "starting"), ("t3", "waiting")] {
                c.execute(
                    "INSERT INTO threads (id, project_id, agent_id, title, slug, branch,
                     base_branch, worktree_path, status, created_at, updated_at)
                     VALUES (?1, 'p', 'claude', 't', 't', 'gond/t', 'main', '/w', ?2, 'x', 'x')",
                    params![id, status],
                )?;
            }
            Ok(())
        })
        .unwrap();
        drop(s);

        let s = Store::open(&path).unwrap();
        let statuses: Vec<String> = s
            .with(|c| {
                let mut st = c.prepare("SELECT status FROM threads ORDER BY id")?;
                let rows = st.query_map([], |r| r.get(0))?;
                Ok(rows.collect::<Result<_, _>>()?)
            })
            .unwrap();
        assert_eq!(statuses, ["stopped", "stopped", "waiting"]);
    }
}
