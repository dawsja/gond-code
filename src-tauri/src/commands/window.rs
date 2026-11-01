use tauri::State;

use crate::error::AppResult;
use crate::window::{InfoCell, WindowInfo};

/// Waits (off the async runtime) for `window::setup`, which may still be running
/// when the page first asks.
#[tauri::command]
pub async fn window_info(info: State<'_, std::sync::Arc<InfoCell>>) -> AppResult<WindowInfo> {
    let cell = info.inner().clone();
    tauri::async_runtime::spawn_blocking(move || cell.wait(std::time::Duration::from_secs(10)))
        .await
        .map_err(|e| crate::error::AppError::Invalid(format!("window_info: {e}")))
}
