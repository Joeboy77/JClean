//! Scanning (spec §4): resolve active rules to concrete locations, settle
//! overlaps so nothing is counted twice, measure in parallel, and stream
//! items to the caller as they're found.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::ops::Bound;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::cancel::CancelToken;
use crate::disktree::{self, DEFAULT_FILE_THRESHOLD, LargeFile, TreeNode, TreeOptions};
use crate::env::Env;
use crate::platform;
use crate::probes::Prober;
use crate::projects::{self, ArtifactRule, DiscoverOptions, Project};
use crate::rules::{
    Audience, Category, Detect, Method, Risk, Rule, RuleSet, UnusedBasis, keep, paths,
};
use crate::safety::{Refusal, Root, SafetyGuard, Snapshot};
use crate::sizing::{InodeSet, Measure, MeasureOptions, SizeCache, SizeError, measure};
use crate::time::{DAY_SECS, now_secs};
use crate::tools::CommandRunner;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanMode {
    Quick,
    Full,
}

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub mode: ScanMode,
    pub audience: Audience,
    /// Where to look for projects and large files. Empty means the home folder.
    pub scan_roots: Vec<PathBuf>,
    /// Extra folders to leave out, on top of the platform defaults.
    pub excluded: Vec<PathBuf>,
    /// Projects untouched for this long are inactive (spec §4.4).
    pub inactive_after_days: u32,
    pub run_probes: bool,
    /// How deep a quick scan looks for projects below each root.
    pub quick_project_depth: usize,
    /// "Now", as Unix seconds. Injectable so tests are deterministic.
    pub now: i64,
    /// Rules switched off in Settings.
    pub disabled_rules: HashSet<String>,
    /// Folder totals from the last scan; used by quick scans only.
    pub size_cache: Option<Arc<SizeCache>>,
}

impl ScanOptions {
    pub fn new(mode: ScanMode, audience: Audience) -> Self {
        Self {
            mode,
            audience,
            scan_roots: Vec::new(),
            excluded: Vec::new(),
            inactive_after_days: 90,
            run_probes: true,
            quick_project_depth: 6,
            now: now_secs(),
            disabled_rules: HashSet::new(),
            size_cache: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanItem {
    /// Stable within a scan: `<rule id>:<path or probe key>`.
    pub id: String,
    pub rule_id: String,
    pub path: Option<PathBuf>,
    /// Item-specific name shown with the rule label (a cache's folder, a project).
    pub name: Option<String>,
    /// Substituted for `{item}` in cleanup commands.
    pub key: String,
    pub category: Category,
    pub group: String,
    /// The rule's risk, raised to `review` for build folders in active projects.
    pub risk: Risk,
    pub bytes: u64,
    /// False when a tool can't report a size.
    pub bytes_known: bool,
    pub files: u64,
    pub last_used: Option<i64>,
    pub project: Option<ProjectRef>,
    pub method: Method,
    pub cleanable: bool,
    pub blocked: Option<Blocked>,
    pub preselected: bool,
    pub may_share_blocks: bool,
    pub snapshot: Snapshot,
    pub roots: Vec<Root>,
    /// Paths inside this item that belong to other items.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub excluded: Vec<PathBuf>,
    /// Folders a tool-managed item occupies (probe items).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tool_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRef {
    pub root: PathBuf,
    pub name: String,
    pub active: bool,
}

/// Why an item is shown but can't be cleaned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Blocked {
    Safety { reason: Refusal },
    ToolMissing { tool: String },
}

impl Blocked {
    pub fn message(&self) -> String {
        match self {
            Self::Safety { reason } => reason.to_string(),
            Self::ToolMissing { tool } => format!("Tool not found: {tool}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanNote {
    pub rule_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub mode: ScanMode,
    pub started_at: i64,
    pub finished_at: i64,
    pub items: Vec<ScanItem>,
    pub projects: Vec<Project>,
    pub notes: Vec<ScanNote>,
    /// Folders that couldn't be read (usually missing Full Disk Access).
    pub unreadable_dirs: u64,
    /// Rules whose locations macOS kept from us, so the UI can show a
    /// "Needs Full Disk Access" row instead of nothing (spec §11).
    #[serde(default)]
    pub needs_access: Vec<String>,
    /// Results are partial.
    pub cancelled: bool,
    /// Full scan only: the disk map of the scan roots.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tree: Vec<TreeNode>,
}

impl ScanResult {
    pub fn item(&self, id: &str) -> Option<&ScanItem> {
        self.items.iter().find(|i| i.id == id)
    }

    /// Total of everything that could be cleaned.
    pub fn reclaimable(&self) -> u64 {
        self.items
            .iter()
            .filter(|i| i.cleanable)
            .map(|i| i.bytes)
            .sum()
    }
}

#[derive(Debug)]
pub enum ScanEvent<'a> {
    Stage(&'static str),
    Progress { done: usize, total: usize },
    Item(&'a ScanItem),
}

pub struct Scanner<'a> {
    pub env: &'a Env,
    pub rules: &'a RuleSet,
    pub runner: &'a dyn CommandRunner,
}

struct Candidate<'r> {
    rule: &'r Rule,
    path: PathBuf,
    roots: Vec<Root>,
    /// Length of the rule's literal root; the most specific rule wins overlaps.
    specificity: usize,
    name: Option<String>,
    project: Option<usize>,
    min_bytes: Option<u64>,
    exclude: Vec<PathBuf>,
    /// Found by the large-file search; dropped if any other item contains it.
    large_file: bool,
}

impl Scanner<'_> {
    pub fn scan(
        &self,
        opts: &ScanOptions,
        cancel: &CancelToken,
        on_event: &(dyn Fn(ScanEvent<'_>) + Sync),
    ) -> ScanResult {
        let started_at = now_secs();
        let env = self.env;
        let active: Vec<&Rule> = self
            .rules
            .active(env.os(), opts.audience)
            .filter(|r| !opts.disabled_rules.contains(&r.id))
            .collect();
        let guard = SafetyGuard::new(env);
        let mut notes = Vec::new();
        let mut unreadable = 0;

        let scan_roots = if opts.scan_roots.is_empty() {
            vec![env.home().to_path_buf()]
        } else {
            opts.scan_roots.clone()
        };
        let mut exclusions = platform::default_scan_exclusions(env);
        exclusions.extend(opts.excluded.iter().cloned());

        // Full scan: walk the scan roots once for the disk map and large files.
        let mut tree = Vec::new();
        let mut large_files: Vec<LargeFile> = Vec::new();
        let large_rules: Vec<&Rule> = active
            .iter()
            .copied()
            .filter(|r| {
                matches!(
                    r.detect,
                    Detect::Query {
                        recursive: true,
                        ..
                    }
                )
            })
            .collect();
        if opts.mode == ScanMode::Full {
            on_event(ScanEvent::Stage("Mapping the disk"));
            let summarize = platform::cloud_dirs(env);
            let large_min = large_rules
                .iter()
                .filter_map(|r| match r.detect {
                    Detect::Query { min_bytes, .. } => min_bytes,
                    _ => None,
                })
                .min();
            let tree_opts = TreeOptions {
                file_threshold: DEFAULT_FILE_THRESHOLD,
                summarize: &summarize,
                large_min_bytes: large_min,
                large_exclude: &exclusions,
            };
            let inodes = InodeSet::new();
            for root in &scan_roots {
                if let Some((node, large, bad)) = disktree::build(root, &tree_opts, &inodes, cancel)
                {
                    tree.push(node);
                    large_files.extend(large);
                    unreadable += bad;
                }
            }
        }

        on_event(ScanEvent::Stage("Finding storage"));
        let mut candidates: Vec<Candidate<'_>> = Vec::new();
        let mut denied: Vec<String> = Vec::new();
        for rule in &active {
            if let Err(err) = self.resolve_rule(rule, opts, &mut candidates, &mut denied) {
                notes.push(ScanNote {
                    rule_id: Some(rule.id.clone()),
                    message: err,
                });
            }
        }

        // Project build folders (spec §4.5).
        let artifact_rules: Vec<ArtifactRule<'_>> = active
            .iter()
            .filter_map(|r| ArtifactRule::compile(r))
            .collect();
        let projects = projects::discover(
            &artifact_rules,
            &DiscoverOptions {
                roots: &scan_roots,
                exclude: &exclusions,
                max_depth: (opts.mode == ScanMode::Quick).then_some(opts.quick_project_depth),
            },
            cancel,
        );
        let project_roots: Vec<Root> = scan_roots
            .iter()
            .map(|p| Root {
                path: p.clone(),
                inclusive: false,
            })
            .collect();
        for (idx, project) in projects.iter().enumerate() {
            for artifact in &project.artifacts {
                if let Some(rule) = self.rules.get(&artifact.rule_id) {
                    candidates.push(Candidate {
                        rule,
                        path: artifact.path.clone(),
                        roots: project_roots.clone(),
                        specificity: artifact.path.components().count(),
                        name: Some(project.name.clone()),
                        project: Some(idx),
                        min_bytes: None,
                        exclude: Vec::new(),
                        large_file: false,
                    });
                }
            }
        }

        // Large files: only those no other rule claims.
        for rule in &large_rules {
            let Detect::Query {
                older_than_days, ..
            } = rule.detect
            else {
                continue;
            };
            let cutoff = older_than_days.map(|d| opts.now - i64::from(d) * DAY_SECS);
            for file in &large_files {
                if cutoff.is_some_and(|c| file.mtime > c) {
                    continue;
                }
                candidates.push(Candidate {
                    rule,
                    path: file.path.clone(),
                    roots: project_roots.clone(),
                    specificity: 0,
                    name: file
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned()),
                    project: None,
                    min_bytes: None,
                    exclude: Vec::new(),
                    large_file: true,
                });
            }
        }

        // Probes (spec §3): tools that know their own storage.
        let mut items: Vec<ScanItem> = Vec::new();
        let mut probe_paths: HashSet<PathBuf> = HashSet::new();
        let inodes = InodeSet::new();
        if opts.run_probes {
            on_event(ScanEvent::Stage("Asking developer tools"));
            let prober = Prober::new(env, self.runner);
            for rule in &active {
                let Detect::Probe { probe } = &rule.detect else {
                    continue;
                };
                match prober.run(probe, &inodes, cancel) {
                    Ok(found) => {
                        for p in found {
                            probe_paths.extend(p.paths.iter().cloned());
                            let item = self.probe_item(rule, p, &guard);
                            on_event(ScanEvent::Item(&item));
                            items.push(item);
                        }
                    }
                    Err(err) => notes.push(ScanNote {
                        rule_id: Some(rule.id.clone()),
                        message: err.to_string(),
                    }),
                }
            }
        }

        let candidates = settle_overlaps(candidates, &probe_paths);

        on_event(ScanEvent::Stage("Measuring"));
        let total = candidates.len();
        let done = AtomicUsize::new(0);
        let found = Mutex::new(Vec::with_capacity(total));
        let denied = Mutex::new(denied);
        let unreadable_total = AtomicUsize::new(0);
        // Known locations first so results show up fast; project folders,
        // usually the bulk of the files, come second.
        let (fixed, project): (Vec<&Candidate<'_>>, Vec<&Candidate<'_>>) =
            candidates.iter().partition(|c| c.project.is_none());
        for batch in [fixed, project] {
            batch.par_iter().for_each(|c| {
                if cancel.is_cancelled() {
                    return;
                }
                let opts_m = MeasureOptions {
                    cross_filesystems: c.rule.cross_filesystems,
                    exclude: c.exclude.iter().cloned().collect(),
                    cache: if opts.mode == ScanMode::Quick {
                        opts.size_cache.clone()
                    } else {
                        None
                    },
                };
                match measure(&c.path, &opts_m, &inodes, cancel) {
                    Ok(m) => {
                        // Exists, but nothing inside could be listed: kept from us.
                        if m.is_dir
                            && m.files == 0
                            && m.unreadable_dirs > 0
                            && let Ok(mut d) = denied.lock()
                            && !d.contains(&c.rule.id)
                        {
                            d.push(c.rule.id.clone());
                        }
                        unreadable_total.fetch_add(
                            usize::try_from(m.unreadable_dirs).unwrap_or(usize::MAX),
                            Ordering::Relaxed,
                        );
                        if let Some(item) = self.item(c, &m, &projects, opts, &guard) {
                            on_event(ScanEvent::Item(&item));
                            if let Ok(mut f) = found.lock() {
                                f.push(item);
                            }
                        }
                    }
                    Err(SizeError::NotFound(_) | SizeError::Cancelled) => {}
                    Err(SizeError::Io { .. }) => {
                        unreadable_total.fetch_add(1, Ordering::Relaxed);
                    }
                }
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                on_event(ScanEvent::Progress { done: n, total });
            });
        }
        items.extend(found.into_inner().unwrap_or_default());
        items.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.id.cmp(&b.id)));
        unreadable += u64::try_from(unreadable_total.into_inner()).unwrap_or(u64::MAX);

        if unreadable > 0 {
            notes.push(ScanNote {
                rule_id: None,
                message: format!(
                    "{unreadable} folders couldn't be read. Grant Full Disk Access to include them."
                ),
            });
        }

        let mut needs_access = denied.into_inner().unwrap_or_default();
        // A rule that found something anyway isn't shown as blocked.
        needs_access.retain(|id| !items.iter().any(|i| &i.rule_id == id));
        needs_access.sort();

        ScanResult {
            mode: opts.mode,
            started_at,
            finished_at: now_secs(),
            items,
            projects,
            notes,
            unreadable_dirs: unreadable,
            needs_access,
            cancelled: cancel.is_cancelled(),
            tree,
        }
    }

    /// Turns a fixed or query rule into candidate paths.
    fn resolve_rule<'r>(
        &self,
        rule: &'r Rule,
        opts: &ScanOptions,
        out: &mut Vec<Candidate<'r>>,
        denied: &mut Vec<String>,
    ) -> Result<(), String> {
        let mut note_denied = |e: &std::io::Error| {
            if e.kind() == std::io::ErrorKind::PermissionDenied && !denied.contains(&rule.id) {
                denied.push(rule.id.clone());
            }
        };
        match &rule.detect {
            Detect::Fixed {
                paths: patterns,
                each_child,
                exclude,
            } => {
                let excluded: Vec<_> = exclude
                    .iter()
                    .filter_map(|g| paths::matcher(g).ok())
                    .collect();
                let mut found: Vec<(PathBuf, Root, usize)> = Vec::new();
                for pattern in patterns {
                    let Some(resolved) =
                        paths::resolve(pattern, self.env).map_err(|e| e.to_string())?
                    else {
                        continue;
                    };
                    let literal = paths::literal_prefix(&resolved);
                    let inclusive = literal == resolved;
                    for matched in paths::expand(&resolved).map_err(|e| e.to_string())? {
                        if *each_child {
                            let entries = match fs::read_dir(&matched) {
                                Ok(entries) => entries,
                                Err(e) => {
                                    note_denied(&e);
                                    continue;
                                }
                            };
                            let specificity = matched.components().count();
                            for entry in entries.filter_map(Result::ok) {
                                if excluded.iter().any(|m| m.is_match(entry.file_name())) {
                                    continue;
                                }
                                found.push((
                                    entry.path(),
                                    Root {
                                        path: matched.clone(),
                                        inclusive: false,
                                    },
                                    specificity,
                                ));
                            }
                        } else {
                            found.push((
                                matched,
                                Root {
                                    path: literal.clone(),
                                    inclusive,
                                },
                                literal.components().count(),
                            ));
                        }
                    }
                }
                if let Some(policy) = &rule.keep {
                    let (clean, _kept) =
                        keep::apply(found.iter().map(|f| f.0.clone()).collect(), policy);
                    let clean: HashSet<PathBuf> = clean.into_iter().collect();
                    found.retain(|f| clean.contains(&f.0));
                }
                for (path, root, specificity) in found {
                    let name = (root.path != path)
                        .then(|| path.file_name().map(|n| n.to_string_lossy().into_owned()))
                        .flatten();
                    out.push(Candidate {
                        rule,
                        path,
                        roots: vec![root],
                        specificity,
                        name,
                        project: None,
                        min_bytes: None,
                        exclude: Vec::new(),
                        large_file: false,
                    });
                }
            }
            Detect::Query {
                paths: patterns,
                names,
                older_than_days,
                min_bytes,
                recursive: false,
            } => {
                let matchers: Vec<_> = names
                    .iter()
                    .filter_map(|g| paths::matcher(g).ok())
                    .collect();
                let cutoff = older_than_days.map(|d| opts.now - i64::from(d) * DAY_SECS);
                for pattern in patterns {
                    let Some(resolved) =
                        paths::resolve(pattern, self.env).map_err(|e| e.to_string())?
                    else {
                        continue;
                    };
                    for dir in paths::expand(&resolved).map_err(|e| e.to_string())? {
                        let entries = match fs::read_dir(&dir) {
                            Ok(entries) => entries,
                            Err(e) => {
                                note_denied(&e);
                                continue;
                            }
                        };
                        for entry in entries.filter_map(Result::ok) {
                            let Ok(meta) = entry.metadata() else { continue };
                            if meta.file_type().is_symlink() {
                                continue;
                            }
                            let name = entry.file_name();
                            if !matchers.is_empty() && !matchers.iter().any(|m| m.is_match(&name)) {
                                continue;
                            }
                            if cutoff.is_some_and(|c| crate::platform::fs::mtime_secs(&meta) > c) {
                                continue;
                            }
                            out.push(Candidate {
                                rule,
                                path: entry.path(),
                                roots: vec![Root {
                                    path: dir.clone(),
                                    inclusive: false,
                                }],
                                specificity: dir.components().count(),
                                name: Some(name.to_string_lossy().into_owned()),
                                project: None,
                                min_bytes: *min_bytes,
                                exclude: Vec::new(),
                                large_file: false,
                            });
                        }
                    }
                }
            }
            // Recursive queries use the full scan's walk; project artifacts
            // and probes are resolved separately.
            Detect::Query {
                recursive: true, ..
            }
            | Detect::ProjectArtifact { .. }
            | Detect::Probe { .. } => {}
        }
        Ok(())
    }

    fn item(
        &self,
        c: &Candidate<'_>,
        m: &Measure,
        projects: &[Project],
        opts: &ScanOptions,
        guard: &SafetyGuard,
    ) -> Option<ScanItem> {
        if m.allocated == 0 || c.min_bytes.is_some_and(|min| m.allocated < min) {
            return None;
        }
        let rule = c.rule;
        let project = c.project.and_then(|i| projects.get(i));
        let inactive_secs = i64::from(opts.inactive_after_days) * DAY_SECS;
        // No activity date at all is treated as active: the careful choice.
        let project_active =
            project.map(|p| p.last_activity.is_none_or(|t| opts.now - t < inactive_secs));

        let mut risk = rule.risk;
        if project_active == Some(true) {
            risk = risk.max(Risk::Review);
        }

        let last_used = match rule.unused.basis {
            UnusedBasis::NewestMtime => m.newest_mtime,
            UnusedBasis::ProjectActivity => project.and_then(|p| p.last_activity),
            UnusedBasis::None => None,
        };

        let blocked = if m.contains_keep_marker {
            Some(Refusal::KeepMarker)
        } else if m.crosses_filesystem && !rule.cross_filesystems {
            Some(Refusal::CrossesFilesystem)
        } else if m.contains_git && !rule.regenerates {
            Some(Refusal::ContainsRepository)
        } else {
            guard.check_path(&c.path, &c.roots).err()
        }
        .map(|reason| Blocked::Safety { reason });

        let (method, blocked) = match (rule.cleanup.method, blocked) {
            (_, Some(b)) => (rule.cleanup.method, Some(b)),
            (Method::Command, None) => self.command_method(rule),
            (method, None) => (method, None),
        };
        let cleanable = blocked.is_none() && method != Method::None;

        let min_age = i64::from(rule.unused.min_age_days) * DAY_SECS;
        let old_enough = match rule.unused.basis {
            UnusedBasis::None => true,
            _ => last_used.is_some_and(|t| opts.now - t >= min_age),
        };
        let preselected =
            cleanable && risk == Risk::Safe && old_enough && project_active != Some(true);

        let key = c
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        Some(ScanItem {
            id: format!("{}:{}", rule.id, c.path.display()),
            rule_id: rule.id.clone(),
            path: Some(c.path.clone()),
            name: c.name.clone(),
            key,
            category: rule.category,
            group: rule.group.clone(),
            risk,
            bytes: m.allocated,
            bytes_known: true,
            files: m.files,
            last_used,
            project: project.map(|p| ProjectRef {
                root: p.root.clone(),
                name: p.name.clone(),
                active: project_active == Some(true),
            }),
            method,
            cleanable,
            blocked,
            preselected,
            may_share_blocks: rule.may_share_blocks,
            snapshot: Snapshot {
                allocated: m.allocated,
                own_mtime: m.own_mtime,
            },
            roots: c.roots.clone(),
            excluded: c.exclude.clone(),
            tool_paths: Vec::new(),
        })
    }

    /// A command cleanup uses the tool if it's installed, else the fallback.
    fn command_method(&self, rule: &Rule) -> (Method, Option<Blocked>) {
        let Some(cmd) = &rule.cleanup.command else {
            return (Method::None, None);
        };
        if self.runner.find_tool(self.env, &cmd.tool).is_some() {
            (Method::Command, None)
        } else if let Some(fallback) = rule.cleanup.fallback {
            (fallback, None)
        } else {
            (
                Method::Command,
                Some(Blocked::ToolMissing {
                    tool: cmd.tool.clone(),
                }),
            )
        }
    }

    fn probe_item(
        &self,
        rule: &Rule,
        p: crate::probes::ProbeItem,
        guard: &SafetyGuard,
    ) -> ScanItem {
        // A folder the probe found, cleaned by path through the guard.
        let by_path = match (&p.root, p.paths.as_slice()) {
            (Some(root), [path])
                if matches!(rule.cleanup.method, Method::Delete | Method::Trash) =>
            {
                Some((
                    path.clone(),
                    vec![Root {
                        path: root.clone(),
                        inclusive: false,
                    }],
                ))
            }
            _ => None,
        };
        let (method, blocked) = match &by_path {
            Some((path, roots)) => (
                rule.cleanup.method,
                guard
                    .check_path(path, roots)
                    .err()
                    .map(|reason| Blocked::Safety { reason }),
            ),
            None => self.command_method(rule),
        };
        let cleanable = blocked.is_none() && method != Method::None;
        let (path, roots) = by_path.map_or((None, Vec::new()), |(p, r)| (Some(p), r));
        ScanItem {
            id: format!("{}:{}", rule.id, p.key),
            rule_id: rule.id.clone(),
            path,
            name: p.name,
            key: p.key,
            category: rule.category,
            group: rule.group.clone(),
            risk: rule.risk,
            bytes: p.bytes.unwrap_or(0),
            bytes_known: p.bytes.is_some(),
            files: 0,
            last_used: None,
            project: None,
            method,
            cleanable,
            blocked,
            // Tool-managed items are never pre-selected: nothing to age them by.
            preselected: false,
            may_share_blocks: rule.may_share_blocks,
            snapshot: Snapshot {
                allocated: p.bytes.unwrap_or(0),
                own_mtime: p.own_mtime,
            },
            roots,
            excluded: Vec::new(),
            tool_paths: p.paths,
        }
    }
}

/// Spec §6.3: when rules claim the same path, the most specific (longest
/// literal root) wins; when one item sits inside another, the outer one
/// leaves it out of its size. Probe-owned paths beat everything.
fn settle_overlaps<'r>(
    candidates: Vec<Candidate<'r>>,
    probe_paths: &HashSet<PathBuf>,
) -> Vec<Candidate<'r>> {
    let mut by_path: HashMap<PathBuf, Candidate<'r>> = HashMap::new();
    for c in candidates {
        if probe_paths.contains(&c.path) {
            continue;
        }
        match by_path.get(&c.path) {
            Some(existing) if existing.large_file => {
                by_path.insert(c.path.clone(), c);
            }
            Some(existing) if c.large_file || existing.specificity >= c.specificity => {}
            _ => {
                by_path.insert(c.path.clone(), c);
            }
        }
    }

    // A large file inside anything another rule found belongs to that item.
    let claimed: HashSet<PathBuf> = by_path
        .values()
        .filter(|c| !c.large_file)
        .map(|c| c.path.clone())
        .chain(probe_paths.iter().cloned())
        .collect();
    by_path
        .retain(|path, c| !c.large_file || !path.ancestors().skip(1).any(|a| claimed.contains(a)));

    let sorted: BTreeMap<PathBuf, ()> = by_path
        .keys()
        .chain(probe_paths)
        .map(|p| (p.clone(), ()))
        .collect();
    let mut out: Vec<Candidate<'r>> = by_path.into_values().collect();
    for c in &mut out {
        c.exclude = sorted
            .range::<Path, _>((Bound::Excluded(c.path.as_path()), Bound::Unbounded))
            .map(|(p, ())| p)
            .take_while(|p| p.starts_with(&c.path))
            .cloned()
            .collect();
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}
