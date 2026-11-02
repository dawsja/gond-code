//! Picking an agent's conversation back up after a restart of the PC (or anything else that
//! ended its terminal): which conversation a pane's agent is having, read from the agent's own
//! session store, and the command that resumes it. Gond only reads those stores; it installs
//! nothing into the agents.
//!
//! Each agent's file in `agents/` says how (`[resume]`): the command and the flags that name a
//! conversation, where its store is, and what kind it is: a SQLite file and the query giving
//! each conversation's id, folder and last change; JSONL files whose first line has them; or a
//! store laid out like no other, read by its own code here (`reader`, by name).
//!
//! A pane's conversation is the newest in its agent's folder that changed since the agent
//! started and that no other pane has. Only a conversation that exists is resumed: an agent
//! with none yet has nothing to come back to.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;

use super::spec::{self, Store};

/// How to resume a pane's agent.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Resume {
    /// The agent (`claude`, `codex`…).
    pub agent: String,
    /// Its conversation.
    pub session: String,
    /// What to type in the pane's shell, in `cwd`.
    pub command: String,
    /// The folder it runs in: its conversation is that folder's.
    pub cwd: String,
}

/// Where each agent keeps its sessions, by agent.
#[derive(Default)]
pub struct Homes(HashMap<String, PathBuf>);

impl Homes {
    /// This user's: for each agent, the first of its file's `[resume] home` that's there, else
    /// the first that's set (its own environment variable, then its usual folder).
    pub fn user() -> Self {
        let Some(r) = spec::Roots::current() else {
            return Homes::default();
        };
        Homes(
            spec::all()
                .iter()
                .filter_map(|s| {
                    let homes = &s.resume.as_ref()?.home;
                    let expanded: Vec<PathBuf> =
                        homes.iter().filter_map(|h| spec::expand(h, &r)).collect();
                    let home = expanded
                        .iter()
                        .find(|p| p.exists())
                        .or(expanded.first())?
                        .clone();
                    Some((s.id.clone(), home))
                })
                .collect(),
        )
    }

    /// These (tests).
    pub fn of(homes: &[(&str, PathBuf)]) -> Self {
        Homes(
            homes
                .iter()
                .map(|(id, p)| (id.to_string(), p.clone()))
                .collect(),
        )
    }

    fn get(&self, agent: &str) -> Option<&Path> {
        self.0.get(agent).map(PathBuf::as_path)
    }
}

/// A conversation that may be the pane's: when it last changed, its id, its folder.
struct Candidate {
    changed: SystemTime,
    id: String,
    cwd: String,
}

/// How `agent`, running in `cwd` since `since`, would be resumed: its
/// conversation, not one of `taken` (which other panes have). None for an agent with no
/// conversation yet, or one Gond can't resume.
pub fn find(
    agent: &str,
    args: &[String],
    cwd: &Path,
    since: SystemTime,
    taken: &[String],
    homes: &Homes,
) -> Option<Resume> {
    let how = spec::get(agent)?.resume.as_ref()?;
    let pick = |c: Vec<Candidate>| newest(c, cwd, since, taken);
    // Started on a conversation (`--resume <id>`…): that one, exactly. Else the store's.
    let session = from_args(how, args).or_else(|| {
        let home = homes.get(agent)?;
        match &how.store {
            Store::Sqlite { file, query } => pick(sessions_in(&under(home, file), query)),
            Store::JsonlHead { dir, kind, id, cwd } => {
                pick(jsonl_heads(&under(home, dir), kind, id, cwd))
            }
            Store::Reader { name } => match name.as_str() {
                "claude" => pick(claude(home, cwd)),
                "codex" => pick(codex(home)),
                "grok" => pick(grok(home)),
                "agy" => pick(agy(home)),
                _ => None,
            },
        }
    })?;
    Some(Resume {
        agent: agent.into(),
        command: how.command.replace("{id}", &session),
        session,
        cwd: cwd.to_string_lossy().into_owned(),
    })
}

/// `rel` (`/`-separated) under `home`.
fn under(home: &Path, rel: &str) -> PathBuf {
    rel.split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .fold(home.to_path_buf(), |d, p| d.join(p))
}

/// The conversation an agent was started on, from its arguments (`claude --resume <id>`,
/// `codex resume <id>`, `opencode --session <id>`…): resuming one doesn't always change its
/// files, but the command line says which it is.
fn from_args(how: &spec::Resume, args: &[String]) -> Option<String> {
    let value = |v: Option<&String>| v.filter(|v| !v.is_empty() && !v.starts_with('-')).cloned();
    if let Some(word) = &how.word {
        let at = args.iter().position(|a| a == word)?;
        return value(args.get(at + 1));
    }
    for (i, a) in args.iter().enumerate() {
        for f in &how.flags {
            if a == f {
                if let Some(v) = value(args.get(i + 1)) {
                    return Some(v);
                }
            } else if let Some(v) = a.strip_prefix(&format!("{f}=")) {
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

/// The newest of `all` in `cwd` that changed since `since` and isn't taken.
fn newest(all: Vec<Candidate>, cwd: &Path, since: SystemTime, taken: &[String]) -> Option<String> {
    let want = same_folder_key(cwd);
    all.into_iter()
        .filter(|c| c.changed >= since && !taken.contains(&c.id))
        .filter(|c| same_folder_key(Path::new(&c.cwd)) == want)
        .max_by_key(|c| c.changed)
        .map(|c| c.id)
}

fn modified(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// The folders (or files) in `dir`.
fn entries(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|r| r.flatten().map(|e| e.path()).collect())
        .unwrap_or_default()
}

fn json(p: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(p).ok()?).ok()
}

/// A file's first line, as JSON.
fn first_line(p: &Path) -> Option<Value> {
    use std::io::BufRead;
    let mut line = String::new();
    std::io::BufReader::new(std::fs::File::open(p).ok()?)
        .read_line(&mut line)
        .ok()?;
    serde_json::from_str(&line).ok()
}

/// Claude Code's name for a working folder's project: every character other than a letter or
/// digit as `-` (`C:\code\gond` → `C--code-gond`).
fn claude_folder(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn claude(home: &Path, cwd: &Path) -> Vec<Candidate> {
    entries(&home.join("projects").join(claude_folder(cwd)))
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .filter_map(|p| {
            Some(Candidate {
                changed: modified(&p)?,
                id: p.file_stem()?.to_string_lossy().into_owned(),
                cwd: cwd.to_string_lossy().into_owned(),
            })
        })
        .collect()
}

/// The last three days' folders: where a conversation from today's run is.
fn recent_days(sessions: &Path) -> Vec<PathBuf> {
    let sorted = |p: &Path| {
        let mut v: Vec<PathBuf> = entries(p).into_iter().filter(|p| p.is_dir()).collect();
        v.sort();
        v.reverse();
        v
    };
    let mut days = vec![];
    'out: for y in sorted(sessions) {
        for m in sorted(&y) {
            for d in sorted(&m) {
                days.push(d);
                if days.len() == 3 {
                    break 'out;
                }
            }
        }
    }
    days
}

fn codex(home: &Path) -> Vec<Candidate> {
    recent_days(&home.join("sessions"))
        .iter()
        .flat_map(|d| entries(d))
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .filter_map(|p| {
            let meta = first_line(&p)?;
            Some(Candidate {
                changed: modified(&p)?,
                id: meta["payload"]["id"].as_str()?.into(),
                cwd: meta["payload"]["cwd"].as_str()?.into(),
            })
        })
        .collect()
}

fn grok(home: &Path) -> Vec<Candidate> {
    entries(&home.join("sessions"))
        .iter()
        .filter(|g| g.is_dir())
        .flat_map(|g| entries(g))
        .filter_map(|d| {
            let path = d.join("summary.json");
            let s = json(&path)?;
            Some(Candidate {
                changed: modified(&path)?,
                id: s["info"]["id"].as_str()?.into(),
                cwd: s["info"]["cwd"].as_str()?.into(),
            })
        })
        .collect()
}

/// The remote host (`gond-remote`) is built without SQLite: its stores aren't read there.
#[cfg(gond_remote)]
fn sessions_in(_: &Path, _: &str) -> Vec<Candidate> {
    vec![]
}
#[cfg(gond_remote)]
fn agy(_: &Path) -> Vec<Candidate> {
    vec![]
}

/// A SQLite store an agent keeps, opened read-only (it may be writing to it).
#[cfg(not(gond_remote))]
fn read_only(p: &Path) -> Option<rusqlite::Connection> {
    use rusqlite::OpenFlags;
    if !p.is_file() {
        return None;
    }
    let c = rusqlite::Connection::open_with_flags(
        p,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    let _ = c.busy_timeout(Duration::from_millis(500));
    Some(c)
}

/// A time an agent's store keeps: seconds or milliseconds since 1970 (a number, or one written
/// as text), or an RFC 3339 date.
#[cfg(not(gond_remote))]
fn sql_time(v: rusqlite::types::Value) -> Option<SystemTime> {
    use rusqlite::types::Value as V;
    let from_number = |n: f64| -> Option<SystemTime> {
        if !(n.is_finite() && n > 0.0) {
            return None;
        }
        // Past the year 33658 in seconds is milliseconds.
        let secs = if n > 1e12 { n / 1000.0 } else { n };
        Some(UNIX_EPOCH + Duration::from_secs_f64(secs))
    };
    match v {
        V::Integer(i) => from_number(i as f64),
        V::Real(f) => from_number(f),
        V::Text(t) => t
            .trim()
            .parse::<f64>()
            .ok()
            .and_then(from_number)
            .or_else(|| {
                let d = time::OffsetDateTime::parse(
                    t.trim(),
                    &time::format_description::well_known::Rfc3339,
                )
                .ok()?;
                Some(
                    UNIX_EPOCH
                        + Duration::from_secs_f64(d.unix_timestamp_nanos().max(0) as f64 / 1e9),
                )
            }),
        _ => None,
    }
}

/// Rows of (id, folder, when) from an agent's SQLite store, by `sql`.
#[cfg(not(gond_remote))]
fn sessions_in(db: &Path, sql: &str) -> Vec<Candidate> {
    let Some(db) = read_only(db) else {
        return vec![];
    };
    let Ok(mut q) = db.prepare(sql) else {
        return vec![];
    };
    q.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, rusqlite::types::Value>(2)?,
        ))
    })
    .map(|rows| {
        rows.flatten()
            .filter_map(|(id, cwd, when)| {
                Some(Candidate {
                    id,
                    cwd: cwd.filter(|c| !c.is_empty())?,
                    changed: sql_time(when)?,
                })
            })
            .collect()
    })
    .unwrap_or_default()
}

/// JSONL files a folder or two down `dir` whose first line has `type` = `kind` and the
/// conversation's id and folder under `id` and `cwd` (Pi's).
fn jsonl_heads(dir: &Path, kind: &str, id: &str, cwd: &str) -> Vec<Candidate> {
    entries(dir)
        .iter()
        .filter(|g| g.is_dir())
        .flat_map(|g| entries(g))
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .filter_map(|p| {
            let head = first_line(&p)?;
            if head["type"] != kind {
                return None;
            }
            Some(Candidate {
                changed: modified(&p)?,
                id: head[id].as_str()?.into(),
                cwd: head[cwd].as_str()?.into(),
            })
        })
        .collect()
}

/// `file:///C:/a%20b` as `C:/a b`.
fn file_uri_path(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    // `file:///C:/x` → `C:/x`; `file:///home/x` → `/home/x`.
    let rest = match rest.strip_prefix('/') {
        Some(r) if r.as_bytes().get(1) == Some(&b':') => r,
        _ => rest,
    };
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&rest[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).ok()
}

#[cfg(not(gond_remote))]
fn agy(home: &Path) -> Vec<Candidate> {
    let Some(db) = read_only(&home.join("conversation_summaries.db")) else {
        return vec![];
    };
    let Ok(mut q) = db.prepare(
        "SELECT conversation_id, workspace_uris FROM conversation_summaries \
         WHERE parent_conversation_id IS NULL OR parent_conversation_id = ''",
    ) else {
        return vec![];
    };
    let rows: Vec<(String, String)> = q
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default();
    rows.into_iter()
        .flat_map(|(id, uris)| {
            // When its conversation last changed: its own database's files.
            let conv = home.join("conversations");
            let changed = [format!("{id}.db"), format!("{id}.db-wal")]
                .iter()
                .filter_map(|f| modified(&conv.join(f)))
                .max();
            let folders: Vec<String> = serde_json::from_str::<Vec<String>>(&uris)
                .unwrap_or_default()
                .iter()
                .filter_map(|u| file_uri_path(u))
                .collect();
            folders.into_iter().filter_map(move |cwd| {
                Some(Candidate {
                    changed: changed?,
                    id: id.clone(),
                    cwd,
                })
            })
        })
        .collect()
}

/// A folder as compared: Windows paths are case-insensitive, either slash will do, and a
/// trailing slash doesn't count.
fn same_folder_key(p: &Path) -> String {
    let s = p
        .to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .replace('/', "\\");
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: &Path, body: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    fn earlier() -> SystemTime {
        SystemTime::now() - Duration::from_secs(5)
    }

    #[test]
    fn claude_folders_are_named_as_claude_names_them() {
        assert_eq!(
            claude_folder(Path::new(r"C:\Users\dawso\Documents\github\gond-code")),
            "C--Users-dawso-Documents-github-gond-code"
        );
    }

    #[test]
    fn a_panes_conversation_is_its_newest_untaken_one_since_it_started() {
        let home = tempfile::tempdir().unwrap();
        let cwd = Path::new("/work/gond");
        let before = earlier();
        let dir = home.path().join("projects").join(claude_folder(cwd));
        write(&dir.join("aaa.jsonl"), "{}");
        std::thread::sleep(Duration::from_millis(20));
        write(&dir.join("bbb.jsonl"), "{}");
        let homes = Homes::of(&[("claude", home.path().to_path_buf())]);
        let r = find("claude", &[], cwd, before, &[], &homes).unwrap();
        assert_eq!(
            (r.session.as_str(), r.command.as_str()),
            ("bbb", "claude --resume bbb")
        );
        // Another pane has that one: this pane's is the other.
        let r = find("claude", &[], cwd, before, &["bbb".into()], &homes).unwrap();
        assert_eq!(r.session, "aaa");
        // No conversation since it started: nothing to resume.
        let later = SystemTime::now() + Duration::from_secs(60);
        assert!(find("claude", &[], cwd, later, &[], &homes).is_none());
        assert!(find("aider", &[], cwd, before, &[], &homes).is_none());
    }

    #[test]
    fn codex_sessions_are_matched_by_the_folder_they_name() {
        let home = tempfile::tempdir().unwrap();
        let day = home.path().join("sessions/2026/11/01");
        let meta = |id: &str, cwd: &str| {
            serde_json::json!({ "type": "session_meta", "payload": { "id": id, "cwd": cwd } })
                .to_string()
                + "\n{}\n"
        };
        write(&day.join("rollout-a.jsonl"), &meta("in-gond", "/work/gond"));
        write(
            &day.join("rollout-b.jsonl"),
            &meta("elsewhere", "/work/other"),
        );
        let homes = Homes::of(&[("codex", home.path().to_path_buf())]);
        let r = find(
            "codex",
            &[],
            Path::new("/work/gond/"),
            earlier(),
            &[],
            &homes,
        )
        .unwrap();
        assert_eq!(
            (r.session.as_str(), r.command.as_str()),
            ("in-gond", "codex resume in-gond")
        );
    }

    /// Each agent's store as the agent writes it (the shapes were read off real installs).
    /// Not in the remote host's build, which reads no SQLite.
    #[test]
    #[cfg(not(gond_remote))]
    fn every_agent_resumes_from_its_own_store() {
        let root = tempfile::tempdir().unwrap();
        let h = |n: &str| root.path().join(n);
        let cwd = Path::new(r"C:\work\gond");

        // Grok: sessions grouped under the encoded folder, each with its summary.
        write(
            &h("grok/sessions/C%3A%5Cwork%5Cgond/g-1/summary.json"),
            &serde_json::json!({ "info": { "id": "g-1", "cwd": r"C:\work\gond" } }).to_string(),
        );
        // Pi: a header line naming the session and its folder.
        write(
            &h("pi/--C--work-gond--/2026-11-01T21-00-00_p-1.jsonl"),
            &(serde_json::json!({ "type": "session", "id": "p-1", "cwd": r"C:\work\gond" })
                .to_string()
                + "\n"),
        );
        // opencode: its database's session table.
        std::fs::create_dir_all(h("opencode")).unwrap();
        let db = rusqlite::Connection::open(h("opencode/opencode.db")).unwrap();
        db.execute_batch(
            "CREATE TABLE session (id TEXT PRIMARY KEY, parent_id TEXT, directory TEXT NOT NULL, time_updated INTEGER NOT NULL);",
        )
        .unwrap();
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        db.execute(
            "INSERT INTO session VALUES ('ses_root', NULL, 'C:/work/gond', ?1), ('ses_child', 'ses_root', 'C:/work/gond', ?2)",
            (now_ms, now_ms + 10),
        )
        .unwrap();
        drop(db);
        // Antigravity: the summaries database, and the conversation's own.
        std::fs::create_dir_all(h("agy/conversations")).unwrap();
        let db = rusqlite::Connection::open(h("agy/conversation_summaries.db")).unwrap();
        db.execute_batch(
            "CREATE TABLE conversation_summaries (conversation_id TEXT, workspace_uris TEXT, parent_conversation_id TEXT NOT NULL DEFAULT '');
             INSERT INTO conversation_summaries VALUES ('a-1', '[\"file:///C:/work/gond\"]', '');",
        )
        .unwrap();
        drop(db);
        write(&h("agy/conversations/a-1.db"), "");
        // Devin: its sessions database (times in seconds); a hidden one doesn't count.
        std::fs::create_dir_all(h("devin")).unwrap();
        let now_s = now_ms / 1000;
        rusqlite::Connection::open(h("devin/sessions.db"))
            .unwrap()
            .execute_batch(&format!(
                "CREATE TABLE sessions (id TEXT, working_directory TEXT, created_at INTEGER, last_activity_at INTEGER, hidden INTEGER);
                 INSERT INTO sessions VALUES ('rogue-tachometer', 'C:/work/gond', {now_s}, {now_s}, 0), ('hidden-one', 'C:/work/gond', {0}, {0}, 1);",
                now_s + 5
            ))
            .unwrap();

        let homes = Homes::of(&[
            ("devin", h("devin")),
            ("grok", h("grok")),
            ("pi", h("pi")),
            ("opencode", h("opencode")),
            ("agy", h("agy")),
        ]);
        let since = earlier();
        let got = |agent: &str| find(agent, &[], cwd, since, &[], &homes).map(|r| r.command);
        assert_eq!(
            got("devin").as_deref(),
            Some("devin --resume rogue-tachometer")
        );
        assert_eq!(got("grok").as_deref(), Some("grok --resume g-1"));
        assert_eq!(got("pi").as_deref(), Some("pi --session p-1"));
        assert_eq!(
            got("opencode").as_deref(),
            Some("opencode --session ses_root")
        );
        assert_eq!(got("agy").as_deref(), Some("agy --conversation a-1"));
        // Another folder has none of them.
        let elsewhere = find("grok", &[], Path::new(r"C:\other"), since, &[], &homes);
        assert!(elsewhere.is_none());
    }

    #[test]
    fn an_agent_started_on_a_conversation_names_it() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let homes = Homes::default();
        let cwd = Path::new("/work/gond");
        let got = |agent: &str, args: &[&str]| {
            find(agent, &a(args), cwd, SystemTime::now(), &[], &homes).map(|r| r.command)
        };
        assert_eq!(
            got("devin", &["devin", "-r", "v-1"]).as_deref(),
            Some("devin --resume v-1")
        );
        assert_eq!(
            got("codex", &["codex", "resume", "x-1"]).as_deref(),
            Some("codex resume x-1")
        );
        assert_eq!(
            got("opencode", &["opencode", "-s", "ses_1"]).as_deref(),
            Some("opencode --session ses_1")
        );
        assert_eq!(
            got("agy", &["agy", "--conversation", "a-1"]).as_deref(),
            Some("agy --conversation a-1")
        );
        // `--resume` with no id opens a picker: nothing to tell from that.
        assert_eq!(got("claude", &["claude", "--resume", "--verbose"]), None);
        assert_eq!(got("grok", &["grok"]), None);
    }

    #[test]
    fn file_uris_are_read_as_paths() {
        assert_eq!(
            file_uri_path("file:///C:/a%20b/c").as_deref(),
            Some("C:/a b/c")
        );
        assert_eq!(
            file_uri_path("file:///home/me/x").as_deref(),
            Some("/home/me/x")
        );
        assert_eq!(file_uri_path("https://x"), None);
    }
}
