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
use crate::agents::spec::Spec;
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

/// An agent Gond can install and update, as its file in `agents/` says (`agents::spec`).
pub struct Harness {
    pub id: &'static str,
    pub name: &'static str,
    pub command: &'static str,
    /// Brand icon key.
    pub provider: &'static str,
    pub changelog: &'static str,
    winget: Option<&'static str>,
    vendor_script: Option<&'static str>,
    /// Where the vendor script puts the program, from the user's folder (`.local\\bin`). Gond
    /// puts it on the user PATH after the script, so a terminal finds it even if the script
    /// didn't (a no-op when it did).
    vendor_bin_dir: Option<String>,
    /// Arguments the vendor script takes (Hermes': `-NonInteractive`): it then runs as a script
    /// block, `& ([scriptblock]::Create((irm <url>))) <args>`.
    vendor_args: Option<&'static str>,
    /// Its vendor script comes before winget.
    script_first: bool,
    bun_package: Option<&'static str>,
    /// Its registry package runs on Node, so a Bun install needs Node too.
    bun_needs_node: bool,
    /// Its own update command's arguments, for a vendor install.
    self_update: Option<Vec<String>>,
    /// Prerequisites it needs to run, whichever way it's installed: installed first in the
    /// same job when missing.
    requires: Vec<&'static str>,
    /// The per-platform packages its npm package pulls in (its native program), which Bun
    /// keeps beside it: they go too when it's uninstalled.
    platform_packages: Vec<&'static str>,
    /// The program it leaves running in the background, which holds its files: stopped, where
    /// it runs from the agent's own folders, before an update or an uninstall.
    background: Option<&'static str>,
}

impl Harness {
    /// An agent, from its file.
    fn of(s: &'static Spec) -> Self {
        let i = &s.install;
        Harness {
            id: &s.id,
            name: &s.name,
            command: &s.command,
            provider: &s.icon,
            changelog: &s.changelog,
            winget: i.winget.as_deref(),
            vendor_script: i.script.as_deref(),
            vendor_bin_dir: i
                .script_bin
                .as_deref()
                .and_then(crate::agents::spec::under_home),
            vendor_args: i.script_args.as_deref(),
            script_first: i.script_first,
            bun_package: i.npm.as_deref(),
            bun_needs_node: i.npm_needs_node,
            self_update: s.update.own.clone(),
            requires: i.requires.iter().map(String::as_str).collect(),
            platform_packages: i.platform_packages.iter().map(String::as_str).collect(),
            background: s.uninstall.background.as_deref(),
        }
    }

    /// A prerequisite Gond sets up (Git, Node…): winget, else its script.
    fn tool(
        name: &'static str,
        command: &'static str,
        winget: &'static str,
        script: Option<&'static str>,
    ) -> Self {
        Harness {
            id: "",
            name,
            command,
            provider: "",
            changelog: "",
            winget: Some(winget),
            vendor_script: script,
            vendor_bin_dir: None,
            vendor_args: None,
            script_first: false,
            bun_package: None,
            bun_needs_node: false,
            self_update: None,
            requires: vec![],
            platform_packages: vec![],
            background: None,
        }
    }
}

/// Every agent Gond can install, in its files' order.
pub fn catalog() -> &'static [Harness] {
    static ALL: std::sync::OnceLock<Vec<Harness>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| crate::agents::spec::all().iter().map(Harness::of).collect())
}

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
    let probe = Harness::tool(name, command, winget, script);
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
    let mut needs: Vec<&str> = h.requires.to_vec();
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
    catalog().iter().find(|h| h.id == id)
}

impl Harness {
    pub fn winget_id(&self) -> Option<&'static str> {
        self.winget
    }

    pub fn bun_package_name(&self) -> Option<&'static str> {
        self.bun_package
    }

    pub fn platform_packages(&self) -> &[&'static str] {
        &self.platform_packages
    }

    /// The program it keeps running in the background, if any (`background`).
    pub fn background_program(&self) -> Option<&'static str> {
        self.background
    }

    /// Whether it keeps a program running in the background (`background`).
    pub fn runs_in_background(&self) -> bool {
        self.background.is_some()
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
                // Windows PowerShell's download progress bar slows Invoke-WebRequest many
                // times over, even hidden (Cursor's script took minutes): off for the script.
                &match self.vendor_args {
                    None => format!("$ProgressPreference = 'SilentlyContinue'; irm {url} | iex"),
                    Some(args) => format!(
                        "$ProgressPreference = 'SilentlyContinue'; & ([scriptblock]::Create((irm {url}))) {args}"
                    ),
                },
            ]
            .map(String::from)
            .into(),
            from: url.into(),
            adds_to_path: self.vendor_bin_dir.clone(),
            provides: self.provides(),
        }
    }

    fn bun_plan(&self, pkg: &str) -> Plan {
        Plan {
            source: Source::Bun,
            program: "bun".into(),
            args: vec!["add".into(), "-g".into(), format!("{pkg}@latest")],
            from: format!("npm registry package {pkg}"),
            // Bun's globals land in ~\.bun\bin, which Bun from winget doesn't put on PATH:
            // without it a terminal can't run them (Gond's own panes find them anyway).
            adds_to_path: Some(".bun\\bin".into()),
            provides: self.provides(),
        }
    }

    /// Ways to install, best first: winget, then the vendor's script (the other way round for
    /// `script_first`), then Bun.
    pub fn install_plans(&self) -> Vec<Plan> {
        let mut out = vec![];
        if let Some(id) = self.winget {
            out.push(self.winget_plan("install", id));
        }
        if let Some(url) = self.vendor_script {
            let at = if self.script_first { 0 } else { out.len() };
            out.insert(at, self.vendor_plan(url));
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
            Source::Vendor => match &self.self_update {
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
    // Where an agent's own script puts it (its file's `detect.script_install`).
    } else if crate::agents::spec::all()
        .iter()
        .flat_map(|s| &s.detect.script_install)
        .any(|piece| p.contains(piece.as_str()))
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
        let handles: Vec<_> = catalog()
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
                        version: bin.as_deref().and_then(|b| detect::version_of(h.id, b)),
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
        // Bun's global bin goes on PATH, so a terminal can run what it installed (Pi).
        assert_eq!(bun.adds_to_path.as_deref(), Some(".bun\\bin"));
        let pi_update = get("pi").unwrap().update_plan(Source::Bun).unwrap();
        assert_eq!(pi_update.adds_to_path.as_deref(), Some(".bun\\bin"));
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
            "$ProgressPreference = 'SilentlyContinue'; irm https://antigravity.google/cli/install.ps1 | iex"
        );
        // Every vendor script and Bun install names the folder it leaves the program in, so
        // Gond puts it on PATH and a terminal can run the agent (winget does that itself).
        for h in catalog() {
            for p in h.install_plans() {
                if matches!(p.source, Source::Vendor | Source::Bun) {
                    assert!(
                        p.adds_to_path.is_some(),
                        "{} {:?} adds no PATH",
                        h.id,
                        p.source
                    );
                }
            }
        }
        // No harness is ever installed with npm.
        for h in catalog() {
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
        assert_eq!(
            ids,
            [
                "claude", "codex", "agy", "grok", "opencode", "pi", "copilot", "cursor", "droid",
                "kimi", "minimax", "hermes", "kilo", "devin", "cline"
            ]
        );
        assert!(found
            .iter()
            .all(|h| !h.installed && h.source.is_none() && !h.can_update));
    }

    /// Copilot: winget, else Bun (with Node, its loader's runtime). Cursor: only its
    /// script, then its own updater for that install.
    #[test]
    fn copilot_and_cursor_plans() {
        let copilot = get("copilot").unwrap();
        let plans = copilot.install_plans();
        assert_eq!(
            plans.iter().map(|p| p.source).collect::<Vec<_>>(),
            [Source::Winget, Source::Bun]
        );
        assert!(plans[0]
            .args
            .windows(2)
            .any(|w| w == ["--id", "GitHub.Copilot"]));
        assert_eq!(plans[1].args.join(" "), "add -g @github/copilot@latest");
        let empty = tempfile::tempdir().unwrap();
        let path = Some(empty.path().as_os_str().to_owned());
        let labels: Vec<_> = install_steps(copilot, Some(1), path)
            .unwrap()
            .into_iter()
            .map(|s| s.label)
            .collect();
        assert_eq!(
            labels,
            [
                "Install Node.js LTS",
                "Install Bun",
                "Install GitHub Copilot"
            ]
        );

        let cursor = get("cursor").unwrap();
        let plans = cursor.install_plans();
        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].args.last().unwrap(),
            "$ProgressPreference = 'SilentlyContinue'; irm https://cursor.com/install?win32=true | iex"
        );
        assert_eq!(plans[0].provides.as_deref(), Some("cursor-agent"));
        let update = cursor.update_plan(Source::Vendor).unwrap();
        assert_eq!(
            (update.program.as_str(), update.args.clone()),
            ("cursor-agent", vec!["update".to_string()])
        );
        for (path, want) in [
            (
                r"C:\Users\d\AppData\Local\cursor-agent\cursor-agent.cmd",
                Source::Vendor,
            ),
            (
                r"C:\Users\d\AppData\Local\Microsoft\WinGet\Packages\GitHub.Copilot_Microsoft.Winget.Source_8wekyb3d8bbwe\copilot.exe",
                Source::Winget,
            ),
        ] {
            assert_eq!(source_of(Path::new(path)), want, "{path}");
        }
    }

    /// The seven added in 0.3: each plan, Hermes' script without its wizard, Git first for the
    /// agents whose shell is Git Bash, and how each is updated.
    #[test]
    fn the_new_agents_plans() {
        let kinds = |id: &str| {
            get(id)
                .unwrap()
                .install_plans()
                .iter()
                .map(|p| p.source)
                .collect::<Vec<_>>()
        };
        assert_eq!(kinds("droid"), [Source::Vendor, Source::Bun]);
        assert_eq!(kinds("kimi"), [Source::Vendor, Source::Winget, Source::Bun]);
        assert_eq!(kinds("minimax"), [Source::Vendor]);
        assert_eq!(kinds("hermes"), [Source::Vendor]);
        assert_eq!(kinds("kilo"), [Source::Winget, Source::Bun]);
        assert_eq!(kinds("devin"), [Source::Winget, Source::Bun]);
        assert_eq!(kinds("cline"), [Source::Bun]);
        let hermes = &get("hermes").unwrap().install_plans()[0];
        assert_eq!(
            hermes.args.last().unwrap(),
            "$ProgressPreference = 'SilentlyContinue'; & ([scriptblock]::Create((irm https://hermes-agent.nousresearch.com/install.ps1))) -NonInteractive"
        );
        let empty = tempfile::tempdir().unwrap();
        let path = Some(empty.path().as_os_str().to_owned());
        let labels: Vec<_> = install_steps(get("kimi").unwrap(), Some(0), path)
            .unwrap()
            .into_iter()
            .map(|s| s.label)
            .collect();
        assert_eq!(labels, ["Install Git", "Install Kimi Code"]);
        let update = |id: &str, s: Source| {
            let p = get(id).unwrap().update_plan(s).unwrap();
            format!("{} {}", p.program, p.args.join(" "))
        };
        assert_eq!(update("droid", Source::Vendor), "droid update");
        assert!(update("kimi", Source::Vendor).starts_with("powershell "));
        assert_eq!(update("hermes", Source::Vendor), "hermes update");
        assert_eq!(update("cline", Source::Bun), "bun add -g cline@latest");
        assert!(update("kilo", Source::Winget).starts_with("winget upgrade --id KiloCode.Kilo"));
        for (path, want) in [
            (r"C:\Users\d\bin\droid.exe", Source::Vendor),
            (r"C:\Users\d\.kimi-code\bin\kimi.exe", Source::Vendor),
            (r"C:\Users\d\.minimax-code\mcode.cmd", Source::Vendor),
            (
                r"C:\Users\d\AppData\Local\hermes\bin\hermes.exe",
                Source::Vendor,
            ),
            (r"C:\Users\d\.bun\bin\cline.exe", Source::Bun),
        ] {
            assert_eq!(source_of(Path::new(path)), want, "{path}");
        }
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
