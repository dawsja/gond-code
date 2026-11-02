//! `machine.*`: the machines a terminal can run on besides this PC (WSL distributions, SSH
//! machines you add), connecting to them, and answering ssh's questions while it signs in
//! (`crate::remote`). SSH machines are `Host`s in Gond's own ssh config, which `~/.ssh/config`
//! includes (`crate::sshconfig`). Each connection's state is in the tree at `/machines/<id>`; a question
//! ssh asks waits at `/askpass/<id>`.
//!
//! `machine.key.*`: your SSH keys (`crate::sshkeys`), which machine signs in with which (its
//! `IdentityFile` there), and putting a key on a machine or taking it off.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::registry::{Def, NoInput, Registry, Risk};
use super::Services;
use crate::error::{AppError, AppResult};
use crate::sshconfig::{self, Entry};
use crate::sshkeys::{self, Key};

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Machine {
    /// `wsl:<distro>` or `ssh:<host>`: a pane's `shell` to open there.
    pub id: String,
    /// `wsl` or `ssh`.
    pub kind: String,
    pub name: String,
    /// `user@host:port` for an SSH machine Gond's ssh config sets up.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// The key an SSH machine signs in with (its `IdentityFile`): a key's name in `~/.ssh`,
    /// else the file's name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct KeyInfo {
    /// Its file name in `~/.ssh` (the public one adds `.pub`).
    pub name: String,
    pub path: String,
    /// `ED25519`, `RSA`, `ECDSA`…
    pub kind: String,
    /// `SHA256:…`.
    pub fingerprint: String,
    pub comment: String,
    /// Whether it has a passphrase.
    pub passphrase: bool,
    /// Its public line, to copy or put in `authorized_keys`.
    pub public_key: String,
    /// One ssh tries by itself (`id_ed25519`…).
    pub default: bool,
    /// The SSH machines that sign in with it.
    pub used_by: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Keys {
    pub keys: Vec<KeyInfo>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateKeyInput {
    /// Its file name in `~/.ssh`: letters, digits, `.`, `_`, `-`.
    pub name: String,
    /// `ed25519` (the default) or `rsa` (4096 bits, for very old servers).
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// Encrypts the private key; ssh asks for it when the key is used.
    pub passphrase: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KeyInput {
    /// A key's name in `~/.ssh`.
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MachineKeyInput {
    /// An SSH machine (`ssh:<host>`).
    pub id: String,
    /// A key's name in `~/.ssh`.
    pub key: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UseKeyInput {
    /// An SSH machine (`ssh:<host>`).
    pub id: String,
    /// A key's name in `~/.ssh`; none leaves the choice to ssh.
    pub key: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Authorized {
    /// `host` (Gond's terminal host there did it) or `ssh` (a one-off ssh, for an older host).
    pub via: String,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Machines {
    pub machines: Vec<Machine>,
    /// `Host`s in your ssh config (and the files it includes) not added yet.
    pub ssh_config: Vec<String>,
    /// Gond's ssh config, where the SSH machines are (`~/.ssh/gond/config`).
    pub config_file: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AddInput {
    /// A `Host` from `~/.ssh/config`, a host name, an address, or `user@host`.
    pub host: String,
    pub port: Option<u16>,
    /// Its `Host` name (`ssh <name>`); else the host's. A host your ssh config sets up keeps
    /// its own.
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditInput {
    /// An SSH machine Gond's ssh config sets up (`ssh:<host>`).
    pub id: String,
    /// Where it is now: a host name, an address, or `user@host`.
    pub host: String,
    pub port: Option<u16>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdInput {
    /// A machine (`wsl:<distro>`, `ssh:<host>`).
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnswerInput {
    /// The question (`/askpass/<id>`).
    pub id: String,
    /// What to tell ssh; none cancels signing in.
    pub answer: Option<String>,
}

/// The key in `keys` an `IdentityFile` value names.
fn key_for<'a>(identity: &str, keys: &'a [Key]) -> Option<&'a Key> {
    let p = sshkeys::identity_path(identity)?;
    keys.iter().find(|k| sshkeys::same_path(&k.path, &p))
}

fn machine(e: &Entry, keys: &[Key]) -> Machine {
    Machine {
        id: format!("ssh:{}", e.alias),
        kind: "ssh".into(),
        name: e.alias.clone(),
        address: e.address(),
        key: e.identity.as_deref().map(|i| match key_for(i, keys) {
            Some(k) => k.name.clone(),
            None => i.rsplit(['/', '\\']).next().unwrap_or(i).to_string(),
        }),
    }
}

/// An SSH machine's `Host`.
fn alias(id: &str) -> AppResult<&str> {
    id.strip_prefix("ssh:")
        .ok_or_else(|| AppError::Invalid("only an SSH machine signs in with a key".into()))
}

/// The machines in Gond's ssh config that sign in with `key`.
fn users_of(key: &Key) -> Vec<Entry> {
    let keys = std::slice::from_ref(key);
    sshconfig::list()
        .into_iter()
        .filter(|e| {
            e.identity
                .as_deref()
                .and_then(|i| key_for(i, keys))
                .is_some()
        })
        .collect()
}

/// A host and user someone typed, checked: no spaces, nothing ssh would take for an option.
fn checked(host: &str) -> AppResult<(Option<String>, String)> {
    let (user, host) = split_user(host.trim());
    if host.is_empty() || host.contains(char::is_whitespace) || host.starts_with('-') {
        return Err(AppError::Invalid("a host name or address".into()));
    }
    if user
        .as_deref()
        .is_some_and(|u| u.contains(char::is_whitespace) || u.starts_with('-'))
    {
        return Err(AppError::Invalid("a user name".into()));
    }
    Ok((user, host))
}

/// `user@host` split; a plain host has no user.
fn split_user(host: &str) -> (Option<String>, String) {
    match host.rsplit_once('@') {
        Some((u, h)) if !u.is_empty() && !h.is_empty() => (Some(u.to_string()), h.to_string()),
        _ => (None, host.to_string()),
    }
}

pub fn register(r: &mut Registry<Services>) {
    r.add(
        Def::new(
            "machine.list",
            "The machines a terminal can run on besides this PC: each WSL distribution, the SSH machines you added, and the hosts in ~/.ssh/config not added yet",
            Risk::Read,
        ),
        |_, _: NoInput| -> AppResult<Machines> {
            let mut machines: Vec<Machine> = crate::shells::list()
                .into_iter()
                .filter(|s| s.id.starts_with("wsl:"))
                .map(|s| Machine {
                    name: s.id.trim_start_matches("wsl:").to_string(),
                    id: s.id,
                    kind: "wsl".into(),
                    address: None,
                    key: None,
                })
                .collect();
            let added = sshconfig::list();
            let keys = sshkeys::list();
            machines.extend(added.iter().map(|e| machine(e, &keys)));
            let ssh_config = sshconfig::hosts()
                .into_iter()
                .filter(|h| !added.iter().any(|e| &e.alias == h))
                .collect();
            Ok(Machines {
                machines,
                ssh_config,
                config_file: sshconfig::shown_path(),
            })
        },
    );
    r.add(
        Def::new(
            "machine.add",
            "Add an SSH machine (a Host from your ssh config, a host name or user@host) for terminals to run on: a Host in Gond's ssh config (~/.ssh/gond/config), which ~/.ssh/config includes",
            Risk::WriteLocal,
        )
        .human_only(),
        |_, i: AddInput| -> AppResult<Machine> {
            let (user, host) = checked(&i.host)?;
            let port = i.port.filter(|p| *p != 22);
            let added = sshconfig::list();
            let known = sshconfig::hosts();
            let (e, note) = if user.is_none()
                && port.is_none()
                && known.contains(&host)
                && !added.iter().any(|e| e.alias == host)
            {
                // A host your ssh config sets up: Gond's Host only names it.
                let e = Entry {
                    alias: host,
                    ..Entry::default()
                };
                (e, Some("Set up in your ssh config."))
            } else {
                let name = i.name.as_deref().map(str::trim).filter(|n| !n.is_empty());
                let alias = sshconfig::alias_of(name.unwrap_or(&host));
                if alias.is_empty() {
                    return Err(AppError::Invalid(
                        "a name of letters, digits, '.', '_' or '-'".into(),
                    ));
                }
                if added.iter().any(|e| e.alias == alias) {
                    return Err(AppError::Invalid(format!("{alias} is added already")));
                }
                if known.contains(&alias) {
                    return Err(AppError::Invalid(format!(
                        "Your ssh config has a Host {alias} already: pick it, or name this one otherwise"
                    )));
                }
                let e = Entry {
                    alias,
                    host_name: Some(host),
                    user,
                    port,
                    identity: None,
                };
                (e, None)
            };
            sshconfig::add(&e, note)?;
            Ok(machine(&e, &sshkeys::list()))
        },
    );
    r.add(
        Def::new(
            "machine.edit",
            "Change where an SSH machine is (its HostName, User and Port in Gond's ssh config); a connection to it starts over",
            Risk::WriteLocal,
        )
        .human_only(),
        |call, i: EditInput| -> AppResult<Machine> {
            let a = alias(&i.id)?;
            let e = sshconfig::find(a)
                .ok_or_else(|| AppError::NotFound(format!("no machine {}", i.id)))?;
            if e.host_name.is_none() {
                return Err(AppError::Invalid(format!(
                    "{a} is set up in your ssh config: change it there"
                )));
            }
            let (user, host) = checked(&i.host)?;
            let port = i.port.filter(|p| *p != 22);
            sshconfig::set_address(a, &host, user.as_deref(), port)?;
            let now = sshconfig::find(a)
                .ok_or_else(|| AppError::NotFound(format!("no machine {}", i.id)))?;
            // Somewhere else now: the next connection goes there (its terminals stay where
            // they were).
            if now.address() != e.address() {
                if let Ok(h) = call.ctx.ptys.hub() {
                    h.disconnect(&i.id);
                }
            }
            Ok(machine(&now, &sshkeys::list()))
        },
    );
    r.add(
        Def::new(
            "machine.remove",
            "Remove an SSH machine from Gond: its Host leaves Gond's ssh config (its terminals there are left running)",
            Risk::Destructive,
        )
        .human_only(),
        |call, i: IdInput| -> AppResult<()> {
            let alias = i
                .id
                .strip_prefix("ssh:")
                .ok_or_else(|| AppError::Invalid("only an SSH machine can be removed".into()))?;
            sshconfig::remove(alias)?;
            if let Ok(h) = call.ctx.ptys.hub() {
                h.forget(&i.id);
            }
            Ok(())
        },
    );
    r.add(
        Def::new(
            "machine.connect",
            "Connect to a machine (WSL or SSH): Gond's helper is copied there once, and its terminals come back",
            Risk::Outward,
        ),
        |call, i: IdInput| -> AppResult<()> { call.ctx.ptys.hub()?.connect(&i.id) },
    );
    r.add(
        Def::new(
            "machine.disconnect",
            "Drop the connection to a machine; its terminals keep running there",
            Risk::WriteLocal,
        ),
        |call, i: IdInput| -> AppResult<()> {
            call.ctx.ptys.hub()?.disconnect(&i.id);
            Ok(())
        },
    );
    r.add(
        Def::new(
            "machine.closeAll",
            "Close every terminal on a machine and stop Gond's terminal host there",
            Risk::Destructive,
        )
        .human_only(),
        |call, i: IdInput| -> AppResult<()> { call.ctx.ptys.hub()?.close_all(&i.id) },
    );
    r.add(
        Def::new(
            "machine.askpass.answer",
            "Answer what ssh asked while signing in (a passphrase, a password, a new host key); no answer cancels",
            Risk::Outward,
        )
        .human_only()
        .redact(&["/answer"]),
        |call, i: AnswerInput| -> AppResult<()> { call.ctx.ptys.hub()?.answer(&i.id, i.answer) },
    );
    r.add(
        Def::new(
            "machine.key.list",
            "Your SSH keys (the key pairs in ~/.ssh) and the SSH machines that sign in with each",
            Risk::Read,
        ),
        |_, _: NoInput| -> AppResult<Keys> {
            let keys = sshkeys::list();
            let machines = sshconfig::list();
            Ok(Keys {
                keys: keys
                    .iter()
                    .map(|k| KeyInfo {
                        name: k.name.clone(),
                        path: k.path.to_string_lossy().into_owned(),
                        kind: k.kind.clone(),
                        fingerprint: k.fingerprint.clone(),
                        comment: k.comment.clone(),
                        passphrase: k.encrypted,
                        public_key: k.public.clone(),
                        default: k.default,
                        used_by: machines
                            .iter()
                            .filter(|e| {
                                e.identity
                                    .as_deref()
                                    .and_then(|i| key_for(i, std::slice::from_ref(k)))
                                    .is_some()
                            })
                            .map(|e| format!("ssh:{}", e.alias))
                            .collect(),
                    })
                    .collect(),
            })
        },
    );
    r.add(
        Def::new(
            "machine.key.create",
            "Make an SSH key in ~/.ssh (ED25519, or RSA 4096 for very old servers), with a passphrase if given",
            Risk::WriteLocal,
        )
        .human_only()
        .redact(&["/passphrase"]),
        |_, i: CreateKeyInput| -> AppResult<String> {
            let make = match i.kind.as_deref().unwrap_or("ed25519") {
                "ed25519" => sshkeys::Make::Ed25519,
                "rsa" => sshkeys::Make::Rsa,
                other => return Err(AppError::Invalid(format!("no key type {other}"))),
            };
            let k = sshkeys::create(i.name.trim(), make, i.passphrase.as_deref())?;
            Ok(k.name)
        },
    );
    r.add(
        Def::new(
            "machine.key.delete",
            "Delete an SSH key (to the Recycle Bin); the machines that signed in with it go back to ssh's own choice",
            Risk::Destructive,
        )
        .human_only(),
        |_, i: KeyInput| -> AppResult<()> {
            let k = sshkeys::find(&i.name)?;
            sshkeys::delete(&k)?;
            for e in users_of(&k) {
                sshconfig::set_identity(&e.alias, None)?;
            }
            Ok(())
        },
    );
    r.add(
        Def::new(
            "machine.key.use",
            "Set the key an SSH machine signs in with (its IdentityFile in Gond's ssh config); none leaves it to ssh",
            Risk::WriteLocal,
        ),
        |_, i: UseKeyInput| -> AppResult<()> {
            let a = alias(&i.id)?;
            match i.key {
                Some(name) => {
                    let k = sshkeys::find(&name)?;
                    sshconfig::set_identity(a, Some(&sshkeys::identity_value(&k)))
                }
                None => sshconfig::set_identity(a, None),
            }
        },
    );
    r.add(
        Def::new(
            "machine.key.authorize",
            "Put a key on an SSH machine (its ~/.ssh/authorized_keys, over Gond's connection) and sign in with it from now on",
            Risk::Outward,
        )
        .human_only(),
        |call, i: MachineKeyInput| -> AppResult<Authorized> {
            let a = alias(&i.id)?;
            let k = sshkeys::find(&i.key)?;
            let via = call
                .ctx
                .ptys
                .hub()?
                .authorize(&i.id, &sshkeys::authorized_line(&k))?;
            sshconfig::set_identity(a, Some(&sshkeys::identity_value(&k)))?;
            Ok(Authorized { via: via.into() })
        },
    );
    r.add(
        Def::new(
            "machine.key.check",
            "Sign in to an SSH machine with its key alone (no password): whether the machine takes it",
            Risk::Outward,
        ),
        |call, i: IdInput| -> AppResult<()> {
            let a = alias(&i.id)?;
            let e = sshconfig::find(a)
                .ok_or_else(|| AppError::NotFound(format!("no machine {}", i.id)))?;
            let identity = e
                .identity
                .ok_or_else(|| AppError::Invalid(format!("{a} has no key set")))?;
            let keys = sshkeys::list();
            let (path, passphrase) = match key_for(&identity, &keys) {
                Some(k) => (k.path.clone(), k.encrypted),
                None => (
                    sshkeys::identity_path(&identity)
                        .ok_or_else(|| AppError::Invalid("no home folder".into()))?,
                    false,
                ),
            };
            call.ctx.ptys.hub()?.check_key(&i.id, &path, passphrase)
        },
    );
    r.add(
        Def::new(
            "machine.key.revoke",
            "Take a key off an SSH machine (out of its ~/.ssh/authorized_keys); if it signed in with that key, ssh picks again",
            Risk::Destructive,
        )
        .human_only(),
        |call, i: MachineKeyInput| -> AppResult<()> {
            let a = alias(&i.id)?;
            let k = sshkeys::find(&i.key)?;
            call.ctx.ptys.hub()?.revoke(&i.id, &sshkeys::blob(&k))?;
            if users_of(&k).iter().any(|e| e.alias == a) {
                sshconfig::set_identity(a, None)?;
            }
            Ok(())
        },
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_user_comes_off_the_host() {
        assert_eq!(
            super::split_user("me@10.0.0.2"),
            (Some("me".into()), "10.0.0.2".into())
        );
        assert_eq!(super::split_user("dev-box"), (None, "dev-box".into()));
        assert_eq!(super::split_user("@x"), (None, "@x".into()));
    }
}
