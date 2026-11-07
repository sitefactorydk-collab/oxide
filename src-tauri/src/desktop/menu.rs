//! The macOS app menu bar. Windows and Linux have Oxide's own title bar and no menu bar.

#[cfg(target_os = "macos")]
use oxide_core::DesktopAction;
#[cfg(target_os = "macos")]
use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
#[cfg(target_os = "macos")]
use tauri::{AppHandle, Wry};

#[cfg(target_os = "macos")]
const SETTINGS: &str = "menu-settings";
#[cfg(target_os = "macos")]
const NEW_CHAT: &str = "menu-new-chat";
#[cfg(target_os = "macos")]
const QUICK: &str = "menu-quick-entry";

/// Oxide, File, Edit, View and Window menus with the standard accelerators.
#[cfg(target_os = "macos")]
pub fn build(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let sep = || PredefinedMenuItem::separator(app);
    let app_menu = Submenu::with_items(
        app,
        "Oxide",
        true,
        &[
            &PredefinedMenuItem::about(app, Some("About Oxide"), None)?,
            &sep()?,
            &MenuItem::with_id(app, SETTINGS, "Settings…", true, Some("CmdOrCtrl+,"))?,
            &sep()?,
            &PredefinedMenuItem::services(app, None)?,
            &sep()?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &sep()?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;
    let file = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &MenuItem::with_id(app, NEW_CHAT, "New Chat", true, Some("CmdOrCtrl+Shift+O"))?,
            &MenuItem::with_id(app, QUICK, "Quick Entry", true, None::<&str>)?,
            &sep()?,
            &PredefinedMenuItem::close_window(app, None)?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &sep()?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;
    let view = Submenu::with_items(
        app,
        "View",
        true,
        &[&PredefinedMenuItem::fullscreen(app, None)?],
    )?;
    let window = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
        ],
    )?;
    Menu::with_items(app, &[&app_menu, &file, &edit, &view, &window])
}

#[cfg(target_os = "macos")]
pub fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        SETTINGS => super::dispatch(app, DesktopAction::OpenSettings { section: None }),
        NEW_CHAT => super::dispatch(app, DesktopAction::NewChat { text: None }),
        QUICK => super::quick::show(app),
        _ => {}
    }
}
