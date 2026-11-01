//! `project.*` and `settings.*` behind the registry. Handlers call the same
//! plain functions the old `#[tauri::command]`s did, then mirror the result into
//! the state tree (`projects`, `settings`).

use std::collections::BTreeMap;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Deserialize;

use super::registry::{Call, Def, NoInput, Registry, Risk};
use super::state::Cause;
use super::Services;
use crate::error::AppResult;
use crate::settings::{self, Settings};
use crate::store::projects::Project;
use crate::{projects, store::Store};

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PathInput {
    /// Absolute path of a git repository.
    pub path: PathBuf,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FolderPathInput {
    /// Absolute path of any folder.
    pub path: PathBuf,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectInput {
    pub project_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsPatch {
    /// Partial `Settings`: only its own keys, typed.
    pub patch: crate::settings::Patch,
}

fn cause(call: &Call<Services>, command: &str) -> Cause {
    Cause::Command {
        event_id: None,
        command: command.into(),
        actor: call.actor.clone(),
    }
}

/// Rewrites `/projects` from the store. Projects are few, so a full refresh is
/// simplest; the tree's diff keeps the patch minimal.
/// The settings key holding closed projects' ids (closed projects stay, but leave the
/// switcher). No migration needed.
const CLOSED: &str = "closedProjects";

fn closed(store: &Store) -> AppResult<Vec<String>> {
    let all = store.with(crate::store::settings::all)?;
    Ok(all
        .into_iter()
        .find(|(k, _)| k == CLOSED)
        .and_then(|(_, v)| serde_json::from_value(v).ok())
        .unwrap_or_default())
}

fn set_closed(store: &Store, id: &str, is_closed: bool) -> AppResult<()> {
    let mut ids = closed(store)?;
    ids.retain(|x| x != id);
    if is_closed {
        ids.push(id.to_string());
    }
    let v = serde_json::to_value(ids).unwrap_or_default();
    store.with(|c| crate::store::settings::set(c, CLOSED, &v))
}

/// A plain folder rather than a Git repo.
pub fn is_folder(path: &std::path::Path) -> bool {
    crate::git::repo_root(path).is_err()
}

pub fn sync_projects(services: &Services, cause: Cause) -> AppResult<()> {
    let list = projects::list(&services.store)?;
    let closed = closed(&services.store)?;
    let map: BTreeMap<String, serde_json::Value> = list
        .into_iter()
        .map(|p| {
            let mut v = serde_json::to_value(&p).unwrap_or_default();
            // a plain folder or a Git repo.
            let folder = is_folder(std::path::Path::new(&p.path));
            v["vcs"] = if folder { "folder" } else { "git" }.into();
            v["open"] = (!closed.contains(&p.id)).into();
            (p.id, v)
        })
        .collect();
    let value = serde_json::to_value(map).unwrap_or_default();
    services.state.commit(cause, |d| d["projects"] = value);
    Ok(())
}

pub fn sync_settings(services: &Services, cause: Cause) -> AppResult<()> {
    let value = serde_json::to_value(settings::get(&services.store)?).unwrap_or_default();
    services.state.commit(cause, |d| d["settings"] = value);
    Ok(())
}

/// Fills the tree's `projects` and `settings` at startup.
pub fn load(services: &Services) -> AppResult<()> {
    let boot = || Cause::Runtime {
        runtime: "startup".into(),
    };
    sync_projects(services, boot())?;
    sync_settings(services, boot())
}

fn store<'a>(call: &'a Call<Services>) -> &'a Store {
    &call.ctx.store
}

pub fn register(r: &mut Registry<Services>) {
    r.add(
        Def::new(
            "project.add",
            "Add a git repository as a project",
            Risk::WriteLocal,
        ),
        |call, i: PathInput| -> AppResult<Project> {
            let p = projects::add(store(call), &i.path)?;
            sync_projects(call.ctx, cause(call, "project.add"))?;
            Ok(p)
        },
    );
    r.add(
        Def::new(
            "project.list",
            "Projects, most recently opened first",
            Risk::Read,
        ),
        |call, _: NoInput| projects::list(store(call)),
    );
    r.add(
        Def::new(
            "folder.git",
            "The git repository and branch a folder is in, if any (the status bar)",
            Risk::Read,
        ),
        |_call, i: FolderPathInput| -> AppResult<crate::git::Place> {
            Ok(crate::git::place(&i.path))
        },
    );
    r.add(
        Def::new(
            "project.touch",
            "Mark a project as just opened",
            Risk::WriteLocal,
        ),
        |call, i: ProjectInput| -> AppResult<()> {
            projects::touch(store(call), &i.project_id)?;
            set_closed(store(call), &i.project_id, false)?;
            sync_projects(call.ctx, cause(call, "project.touch"))?;
            Ok(())
        },
    );
    r.add(
        Def::new(
            "project.remove",
            "Remove a project from Gond (its repo stays on disk); its terminals close",
            Risk::WriteLocal,
        ),
        |call, i: ProjectInput| -> AppResult<()> {
            let s = call.ctx;
            projects::remove(&s.store, &i.project_id)?;
            s.ptys
                .close_thread(&super::pty::project_owner(&i.project_id));
            set_closed(&s.store, &i.project_id, false)?;
            sync_projects(s, cause(call, "project.remove"))
        },
    );
    r.add(
        Def::new("settings.get", "The app's settings", Risk::Read),
        |call, _: NoInput| settings::get(store(call)),
    );
    r.add(
        Def::new(
            "settings.set",
            "Change settings (partial patch)",
            Risk::WriteLocal,
        ),
        |call, i: SettingsPatch| -> AppResult<Settings> {
            let s = settings::set(store(call), i.patch.into_map())?;
            sync_settings(call.ctx, cause(call, "settings.set"))?;
            Ok(s)
        },
    );
}

#[cfg(test)]
mod tests {
    use crate::api::{registry, Actor, Services};
    use serde_json::json;
    use std::process::Command;

    fn repo() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            assert!(Command::new("git")
                .args(args)
                .current_dir(d.path())
                .status()
                .unwrap()
                .success());
        };
        git(&["init", "-q", "-b", "main"]);
        git(&[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "init",
        ]);
        d
    }

    #[test]
    fn project_add_list_and_tree() {
        let (r, s, h) = (registry(), Services::default(), Actor::human());
        let d = repo();
        let p = r
            .run(&s, &h, "project.add", json!({ "path": d.path() }))
            .unwrap();
        let id = p["id"].as_str().unwrap();
        let list = r.run(&s, &h, "project.list", json!({})).unwrap();
        assert_eq!(list[0]["id"], id);
        let (_, tree) = s.state.get(&format!("/projects/{id}/name")).unwrap();
        assert_eq!(tree, p["name"]);
        let e = r
            .run(
                &s,
                &h,
                "project.add",
                json!({ "path": "/definitely/not/a/repo" }),
            )
            .unwrap_err();
        assert!(
            matches!(e.code(), "not_a_git_repo" | "invalid" | "not_found"),
            "{}",
            e.code()
        );
    }

    #[test]
    fn settings_round_trip_into_the_tree() {
        let (r, s, h) = (registry(), Services::default(), Actor::human());
        let v = r
            .run(
                &s,
                &h,
                "settings.set",
                json!({ "patch": { "tint": "green" } }),
            )
            .unwrap();
        assert_eq!(v["tint"], "green");
        assert_eq!(s.state.get("/settings/tint").unwrap().1, "green");
        assert_eq!(
            r.run(&s, &h, "settings.get", json!({})).unwrap()["tint"],
            "green"
        );
    }
}
