//! `git.identity*` behind the registry: after Git is installed, the UI asks for
//! user.name/user.email if they're unset, prefilled from the active GitHub account.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::registry::{Def, Registry, Risk};
use super::Services;
use crate::error::AppResult;
use crate::git::identity::{self, Identity};
use crate::github::DEFAULT_HOST;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoInput {}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetInput {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IdentityStatus {
    /// What Git's global config says now.
    pub current: Identity,
    /// True when user.name or user.email is unset, so commits would fail.
    pub needed: bool,
    /// From the active github.com account, when `needed` and gh is signed in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested: Option<Identity>,
}

pub fn register(r: &mut Registry<Services>) {
    r.add(
        Def::new(
            "git.identity",
            "Git's user.name and user.email, and a suggestion from GitHub if unset",
            Risk::Read,
        ),
        |call, _: NoInput| -> AppResult<IdentityStatus> {
            let s = call.ctx;
            let current = identity::read(s.git_global.as_deref())?;
            let needed = current.name.is_none() || current.email.is_none();
            let suggested = needed
                .then(|| s.github.cli.user(DEFAULT_HOST))
                .flatten()
                .map(|(name, email)| Identity {
                    name: current.name.clone().or(Some(name)),
                    email: current.email.clone().or(Some(email)),
                });
            Ok(IdentityStatus {
                current,
                needed,
                suggested,
            })
        },
    );
    r.add(
        Def::new(
            "git.identity.set",
            "Set Git's global user.name and user.email",
            Risk::WriteLocal,
        )
        .human_only(),
        |call, i: SetInput| -> AppResult<Identity> {
            identity::set(call.ctx.git_global.as_deref(), &i.name, &i.email)
        },
    );
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use serde_json::json;

    use crate::api::{registry, Actor, ActorKind, Services};
    use crate::github::{GhCli, Github};

    #[test]
    fn suggests_from_github_then_sets() {
        let dir = tempfile::tempdir().unwrap();
        let gh = dir.path().join("gh");
        std::fs::write(
            &gh,
            "#!/bin/sh\n[ \"$1 $4\" = 'api user' ] && echo '{\"login\":\"sam\",\"id\":42,\"name\":null,\"email\":null}'\n",
        )
        .unwrap();
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        let file = dir.path().join("gitconfig");
        std::fs::write(&file, "[user]\n\tname = Sam\n").unwrap();
        let s = Services {
            github: Github::new(GhCli::with_path(format!(
                "{}:/usr/bin:/bin",
                dir.path().display()
            ))),
            git_global: Some(file.clone()),
            ..Services::default()
        };
        let (r, h) = (registry(), Actor::human());

        let st = r.run(&s, &h, "git.identity", json!({})).unwrap();
        assert_eq!(
            st,
            json!({
                "current": { "name": "Sam" },
                "needed": true,
                "suggested": { "name": "Sam", "email": "42+sam@users.noreply.github.com" },
            })
        );

        let agent = Actor {
            kind: ActorKind::Agent,
            id: "agent:T1".into(),
            label: "a".into(),
            thread_id: Some("T1".into()),
            session_id: None,
        };
        let set = json!({ "name": "Sam", "email": "d@example.com" });
        let e = r
            .run(&s, &agent, "git.identity.set", set.clone())
            .unwrap_err();
        assert_eq!(e.code(), "forbidden");
        r.run(&s, &h, "git.identity.set", set).unwrap();
        let st = r.run(&s, &h, "git.identity", json!({})).unwrap();
        assert_eq!(st["needed"], false);
        assert!(st.get("suggested").is_none());
    }
}
