//! Thin Tauri shell over `jclean-core`. Commands stay small and push heavy
//! work to background threads; see `docs/SPEC.md` §3.

use serde::Serialize;
use specta::Type;
use tauri_specta::{Builder, collect_commands};

mod clean;
mod engine;
mod settings;
mod system;
mod tray;
mod updater;
mod volume;
mod window;

use std::sync::Arc;

use tauri::Manager;

/// Where the generated IPC bindings are written, relative to this crate.
const BINDINGS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings.ts");

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub core_version: String,
    pub platform: String,
    /// The user's home folder, for showing paths as `~/…`.
    pub home: String,
}

#[tauri::command]
#[specta::specta]
fn app_info(app: tauri::AppHandle, engine: tauri::State<'_, Arc<engine::Engine>>) -> AppInfo {
    AppInfo {
        home: engine::home(&engine),
        version: app.package_info().version.to_string(),
        core_version: jclean_core::VERSION.to_string(),
        platform: std::env::consts::OS.to_string(),
    }
}

/// Dev builds only: prints a message from the UI to the terminal (used by
/// the frame-rate probe). Does nothing in release builds.
#[tauri::command]
#[specta::specta]
fn dev_log(message: String) {
    #[cfg(debug_assertions)]
    eprintln!("[ui] {message}");
    #[cfg(not(debug_assertions))]
    let _ = message;
}

/// All commands and events exposed to the frontend. The TypeScript bindings
/// are generated from this, so frontend and backend can't drift.
pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new().commands(collect_commands![
        app_info,
        dev_log,
        window::set_window_mode,
        engine::start_scan,
        engine::cancel_scan,
        engine::map_level,
        engine::volume_info,
        engine::cached_scan,
        clean::plan_clean,
        clean::run_clean,
        clean::empty_trash,
        settings::get_settings,
        settings::save_settings,
        settings::add_custom_folder,
        settings::add_rule_pack,
        settings::remove_custom_rule,
        system::full_disk_access,
        system::open_link,
        system::reveal_item,
        system::pick_folder,
        system::pick_rule_pack,
        system::get_rules,
        system::history_cleanups,
        system::history_actions,
        system::export_history,
        updater::check_for_update,
        updater::install_update,
    ])
}

/// Writes `src/bindings.ts` for the frontend.
pub fn export_bindings(builder: &Builder<tauri::Wry>) -> Result<(), String> {
    builder
        .export(
            specta_typescript::Typescript::default()
                .header("// Regenerate with `cargo test -p jclean-desktop`.\n"),
            BINDINGS_PATH,
        )
        .map_err(|err| err.to_string())
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let builder = specta_builder();
    let engine = Arc::new(engine::Engine::new()?);

    #[cfg(debug_assertions)]
    if let Err(err) = export_bindings(&builder) {
        eprintln!("Couldn't export TypeScript bindings: {err}");
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(updater::PendingUpdate::default())
        .manage(window::LastExpandedWidth::default())
        .manage(engine)
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            tray::refresh(app.handle());
            Ok(())
        })
        // With the menu bar icon on, closing the window keeps JClean running
        // there; "Open JClean" brings it back (spec §5.10).
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let keep = window
                    .app_handle()
                    .try_state::<Arc<engine::Engine>>()
                    .is_some_and(|e| engine::lock(&e.settings).menu_bar_icon);
                if keep {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .run(tauri::generate_context!())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keeps `src/bindings.ts` in sync. CI fails if this changes the file.
    #[test]
    fn export_typescript_bindings() {
        export_bindings(&specta_builder()).expect("bindings export");
    }
}
