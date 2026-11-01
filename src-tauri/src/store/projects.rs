use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::error::AppResult;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    pub default_branch: String,
    pub added_at: String,
    pub last_opened_at: String,
}

const COLS: &str = "id, name, path, default_branch, added_at, last_opened_at";

fn from_row(r: &Row) -> rusqlite::Result<Project> {
    Ok(Project {
        id: r.get(0)?,
        name: r.get(1)?,
        path: r.get(2)?,
        default_branch: r.get(3)?,
        added_at: r.get(4)?,
        last_opened_at: r.get(5)?,
    })
}

pub fn insert(c: &Connection, p: &Project) -> AppResult<()> {
    c.execute(
        &format!("INSERT INTO projects ({COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6)"),
        params![
            p.id,
            p.name,
            p.path,
            p.default_branch,
            p.added_at,
            p.last_opened_at
        ],
    )?;
    Ok(())
}

/// Most recently opened first.
pub fn list(c: &Connection) -> AppResult<Vec<Project>> {
    let mut stmt = c.prepare(&format!(
        "SELECT {COLS} FROM projects ORDER BY last_opened_at DESC, added_at DESC"
    ))?;
    let rows = stmt.query_map([], from_row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn get(c: &Connection, id: &str) -> AppResult<Option<Project>> {
    let sql = format!("SELECT {COLS} FROM projects WHERE id = ?1");
    Ok(c.query_row(&sql, [id], from_row).optional()?)
}

pub fn get_by_path(c: &Connection, path: &str) -> AppResult<Option<Project>> {
    let sql = format!("SELECT {COLS} FROM projects WHERE path = ?1");
    Ok(c.query_row(&sql, [path], from_row).optional()?)
}

/// Returns whether a row was deleted.
pub fn delete(c: &Connection, id: &str) -> AppResult<bool> {
    Ok(c.execute("DELETE FROM projects WHERE id = ?1", [id])? > 0)
}

/// Sets last_opened_at. Returns whether the project exists.
pub fn touch(c: &Connection, id: &str, at: &str) -> AppResult<bool> {
    let n = c.execute(
        "UPDATE projects SET last_opened_at = ?2 WHERE id = ?1",
        params![id, at],
    )?;
    Ok(n > 0)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::store::Store;

    pub fn sample(id: &str, path: &str, opened: &str) -> Project {
        Project {
            id: id.into(),
            name: "repo".into(),
            path: path.into(),
            default_branch: "main".into(),
            added_at: "2026-10-29T00:00:00Z".into(),
            last_opened_at: opened.into(),
        }
    }

    #[test]
    fn crud_and_ordering() {
        let s = Store::open_in_memory().unwrap();
        s.with(|c| {
            insert(c, &sample("a", "/a", "2026-10-29T01:00:00Z"))?;
            insert(c, &sample("b", "/b", "2026-10-29T02:00:00Z"))?;
            let ids: Vec<_> = list(c)?.into_iter().map(|p| p.id).collect();
            assert_eq!(ids, ["b", "a"]);

            assert!(touch(c, "a", "2026-10-29T03:00:00Z")?);
            assert_eq!(list(c)?[0].id, "a");
            assert!(!touch(c, "missing", "2026-10-29T03:00:00Z")?);

            assert_eq!(get_by_path(c, "/b")?.unwrap().id, "b");
            assert!(get(c, "missing")?.is_none());

            assert!(delete(c, "a")?);
            assert!(!delete(c, "a")?);
            assert_eq!(list(c)?.len(), 1);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn path_is_unique() {
        let s = Store::open_in_memory().unwrap();
        s.with(|c| {
            insert(c, &sample("a", "/a", "2026-10-29T01:00:00Z"))?;
            assert!(insert(c, &sample("b", "/a", "2026-10-29T01:00:00Z")).is_err());
            Ok(())
        })
        .unwrap();
    }
}
