//! Bridges `jclean-core` to the UI: scans run on a background thread and
//! stream results through a Tauri channel (spec §3); the last result stays
//! here so the map can be browsed level by level and cleaned from later.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use jclean_core::cancel::CancelToken;
use jclean_core::cleaner::{SystemTrash, Trasher};
use jclean_core::env::Env;
use jclean_core::history::History;
use jclean_core::map::{self, MapCell};
use jclean_core::planner::CleanPlan;
use jclean_core::platform;
use jclean_core::rules::{Audience, Category, Method, Risk, RuleSet};
use jclean_core::scanner::{ScanEvent, ScanItem, ScanMode, ScanOptions, ScanResult, Scanner};
use jclean_core::time::now_secs;
use jclean_core::tools::{CommandRunner, SystemRunner};

use crate::settings::Settings;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;
use tauri::ipc::Channel;

/// Engine state shared by the commands.
pub struct Engine {
    pub(crate) env: Env,
    /// Built-in rules, embedded at build time.
    pub(crate) builtin: RuleSet,
    /// Built-in plus custom rules; rebuilt when settings change.
    rules: RwLock<Arc<RuleSet>>,
    pub(crate) settings: Mutex<Settings>,
    pub(crate) last: Mutex<Option<Arc<ScanResult>>>,
    cancel: Mutex<Option<CancelToken>>,
    /// The deletion log and scan history (spec §7.5, §10). `None` if the
    /// database couldn't be opened; cleaning then refuses to run.
    pub(crate) history: Option<History>,
    pub(crate) last_scan_id: Mutex<Option<i64>>,
    /// The plan shown in the confirmation sheet, run on confirm.
    pub(crate) plan: Mutex<Option<CleanPlan>>,
    pub(crate) runner: Arc<dyn CommandRunner>,
    pub(crate) trasher: Arc<dyn Trasher>,
}

impl Engine {
    pub fn new() -> Result<Self, String> {
        let (env, runner, trasher) = environment()?;
        let builtin = RuleSet::builtin(env.os()).map_err(|e| e.to_string())?;
        let settings = Settings::load(&crate::settings::settings_path(&env));
        let (rules, _) = settings.rule_set(&builtin, &env);
        let history = History::open(&platform::app_data_dir(&env).join("history.sqlite")).ok();
        if let Some(h) = &history {
            let _ = h.prune(now_secs());
        }
        Ok(Self {
            env,
            builtin,
            rules: RwLock::new(Arc::new(rules)),
            settings: Mutex::new(settings),
            last: Mutex::new(None),
            cancel: Mutex::new(None),
            history,
            last_scan_id: Mutex::new(None),
            plan: Mutex::new(None),
            runner,
            trasher,
        })
    }

    /// The rules in force now.
    pub(crate) fn rules(&self) -> Arc<RuleSet> {
        Arc::clone(&self.rules.read().unwrap_or_else(|p| p.into_inner()))
    }

    pub(crate) fn apply_settings(&self, settings: Settings) {
        let (rules, _) = settings.rule_set(&self.builtin, &self.env);
        *self.rules.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(rules);
        *lock(&self.settings) = settings;
    }

    /// Scan options from Settings (spec §4.4, §4.5, §5.11).
    pub(crate) fn scan_options(&self, mode: ScanMode) -> ScanOptions {
        let s = lock(&self.settings);
        // Always scan everything; Everyday mode filters in the UI so switching is instant.
        let mut opts = ScanOptions::new(mode, Audience::Developer);
        opts.scan_roots = s
            .project_roots
            .iter()
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .collect();
        opts.excluded = s.excluded_folders.iter().map(PathBuf::from).collect();
        opts.inactive_after_days = s.inactive_after_days;
        opts.disabled_rules = s.disabled_rules.iter().cloned().collect();
        opts
    }

    pub(crate) fn plan_options(&self) -> jclean_core::planner::PlanOptions {
        jclean_core::planner::PlanOptions {
            delete_user_files: lock(&self.settings).delete_user_files,
        }
    }

    pub(crate) fn cache_path(&self) -> PathBuf {
        platform::app_data_dir(&self.env).join("last-scan.json")
    }
}

type Environment = (Env, Arc<dyn CommandRunner>, Arc<dyn Trasher>);

/// The real machine. In dev builds, `JCLEAN_DEV_ROOT=<dir>` points the app at
/// a fixture made by `jclean-cli fixture <dir>` instead, so cleaning can be
/// tried end to end without touching real files: no tools run, and the
/// Trash is the fixture's own.
fn environment() -> Result<Environment, String> {
    #[cfg(debug_assertions)]
    if let Some(dir) = std::env::var_os("JCLEAN_DEV_ROOT") {
        let root =
            std::fs::canonicalize(PathBuf::from(dir).join("root")).map_err(|e| e.to_string())?;
        let env = Env::new(
            root.join("Users/tester"),
            &root,
            jclean_core::env::Os::current(),
        );
        let trash = FixtureTrash(env.home().join(".Trash"));
        return Ok((env, Arc::new(NoTools), Arc::new(trash)));
    }
    let env = Env::from_system().map_err(|e| e.to_string())?;
    Ok((env, Arc::new(SystemRunner), Arc::new(SystemTrash)))
}

/// Dev fixtures have no tools, and real ones must never run against them.
#[cfg(debug_assertions)]
struct NoTools;

#[cfg(debug_assertions)]
impl CommandRunner for NoTools {
    fn run(
        &self,
        program: &std::path::Path,
        _args: &[String],
        _timeout: std::time::Duration,
    ) -> Result<jclean_core::tools::CommandOutput, jclean_core::tools::CommandError> {
        Err(jclean_core::tools::CommandError::Spawn {
            program: program.display().to_string(),
            source: std::io::Error::other("tools are disabled for dev fixtures"),
        })
    }

    fn find_tool(&self, _env: &Env, _name: &str) -> Option<PathBuf> {
        None
    }
}

/// Moves items into the fixture's own `.Trash`, never the real one.
#[cfg(debug_assertions)]
struct FixtureTrash(PathBuf);

#[cfg(debug_assertions)]
impl Trasher for FixtureTrash {
    fn trash(&self, path: &std::path::Path) -> Result<(), String> {
        let name = path.file_name().ok_or("no file name")?;
        std::fs::create_dir_all(&self.0).map_err(|e| e.to_string())?;
        let mut dest = self.0.join(name);
        let mut n = 1;
        while dest.exists() {
            dest = self.0.join(format!("{} {n}", name.to_string_lossy()));
            n += 1;
        }
        std::fs::rename(path, dest).map_err(|e| e.to_string())
    }
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
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
pub(crate) fn item_dto(item: &ScanItem) -> ItemDto {
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
        /// Rules whose locations need Full Disk Access (spec §11).
        needs_access: Vec<String>,
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
    pub needs_access: Vec<String>,
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
    app: tauri::AppHandle,
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
            let opts = engine.scan_options(mode);
            let rules = engine.rules();
            let scanner = Scanner {
                env: &engine.env,
                rules: &rules,
                runner: engine.runner.as_ref(),
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
                needs_access: result.needs_access.clone(),
            };
            let result = Arc::new(result);
            if !result.cancelled {
                save_cache(&engine, &result);
                if let Some(h) = &engine.history {
                    let total = result.items.iter().map(|i| i.bytes).sum();
                    let mode = if result.mode == ScanMode::Full {
                        "full"
                    } else {
                        "quick"
                    };
                    let id = h
                        .record_scan(
                            result.started_at,
                            result.finished_at,
                            mode,
                            total,
                            result.reclaimable(),
                            result.items.len(),
                        )
                        .ok();
                    *lock(&engine.last_scan_id) = id;
                }
            }
            *lock(&engine.last) = Some(result);
            let _ = on_update.send(finished);
            crate::tray::refresh(&app);
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
        needs_access: file.scan.needs_access.clone(),
    };
    let mut last = lock(&engine.last);
    if last.is_none() {
        *last = Some(Arc::new(file.scan));
    }
    Some(cached)
}

/// Saves the scan without its folder tree, which can be large and is only
/// useful while fresh.
pub(crate) fn save_cache(engine: &Engine, result: &ScanResult) {
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
