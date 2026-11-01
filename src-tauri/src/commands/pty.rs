use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::pty::PtySink;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Data<'a> {
    pty_id: &'a str,
    data: String,
    /// The terminal's output length after `data`: a pane that attached skips what its
    /// replay already drew.
    end: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Exit<'a> {
    pty_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<u32>,
}

/// Sends PTY output to the UI as `pty:data` / `pty:exit`.
pub struct EventSink(pub AppHandle);

impl PtySink for EventSink {
    fn data(&self, pty_id: &str, data: String, end: u64) {
        let _ = self.0.emit("pty:data", Data { pty_id, data, end });
    }
    fn exit(&self, pty_id: &str, code: Option<u32>) {
        let _ = self.0.emit("pty:exit", Exit { pty_id, code });
    }
}
