//! Local state in SQLite: `%APPDATA%\GondCode\gond-code.db` on Windows,
//! `$XDG_DATA_HOME/gond-code/gond-code.db` on Linux.

pub mod custom_agents;
pub mod projects;
pub mod settings;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::error::AppResult;

/// Numbered migrations, applied in order. Never edit one that has shipped; add a new one.
const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("migrations/0001_init.sql")),
    (2, include_str!("migrations/0002_drop_theme.sql")),
    (3, include_str!("migrations/0003_custom_agent_provider.sql")),
    (
        4,
        include_str!("migrations/0004_custom_agent_prompt_via.sql"),
    ),
    (5, include_str!("migrations/0005_accounts.sql")),
    (6, include_str!("migrations/0006_thread_account.sql")),
    (7, include_str!("migrations/0007_thread_model.sql")),
    (8, include_str!("migrations/0008_events.sql")),
    // Versions may land out of order (0008 after 0009); see `migrate`.
    (9, include_str!("migrations/0009_tasks.sql")),
    (10, include_str!("migrations/0010_thread_handle_plan.sql")),
    (11, include_str!("migrations/0011_account_options.sql")),
    (12, include_str!("migrations/0012_receipts.sql")),
    (13, include_str!("migrations/0013_loops.sql")),
    (14, include_str!("migrations/0014_mcp_servers.sql")),
    (15, include_str!("migrations/0015_thread_parent.sql")),
    (16, include_str!("migrations/0016_thread_group.sql")),
    (17, include_str!("migrations/0017_account_identity.sql")),
    (18, include_str!("migrations/0018_drop_mcp_setting.sql")),
    (19, include_str!("migrations/0019_drop_egress.sql")),
    (
        20,
        include_str!("migrations/0020_thread_permission_mode.sql"),
    ),
];

/// Cheap to clone: clones share one connection.
#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,
}

impl Store {
    pub fn open(path: &Path) -> AppResult<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> AppResult<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> AppResult<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrate(&mut conn)?;
        Ok(Store {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// The folder the database is in; `None` for an in-memory store.
    pub fn dir(&self) -> Option<PathBuf> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.path()
            .filter(|p| !p.is_empty())
            .and_then(|p| Path::new(p).parent().map(Path::to_path_buf))
    }

    /// Run `f` with the connection held.
    pub fn with<T>(&self, f: impl FnOnce(&Connection) -> AppResult<T>) -> AppResult<T> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        f(&conn)
    }
}

pub fn default_db_path() -> Option<PathBuf> {
    crate::appdata::roaming().map(|d| d.join("gond-code.db"))
}

/// Applies every migration newer than the recorded version, in one transaction.
fn migrate(conn: &mut Connection) -> AppResult<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL)")?;
    // Every version not applied yet, not just those above the max: numbers are
    // reserved per PR and can land out of order (0009 before 0008).
    let applied: std::collections::HashSet<i64> = {
        let mut stmt = conn.prepare("SELECT version FROM schema_version")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<Result<_, _>>()?
    };
    let tx = conn.transaction()?;
    for (version, sql) in MIGRATIONS.iter().filter(|(v, _)| !applied.contains(v)) {
        tx.execute_batch(sql)?;
        tx.execute(
            "INSERT INTO schema_version (version) VALUES (?1)",
            [version],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Current time as an RFC 3339 UTC string.
pub fn now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .expect("RFC 3339 formatting never fails for UTC")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(store: &Store) -> i64 {
        store
            .with(|c| Ok(c.query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))?))
            .unwrap()
    }

    #[test]
    fn migrates_fresh_db() {
        let store = Store::open_in_memory().unwrap();
        assert_eq!(version(&store), MIGRATIONS.last().unwrap().0);
    }

    #[test]
    fn a_lower_migration_landing_later_still_runs() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        // Pretend an older build applied everything but 7.
        conn.execute("DELETE FROM schema_version WHERE version = 7", [])
            .unwrap();
        conn.execute_batch(
            "ALTER TABLE threads DROP COLUMN model; ALTER TABLE threads DROP COLUMN thinking;",
        )
        .unwrap();
        migrate(&mut conn).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM schema_version WHERE version = 7",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
        conn.execute_batch("SELECT model, thinking FROM threads")
            .unwrap();
    }

    /// The egress allowlist is gone: its host lists go, and the
    /// stored policy keeps everything but its `egress` section.
    #[test]
    fn migration_drops_the_egress_allowlist() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        conn.execute("DELETE FROM schema_version WHERE version = 19", [])
            .unwrap();
        conn.execute_batch(
            r#"INSERT INTO settings (key, value_json) VALUES
                 ('egress.threads', '{"T1":["example.com"]}'),
                 ('egress.projects', '{"P1":["*.pkg.dev"]}'),
                 ('policy', '{"byRisk":{"read":"auto"},"egress":{"allow":["x.com"],"devDefaults":true}}');"#,
        )
        .unwrap();
        migrate(&mut conn).unwrap();
        let keys: Vec<String> = conn
            .prepare("SELECT key FROM settings ORDER BY key")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(keys, ["policy"]);
        let policy: String = conn
            .query_row(
                "SELECT value_json FROM settings WHERE key = 'policy'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(policy, r#"{"byRisk":{"read":"auto"}}"#);
    }

    #[test]
    fn reopening_existing_db_is_a_no_op() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("gond-code.db");
        drop(Store::open(&path).unwrap());
        let store = Store::open(&path).unwrap();
        assert_eq!(version(&store), MIGRATIONS.last().unwrap().0);
        let rows: i64 = store
            .with(|c| Ok(c.query_row("SELECT COUNT(*) FROM schema_version", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(rows, MIGRATIONS.len() as i64);
        let mode: String = store
            .with(|c| Ok(c.query_row("PRAGMA journal_mode", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(mode, "wal");
    }

    #[test]
    fn now_is_rfc3339_utc() {
        let t = now();
        assert!(t.ends_with('Z') && t.contains('T'), "{t}");
    }
}
