//! Models offered by the configured endpoints.

use oxide_core::ProviderModels;
use tauri::State;

use crate::error::CmdResult;
use crate::state::AppState;

/// Models of every enabled endpoint, with capabilities. Endpoints that can't be reached come
/// back with `error` set. `refresh` skips the cache and re-checks tool support where it had
/// to be probed (Strata).
#[tauri::command]
#[specta::specta]
pub async fn list_models(
    state: State<'_, AppState>,
    refresh: bool,
) -> CmdResult<Vec<ProviderModels>> {
    Ok(state.models.list(refresh).await?)
}
