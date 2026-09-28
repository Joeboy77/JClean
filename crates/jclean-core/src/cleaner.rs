//! The only module that removes anything (CLAUDE.md safety rule 1).
//!
//! Every path is re-verified by the [`SafetyGuard`] immediately before it's
//! touched, every action is written to the deletion log, and dry runs go
//! through exactly the same checks without changing anything.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde::Serialize;

use crate::cancel::CancelToken;
use crate::history::{History, NewAction};
use crate::planner::{CleanPlan, PlanItem};
use crate::rules::Method;
use crate::safety::{ProcessChecker, SafetyGuard, Target};
use crate::tools::{CommandRunner, DEFAULT_TIMEOUT};

/// Moves things to the Trash. Tests substitute a fake so nothing ever lands
/// in a real Trash.
pub trait Trasher: Send + Sync {
    fn trash(&self, path: &Path) -> Result<(), String>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemTrash;

impl Trasher for SystemTrash {
    fn trash(&self, path: &Path) -> Result<(), String> {
        #[allow(unused_mut)]
        let mut ctx = trash::TrashContext::default();
        #[cfg(target_os = "macos")]
        {
            use trash::macos::{DeleteMethod, TrashContextExtMacos};
            // NSFileManager avoids asking the user to let JClean control Finder.
            ctx.set_delete_method(DeleteMethod::NsFileManager);
        }
        ctx.delete(path).map_err(|e| e.to_string())
    }
}

pub struct CleanContext<'a> {
    pub guard: &'a SafetyGuard,
    pub runner: &'a dyn CommandRunner,
    pub trasher: &'a dyn Trasher,
    pub processes: &'a dyn ProcessChecker,
    pub history: Option<&'a History>,
    pub scan_id: Option<i64>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Outcome {
    Cleaned { bytes: u64 },
    WouldClean { bytes: u64 },
    Skipped { reason: String },
    Failed { reason: String },
}

impl Outcome {
    fn log_name(&self) -> &'static str {
        match self {
            Self::Cleaned { .. } => "cleaned",
            Self::WouldClean { .. } => "dry-run",
            Self::Skipped { .. } => "skipped",
            Self::Failed { .. } => "failed",
        }
    }

    fn bytes(&self) -> u64 {
        match self {
            Self::Cleaned { bytes } | Self::WouldClean { bytes } => *bytes,
            Self::Skipped { .. } | Self::Failed { .. } => 0,
        }
    }

    fn reason(&self) -> Option<&str> {
        match self {
            Self::Skipped { reason } | Self::Failed { reason } => Some(reason),
            Self::Cleaned { .. } | Self::WouldClean { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemOutcome {
    pub item_id: String,
    pub label: String,
    pub method: Method,
    pub outcome: Outcome,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanReport {
    pub dry_run: bool,
    pub cleanup_id: Option<i64>,
    pub outcomes: Vec<ItemOutcome>,
    /// Estimated from the sizes measured right before cleaning.
    pub cleaned_bytes: u64,
    pub failed: usize,
    pub skipped: usize,
    /// Moved to the Trash: frees nothing until the Trash is emptied.
    pub trashed_bytes: u64,
}

#[derive(Debug)]
pub enum CleanEvent<'a> {
    Started { total: usize },
    ItemDone(&'a ItemOutcome),
}

pub fn execute(
    plan: &CleanPlan,
    ctx: &CleanContext<'_>,
    cancel: &CancelToken,
    on_event: &(dyn Fn(CleanEvent<'_>) + Sync),
) -> CleanReport {
    on_event(CleanEvent::Started {
        total: plan.items.len(),
    });
    let cleanup_id = ctx.history.and_then(|h| {
        h.start_cleanup(ctx.scan_id, plan.total_bytes, ctx.dry_run)
            .ok()
    });

    let finish = |item: &PlanItem, outcome: Outcome, logged_path: &str| {
        let result = ItemOutcome {
            item_id: item.item_id.clone(),
            label: item.label.clone(),
            method: item.method,
            outcome,
        };
        if let (Some(h), Some(id)) = (ctx.history, cleanup_id) {
            let _ = h.record_action(&NewAction {
                cleanup_id: id,
                rule_id: &item.rule_id,
                path: logged_path,
                method: item.method.as_str(),
                bytes: result.outcome.bytes(),
                outcome: result.outcome.log_name(),
                error: result.outcome.reason(),
            });
        }
        on_event(CleanEvent::ItemDone(&result));
        result
    };

    // Administrator items share one password prompt (spec §7.4).
    let (admin, regular): (Vec<&PlanItem>, Vec<&PlanItem>) =
        plan.items.iter().partition(|i| i.requires_admin);
    let mut outcomes: Vec<ItemOutcome> = Vec::new();
    for (item, outcome, logged) in run_admin(&admin, ctx, cancel) {
        outcomes.push(finish(item, outcome, &logged));
    }

    // Independent path items run in parallel.
    let (commands, paths): (Vec<&PlanItem>, Vec<&PlanItem>) = regular
        .into_iter()
        .partition(|i| i.method == Method::Command);
    let path_outcomes: Vec<ItemOutcome> = paths
        .par_iter()
        .map(|item| {
            let outcome = clean_path(item, ctx, cancel);
            let logged = item
                .path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            finish(item, outcome, &logged)
        })
        .collect();
    outcomes.extend(path_outcomes);

    // Commands run one at a time; items sharing a command run it once.
    let mut groups: BTreeMap<String, Vec<&PlanItem>> = BTreeMap::new();
    for item in commands {
        let key = item
            .command
            .as_ref()
            .map(|c| format!("{}\0{}", c.program.display(), c.args.join("\0")))
            .unwrap_or_default();
        groups.entry(key).or_default().push(item);
    }
    for group in groups.values() {
        let results = run_command_group(group, ctx, cancel);
        for (item, outcome) in group.iter().zip(results) {
            let logged = item
                .command
                .as_ref()
                .map(|c| c.display())
                .unwrap_or_default();
            outcomes.push(finish(item, outcome, &logged));
        }
    }

    let cleaned_bytes = outcomes
        .iter()
        .filter(|o| matches!(o.outcome, Outcome::Cleaned { .. }))
        .map(|o| o.outcome.bytes())
        .sum();
    let trashed_bytes = outcomes
        .iter()
        .filter(|o| o.method == Method::Trash && matches!(o.outcome, Outcome::Cleaned { .. }))
        .map(|o| o.outcome.bytes())
        .sum();
    if let (Some(h), Some(id)) = (ctx.history, cleanup_id) {
        let _ = h.finish_cleanup(id, cleaned_bytes);
    }

    CleanReport {
        dry_run: ctx.dry_run,
        cleanup_id,
        failed: outcomes
            .iter()
            .filter(|o| matches!(o.outcome, Outcome::Failed { .. }))
            .count(),
        skipped: outcomes
            .iter()
            .filter(|o| matches!(o.outcome, Outcome::Skipped { .. }))
            .count(),
        outcomes,
        cleaned_bytes,
        trashed_bytes,
    }
}

const OSASCRIPT: &str = "/usr/bin/osascript";
/// Long enough for someone to find and type their password.
const ADMIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// Runs every administrator item after one password prompt. Paths are
/// verified by the SafetyGuard first; only allowlisted tools run.
fn run_admin<'p>(
    items: &[&'p PlanItem],
    ctx: &CleanContext<'_>,
    cancel: &CancelToken,
) -> Vec<(&'p PlanItem, Outcome, String)> {
    use crate::platform::privileged::{PrivilegedOp, script, succeeded};

    let mut done = Vec::new();
    // For each item still going ahead: its ops' indexes and its size.
    let mut pending: Vec<(&PlanItem, std::ops::Range<usize>, u64, String)> = Vec::new();
    let mut ops: Vec<PrivilegedOp> = Vec::new();
    for item in items {
        if cancel.is_cancelled() {
            done.push((*item, skipped("Cancelled"), String::new()));
            continue;
        }
        let running = ctx.processes.running(&item.related_apps);
        if !running.is_empty() {
            done.push((
                *item,
                skipped(format!("Close {} first", running.join(", "))),
                String::new(),
            ));
            continue;
        }
        let start = ops.len();
        match (&item.command, &item.path) {
            (Some(cmd), _) if item.method == Method::Command => {
                ops.push(PrivilegedOp::Command {
                    program: cmd.program.clone(),
                    args: cmd.args.clone(),
                });
                pending.push((*item, start..ops.len(), item.bytes, cmd.display()));
            }
            (_, Some(path)) => {
                let target = Target {
                    path,
                    roots: &item.roots,
                    snapshot: item.snapshot,
                    regenerates: item.regenerates,
                    cross_filesystems: item.cross_filesystems,
                    exclude: &item.excluded,
                    related_apps: &item.related_apps,
                };
                let verified = match ctx.guard.check(&target, ctx.processes, cancel) {
                    Ok(v) => v,
                    Err(refusal) => {
                        done.push((
                            *item,
                            skipped(refusal.to_string()),
                            path.display().to_string(),
                        ));
                        continue;
                    }
                };
                if verified.is_dir && item.keep_root {
                    // Clear the contents, keep the folder: one op per entry inside.
                    let Ok(entries) = fs::read_dir(&verified.path) else {
                        done.push((
                            *item,
                            Outcome::Failed {
                                reason: "JClean couldn't list what's inside".to_string(),
                            },
                            path.display().to_string(),
                        ));
                        continue;
                    };
                    for entry in entries.flatten() {
                        ops.push(PrivilegedOp::Remove(entry.path()));
                    }
                } else {
                    ops.push(PrivilegedOp::Remove(verified.path.clone()));
                }
                pending.push((
                    *item,
                    start..ops.len(),
                    verified.allocated,
                    path.display().to_string(),
                ));
            }
            _ => done.push((
                *item,
                skipped("Nothing to clean at a known location"),
                String::new(),
            )),
        }
    }
    if pending.is_empty() {
        return done;
    }

    if ctx.dry_run {
        done.extend(
            pending
                .into_iter()
                .map(|(i, _, bytes, logged)| (i, Outcome::WouldClean { bytes }, logged)),
        );
        return done;
    }

    let fail_all = |pending: Vec<(&'p PlanItem, std::ops::Range<usize>, u64, String)>,
                    outcome: Outcome| {
        pending
            .into_iter()
            .map(move |(i, _, _, logged)| (i, outcome.clone(), logged))
    };
    let script = match script(
        &ops,
        "JClean needs your password to clean system files you selected.",
    ) {
        Ok(s) => s,
        Err(e) => {
            done.extend(fail_all(
                pending,
                Outcome::Failed {
                    reason: e.to_string(),
                },
            ));
            return done;
        }
    };
    match ctx.runner.run(
        Path::new(OSASCRIPT),
        &["-e".to_string(), script],
        ADMIN_TIMEOUT,
    ) {
        Ok(out) if out.success() => {
            let ok = succeeded(&out.stdout, ops.len());
            for (item, range, bytes, logged) in pending {
                let all_ok = range.clone().all(|n| ok.get(n).copied().unwrap_or(false));
                let outcome = if all_ok {
                    Outcome::Cleaned { bytes }
                } else {
                    Outcome::Failed {
                        reason: "macOS didn't allow part of it to be removed".to_string(),
                    }
                };
                done.push((item, outcome, logged));
            }
        }
        // AppleScript error -128: the password prompt was cancelled.
        Ok(out) if out.stderr.contains("-128") => {
            done.extend(fail_all(
                pending,
                skipped("You cancelled the password request"),
            ));
        }
        Ok(out) => {
            let msg = out
                .stderr
                .lines()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("it didn't finish")
                .trim()
                .to_string();
            done.extend(fail_all(
                pending,
                Outcome::Failed {
                    reason: format!("Administrator cleanup failed: {msg}"),
                },
            ));
        }
        Err(e) => done.extend(fail_all(
            pending,
            Outcome::Failed {
                reason: e.to_string(),
            },
        )),
    }
    done
}

fn skipped(reason: impl Into<String>) -> Outcome {
    Outcome::Skipped {
        reason: reason.into(),
    }
}

fn clean_path(item: &PlanItem, ctx: &CleanContext<'_>, cancel: &CancelToken) -> Outcome {
    if cancel.is_cancelled() {
        return skipped("Cancelled");
    }
    let Some(path) = &item.path else {
        return skipped("Nothing to clean at a known location");
    };
    let target = Target {
        path,
        roots: &item.roots,
        snapshot: item.snapshot,
        regenerates: item.regenerates,
        cross_filesystems: item.cross_filesystems,
        exclude: &item.excluded,
        related_apps: &item.related_apps,
    };
    let verified = match ctx.guard.check(&target, ctx.processes, cancel) {
        Ok(v) => v,
        Err(refusal) => return skipped(refusal.to_string()),
    };
    let bytes = if verified.is_symlink {
        0
    } else {
        verified.allocated
    };
    if ctx.dry_run {
        return Outcome::WouldClean { bytes };
    }

    // Paths inside this item that belong to other items, rebased onto the verified path.
    let keep: HashSet<PathBuf> = item
        .excluded
        .iter()
        .filter_map(|e| e.strip_prefix(path).ok())
        .map(|rel| verified.path.join(rel))
        .collect();

    let result = match item.method {
        Method::Delete => remove(
            &verified.path,
            verified.is_dir && (item.keep_root || !keep.is_empty()),
            &keep,
            &|p| delete_entry(p),
        ),
        Method::Trash => remove(
            &verified.path,
            verified.is_dir && (item.keep_root || !keep.is_empty()),
            &keep,
            &|p| ctx.trasher.trash(p),
        ),
        Method::Command | Method::None => Err("This item can't be cleaned by path".to_string()),
    };
    match result {
        Ok(()) => Outcome::Cleaned { bytes },
        Err(reason) => Outcome::Failed { reason },
    }
}

/// Removes `path`, or only its contents when `contents_only`, never touching
/// anything in `keep`.
fn remove(
    path: &Path,
    contents_only: bool,
    keep: &HashSet<PathBuf>,
    op: &dyn Fn(&Path) -> Result<(), String>,
) -> Result<(), String> {
    if !contents_only {
        return op(path);
    }
    let entries = fs::read_dir(path).map_err(|e| describe(&e))?;
    let mut errors = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| describe(&e))?;
        let child = entry.path();
        if keep.contains(&child) {
            continue;
        }
        let holds_kept = keep.iter().any(|k| k.starts_with(&child));
        let result = if holds_kept && fs::symlink_metadata(&child).is_ok_and(|m| m.is_dir()) {
            remove(&child, true, keep, op)
        } else {
            op(&child)
        };
        if let Err(e) = result {
            errors.push(e);
        }
    }
    match errors.len() {
        0 => Ok(()),
        1 => Err(errors.remove(0)),
        n => Err(format!(
            "{n} items couldn't be removed. First: {}",
            errors[0]
        )),
    }
}

/// The only permanent deletion in the codebase. A symlink is removed as a
/// link; `remove_dir_all` never follows symlinks inside the tree.
fn delete_entry(path: &Path) -> Result<(), String> {
    let meta = fs::symlink_metadata(path).map_err(|e| describe(&e))?;
    if meta.is_dir() {
        fs::remove_dir_all(path).map_err(|e| describe(&e))
    } else {
        fs::remove_file(path).map_err(|e| describe(&e))
    }
}

fn describe(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => {
            "JClean doesn't have permission to remove it".to_string()
        }
        std::io::ErrorKind::NotFound => "It's already gone".to_string(),
        _ => e.to_string(),
    }
}

fn run_command_group(
    group: &[&PlanItem],
    ctx: &CleanContext<'_>,
    cancel: &CancelToken,
) -> Vec<Outcome> {
    let all = |o: Outcome| vec![o; group.len()];
    let Some(first) = group.first() else {
        return Vec::new();
    };
    if cancel.is_cancelled() {
        return all(skipped("Cancelled"));
    }
    let Some(cmd) = &first.command else {
        return all(Outcome::Failed {
            reason: "No command to run".to_string(),
        });
    };
    let mut apps: Vec<String> = group
        .iter()
        .flat_map(|i| i.related_apps.iter().cloned())
        .collect();
    apps.sort();
    apps.dedup();
    let running = ctx.processes.running(&apps);
    if !running.is_empty() {
        return all(skipped(format!("Close {} first", running.join(", "))));
    }
    if ctx.dry_run {
        return group
            .iter()
            .map(|i| Outcome::WouldClean { bytes: i.bytes })
            .collect();
    }
    match ctx.runner.run(&cmd.program, &cmd.args, DEFAULT_TIMEOUT) {
        Ok(out) if out.success() => group
            .iter()
            .map(|i| Outcome::Cleaned { bytes: i.bytes })
            .collect(),
        Ok(out) => {
            let msg = out
                .stderr
                .lines()
                .chain(out.stdout.lines())
                .map(str::trim)
                .find(|l| !l.is_empty())
                .unwrap_or("it exited with an error");
            all(Outcome::Failed {
                reason: format!("{} failed: {msg}", cmd.display()),
            })
        }
        Err(e) => all(Outcome::Failed {
            reason: e.to_string(),
        }),
    }
}
