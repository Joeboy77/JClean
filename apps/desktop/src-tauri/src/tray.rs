//! The menu bar icon (spec §5.10): free space, what the last scan found,
//! Quick scan, Open JClean and Quit. Optional, on by default.

use std::sync::Arc;

use jclean_core::units::format_bytes;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::engine::{Engine, lock};

const TRAY_ID: &str = "jclean";
/// Sent to the UI when "Quick scan" is chosen from the menu bar.
pub const QUICK_SCAN_EVENT: &str = "tray-quick-scan";

/// Shows, hides and refreshes the icon to match Settings.
pub fn apply(app: &AppHandle, engine: &Engine) {
    let settings = lock(&engine.settings).clone();
    if !settings.menu_bar_icon {
        let _ = app.remove_tray_by_id(TRAY_ID);
        return;
    }
    let Ok(menu) = menu(app, engine) else { return };
    let title = if settings.menu_bar_free_space {
        crate::volume::main_volume().map(|v| format_bytes(bytes(v.available)))
    } else {
        None
    };
    // Windows tray icons can't show text beside them, so it goes in the tooltip.
    let tooltip = match (&title, cfg!(windows)) {
        (Some(free), true) => format!("JClean · {free} free"),
        _ => "JClean".to_string(),
    };
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_menu(Some(menu));
        let _ = tray.set_title(title.as_deref());
        let _ = tray.set_tooltip(Some(&tooltip));
        return;
    }
    // macOS tints a black template icon to match the menu bar; Windows shows
    // icons as they are, so it gets the colour app icon.
    let bytes: &[u8] = if cfg!(windows) {
        include_bytes!("../icons/32x32.png")
    } else {
        include_bytes!("../icons/tray.png")
    };
    let Ok(icon) = Image::from_bytes(bytes) else {
        return;
    };
    let built = TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .icon_as_template(!cfg!(windows))
        .tooltip(&tooltip)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "scan" => {
                show_main(app);
                let _ = app.emit(QUICK_SCAN_EVENT, ());
            }
            "open" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app);
    if let (Ok(tray), Some(title)) = (built, title) {
        let _ = tray.set_title(Some(title));
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn bytes(n: f64) -> u64 {
    n.max(0.0) as u64
}

fn menu(app: &AppHandle, engine: &Engine) -> tauri::Result<Menu<Wry>> {
    let free = crate::volume::main_volume().map_or_else(
        || "Free space unknown".to_string(),
        |v| format!("{} free", format_bytes(bytes(v.available))),
    );
    let found = lock(&engine.last).as_ref().map_or_else(
        || "Not scanned yet".to_string(),
        |scan| format!("{} can be freed", format_bytes(scan.reclaimable())),
    );
    Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "free", free, false, None::<&str>)?,
            &MenuItem::with_id(app, "found", found, false, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "scan", "Quick scan", true, None::<&str>)?,
            &MenuItem::with_id(app, "open", "Open JClean", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", "Quit JClean", true, Some("CmdOrCtrl+Q"))?,
        ],
    )
}

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Refreshes the menu's numbers after a scan or clean.
pub fn refresh(app: &AppHandle) {
    if let Some(engine) = app.try_state::<Arc<Engine>>() {
        apply(app, &engine);
    }
}
