//! Turning a selection into a [`CleanPlan`] (spec §9, step 2): the items,
//! how each is cleaned, the total, and what the user must be told first.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;

use crate::env::Env;
use crate::rules::{Method, Risk, RuleSet, RuleSource};
use crate::safety::{Root, Snapshot};
use crate::scanner::{ScanItem, ScanResult};
use crate::tools::CommandRunner;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    /// What the app pre-selects: safe, unused items (spec §9, step 1).
    Preselected,
    /// Every cleanable safe item, used or not.
    AllSafe,
    Ids(Vec<String>),
}

impl Selection {
    pub fn resolve(&self, scan: &ScanResult) -> Vec<String> {
        match self {
            Self::Preselected => scan
                .items
                .iter()
                .filter(|i| i.preselected)
                .map(|i| i.id.clone())
                .collect(),
            Self::AllSafe => scan
                .items
                .iter()
                .filter(|i| i.cleanable && i.risk == Risk::Safe)
                .map(|i| i.id.clone())
                .collect(),
            Self::Ids(ids) => ids.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
}

impl PlannedCommand {
    /// For logs and the confirmation sheet, e.g. `npm cache clean --force`.
    pub fn display(&self) -> String {
        let name = self.program.file_name().map_or_else(
            || self.program.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        std::iter::once(name)
            .chain(self.args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    pub item_id: String,
    pub rule_id: String,
    pub label: String,
    pub path: Option<PathBuf>,
    pub method: Method,
    pub command: Option<PlannedCommand>,
    pub bytes: u64,
    pub risk: Risk,
    pub regenerates: bool,
    pub cross_filesystems: bool,
    pub keep_root: bool,
    pub requires_admin: bool,
    pub related_apps: Vec<String>,
    pub snapshot: Snapshot,
    pub roots: Vec<Root>,
    pub excluded: Vec<PathBuf>,
    pub tool_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedItem {
    pub item_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MethodTotal {
    pub method: Method,
    pub items: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanPlan {
    pub scan_started_at: i64,
    pub items: Vec<PlanItem>,
    pub skipped: Vec<SkippedItem>,
    pub total_bytes: u64,
    pub by_method: Vec<MethodTotal>,
    /// `caution` items need an explicit second confirmation (spec §7.3).
    pub needs_second_confirmation: bool,
    pub needs_admin: bool,
    /// Apps to close first, across all items.
    pub related_apps: Vec<String>,
}

/// Choices from Settings that change how items are cleaned (spec §5.11).
#[derive(Debug, Clone, Copy, Default)]
pub struct PlanOptions {
    /// Delete user files that need review permanently instead of moving them
    /// to the Trash. Caution items and custom folders always go to the Trash.
    pub delete_user_files: bool,
}

pub fn build_plan(
    scan: &ScanResult,
    rules: &RuleSet,
    selected: &[String],
    env: &Env,
    runner: &dyn CommandRunner,
) -> CleanPlan {
    build_plan_with(scan, rules, selected, env, runner, PlanOptions::default())
}

pub fn build_plan_with(
    scan: &ScanResult,
    rules: &RuleSet,
    selected: &[String],
    env: &Env,
    runner: &dyn CommandRunner,
    options: PlanOptions,
) -> CleanPlan {
    let mut items = Vec::new();
    let mut skipped = Vec::new();
    let skip = |id: &str, reason: String| SkippedItem {
        item_id: id.to_string(),
        reason,
    };

    let mut seen = std::collections::HashSet::new();
    for id in selected {
        if !seen.insert(id.as_str()) {
            continue;
        }
        let Some(item) = scan.item(id) else {
            skipped.push(skip(id, "Not found in this scan".to_string()));
            continue;
        };
        let Some(rule) = rules.get(&item.rule_id) else {
            skipped.push(skip(id, "Its rule is no longer available".to_string()));
            continue;
        };
        if !item.cleanable {
            let reason = item
                .blocked
                .as_ref()
                .map_or_else(|| "Shown for information only".to_string(), |b| b.message());
            skipped.push(skip(id, reason));
            continue;
        }

        let mut method = item.method;
        if options.delete_user_files && method == Method::Trash && item.risk == Risk::Review {
            method = Method::Delete;
        }
        // Custom rules can only ever move to the Trash (spec §6.4), and caution
        // items are never deleted permanently (spec §7.3).
        if rule.source == RuleSource::Custom
            || item.risk == Risk::Caution && method == Method::Delete
        {
            method = Method::Trash;
        }
        let mut command = None;
        let uses_key = rule
            .cleanup
            .command
            .as_ref()
            .is_some_and(|c| c.args.iter().any(|a| a.contains("{item}")));
        if method == Method::Command && uses_key && !is_safe_argument(&item.key) {
            // A name like `--all` would turn into a flag for the tool.
            skipped.push(skip(
                id,
                "Its name can't be passed to the tool safely".to_string(),
            ));
            continue;
        }
        if method == Method::Command {
            match plan_command(item, rule, env, runner) {
                Some(cmd) => command = Some(cmd),
                None => match rule.cleanup.fallback {
                    Some(fallback) => method = fallback,
                    None => {
                        let tool = rule
                            .cleanup
                            .command
                            .as_ref()
                            .map_or("tool", |c| c.tool.as_str());
                        skipped.push(skip(id, format!("Tool not found: {tool}")));
                        continue;
                    }
                },
            }
        }
        if method != Method::Command && item.path.is_none() {
            skipped.push(skip(
                id,
                "Only its tool can clean it, and the tool isn't available".to_string(),
            ));
            continue;
        }

        items.push(PlanItem {
            item_id: item.id.clone(),
            rule_id: rule.id.clone(),
            label: label(item, &rule.labels.developer),
            path: item.path.clone(),
            method,
            command,
            bytes: item.bytes,
            risk: item.risk,
            regenerates: rule.regenerates,
            cross_filesystems: rule.cross_filesystems,
            keep_root: rule.cleanup.keep_root,
            requires_admin: rule.cleanup.requires_admin,
            related_apps: rule.related_apps.clone(),
            snapshot: item.snapshot,
            roots: item.roots.clone(),
            excluded: item.excluded.clone(),
            tool_paths: item.tool_paths.clone(),
        });
    }

    let mut by_method: BTreeMap<&'static str, MethodTotal> = BTreeMap::new();
    for item in &items {
        let entry = by_method
            .entry(item.method.as_str())
            .or_insert(MethodTotal {
                method: item.method,
                items: 0,
                bytes: 0,
            });
        entry.items += 1;
        entry.bytes += item.bytes;
    }
    let mut related_apps: Vec<String> = items
        .iter()
        .flat_map(|i| i.related_apps.iter().cloned())
        .collect();
    related_apps.sort();
    related_apps.dedup();

    CleanPlan {
        scan_started_at: scan.started_at,
        total_bytes: items.iter().map(|i| i.bytes).sum(),
        needs_second_confirmation: items.iter().any(|i| i.risk == Risk::Caution),
        needs_admin: items.iter().any(|i| i.requires_admin),
        by_method: by_method.into_values().collect(),
        related_apps,
        items,
        skipped,
    }
}

fn plan_command(
    item: &ScanItem,
    rule: &crate::rules::Rule,
    env: &Env,
    runner: &dyn CommandRunner,
) -> Option<PlannedCommand> {
    let cmd = rule.cleanup.command.as_ref()?;
    let program = runner.find_tool(env, &cmd.tool)?;
    let args = cmd
        .args
        .iter()
        .map(|a| a.replace("{item}", &item.key))
        .collect();
    Some(PlannedCommand { program, args })
}

fn is_safe_argument(key: &str) -> bool {
    !key.is_empty() && !key.starts_with('-') && !key.contains(['\0', '\n'])
}

fn label(item: &ScanItem, rule_label: &str) -> String {
    match &item.name {
        Some(name) => format!("{rule_label} · {name}"),
        None => rule_label.to_string(),
    }
}
