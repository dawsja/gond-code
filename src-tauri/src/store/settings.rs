//! Raw key → JSON value storage. The typed `Settings` and its defaults live with
//! the settings commands.

use rusqlite::{params, Connection};
use serde_json::Value;

use crate::error::AppResult;

pub fn all(c: &Connection) -> AppResult<Vec<(String, Value)>> {
    let mut stmt = c.prepare("SELECT key, value_json FROM settings ORDER BY key")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut out = Vec::new();
    for row in rows {
        let (key, json) = row?;
        // A value we can't parse is treated as unset rather than failing every read.
        if let Ok(v) = serde_json::from_str(&json) {
            out.push((key, v));
        }
    }
    Ok(out)
}

/// Stores `value` under `key`; `Value::Null` removes it.
pub fn set(c: &Connection, key: &str, value: &Value) -> AppResult<()> {
    if value.is_null() {
        c.execute("DELETE FROM settings WHERE key = ?1", [key])?;
    } else {
        c.execute(
            "INSERT INTO settings (key, value_json) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
            params![key, value.to_string()],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use serde_json::json;

    #[test]
    fn set_overwrite_and_clear() {
        let s = Store::open_in_memory().unwrap();
        s.with(|c| {
            set(c, "theme", &json!("dark"))?;
            set(c, "tint", &json!("blue"))?;
            set(c, "theme", &json!("light"))?;
            assert_eq!(
                all(c)?,
                vec![
                    ("theme".into(), json!("light")),
                    ("tint".into(), json!("blue"))
                ]
            );
            set(c, "tint", &Value::Null)?;
            assert_eq!(all(c)?.len(), 1);
            Ok(())
        })
        .unwrap();
    }
}
