//! AgentRunner: how a pane starts an agent. The
//! agent's own interactive CLI, in the pane's folder, signed in as the thread's account, with
//! its model and thinking level, resumed on its session where it has one.
//! Gond runs nothing in the background: each agent works in its terminal as it does on its own.

use std::sync::Arc;

use crate::agents::{claude::Claude, codex::Codex, custom::Custom, Driver};
use crate::error::{AppError, AppResult};
use crate::store::threads::Thread;
use crate::store::{custom_agents, Store};
use crate::vault::SecretResolver;

/// The driver for an agent id: a built-in, or a saved custom agent.
pub fn driver_for(store: &Store, agent_id: &str) -> AppResult<Arc<dyn Driver>> {
    match agent_id {
        "claude" => Ok(Arc::new(Claude)),
        "codex" => Ok(Arc::new(Codex)),
        "agy" => Ok(Arc::new(crate::agents::agy::Agy)),
        "grok" => Ok(Arc::new(crate::agents::grok::Grok)),
        "opencode" => Ok(Arc::new(crate::agents::opencode::Opencode)),
        "pi" => Ok(Arc::new(crate::agents::pi::Pi)),
        other => store
            .with(custom_agents::list)?
            .into_iter()
            .find(|a| a.id == other)
            .map(|a| Arc::new(Custom(a)) as Arc<dyn Driver>)
            .ok_or_else(|| AppError::AgentNotFound(format!("agent {other} not found"))),
    }
}

/// An agent's interactive CLI, as `AgentRunner::interactive` builds it.
#[derive(Debug, Clone, PartialEq)]
pub struct Interactive {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub remove: Vec<String>,
    pub cwd: std::path::PathBuf,
}

#[derive(Clone)]
pub struct AgentRunner {
    store: Store,
    /// Resolves account secrets at spawn; only ever into the child's env.
    secrets: Option<Arc<dyn SecretResolver>>,
}

impl AgentRunner {
    pub fn new(store: Store) -> Self {
        AgentRunner {
            store,
            secrets: None,
        }
    }

    pub fn with_secrets(mut self, secrets: Arc<dyn SecretResolver>) -> Self {
        self.secrets = Some(secrets);
        self
    }

    /// The env an account runs with: what to unset, then what to set (its home and its
    /// secrets, resolved), and the global options it puts before the agent's arguments.
    #[allow(clippy::type_complexity)]
    fn account_vars(
        &self,
        driver: &dyn Driver,
        id: &str,
    ) -> AppResult<(Vec<&'static str>, Vec<(String, String)>, Vec<String>)> {
        let account = self
            .store
            .with(|c| crate::store::accounts::get(c, id))?
            .ok_or_else(|| AppError::NotFound(format!("account {id} not found")))?;
        let e = driver.account_env(&account)?;
        if let Some(dir) = &account.config_dir {
            std::fs::create_dir_all(dir)?;
        }
        let vault = || {
            self.secrets
                .as_ref()
                .ok_or_else(|| AppError::Invalid("the accounts vault isn't available".into()))
        };
        let mut set = e.env;
        for (var, at) in &e.secrets {
            set.push((var.clone(), vault()?.resolve(at)?.expose().to_string()));
        }
        // Values built from text and secrets (Claude's header block).
        for (var, parts) in &e.composed {
            let mut value = String::new();
            for part in parts {
                match part {
                    crate::agents::driver::Part::Text(t) => value.push_str(t),
                    crate::agents::driver::Part::Secret(at) => {
                        value.push_str(vault()?.resolve(at)?.expose())
                    }
                }
            }
            set.push((var.clone(), value));
        }
        Ok((e.remove, set, e.args))
    }

    /// The Claude Code config folder a thread uses: its account's, else yours.
    pub fn claude_config(&self, thread: &Thread) -> Option<std::path::PathBuf> {
        let account = thread.account_id.as_ref().and_then(|id| {
            self.store
                .with(|c| crate::store::accounts::get(c, id))
                .ok()
                .flatten()
        });
        account
            .and_then(|a| a.config_dir)
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("CLAUDE_CONFIG_DIR").map(std::path::PathBuf::from))
            .or_else(|| dirs::home_dir().map(|h| h.join(".claude")))
    }

    /// The agent's own interactive CLI for a pane: the thread's folder,
    /// account, model and thinking level, resumed on its session. Permissions are the agent's
    /// own: Gond passes none, so it asks (or doesn't) as you've set it up.
    pub fn interactive(&self, thread: &Thread) -> AppResult<Interactive> {
        let driver = driver_for(&self.store, &thread.agent_id)?;
        let (remove, env, mut args) = match &thread.account_id {
            Some(id) => self.account_vars(&*driver, id)?,
            None => (vec![], vec![], vec![]),
        };
        let sid = thread.agent_session_id.clone();
        let program = match thread.agent_id.as_str() {
            "claude" => {
                if let Some(id) = &sid {
                    args.extend(["--resume".into(), id.clone()]);
                }
                if let Some(model) = &thread.model {
                    args.extend(["--model".into(), model.clone()]);
                }
                if let Some(level) = thread.thinking {
                    args.extend(["--effort".into(), level.as_str().into()]);
                }
                "claude".to_string()
            }
            "codex" => {
                if let Some(model) = &thread.model {
                    args.extend(["-m".into(), model.clone()]);
                }
                if let Some(id) = &sid {
                    args.extend(["resume".into(), id.clone()]);
                }
                "codex".to_string()
            }
            // Grok and agy resume a session by id too, with their model.
            "grok" => {
                if let Some(id) = &sid {
                    args.extend(["-r".into(), id.clone()]);
                }
                if let Some(model) = &thread.model {
                    args.extend(["-m".into(), model.clone()]);
                }
                if let Some(level) = thread.thinking {
                    args.extend(["--effort".into(), level.as_str().into()]);
                }
                "grok".to_string()
            }
            // Pi opens the session with this id in the folder, or creates it.
            "pi" => {
                if let Some(id) = &sid {
                    args.extend(["--session-id".into(), id.clone()]);
                }
                if let Some(model) = &thread.model {
                    args.extend(["--model".into(), model.clone()]);
                }
                if let Some(level) = thread.thinking {
                    args.extend(["--thinking".into(), level.as_str().into()]);
                }
                "pi".to_string()
            }
            "agy" => {
                if let Some(id) = &sid {
                    args.extend(["--conversation".into(), id.clone()]);
                }
                if let Some(model) = &thread.model {
                    args.extend([
                        "--model".into(),
                        crate::agents::models::agy_slug(model, thread.thinking),
                    ]);
                }
                "agy".to_string()
            }
            _ => driver.detect().command,
        };
        Ok(Interactive {
            program,
            args,
            env,
            remove: remove.into_iter().map(String::from).collect(),
            cwd: thread.worktree_path.clone().into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pane's agent starts as its own CLI with the thread's model and thinking level, on
    /// its session, in the thread's folder; Gond adds no permission flags of its own.
    #[test]
    fn a_pane_runs_the_agents_own_cli_with_its_model_and_session() {
        let store = Store::open_in_memory().unwrap();
        let repo_dir = crate::git::tests::temp_repo();
        let p = crate::projects::add(&store, repo_dir.path()).unwrap();
        let mut t = crate::threads::create(
            &store,
            crate::threads::NewThread {
                project_id: p.id,
                agent_id: "claude".into(),
                title: None,
                account_id: None,
                model: None,
                thinking: None,
            },
        )
        .unwrap();
        t.model = Some("opus".into());
        t.thinking = Some(crate::agents::driver::Level::Xhigh);
        t.agent_session_id = Some("s-1".into());
        let it = AgentRunner::new(store).interactive(&t).unwrap();
        assert_eq!(it.program, "claude");
        assert_eq!(
            it.args,
            ["--resume", "s-1", "--model", "opus", "--effort", "xhigh"]
        );
        assert_eq!(it.cwd, std::path::PathBuf::from(&t.worktree_path));
        assert!(!it.args.iter().any(|a| a.contains("permission")));
        // Pi opens (or creates) the session with Gond's id, with its model and thinking.
        t.agent_id = "pi".into();
        t.model = Some("anthropic/claude-opus-5-5".into());
        t.thinking = Some(crate::agents::driver::Level::High);
        let pi = AgentRunner::new(Store::open_in_memory().unwrap())
            .interactive(&t)
            .unwrap();
        assert_eq!(pi.program, "pi");
        assert_eq!(
            pi.args,
            [
                "--session-id",
                "s-1",
                "--model",
                "anthropic/claude-opus-5-5",
                "--thinking",
                "high"
            ]
        );
    }
}
