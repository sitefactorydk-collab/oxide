//! The tray (Windows notification area, macOS menu bar, Linux status area) icon and menu.

use oxide_core::DesktopAction;
use tauri::AppHandle;
use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use super::{dispatch, quick, quit, show_main, toggle_main};

const SHOW: &str = "tray-show";
const NEW_CHAT: &str = "tray-new-chat";
const QUICK: &str = "tray-quick-entry";
const QUIT: &str = "tray-quit";

pub fn build(app: &AppHandle) -> tauri::Result<TrayIcon> {
    let show = MenuItem::with_id(app, SHOW, "Show Oxide", true, None::<&str>)?;
    let new_chat = MenuItem::with_id(app, NEW_CHAT, "New chat", true, None::<&str>)?;
    let quick = MenuItem::with_id(app, QUICK, "Quick entry", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, QUIT, "Quit Oxide", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &new_chat, &quick, &separator, &quit])?;

    let mut builder = TrayIconBuilder::with_id("oxide")
        .tooltip("Oxide")
        .menu(&menu)
        // Left click opens the window; right click opens the menu.
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu_event)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)
}

fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        SHOW => show_main(app),
        NEW_CHAT => dispatch(app, DesktopAction::NewChat { text: None }),
        QUICK => quick::show(app),
        QUIT => quit(app),
        _ => {}
    }
}
