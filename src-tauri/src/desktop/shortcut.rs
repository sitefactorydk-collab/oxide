//! The global quick entry shortcut (`tauri-plugin-global-shortcut`).

use oxide_core::DesktopSettings;
use serde::Serialize;
use specta::Type;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{
    GlobalShortcutExt as _, Shortcut, ShortcutEvent, ShortcutState,
};

use super::{DesktopState, lock, quick};

/// What happened when Oxide last tried to register the shortcut.
#[derive(Debug, Clone, Default, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutStatus {
    /// The shortcut that should be registered (`None` when quick entry is off).
    pub wanted: Option<String>,
    /// The shortcut that is registered.
    pub active: Option<String>,
    /// Why `wanted` couldn't be registered, for Settings.
    pub error: Option<String>,
}

/// The plugin's handler: any registered shortcut is the quick entry one.
pub fn handle(app: &AppHandle, _shortcut: &Shortcut, event: ShortcutEvent) {
    if event.state == ShortcutState::Pressed {
        quick::toggle(app);
    }
}

/// Registers the shortcut from `settings` (or none). Does nothing when it's already active;
/// retries one that failed before (another app may have let go of it).
pub fn apply(app: &AppHandle, state: &DesktopState, settings: &DesktopSettings) {
    let wanted = settings
        .quick_entry
        .then(|| settings.quick_entry_shortcut.clone());
    let mut status = lock(&state.shortcut);
    if status.wanted == wanted && status.active == wanted {
        return;
    }
    let gs = app.global_shortcut();
    if let Err(e) = gs.unregister_all() {
        tracing::warn!("couldn't release the quick entry shortcut: {e}");
    }
    *status = ShortcutStatus {
        wanted: wanted.clone(),
        active: None,
        error: None,
    };
    let Some(text) = wanted else { return };
    match text.parse::<Shortcut>() {
        Err(e) => {
            tracing::warn!("quick entry shortcut {text:?} doesn't parse: {e}");
            status.error = Some(format!("“{text}” isn't a shortcut Oxide can use."));
        }
        Ok(shortcut) => match gs.register(shortcut) {
            Ok(()) => {
                tracing::info!("quick entry shortcut: {text}");
                status.active = Some(text);
            }
            Err(e) => {
                tracing::warn!("couldn't register {text}: {e}");
                status.error = Some(format!(
                    "{text} couldn't be registered. Another app may already use it: pick another shortcut."
                ));
            }
        },
    }
}
