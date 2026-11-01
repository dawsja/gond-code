//! What the app and the terminal host say to each other: one JSON object per line. The app
//! sends requests (`Envelope`), each with an id; the host answers each (`Reply`) and, in
//! between, streams every terminal's output (`Data`) and end (`Exit`).
//!
//! Requests and fields are only ever added, never changed or taken away: a newer app still
//! talks to an older build's host (where its panes from before an update still run) with the
//! requests that host knows, and a host answers a request it doesn't know with an error rather
//! than silence. Bump `PROTOCOL` when you add one, so the app can tell.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Spawn;
use crate::error::AppError;

pub const PROTOCOL: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Request {
    /// → `{ protocol, build, pid }`. `build`: the app's; a host of another build knows a newer
    /// one has taken over, and ends once its last terminal has.
    Hello {
        #[serde(default)]
        build: Option<String>,
    },
    /// → `Opened`.
    #[serde(rename_all = "camelCase")]
    Spawn {
        owner: String,
        key: Option<String>,
        spawn: Spawn,
        cols: u16,
        rows: u16,
        headless: bool,
    },
    /// → `Attached` or null.
    Attach {
        key: String,
    },
    #[serde(rename_all = "camelCase")]
    Write {
        pty_id: String,
        data: String,
    },
    #[serde(rename_all = "camelCase")]
    Resize {
        pty_id: String,
        cols: u16,
        rows: u16,
    },
    /// → `PtyRead`.
    #[serde(rename_all = "camelCase")]
    Read {
        pty_id: String,
        since: Option<u64>,
        wait_for: Option<String>,
        timeout_ms: Option<u64>,
    },
    /// → a pid or null.
    #[serde(rename_all = "camelCase")]
    Pid {
        pty_id: String,
    },
    /// → every running terminal's pid.
    Pids,
    #[serde(rename_all = "camelCase")]
    Close {
        pty_id: String,
    },
    CloseAll,
    /// Closes every terminal `owner` opened (a project removed).
    CloseOwner {
        owner: String,
    },
    /// → how many were closed.
    Prune {
        keep: Vec<String>,
    },
    /// Closes every terminal and ends the host.
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub id: u64,
    #[serde(flatten)]
    pub req: Request,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FromHost {
    Reply {
        id: u64,
        #[serde(default)]
        ok: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        err: Option<Failure>,
    },
    #[serde(rename_all = "camelCase")]
    Data {
        pty_id: String,
        data: String,
        end: u64,
    },
    #[serde(rename_all = "camelCase")]
    Exit { pty_id: String, code: Option<u32> },
}

/// An `AppError` across the pipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Failure {
    pub code: String,
    pub message: String,
}

impl From<&AppError> for Failure {
    fn from(e: &AppError) -> Self {
        Failure {
            code: e.code().into(),
            message: e.to_string(),
        }
    }
}

impl From<Failure> for AppError {
    fn from(f: Failure) -> Self {
        match f.code.as_str() {
            "not_found" => AppError::NotFound(f.message),
            "invalid" => AppError::Invalid(f.message),
            _ => AppError::PtyFailed(f.message),
        }
    }
}

/// One line of JSON.
pub fn line(v: &impl Serialize) -> String {
    let mut s = serde_json::to_string(v).unwrap_or_default();
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn requests_and_events_round_trip_as_one_line_each() {
        let e = Envelope {
            id: 7,
            req: Request::Write {
                pty_id: "p".into(),
                data: "ls\r\n".into(),
            },
        };
        let l = line(&e);
        assert_eq!(l.matches('\n').count(), 1, "{l}");
        let v: Value = serde_json::from_str(&l).unwrap();
        assert_eq!(
            v,
            json!({ "id": 7, "op": "write", "ptyId": "p", "data": "ls\r\n" })
        );
        let back: Envelope = serde_json::from_str(&l).unwrap();
        assert!(matches!(back.req, Request::Write { .. }));

        let d: FromHost = serde_json::from_str(&line(&FromHost::Data {
            pty_id: "p".into(),
            data: "\x1b[31mred\n".into(),
            end: 9,
        }))
        .unwrap();
        assert!(matches!(d, FromHost::Data { end: 9, .. }));
        let err: AppError = Failure::from(&AppError::NotFound("gone".into())).into();
        assert_eq!(err.code(), "not_found");
    }
}
