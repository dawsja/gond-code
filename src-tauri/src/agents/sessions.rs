//! Where each agent CLI keeps its sessions, so a pane picks its conversation back up after a
//! restart. Claude Code, Pi and Grok Build take a session id Gond chooses up front; Codex and
//! Antigravity name theirs, so Gond finds the one a pane started from what they write to disk.

use std::path::{Path, PathBuf};

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// A folder as the CLIs record it, comparable whatever its case, separators or `\\?\` prefix.
fn same_folder(a: &str, b: &Path) -> bool {
    let norm = |s: &str| {
        s.trim_start_matches(r"\\?\")
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    };
    norm(a) == norm(&b.to_string_lossy())
}

/// The first line of a file, without reading the rest.
fn first_line(path: &Path) -> Option<String> {
    use std::io::BufRead;
    let f = std::fs::File::open(path).ok()?;
    let mut line = String::new();
    std::io::BufReader::new(f).read_line(&mut line).ok()?;
    Some(line)
}

/// The Codex session a pane started: the earliest interactive one in `cwd` that began after
/// `since` and no other pane has. Codex writes `<home>/sessions/YYYY/MM/DD/rollout-…-<id>.jsonl`,
/// whose first line is `{"type":"session_meta","payload":{"id","cwd","timestamp","originator"}}`,
/// once the session has something to keep (so a pane closed before its first message has none).
pub fn codex(home: &Path, cwd: &Path, since: OffsetDateTime, taken: &[String]) -> Option<String> {
    let earliest = since - time::Duration::seconds(10);
    let mut best: Option<(OffsetDateTime, String)> = None;
    // Day folders are local dates: look from the day before `since` on.
    let first_day = (since - time::Duration::days(1)).date();
    for year in dirs_in(&home.join("sessions")) {
        for month in dirs_in(&year) {
            for day in dirs_in(&month) {
                let num = |p: &Path| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .and_then(|n| n.parse::<i32>().ok())
                };
                let date = match (num(&year), num(&month), num(&day)) {
                    (Some(y), Some(m), Some(d)) => time::Month::try_from(m as u8)
                        .ok()
                        .and_then(|m| time::Date::from_calendar_date(y, m, d as u8).ok()),
                    _ => None,
                };
                let on_or_after = date.is_some_and(|d| d >= first_day);
                if !on_or_after {
                    continue;
                }
                for file in std::fs::read_dir(&day).into_iter().flatten().flatten() {
                    let path = file.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                        continue;
                    }
                    let Some(meta) = first_line(&path)
                        .and_then(|l| serde_json::from_str::<serde_json::Value>(&l).ok())
                    else {
                        continue;
                    };
                    let p = &meta["payload"];
                    let (Some(id), Some(dir), Some(at)) =
                        (p["id"].as_str(), p["cwd"].as_str(), p["timestamp"].as_str())
                    else {
                        continue;
                    };
                    let Ok(at) = OffsetDateTime::parse(at, &Rfc3339) else {
                        continue;
                    };
                    if meta["type"] != "session_meta"
                        || p["originator"] == "codex_exec"
                        || at < earliest
                        || !same_folder(dir, cwd)
                        || taken.iter().any(|t| t == id)
                    {
                        continue;
                    }
                    if best.as_ref().is_none_or(|(t, _)| at < *t) {
                        best = Some((at, id.to_string()));
                    }
                }
            }
        }
    }
    best.map(|(_, id)| id)
}

fn dirs_in(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    out.sort();
    out
}

/// Antigravity's latest conversation in `cwd`, from its own record of the last conversation in
/// each workspace (`<gemini>/antigravity-cli/cache/last_conversations.json`, folder → id).
pub fn agy(gemini_dir: &Path, cwd: &Path) -> Option<String> {
    let path = gemini_dir
        .join("antigravity-cli")
        .join("cache")
        .join("last_conversations.json");
    let map: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    map.iter()
        .find(|(folder, _)| same_folder(folder, cwd))
        .and_then(|(_, id)| id.as_str().map(String::from))
}

/// Whether Grok Build has saved session `id` for `cwd` (its `summary.json`): then it resumes
/// (`-r`); before that, `-s` creates it with that id. Grok names the folder after the cwd,
/// percent-encoded.
pub fn grok_saved(grok_home: &Path, cwd: &Path, id: &str) -> bool {
    let encoded: String = cwd
        .to_string_lossy()
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect();
    let sessions = grok_home.join("sessions");
    if sessions
        .join(&encoded)
        .join(id)
        .join("summary.json")
        .is_file()
    {
        return true;
    }
    // A cwd too long for a folder name is a hashed slug; look for the id under any of them.
    dirs_in(&sessions)
        .iter()
        .any(|d| d.join(id).join("summary.json").is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> OffsetDateTime {
        OffsetDateTime::parse(s, &Rfc3339).unwrap()
    }

    /// The pane's own session: in its folder, after it started, not another pane's, not an exec.
    #[test]
    fn codex_finds_the_session_a_pane_started() {
        let home = tempfile::tempdir().unwrap();
        let day = home
            .path()
            .join("sessions")
            .join("2026")
            .join("11")
            .join("01");
        std::fs::create_dir_all(&day).unwrap();
        let write = |name: &str, id: &str, cwd: &str, ts: &str, origin: &str| {
            let line = serde_json::json!({
                "type": "session_meta",
                "payload": { "id": id, "cwd": cwd, "timestamp": ts, "originator": origin }
            });
            std::fs::write(day.join(name), format!("{line}\n{{}}\n")).unwrap();
        };
        write(
            "rollout-a.jsonl",
            "old",
            r"C:\src\app",
            "2026-11-01T09:00:00Z",
            "codex-tui",
        );
        write(
            "rollout-b.jsonl",
            "elsewhere",
            r"C:\src\other",
            "2026-11-01T10:00:05Z",
            "codex-tui",
        );
        write(
            "rollout-c.jsonl",
            "exec",
            r"C:\src\app",
            "2026-11-01T10:00:06Z",
            "codex_exec",
        );
        write(
            "rollout-d.jsonl",
            "mine",
            r"\\?\c:\src\app\",
            "2026-11-01T10:00:07Z",
            "codex-tui",
        );
        write(
            "rollout-e.jsonl",
            "later",
            r"C:\src\app",
            "2026-11-01T10:30:00Z",
            "codex-tui",
        );
        let cwd = Path::new(r"C:\src\app");
        let since = at("2026-11-01T10:00:00Z");
        assert_eq!(codex(home.path(), cwd, since, &[]).as_deref(), Some("mine"));
        assert_eq!(
            codex(home.path(), cwd, since, &["mine".into()]).as_deref(),
            Some("later")
        );
        assert_eq!(
            codex(home.path(), cwd, at("2026-11-01T11:00:00Z"), &[]),
            None
        );
    }

    #[test]
    fn agy_reads_the_last_conversation_for_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("antigravity-cli").join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(
            cache.join("last_conversations.json"),
            r#"{"c:/src/app":"conv-1","C:\\src\\other":"conv-2"}"#,
        )
        .unwrap();
        assert_eq!(
            agy(dir.path(), Path::new(r"C:\src\app")).as_deref(),
            Some("conv-1")
        );
        assert_eq!(agy(dir.path(), Path::new(r"C:\src\none")), None);
    }

    #[test]
    fn grok_knows_a_saved_session() {
        let home = tempfile::tempdir().unwrap();
        let cwd = Path::new(r"C:\src\my app");
        assert!(!grok_saved(home.path(), cwd, "s1"));
        let folder = home
            .path()
            .join("sessions")
            .join("C%3A%5Csrc%5Cmy%20app")
            .join("s1");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("summary.json"), "{}").unwrap();
        assert!(grok_saved(home.path(), cwd, "s1"));
    }

    /// On a real PC (ignored): this PC's own Codex sessions in the repo's folder, since a day ago.
    #[test]
    #[ignore]
    fn real_codex_sessions() {
        let home = dirs::home_dir().unwrap().join(".codex");
        let cwd = std::env::current_dir().unwrap();
        let cwd = cwd.parent().unwrap();
        let since = OffsetDateTime::now_utc() - time::Duration::days(1);
        println!("{:?}", codex(&home, cwd, since, &[]));
    }
}
