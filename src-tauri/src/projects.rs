//! Project logic behind the `project_*` commands.

use std::path::Path;

use crate::error::{AppError, AppResult};
use crate::git;
use crate::store::{now, projects, projects::Project, Store};

/// Adds a project: the git repo containing `path`, or the folder itself when it isn't in one.
/// Gond never writes to it; the agents in its panes work there as they would in a terminal.
/// Adding one that is already known just opens it again.
pub fn add(store: &Store, path: &Path) -> AppResult<Project> {
    let root = match git::repo_root(path) {
        Ok(root) => root,
        Err(AppError::NotAGitRepo(_)) if path.is_dir() => {
            std::fs::canonicalize(path).map(git::dunce_path)?
        }
        Err(e) => return Err(e),
    };
    let root_str = root.to_string_lossy().to_string();
    if let Some(existing) = store.with(|c| projects::get_by_path(c, &root_str))? {
        touch(store, &existing.id)?;
        return get(store, &existing.id);
    }
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root_str.clone());
    let at = now();
    let project = Project {
        id: ulid::Ulid::generate().to_string(),
        name,
        path: root_str,
        default_branch: git::default_branch(&root),
        added_at: at.clone(),
        last_opened_at: at,
    };
    store.with(|c| projects::insert(c, &project))?;
    Ok(project)
}

pub fn list(store: &Store) -> AppResult<Vec<Project>> {
    store.with(projects::list)
}

pub fn get(store: &Store, id: &str) -> AppResult<Project> {
    store
        .with(|c| projects::get(c, id))?
        .ok_or_else(|| AppError::NotFound(format!("project {id} not found")))
}

/// Forgets the project. Never touches its files.
pub fn remove(store: &Store, id: &str) -> AppResult<()> {
    match store.with(|c| projects::delete(c, id))? {
        true => Ok(()),
        false => Err(AppError::NotFound(format!("project {id} not found"))),
    }
}

pub fn touch(store: &Store, id: &str) -> AppResult<()> {
    match store.with(|c| projects::touch(c, id, &now()))? {
        true => Ok(()),
        false => Err(AppError::NotFound(format!("project {id} not found"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::tests::temp_repo;

    #[test]
    fn add_list_touch_remove() {
        let store = Store::open_in_memory().unwrap();
        let repo = temp_repo();
        let p = add(&store, repo.path()).unwrap();
        assert_eq!(p.default_branch, "main");
        assert_eq!(p.name, repo.path().file_name().unwrap().to_string_lossy());
        assert_eq!(p.id.len(), 26);

        // Re-adding (even from a subfolder) returns the same project.
        std::fs::create_dir(repo.path().join("src")).unwrap();
        assert_eq!(add(&store, &repo.path().join("src")).unwrap().id, p.id);
        assert_eq!(list(&store).unwrap().len(), 1);

        touch(&store, &p.id).unwrap();
        remove(&store, &p.id).unwrap();
        assert!(repo.path().exists());
        assert_eq!(remove(&store, &p.id).unwrap_err().code(), "not_found");
        assert_eq!(touch(&store, &p.id).unwrap_err().code(), "not_found");
    }

    #[test]
    fn a_plain_folder_is_a_project_and_a_missing_one_isnt() {
        let store = Store::open_in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let p = add(&store, dir.path()).unwrap();
        assert!(
            !dir.path().join(".git").exists(),
            "Gond never writes to a plain folder"
        );
        assert_eq!(
            add(&store, dir.path()).unwrap().id,
            p.id,
            "adding it again finds it"
        );
        assert_eq!(
            add(&store, &dir.path().join("nope")).unwrap_err().code(),
            "not_a_git_repo"
        );
        assert_eq!(list(&store).unwrap().len(), 1);
    }
}
