//! `pty.*` behind the registry: terminals in a thread's worktree or at a
//! project's root, and `pty.read` so an agent can run a command
//! and read its result. Raw output still streams to the UI as `pty:data`; the
//! tree keeps one entry per open terminal at `/terminals/{ptyId}`.

use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::registry::{Call, Def, Registry, Risk};
use super::state::Cause;
use super::Services;
use crate::agents::sessions;
use crate::error::{AppError, AppResult};
use crate::pty::PtyRead;
use crate::threads;
use std::path::Path;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenInput {
    /// Open in this thread's folder...
    pub thread_id: Option<String>,
    /// ...or this project's folder; with neither, your home folder.
    pub project_id: Option<String>,
    /// A shell.list id (`pwsh`, `cmd`, `wsl:Ubuntu`…), so a pane keeps the shell it had; none
    /// (or one no longer on this PC) = the one picked in Settings → General.
    pub shell: Option<String>,
    /// Start in this folder instead (a shell coming back after an update, where it was).
    pub cwd: Option<String>,
    /// Default 120.
    pub cols: Option<u16>,
    /// Default 32.
    pub rows: Option<u16>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionInput {
    pub thread_id: String,
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

/// Claude Code's folder for a working folder's sessions: every character that isn't a letter
/// or a digit becomes `-` (`C:\code\cura` → `C--code-cura`).
fn claude_folder(cwd: &std::path::Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Where an agent keeps its sessions: the home an account's sign-in gives it (`var` in its
/// env), else the CLI's own `~/<default>`.
fn env_dir(
    it: &crate::runner::Interactive,
    var: &str,
    default: &str,
) -> Option<std::path::PathBuf> {
    it.env
        .iter()
        .find(|(k, _)| k == var)
        .map(|(_, v)| std::path::PathBuf::from(v))
        .or_else(|| std::env::var_os(var).map(std::path::PathBuf::from))
        .or_else(|| dirs::home_dir().map(|h| h.join(default)))
}

/// A random UUID (v4 layout) for a new session (`--session-id` wants one).
fn session_uuid() -> String {
    let n = ulid::Ulid::generate().0 ^ (std::process::id() as u128).rotate_left(64);
    let n = (n & !(0xf << 76) | (0x4 << 76)) & !(0x3 << 62) | (0x2 << 62);
    let h = format!("{n:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
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

fn session_command(it: crate::runner::Interactive) -> AppResult<portable_pty::CommandBuilder> {
    let path = crate::process::fresh_path();
    let program = crate::agents::detect::find(&it.program, path.clone()).ok_or_else(|| {
        AppError::AgentNotFound(format!("{} isn't installed or isn't on PATH", it.program))
    })?;
    let shim = program
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"));
    let mut cmd = if shim {
        let mut c = portable_pty::CommandBuilder::new("cmd.exe");
        c.args(["/d", "/c"]);
        c.arg(&program);
        c
    } else {
        portable_pty::CommandBuilder::new(&program)
    };
    cmd.args(&it.args);
    cmd.cwd(&it.cwd);
    cmd.env("TERM", "xterm-256color");
    if let Some(p) = &path {
        cmd.env("PATH", p);
    }
    for var in &it.remove {
        cmd.env_remove(var);
    }
    for (k, v) in &it.env {
        cmd.env(k, v);
    }
    Ok(cmd)
}

pub fn register(r: &mut Registry<Services>) {
    r.add(
        Def::new(
            "pane.start",
            "Start a pane's agent: its own interactive CLI on the thread's session (account, model, mode), in a terminal that runs until the pane closes",
            Risk::WriteLocal,
        )
        .human_only(),
        |call, i: SessionInput| -> AppResult<Opened> {
            let s = call.ctx;
            let mut thread = threads::get(&s.store, &i.thread_id)?;
            // A pane opening again (after a restart or an update), not for the first time.
            let again = time::OffsetDateTime::parse(
                &thread.created_at,
                &time::format_description::well_known::Rfc3339,
            )
            .is_ok_and(|t| time::OffsetDateTime::now_utc() - t > time::Duration::seconds(15));
            // Claude Code takes a session id up front, so the pane can resume it after a
            // restart; one it never wrote (closed before the first message) starts fresh again.
            let mut fresh = false;
            if thread.agent_id == "claude" {
                let written = thread.agent_session_id.as_ref().is_some_and(|id| {
                    s.runner.claude_config(&thread).is_some_and(|dir| {
                        dir.join("projects")
                            .join(claude_folder(std::path::Path::new(
                                &thread.worktree_path,
                            )))
                            .join(format!("{id}.jsonl"))
                            .is_file()
                    })
                });
                if !written {
                    let id = session_uuid();
                    s.store
                        .with(|c| crate::store::threads::set_session(c, &thread.id, &id))?;
                    thread.agent_session_id = Some(id);
                    fresh = true;
                }
            }
            if thread.agent_session_id.is_none() {
                let found = match thread.agent_id.as_str() {
                    // Pi, Grok Build and Copilot take an id Gond chooses too.
                    "pi" | "grok" | "copilot" => Some(session_uuid()),
                    // Codex and Antigravity name their own: find the one this pane started.
                    "codex" if again => {
                        let it = s.runner.interactive(&thread)?;
                        let home = env_dir(&it, "CODEX_HOME", ".codex");
                        let since = time::OffsetDateTime::parse(
                            &thread.created_at,
                            &time::format_description::well_known::Rfc3339,
                        )
                        .unwrap_or_else(|_| time::OffsetDateTime::now_utc());
                        let taken: Vec<String> = s
                            .store
                            .with(crate::store::threads::list_unarchived)?
                            .into_iter()
                            .filter(|t| t.agent_id == "codex" && t.id != thread.id)
                            .filter_map(|t| t.agent_session_id)
                            .collect();
                        home.and_then(|h| {
                            sessions::codex(&h, Path::new(&thread.worktree_path), since, &taken)
                        })
                    }
                    "agy" if again => dirs::home_dir().and_then(|h| {
                        sessions::agy(&h.join(".gemini"), Path::new(&thread.worktree_path))
                    }),
                    // Cursor makes the chat's id: ask for an empty one up front.
                    "cursor" => crate::runner::cursor_new_chat(&s.runner.interactive(&thread)?),
                    _ => None,
                };
                if let Some(id) = found {
                    s.store
                        .with(|c| crate::store::threads::set_session(c, &thread.id, &id))?;
                    thread.agent_session_id = Some(id);
                }
            }
            let mut it = s.runner.interactive(&thread)?;
            if fresh {
                if let Some(a) = it.args.iter_mut().find(|a| *a == "--resume") {
                    *a = "--session-id".into();
                }
            }
            // Grok resumes a session it has saved (-r) and creates one with that id (-s).
            if let (Some(id), Some(home)) = (
                thread.agent_session_id.clone(),
                env_dir(&it, "GROK_HOME", ".grok").filter(|_| thread.agent_id == "grok"),
            ) {
                if !sessions::grok_saved(&home, Path::new(&thread.worktree_path), &id) {
                    if let Some(a) = it.args.iter_mut().find(|a| *a == "-r") {
                        *a = "-s".into();
                    }
                }
            }
            // opencode can't take an id: opening again, it continues the folder's latest.
            if thread.agent_id == "opencode" && again {
                it.args.push("--continue".into());
            }
            let cmd = session_command(it)?;
            let (cols, rows) = (i.cols.unwrap_or(120), i.rows.unwrap_or(32));
            let pty_id = s.ptys.open_command(&thread.id, cmd, cols, rows)?;
            Ok(Opened {
                pty_id,
                shell: None,
            })
        },
    );
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
            "Open a terminal in a thread's worktree or at a project's root",
            Risk::WriteLocal,
        ),
        |call, i: OpenInput| -> AppResult<Opened> {
            let s = call.ctx;
            let (owner, cwd) = threads::terminal_target(
                &s.store,
                i.thread_id.as_deref(),
                i.project_id.as_deref(),
            )?;
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
            let pty_id = if let (true, Some(sh)) = (human, shell) {
                let mut cmd = portable_pty::CommandBuilder::new(&sh.program);
                cmd.args(&sh.args);
                cmd.cwd(&cwd);
                cmd.env("TERM", "xterm-256color");
                if let Some(path) = crate::process::fresh_path() {
                    cmd.env("PATH", path);
                }
                s.ptys.open_command(&owner, cmd, cols, rows)?
            } else if human {
                s.ptys.open(&owner, &cwd, cols, rows)?
            } else {
                s.ptys.open_headless(&owner, &cwd, cols, rows)?
            };
            let mut entry = json!({
                "id": pty_id, "owner": owner, "cols": cols, "rows": rows,
                "status": "open", "openedBy": call.actor,
            });
            // a thread's terminal names its thread; a project's, its project.
            match owner.strip_prefix("project:") {
                Some(p) => entry["projectId"] = json!(p),
                None if owner == "home" => {}
                None => entry["threadId"] = json!(owner),
            }
            let id = pty_id.clone();
            s.state.commit(cause(call, "pty.open"), move |d| {
                d["terminals"][&id] = entry
            });
            Ok(Opened { pty_id, shell: ran })
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

        // With neither a thread nor a project, it opens in your home folder (an empty session's
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
mod session_id {
    /// Claude Code refuses a `--session-id` that isn't a UUID: each pane's is a v4 one, and new.
    #[test]
    fn a_pane_session_id_is_a_fresh_uuid() {
        let (a, b) = (super::session_uuid(), super::session_uuid());
        assert_ne!(a, b);
        let parts: Vec<&str> = a.split('-').collect();
        assert_eq!(
            parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
            [8, 4, 4, 4, 12]
        );
        assert!(a.chars().all(|c| c == '-' || c.is_ascii_hexdigit()));
        assert!(parts[2].starts_with('4'), "version 4: {a}");
        assert!("89ab".contains(&parts[3][..1]), "RFC 4122 variant: {a}");
    }
}

#[cfg(test)]
mod terminal {
    use crate::api::{registry, Actor, Services};
    use crate::git::tests::temp_repo;
    use serde_json::json;

    /// A thread's terminal runs a shell in its folder: what you type there runs, and its
    /// output reads back.
    #[test]
    fn a_threads_terminal_shows_output() {
        let (r, s, h) = (registry(), Services::default(), Actor::human());
        let repo = temp_repo();
        let p = r
            .run(&s, &h, "project.add", json!({ "path": repo.path() }))
            .unwrap();
        let t = r
            .run(
                &s,
                &h,
                "thread.create",
                json!({ "projectId": p["id"], "agentId": "claude", "title": "Pty" }),
            )
            .unwrap();
        let tid = t["id"].as_str().unwrap().to_string();
        let o = r
            .run(&s, &h, "pty.open", json!({ "threadId": tid }))
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
