//! SSH keys on this PC (Settings → Machines → SSH keys): the key pairs in `~/.ssh`, which
//! machines use each (their `IdentityFile` in Gond's ssh config, `crate::sshconfig`), and keys
//! made here. A key is made in OpenSSH's own format, with a passphrase encrypting it as
//! ssh-keygen does, and its private half readable by you alone (Windows' OpenSSH refuses a key
//! others can read). Putting one on a machine is `crate::remote::Hub::authorize`.

use std::path::{Path, PathBuf};

use ssh_key::{Algorithm, HashAlg, LineEnding, PrivateKey, PublicKey};

use crate::error::{AppError, AppResult};

/// A key pair in `~/.ssh`.
#[derive(Debug, Clone, PartialEq)]
pub struct Key {
    /// The private key's file name (`id_ed25519`); the public one adds `.pub`.
    pub name: String,
    pub path: PathBuf,
    /// `ED25519`, `RSA`, `ECDSA`…
    pub kind: String,
    /// `SHA256:…`, as `ssh-keygen -l` shows it.
    pub fingerprint: String,
    pub comment: String,
    /// Whether the private key has a passphrase.
    pub encrypted: bool,
    /// The public key's line, for a machine's `authorized_keys`.
    pub public: String,
    /// One ssh tries by itself (`id_ed25519`, `id_rsa`…).
    pub default: bool,
}

/// The names ssh tries without being told.
const DEFAULTS: [&str; 6] = [
    "id_ed25519",
    "id_ecdsa",
    "id_rsa",
    "id_ed25519_sk",
    "id_ecdsa_sk",
    "id_dsa",
];

pub fn ssh_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".ssh"))
}

fn kind(a: &Algorithm) -> String {
    match a {
        Algorithm::Ed25519 => "ED25519".into(),
        Algorithm::Rsa { .. } => "RSA".into(),
        Algorithm::Ecdsa { .. } => "ECDSA".into(),
        Algorithm::Dsa => "DSA".into(),
        Algorithm::SkEd25519 => "ED25519-SK".into(),
        Algorithm::SkEcdsaSha2NistP256 => "ECDSA-SK".into(),
        other => other.as_str().to_uppercase(),
    }
}

/// The key whose public half is `pub_file`, if its private half is beside it.
fn read(pub_file: &Path) -> Option<Key> {
    let name = pub_file.file_stem()?.to_string_lossy().into_owned();
    let private = pub_file.with_extension("");
    if !private.is_file() {
        return None;
    }
    let public = PublicKey::from_openssh(std::fs::read_to_string(pub_file).ok()?.trim()).ok()?;
    let secret = std::fs::read_to_string(&private).unwrap_or_default();
    // Older PEM keys (`BEGIN RSA PRIVATE KEY`) say in their header when they're encrypted.
    let encrypted = PrivateKey::from_openssh(&secret)
        .map(|k| k.is_encrypted())
        .unwrap_or_else(|_| secret.contains("ENCRYPTED"));
    Some(Key {
        default: DEFAULTS.contains(&name.as_str()),
        kind: kind(&public.algorithm()),
        fingerprint: public.fingerprint(HashAlg::Sha256).to_string(),
        comment: public.comment().to_string(),
        public: public.to_openssh().ok()?,
        encrypted,
        path: private,
        name,
    })
}

/// The key pairs in `dir`: each `<name>.pub` with its `<name>` beside it. Others (PuTTY's
/// `.ppk`, a public key alone) aren't listed.
pub fn list_in(dir: &Path) -> Vec<Key> {
    let mut keys: Vec<Key> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "pub") && p.is_file())
        .filter_map(|p| read(&p))
        .collect();
    keys.sort_by(|a, b| a.name.cmp(&b.name));
    keys
}

/// Your SSH keys.
pub fn list() -> Vec<Key> {
    ssh_dir().map(|d| list_in(&d)).unwrap_or_default()
}

/// The key named `name` in `~/.ssh`.
pub fn find(name: &str) -> AppResult<Key> {
    list()
        .into_iter()
        .find(|k| k.name == name)
        .ok_or_else(|| AppError::NotFound(format!("no SSH key {name}")))
}

/// A file name for a key: letters, digits, `.`, `_` and `-`, not starting with `.` or `-`.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with(['.', '-'])
        && !name.ends_with(".pub")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// The type of key to make.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Make {
    Ed25519,
    /// 4096 bits, for servers too old for ED25519.
    Rsa,
}

/// Makes a key pair in `dir` as `<name>` and `<name>.pub`, never over a file that's there.
pub fn create_in(
    dir: &Path,
    name: &str,
    make: Make,
    passphrase: Option<&str>,
    comment: &str,
) -> AppResult<Key> {
    if !valid_name(name) {
        return Err(AppError::Invalid(
            "a key name of letters, digits, '.', '_' or '-'".into(),
        ));
    }
    let private = dir.join(name);
    let public = dir.join(format!("{name}.pub"));
    if private.exists() || public.exists() {
        return Err(AppError::Invalid(format!(
            "~/.ssh/{name} is there already: pick another name"
        )));
    }
    let failed = |e: ssh_key::Error| AppError::Invalid(format!("couldn't make the key: {e}"));
    let mut rng = ssh_key::rand_core::OsRng;
    let algorithm = match make {
        Make::Ed25519 => Algorithm::Ed25519,
        Make::Rsa => Algorithm::Rsa { hash: None },
    };
    let mut key = PrivateKey::random(&mut rng, algorithm).map_err(failed)?;
    key.set_comment(comment);
    if let Some(p) = passphrase.filter(|p| !p.is_empty()) {
        key = key.encrypt(&mut rng, p).map_err(failed)?;
    }
    let secret = key.to_openssh(LineEnding::LF).map_err(failed)?;
    let line = key.public_key().to_openssh().map_err(failed)?;
    std::fs::create_dir_all(dir)?;
    write_private(&private, secret.as_bytes())?;
    if let Err(e) = std::fs::write(&public, format!("{line}\n")) {
        let _ = std::fs::remove_file(&private);
        return Err(e.into());
    }
    read(&public).ok_or_else(|| AppError::Invalid("the new key didn't read back".into()))
}

/// Makes a key in `~/.ssh`, its comment `you@this-pc` as ssh-keygen's.
pub fn create(name: &str, make: Make, passphrase: Option<&str>) -> AppResult<Key> {
    let dir = ssh_dir().ok_or_else(|| AppError::Invalid("no home folder".into()))?;
    let var = |k: &str| std::env::var(k).unwrap_or_default();
    let user = if cfg!(windows) {
        var("USERNAME")
    } else {
        var("USER")
    };
    let host = if cfg!(windows) {
        var("COMPUTERNAME")
    } else {
        std::fs::read_to_string("/etc/hostname").unwrap_or_default()
    };
    let comment = format!("{user}@{}", host.trim().to_lowercase());
    create_in(&dir, name, make, passphrase, &comment)
}

/// Writes a private key that only you (and SYSTEM) can open, as a new file.
#[cfg(windows)]
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
    };

    let sid = crate::pty::host::win::user_sid()
        .ok_or_else(|| std::io::Error::other("couldn't tell who you are"))?;
    let mut sec = crate::pty::host::win::Security::only(&sid)?;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: a NUL-terminated path and a security descriptor alive for the call; the handle
    // becomes the File's, which closes it.
    let file = unsafe {
        let h = CreateFileW(
            wide.as_ptr(),
            FILE_GENERIC_READ | FILE_GENERIC_WRITE,
            0,
            sec.as_ptr().cast(),
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        );
        if h == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error());
        }
        std::fs::File::from_raw_handle(h)
    };
    let mut file = file;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(not(windows))]
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(bytes)?;
    f.sync_all()
}

/// Deletes a key pair: to the Recycle Bin on Windows, so it can be had back.
pub fn delete(key: &Key) -> AppResult<()> {
    let public = key.path.with_extension("pub");
    #[cfg(windows)]
    {
        to_recycle_bin(&[&key.path, &public])?;
    }
    #[cfg(not(windows))]
    {
        std::fs::remove_file(&key.path)?;
        let _ = std::fs::remove_file(&public);
    }
    Ok(())
}

#[cfg(windows)]
fn to_recycle_bin(paths: &[&Path]) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::{
        SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FO_DELETE,
        SHFILEOPSTRUCTW,
    };
    // Each path NUL-terminated, the list ended by another NUL.
    let mut from: Vec<u16> = vec![];
    for p in paths.iter().filter(|p| p.exists()) {
        from.extend(p.as_os_str().encode_wide());
        from.push(0);
    }
    if from.is_empty() {
        return Ok(());
    }
    from.push(0);
    let mut op = SHFILEOPSTRUCTW {
        hwnd: std::ptr::null_mut(),
        wFunc: FO_DELETE,
        pFrom: from.as_ptr(),
        pTo: std::ptr::null(),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT) as u16,
        fAnyOperationsAborted: 0,
        hNameMappings: std::ptr::null_mut(),
        lpszProgressTitle: std::ptr::null(),
    };
    // SAFETY: `from` is a double-NUL-terminated list alive for the call.
    let r = unsafe { SHFileOperationW(&mut op) };
    if r != 0 || op.fAnyOperationsAborted != 0 {
        return Err(std::io::Error::other(format!(
            "couldn't move the key to the Recycle Bin ({r})"
        )));
    }
    Ok(())
}

/// The file an `IdentityFile` value names: `~/` from home, a relative one from `~/.ssh`.
pub fn identity_path(value: &str) -> Option<PathBuf> {
    let v = value.trim().trim_matches('"').replace('\\', "/");
    let home = dirs::home_dir()?;
    Some(if let Some(rest) = v.strip_prefix("~/") {
        home.join(rest)
    } else if let Some(rest) = v.strip_prefix("%d/") {
        home.join(rest)
    } else if Path::new(&v).is_absolute() {
        PathBuf::from(v)
    } else {
        home.join(".ssh").join(v)
    })
}

/// Whether two paths are the same file's, as Windows compares them (any case, either slash).
pub fn same_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        let s = p.to_string_lossy().replace('\\', "/");
        if cfg!(windows) {
            s.to_lowercase()
        } else {
            s
        }
    };
    norm(a) == norm(b)
}

/// The `IdentityFile` value for a key in `~/.ssh`.
pub fn identity_value(key: &Key) -> String {
    format!("~/.ssh/{}", key.name)
}

/// A key's `authorized_keys` line: type, key and a comment kept to plain characters, so it
/// goes through a shell's quotes untouched.
pub fn authorized_line(key: &Key) -> String {
    let mut parts = key.public.split_whitespace();
    let (t, b) = (
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
    );
    let comment: String = key
        .comment
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | '-'))
        .collect();
    if comment.is_empty() {
        format!("{t} {b}")
    } else {
        format!("{t} {b} {comment}")
    }
}

/// A key's base64 part: what says it's the same key on any line.
pub fn blob(key: &Key) -> String {
    key.public
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_made_read_back_and_listed() {
        let d = tempfile::tempdir().unwrap();
        let k = create_in(d.path(), "gond-laptop", Make::Ed25519, None, "me@pc").unwrap();
        assert_eq!(k.kind, "ED25519");
        assert!(k.fingerprint.starts_with("SHA256:"));
        assert!(!k.encrypted && !k.default);
        assert!(k.public.starts_with("ssh-ed25519 ") && k.public.ends_with(" me@pc"));
        // What ssh reads: the private key opens without a passphrase and matches the public one.
        let secret = std::fs::read_to_string(d.path().join("gond-laptop")).unwrap();
        let p = PrivateKey::from_openssh(&secret).unwrap();
        assert_eq!(p.public_key().to_openssh().unwrap(), k.public);
        assert_eq!(authorized_line(&k), k.public);

        // Never over a key that's there.
        assert!(create_in(d.path(), "gond-laptop", Make::Ed25519, None, "x").is_err());
        assert!(create_in(d.path(), "../evil", Make::Ed25519, None, "x").is_err());

        // With a passphrase it's encrypted, and opens with it alone.
        let e = create_in(d.path(), "id_ed25519", Make::Ed25519, Some("pw"), "me@pc").unwrap();
        assert!(e.encrypted && e.default);
        let secret = std::fs::read_to_string(d.path().join("id_ed25519")).unwrap();
        let p = PrivateKey::from_openssh(&secret).unwrap();
        assert!(p.decrypt("nope").is_err());
        assert!(p.decrypt("pw").is_ok());

        // A public key alone isn't a key pair.
        std::fs::write(d.path().join("lone.pub"), &k.public).unwrap();
        let names: Vec<_> = list_in(d.path()).into_iter().map(|k| k.name).collect();
        assert_eq!(names, ["gond-laptop", "id_ed25519"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_private_key_is_yours_alone() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        create_in(d.path(), "k", Make::Ed25519, None, "c").unwrap();
        let mode = std::fs::metadata(d.path().join("k"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_comment_goes_through_quotes_untouched() {
        let k = Key {
            name: "k".into(),
            path: PathBuf::from("k"),
            kind: "ED25519".into(),
            fingerprint: String::new(),
            comment: "it's \"me\" $HOME".into(),
            encrypted: false,
            public: "ssh-ed25519 AAAAC3Nza it's".into(),
            default: false,
        };
        assert_eq!(authorized_line(&k), "ssh-ed25519 AAAAC3Nza itsmeHOME");
        assert_eq!(blob(&k), "AAAAC3Nza");
    }
}
