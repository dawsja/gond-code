//! The SSH machines you add in Gond are `Host`s in an ssh config of Gond's own,
//! `~/.ssh/gond/config` (Gond Code Dev: `~/.ssh/gond/config-dev`), which `~/.ssh/config`
//! includes from its first line. So a machine is `ssh <name>` in any terminal too, the list moves
//! with your `~/.ssh` folder, and its options (an `IdentityFile`, a `ProxyJump`) can be edited
//! there. Adding a machine in Gond writes its `Host`; removing it takes the `Host` out. A host
//! already in your ssh config gets an empty `Host` here: its settings stay where they are.
//!
//! The `Include` goes in when the first machine is added, or when Gond connects and finds it
//! gone. It has to come before any `Host` or `Match`, where it applies to every host.

use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

/// A `Host` in Gond's file: one machine, `ssh:<alias>`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Entry {
    pub alias: String,
    pub host_name: Option<String>,
    pub user: Option<String>,
    pub port: Option<u16>,
    /// The key it signs in with (`IdentityFile`), as written.
    pub identity: Option<String>,
}

impl Entry {
    /// `user@host:port`, as far as this file says; none for a host set up elsewhere.
    pub fn address(&self) -> Option<String> {
        let host = self.host_name.as_deref()?;
        let mut s = String::new();
        if let Some(u) = &self.user {
            s.push_str(u);
            s.push('@');
        }
        s.push_str(host);
        if let Some(p) = self.port {
            s.push_str(&format!(":{p}"));
        }
        Some(s)
    }
}

const HEADER: &str = "\
# Gond Code's SSH machines. Settings -> Machines adds and removes them here, and ~/.ssh/config
# includes this file, so `ssh <name>` reaches them from any terminal too. Options can be added
# to a machine; Gond keeps to its Host line.
";

/// Gond's file in `~/.ssh/gond/`.
fn file_name() -> &'static str {
    if crate::channel::is_dev() {
        "config-dev"
    } else {
        "config"
    }
}

pub fn ssh_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".ssh"))
}

/// Gond's ssh config.
pub fn path() -> Option<PathBuf> {
    ssh_dir().map(|d| d.join("gond").join(file_name()))
}

/// Gond's ssh config as you'd write it: `~/.ssh/gond/config`.
pub fn shown_path() -> String {
    format!("~/.ssh/gond/{}", file_name())
}

/// What `~/.ssh/config` says to read Gond's file (relative: from `~/.ssh`).
fn include_arg() -> String {
    format!("gond/{}", file_name())
}

/// A config line's keyword and the rest (`Key value`, `Key=value`), or none for a blank line or
/// a comment.
fn keyword(line: &str) -> Option<(String, &str)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let end = line
        .find(|c: char| c.is_whitespace() || c == '=')
        .unwrap_or(line.len());
    let rest = line[end..].trim_start().trim_start_matches('=').trim();
    Some((line[..end].to_ascii_lowercase(), rest))
}

/// A `Host` word that names one machine, not a pattern.
fn is_alias(word: &str) -> bool {
    !word.is_empty() && !word.starts_with('-') && !word.contains(['*', '?', '!', '/', '\\'])
}

fn words(rest: &str) -> impl Iterator<Item = &str> {
    rest.split_whitespace().map(|w| w.trim_matches('"'))
}

/// The machines in an ssh config's text: each alias of each `Host`, with what its block says.
pub fn entries(text: &str) -> Vec<Entry> {
    let mut out: Vec<Entry> = vec![];
    // Where the current block's entries start in `out`; none outside a `Host`.
    let mut block: Option<usize> = None;
    for line in text.lines() {
        let Some((key, rest)) = keyword(line) else {
            continue;
        };
        match key.as_str() {
            "host" => {
                block = Some(out.len());
                for w in words(rest).filter(|w| is_alias(w)) {
                    if !out.iter().any(|e| e.alias == w) {
                        out.push(Entry {
                            alias: w.to_string(),
                            ..Entry::default()
                        });
                    }
                }
            }
            "match" => block = None,
            "hostname" | "user" | "port" | "identityfile" => {
                let Some(start) = block else { continue };
                let value = words(rest).next().unwrap_or_default().to_string();
                for e in &mut out[start..] {
                    // The first value is the one ssh uses.
                    match key.as_str() {
                        "hostname" if e.host_name.is_none() => e.host_name = Some(value.clone()),
                        "user" if e.user.is_none() => e.user = Some(value.clone()),
                        "port" if e.port.is_none() => e.port = value.parse().ok(),
                        "identityfile" if e.identity.is_none() => e.identity = Some(value.clone()),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// The text with `e`'s `Host` added at the end.
pub fn with_entry(text: &str, e: &Entry, note: Option<&str>) -> String {
    let mut s = if text.trim().is_empty() {
        HEADER.to_string()
    } else {
        text.trim_end().to_string() + "\n"
    };
    s.push_str(&format!("\nHost {}\n", e.alias));
    if let Some(n) = note {
        s.push_str(&format!("    # {n}\n"));
    }
    if let Some(h) = &e.host_name {
        s.push_str(&format!("    HostName {h}\n"));
    }
    if let Some(u) = &e.user {
        s.push_str(&format!("    User {u}\n"));
    }
    if let Some(p) = e.port {
        s.push_str(&format!("    Port {p}\n"));
    }
    if let Some(i) = &e.identity {
        s.push_str(&format!("    IdentityFile {i}\n"));
    }
    s
}

/// The text without `alias`: its `Host` block goes, or just the word when the line names
/// others too. None when it isn't there.
pub fn without(text: &str, alias: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<String> = vec![];
    let mut found = false;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if let Some(("host", rest)) = keyword(line).as_ref().map(|(k, r)| (k.as_str(), *r)) {
            let all: Vec<&str> = words(rest).collect();
            if all.contains(&alias) {
                found = true;
                let others: Vec<&str> = all.into_iter().filter(|w| *w != alias).collect();
                if others.is_empty() {
                    // The whole block, to the next `Host` or `Match`, and the blank line before.
                    i += 1;
                    while i < lines.len()
                        && !matches!(
                            keyword(lines[i]).map(|(k, _)| k).as_deref(),
                            Some("host" | "match")
                        )
                    {
                        i += 1;
                    }
                    while out.last().is_some_and(|l| l.trim().is_empty()) {
                        out.pop();
                    }
                    if i < lines.len() {
                        out.push(String::new());
                    }
                    continue;
                }
                out.push(format!("Host {}", others.join(" ")));
                i += 1;
                continue;
            }
        }
        out.push(line.to_string());
        i += 1;
    }
    found.then(|| {
        let mut s = out.join("\n").trim_end().to_string();
        s.push('\n');
        s
    })
}

/// `~/.ssh/config`'s text with Gond's `Include` first, or none when it's there already (before
/// any `Host` or `Match`, where it applies to every host). Its line endings are kept.
pub fn with_include(text: &str, arg: &str) -> Option<String> {
    for line in text.lines() {
        match keyword(line) {
            Some((k, rest)) if k == "include" => {
                let ours = words(rest).any(|w| {
                    let w = w.replace('\\', "/");
                    w == arg || w == format!("~/.ssh/{arg}")
                });
                if ours {
                    return None;
                }
            }
            Some((k, _)) if k == "host" || k == "match" => break,
            _ => {}
        }
    }
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let (bom, body) = match text.strip_prefix('\u{feff}') {
        Some(b) => ("\u{feff}", b),
        None => ("", text),
    };
    let mut s = format!(
        "{bom}# Gond Code's SSH machines (Settings -> Machines). Keep this above any Host.{nl}Include {arg}{nl}"
    );
    if !body.trim().is_empty() {
        s.push_str(nl);
        s.push_str(body);
    }
    Some(s)
}

/// The machines in Gond's file.
pub fn list() -> Vec<Entry> {
    path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| entries(&t))
        .unwrap_or_default()
}

/// The machine `alias` in Gond's file.
pub fn find(alias: &str) -> Option<Entry> {
    list().into_iter().find(|e| e.alias == alias)
}

fn write(path: &Path, text: &str) -> AppResult<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, text)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

/// Adds `e` to Gond's file and makes sure `~/.ssh/config` includes it. `note` says where its
/// settings are when this file has none.
pub fn add(e: &Entry, note: Option<&str>) -> AppResult<()> {
    let p = path().ok_or_else(|| AppError::Invalid("no home folder".into()))?;
    let text = std::fs::read_to_string(&p).unwrap_or_default();
    if entries(&text).iter().any(|x| x.alias == e.alias) {
        return Err(AppError::Invalid(format!("{} is added already", e.alias)));
    }
    write(&p, &with_entry(&text, e, note))?;
    ensure_included()
}

/// The text with `alias`'s block holding `options` (`("HostName", Some("x"))`; none takes the
/// option out), right under its `Host` line. Its other lines stay. None when `alias` isn't
/// there.
pub fn with_options(text: &str, alias: &str, options: &[(&str, Option<String>)]) -> Option<String> {
    let mut out: Vec<String> = vec![];
    let mut inside = false;
    let mut found = false;
    for line in text.lines() {
        match keyword(line).as_ref().map(|(k, r)| (k.as_str(), *r)) {
            Some(("host", rest)) => {
                inside = words(rest).any(|w| w == alias);
                out.push(line.to_string());
                if inside {
                    found = true;
                    for (k, v) in options {
                        if let Some(v) = v {
                            out.push(format!("    {k} {v}"));
                        }
                    }
                }
                continue;
            }
            Some(("match", _)) => inside = false,
            Some((k, _)) if inside && options.iter().any(|(o, _)| o.eq_ignore_ascii_case(k)) => {
                continue
            }
            _ => {}
        }
        out.push(line.to_string());
    }
    found.then(|| {
        out.join(
            "
",
        ) + "
"
    })
}

/// The text with `alias`'s block signing in with `identity` (its `IdentityFile`), or with no
/// key of its own when none.
pub fn with_identity(text: &str, alias: &str, identity: Option<&str>) -> Option<String> {
    with_options(text, alias, &[("IdentityFile", identity.map(Into::into))])
}

fn edit(alias: &str, options: &[(&str, Option<String>)]) -> AppResult<()> {
    let p = path().ok_or_else(|| AppError::Invalid("no home folder".into()))?;
    let text = std::fs::read_to_string(&p).unwrap_or_default();
    match with_options(&text, alias, options) {
        Some(t) => write(&p, &t),
        None => Err(AppError::NotFound(format!("no machine {alias}"))),
    }
}

/// Sets where machine `alias` is: its `HostName`, `User` and `Port` (22 needs none).
pub fn set_address(
    alias: &str,
    host: &str,
    user: Option<&str>,
    port: Option<u16>,
) -> AppResult<()> {
    edit(
        alias,
        &[
            ("HostName", Some(host.to_string())),
            ("User", user.map(Into::into)),
            ("Port", port.filter(|p| *p != 22).map(|p| p.to_string())),
        ],
    )
}

/// Sets the key machine `alias` signs in with in Gond's file (none: ssh's own choice).
pub fn set_identity(alias: &str, identity: Option<&str>) -> AppResult<()> {
    edit(alias, &[("IdentityFile", identity.map(Into::into))])
}

/// Takes `alias` out of Gond's file. The `Include` stays: an empty file is no harm.
pub fn remove(alias: &str) -> AppResult<()> {
    let p = path().ok_or_else(|| AppError::Invalid("no home folder".into()))?;
    let text = std::fs::read_to_string(&p).unwrap_or_default();
    match without(&text, alias) {
        Some(t) => write(&p, &t),
        None => Err(AppError::NotFound(format!("no machine {alias}"))),
    }
}

/// Puts Gond's `Include` at the top of `~/.ssh/config` if it isn't there. The file is written
/// in place, so its permissions (which ssh checks) stay as they are.
pub fn ensure_included() -> AppResult<()> {
    let dir = ssh_dir().ok_or_else(|| AppError::Invalid("no home folder".into()))?;
    let cfg = dir.join("config");
    let text = std::fs::read_to_string(&cfg).unwrap_or_default();
    if let Some(t) = with_include(&text, &include_arg()) {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(&cfg, t)?;
    }
    Ok(())
}

/// Whether `name` matches an `Include` pattern's last part (`*`, `?`).
fn glob_match(pattern: &[u8], name: &[u8]) -> bool {
    match (pattern.first(), name.first()) {
        (None, None) => true,
        (Some(b'*'), _) => {
            glob_match(&pattern[1..], name) || (!name.is_empty() && glob_match(pattern, &name[1..]))
        }
        (Some(b'?'), Some(_)) => glob_match(&pattern[1..], &name[1..]),
        (Some(p), Some(n)) if p.eq_ignore_ascii_case(n) => glob_match(&pattern[1..], &name[1..]),
        _ => false,
    }
}

/// The files an `Include` word names: from `~/.ssh` when relative, a pattern in its last part.
fn included(ssh: &Path, word: &str) -> Vec<PathBuf> {
    let word = word.replace('\\', "/");
    let p = if let Some(rest) = word.strip_prefix("~/") {
        ssh.parent().map(|h| h.join(rest)).unwrap_or_default()
    } else if Path::new(&word).is_absolute() {
        PathBuf::from(&word)
    } else {
        ssh.join(&word)
    };
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned());
    match (name, p.parent()) {
        (Some(n), Some(dir)) if n.contains(['*', '?']) => {
            let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
                .into_iter()
                .flatten()
                .flatten()
                .filter(|e| glob_match(n.as_bytes(), e.file_name().to_string_lossy().as_bytes()))
                .map(|e| e.path())
                .collect();
            found.sort();
            found
        }
        _ => vec![p],
    }
}

fn hosts_in(ssh: &Path, file: &Path, depth: u8, out: &mut Vec<String>) {
    let Ok(text) = std::fs::read_to_string(file) else {
        return;
    };
    for line in text.lines() {
        match keyword(line) {
            Some((k, rest)) if k == "host" => {
                for w in words(rest).filter(|w| is_alias(w)) {
                    if !out.iter().any(|o| o == w) {
                        out.push(w.to_string());
                    }
                }
            }
            Some((k, rest)) if k == "include" && depth < 8 => {
                for w in words(rest) {
                    for f in included(ssh, w) {
                        hosts_in(ssh, &f, depth + 1, out);
                    }
                }
            }
            _ => {}
        }
    }
}

/// The hosts your ssh config names, the files it includes too (Gond's own among them).
pub fn hosts() -> Vec<String> {
    let mut out = vec![];
    if let Some(ssh) = ssh_dir() {
        hosts_in(&ssh, &ssh.join("config"), 0, &mut out);
    }
    out
}

/// A name ssh takes as a `Host` alias: letters, digits, `.`, `_` and `-`; the rest become `-`.
pub fn alias_of(name: &str) -> String {
    let mut s = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
            s.push(c);
        } else if !s.ends_with('-') {
            s.push('-');
        }
    }
    s.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machines_are_added_read_and_removed() {
        let box_ = Entry {
            alias: "build-box".into(),
            host_name: Some("10.0.0.2".into()),
            user: Some("me".into()),
            port: Some(2222),
            identity: None,
        };
        let t = with_entry("", &box_, None);
        assert!(t.starts_with("# Gond Code's SSH machines"));
        let t = with_entry(
            &t,
            &Entry {
                alias: "pi".into(),
                ..Entry::default()
            },
            Some("Set up in ~/.ssh/config."),
        );
        let t = with_entry(
            &t,
            &Entry {
                alias: "gpu".into(),
                host_name: Some("gpu.lan".into()),
                ..Entry::default()
            },
            None,
        );
        assert_eq!(
            entries(&t),
            [
                box_.clone(),
                Entry {
                    alias: "pi".into(),
                    ..Entry::default()
                },
                Entry {
                    alias: "gpu".into(),
                    host_name: Some("gpu.lan".into()),
                    ..Entry::default()
                }
            ]
        );
        assert_eq!(box_.address().as_deref(), Some("me@10.0.0.2:2222"));

        // The middle one goes with its note; the others are as they were.
        let t2 = without(&t, "pi").unwrap();
        assert!(!t2.contains("Host pi") && !t2.contains("Set up in"));
        assert_eq!(
            entries(&t2)
                .iter()
                .map(|e| e.alias.as_str())
                .collect::<Vec<_>>(),
            ["build-box", "gpu"]
        );
        assert!(t2.contains("    Port 2222\n\nHost gpu\n"));
        // The last one too, and an option you added by hand goes with it.
        let t3 = without(
            &t2.replace(
                "HostName gpu.lan",
                "HostName gpu.lan\n    ProxyJump bastion",
            ),
            "gpu",
        )
        .unwrap();
        assert!(t3.ends_with("    Port 2222\n") && !t3.contains("ProxyJump"));
        assert_eq!(without(&t3, "nope"), None);
        // One name of several on a Host line.
        let t4 = without("Host a b\n  HostName x\n", "a").unwrap();
        assert_eq!(
            entries(&t4),
            [Entry {
                alias: "b".into(),
                host_name: Some("x".into()),
                ..Entry::default()
            }]
        );
    }

    #[test]
    fn a_machine_gets_its_key() {
        let t = "# head\n\nHost a\n    HostName x\n    IdentityFile ~/.ssh/old\n\nHost b\n    HostName y\n";
        let t2 = with_identity(t, "a", Some("~/.ssh/gond-laptop")).unwrap();
        assert_eq!(
            t2,
            "# head\n\nHost a\n    IdentityFile ~/.ssh/gond-laptop\n    HostName x\n\nHost b\n    HostName y\n"
        );
        let a = &entries(&t2)[0];
        assert_eq!(a.identity.as_deref(), Some("~/.ssh/gond-laptop"));
        assert_eq!(a.host_name.as_deref(), Some("x"));
        // None takes it out; b is untouched throughout.
        let t3 = with_identity(&t2, "a", None).unwrap();
        assert!(!t3.contains("IdentityFile") && t3.contains("Host b\n    HostName y\n"));
        assert_eq!(with_identity(t, "c", None), None);
        // A new address: its old HostName goes, a Port of 22 isn't written, its key stays.
        let t4 = with_options(
            &t2,
            "a",
            &[
                ("HostName", Some("10.0.0.9".into())),
                ("User", Some("me".into())),
                ("Port", None),
            ],
        )
        .unwrap();
        let a = &entries(&t4)[0];
        assert_eq!(a.host_name.as_deref(), Some("10.0.0.9"));
        assert_eq!(a.user.as_deref(), Some("me"));
        assert_eq!(a.identity.as_deref(), Some("~/.ssh/gond-laptop"));
        assert_eq!(t4.matches("HostName").count(), 2);
    }

    #[test]
    fn the_include_goes_first_once() {
        let arg = "gond/config";
        let t = with_include("Include toad/config", arg).unwrap();
        assert_eq!(
            t,
            "# Gond Code's SSH machines (Settings -> Machines). Keep this above any Host.\nInclude gond/config\n\nInclude toad/config"
        );
        assert_eq!(with_include(&t, arg), None);
        assert_eq!(
            with_include("Include ~/.ssh/gond/config\r\nHost x\r\n", arg),
            None
        );
        // Inside a Host it applies to that host alone: it goes first again.
        let t = with_include("Host x\r\n  Include gond/config\r\n", arg).unwrap();
        assert!(t.starts_with("# Gond") && t.contains("\r\nInclude gond/config\r\n\r\nHost x\r\n"));
        assert!(with_include("", arg)
            .unwrap()
            .ends_with("Include gond/config\n"));
    }

    #[test]
    fn included_files_offer_their_hosts() {
        let home = tempfile::tempdir().unwrap();
        let ssh = home.path().join(".ssh");
        std::fs::create_dir_all(ssh.join("conf.d")).unwrap();
        std::fs::write(
            ssh.join("config"),
            "Include conf.d/*.conf\nHost dev-box *.lan !x\n",
        )
        .unwrap();
        std::fs::write(ssh.join("conf.d/a.conf"), "Host=gpu\n").unwrap();
        std::fs::write(ssh.join("conf.d/b.txt"), "Host skipped\n").unwrap();
        let mut out = vec![];
        hosts_in(&ssh, &ssh.join("config"), 0, &mut out);
        assert_eq!(out, ["gpu", "dev-box"]);
    }

    #[test]
    fn a_name_becomes_an_alias() {
        assert_eq!(alias_of(" Build box #2 "), "Build-box-2");
        assert_eq!(alias_of("me@10.0.0.2"), "me-10.0.0.2");
        assert_eq!(alias_of("@@"), "");
    }
}
