//! Installs without winget: Git, GitHub CLI and Node from their vendors' own
//! releases when winget is missing or broken (Windows 10 LTSC, Server, a stripped image).
//!
//! Each tool's latest release is read from its vendor's metadata over HTTPS (GitHub's
//! release API for Git for Windows and the GitHub CLI, nodejs.org's index for Node LTS),
//! the file is downloaded with the same stages and bytes a winget install reports, and its
//! SHA-256 is checked against the vendor's published one before anything runs. Then:
//! Git's own installer, silent and per user (`/CURRENTUSER`, no UAC); GitHub CLI's and
//! Node's official zips unpacked under `%LOCALAPPDATA%\Programs` and put on the user PATH.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;
use sha2::{Digest, Sha256};

/// What the step does once the file is verified.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// Run the vendor's installer with these arguments.
    Installer(Vec<String>),
    /// Unpack the zip into `%LOCALAPPDATA%\Programs\<dir>`, without the archive's single
    /// top folder when `strip` (Node's `node-v…-win-x64/`); the program sits in `bin`.
    Zip {
        dir: String,
        bin: String,
        strip: bool,
    },
}

/// One verified download.
#[derive(Debug, Clone, PartialEq)]
pub struct Artifact {
    pub name: String,
    pub version: String,
    pub url: String,
    /// Lowercase hex, from the vendor.
    pub sha256: String,
    pub size: Option<u64>,
    pub kind: Kind,
}

/// The tools this module installs (ids as in `harness::PREREQS`).
pub fn supported(id: &str) -> bool {
    matches!(id, "git" | "gh" | "node")
}

/// The asset in a GitHub release (the API's JSON) whose name `want` accepts, with the
/// SHA-256 GitHub publishes for it (`digest: "sha256:…"`).
pub fn github_asset(
    release: &Value,
    want: impl Fn(&str) -> bool,
) -> Option<(String, String, String, Option<u64>)> {
    release["assets"].as_array()?.iter().find_map(|a| {
        let name = a["name"].as_str()?;
        if !want(name) {
            return None;
        }
        let sha = a["digest"]
            .as_str()?
            .strip_prefix("sha256:")?
            .to_lowercase();
        (sha.len() == 64).then(|| {
            (
                name.to_string(),
                a["browser_download_url"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                sha,
                a["size"].as_u64(),
            )
        })
    })
}

/// The newest LTS version in nodejs.org's `dist/index.json` with a 64-bit Windows zip.
pub fn node_lts(index: &Value) -> Option<String> {
    index.as_array()?.iter().find_map(|r| {
        let lts = !r["lts"].is_null() && r["lts"] != false;
        let zip = r["files"]
            .as_array()
            .is_some_and(|f| f.iter().any(|x| x == "win-x64-zip"));
        (lts && zip)
            .then(|| r["version"].as_str().map(String::from))
            .flatten()
    })
}

/// The SHA-256 for `file` in a `SHASUMS256.txt` (`<hex>  <file>` per line).
pub fn sha_for(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut parts = l.split_whitespace();
        let (sha, name) = (parts.next()?, parts.next()?);
        (name == file && sha.len() == 64).then(|| sha.to_lowercase())
    })
}

/// Where a download may come from: https on the vendors' own hosts, nothing else, so
/// metadata pointing elsewhere can't swap in another file.
pub const HOSTS: &[&str] = &["github.com", "objects.githubusercontent.com", "nodejs.org"];

/// Whether `url` is https on one of [`HOSTS`].
pub fn allowed_url(url: &str) -> bool {
    url.strip_prefix("https://")
        .and_then(|rest| rest.split(['/', '?', '#']).next())
        .is_some_and(|host| {
            !host.contains(['@', ':']) && HOSTS.contains(&host.to_ascii_lowercase().as_str())
        })
}

/// The artifact for `id` from its vendor's metadata, fetched with `get`. Its download
/// URL must pass [`allowed_url`].
pub fn resolve(id: &str, get: &dyn Fn(&str) -> Result<String, String>) -> Result<Artifact, String> {
    resolve_checked(id, get, allowed_url)
}

/// `resolve` with the URL check given (tests serve from 127.0.0.1).
pub fn resolve_checked(
    id: &str,
    get: &dyn Fn(&str) -> Result<String, String>,
    allowed: fn(&str) -> bool,
) -> Result<Artifact, String> {
    let art = resolve_any(id, get)?;
    if !allowed(&art.url) {
        return Err(format!(
            "Gond won't download {} from {}: only https from {} is allowed",
            art.name,
            art.url,
            HOSTS.join(", ")
        ));
    }
    Ok(art)
}

fn resolve_any(id: &str, get: &dyn Fn(&str) -> Result<String, String>) -> Result<Artifact, String> {
    let json = |url: &str| -> Result<Value, String> {
        serde_json::from_str(&get(url)?).map_err(|e| format!("unreadable release metadata: {e}"))
    };
    match id {
        "git" => {
            let r = json("https://api.github.com/repos/git-for-windows/git/releases/latest")?;
            let (name, url, sha256, size) = github_asset(&r, |n| {
                n.starts_with("Git-") && n.ends_with("-64-bit.exe") && !n.contains("-rc")
            })
            .ok_or("Git for Windows' latest release has no 64-bit installer with a checksum")?;
            Ok(Artifact {
                version: r["tag_name"]
                    .as_str()
                    .unwrap_or_default()
                    .trim_start_matches('v')
                    .into(),
                name,
                url,
                sha256,
                size,
                // Inno Setup, per user (`%LOCALAPPDATA%\Programs\Git`), no prompts, no restart.
                kind: Kind::Installer(
                    [
                        "/VERYSILENT",
                        "/SUPPRESSMSGBOXES",
                        "/NORESTART",
                        "/NOCANCEL",
                        "/SP-",
                        "/CURRENTUSER",
                    ]
                    .map(String::from)
                    .into(),
                ),
            })
        }
        "gh" => {
            let r = json("https://api.github.com/repos/cli/cli/releases/latest")?;
            let (name, url, sha256, size) = github_asset(&r, |n| {
                n.starts_with("gh_") && n.ends_with("_windows_amd64.zip")
            })
            .ok_or("GitHub CLI's latest release has no Windows zip with a checksum")?;
            Ok(Artifact {
                version: r["tag_name"]
                    .as_str()
                    .unwrap_or_default()
                    .trim_start_matches('v')
                    .into(),
                name,
                url,
                sha256,
                size,
                kind: Kind::Zip {
                    dir: "gh".into(),
                    bin: "bin".into(),
                    strip: false,
                },
            })
        }
        "node" => {
            let version = node_lts(&json("https://nodejs.org/dist/index.json")?)
                .ok_or("nodejs.org lists no LTS release for 64-bit Windows")?;
            let name = format!("node-{version}-win-x64.zip");
            let sums = get(&format!("https://nodejs.org/dist/{version}/SHASUMS256.txt"))?;
            let sha256 =
                sha_for(&sums, &name).ok_or("nodejs.org publishes no checksum for that zip")?;
            Ok(Artifact {
                url: format!("https://nodejs.org/dist/{version}/{name}"),
                version: version.trim_start_matches('v').into(),
                name,
                sha256,
                size: None,
                kind: Kind::Zip {
                    dir: "nodejs".into(),
                    bin: String::new(),
                    strip: true,
                },
            })
        }
        other => Err(format!("Gond can't install {other} without winget")),
    }
}

/// Where unpacked tools live: `%LOCALAPPDATA%\Programs\<dir>`.
pub fn programs_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("Programs"))
}

/// What a download reports as it goes.
pub trait OnBytes {
    fn bytes(&mut self, done: u64, total: Option<u64>);
}

/// Streams `from` to `to`, hashing as it goes; returns the SHA-256 (lowercase hex).
/// Reports at most every 100 ms; stops with an error once `stop` says so.
pub fn copy_hashed(
    mut from: impl Read,
    to: &Path,
    total: Option<u64>,
    on: &mut dyn OnBytes,
    stop: &dyn Fn() -> bool,
) -> Result<String, String> {
    let mut out =
        std::fs::File::create(to).map_err(|e| format!("couldn't save the download: {e}"))?;
    let (mut hash, mut done, mut buf) = (Sha256::new(), 0u64, vec![0u8; 64 * 1024]);
    let mut last = Instant::now() - Duration::from_secs(1);
    loop {
        if stop() {
            return Err("Cancelled".into());
        }
        let n = from
            .read(&mut buf)
            .map_err(|e| format!("the download broke off: {e}"))?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        out.write_all(&buf[..n])
            .map_err(|e| format!("couldn't save the download: {e}"))?;
        done += n as u64;
        if last.elapsed() >= Duration::from_millis(100) {
            on.bytes(done, total);
            last = Instant::now();
        }
    }
    on.bytes(done, total.or(Some(done)));
    out.flush()
        .map_err(|e| format!("couldn't save the download: {e}"))?;
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Unpacks `zip` into `dest` (replacing it). With `strip`, the archive's single top
/// folder is dropped (`node-v22-win-x64/node.exe` → `dest/node.exe`). Paths that would
/// leave `dest` are refused.
pub fn unzip(zip: &Path, dest: &Path, strip: bool) -> Result<(), String> {
    let file = std::fs::File::open(zip).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("not a zip: {e}"))?;
    let top = if strip {
        let first = archive
            .by_index(0)
            .ok()
            .and_then(|e| e.enclosed_name())
            .and_then(|n| n.components().next().map(|c| c.as_os_str().to_owned()));
        let all_under = (0..archive.len()).all(|i| {
            archive
                .by_index(i)
                .ok()
                .and_then(|e| e.enclosed_name())
                .zip(first.as_ref())
                .is_some_and(|(n, t)| n.starts_with(t))
        });
        first.filter(|_| all_under)
    } else {
        None
    };
    let staging = dest.with_extension("new");
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(name) = entry.enclosed_name() else {
            return Err(format!("the zip has an unsafe path: {:?}", entry.name()));
        };
        let rel = match &top {
            Some(t) => name.strip_prefix(t).unwrap_or(&name).to_path_buf(),
            None => name,
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let path = staging.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
    }
    let _ = std::fs::remove_dir_all(dest);
    std::fs::rename(&staging, dest).map_err(|e| format!("couldn't put it in place: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_vendor_metadata() {
        let release = json!({ "tag_name": "v2.103.3", "assets": [
            { "name": "gh_2.103.3_windows_arm64.zip", "digest": format!("sha256:{}", "a".repeat(64)), "browser_download_url": "u1" },
            { "name": "gh_2.103.3_windows_amd64.zip", "digest": format!("sha256:{}", "B".repeat(64)), "browser_download_url": "u2", "size": 12 },
            { "name": "gh_2.103.3_windows_amd64.msi", "digest": null, "browser_download_url": "u3" },
        ]});
        let (name, url, sha, size) =
            github_asset(&release, |n| n.ends_with("_windows_amd64.zip")).unwrap();
        assert_eq!(
            (name.as_str(), url.as_str(), size),
            ("gh_2.103.3_windows_amd64.zip", "u2", Some(12))
        );
        assert_eq!(sha, "b".repeat(64));
        assert!(
            github_asset(&release, |n| n.ends_with(".msi")).is_none(),
            "no checksum, no install"
        );

        let index = json!([
            { "version": "v27.0.0", "lts": false, "files": ["win-x64-zip"] },
            { "version": "v26.13.0", "lts": "Lithium", "files": ["linux-x64", "win-x64-zip"] },
            { "version": "v24.1.0", "lts": "Jod", "files": ["win-x64-zip"] },
        ]);
        assert_eq!(node_lts(&index).as_deref(), Some("v26.13.0"));
        let sums = format!(
            "{}  node-v26.13.0-x64.msi\n{}  node-v26.13.0-win-x64.zip\n",
            "1".repeat(64),
            "2".repeat(64)
        );
        assert_eq!(
            sha_for(&sums, "node-v26.13.0-win-x64.zip"),
            Some("2".repeat(64))
        );
        assert_eq!(sha_for(&sums, "missing.zip"), None);
    }

    #[test]
    fn resolves_each_tool_from_its_vendor() {
        let get = |url: &str| -> Result<String, String> {
            Ok(match url {
                u if u.contains("git-for-windows") => json!({ "tag_name": "v2.56.0.windows.4", "assets": [
                    { "name": "Git-2.56.0.4-64-bit.exe", "digest": format!("sha256:{}", "c".repeat(64)), "browser_download_url": "https://github.com/g.exe" },
                    { "name": "Git-2.56.0.4-64-bit.tar.bz2", "digest": format!("sha256:{}", "d".repeat(64)), "browser_download_url": "x" } ] }).to_string(),
                u if u.contains("cli/cli") => json!({ "tag_name": "v2.103.3", "assets": [
                    { "name": "gh_2.103.3_windows_amd64.zip", "digest": format!("sha256:{}", "e".repeat(64)), "browser_download_url": "https://github.com/gh.zip" } ] }).to_string(),
                u if u.ends_with("index.json") => json!([{ "version": "v26.13.0", "lts": "Lithium", "files": ["win-x64-zip"] }]).to_string(),
                u if u.ends_with("SHASUMS256.txt") => format!("{}  node-v26.13.0-win-x64.zip\n", "f".repeat(64)),
                other => return Err(format!("unexpected {other}")),
            })
        };
        let git = resolve("git", &get).unwrap();
        assert_eq!(
            (git.name.as_str(), git.version.as_str()),
            ("Git-2.56.0.4-64-bit.exe", "2.56.0.windows.4")
        );
        assert!(matches!(&git.kind, Kind::Installer(a) if a.contains(&"/CURRENTUSER".to_string())));
        let gh = resolve("gh", &get).unwrap();
        assert_eq!(
            gh.kind,
            Kind::Zip {
                dir: "gh".into(),
                bin: "bin".into(),
                strip: false
            }
        );
        let node = resolve("node", &get).unwrap();
        assert_eq!(
            node.url,
            "https://nodejs.org/dist/v26.13.0/node-v26.13.0-win-x64.zip"
        );
        assert_eq!(node.sha256, "f".repeat(64));
        assert!(resolve("bun", &get).is_err());
    }

    #[test]
    fn downloads_only_from_the_vendors_hosts_over_https() {
        for ok in [
            "https://github.com/cli/cli/releases/download/v2/gh.zip",
            "https://objects.githubusercontent.com/x",
            "https://nodejs.org/dist/v26/node.zip",
        ] {
            assert!(allowed_url(ok), "{ok}");
        }
        for bad in [
            "http://github.com/x",
            "https://evil.example/x",
            "https://github.com.evil.example/x",
            "https://user@github.com/x",
            "https://github.com:8443/x",
            "ftp://nodejs.org/x",
        ] {
            assert!(!allowed_url(bad), "{bad}");
        }
        let get = |_: &str| -> Result<String, String> {
            Ok(
                json!({ "tag_name": "v1", "assets": [{ "name": "gh_1_windows_amd64.zip",
                "digest": format!("sha256:{}", "a".repeat(64)),
                "browser_download_url": "https://mirror.example/gh.zip" }] })
                .to_string(),
            )
        };
        let e = resolve("gh", &get).unwrap_err();
        assert!(e.contains("only https from"), "{e}");
    }

    struct Seen(Vec<(u64, Option<u64>)>);
    impl OnBytes for Seen {
        fn bytes(&mut self, done: u64, total: Option<u64>) {
            self.0.push((done, total));
        }
    }

    #[test]
    fn downloads_hash_and_unzip() {
        let d = tempfile::tempdir().unwrap();
        let data = vec![7u8; 300_000];
        let mut seen = Seen(vec![]);
        let sha = copy_hashed(
            &data[..],
            &d.path().join("f"),
            Some(300_000),
            &mut seen,
            &|| false,
        )
        .unwrap();
        let want: String = Sha256::digest(&data)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(sha, want);
        assert_eq!(seen.0.last(), Some(&(300_000, Some(300_000))));
        assert!(copy_hashed(&data[..], &d.path().join("g"), None, &mut seen, &|| true).is_err());

        // A zip with one top folder, like Node's: unpacked without it.
        let zpath = d.path().join("n.zip");
        {
            let mut z = zip::ZipWriter::new(std::fs::File::create(&zpath).unwrap());
            let o = zip::write::SimpleFileOptions::default();
            z.add_directory("node-v1-win-x64/", o).unwrap();
            z.start_file("node-v1-win-x64/node.exe", o).unwrap();
            z.write_all(b"exe").unwrap();
            z.start_file("node-v1-win-x64/lib/x.js", o).unwrap();
            z.write_all(b"js").unwrap();
            z.finish().unwrap();
        }
        let dest = d.path().join("nodejs");
        std::fs::create_dir_all(dest.join("old")).unwrap();
        unzip(&zpath, &dest, true).unwrap();
        assert_eq!(std::fs::read(dest.join("node.exe")).unwrap(), b"exe");
        assert!(dest.join("lib/x.js").is_file());
        assert!(!dest.join("old").exists(), "replaced, not merged");

        // gh's zip has bin/ at its root: kept as is.
        let gz = d.path().join("gh.zip");
        {
            let mut z = zip::ZipWriter::new(std::fs::File::create(&gz).unwrap());
            let o = zip::write::SimpleFileOptions::default();
            z.start_file("bin/gh.exe", o).unwrap();
            z.write_all(b"gh").unwrap();
            z.start_file("LICENSE", o).unwrap();
            z.write_all(b"l").unwrap();
            z.finish().unwrap();
        }
        unzip(&gz, &d.path().join("gh"), false).unwrap();
        assert!(d.path().join("gh/bin/gh.exe").is_file());
    }
}
