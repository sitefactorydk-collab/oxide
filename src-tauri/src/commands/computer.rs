//! Settings → Computer use and the control bar.

use oxide_core::computer::{ComputerActivity, ComputerStatus, PANIC_SHORTCUT_LABEL};
use tauri::{AppHandle, Manager, State};

use crate::error::CmdResult;
use crate::state::AppState;

/// Whether this build can control the computer, its screens, the chats doing so now and the
/// panic shortcut's state.
#[tauri::command]
#[specta::specta]
pub async fn computer_status(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<ComputerStatus> {
    let panic_error = app
        .try_state::<crate::desktop::DesktopState>()
        .and_then(|d| d.panic_error());
    let Some(cu) = state.computer.clone() else {
        return Ok(ComputerStatus {
            supported: false,
            displays: Vec::new(),
            error: None,
            active: Vec::new(),
            panic_shortcut: PANIC_SHORTCUT_LABEL.into(),
            panic_error,
        });
    };
    let listed = tauri::async_runtime::spawn_blocking({
        let cu = cu.clone();
        move || cu.displays()
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));
    let (displays, error) = match listed {
        Ok(d) => (d, None),
        Err(e) => (
            Vec::new(),
            Some(format!("Oxide couldn't list the screens: {e}")),
        ),
    };
    Ok(ComputerStatus {
        supported: true,
        displays,
        error,
        active: cu.active(),
        panic_shortcut: PANIC_SHORTCUT_LABEL.into(),
        panic_error,
    })
}

/// What chats controlling the computer are doing (the control bar asks when it opens).
#[tauri::command]
#[specta::specta]
pub async fn computer_activity(state: State<'_, AppState>) -> CmdResult<Vec<ComputerActivity>> {
    Ok(state.computer_bar.activity())
}

/// Measures how a model points at things on screenshots and saves the correction (Settings
/// → Computer use). Takes as long as two short replies of that model.
#[tauri::command]
#[specta::specta]
pub async fn computer_calibrate(
    state: State<'_, AppState>,
    model: String,
) -> CmdResult<oxide_core::PointerCalibration> {
    Ok(state.chat.calibrate_pointer(&model).await?)
}

/// Stops every chat that controls the computer (the control bar's Stop); returns them.
#[tauri::command]
#[specta::specta]
pub async fn computer_stop_all(app: AppHandle) -> CmdResult<Vec<String>> {
    Ok(crate::computer::stop_all(&app))
}
