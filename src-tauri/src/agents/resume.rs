//! Picking an agent's conversation back up after a restart of the PC (or anything else that
//! ended its terminal): which conversation a pane's agent is having, read from the agent's own
//! session files, and the command that resumes it. Gond only reads those files; it installs
//! nothing into the agents.
//!
//! - Claude Code: `~/.claude/projects/<folder>/<session>.jsonl` (`CLAUDE_CONFIG_DIR`), the
//!   folder being the working folder with every other character than a letter or digit as `-`.
//!   Resumed with `claude --resume <session>`.
//! - Codex: `~/.codex/sessions/<y>/<m>/<d>/rollout-…-<session>.jsonl` (`CODEX_HOME`), whose
//!   first line names the session and its working folder. Resumed with `codex resume <session>`.
//!
//! Only a conversation that exists is resumed: an agent with none yet (just started, nothing
//! said) has nothing to come back to. Other agents aren't resumed yet: their pane comes back
//! as its shell.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;

/// How to resume a pane's agent.
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Resume {
    /// The agent (`claude`, `codex`).
    pub agent: String,
    /// Its conversation.
    pub session: String,
    /// What to type in the pane's shell, in `cwd`.
    pub command: String,
    /// The folder it runs in: its conversation is that folder's.
    pub cwd: String,
}

/// Where each agent keeps its files.
pub struct Homes {
    pub claude: Option<PathBuf>,
    pub codex: Option<PathBuf>,
}

impl Homes {
    /// This user's: the agents' own environment variables, else their usual folders.
    pub fn user() -> Self {
        let home = |var: &str, dir: &str| {
            std::env::var_os(var)
                .map(PathBuf::from)
                .or_else(|| dirs::home_dir().map(|h| h.join(dir)))
        };
        Homes {
            claude: home("CLAUDE_CONFIG_DIR", ".claude"),
            codex: home("CODEX_HOME", ".codex"),
        }
    }
}

/// How `agent`, running in `cwd` since `since`, would be resumed: its conversation (not one of
/// `taken`, which other panes have). None for an agent with no conversation yet, or one Gond
/// can't resume.
pub fn find(
    agent: &str,
    cwd: &Path,
    since: SystemTime,
    taken: &[String],
    homes: &Homes,
) -> Option<Resume> {
    let (session, command) = match agent {
        "claude" => {
            let id = claude_session(homes.claude.as_deref()?, cwd, since, taken)?;
            (id.clone(), format!("claude --resume {id}"))
        }
        "codex" => {
            let id = codex_session(homes.codex.as_deref()?, cwd, since, taken)?;
            (id.clone(), format!("codex resume {id}"))
        }
        _ => return None,
    };
    Some(Resume {
        agent: agent.into(),
        session,
        command,
        cwd: cwd.to_string_lossy().into_owned(),
    })
}

/// Claude Code's name for a working folder's project: every character other than a letter or
/// digit as `-` (`C:\code\gond` → `C--code-gond`).
fn claude_folder(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// The newest conversation file `dir` holds that changed since `since` and isn't taken.
fn newest(
    files: impl Iterator<Item = (PathBuf, String)>,
    since: SystemTime,
    taken: &[String],
) -> Option<String> {
    files
        .filter(|(_, id)| !taken.contains(id))
        .filter_map(|(p, id)| {
            let t = std::fs::metadata(&p).and_then(|m| m.modified()).ok()?;
            (t >= since).then_some((t, id))
        })
        .max_by_key(|(t, _)| *t)
        .map(|(_, id)| id)
}

fn claude_session(home: &Path, cwd: &Path, since: SystemTime, taken: &[String]) -> Option<String> {
    let dir = home.join("projects").join(claude_folder(cwd));
    let files = std::fs::read_dir(dir).ok()?.flatten().filter_map(|e| {
        let p = e.path();
        (p.extension()? == "jsonl").then(|| {
            let id = p.file_stem()?.to_string_lossy().into_owned();
            Some((p, id))
        })?
    });
    newest(files, since, taken)
}

/// The folders a conversation started on recently is in: the last three days'.
fn codex_days(home: &Path) -> Vec<PathBuf> {
    let mut days = vec![];
    let sorted = |p: &Path| {
        let mut v: Vec<PathBuf> = std::fs::read_dir(p)
            .map(|r| {
                r.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_dir())
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v.reverse();
        v
    };
    'out: for y in sorted(&home.join("sessions")) {
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

/// Codex's session in `cwd`: its first line names it and its working folder.
fn codex_session(home: &Path, cwd: &Path, since: SystemTime, taken: &[String]) -> Option<String> {
    use std::io::BufRead;
    let want = same_folder_key(cwd);
    let files = codex_days(home).into_iter().flat_map(|day| {
        std::fs::read_dir(day)
            .map(|r| r.flatten().map(|e| e.path()).collect::<Vec<_>>())
            .unwrap_or_default()
    });
    let mine = files.filter_map(|p| {
        if p.extension()? != "jsonl" {
            return None;
        }
        let modified = std::fs::metadata(&p).and_then(|m| m.modified()).ok()?;
        if modified < since {
            return None;
        }
        let mut first = String::new();
        std::io::BufReader::new(std::fs::File::open(&p).ok()?)
            .read_line(&mut first)
            .ok()?;
        let v: serde_json::Value = serde_json::from_str(&first).ok()?;
        let meta = &v["payload"];
        let id = meta["id"].as_str()?.to_string();
        (same_folder_key(Path::new(meta["cwd"].as_str()?)) == want).then_some((p, id))
    });
    newest(mine, since, taken)
}

/// A folder as compared: Windows paths are case-insensitive, and a trailing slash doesn't count.
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
    use std::time::Duration;

    fn touch(p: &Path, body: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
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
        let before = SystemTime::now() - Duration::from_secs(5);
        let dir = home.path().join("projects").join(claude_folder(cwd));
        touch(&dir.join("aaa.jsonl"), "{}");
        std::thread::sleep(Duration::from_millis(20));
        touch(&dir.join("bbb.jsonl"), "{}");
        let homes = Homes {
            claude: Some(home.path().into()),
            codex: None,
        };
        let r = find("claude", cwd, before, &[], &homes).unwrap();
        assert_eq!(r.session, "bbb");
        assert_eq!(r.command, "claude --resume bbb");
        // Another pane has that one: this pane's is the other.
        let r = find("claude", cwd, before, &["bbb".into()], &homes).unwrap();
        assert_eq!(r.session, "aaa");
        // No conversation since it started: nothing to resume.
        let later = SystemTime::now() + Duration::from_secs(60);
        assert!(find("claude", cwd, later, &[], &homes).is_none());
        assert!(find("aider", cwd, before, &[], &homes).is_none());
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
        touch(&day.join("rollout-a.jsonl"), &meta("in-gond", "/work/gond"));
        touch(
            &day.join("rollout-b.jsonl"),
            &meta("elsewhere", "/work/other"),
        );
        let homes = Homes {
            claude: None,
            codex: Some(home.path().into()),
        };
        let since = SystemTime::now() - Duration::from_secs(5);
        let r = find("codex", Path::new("/work/gond/"), since, &[], &homes).unwrap();
        assert_eq!(r.session, "in-gond");
        assert_eq!(r.command, "codex resume in-gond");
    }
}
