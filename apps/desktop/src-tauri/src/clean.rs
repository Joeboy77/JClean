//! Cleaning (spec §9): plan the selection for the confirmation sheet, then
//! run it on a background thread through `cleaner::execute`, the only code
//! that deletes, streaming each item's outcome to the UI.

use std::collections::HashSet;
use std::sync::Arc;

use jclean_core::cancel::CancelToken;
use jclean_core::cleaner::{self, CleanContext, CleanEvent, Outcome};
use jclean_core::planner::{CleanPlan, Selection, build_plan, build_plan_with};
use jclean_core::rules::{Audience, Method, RuleSet};
use jclean_core::safety::{ProcessChecker, SafetyGuard, SystemProcesses};
use jclean_core::scanner::{ScanMode, ScanOptions, ScanResult, Scanner};
use serde::Serialize;
use specta::Type;
use tauri::State;
use tauri::ipc::Channel;

use crate::engine::{Engine, MethodDto, RiskDto, lock, save_cache};

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanItemDto {
    pub item_id: String,
    #[specta(type = u32)]
    pub bytes: f64,
    pub method: MethodDto,
    pub risk: RiskDto,
    /// e.g. `npm cache clean --force`.
    pub command: Option<String>,
    pub requires_admin: bool,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SkippedDto {
    pub item_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MethodTotalDto {
    pub method: MethodDto,
    pub items: u32,
    #[specta(type = u32)]
    pub bytes: f64,
}

/// What the confirmation sheet shows (spec §5.4).
#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanDto {
    #[specta(type = u32)]
    pub total_bytes: f64,
    pub items: Vec<PlanItemDto>,
    pub skipped: Vec<SkippedDto>,
    pub by_method: Vec<MethodTotalDto>,
    /// Tools that will run their own cleanup, e.g. "docker", "brew".
    pub tools: Vec<String>,
    /// `caution` items need an explicit second confirmation (spec §7.3).
    pub needs_second_confirmation: bool,
    /// Related apps that are running now; their items will be skipped.
    pub running_apps: Vec<String>,
}

#[allow(clippy::cast_precision_loss)]
fn plan_dto(plan: &CleanPlan, running_apps: Vec<String>) -> PlanDto {
    let mut tools: Vec<String> = plan
        .items
        .iter()
        .filter_map(|i| i.command.as_ref())
        .filter_map(|c| {
            c.program
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .collect();
    tools.sort();
    tools.dedup();
    PlanDto {
        total_bytes: plan.total_bytes as f64,
        items: plan
            .items
            .iter()
            .map(|i| PlanItemDto {
                item_id: i.item_id.clone(),
                bytes: i.bytes as f64,
                method: i.method.into(),
                risk: i.risk.into(),
                command: i
                    .command
                    .as_ref()
                    .map(jclean_core::planner::PlannedCommand::display),
                requires_admin: i.requires_admin,
            })
            .collect(),
        skipped: plan
            .skipped
            .iter()
            .map(|s| SkippedDto {
                item_id: s.item_id.clone(),
                reason: s.reason.clone(),
            })
            .collect(),
        by_method: plan
            .by_method
            .iter()
            .map(|m| MethodTotalDto {
                method: m.method.into(),
                items: u32::try_from(m.items).unwrap_or(u32::MAX),
                bytes: m.bytes as f64,
            })
            .collect(),
        tools,
        needs_second_confirmation: plan.needs_second_confirmation,
        running_apps,
    }
}

/// Builds the plan for the selected items and keeps it for `run_clean`.
#[tauri::command]
#[specta::specta]
pub fn plan_clean(
    engine: State<'_, Arc<Engine>>,
    item_ids: Vec<String>,
) -> Result<PlanDto, String> {
    let last = lock(&engine.last)
        .clone()
        .ok_or("Scan first, then choose what to clean")?;
    let plan = build_plan_with(
        &last,
        &engine.rules(),
        &item_ids,
        &engine.env,
        engine.runner.as_ref(),
        engine.plan_options(),
    );
    let running = SystemProcesses.running(&plan.related_apps);
    let dto = plan_dto(&plan, running);
    *lock(&engine.plan) = Some(plan);
    Ok(dto)
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum CleanUpdate {
    Started {
        total: u32,
    },
    Item {
        item_id: String,
        /// `cleaned`, `skipped` or `failed`.
        outcome: String,
        #[specta(type = u32)]
        bytes: f64,
        reason: Option<String>,
        method: MethodDto,
    },
    Finished(CleanSummary),
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CleanSummary {
    /// Measured just before cleaning, from the plan's items.
    #[specta(type = u32)]
    pub cleaned_bytes: f64,
    /// Of that, moved to the Trash: frees nothing until it's emptied.
    #[specta(type = u32)]
    pub trashed_bytes: f64,
    /// Change in the volume's free space, when it could be read (spec §9, 5).
    #[specta(type = Option<u32>)]
    pub measured_freed: Option<f64>,
    pub failed: u32,
    pub skipped: u32,
}

#[allow(clippy::cast_precision_loss)]
fn outcome_update(o: &cleaner::ItemOutcome) -> CleanUpdate {
    let (outcome, bytes, reason) = match &o.outcome {
        Outcome::Cleaned { bytes } | Outcome::WouldClean { bytes } => ("cleaned", *bytes, None),
        Outcome::Skipped { reason } => ("skipped", 0, Some(reason.clone())),
        Outcome::Failed { reason } => ("failed", 0, Some(reason.clone())),
    };
    CleanUpdate::Item {
        item_id: o.item_id.clone(),
        outcome: outcome.to_string(),
        bytes: bytes as f64,
        reason,
        method: o.method.into(),
    }
}

/// Runs the confirmed plan on a background thread. Failures never stop the
/// rest of the plan (spec §9, 6).
#[tauri::command]
#[specta::specta]
pub fn run_clean(
    app: tauri::AppHandle,
    engine: State<'_, Arc<Engine>>,
    on_update: Channel<CleanUpdate>,
) -> Result<(), String> {
    if engine.history.is_none() {
        return Err(
            "JClean couldn't open its deletion log, so it won't clean anything.".to_string(),
        );
    }
    let plan = lock(&engine.plan)
        .take()
        .ok_or("There's nothing to clean. Choose items and try again.")?;
    let engine = Arc::clone(&engine);
    std::thread::Builder::new()
        .name("jclean-clean".to_string())
        .spawn(move || {
            let summary = execute(&engine, &plan, &on_update);
            let _ = on_update.send(CleanUpdate::Finished(summary));
            crate::tray::refresh(&app);
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[allow(clippy::cast_precision_loss)]
fn execute(engine: &Engine, plan: &CleanPlan, on_update: &Channel<CleanUpdate>) -> CleanSummary {
    let before = crate::volume::main_volume();
    let guard = SafetyGuard::new(&engine.env);
    let scan_id = *lock(&engine.last_scan_id);
    let ctx = CleanContext {
        guard: &guard,
        runner: engine.runner.as_ref(),
        trasher: engine.trasher.as_ref(),
        processes: &SystemProcesses,
        history: engine.history.as_ref(),
        scan_id,
        dry_run: false,
    };
    let report = cleaner::execute(plan, &ctx, &CancelToken::new(), &|event| {
        let update = match event {
            CleanEvent::Started { total } => CleanUpdate::Started {
                total: u32::try_from(total).unwrap_or(u32::MAX),
            },
            CleanEvent::ItemDone(o) => outcome_update(o),
        };
        let _ = on_update.send(update);
    });

    // Forget what's gone, so the map and the cache stay true.
    let cleaned: HashSet<&str> = report
        .outcomes
        .iter()
        .filter(|o| matches!(o.outcome, Outcome::Cleaned { .. }))
        .map(|o| o.item_id.as_str())
        .collect();
    let mut last = lock(&engine.last);
    if let Some(scan) = last.as_ref() {
        let mut next: ScanResult = (**scan).clone();
        next.items.retain(|i| !cleaned.contains(i.id.as_str()));
        save_cache(engine, &next);
        *last = Some(Arc::new(next));
    }
    drop(last);

    let after = crate::volume::main_volume();
    let measured_freed = before
        .zip(after)
        .map(|(b, a)| (a.available - b.available).max(0.0));
    CleanSummary {
        cleaned_bytes: report.cleaned_bytes as f64,
        trashed_bytes: report.trashed_bytes as f64,
        measured_freed,
        failed: u32::try_from(report.failed).unwrap_or(u32::MAX),
        skipped: u32::try_from(report.skipped).unwrap_or(u32::MAX),
    }
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EmptyTrashResult {
    #[specta(type = u32)]
    pub freed: f64,
    pub failed: u32,
    /// Set when the Trash couldn't be read (usually missing Full Disk Access).
    pub note: Option<String>,
}

/// Empties the Trash (or Recycle Bin) through the same guard and log as any
/// clean: finds what's in it with the platform's trash rule and deletes it.
#[tauri::command]
#[specta::specta]
pub async fn empty_trash(engine: State<'_, Arc<Engine>>) -> Result<EmptyTrashResult, String> {
    if engine.history.is_none() {
        return Err(
            "JClean couldn't open its deletion log, so it won't clean anything.".to_string(),
        );
    }
    let engine = Arc::clone(&engine);
    tauri::async_runtime::spawn_blocking(move || empty_trash_blocking(&engine))
        .await
        .map_err(|e| e.to_string())?
}

#[allow(clippy::cast_precision_loss)]
fn empty_trash_blocking(engine: &Engine) -> Result<EmptyTrashResult, String> {
    let rule = engine
        .builtin
        .get(jclean_core::platform::trash_rule_id(engine.env.os()))
        .cloned()
        .ok_or("The Trash rule is missing")?;
    let rules = RuleSet::from_rules(vec![rule]).map_err(|e| e.to_string())?;
    let mut opts = ScanOptions::new(ScanMode::Quick, Audience::Everyday);
    opts.run_probes = false;
    let scan = Scanner {
        env: &engine.env,
        rules: &rules,
        runner: engine.runner.as_ref(),
    }
    .scan(&opts, &CancelToken::new(), &|_| {});
    if scan.items.is_empty() {
        let note = (scan.unreadable_dirs > 0).then(|| {
            if cfg!(windows) {
                "JClean couldn't look inside the Recycle Bin. Empty it from the desktop instead.".to_string()
            } else {
                "JClean couldn't look inside the Trash. Grant Full Disk Access, or empty it from the Dock.".to_string()
            }
        });
        return Ok(EmptyTrashResult {
            freed: 0.0,
            failed: 0,
            note,
        });
    }
    let ids = Selection::AllSafe.resolve(&scan);
    let plan = build_plan(&scan, &rules, &ids, &engine.env, engine.runner.as_ref());
    debug_assert!(plan.items.iter().all(|i| i.method == Method::Delete));
    let guard = SafetyGuard::new(&engine.env);
    let ctx = CleanContext {
        guard: &guard,
        runner: engine.runner.as_ref(),
        trasher: engine.trasher.as_ref(),
        processes: &SystemProcesses,
        history: engine.history.as_ref(),
        scan_id: None,
        dry_run: false,
    };
    let report = cleaner::execute(&plan, &ctx, &CancelToken::new(), &|_| {});
    Ok(EmptyTrashResult {
        freed: report.cleaned_bytes as f64,
        failed: u32::try_from(report.failed + report.skipped).unwrap_or(u32::MAX),
        note: None,
    })
}
