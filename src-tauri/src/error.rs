use serde::ser::SerializeStruct;

/// Every command returns `Result<T, AppError>`; the UI receives `{ code, message }`.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    NotAGitRepo(String),
    #[error("{0}")]
    GitFailed(String),
    #[error("{0}")]
    AgentNotFound(String),
    #[error("{0}")]
    AgentFailed(String),
    #[error("{0}")]
    PtyFailed(String),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("{0}")]
    Invalid(String),
    // Agent API
    /// The call is waiting on the human's approval (`pending`).
    #[error("waiting for the human's approval ({approval_id}); call approval.wait")]
    Pending { approval_id: String },
    /// The human said no (`rejected`).
    #[error("rejected by the human{}", .reason.as_deref().map(|r| format!(": \"{r}\"")).unwrap_or_default())]
    Rejected {
        approval_id: String,
        reason: Option<String>,
    },
    #[error("unknown command: {0}")]
    UnknownCommand(String),
    /// Input failed its schema; `pointer` is the JSON Pointer of the bad field.
    #[error("{message}")]
    InvalidInput { pointer: String, message: String },
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    StaleVersion(String),
    #[error("{0}")]
    Unauthorized(String),
    /// The GitHub CLI isn't installed; the UI offers `prereq.install` for "gh".
    #[error("{0}")]
    GhMissing(String),
    /// Git isn't installed; the UI shows Install Git (`prereq.install` "git").
    #[error("{0}")]
    GitMissing(String),
    /// The thread is paused (denials or a limit); ask the human.
    #[error("{0}")]
    Paused(String),
    /// The human has taken over this terminal. Code `forbidden`, but it never
    /// counts as a denial, so a take-over can't pause the agent.
    #[error("{0}")]
    TakenOver(String),
    /// Merging a thread into its base would conflict; nothing was changed.
    #[error("{0}")]
    MergeConflict(String),
    /// A `gh auth` step failed; the message is one plain sentence, never a token.
    #[error("{0}")]
    GithubFailed(String),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::NotFound(_) => "not_found",
            AppError::NotAGitRepo(_) => "not_a_git_repo",
            AppError::GitFailed(_) => "git_failed",
            AppError::AgentNotFound(_) => "agent_not_found",
            AppError::AgentFailed(_) => "agent_failed",
            AppError::PtyFailed(_) => "pty_failed",
            AppError::Db(_) => "db",
            AppError::Invalid(_) => "invalid",
            AppError::UnknownCommand(_) => "unknown_command",
            AppError::InvalidInput { .. } => "invalid_input",
            AppError::Forbidden(_) => "forbidden",
            AppError::StaleVersion(_) => "stale_version",
            AppError::Unauthorized(_) => "unauthorized",
            AppError::Pending { .. } => "pending",
            AppError::Rejected { .. } => "rejected",
            AppError::GhMissing(_) => "gh_missing",
            AppError::GithubFailed(_) => "github_failed",
            AppError::MergeConflict(_) => "merge_conflict",
            AppError::Paused(_) => "paused",
            AppError::TakenOver(_) => "forbidden",
            AppError::GitMissing(_) => "git_missing",
        }
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Invalid(e.to_string())
    }
}

impl serde::Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let pointer = match self {
            AppError::InvalidInput { pointer, .. } => Some(pointer),
            _ => None,
        };
        let approval = match self {
            AppError::Pending { approval_id } | AppError::Rejected { approval_id, .. } => {
                Some(approval_id)
            }
            _ => None,
        };
        let n = 2 + pointer.is_some() as usize + approval.is_some() as usize;
        let mut st = s.serialize_struct("AppError", n)?;
        st.serialize_field("code", self.code())?;
        st.serialize_field("message", &self.to_string())?;
        if let Some(p) = pointer {
            st.serialize_field("pointer", p)?;
        }
        if let Some(a) = approval {
            st.serialize_field("approvalId", a)?;
        }
        st.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialises_code_and_message() {
        let e = AppError::NotAGitRepo("/tmp/x is not a git repository".into());
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            serde_json::json!({ "code": "not_a_git_repo", "message": "/tmp/x is not a git repository" })
        );
    }
}
