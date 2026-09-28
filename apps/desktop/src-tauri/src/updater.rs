//! Updates (spec §14): checked against GitHub Releases when Settings allow,
//! downloaded in the background, installed only when the user says so.
//! Update packages are signed; the public key is in tauri.conf.json.

use std::sync::{Arc, Mutex};

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, State};
use tauri_plugin_updater::{Update, UpdaterExt as _};

use crate::engine::{Engine, lock};

/// A downloaded update waiting for the user to restart.
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<(Update, Vec<u8>)>>);

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
}

/// Checks for a newer version and downloads it. `None` if JClean is up to
/// date or update checks are off in Settings.
#[tauri::command]
#[specta::specta]
pub async fn check_for_update(
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
    pending: State<'_, PendingUpdate>,
) -> Result<Option<UpdateInfo>, String> {
    if !lock(&engine.settings).check_for_updates {
        return Ok(None);
    }
    if let Some((update, _)) = lock(&pending.0).as_ref() {
        return Ok(Some(info(update)));
    }
    let Some(update) = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?
    else {
        return Ok(None);
    };
    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    let found = info(&update);
    *lock(&pending.0) = Some((update, bytes));
    Ok(Some(found))
}

fn info(update: &Update) -> UpdateInfo {
    UpdateInfo {
        version: update.version.clone(),
        notes: update.body.clone(),
    }
}

/// Installs the downloaded update and restarts JClean.
#[tauri::command]
#[specta::specta]
pub fn install_update(app: AppHandle, pending: State<'_, PendingUpdate>) -> Result<(), String> {
    let (update, bytes) = lock(&pending.0)
        .take()
        .ok_or("There's no update ready to install")?;
    update.install(bytes).map_err(|e| e.to_string())?;
    app.restart()
}
