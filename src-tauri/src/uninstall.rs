//! Uninstall a harness: every way it was installed, its PATH entries, and with
//! "Remove everything" its settings, sign-ins and Gond's accounts for it. What each
//! install leaves was recorded on windows-latest (the Install check job): the lists
//! below are exactly those files, nothing found by searching.

use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::Serialize;

use crate::harness::{Harness, Plan, Source};
use crate::install::Step;

/// The user's folders an uninstall works under: the ones an agent's file starts its paths from.
pub use crate::agents::spec::Roots;

/// What a harness's vendor installer leaves, and its settings.
struct Footprint {
    /// Program files and caches from the vendor's installer; the first is the program.
    install: Vec<PathBuf>,
    /// PATH entries the installer added.
    path: Vec<PathBuf>,
    /// A PATH entry shared with other tools: removed only once nothing else is left in it.
    shared_path: Option<PathBuf>,
    /// Settings and sign-ins, removed only with "Remove everything".
    config: Vec<PathBuf>,
}

/// What agent `id`'s file says its installs leave (`[uninstall]`).
fn footprint(id: &str, r: &Roots) -> Footprint {
    let Some(s) = crate::agents::spec::get(id) else {
        return Footprint {
            install: vec![],
            path: vec![],
            shared_path: None,
            config: vec![],
        };
    };
    let paths = |v: &[String]| -> Vec<PathBuf> {
        v.iter()
            .filter_map(|p| crate::agents::spec::expand(p, r))
            .collect()
    };
    let u = &s.uninstall;
    Footprint {
        install: paths(&u.program),
        path: paths(&u.path),
        shared_path: u
            .shared_path
            .as_deref()
            .and_then(|p| crate::agents::spec::expand(p, r)),
        config: paths(&u.settings),
    }
}

/// Whether harness `id`, not installed, left something an uninstall would clear: its settings,
/// a PATH entry, the rest of an uninstall that stopped halfway (a file in use).
pub fn leftovers(id: &str) -> bool {
    let (Some(h), Some(r)) = (crate::harness::get(id), Roots::current()) else {
        return false;
    };
    !plan(h, &r, &crate::pathenv::user_entries(), true)
        .steps
        .is_empty()
}

/// What an uninstall will do, for the confirm card (`harness.uninstallPlan`).
#[derive(Debug, Clone, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UninstallPlan {
    pub harness_id: String,
    /// The install methods found: winget, vendor, bun, npm.
    pub methods: Vec<String>,
    pub steps: Vec<Step>,
    /// Every file and folder that will be deleted, exactly.
    pub deletes: Vec<String>,
    /// User PATH entries that will be removed.
    pub path_entries: Vec<String>,
    /// `false`: settings, sign-ins and Gond's accounts go too ("Remove everything").
    pub keep_data: bool,
}

fn exists(p: &Path) -> bool {
    p.symlink_metadata().is_ok()
}

fn show(p: &Path) -> String {
    p.to_string_lossy().to_string()
}

/// The plan for `h`, from what's on disk under `r` and the user PATH `user_path`.
pub fn plan(
    h: &Harness,
    r: &Roots,
    user_path: &[String],
    remove_everything: bool,
) -> UninstallPlan {
    let mut methods = vec![];
    let mut steps = vec![];
    let mut deletes: Vec<PathBuf> = vec![];
    let cmd = h.command;

    // winget: its own uninstall removes the package folder and the Links shim.
    if let Some(id) = h.winget_id() {
        let winget = r.local.join("Microsoft").join("WinGet");
        let has_package = std::fs::read_dir(winget.join("Packages"))
            .map(|d| {
                d.filter_map(Result::ok).any(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .starts_with(&format!("{id}_"))
                })
            })
            .unwrap_or(false);
        if has_package || exists(&winget.join("Links").join(format!("{cmd}.exe"))) {
            methods.push("winget".to_string());
            steps.push(Step {
                label: format!("Uninstall {} (winget)", h.name),
                plan: Plan {
                    source: Source::Winget,
                    program: "winget".into(),
                    args: ["uninstall", "--id", id, "--exact", "--source", "winget"]
                        .into_iter()
                        .chain([
                            "--silent",
                            "--disable-interactivity",
                            "--accept-source-agreements",
                        ])
                        .map(String::from)
                        .collect(),
                    from: format!("winget package {id}"),
                    adds_to_path: None,
                    provides: None,
                },
            });
        }
    }
    // npm and Bun globals: their shims and the package folder.
    if let Some(pkg) = h.bun_package_name() {
        let pkg_dir = |root: PathBuf| pkg.split('/').fold(root, |p, part| p.join(part));
        let npm = r.roaming.join("npm");
        if exists(&npm.join(format!("{cmd}.cmd"))) {
            methods.push("npm".to_string());
            for shim in [cmd.to_string(), format!("{cmd}.cmd"), format!("{cmd}.ps1")] {
                deletes.push(npm.join(shim));
            }
            deletes.push(pkg_dir(npm.join("node_modules")));
            for p in h.platform_packages() {
                deletes.push(
                    p.split('/')
                        .fold(npm.join("node_modules"), |d, part| d.join(part)),
                );
            }
        }
        let bun = r.home.join(".bun");
        if exists(&bun.join("bin").join(format!("{cmd}.exe"))) {
            methods.push("bun".to_string());
            deletes.push(bun.join("bin").join(format!("{cmd}.exe")));
            deletes.push(bun.join("bin").join(format!("{cmd}.bunx")));
            deletes.push(pkg_dir(
                bun.join("install").join("global").join("node_modules"),
            ));
            // Its native program, which Bun keeps beside it, not inside.
            for p in h.platform_packages() {
                deletes.push(p.split('/').fold(
                    bun.join("install").join("global").join("node_modules"),
                    |d, part| d.join(part),
                ));
            }
        }
    }
    // The vendor's installer.
    let f = footprint(h.id, r);
    // Its program (the first entry) marks a vendor install; caches alone (Codex writes
    // ~\.codex\tmp on any run, winget installs included) don't.
    if f.install.first().is_some_and(|p| exists(p)) {
        methods.push("vendor".to_string());
        deletes.extend(f.install.iter().filter(|p| exists(p)).cloned());
    }
    if remove_everything {
        deletes.extend(f.config.iter().cloned());
    }
    // Only what's there, once, and nothing inside something already going.
    deletes.retain(|p| exists(p));
    deletes.sort();
    deletes.dedup();
    let all = deletes.clone();
    deletes.retain(|p| !all.iter().any(|o| o != p && p.starts_with(o)));

    // PATH: the installer's own entries, and a shared folder only once it's emptied.
    let on_path = |d: &Path| {
        user_path
            .iter()
            .any(|e| crate::pathenv::same_entry(e, &show(d)))
    };
    let mut entries: Vec<PathBuf> = f.path.iter().filter(|d| on_path(d)).cloned().collect();
    let mut shared: Vec<PathBuf> = f.shared_path.iter().cloned().collect();
    // Bun's global bin, which Gond puts on PATH for Bun installs: shared by every Bun tool.
    if methods.iter().any(|m| m == "bun") {
        shared.push(r.home.join(".bun").join("bin"));
    }
    for dir in &shared {
        let left = std::fs::read_dir(dir)
            .map(|d| {
                d.filter_map(Result::ok)
                    .any(|e| !deletes.contains(&e.path()))
            })
            .unwrap_or(false);
        if on_path(dir) && !left {
            entries.push(dir.clone());
        }
    }

    // A program it keeps running in the background (Cline's hub) holds its files: stopped
    // first.
    if h.runs_in_background() && (!methods.is_empty() || !deletes.is_empty()) {
        steps.insert(0, stop_step(h, r));
    }
    if !deletes.is_empty() {
        steps.push(Step {
            label: format!("Remove {}'s files", h.name),
            plan: gond("remove", deletes.iter().map(|p| show(p)).collect()),
        });
    }
    if !entries.is_empty() {
        steps.push(Step {
            label: format!("Take {} off PATH", h.name),
            plan: gond("path-remove", entries.iter().map(|p| show(p)).collect()),
        });
    }
    UninstallPlan {
        harness_id: h.id.to_string(),
        methods,
        steps,
        deletes: deletes.iter().map(|p| show(p)).collect(),
        path_entries: entries.iter().map(|p| show(p)).collect(),
        keep_data: !remove_everything,
    }
}

/// The step that stops what agent `h` runs in the background: its program running from
/// where npm or Bun keep its native program.
pub fn stop_step(h: &Harness, r: &Roots) -> Step {
    let roots = [
        r.roaming.join("npm").join("node_modules"),
        r.home
            .join(".bun")
            .join("install")
            .join("global")
            .join("node_modules"),
    ];
    let exe = h
        .background_program()
        .map(String::from)
        .unwrap_or_else(|| format!("{}.exe", h.command));
    let mut args = vec![exe];
    for root in &roots {
        for p in h.platform_packages() {
            args.push(show(
                &p.split('/').fold(root.clone(), |d, part| d.join(part)),
            ));
        }
    }
    // Its vendor install's own folders (Hermes' Python is in %LOCALAPPDATA%\hermes\tools).
    args.extend(footprint(h.id, r).install.iter().map(|p| show(p)));
    Step {
        label: format!("Stop {} in the background", h.name),
        plan: gond("stop", args),
    }
}

fn gond(op: &str, args: Vec<String>) -> Plan {
    Plan {
        source: Source::Gond,
        program: op.into(),
        args,
        from: "Gond".into(),
        adds_to_path: None,
        provides: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A relative path with the platform's separators, so expected paths compare equal.
    fn j(rel: &str) -> PathBuf {
        rel.split('/').collect()
    }

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "x").unwrap();
    }

    fn roots(t: &Path) -> Roots {
        Roots {
            home: t.join("u"),
            local: t.join(j("u/AppData/Local")),
            roaming: t.join(j("u/AppData/Roaming")),
        }
    }

    #[test]
    fn claude_vendor_keeps_settings_unless_everything_and_shares_local_bin() {
        let t = tempfile::tempdir().unwrap();
        let r = roots(t.path());
        let bin = r.home.join(j(".local/bin"));
        touch(&bin.join("claude.exe"));
        touch(&r.home.join(j(".local/share/claude/versions/2.1.314")));
        touch(&r.home.join(j(".claude/settings.json")));
        touch(&r.home.join(".claude.json"));
        let h = crate::harness::get("claude").unwrap();
        let path = vec![show(&bin)];

        let p = plan(h, &r, &path, false);
        assert_eq!(p.methods, ["vendor"]);
        assert_eq!(
            p.deletes,
            [
                show(&bin.join("claude.exe")),
                show(&r.home.join(j(".local/share/claude")))
            ]
        );
        assert_eq!(
            p.path_entries,
            [show(&bin)],
            "nothing else left in ~/.local/bin"
        );

        // Another tool in ~/.local/bin keeps the PATH entry.
        touch(&bin.join("uv.exe"));
        assert!(plan(h, &r, &path, false).path_entries.is_empty());

        let all = plan(h, &r, &path, true);
        for want in [r.home.join(".claude"), r.home.join(".claude.json")] {
            assert!(
                all.deletes.contains(&show(&want)),
                "{want:?} in {:?}",
                all.deletes
            );
        }
        assert!(
            !all.deletes.iter().any(|d| d.ends_with("uv.exe")),
            "never another tool's file"
        );
    }

    /// Cursor's CLI: its folder, its compile cache and its PATH entry; with everything, only
    /// the CLI's own files in ~\.cursor and %APPDATA%\Cursor, which the Cursor editor shares.
    #[test]
    fn cursor_removes_only_the_clis_files() {
        let t = tempfile::tempdir().unwrap();
        let r = roots(t.path());
        touch(&r.local.join(j("cursor-agent/cursor-agent.cmd")));
        touch(&r.local.join(j("cursor-compile-cache/x")));
        touch(&r.home.join(j(".cursor/cli-config.json")));
        touch(&r.home.join(j(".cursor/chats/abc/def/store.db")));
        touch(&r.home.join(j(".cursor/extensions/editor-ext/package.json")));
        touch(&r.roaming.join(j("Cursor/auth.json")));
        touch(&r.roaming.join(j("Cursor/User/settings.json")));
        let h = crate::harness::get("cursor").unwrap();
        let entry = show(&r.local.join("cursor-agent"));

        let p = plan(h, &r, std::slice::from_ref(&entry), false);
        assert_eq!(p.methods, ["vendor"]);
        assert_eq!(
            p.deletes,
            [
                show(&r.local.join("cursor-agent")),
                show(&r.local.join("cursor-compile-cache"))
            ]
        );
        assert_eq!(p.path_entries, std::slice::from_ref(&entry));

        let all = plan(h, &r, &[entry], true);
        for want in [".cursor/cli-config.json", ".cursor/chats"] {
            assert!(all.deletes.contains(&show(&r.home.join(j(want)))), "{want}");
        }
        assert!(all
            .deletes
            .contains(&show(&r.roaming.join(j("Cursor/auth.json")))));
        let editor = [
            r.home.join(j(".cursor/extensions/editor-ext/package.json")),
            r.roaming.join(j("Cursor/User/settings.json")),
        ];
        assert!(
            !all.deletes
                .iter()
                .any(|d| editor.iter().any(|e| e.starts_with(d))),
            "never the editor's files: {:?}",
            all.deletes
        );
    }

    /// Copilot from winget: winget removes it; with everything, ~\.copilot goes too.
    #[test]
    fn copilot_from_winget() {
        let t = tempfile::tempdir().unwrap();
        let r = roots(t.path());
        touch(&r.local.join(j(
            "Microsoft/WinGet/Packages/GitHub.Copilot_Microsoft.Winget.Source_8wekyb3d8bbwe/copilot.exe",
        )));
        touch(&r.home.join(j(".copilot/config.json")));
        let h = crate::harness::get("copilot").unwrap();
        let p = plan(h, &r, &[], false);
        assert_eq!(p.methods, ["winget"]);
        assert!(p.deletes.is_empty());
        assert!(p.steps[0]
            .plan
            .args
            .join(" ")
            .contains("uninstall --id GitHub.Copilot"));
        let all = plan(h, &r, &[], true);
        assert_eq!(all.deletes, [show(&r.home.join(".copilot"))]);
    }

    #[test]
    fn agy_removes_its_folder_and_path_entry_and_only_its_gemini_settings() {
        let t = tempfile::tempdir().unwrap();
        let r = roots(t.path());
        touch(&r.local.join(j("agy/bin/agy.exe")));
        touch(&r.home.join(j(".gemini/antigravity-cli/settings.json")));
        touch(&r.home.join(j(".gemini/other-tool.json")));
        let h = crate::harness::get("agy").unwrap();
        let entry = show(&r.local.join(j("agy/bin")));
        let p = plan(h, &r, std::slice::from_ref(&entry), true);
        assert_eq!(
            p.deletes,
            [
                show(&r.home.join(j(".gemini/antigravity-cli"))),
                show(&r.local.join("agy"))
            ]
        );
        assert_eq!(p.path_entries, [entry]);
        assert_eq!(p.steps.len(), 2);
    }

    /// Pi from Bun: its shims and package go, and ~\.bun\bin comes off PATH once nothing
    /// else is in it; while another Bun tool is there, the entry stays.
    #[test]
    fn bun_bin_leaves_path_with_the_last_bun_tool() {
        let t = tempfile::tempdir().unwrap();
        let r = roots(t.path());
        let bin = r.home.join(j(".bun/bin"));
        touch(&bin.join("pi.exe"));
        touch(&bin.join("pi.bunx"));
        touch(&r.home.join(j(
            ".bun/install/global/node_modules/@earendil-works/pi-coding-agent/package.json",
        )));
        let h = crate::harness::get("pi").unwrap();
        let path = vec![show(&bin)];
        let p = plan(h, &r, &path, false);
        assert_eq!(p.methods, ["bun"]);
        assert!(p.deletes.contains(&show(&bin.join("pi.exe"))));
        assert_eq!(p.path_entries, [show(&bin)]);

        touch(&bin.join("opencode.exe"));
        assert!(plan(h, &r, &path, false).path_entries.is_empty());
    }

    #[test]
    fn winget_and_npm_installs_are_found() {
        let t = tempfile::tempdir().unwrap();
        let r = roots(t.path());
        std::fs::create_dir_all(
            r.local.join(
                "Microsoft/WinGet/Packages/OpenAI.Codex_Microsoft.Winget.Source_8wekyb3d8bbwe",
            ),
        )
        .unwrap();
        touch(&r.roaming.join(j("npm/codex.cmd")));
        touch(
            &r.roaming
                .join(j("npm/node_modules/@openai/codex/package.json")),
        );
        let h = crate::harness::get("codex").unwrap();
        let p = plan(h, &r, &[], false);
        assert_eq!(p.methods, ["winget", "npm"]);
        assert_eq!(
            p.steps[0].plan.args[..3],
            ["uninstall", "--id", "OpenAI.Codex"]
        );
        assert!(p
            .deletes
            .contains(&show(&r.roaming.join(j("npm/node_modules/@openai/codex")))));
        assert!(
            !p.deletes.iter().any(|d| d.ends_with(".codex")),
            "settings stay"
        );
    }

    /// A Bun install of an agent with a native program: its platform package goes too, and
    /// Cline's background hub is stopped first.
    #[test]
    fn cline_from_bun_goes_with_its_program_and_its_hub_stops_first() {
        let t = tempfile::tempdir().unwrap();
        let r = roots(t.path());
        let g = r
            .home
            .join(".bun")
            .join("install")
            .join("global")
            .join("node_modules");
        touch(&r.home.join(".bun").join("bin").join("cline.exe"));
        touch(&g.join("cline").join("package.json"));
        touch(
            &g.join("@cline")
                .join("cli-windows-x64")
                .join("bin")
                .join("cline.exe"),
        );
        touch(
            &r.home
                .join(".cline")
                .join("data")
                .join("settings")
                .join("providers.json"),
        );
        let p = plan(crate::harness::get("cline").unwrap(), &r, &[], false);
        assert_eq!(p.methods, ["bun"]);
        assert_eq!(p.steps[0].plan.program, "stop");
        assert_eq!(p.steps[0].plan.args[0], "cline.exe");
        let gone = |p: &UninstallPlan, at: &Path| p.deletes.contains(&show(at));
        assert!(gone(&p, &g.join("@cline").join("cli-windows-x64")));
        assert!(gone(&p, &g.join("cline")));
        assert!(gone(&p, &r.home.join(".bun").join("bin").join("cline.exe")));
        // Its sign-ins stay unless everything goes.
        assert!(!gone(&p, &r.home.join(".cline")));
        let all = plan(crate::harness::get("cline").unwrap(), &r, &[], true);
        assert!(gone(&all, &r.home.join(".cline")));
    }

    /// Droid's script shares ~\bin: its program goes, and the folder leaves PATH only
    /// when nothing else is in it.
    #[test]
    fn droid_takes_only_its_program_out_of_the_shared_bin() {
        let t = tempfile::tempdir().unwrap();
        let r = roots(t.path());
        let bin = r.home.join("bin");
        touch(&bin.join("droid.exe"));
        touch(&bin.join("other-tool.exe"));
        let path = vec![show(&bin)];
        let p = plan(crate::harness::get("droid").unwrap(), &r, &path, false);
        assert_eq!(p.methods, ["vendor"]);
        assert_eq!(p.deletes, [show(&bin.join("droid.exe"))]);
        assert!(p.path_entries.is_empty());
        std::fs::remove_file(bin.join("other-tool.exe")).unwrap();
        let p = plan(crate::harness::get("droid").unwrap(), &r, &path, false);
        assert_eq!(p.path_entries, [show(&bin)]);
    }

    /// Hermes' gateway (its own Python, running by itself) holds its folder: stopped first,
    /// even when only its settings are left to remove.
    #[test]
    fn hermes_gateway_stops_before_its_folder_goes() {
        let t = tempfile::tempdir().unwrap();
        let r = roots(t.path());
        let home = r.local.join("hermes");
        touch(&home.join("state.db"));
        touch(&home.join("tools").join("python-3.14").join("python.exe"));
        let p = plan(crate::harness::get("hermes").unwrap(), &r, &[], true);
        assert_eq!(p.steps[0].plan.program, "stop");
        assert_eq!(p.steps[0].plan.args[0], "python.exe");
        assert!(p.steps[0].plan.args.contains(&show(&home.join("tools"))));
        assert!(p.deletes.contains(&show(&home)));
        // Nothing of it there: no steps at all.
        let empty = tempfile::tempdir().unwrap();
        let none = plan(
            crate::harness::get("hermes").unwrap(),
            &roots(empty.path()),
            &[],
            true,
        );
        assert!(none.steps.is_empty());
    }

    #[test]
    fn nothing_installed_is_an_empty_plan() {
        let t = tempfile::tempdir().unwrap();
        let p = plan(
            crate::harness::get("grok").unwrap(),
            &roots(t.path()),
            &[],
            false,
        );
        assert!(p.steps.is_empty() && p.deletes.is_empty() && p.methods.is_empty());
    }

    #[test]
    fn a_cache_alone_is_not_a_vendor_install() {
        let t = tempfile::tempdir().unwrap();
        let r = roots(t.path());
        touch(&r.home.join(j(".codex/tmp/arg0/x")));
        let p = plan(crate::harness::get("codex").unwrap(), &r, &[], false);
        assert!(p.methods.is_empty() && p.deletes.is_empty(), "{p:?}");
    }
}
