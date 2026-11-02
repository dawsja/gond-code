//! The agents Gond knows, as data: one TOML file per agent in the repo's `agents/` folder,
//! built into the app (and into the terminal host for WSL and SSH machines). Everything about
//! an agent that its vendor can change lives there: how it's installed, updated and
//! uninstalled, where it puts itself, how it's seen running, how its conversations resume,
//! what its prompts look like, where its plan usage comes from. When an agent's CLI changes,
//! its file is what changes. What's in code is the general machinery, and a few readers a file
//! names by kind (a store laid out like no other).
//!
//! Paths in a file use `/` and start from `~` (the user's folder), `%LOCALAPPDATA%`,
//! `%APPDATA%` or `$VAR` (an environment variable the agent reads); `expand` resolves them.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Deserialize;

/// Each agent's file, in the order Gond lists them. Adding an agent: its file here and a line
/// below (a test checks the two match).
pub const FILES: &[(&str, &str)] = &[
    ("claude", include_str!("../../../agents/claude.toml")),
    ("codex", include_str!("../../../agents/codex.toml")),
    ("agy", include_str!("../../../agents/agy.toml")),
    ("grok", include_str!("../../../agents/grok.toml")),
    ("opencode", include_str!("../../../agents/opencode.toml")),
    ("pi", include_str!("../../../agents/pi.toml")),
    ("copilot", include_str!("../../../agents/copilot.toml")),
    ("cursor", include_str!("../../../agents/cursor.toml")),
    ("droid", include_str!("../../../agents/droid.toml")),
    ("kimi", include_str!("../../../agents/kimi.toml")),
    ("minimax", include_str!("../../../agents/minimax.toml")),
    ("hermes", include_str!("../../../agents/hermes.toml")),
    ("kilo", include_str!("../../../agents/kilo.toml")),
    ("devin", include_str!("../../../agents/devin.toml")),
    ("cline", include_str!("../../../agents/cline.toml")),
];

/// One agent.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub id: String,
    pub name: String,
    /// What you type to start it.
    pub command: String,
    /// Its brand icon (`src/components/brand/brands.ts`).
    pub icon: String,
    pub changelog: String,
    /// Its plan usage's reader in `plan_usage.rs` (by name), if Gond can read one.
    #[serde(default)]
    pub usage: Option<String>,
    #[serde(default)]
    pub install: Install,
    #[serde(default)]
    pub update: Update,
    #[serde(default)]
    pub detect: Detect,
    #[serde(default)]
    pub uninstall: Uninstall,
    #[serde(default)]
    pub resume: Option<Resume>,
    #[serde(default)]
    pub status: Status,
}

/// The ways to install it, best first: winget, its script, its npm package through Bun.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Install {
    /// winget package id.
    pub winget: Option<String>,
    /// The vendor's PowerShell script (`irm <url> | iex`).
    pub script: Option<String>,
    /// Arguments the script takes; it then runs as a script block.
    pub script_args: Option<String>,
    /// Where the script puts the program (under `~`): Gond puts it on the user PATH after.
    pub script_bin: Option<String>,
    /// The script before winget (winget's package downloads from somewhere out of reach).
    #[serde(default)]
    pub script_first: bool,
    /// Its npm package, installed with `bun add -g`.
    pub npm: Option<String>,
    /// The npm package runs on Node, so a Bun install needs Node too.
    #[serde(default)]
    pub npm_needs_node: bool,
    /// Prerequisites it needs to run, whichever way it's installed (`git`, `node`).
    #[serde(default)]
    pub requires: Vec<String>,
    /// The per-platform npm packages its package pulls in (its native program): uninstalled
    /// with it.
    #[serde(default)]
    pub platform_packages: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Update {
    /// Its own update command's arguments, for a script install; none re-runs the script.
    #[serde(rename = "self")]
    pub own: Option<Vec<String>>,
    /// Where its latest version is published.
    pub latest: Option<Latest>,
}

/// Where an agent publishes its latest version.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum Latest {
    /// The npm registry's `latest`.
    Npm(String),
    /// A GitHub repo's latest release.
    Github(String),
    /// The vendor's install script, which names the version (`$version = '…'`).
    Script(String),
    /// A plain-text file holding the version and nothing else.
    Text(String),
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Detect {
    /// Folders its installers put it in, looked in before PATH has them.
    #[serde(default)]
    pub dirs: Vec<String>,
    /// Pieces of a program's path (lowercase, `/`) that say its script installed it.
    #[serde(default)]
    pub script_install: Vec<String>,
    /// Folders on a runtime's path, or in its arguments, that say it runs this agent (MiniMax
    /// Code's `mcode` is Node running a script).
    #[serde(default)]
    pub markers: Vec<String>,
    /// A file its install writes its version in, read before running `--version`.
    pub version_file: Option<VersionFile>,
    /// Its program is a launcher whose own folder isn't the one it runs in: its first child's
    /// is.
    #[serde(default)]
    pub cwd_from_child: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionFile {
    /// From the program's folder (`../hermes-agent/install-stamp.json`).
    pub path: String,
    /// Its JSON key.
    pub key: String,
}

/// What an uninstall removes.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Uninstall {
    /// Its script install's program files; the first is the program, which marks the install.
    #[serde(default)]
    pub program: Vec<String>,
    /// PATH entries its script added.
    #[serde(default)]
    pub path: Vec<String>,
    /// A PATH entry shared with other tools: removed once nothing else is in it.
    pub shared_path: Option<String>,
    /// Settings and sign-ins, removed with "Remove everything".
    #[serde(default)]
    pub settings: Vec<String>,
    /// A program it leaves running in the background (`cline.exe`), stopped first.
    pub background: Option<String>,
}

/// How its conversation resumes.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resume {
    /// The resume command; `{id}` is the conversation.
    pub command: String,
    /// The flags that name a conversation on its command line (`--resume`, `-r`).
    #[serde(default)]
    pub flags: Vec<String>,
    /// A word on its command line followed by the conversation (Codex's `resume <id>`).
    pub word: Option<String>,
    /// Where its store is: the first of these that's set.
    #[serde(default)]
    pub home: Vec<String>,
    pub store: Store,
}

/// An agent's store of conversations.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Store {
    /// A SQLite file, and the query giving each conversation's id, folder and last change
    /// (seconds, milliseconds or a date), in that order.
    Sqlite { file: String, query: String },
    /// JSONL files a folder or two down `dir`, whose first line has a `type`, an id and a
    /// folder.
    JsonlHead {
        dir: String,
        #[serde(rename = "type")]
        kind: String,
        id: String,
        cwd: String,
    },
    /// A store laid out like no other, read by its own code in `resume.rs` (by name).
    Reader { name: String },
}

/// How its status shows, for the pane (`status.ts`).
#[derive(Debug, Clone, PartialEq, Deserialize, serde::Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Status {
    /// Its title's reader in `status.ts` (by name), if its title says what it's doing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Its prompts' own words (regular expressions, any case): on screen, it needs you.
    #[serde(default)]
    pub prompts: Vec<String>,
    /// Where it asks: `bottom` (the pane's last rows and the cursor's) or `screen` (anywhere).
    #[serde(default = "screen")]
    pub scan: String,
    /// The key it pastes an image on: `ctrl+v` or `alt+v`.
    #[serde(default = "ctrl_v")]
    pub paste: String,
    /// Lines its prompts show, as you'd see them on screen: each must match one of `prompts`
    /// (checked by the tests, so a pattern that stops matching is caught).
    #[serde(default, skip_serializing)]
    pub examples: Vec<String>,
}

impl Default for Status {
    fn default() -> Self {
        Status {
            title: None,
            prompts: vec![],
            scan: screen(),
            paste: ctrl_v(),
            examples: vec![],
        }
    }
}

fn screen() -> String {
    "screen".into()
}

fn ctrl_v() -> String {
    "ctrl+v".into()
}

/// The readers a file can name, each written for one agent's own format: `usage` (in
/// `api/usage.rs` and `plan_usage.rs`), `[resume] store = { kind = "reader" }` (in
/// `resume.rs`) and `[status] title` (in `src/features/workspace/status.ts`). A name not here
/// reads nothing; the tests check every file's.
pub const USAGE_READERS: &[&str] = &[
    "claude", "codex", "grok", "agy", "copilot", "cursor", "kimi", "minimax", "hermes",
];
pub const RESUME_READERS: &[&str] = &[
    "claude", "codex", "copilot", "cursor", "grok", "agy", "kimi",
];
pub const TITLE_READERS: &[&str] = &["claude", "codex", "minimax"];

/// Every agent, from its file. A file that doesn't read is a bug the tests catch.
pub fn all() -> &'static [Spec] {
    static ALL: OnceLock<Vec<Spec>> = OnceLock::new();
    ALL.get_or_init(|| {
        FILES
            .iter()
            .map(|(id, text)| {
                let s: Spec = toml::from_str(text)
                    .unwrap_or_else(|e| panic!("agents/{id}.toml doesn't read: {e}"));
                assert_eq!(&s.id, id, "agents/{id}.toml names itself {}", s.id);
                s
            })
            .collect()
    })
}

/// Agent `id`.
pub fn get(id: &str) -> Option<&'static Spec> {
    all().iter().find(|s| s.id == id)
}

/// The user's folders a path in a file starts from.
#[derive(Debug, Clone)]
pub struct Roots {
    pub home: PathBuf,
    /// `%LOCALAPPDATA%`
    pub local: PathBuf,
    /// `%APPDATA%`
    pub roaming: PathBuf,
}

impl Roots {
    /// This user's. Off Windows, `%LOCALAPPDATA%` is `~/.local/share` and `%APPDATA%` is
    /// `~/.config`.
    pub fn current() -> Option<Self> {
        Some(Roots {
            home: dirs::home_dir()?,
            local: dirs::data_local_dir()?,
            roaming: dirs::config_dir()?,
        })
    }
}

/// A file's path made real: `~/x`, `%LOCALAPPDATA%/x`, `%APPDATA%/x`, `$VAR/x` (none when the
/// variable isn't set), or a plain absolute path.
pub fn expand(path: &str, r: &Roots) -> Option<PathBuf> {
    expand_with(path, r, |v| std::env::var_os(v))
}

pub fn expand_with(
    path: &str,
    r: &Roots,
    env: impl Fn(&str) -> Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    let (head, rest) = path.split_once('/').unwrap_or((path, ""));
    let base = match head {
        "~" => r.home.clone(),
        "%LOCALAPPDATA%" => r.local.clone(),
        "%APPDATA%" => r.roaming.clone(),
        v if v.starts_with('$') => PathBuf::from(env(&v[1..]).filter(|x| !x.is_empty())?),
        _ => return Some(PathBuf::from(path)),
    };
    Some(
        rest.split('/')
            .filter(|p| !p.is_empty())
            .fold(base, |p, part| p.join(part)),
    )
}

/// A path under `~` as the installer's own form, from the user's folder with `\`
/// (`.local\bin`, `AppData\Local\agy\bin`): what an install plan puts on PATH.
pub fn under_home(path: &str) -> Option<String> {
    let (head, rest) = path.split_once('/').unwrap_or((path, ""));
    let base = match head {
        "~" => "",
        "%LOCALAPPDATA%" => "AppData/Local/",
        "%APPDATA%" => "AppData/Roaming/",
        _ => return None,
    };
    Some(format!("{base}{rest}").replace('/', "\\"))
}

/// The first of `paths` that's there to start from (`$VAR` set, or `~`…).
pub fn first_home(paths: &[String], r: &Roots) -> Option<PathBuf> {
    paths.iter().find_map(|p| expand(p, r))
}

impl Spec {
    /// Its resume store's folder for this user.
    pub fn resume_home(&self, r: &Roots) -> Option<PathBuf> {
        first_home(&self.resume.as_ref()?.home, r)
    }
}

/// Whether `p` is inside `dir` (any case, either slash).
pub fn inside(p: &Path, dir: &Path) -> bool {
    let norm = |x: &Path| x.to_string_lossy().replace('\\', "/").to_lowercase();
    let (p, d) = (norm(p), norm(dir));
    p == d || p.starts_with(&format!("{}/", d.trim_end_matches('/')))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every file in `agents/` reads, names itself, is listed once, and has what Gond needs.
    #[test]
    fn every_agent_file_reads_and_is_listed() {
        // The repo's `agents/`, from the app's crate or the remote host's. Not there for a
        // test binary run away from its checkout (the remote host's, built on Windows and run
        // in WSL); the app's own run always compares them.
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .map(|d| d.join("agents"))
            .find(|d| d.join("claude.toml").is_file());
        assert!(
            dir.is_some() || cfg!(gond_remote),
            "the repo's agents/ isn't there"
        );
        if let Some(dir) = dir {
            let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
                .unwrap()
                .flatten()
                .filter_map(|e| {
                    e.file_name()
                        .to_str()?
                        .strip_suffix(".toml")
                        .map(String::from)
                })
                .collect();
            on_disk.sort();
            let mut listed: Vec<String> = FILES.iter().map(|(id, _)| id.to_string()).collect();
            listed.sort();
            assert_eq!(on_disk, listed, "agents/ and spec::FILES differ");
        }
        for s in all() {
            assert!(!s.name.is_empty() && !s.command.is_empty(), "{}", s.id);
            assert!(
                s.install.winget.is_some() || s.install.script.is_some() || s.install.npm.is_some(),
                "{} has no way to install",
                s.id
            );
            if let Some(r) = &s.resume {
                assert!(
                    r.command.contains("{id}"),
                    "{}'s resume names no {{id}}",
                    s.id
                );
            }
            assert!(
                ["screen", "bottom"].contains(&s.status.scan.as_str()),
                "{}: scan",
                s.id
            );
            assert!(
                ["ctrl+v", "alt+v"].contains(&s.status.paste.as_str()),
                "{}: paste",
                s.id
            );
            // Its prompt patterns compile (the pane reads them as JavaScript regular
            // expressions, any case, a line at a time: these are the same syntax), and each
            // example line matches one.
            let patterns: Vec<regex::Regex> = s
                .status
                .prompts
                .iter()
                .map(|p| {
                    assert!(!p.is_empty(), "{}: an empty prompt pattern", s.id);
                    regex::Regex::new(&format!("(?im){p}"))
                        .unwrap_or_else(|e| panic!("{}: {p}: {e}", s.id))
                })
                .collect();
            for line in &s.status.examples {
                assert!(
                    patterns.iter().any(|r| r.is_match(line)),
                    "{}: no prompt pattern matches {line:?}",
                    s.id
                );
            }
            // A reader it names is one the code has.
            if let Some(u) = &s.usage {
                assert!(USAGE_READERS.contains(&u.as_str()), "{}: usage {u:?}", s.id);
            }
            if let Some(Resume {
                store: Store::Reader { name },
                ..
            }) = &s.resume
            {
                assert!(
                    RESUME_READERS.contains(&name.as_str()),
                    "{}: reader {name:?}",
                    s.id
                );
            }
            if let Some(t) = &s.status.title {
                assert!(TITLE_READERS.contains(&t.as_str()), "{}: title {t:?}", s.id);
            }
        }
    }

    #[test]
    fn paths_expand_from_the_users_folders() {
        let r = Roots {
            home: PathBuf::from("/h"),
            local: PathBuf::from("/h/AppData/Local"),
            roaming: PathBuf::from("/h/AppData/Roaming"),
        };
        let env = |v: &str| (v == "SET").then(|| std::ffi::OsString::from("/set"));
        assert_eq!(
            expand_with("~/.local/bin", &r, env),
            Some(PathBuf::from("/h").join(".local").join("bin"))
        );
        assert_eq!(
            expand_with("%LOCALAPPDATA%/agy", &r, env),
            Some(PathBuf::from("/h/AppData/Local").join("agy"))
        );
        assert_eq!(
            expand_with("$SET/sessions", &r, env),
            Some(PathBuf::from("/set").join("sessions"))
        );
        assert_eq!(expand_with("$UNSET/x", &r, env), None);
        assert_eq!(under_home("~/.local/bin").as_deref(), Some(".local\\bin"));
        assert_eq!(
            under_home("%LOCALAPPDATA%/agy/bin").as_deref(),
            Some("AppData\\Local\\agy\\bin")
        );
    }
}
