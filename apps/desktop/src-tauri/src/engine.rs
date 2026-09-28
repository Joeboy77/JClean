//! Bridges `jclean-core` to the UI: scans run on a background thread and
//! stream results through a Tauri channel (spec §3); the last result stays
//! here so the map can be browsed level by level and cleaned from later.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use jclean_core::cancel::CancelToken;
use jclean_core::env::Env;
use jclean_core::map::{self, MapCell};
use jclean_core::platform;
use jclean_core::rules::{Audience, Category, Method, Risk, RuleSet};
use jclean_core::scanner::{ScanEvent, ScanItem, ScanMode, ScanOptions, ScanResult, Scanner};
use jclean_core::time::now_secs;
use jclean_core::tools::SystemRunner;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;
use tauri::ipc::Channel;

/// Engine state shared by the commands.
pub struct Engine {
    env: Env,
    rules: RuleSet,
    last: Mutex<Option<Arc<ScanResult>>>,
    cancel: Mutex<Option<CancelToken>>,
}

impl Engine {
    pub fn new() -> Result<Self, String> {
        let env = Env::from_system().map_err(|e| e.to_string())?;
        let rules = RuleSet::builtin(env.os()).map_err(|e| e.to_string())?;
        Ok(Self {
            env,
            rules,
            last: Mutex::new(None),
            cancel: Mutex::new(None),
        })
    }

    fn cache_path(&self) -> PathBuf {
        platform::app_data_dir(&self.env).join("last-scan.json")
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Quick,
    Full,
}

#[derive(Debug, Clone, Copy, Serialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum RiskDto {
    Safe,
    Review,
    Caution,
    Info,
}

#[derive(Debug, Clone, Copy, Serialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum CategoryDto {
    Apps,
    Developer,
    System,
    Media,
    Documents,
    Other,
}

#[derive(Debug, Clone, Copy, Serialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum MethodDto {
    Delete,
    Trash,
    Command,
    None,
}

impl From<Risk> for RiskDto {
    fn from(r: Risk) -> Self {
        match r {
            Risk::Safe => Self::Safe,
            Risk::Review => Self::Review,
            Risk::Caution => Self::Caution,
            Risk::Info => Self::Info,
        }
    }
}

impl From<Category> for CategoryDto {
    fn from(c: Category) -> Self {
        match c {
            Category::Apps => Self::Apps,
            Category::Developer => Self::Developer,
            Category::System => Self::System,
            Category::Media => Self::Media,
            Category::Documents => Self::Documents,
            Category::Other => Self::Other,
        }
    }
}

impl From<Method> for MethodDto {
    fn from(m: Method) -> Self {
        match m {
            Method::Delete => Self::Delete,
            Method::Trash => Self::Trash,
            Method::Command => Self::Command,
            Method::None => Self::None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDto {
    pub name: String,
    pub root: String,
    pub active: bool,
}

/// A list item as the UI sees it. Sizes are numbers of bytes (JS numbers are
/// exact up to 9 PB). `#[specta(type = u32)]` only makes the exported
/// TypeScript type a plain `number`: these values are never NaN.
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ItemDto {
    pub id: String,
    pub rule_id: String,
    pub name: Option<String>,
    pub path: Option<String>,
    #[specta(type = u32)]
    pub bytes: f64,
    pub bytes_known: bool,
    #[specta(type = Option<u32>)]
    pub last_used: Option<f64>,
    pub risk: RiskDto,
    pub category: CategoryDto,
    pub method: MethodDto,
    pub cleanable: bool,
    pub blocked_reason: Option<String>,
    pub preselected: bool,
    pub may_share_blocks: bool,
    pub project: Option<ProjectDto>,
}

#[allow(clippy::cast_precision_loss)]
fn item_dto(item: &ScanItem) -> ItemDto {
    ItemDto {
        id: item.id.clone(),
        rule_id: item.rule_id.clone(),
        name: item.name.clone(),
        path: item.path.as_ref().map(|p| p.display().to_string()),
        bytes: item.bytes as f64,
        bytes_known: item.bytes_known,
        last_used: item.last_used.map(|t| t as f64),
        risk: item.risk.into(),
        category: item.category.into(),
        method: item.method.into(),
        cleanable: item.cleanable,
        blocked_reason: item
            .blocked
            .as_ref()
            .map(jclean_core::scanner::Blocked::message),
        preselected: item.preselected,
        may_share_blocks: item.may_share_blocks,
        project: item.project.as_ref().map(|p| ProjectDto {
            name: p.name.clone(),
            root: p.root.display().to_string(),
            active: p.active,
        }),
    }
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum ScanUpdate {
    Stage {
        label: String,
    },
    Progress {
        #[specta(type = u32)]
        fraction: f64,
    },
    Item {
        item: ItemDto,
    },
    Finished {
        partial: bool,
        notes: Vec<String>,
        has_tree: bool,
    },
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MapCellDto {
    pub id: String,
    pub name: String,
    #[specta(type = u32)]
    pub bytes: f64,
    pub category: CategoryDto,
    #[specta(type = u32)]
    pub reclaimable: f64,
    pub item_id: Option<String>,
    pub has_children: bool,
    pub other: bool,
}

#[allow(clippy::cast_precision_loss)]
fn cell_dto(c: MapCell) -> MapCellDto {
    MapCellDto {
        id: c.id,
        name: c.name,
        bytes: c.bytes as f64,
        category: c.category.into(),
        reclaimable: c.reclaimable as f64,
        item_id: c.item_id,
        has_children: c.has_children,
        other: c.other,
    }
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CachedScan {
    /// Unix seconds.
    #[specta(type = u32)]
    pub saved_at: f64,
    pub items: Vec<ItemDto>,
}

#[derive(Serialize, Deserialize)]
struct CacheFile {
    saved_at: i64,
    scan: ScanResult,
}

/// Starts a scan on a background thread. Results stream through `on_update`;
/// the call returns immediately.
#[tauri::command]
#[specta::specta]
pub fn start_scan(
    engine: State<'_, Arc<Engine>>,
    mode: Mode,
    on_update: Channel<ScanUpdate>,
) -> Result<(), String> {
    let engine = Arc::clone(&engine);
    let cancel = CancelToken::new();
    if let Some(previous) = lock(&engine.cancel).replace(cancel.clone()) {
        previous.cancel();
    }

    std::thread::Builder::new()
        .name("jclean-scan".to_string())
        .spawn(move || {
            let mode = match mode {
                Mode::Quick => ScanMode::Quick,
                Mode::Full => ScanMode::Full,
            };
            // Always scan everything; Everyday mode filters in the UI so switching is instant.
            let opts = ScanOptions::new(mode, Audience::Developer);
            let runner = SystemRunner;
            let scanner = Scanner {
                env: &engine.env,
                rules: &engine.rules,
                runner: &runner,
            };
            let last_percent = AtomicUsize::new(usize::MAX);
            let result = scanner.scan(&opts, &cancel, &|event| {
                let update = match event {
                    ScanEvent::Stage(label) => ScanUpdate::Stage {
                        label: label.to_string(),
                    },
                    ScanEvent::Item(item) => ScanUpdate::Item {
                        item: item_dto(item),
                    },
                    ScanEvent::Progress { done, total } => {
                        let percent = (done * 100).checked_div(total).unwrap_or(100);
                        if last_percent.swap(percent, Ordering::Relaxed) == percent {
                            return;
                        }
                        #[allow(clippy::cast_precision_loss)]
                        ScanUpdate::Progress {
                            fraction: percent as f64 / 100.0,
                        }
                    }
                };
                let _ = on_update.send(update);
            });

            let notes = result.notes.iter().map(|n| n.message.clone()).collect();
            let finished = ScanUpdate::Finished {
                partial: result.cancelled,
                notes,
                has_tree: !result.tree.is_empty(),
            };
            let result = Arc::new(result);
            if !result.cancelled {
                save_cache(&engine, &result);
            }
            *lock(&engine.last) = Some(result);
            let _ = on_update.send(finished);
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Stops the running scan; what was found so far is kept.
#[tauri::command]
#[specta::specta]
pub fn cancel_scan(engine: State<'_, Arc<Engine>>) {
    if let Some(cancel) = lock(&engine.cancel).as_ref() {
        cancel.cancel();
    }
}

/// One level of the full scan's folder map. `""` is the top level.
#[tauri::command]
#[specta::specta]
pub fn map_level(engine: State<'_, Arc<Engine>>, id: String) -> Result<Vec<MapCellDto>, String> {
    let last = lock(&engine.last)
        .clone()
        .ok_or("Nothing has been scanned yet")?;
    map::children(&last, &engine.env, &id)
        .map(|cells| cells.into_iter().map(cell_dto).collect())
        .ok_or_else(|| {
            "There's no folder map for this scan. Run a full scan to see one.".to_string()
        })
}

#[tauri::command]
#[specta::specta]
pub fn volume_info() -> Option<crate::volume::VolumeInfo> {
    crate::volume::main_volume()
}

/// The last completed scan, shown instantly at launch until a fresh one
/// finishes (spec §10).
#[tauri::command]
#[specta::specta]
pub fn cached_scan(engine: State<'_, Arc<Engine>>) -> Option<CachedScan> {
    let text = std::fs::read_to_string(engine.cache_path()).ok()?;
    let file: CacheFile = serde_json::from_str(&text).ok()?;
    #[allow(clippy::cast_precision_loss)]
    let cached = CachedScan {
        saved_at: file.saved_at as f64,
        items: file.scan.items.iter().map(item_dto).collect(),
    };
    let mut last = lock(&engine.last);
    if last.is_none() {
        *last = Some(Arc::new(file.scan));
    }
    Some(cached)
}

/// Saves the scan without its folder tree, which can be large and is only
/// useful while fresh.
fn save_cache(engine: &Engine, result: &ScanResult) {
    let mut scan = result.clone();
    scan.tree.clear();
    let file = CacheFile {
        saved_at: now_secs(),
        scan,
    };
    let path = engine.cache_path();
    if let (Some(dir), Ok(json)) = (path.parent(), serde_json::to_string(&file)) {
        let tmp = path.with_extension("json.tmp");
        // Write then rename, so a crash never leaves a half-written cache.
        if std::fs::create_dir_all(dir).is_ok() && std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

/// The user's home folder, for showing paths as `~/…`.
pub fn home(engine: &Engine) -> String {
    engine.env.home().display().to_string()
}
