//! Picking an agent's conversation back up after a restart of the PC (or anything else that
//! ended its terminal): which conversation a pane's agent is having, read from the agent's own
//! session store, and the command that resumes it. Gond only reads those stores; it installs
//! nothing into the agents.
//!
//! | Agent | Its sessions | The folder from | Resumed with |
//! |---|---|---|---|
//! | Claude Code | `~/.claude/projects/<folder>/<id>.jsonl` (`CLAUDE_CONFIG_DIR`) | the folder name | `claude --resume <id>` |
//! | Codex | `~/.codex/sessions/<y>/<m>/<d>/rollout-…-<id>.jsonl` (`CODEX_HOME`) | first line `payload.cwd` | `codex resume <id>` |
//! | GitHub Copilot | `~/.copilot/session-state/<id>/workspace.yaml` (`COPILOT_HOME`) | `cwd:`; `inuse.<pid>.lock` names the very process | `copilot --resume=<id>` |
//! | Cursor | `~/.cursor/chats/<hash>/<id>/meta.json` | `cwd`, with `hasConversation` | `cursor-agent --resume <id>` |
//! | Grok Build | `~/.grok/sessions/<folder>/<id>/summary.json` (`GROK_HOME`) | `info.cwd` | `grok --resume <id>` |
//! | opencode | `~/.local/share/opencode/opencode.db`, table `session` (`XDG_DATA_HOME`) | `directory` | `opencode --session <id>` |
//! | Pi | `~/.pi/agent/sessions/--<folder>--/<time>_<id>.jsonl` (`PI_CODING_AGENT_SESSION_DIR`) | first line `cwd` | `pi --session <id>` |
//! | Antigravity | `~/.gemini/antigravity-cli/conversation_summaries.db` and `conversations/<id>.db` | `workspace_uris` | `agy --conversation <id>` |
//!
//! A pane's conversation is the newest in its agent's folder that changed since the agent
//! started and that no other pane has (Copilot's is known exactly from its process). Only a
//! conversation that exists is resumed: an agent with none yet has nothing to come back to.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;

/// How to resume a pane's agent.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
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

/// Where each agent keeps its sessions.
#[derive(Default)]
pub struct Homes {
    pub claude: Option<PathBuf>,
    pub codex: Option<PathBuf>,
    pub copilot: Option<PathBuf>,
    pub cursor: Option<PathBuf>,
    pub grok: Option<PathBuf>,
    /// opencode's data folder (holding `opencode.db`).
    pub opencode: Option<PathBuf>,
    /// Pi's sessions folder itself.
    pub pi: Option<PathBuf>,
    pub agy: Option<PathBuf>,
}

impl Homes {
    /// This user's: the agents' own environment variables, else their usual folders.
    pub fn user() -> Self {
        let env = |var: &str| std::env::var_os(var).map(PathBuf::from);
        let home = dirs::home_dir();
        let under = |p: &str| home.as_ref().map(|h| h.join(p));
        Homes {
            claude: env("CLAUDE_CONFIG_DIR").or_else(|| under(".claude")),
            codex: env("CODEX_HOME").or_else(|| under(".codex")),
            copilot: env("COPILOT_HOME").or_else(|| under(".copilot")),
            cursor: under(".cursor"),
            grok: env("GROK_HOME").or_else(|| under(".grok")),
            opencode: env("XDG_DATA_HOME")
                .map(|d| d.join("opencode"))
                .or_else(|| under(".local/share/opencode")),
            pi: env("PI_CODING_AGENT_SESSION_DIR")
                .or_else(|| env("PI_CODING_AGENT_DIR").map(|d| d.join("sessions")))
                .or_else(|| under(".pi/agent/sessions")),
            agy: under(".gemini/antigravity-cli"),
        }
    }
}

/// A conversation that may be the pane's: when it last changed, its id, its folder.
struct Candidate {
    changed: SystemTime,
    id: String,
    cwd: String,
}

/// How `agent` (process `pid`), running in `cwd` since `since`, would be resumed: its
/// conversation, not one of `taken` (which other panes have). None for an agent with no
/// conversation yet, or one Gond can't resume.
pub fn find(
    agent: &str,
    pid: Option<u32>,
    args: &[String],
    cwd: &Path,
    since: SystemTime,
    taken: &[String],
    homes: &Homes,
) -> Option<Resume> {
    let pick = |c: Vec<Candidate>| newest(c, cwd, since, taken);
    // Started on a conversation (`--resume <id>`…): that one, exactly. Else the store's.
    let session = from_args(agent, args).or_else(|| match agent {
        "claude" => pick(claude(homes.claude.as_deref()?, cwd)),
        "codex" => pick(codex(homes.codex.as_deref()?)),
        "copilot" => {
            let home = homes.copilot.as_deref()?;
            pid.and_then(|p| copilot_held_by(home, p))
                .filter(|id| !taken.contains(id))
                .or_else(|| pick(copilot(home)))
        }
        "cursor" => pick(cursor(homes.cursor.as_deref()?)),
        "grok" => pick(grok(homes.grok.as_deref()?)),
        "opencode" => pick(opencode(homes.opencode.as_deref()?)),
        "pi" => pick(pi(homes.pi.as_deref()?)),
        "agy" => pick(agy(homes.agy.as_deref()?)),
        _ => None,
    })?;
    Some(Resume {
        agent: agent.into(),
        command: command(agent, &session)?,
        session,
        cwd: cwd.to_string_lossy().into_owned(),
    })
}

/// The command that resumes `agent`'s conversation `id`.
fn command(agent: &str, id: &str) -> Option<String> {
    Some(match agent {
        "claude" => format!("claude --resume {id}"),
        "codex" => format!("codex resume {id}"),
        "copilot" => format!("copilot --resume={id}"),
        "cursor" => format!("cursor-agent --resume {id}"),
        "grok" => format!("grok --resume {id}"),
        "opencode" => format!("opencode --session {id}"),
        "pi" => format!("pi --session {id}"),
        "agy" => format!("agy --conversation {id}"),
        _ => return None,
    })
}

/// The conversation an agent was started on, from its arguments (`claude --resume <id>`,
/// `codex resume <id>`, `opencode --session <id>`…): resuming one doesn't always change its
/// files, but the command line says which it is.
fn from_args(agent: &str, args: &[String]) -> Option<String> {
    let value = |v: Option<&String>| v.filter(|v| !v.is_empty() && !v.starts_with('-')).cloned();
    let flags: &[&str] = match agent {
        "codex" => {
            let at = args.iter().position(|a| a == "resume")?;
            return value(args.get(at + 1));
        }
        "claude" | "copilot" | "cursor" | "grok" => &["--resume", "-r"],
        "opencode" => &["--session", "-s"],
        "pi" => &["--session"],
        "agy" => &["--conversation"],
        _ => return None,
    };
    for (i, a) in args.iter().enumerate() {
        for f in flags {
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

/// The `key: value` lines of a small YAML file.
fn yaml_field(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let v = l.strip_prefix(key)?.strip_prefix(':')?.trim();
        Some(v.trim_matches(['"', '\'']).to_string()).filter(|v| !v.is_empty())
    })
}

/// The Copilot session process `pid` has open: its folder holds `inuse.<pid>.lock`.
fn copilot_held_by(home: &Path, pid: u32) -> Option<String> {
    let lock = format!("inuse.{pid}.lock");
    entries(&home.join("session-state"))
        .into_iter()
        .find(|d| d.join(&lock).is_file())
        .and_then(|d| Some(d.file_name()?.to_string_lossy().into_owned()))
}

fn copilot(home: &Path) -> Vec<Candidate> {
    entries(&home.join("session-state"))
        .into_iter()
        .filter_map(|d| {
            let ws = d.join("workspace.yaml");
            let text = std::fs::read_to_string(&ws).ok()?;
            let changed = [ws.clone(), d.join("events.jsonl")]
                .iter()
                .filter_map(|p| modified(p))
                .max()?;
            Some(Candidate {
                changed,
                id: yaml_field(&text, "id")?,
                cwd: yaml_field(&text, "cwd")?,
            })
        })
        .collect()
}

fn cursor(home: &Path) -> Vec<Candidate> {
    entries(&home.join("chats"))
        .iter()
        .flat_map(|h| entries(h))
        .filter_map(|d| {
            let meta = json(&d.join("meta.json"))?;
            if meta["hasConversation"] == false {
                return None;
            }
            let changed = meta["updatedAtMs"]
                .as_u64()
                .map(|ms| UNIX_EPOCH + Duration::from_millis(ms))
                .or_else(|| modified(&d.join("meta.json")))?;
            Some(Candidate {
                changed,
                id: d.file_name()?.to_string_lossy().into_owned(),
                cwd: meta["cwd"].as_str()?.into(),
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

/// A SQLite store an agent keeps, opened read-only (it may be writing to it).
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

fn opencode(home: &Path) -> Vec<Candidate> {
    let Some(db) = read_only(&home.join("opencode.db")) else {
        return vec![];
    };
    // Top-level sessions only: a sub-agent's has a parent.
    let Ok(mut q) = db.prepare(
        "SELECT id, directory, time_updated FROM session WHERE parent_id IS NULL OR parent_id = ''",
    ) else {
        return vec![];
    };
    q.query_map([], |r| {
        Ok(Candidate {
            id: r.get(0)?,
            cwd: r.get(1)?,
            changed: UNIX_EPOCH + Duration::from_millis(r.get::<_, i64>(2)?.max(0) as u64),
        })
    })
    .map(|rows| rows.flatten().collect())
    .unwrap_or_default()
}

fn pi(sessions: &Path) -> Vec<Candidate> {
    entries(sessions)
        .iter()
        .filter(|g| g.is_dir())
        .flat_map(|g| entries(g))
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .filter_map(|p| {
            let head = first_line(&p)?;
            if head["type"] != "session" {
                return None;
            }
            Some(Candidate {
                changed: modified(&p)?,
                id: head["id"].as_str()?.into(),
                cwd: head["cwd"].as_str()?.into(),
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
        let homes = Homes {
            claude: Some(home.path().into()),
            ..Homes::default()
        };
        let r = find("claude", None, &[], cwd, before, &[], &homes).unwrap();
        assert_eq!(
            (r.session.as_str(), r.command.as_str()),
            ("bbb", "claude --resume bbb")
        );
        // Another pane has that one: this pane's is the other.
        let r = find("claude", None, &[], cwd, before, &["bbb".into()], &homes).unwrap();
        assert_eq!(r.session, "aaa");
        // No conversation since it started: nothing to resume.
        let later = SystemTime::now() + Duration::from_secs(60);
        assert!(find("claude", None, &[], cwd, later, &[], &homes).is_none());
        assert!(find("aider", None, &[], cwd, before, &[], &homes).is_none());
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
        let homes = Homes {
            codex: Some(home.path().into()),
            ..Homes::default()
        };
        let r = find(
            "codex",
            None,
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
    #[test]
    fn every_agent_resumes_from_its_own_store() {
        let root = tempfile::tempdir().unwrap();
        let h = |n: &str| root.path().join(n);
        let cwd = Path::new(r"C:\work\gond");

        // Copilot: a folder per session; the process that has it open holds a lock in it.
        write(
            &h("copilot/session-state/c-old/workspace.yaml"),
            "id: c-old\ncwd: C:\\work\\gond\nclient_name: github/cli\n",
        );
        write(
            &h("copilot/session-state/c-mine/workspace.yaml"),
            "id: c-mine\ncwd: C:\\work\\gond\n",
        );
        write(&h("copilot/session-state/c-mine/inuse.4242.lock"), "4242");
        // Cursor: a chat folder whose meta says its folder and whether it has a conversation.
        write(
            &h("cursor/chats/hash/u-1/meta.json"),
            &serde_json::json!({ "cwd": r"C:\work\gond", "hasConversation": true,
                "updatedAtMs": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64 })
            .to_string(),
        );
        write(
            &h("cursor/chats/hash/u-empty/meta.json"),
            &serde_json::json!({ "cwd": r"C:\work\gond", "hasConversation": false }).to_string(),
        );
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

        let homes = Homes {
            copilot: Some(h("copilot")),
            cursor: Some(h("cursor")),
            grok: Some(h("grok")),
            pi: Some(h("pi")),
            opencode: Some(h("opencode")),
            agy: Some(h("agy")),
            ..Homes::default()
        };
        let since = earlier();
        let got = |agent: &str, pid: Option<u32>| {
            find(agent, pid, &[], cwd, since, &[], &homes).map(|r| r.command)
        };
        assert_eq!(
            got("copilot", Some(4242)).as_deref(),
            Some("copilot --resume=c-mine")
        );
        assert_eq!(
            got("cursor", None).as_deref(),
            Some("cursor-agent --resume u-1")
        );
        assert_eq!(got("grok", None).as_deref(), Some("grok --resume g-1"));
        assert_eq!(got("pi", None).as_deref(), Some("pi --session p-1"));
        assert_eq!(
            got("opencode", None).as_deref(),
            Some("opencode --session ses_root")
        );
        assert_eq!(got("agy", None).as_deref(), Some("agy --conversation a-1"));
        // Another folder has none of them.
        let elsewhere = find(
            "grok",
            None,
            &[],
            Path::new(r"C:\other"),
            since,
            &[],
            &homes,
        );
        assert!(elsewhere.is_none());
    }

    #[test]
    fn an_agent_started_on_a_conversation_names_it() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let homes = Homes::default();
        let cwd = Path::new("/work/gond");
        let got = |agent: &str, args: &[&str]| {
            find(agent, None, &a(args), cwd, SystemTime::now(), &[], &homes).map(|r| r.command)
        };
        assert_eq!(
            got("copilot", &["copilot.exe", "--resume=c-1"]).as_deref(),
            Some("copilot --resume=c-1")
        );
        assert_eq!(
            got("cursor", &["node.exe", "index.js", "--resume", "u-1"]).as_deref(),
            Some("cursor-agent --resume u-1")
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
