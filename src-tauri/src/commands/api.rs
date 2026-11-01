//! The one IPC path for the agent API: the UI calls
//! `command_run` / `command_list`; the old per-feature commands move behind the
//! registry in.

use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::api::{state::Patch, Actor, CommandInfo, Registry, Risk, Services};
use crate::error::{AppError, AppResult};

/// The registry and the services its handlers reach, managed by Tauri.
pub struct Api {
    pub registry: Registry<Services>,
    pub services: Services,
}

/// Runs a registered command as the human (the window is the human).
#[tauri::command]
pub async fn command_run(app: AppHandle, name: String, input: Option<Value>) -> AppResult<Value> {
    tauri::async_runtime::spawn_blocking(move || {
        let api = app.state::<Api>();
        api.registry.run(
            &api.services,
            &Actor::human(),
            &name,
            input.unwrap_or(Value::Null),
        )
    })
    .await
    .map_err(|e| AppError::Invalid(format!("background task failed: {e}")))?
}

#[tauri::command]
pub fn command_list(
    api: tauri::State<'_, Api>,
    risk: Option<Risk>,
    prefix: Option<String>,
) -> Vec<CommandInfo> {
    api.registry.list(risk, prefix.as_deref())
}

/// The whole tree for the UI store on load; then it applies `state:patch` events.
#[tauri::command]
pub fn state_get(api: tauri::State<'_, Api>) -> Value {
    let s = &api.services.state;
    let (version, value) = s.get("").unwrap_or_default();
    serde_json::json!({ "epoch": s.epoch(), "version": version, "value": value })
}

/// Forwards every state patch to the window as `state:patch`.
pub fn emit_patches(app: &AppHandle, api: &Api) {
    let app = app.clone();
    api.services.state.subscribe(move |p: &Patch| {
        let _ = app.emit("state:patch", p);
    });
}
