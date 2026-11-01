//! The tray icon: the Gond mark and a menu (Show, Quit); a click brings the window forward.
//! The window's X hides to it (Settings → General), so the agents in their panes keep working.

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

/// The tray, kept alive while the app runs.
pub struct Tray {
    _icon: TrayIcon<Wry>,
}

fn show(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(crate::window_label()) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn setup(app: &AppHandle) -> tauri::Result<Tray> {
    let name = crate::channel::CHANNEL.name();
    let open = MenuItem::with_id(app, "show", format!("Show {name}"), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", format!("Quit {name}"), true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut builder = TrayIconBuilder::with_id("gond")
        .tooltip(name)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "show" => show(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = e
            {
                show(tray.app_handle());
            }
        });
    // The bundle's icon: the official Gond mark (`bun run icons`), never the mascot; Gond Code
    // Dev's carries an amber dot.
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(if crate::channel::is_dev() {
            crate::channel::badge(icon)
        } else {
            icon.clone()
        });
    }
    Ok(Tray {
        _icon: builder.build(app)?,
    })
}
