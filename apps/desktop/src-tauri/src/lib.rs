//! Thin Tauri shell over `jclean-core`. Commands stay small and push heavy
//! work to background threads; see `docs/SPEC.md` §3.

use serde::Serialize;
use specta::Type;
use tauri_specta::{Builder, collect_commands};

/// Where the generated IPC bindings are written, relative to this crate.
const BINDINGS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings.ts");

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub core_version: String,
    pub platform: String,
}

#[tauri::command]
#[specta::specta]
fn app_info(app: tauri::AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        core_version: jclean_core::VERSION.to_string(),
        platform: std::env::consts::OS.to_string(),
    }
}

/// All commands and events exposed to the frontend. The TypeScript bindings
/// are generated from this, so frontend and backend can't drift.
pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new().commands(collect_commands![app_info])
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

pub fn run() -> tauri::Result<()> {
    let builder = specta_builder();

    #[cfg(debug_assertions)]
    if let Err(err) = export_bindings(&builder) {
        eprintln!("Couldn't export TypeScript bindings: {err}");
    }

    tauri::Builder::default()
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            Ok(())
        })
        .run(tauri::generate_context!())
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
