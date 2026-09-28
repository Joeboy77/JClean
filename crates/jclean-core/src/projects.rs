//! Finding projects and their build/dependency folders (spec §4.5), and
//! when each project was last worked on (spec §4.4).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use globset::{GlobSet, GlobSetBuilder};
use rayon::prelude::*;
use serde::Serialize;

use crate::cancel::CancelToken;
use crate::platform::fs::{device, mtime_secs};
use crate::rules::{Detect, Rule};

/// One project-artifact rule, compiled.
pub struct ArtifactRule<'a> {
    pub rule: &'a Rule,
    markers: GlobSet,
    /// Folder names (globs) directly inside the project.
    folders: GlobSet,
    /// Nested folder paths such as `.angular/cache`.
    nested: Vec<String>,
}

impl<'a> ArtifactRule<'a> {
    pub fn compile(rule: &'a Rule) -> Option<Self> {
        let Detect::ProjectArtifact { markers, folders } = &rule.detect else {
            return None;
        };
        let set = |globs: &mut dyn Iterator<Item = &String>| {
            let mut b = GlobSetBuilder::new();
            for g in globs {
                if let Ok(glob) = globset::Glob::new(g) {
                    b.add(glob);
                }
            }
            b.build().ok()
        };
        Some(Self {
            rule,
            markers: set(&mut markers.iter())?,
            folders: set(&mut folders.iter().filter(|f| !f.contains('/')))?,
            nested: folders
                .iter()
                .filter(|f| f.contains('/'))
                .cloned()
                .collect(),
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub root: PathBuf,
    pub name: String,
    /// Newest of the last commit and the newest source edit, Unix seconds.
    pub last_activity: Option<i64>,
    pub artifacts: Vec<Artifact>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub rule_id: String,
    pub path: PathBuf,
}

pub struct DiscoverOptions<'a> {
    pub roots: &'a [PathBuf],
    pub exclude: &'a [PathBuf],
    /// Folder depth below each root; `None` walks everything (full scan).
    pub max_depth: Option<usize>,
}

/// Folders never descended into while looking for projects: dependency
/// trees and VCS data are huge and never contain projects of interest.
const NEVER_DESCEND: &[&str] = &["node_modules", ".git", "Pods", "DerivedData"];

pub fn discover(
    rules: &[ArtifactRule<'_>],
    opts: &DiscoverOptions<'_>,
    cancel: &CancelToken,
) -> Vec<Project> {
    if rules.is_empty() {
        return Vec::new();
    }
    let exclude: HashSet<&Path> = opts.exclude.iter().map(PathBuf::as_path).collect();
    let mut projects: Vec<Project> = opts
        .roots
        .par_iter()
        .flat_map_iter(|root| {
            let Ok(meta) = fs::symlink_metadata(root) else {
                return Vec::new();
            };
            if !meta.is_dir() {
                return Vec::new();
            }
            let walker = Finder {
                rules,
                exclude: &exclude,
                max_depth: opts.max_depth,
                dev: device(&meta),
                scan_root: root,
                cancel,
            };
            walker.visit(root, 0)
        })
        .collect();
    projects.sort_by(|a, b| a.root.cmp(&b.root));
    projects
}

struct Finder<'a> {
    rules: &'a [ArtifactRule<'a>],
    exclude: &'a HashSet<&'a Path>,
    max_depth: Option<usize>,
    dev: u64,
    scan_root: &'a Path,
    cancel: &'a CancelToken,
}

impl Finder<'_> {
    fn visit(&self, dir: &Path, depth: usize) -> Vec<Project> {
        if self.cancel.is_cancelled() {
            return Vec::new();
        }
        let Ok(entries) = fs::read_dir(dir) else {
            return Vec::new();
        };

        // Names and types come from the directory listing: no extra stat calls.
        let mut files = Vec::new();
        let mut dirs = Vec::new();
        for entry in entries.filter_map(Result::ok) {
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if ft.is_dir() {
                dirs.push(name);
            } else {
                files.push(name);
            }
        }

        let mut artifacts = Vec::new();
        let mut artifact_names: HashSet<String> = HashSet::new();
        for rule in self.rules {
            // Markers can be files (`package.json`) or folders (`App.xcodeproj`).
            let has_marker = files.iter().chain(&dirs).any(|n| rule.markers.is_match(n));
            if !has_marker {
                continue;
            }
            for name in &dirs {
                if rule.folders.is_match(name) {
                    artifact_names.insert(name.clone());
                    artifacts.push(Artifact {
                        rule_id: rule.rule.id.clone(),
                        path: dir.join(name),
                    });
                }
            }
            for nested in &rule.nested {
                let path = dir.join(nested);
                if fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir()) {
                    if let Some(first) = nested.split('/').next() {
                        artifact_names.insert(first.to_string());
                    }
                    artifacts.push(Artifact {
                        rule_id: rule.rule.id.clone(),
                        path,
                    });
                }
            }
        }

        let mut found = Vec::new();
        if !artifacts.is_empty() {
            artifacts.sort_by(|a, b| a.path.cmp(&b.path));
            artifacts.dedup_by(|a, b| a.path == b.path);
            found.push(Project {
                root: dir.to_path_buf(),
                name: dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                last_activity: last_activity(dir, &artifact_names, self.scan_root),
                artifacts,
            });
        }

        if self.max_depth.is_some_and(|max| depth >= max) {
            return found;
        }
        let children: Vec<PathBuf> = dirs
            .iter()
            .filter(|n| {
                !n.starts_with('.')
                    && !NEVER_DESCEND.contains(&n.as_str())
                    && !artifact_names.contains(*n)
            })
            .map(|n| dir.join(n))
            .filter(|p| !self.exclude.contains(p.as_path()))
            .filter(|p| fs::symlink_metadata(p).is_ok_and(|m| device(&m) == self.dev))
            .collect();
        found.extend(
            children
                .par_iter()
                .flat_map_iter(|c| self.visit(c, depth + 1))
                .collect::<Vec<_>>(),
        );
        found
    }
}

/// Spec §4.4: the newer of the last git activity (mtime of `.git/logs/HEAD`)
/// and the newest source file in the project's top two levels, ignoring
/// build folders, hidden folders and `.git`.
pub fn last_activity(
    project: &Path,
    artifact_names: &HashSet<String>,
    scan_root: &Path,
) -> Option<i64> {
    let mut newest: Option<i64> = None;
    let mut see = |t: i64| newest = Some(newest.map_or(t, |n| n.max(t)));

    // Monorepo packages share the repository at an ancestor.
    let mut dir = Some(project);
    while let Some(d) = dir {
        if let Ok(meta) = fs::symlink_metadata(d.join(".git/logs/HEAD")) {
            see(mtime_secs(&meta));
            break;
        }
        if d == scan_root {
            break;
        }
        dir = d.parent();
    }

    let skip = |name: &str| {
        name.starts_with('.') || artifact_names.contains(name) || NEVER_DESCEND.contains(&name)
    };
    let Ok(entries) = fs::read_dir(project) else {
        return newest;
    };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_file() && !name.starts_with('.') {
            see(mtime_secs(&meta));
        } else if meta.is_dir() && !skip(&name) {
            let Ok(inner) = fs::read_dir(entry.path()) else {
                continue;
            };
            for child in inner.filter_map(Result::ok) {
                if let Ok(m) = child.metadata()
                    && m.is_file()
                {
                    see(mtime_secs(&m));
                }
            }
        }
    }
    newest
}
