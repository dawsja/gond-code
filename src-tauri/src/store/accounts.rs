use rusqlite::{params, Connection, Row};

use crate::accounts::{Account, AccountKind};
use crate::error::AppResult;
use crate::vault::SecretRef;

const COLS: &str =
    "id, harness, provider, label, kind, secret_ref, config_dir, endpoint, created_at, options, identity, is_default";

fn row(r: &Row) -> rusqlite::Result<Account> {
    let mut a = Account {
        id: r.get(0)?,
        harness: r.get(1)?,
        provider: r.get(2)?,
        label: r.get(3)?,
        kind: AccountKind::parse(&r.get::<_, String>(4)?),
        secret_ref: r.get::<_, Option<String>>(5)?.map(SecretRef),
        config_dir: r.get(6)?,
        endpoint: r.get(7)?,
        created_at: r.get(8)?,
        options: serde_json::from_str(&r.get::<_, String>(9)?).unwrap_or_default(),
        identity: r.get(10)?,
        is_default: r.get::<_, i64>(11)? != 0,
        own: false,
    };
    a.own = a.is_own();
    Ok(a)
}

pub fn list(c: &Connection) -> AppResult<Vec<Account>> {
    let mut stmt = c.prepare(&format!(
        "SELECT {COLS} FROM accounts ORDER BY harness, created_at"
    ))?;
    let rows = stmt.query_map([], row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn get(c: &Connection, id: &str) -> AppResult<Option<Account>> {
    let mut stmt = c.prepare(&format!("SELECT {COLS} FROM accounts WHERE id = ?1"))?;
    let mut rows = stmt.query_map([id], row)?;
    Ok(rows.next().transpose()?)
}

pub fn insert(c: &Connection, a: &Account) -> AppResult<()> {
    c.execute(
        &format!("INSERT INTO accounts ({COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)"),
        params![
            a.id,
            a.harness,
            a.provider,
            a.label,
            a.kind.as_str(),
            a.secret_ref.as_ref().map(|r| &r.0),
            a.config_dir,
            a.endpoint,
            a.created_at,
            serde_json::to_string(&a.options).unwrap_or_else(|_| "{}".into()),
            a.identity,
            a.is_default as i64
        ],
    )?;
    Ok(())
}

pub fn set_label(c: &Connection, id: &str, label: &str) -> AppResult<()> {
    c.execute(
        "UPDATE accounts SET label = ?2 WHERE id = ?1",
        params![id, label],
    )?;
    Ok(())
}

/// Makes `id` the only default among its harness's accounts.
pub fn make_default(c: &Connection, id: &str) -> AppResult<()> {
    c.execute(
        "UPDATE accounts SET is_default = (id = ?1) WHERE harness = (SELECT harness FROM accounts WHERE id = ?1)",
        params![id],
    )?;
    Ok(())
}

pub fn set_identity(c: &Connection, id: &str, identity: Option<&str>) -> AppResult<()> {
    c.execute(
        "UPDATE accounts SET identity = ?2 WHERE id = ?1",
        params![id, identity],
    )?;
    Ok(())
}

pub fn set_config_dir(c: &Connection, id: &str, dir: Option<&str>) -> AppResult<()> {
    c.execute(
        "UPDATE accounts SET config_dir = ?2 WHERE id = ?1",
        params![id, dir],
    )?;
    Ok(())
}

/// Moves every thread on account `from` to account `to` (merging duplicates).
pub fn repoint_threads(c: &Connection, from: &str, to: &str) -> AppResult<usize> {
    Ok(c.execute(
        "UPDATE threads SET account_id = ?2 WHERE account_id = ?1",
        params![from, to],
    )?)
}

/// Returns whether a row was deleted.
pub fn delete(c: &Connection, id: &str) -> AppResult<bool> {
    Ok(c.execute("DELETE FROM accounts WHERE id = ?1", [id])? > 0)
}
