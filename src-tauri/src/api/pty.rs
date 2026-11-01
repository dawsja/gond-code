//! `pty.*` behind the registry: terminals at a project's root (or your home folder), and
//! `pty.read` so a caller can run a command and read its result. Raw output still streams to the UI as `pty:data`; the
//! tree keeps one entry per open terminal at `/terminals/{ptyId}`.

use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::registry::{Call, Def, Registry, Risk};
use super::state::Cause;
use super::Services;
use crate::error::{AppError, AppResult};
use crate::pty::PtyRead;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenInput {
    /// Open in this project's folder; without one, your home folder.
    pub project_id: Option<String>,
    /// A shell.list id (`pwsh`, `cmd`, `wsl:Ubuntu`…), so a pane keeps the shell it had; none
    /// (or one no longer on this PC) = the one picked in Settings → General.
    pub shell: Option<String>,
    /// Start in this folder instead (a shell coming back after an update, where it was).
    pub cwd: Option<String>,
    /// The pane this terminal is for: it outlives the app, and `pty.attach` finds it again by
    /// this key. A terminal already under the key is closed first (the pane restarted).
    pub key: Option<String>,
    /// Default 120.
    pub cols: Option<u16>,
    /// Default 32.
    pub rows: Option<u16>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FolderInput {
    pub project_id: String,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Opened {
    pub pty_id: String,
    /// The shell a plain terminal runs (shell.list id), for its pane to reopen the same one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteInput {
    pub pty_id: String,
    /// UTF-8 text; end a command with "\r".
    pub data: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResizeInput {
    pub pty_id: String,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PtyInput {
    pub pty_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttachInput {
    /// The pane's id (`pty.open`'s `key`).
    pub key: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PruneInput {
    /// Every pane that exists; the others' terminals are closed.
    pub keep: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadInput {
    pub pty_id: String,
    /// Only output after this offset (from a previous read).
    pub since: Option<u64>,
    /// Wait until the new output matches this regex.
    pub wait_for: Option<String>,
    /// How long `waitFor` waits; default 10 000, max 120 000.
    pub timeout_ms: Option<u64>,
}

fn cause(call: &Call<Services>, command: &str) -> Cause {
    Cause::Command {
        event_id: None,
        command: command.into(),
        actor: call.actor.clone(),
    }
}

/// Every agent Gond knows, built in and added: (id, command).
pub fn known_agents(store: &crate::store::Store) -> Vec<(String, String)> {
    let mut agents: Vec<(String, String)> = crate::agents::BUILT_IN
        .iter()
        .map(|(id, _, _, command)| (id.to_string(), command.to_string()))
        .collect();
    if let Ok(customs) = crate::agents::list_custom(store) {
        agents.extend(customs.into_iter().map(|c| (c.id, c.command)));
    }
    agents
}

/// The agents running in any terminal right now (each once).
pub fn running_agents(s: &Services) -> Vec<String> {
    let procs = crate::agents::running::snapshot();
    let agents = known_agents(&s.store);
    let mut found: Vec<String> = s
        .ptys
        .pids()
        .into_iter()
        .filter_map(|pid| crate::agents::running::agent_under(pid, &procs, &agents))
        .collect();
    found.sort();
    found.dedup();
    found
}

/// The owner key for a project's terminals: removing the project closes them.
pub fn project_owner(project_id: &str) -> String {
    format!("project:{project_id}")
}

/// Where a terminal opens and who owns it: a project's folder (owner `project:<id>`), else
/// your home folder (owner `home`), a new session's shell before you've added any folder.
fn terminal_target(
    store: &crate::store::Store,
    project_id: Option<&str>,
) -> AppResult<(String, std::path::PathBuf)> {
    match project_id {
        Some(p) => Ok((
            project_owner(p),
            std::path::PathBuf::from(crate::projects::get(store, p)?.path),
        )),
        None => Ok((
            "home".into(),
            dirs::home_dir().ok_or_else(|| AppError::Invalid("no home folder".into()))?,
        )),
    }
}

pub fn register(r: &mut Registry<Services>) {
    r.add(
        Def::new(
            "shell.list",
            "The shells a plain terminal can run on this PC (PowerShell, Command Prompt, Git Bash, WSL distributions)",
            Risk::Read,
        ),
        |_call, _: super::registry::NoInput| -> AppResult<Vec<crate::shells::Shell>> {
            Ok(crate::shells::list())
        },
    );
    r.add(
        Def::new(
            "folder.open",
            "Open a folder's project in File Explorer",
            Risk::Read,
        )
        .human_only(),
        |call, i: FolderInput| -> AppResult<()> {
            let p = crate::projects::get(&call.ctx.store, &i.project_id)?;
            tauri_plugin_opener::open_path(&p.path, None::<&str>)
                .map_err(|e| AppError::Invalid(e.to_string()))
        },
    );
    r.add(
        Def::new(
            "pty.open",
            "Open a terminal at a project's root, or in your home folder",
            Risk::WriteLocal,
        ),
        |call, i: OpenInput| -> AppResult<Opened> {
            let s = call.ctx;
            let (owner, cwd) = terminal_target(&s.store, i.project_id.as_deref())?;
            let cwd = match i.cwd.map(std::path::PathBuf::from) {
                Some(dir) if dir.is_dir() => dir,
                _ => cwd,
            };
            let (cols, rows) = (i.cols.unwrap_or(120), i.rows.unwrap_or(32));
            // Only the human's UI has xterm.js listening from the first byte.
            let human = call.actor.kind == super::registry::ActorKind::Human;
            // The pane's own shell, else the one picked in Settings → General, else the usual
            // (shell.list's first: PowerShell 7, Windows PowerShell, Command Prompt).
            let shell = crate::shells::chosen(i.shell.as_deref())
                .or_else(|| {
                    crate::settings::get(&s.store)
                        .ok()
                        .and_then(|x| crate::shells::chosen(x.shell.as_deref()))
                })
                .or_else(|| crate::shells::list().into_iter().next());
            let ran = shell.as_ref().filter(|_| human).map(|sh| sh.id.clone());
            let spawn = match (human, shell) {
                (true, Some(sh)) => crate::pty::Spawn::new(&sh.program, &cwd).args(sh.args),
                _ => crate::pty::Spawn::new(crate::pty::default_shell(), &cwd),
            };
            let pty_id = s
                .ptys
                .spawn(&owner, i.key.as_deref(), &spawn, cols, rows, !human)?
                .pty_id;
            let mut entry = json!({
                "id": pty_id, "owner": owner, "cols": cols, "rows": rows,
                "status": "open", "openedBy": call.actor,
            });
            // A project's terminal names its project.
            if let Some(p) = owner.strip_prefix("project:") {
                entry["projectId"] = json!(p);
            }
            let id = pty_id.clone();
            s.state.commit(cause(call, "pty.open"), move |d| {
                d["terminals"][&id] = entry
            });
            Ok(Opened { pty_id, shell: ran })
        },
    );
    r.add(
        Def::new(
            "pty.attach",
            "A pane's terminal still running from before (Gond Code restarted, updated or crashed), with what it last printed",
            Risk::Read,
        ),
        |call, i: AttachInput| -> AppResult<Option<crate::pty::Attached>> {
            let found = call.ctx.ptys.attach(&i.key)?;
            if let Some(a) = found.as_ref().filter(|a| !a.exited) {
                let id = a.pty_id.clone();
                let entry = json!({ "id": id, "status": "open", "key": i.key });
                call.ctx.state.commit(cause(call, "pty.attach"), move |d| {
                    d["terminals"][&id] = entry
                });
            }
            Ok(found)
        },
    );
    r.add(
        Def::new(
            "pty.prune",
            "Close the terminals of panes that no longer exist",
            Risk::WriteLocal,
        ),
        |call, i: PruneInput| -> AppResult<usize> { Ok(call.ctx.ptys.prune(&i.keep)) },
    );
    r.add(
        Def::new(
            "clipboard.read",
            "The clipboard's text, and whether it holds a picture, for a paste into a pane",
            Risk::Read,
        )
        .human_only(),
        |_, _: super::registry::NoInput| -> AppResult<crate::clipboard::Clip> {
            Ok(crate::clipboard::read())
        },
    );
    r.add(
        Def::new(
            "pty.agent",
            "The agent running in a terminal (one you typed into its shell), if any",
            Risk::Read,
        ),
        |call, i: PtyInput| -> AppResult<Option<String>> {
            let Some(pid) = call.ctx.ptys.pid(&i.pty_id) else {
                return Ok(None);
            };
            let procs = crate::agents::running::snapshot();
            Ok(crate::agents::running::agent_under(
                pid,
                &procs,
                &known_agents(&call.ctx.store),
            ))
        },
    );
    r.add(
        Def::new("pty.write", "Type into a terminal", Risk::WriteLocal),
        |call, i: WriteInput| -> AppResult<()> { call.ctx.ptys.write(&i.pty_id, &i.data) },
    );
    r.add(
        Def::new("pty.resize", "Resize a terminal", Risk::WriteLocal),
        |call, i: ResizeInput| -> AppResult<()> {
            call.ctx.ptys.resize(&i.pty_id, i.cols, i.rows)?;
            let (id, cols, rows) = (i.pty_id, i.cols, i.rows);
            call.ctx.state.commit(cause(call, "pty.resize"), move |d| {
                if d["terminals"][&id].is_object() {
                    d["terminals"][&id]["cols"] = json!(cols);
                    d["terminals"][&id]["rows"] = json!(rows);
                }
            });
            Ok(())
        },
    );
    r.add(
        Def::new("pty.close", "Close a terminal", Risk::WriteLocal),
        |call, i: PtyInput| -> AppResult<()> {
            call.ctx.ptys.close(&i.pty_id)?;
            let id = i.pty_id;
            call.ctx.state.commit(cause(call, "pty.close"), move |d| {
                if let Some(m) = d["terminals"].as_object_mut() {
                    m.remove(&id);
                }
            });
            Ok(())
        },
    );
    r.add(
        Def::new(
            "pty.read",
            "A terminal's output as plain text, optionally waiting for a match",
            Risk::Read,
        ),
        |call, i: ReadInput| -> AppResult<PtyRead> {
            let timeout = i.timeout_ms.map(Duration::from_millis);
            call.ctx
                .ptys
                .read(&i.pty_id, i.since, i.wait_for.as_deref(), timeout)
        },
    );
}

#[cfg(all(test, unix))]
mod tests {
    use crate::api::{registry, Actor, Services};
    use crate::git::tests::temp_repo;
    use serde_json::json;

    #[test]
    fn open_write_read_close_through_the_registry() {
        let (r, s, h) = (registry(), Services::default(), Actor::human());
        let repo = temp_repo();
        let p = r
            .run(&s, &h, "project.add", json!({ "path": repo.path() }))
            .unwrap();
        let o = r
            .run(
                &s,
                &h,
                "pty.open",
                json!({ "projectId": p["id"], "cols": 80, "rows": 24 }),
            )
            .unwrap();
        let id = o["ptyId"].as_str().unwrap().to_string();
        assert_eq!(s.state.get(&format!("/terminals/{id}/cols")).unwrap().1, 80);

        r.run(
            &s,
            &h,
            "pty.write",
            json!({ "ptyId": id, "data": "echo gond-$((6*7))\n" }),
        )
        .unwrap();
        let out = r
            .run(
                &s,
                &h,
                "pty.read",
                json!({ "ptyId": id, "waitFor": "gond-42", "timeoutMs": 10000 }),
            )
            .unwrap();
        assert_eq!(out["matched"], true, "{out}");
        assert!(out["offset"].as_u64().unwrap() > 0);

        r.run(
            &s,
            &h,
            "pty.resize",
            json!({ "ptyId": id, "cols": 100, "rows": 30 }),
        )
        .unwrap();
        assert_eq!(
            s.state.get(&format!("/terminals/{id}/cols")).unwrap().1,
            100
        );
        r.run(&s, &h, "pty.close", json!({ "ptyId": id })).unwrap();
        assert!(s.state.get(&format!("/terminals/{id}")).is_none());

        // Without a project, it opens in your home folder (an empty session's
        // terminal).
        let home = r.run(&s, &h, "pty.open", json!({})).unwrap();
        let home = home["ptyId"].as_str().unwrap().to_string();
        assert!(s.state.get(&format!("/terminals/{home}")).is_some());
        r.run(&s, &h, "pty.close", json!({ "ptyId": home }))
            .unwrap();
        assert_eq!(
            r.run(&s, &h, "pty.read", json!({ "ptyId": "nope" }))
                .unwrap_err()
                .code(),
            "not_found"
        );
    }
}

#[cfg(test)]
mod terminal {
    use crate::api::{registry, Actor, Services};
    use crate::git::tests::temp_repo;
    use serde_json::json;

    /// A project's terminal runs a shell in its folder: what you type there runs, and its
    /// output reads back.
    #[test]
    fn a_projects_terminal_shows_output() {
        let (r, s, h) = (registry(), Services::default(), Actor::human());
        let repo = temp_repo();
        let p = r
            .run(&s, &h, "project.add", json!({ "path": repo.path() }))
            .unwrap();
        let o = r
            .run(&s, &h, "pty.open", json!({ "projectId": p["id"] }))
            .unwrap();
        let id = o["ptyId"].as_str().unwrap().to_string();
        // ConPTY asks where the cursor is and waits; the app's terminal answers, so do we.
        for _ in 0..3 {
            std::thread::sleep(std::time::Duration::from_millis(400));
            let _ = r.run(
                &s,
                &h,
                "pty.write",
                json!({ "ptyId": id, "data": "[1;1R" }),
            );
        }
        // Computed, so a match can't be the typed command echoing back.
        let line = if cfg!(windows) {
            if crate::pty::default_shell()
                .to_string_lossy()
                .to_lowercase()
                .contains("cmd")
            {
                "for /f %i in ('set /a 6*7') do @echo agent-%i\r"
            } else {
                "Write-Output ('agent-' + (6*7))\r"
            }
        } else {
            "echo agent-$((6*7))\n"
        };
        let read = |wait: &str, since: u64, ms: u64| {
            r.run(
                &s,
                &h,
                "pty.read",
                json!({ "ptyId": id, "since": since, "waitFor": wait, "timeoutMs": ms }),
            )
            .unwrap()
        };
        // Under load the shell may still be starting; keys typed before its prompt can
        // be dropped. So wait for the prompt (any `>` or `$`), then type, and once more
        // if nothing came back.
        let prompt = read(r"[>$#]\s*$", 0, 60_000);
        let mut since = prompt["offset"].as_u64().unwrap_or(0);
        let mut out = serde_json::Value::Null;
        for _ in 0..2 {
            r.run(&s, &h, "pty.write", json!({ "ptyId": id, "data": line }))
                .unwrap();
            out = read("agent-42", since, 60_000);
            if out["matched"] == true {
                break;
            }
            since = out["offset"].as_u64().unwrap_or(since);
        }
        assert_eq!(out["matched"], true, "{out}");
        // A full-colour terminal: the shell (and any agent in it) is told so.
        let colour = if cfg!(windows) {
            if crate::pty::default_shell()
                .to_string_lossy()
                .to_lowercase()
                .contains("cmd")
            {
                "echo colour-%COLORTERM%\r"
            } else {
                "Write-Output ('colour-' + $env:COLORTERM)\r"
            }
        } else {
            "echo colour-$COLORTERM\n"
        };
        let since = out["offset"].as_u64().unwrap_or(0);
        r.run(&s, &h, "pty.write", json!({ "ptyId": id, "data": colour }))
            .unwrap();
        let out = read("colour-truecolor", since, 60_000);
        assert_eq!(out["matched"], true, "{out}");
        r.run(&s, &h, "pty.close", json!({ "ptyId": id })).unwrap();
    }
}
