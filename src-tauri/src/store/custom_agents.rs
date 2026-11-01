use rusqlite::{params, Connection};

use crate::agents::{CustomAgent, PromptVia};
use crate::error::AppResult;

pub fn list(c: &Connection) -> AppResult<Vec<CustomAgent>> {
    let mut stmt = c.prepare(
        "SELECT id, name, command, args_json, provider, prompt_via FROM custom_agents ORDER BY name",
    )?;
    let rows = stmt.query_map([], |r| {
        let args: String = r.get(3)?;
        Ok(CustomAgent {
            id: r.get(0)?,
            name: r.get(1)?,
            command: r.get(2)?,
            args: serde_json::from_str(&args).unwrap_or_default(),
            provider: r.get(4)?,
            prompt_via: match r.get::<_, String>(5)?.as_str() {
                "stdin" => PromptVia::Stdin,
                _ => PromptVia::Arg,
            },
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Inserts or replaces by id.
pub fn upsert(c: &Connection, a: &CustomAgent) -> AppResult<()> {
    c.execute(
        "INSERT INTO custom_agents (id, name, command, args_json, provider, prompt_via)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, command = excluded.command,
           args_json = excluded.args_json, provider = excluded.provider,
           prompt_via = excluded.prompt_via",
        params![
            a.id,
            a.name,
            a.command,
            serde_json::to_string(&a.args).unwrap(),
            a.provider,
            a.prompt_via.as_str()
        ],
    )?;
    Ok(())
}

/// Returns whether a row was deleted.
pub fn delete(c: &Connection, id: &str) -> AppResult<bool> {
    Ok(c.execute("DELETE FROM custom_agents WHERE id = ?1", [id])? > 0)
}
