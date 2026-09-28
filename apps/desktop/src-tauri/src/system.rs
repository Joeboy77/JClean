//! Talking to macOS on the UI's behalf: Full Disk Access (spec §11), links
//! out, Show in Finder, pickers, launch at login, the rule catalog and the
//! history (spec §7.5).

use std::path::PathBuf;
use std::sync::Arc;

use jclean_core::rules::{Audience, Rule, RuleSource};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_dialog::DialogExt as _;
use tauri_plugin_opener::OpenerExt as _;

use crate::engine::{CategoryDto, Engine, MethodDto, RiskDto, lock};

/// Whether JClean can see folders macOS guards with Full Disk Access.
#[tauri::command]
#[specta::specta]
pub fn full_disk_access(engine: State<'_, Arc<Engine>>) -> bool {
    jclean_core::platform::has_full_disk_access(&engine.env)
}

/// The only places JClean links out to.
#[derive(Debug, Clone, Copy, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Link {
    /// System Settings → Privacy & Security → Full Disk Access.
    FullDiskAccessSettings,
    Repository,
    Releases,
}

#[tauri::command]
#[specta::specta]
pub fn open_link(app: AppHandle, link: Link) -> Result<(), String> {
    let url = match link {
        Link::FullDiskAccessSettings => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"
        }
        Link::Repository => "https://github.com/Joeboy77/JClean",
        Link::Releases => "https://github.com/Joeboy77/JClean/releases",
    };
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

/// Shows a scanned item in Finder. Only paths from the scan can be revealed.
#[tauri::command]
#[specta::specta]
pub fn reveal_item(
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
    item_id: String,
) -> Result<(), String> {
    let path = lock(&engine.last)
        .as_ref()
        .and_then(|scan| scan.item(&item_id).and_then(|i| i.path.clone()))
        .ok_or("That item isn't in the current scan")?;
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|e| e.to_string())
}

/// Asks for a folder, e.g. for a custom rule or a project root.
#[tauri::command]
#[specta::specta]
pub async fn pick_folder(app: AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .blocking_pick_folder()
        .and_then(|f| f.into_path().ok())
        .map(|p| p.display().to_string())
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PickedFile {
    pub name: String,
    pub text: String,
}

/// Asks for a rule pack (JSON) and reads it. Only this user-chosen file is
/// read, never anything found by a scan.
#[tauri::command]
#[specta::specta]
pub async fn pick_rule_pack(app: AppHandle) -> Result<Option<PickedFile>, String> {
    let Some(path) = app
        .dialog()
        .file()
        .add_filter("Rule pack", &["json"])
        .blocking_pick_file()
        .and_then(|f| f.into_path().ok())
    else {
        return Ok(None);
    };
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if meta.len() > 1_000_000 {
        return Err("That rule pack is too large.".to_string());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let name = path.file_name().map_or_else(
        || "rules.json".to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    Ok(Some(PickedFile { name, text }))
}

pub fn apply_launch_at_login(app: &AppHandle, on: bool) {
    let launcher = app.autolaunch();
    if launcher.is_enabled().unwrap_or(!on) != on {
        let _ = if on {
            launcher.enable()
        } else {
            launcher.disable()
        };
    }
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LabelsDto {
    pub developer: String,
    pub everyday: String,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DescriptionDto {
    pub what: String,
    pub if_cleared: String,
}

#[derive(Debug, Clone, Copy, Serialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum AudienceDto {
    Everyday,
    Developer,
}

/// A rule as the UI shows it (spec §6).
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RuleDto {
    pub id: String,
    pub group: String,
    pub labels: LabelsDto,
    pub description: DescriptionDto,
    pub icon: String,
    pub category: CategoryDto,
    pub risk: RiskDto,
    pub regenerates: bool,
    pub method: MethodDto,
    pub command: Option<String>,
    pub audience: Vec<AudienceDto>,
    pub docs: Option<String>,
    /// Added by the user; shows a "Custom" badge (spec §6.4).
    pub custom: bool,
}

fn rule_dto(r: &Rule) -> RuleDto {
    RuleDto {
        id: r.id.clone(),
        group: r.group.clone(),
        labels: LabelsDto {
            developer: r.labels.developer.clone(),
            everyday: r.labels.everyday.clone(),
        },
        description: DescriptionDto {
            what: r.description.what.clone(),
            if_cleared: r.description.if_cleared.clone(),
        },
        icon: r.icon.clone(),
        category: r.category.into(),
        risk: r.risk.into(),
        regenerates: r.regenerates,
        method: r.cleanup.method.into(),
        command: r.cleanup.command.as_ref().map(|c| {
            std::iter::once(c.tool.clone())
                .chain(c.args.iter().cloned())
                .collect::<Vec<_>>()
                .join(" ")
        }),
        audience: r
            .audience
            .iter()
            .map(|a| match a {
                Audience::Everyday => AudienceDto::Everyday,
                Audience::Developer => AudienceDto::Developer,
            })
            .collect(),
        docs: r.docs.clone(),
        custom: r.source == RuleSource::Custom,
    }
}

/// Every rule in force, built-in and custom.
#[tauri::command]
#[specta::specta]
pub fn get_rules(engine: State<'_, Arc<Engine>>) -> Vec<RuleDto> {
    engine.rules().rules().iter().map(rule_dto).collect()
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CleanupDto {
    pub id: i32,
    #[specta(type = u32)]
    pub time: f64,
    #[specta(type = u32)]
    pub planned_bytes: f64,
    #[specta(type = Option<u32>)]
    pub freed_bytes: Option<f64>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ActionDto {
    #[specta(type = u32)]
    pub time: f64,
    pub rule_id: String,
    pub path: String,
    pub method: String,
    #[specta(type = u32)]
    pub bytes: f64,
    pub outcome: String,
    pub error: Option<String>,
}

/// Past cleanups, newest first (spec §5.4, History).
#[tauri::command]
#[specta::specta]
#[allow(clippy::cast_precision_loss)]
pub fn history_cleanups(engine: State<'_, Arc<Engine>>) -> Result<Vec<CleanupDto>, String> {
    let history = engine.history.as_ref().ok_or("History isn't available")?;
    Ok(history
        .cleanups()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|c| CleanupDto {
            id: i32::try_from(c.id).unwrap_or(i32::MAX),
            time: c.time as f64,
            planned_bytes: c.planned_bytes as f64,
            freed_bytes: c.freed_bytes.map(|b| b as f64),
            dry_run: c.dry_run,
        })
        .collect())
}

/// The deletion log of one cleanup (spec §7.5).
#[tauri::command]
#[specta::specta]
#[allow(clippy::cast_precision_loss)]
pub fn history_actions(
    engine: State<'_, Arc<Engine>>,
    cleanup_id: i32,
) -> Result<Vec<ActionDto>, String> {
    let history = engine.history.as_ref().ok_or("History isn't available")?;
    Ok(history
        .actions(Some(i64::from(cleanup_id)))
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|a| ActionDto {
            time: a.time as f64,
            rule_id: a.rule_id,
            path: a.path,
            method: a.method,
            bytes: a.bytes as f64,
            outcome: a.outcome,
            error: a.error,
        })
        .collect())
}

/// Saves the whole deletion log as CSV where the user chooses.
#[tauri::command]
#[specta::specta]
pub async fn export_history(
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
) -> Result<bool, String> {
    let history = engine.history.as_ref().ok_or("History isn't available")?;
    let csv = history.export_csv().map_err(|e| e.to_string())?;
    let Some(path): Option<PathBuf> = app
        .dialog()
        .file()
        .set_file_name("jclean-history.csv")
        .add_filter("CSV", &["csv"])
        .blocking_save_file()
        .and_then(|f| f.into_path().ok())
    else {
        return Ok(false);
    };
    std::fs::write(path, csv).map_err(|e| e.to_string())?;
    Ok(true)
}
