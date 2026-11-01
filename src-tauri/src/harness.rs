//! The harness manager: which agent CLIs Gond knows how to install, how each was
//! installed, and the exact command that installs or updates it.
//! Plans are data: running one is `harness.install` / `harness.update` (outward, asks).
//! Every plan runs in the background: silent, per-user, no console window; the
//! runner starts it with CREATE_NO_WINDOW and the UI shows only Gond's progress card.

use std::ffi::OsString;
use std::path::Path;

use schemars::JsonSchema;
use serde::Serialize;

use crate::agents::detect;
use crate::install::Step;

/// How a harness got onto this machine, read from where its binary lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    Winget,
    /// The vendor's own installer (`irm … | iex`), usually self-updating.
    Vendor,
    /// A package from the npm registry, installed with `bun add -g`.
    Bun,
    /// Found on PATH, but not in a place we recognise (npm, scoop, a manual copy …).
    Unknown,
    /// A step Gond does itself (an uninstall's file and PATH clean-up).
    Gond,
    /// The vendor's official release, downloaded and checksum-verified by Gond, for when
    /// winget is missing (`fallback`). `args[0]` is the prerequisite's id.
    Download,
}

/// One way to run an install or update. Never a shell string we build from input:
/// the program and its arguments are fixed per harness.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub source: Source,
    pub program: String,
    pub args: Vec<String>,
    /// Where the code comes , for the approval card.
    pub from: String,
    /// A folder under the user's profile the installer puts the program in without adding
    /// it to PATH (Claude's script: `.local\\bin`). Gond adds it to the user PATH after.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adds_to_path: Option<String>,
    /// The command this plan installs. After the step Gond looks for it, so a job never
    /// reports success while the card still says "Not installed".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provides: Option<String>,
}

impl Plan {
    /// The same plan for all users: winget's `--scope machine`, which raises one Windows
    /// prompt. Used only after a per-user install found no installer and the human chose it.
    pub fn machine_wide(mut self) -> Self {
        if let Some(i) = self.args.iter().position(|a| a == "--scope") {
            if let Some(v) = self.args.get_mut(i + 1) {
                *v = "machine".into();
            }
        }
        self
    }
}

/// A harness Gond can install and update (the v0.1 drivers).
pub struct Harness {
    pub id: &'static str,
    pub name: &'static str,
    pub command: &'static str,
    /// Brand icon key (@lobehub/icons).
    pub provider: &'static str,
    winget: Option<&'static str>,
    vendor_script: Option<&'static str>,
    /// Where the vendor script puts the program when it doesn't add it to PATH itself.
    vendor_bin_dir: Option<&'static str>,
    bun_package: Option<&'static str>,
    /// The harness's own update subcommand, for vendor installs.
    self_update: Option<&'static [&'static str]>,
    /// Its registry package runs on Node, so a Bun install needs Node too.
    bun_needs_node: bool,
    pub changelog: &'static str,
}

pub const CATALOG: &[Harness] = &[
    Harness {
        id: "claude",
        name: "Claude Code",
        command: "claude",
        provider: "claude",
        winget: Some("Anthropic.ClaudeCode"),
        vendor_script: Some("https://claude.ai/install.ps1"),
        vendor_bin_dir: Some(".local\\bin"),
        bun_package: Some("@anthropic-ai/claude-code"),
        self_update: Some(&["update"]),
        bun_needs_node: true,
        changelog: "https://github.com/anthropics/claude-code/releases",
    },
    Harness {
        id: "codex",
        name: "Codex",
        command: "codex",
        provider: "codex",
        winget: Some("OpenAI.Codex"),
        vendor_script: Some("https://chatgpt.com/codex/install.ps1"),
        vendor_bin_dir: None,
        bun_package: Some("@openai/codex"),
        self_update: Some(&["update"]),
        bun_needs_node: true,
        changelog: "https://github.com/openai/codex/releases",
    },
    Harness {
        id: "agy",
        name: "Antigravity",
        command: "agy",
        provider: "antigravity",
        winget: None,
        vendor_script: Some("https://antigravity.google/cli/install.ps1"),
        vendor_bin_dir: None,
        bun_package: None,
        // `agy update` is unverified: re-run the installer instead.
        self_update: None,
        bun_needs_node: false,
        changelog: "https://github.com/google-antigravity/antigravity-cli/releases",
    },
    Harness {
        id: "grok",
        name: "Grok Build",
        command: "grok",
        provider: "grok",
        winget: Some("xAI.GrokBuild"),
        vendor_script: Some("https://x.ai/cli/install.ps1"),
        vendor_bin_dir: None,
        bun_package: None,
        self_update: Some(&["update"]),
        bun_needs_node: false,
        changelog: "https://x.ai/changelog/build",
    },
    // v0.2, back: winget's SST.opencode, else the npm package
    // through Bun (its launcher runs on Node). No Windows vendor script; it updates itself.
    Harness {
        id: "opencode",
        name: "opencode",
        command: "opencode",
        provider: "opencode",
        winget: Some("SST.opencode"),
        vendor_script: None,
        vendor_bin_dir: None,
        bun_package: Some("opencode-ai"),
        self_update: Some(&["upgrade"]),
        bun_needs_node: true,
        changelog: "https://github.com/sst/opencode/releases",
    },
    // Pi (pi.dev): the npm package through Bun, on Node 22.19+. Not on
    // winget, and its own installer can stop to ask about PATH, so Bun installs, updates and
    // removes it.
    Harness {
        id: "pi",
        name: "Pi",
        command: "pi",
        provider: "pi",
        winget: None,
        vendor_script: None,
        vendor_bin_dir: None,
        bun_package: Some("@earendil-works/pi-coding-agent"),
        self_update: None,
        bun_needs_node: true,
        changelog: "https://github.com/earendil-works/pi/releases",
    },
];

/// Prerequisites Gond can set up: (id, name, command, winget id, vendor script).
/// Without winget, Git, GitHub CLI and Node come from their official releases.
pub const PREREQS: &[(&str, &str, &str, &str, Option<&str>)] = &[
    ("git", "Git", "git", "Git.Git", None),
    ("gh", "GitHub CLI", "gh", "GitHub.cli", None),
    ("node", "Node.js LTS", "node", "OpenJS.NodeJS.LTS", None),
    (
        "bun",
        "Bun",
        "bun",
        "Oven-sh.Bun",
        Some("https://bun.sh/install.ps1"),
    ),
];

/// The plan that installs prerequisite `id`: winget, or the vendor script without it.
pub fn prereq_step(id: &str, path_var: Option<OsString>) -> Option<Step> {
    let &(_, name, command, winget, script) = PREREQS.iter().find(|p| p.0 == id)?;
    let probe = Harness {
        id: "",
        name,
        command,
        provider: "",
        winget: Some(winget),
        vendor_script: script,
        vendor_bin_dir: None,
        bun_package: None,
        self_update: None,
        bun_needs_node: false,
        changelog: "",
    };
    let winget_here = detect::find("winget", path_var).is_some();
    let plan = if winget_here {
        probe.winget_plan("install", winget)
    } else if crate::fallback::supported(id) {
        download_plan(id, name, command)
    } else if let Some(script) = script {
        probe.vendor_plan(script)
    } else {
        probe.winget_plan("install", winget)
    };
    Some(Step {
        label: format!("Install {name}"),
        plan,
    })
}

/// Without winget: the vendor's official release, checksum-verified.
pub fn download_plan(id: &str, name: &str, command: &str) -> Plan {
    let from = match id {
        "git" => "Git for Windows' official release on GitHub, checksum-verified",
        "gh" => "GitHub CLI's official release on GitHub, checksum-verified",
        _ => "nodejs.org's official LTS release, checksum-verified",
    };
    Plan {
        source: Source::Download,
        program: "gond".into(),
        args: vec![id.to_string()],
        from: format!("{name}: {from}"),
        adds_to_path: None,
        provides: Some(command.to_string()),
    }
}

/// Prerequisites not found on `path_var`, among `ids`.
pub fn missing_prereqs(ids: &[&str], path_var: Option<OsString>) -> Vec<&'static str> {
    PREREQS
        .iter()
        .filter(|p| ids.contains(&p.0) && detect::find(p.2, path_var.clone()).is_none())
        .map(|p| p.0)
        .collect()
}

/// One job's steps to install `h` with plan `plan_index` (default: the first), with the
/// prerequisites that plan needs batched in front, so there's one approval.
pub fn install_steps(
    h: &Harness,
    plan_index: Option<usize>,
    path_var: Option<OsString>,
) -> Result<Vec<Step>, String> {
    let plans = h.install_plans();
    // The default is the best plan this PC can run: a PC without winget (Windows 10
    // LTSC, Server, a stripped image) starts at the vendor script.
    let index = plan_index.unwrap_or_else(|| {
        plans
            .iter()
            .position(|p| {
                p.source != Source::Winget || detect::find("winget", path_var.clone()).is_some()
            })
            .unwrap_or(0)
    });
    let plan = plans
        .get(index)
        .cloned()
        .ok_or_else(|| format!("{} has {} install plans", h.name, plans.len()))?;
    let mut needs = vec![];
    if plan.source == Source::Bun {
        needs.push("bun");
        if h.bun_needs_node {
            needs.push("node");
        }
    }
    let mut steps: Vec<Step> = missing_prereqs(&needs, path_var.clone())
        .into_iter()
        .filter_map(|id| prereq_step(id, path_var.clone()))
        .collect();
    steps.push(Step {
        label: format!("Install {}", h.name),
        plan,
    });
    Ok(steps)
}

pub fn get(id: &str) -> Option<&'static Harness> {
    CATALOG.iter().find(|h| h.id == id)
}

impl Harness {
    pub fn winget_id(&self) -> Option<&'static str> {
        self.winget
    }

    pub fn bun_package_name(&self) -> Option<&'static str> {
        self.bun_package
    }

    fn provides(&self) -> Option<String> {
        (!self.command.is_empty()).then(|| self.command.to_string())
    }

    fn winget_plan(&self, verb: &str, id: &str) -> Plan {
        Plan {
            source: Source::Winget,
            program: "winget".into(),
            // Silent, per-user (no UAC), no prompts: the background-install rule.
            args: [verb, "--id", id, "--exact", "--source", "winget"]
                .into_iter()
                .chain(["--silent", "--scope", "user", "--disable-interactivity"])
                .chain(["--accept-package-agreements", "--accept-source-agreements"])
                .map(String::from)
                .collect(),
            from: format!("winget package {id}"),
            adds_to_path: None,
            provides: self.provides(),
        }
    }

    fn vendor_plan(&self, url: &str) -> Plan {
        Plan {
            source: Source::Vendor,
            program: "powershell".into(),
            args: [
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &format!("irm {url} | iex"),
            ]
            .map(String::from)
            .into(),
            from: url.into(),
            adds_to_path: self.vendor_bin_dir.map(String::from),
            provides: self.provides(),
        }
    }

    fn bun_plan(&self, pkg: &str) -> Plan {
        Plan {
            source: Source::Bun,
            program: "bun".into(),
            args: vec!["add".into(), "-g".into(), format!("{pkg}@latest")],
            from: format!("npm registry package {pkg}"),
            adds_to_path: None,
            provides: self.provides(),
        }
    }

    /// Ways to install, best first: winget, then the vendor's script, then Bun.
    pub fn install_plans(&self) -> Vec<Plan> {
        let mut out = vec![];
        if let Some(id) = self.winget {
            out.push(self.winget_plan("install", id));
        }
        if let Some(url) = self.vendor_script {
            out.push(self.vendor_plan(url));
        }
        if let Some(pkg) = self.bun_package {
            out.push(self.bun_plan(pkg));
        }
        out
    }

    /// How to update an install from `source`. `None` when we can't do it safely
    /// (installed some way we don't recognise).
    pub fn update_plan(&self, source: Source) -> Option<Plan> {
        match source {
            Source::Winget => self.winget.map(|id| self.winget_plan("upgrade", id)),
            Source::Bun => self.bun_package.map(|p| self.bun_plan(p)),
            Source::Vendor => match self.self_update {
                Some(args) => Some(Plan {
                    source: Source::Vendor,
                    program: self.command.into(),
                    args: args.iter().map(|a| a.to_string()).collect(),
                    from: format!("{}'s own updater", self.name),
                    adds_to_path: None,
                    provides: self.provides(),
                }),
                None => self.vendor_script.map(|u| self.vendor_plan(u)),
            },
            Source::Unknown | Source::Gond | Source::Download => None,
        }
    }
}

/// Reads how a binary was installed from its path. Windows paths are matched
/// case-insensitively, on either slash.
pub fn source_of(bin: &Path) -> Source {
    let p = bin.to_string_lossy().replace('\\', "/").to_lowercase();
    if p.contains("/microsoft/winget/") || p.contains("/windowsapps/") {
        Source::Winget
    } else if p.contains("/.bun/bin/") || p.contains("/.bun/install/") {
        Source::Bun
    // The Claude installer uses ~/.local/bin; the Codex and Grok homes are unverified on
    // Windows until the smoke test records them.
    } else if p.contains("/.local/bin/")
        || p.contains("/appdata/local/programs/")
        // agy's installer: %LOCALAPPDATA%\agy\bin (recorded on windows-latest, 1.4.10).
        || p.contains("/appdata/local/agy/")
        || p.contains("/.grok/")
        || p.contains("/.codex/")
    {
        Source::Vendor
    } else {
        Source::Unknown
    }
}

/// What `harness.list` returns per harness.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HarnessInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    /// Shown as "Updates itself" in the UI when the vendor install auto-updates.
    pub can_update: bool,
    pub changelog: String,
}

/// Probes every catalog harness. Blocks a few seconds: call it off the async runtime.
pub fn list(path_var: Option<OsString>) -> Vec<HarnessInfo> {
    std::thread::scope(|s| {
        let handles: Vec<_> = CATALOG
            .iter()
            .map(|h| {
                let path_var = path_var.clone();
                s.spawn(move || {
                    let bin = detect::find(h.command, path_var);
                    let source = bin.as_deref().map(source_of);
                    HarnessInfo {
                        id: h.id.into(),
                        name: h.name.into(),
                        provider: h.provider.into(),
                        installed: bin.is_some(),
                        version: bin.as_deref().and_then(detect::version),
                        source,
                        can_update: source.is_some_and(|s| h.update_plan(s).is_some()),
                        changelog: h.changelog.into(),
                    }
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_plans_prefer_winget_then_vendor_then_bun() {
        let claude = get("claude").unwrap();
        let sources: Vec<_> = claude.install_plans().iter().map(|p| p.source).collect();
        assert_eq!(sources, [Source::Winget, Source::Vendor, Source::Bun]);
        let bun = &claude.install_plans()[2];
        assert_eq!(
            (bun.program.as_str(), bun.args.join(" ")),
            ("bun", "add -g @anthropic-ai/claude-code@latest".into())
        );
        let w = &claude.install_plans()[0].args;
        for flag in ["--silent", "--scope", "user", "--disable-interactivity"] {
            assert!(w.iter().any(|a| a == flag), "winget plan lacks {flag}");
        }
        let v = &claude.install_plans()[1].args;
        assert!(v.iter().any(|a| a == "-NonInteractive") && v.iter().any(|a| a == "Hidden"));
        let agy = get("agy").unwrap();
        assert_eq!(agy.install_plans().len(), 1);
        assert_eq!(
            agy.install_plans()[0].args.last().unwrap(),
            "irm https://antigravity.google/cli/install.ps1 | iex"
        );
        // No harness is ever installed with npm.
        for h in CATALOG {
            assert!(h.install_plans().iter().all(|p| p.program != "npm"));
        }
    }

    #[test]
    fn update_follows_how_it_was_installed() {
        let codex = get("codex").unwrap();
        assert_eq!(
            codex.update_plan(Source::Winget).unwrap().args[0],
            "upgrade"
        );
        let own = codex.update_plan(Source::Vendor).unwrap();
        assert_eq!(
            (own.program.as_str(), own.args.clone()),
            ("codex", vec!["update".into()])
        );
        assert!(codex.update_plan(Source::Unknown).is_none());
        // agy has no verified updater: the installer runs again.
        let agy = get("agy").unwrap().update_plan(Source::Vendor).unwrap();
        assert_eq!(agy.program, "powershell");
        assert!(get("grok").unwrap().update_plan(Source::Bun).is_none());
    }

    #[test]
    fn bun_installs_batch_their_prerequisites() {
        let empty = tempfile::tempdir().unwrap();
        let path = Some(empty.path().as_os_str().to_owned());
        let claude = get("claude").unwrap();
        let labels = |steps: Vec<Step>| steps.into_iter().map(|s| s.label).collect::<Vec<_>>();
        assert_eq!(
            labels(install_steps(claude, Some(2), path.clone()).unwrap()),
            ["Install Node.js LTS", "Install Bun", "Install Claude Code"]
        );
        // Without winget, Bun falls back to its own script and Node to its official
        // release.
        let steps = install_steps(claude, Some(2), path.clone()).unwrap();
        assert_eq!(steps[0].plan.source, Source::Download);
        assert_eq!(steps[0].plan.args, ["node"]);
        assert_eq!(steps[1].plan.from, "https://bun.sh/install.ps1");
        assert_eq!(
            labels(install_steps(claude, None, path.clone()).unwrap()),
            ["Install Claude Code"]
        );
        assert!(install_steps(claude, Some(9), path).is_err());
        assert_eq!(
            missing_prereqs(&["git", "nope"], Some(empty.path().as_os_str().to_owned())),
            ["git"]
        );
    }

    #[test]
    fn sources_from_paths() {
        for (path, want) in [
            (
                r"C:\Users\d\AppData\Local\Microsoft\WinGet\Links\claude.exe",
                Source::Winget,
            ),
            (r"C:\Users\d\.bun\bin\codex.exe", Source::Bun),
            (r"C:\Users\d\.local\bin\claude.exe", Source::Vendor),
            (r"C:\Users\d\.grok\bin\grok.exe", Source::Vendor),
            (r"C:\Users\d\AppData\Local\agy\bin\agy.exe", Source::Vendor),
            (r"C:\Program Files\nodejs\codex.cmd", Source::Unknown),
        ] {
            assert_eq!(source_of(Path::new(path)), want, "{path}");
        }
    }

    #[test]
    fn lists_every_catalog_harness() {
        let dir = tempfile::tempdir().unwrap();
        let found = list(Some(dir.path().as_os_str().to_owned()));
        let ids: Vec<_> = found.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(ids, ["claude", "codex", "agy", "grok", "opencode", "pi"]);
        assert!(found
            .iter()
            .all(|h| !h.installed && h.source.is_none() && !h.can_update));
    }

    #[test]
    fn without_winget_the_default_is_the_vendor_script() {
        let none = Some(std::ffi::OsString::from("/nonexistent-gond-path"));
        let steps = install_steps(get("claude").unwrap(), None, none.clone()).unwrap();
        assert_eq!(steps.last().unwrap().plan.source, Source::Vendor);
        assert_eq!(
            steps.last().unwrap().plan.adds_to_path.as_deref(),
            Some(".local\\bin")
        );
        // An explicit choice is kept.
        let steps = install_steps(get("claude").unwrap(), Some(0), none).unwrap();
        assert_eq!(steps.last().unwrap().plan.source, Source::Winget);
    }
}
