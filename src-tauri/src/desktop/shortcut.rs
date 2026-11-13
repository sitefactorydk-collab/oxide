//! Global shortcuts (`tauri-plugin-global-shortcut`): quick entry's, and computer use's
//! panic shortcut (Ctrl+Alt+Esc), registered while Settings → Computer use is on.

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

/// The panic shortcut.
fn panic_shortcut() -> Option<Shortcut> {
    oxide_core::computer::PANIC_SHORTCUT
        .parse::<Shortcut>()
        .ok()
}

/// The plugin's handler: the panic shortcut stops computer use; any other is quick entry's.
pub fn handle(app: &AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    if event.state != ShortcutState::Pressed {
        return;
    }
    if panic_shortcut().as_ref() == Some(shortcut) {
        crate::computer::stop_all(app);
    } else {
        quick::toggle(app);
    }
}

/// Registers the panic shortcut while computer use is on (and releases it when it's off).
/// Returns why it couldn't be registered.
pub fn apply_panic(app: &AppHandle, state: &DesktopState, on: bool) {
    let gs = app.global_shortcut();
    let mut error = lock(&state.panic_error);
    let Some(panic) = panic_shortcut() else {
        *error = Some("The panic shortcut doesn't parse.".into());
        return;
    };
    let registered = gs.is_registered(panic);
    if on == registered {
        if on {
            *error = None;
        }
        return;
    }
    if !on {
        if let Err(e) = gs.unregister(panic) {
            tracing::warn!("couldn't release the panic shortcut: {e}");
        }
        *error = None;
        return;
    }
    match gs.register(panic) {
        Ok(()) => {
            tracing::info!("panic shortcut: {}", oxide_core::computer::PANIC_SHORTCUT);
            *error = None;
        }
        Err(e) => {
            tracing::warn!("couldn't register the panic shortcut: {e}");
            *error = Some(format!(
                "{} couldn't be registered (another app may use it). Stop computer use with \
                 the control bar's Stop button instead.",
                oxide_core::computer::PANIC_SHORTCUT_LABEL
            ));
        }
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
    // Only quick entry's own shortcut: the panic shortcut stays.
    if let Some(old) = status
        .active
        .as_deref()
        .and_then(|t| t.parse::<Shortcut>().ok())
        && Some(old) != panic_shortcut()
        && let Err(e) = gs.unregister(old)
    {
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
        Ok(shortcut) if Some(shortcut) == panic_shortcut() => {
            status.error = Some(format!(
                "{text} stops computer use: pick another shortcut for quick entry."
            ));
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
