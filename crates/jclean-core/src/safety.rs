//! The SafetyGuard (spec §7.1). Every path is re-checked immediately before
//! it's cleaned, not just at scan time:
//!
//! 1. Canonicalize (resolving symlinks in parent folders only) and confirm the
//!    path sits inside a root declared by the matching rule.
//! 2. Refuse protected paths, their ancestors, `.git` folders and anything
//!    marked with `.jclean-keep`.
//! 3. A symlink is removed as a link and never followed.
//! 4. Re-measure: refuse if the size moved more than 10% or the item was
//!    modified after the scan.
//! 5. Refuse while a related app is running.
//!
//! There is no way to switch the guard off.

use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::cancel::CancelToken;
use crate::env::Env;
use crate::platform::fs::mtime_secs;
use crate::platform::{self, ProtectedPath, Scope};
use crate::sizing::{InodeSet, KEEP_MARKER, MeasureOptions, measure};

/// What the scan saw, compared against the disk right before cleaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub allocated: u64,
    pub own_mtime: i64,
}

/// A folder a rule is allowed to clean inside.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Root {
    pub path: PathBuf,
    /// Whether the root itself may be cleaned (a rule naming an exact path),
    /// or only things inside it.
    pub inclusive: bool,
}

pub struct Target<'a> {
    pub path: &'a Path,
    pub roots: &'a [Root],
    pub snapshot: Snapshot,
    pub regenerates: bool,
    pub cross_filesystems: bool,
    /// Paths inside the target that belong to other items and were left out
    /// of its size.
    pub exclude: &'a [PathBuf],
    pub related_apps: &'a [String],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    /// Canonical path to act on.
    pub path: PathBuf,
    pub is_symlink: bool,
    pub is_dir: bool,
    pub allocated: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Refusal {
    #[error("The path isn't valid: {0}")]
    Invalid(String),
    #[error("It's already gone")]
    NotFound,
    #[error("It's outside the folders this rule may clean")]
    OutsideRuleRoot,
    #[error("It's a protected location")]
    Protected,
    #[error("It's marked to keep with a .jclean-keep file")]
    KeepMarker,
    #[error("It contains a Git repository")]
    ContainsRepository,
    #[error("It contains another drive")]
    CrossesFilesystem,
    #[error("Changed since scan")]
    ChangedSinceScan,
    #[error("Close {} first", .0.join(", "))]
    AppRunning(Vec<String>),
}

/// Which processes are running. Tests substitute a fake.
pub trait ProcessChecker: Send + Sync {
    /// Returns the names from `names` that are running.
    fn running(&self, names: &[String]) -> Vec<String>;
}

/// Reads the real process list.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemProcesses;

impl ProcessChecker for SystemProcesses {
    fn running(&self, names: &[String]) -> Vec<String> {
        if names.is_empty() {
            return Vec::new();
        }
        let mut sys = sysinfo::System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let running: HashSet<String> = sys
            .processes()
            .values()
            .map(|p| p.name().to_string_lossy().to_lowercase())
            .collect();
        names
            .iter()
            .filter(|n| running.contains(&n.to_lowercase()))
            .cloned()
            .collect()
    }
}

/// Reports nothing running.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoProcesses;

impl ProcessChecker for NoProcesses {
    fn running(&self, _names: &[String]) -> Vec<String> {
        Vec::new()
    }
}

pub struct SafetyGuard {
    protected: Vec<ProtectedPath>,
}

impl SafetyGuard {
    pub fn new(env: &Env) -> Self {
        Self::with_protected(platform::protected_paths(env))
    }

    pub fn with_protected(paths: Vec<ProtectedPath>) -> Self {
        // Keep both spellings: `/etc` and `/private/etc` are the same folder on macOS.
        let mut protected = Vec::with_capacity(paths.len() * 2);
        for p in paths {
            if let Ok(canonical) = fs::canonicalize(&p.path)
                && canonical != p.path
            {
                protected.push(ProtectedPath {
                    path: canonical,
                    scope: p.scope,
                });
            }
            protected.push(p);
        }
        Self { protected }
    }

    pub fn protected(&self) -> &[ProtectedPath] {
        &self.protected
    }

    pub fn check(
        &self,
        target: &Target<'_>,
        processes: &dyn ProcessChecker,
        cancel: &CancelToken,
    ) -> Result<Verified, Refusal> {
        let canonical = self.check_path(target.path, target.roots)?;

        let meta = fs::symlink_metadata(&canonical).map_err(|_| Refusal::NotFound)?;
        let mut verified = Verified {
            path: canonical.clone(),
            is_symlink: meta.file_type().is_symlink(),
            is_dir: meta.is_dir(),
            allocated: 0,
        };

        if !verified.is_symlink {
            let exclude: HashSet<PathBuf> = target
                .exclude
                .iter()
                .filter_map(|e| e.strip_prefix(target.path).ok())
                .map(|rel| canonical.join(rel))
                .collect();
            let opts = MeasureOptions {
                cross_filesystems: target.cross_filesystems,
                exclude,
                // Always measure fresh right before cleaning: never the cache.
                cache: None,
            };
            let now = measure(&canonical, &opts, &InodeSet::new(), cancel)
                .map_err(|_| Refusal::NotFound)?;
            if now.contains_keep_marker {
                return Err(Refusal::KeepMarker);
            }
            if now.crosses_filesystem {
                return Err(Refusal::CrossesFilesystem);
            }
            if now.contains_git && !target.regenerates {
                return Err(Refusal::ContainsRepository);
            }
            if changed(target.snapshot.allocated, now.allocated)
                || mtime_secs(&meta) > target.snapshot.own_mtime
            {
                return Err(Refusal::ChangedSinceScan);
            }
            verified.allocated = now.allocated;
        }

        let running = processes.running(target.related_apps);
        if !running.is_empty() {
            return Err(Refusal::AppRunning(running));
        }
        Ok(verified)
    }

    /// Checks 1 and 2: returns the canonical path if it's inside a rule root
    /// and not protected.
    pub fn check_path(&self, path: &Path, roots: &[Root]) -> Result<PathBuf, Refusal> {
        if !path.is_absolute() {
            return Err(Refusal::Invalid("not an absolute path".to_string()));
        }
        if path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        {
            return Err(Refusal::Invalid("contains '.' or '..'".to_string()));
        }
        let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
            return Err(Refusal::Protected);
        };
        let parent = fs::canonicalize(parent).map_err(|_| Refusal::NotFound)?;
        let canonical = parent.join(name);

        let inside = roots.iter().any(|root| {
            let Ok(root_path) = fs::canonicalize(&root.path) else {
                return false;
            };
            canonical.starts_with(&root_path) && (canonical != root_path || root.inclusive)
        });
        if !inside {
            return Err(Refusal::OutsideRuleRoot);
        }

        if protection_for(&canonical, &self.protected).is_some() {
            return Err(Refusal::Protected);
        }
        if canonical.components().any(|c| c.as_os_str() == ".git") {
            return Err(Refusal::Protected);
        }
        if canonical
            .ancestors()
            .skip(1)
            .any(|dir| fs::symlink_metadata(dir.join(KEEP_MARKER)).is_ok())
        {
            return Err(Refusal::KeepMarker);
        }
        Ok(canonical)
    }
}

/// The protected entry that forbids `path`, if any: the path itself, any of
/// its descendants, or (for subtree entries) any of its ancestors.
pub fn protection_for<'a>(
    path: &Path,
    protected: &'a [ProtectedPath],
) -> Option<&'a ProtectedPath> {
    protected.iter().find(|p| {
        p.path.starts_with(path) || (p.scope == Scope::Subtree && path.starts_with(&p.path))
    })
}

/// More than a 10% difference from the scan (spec §7.1, 4).
fn changed(before: u64, now: u64) -> bool {
    u128::from(before.abs_diff(now)) * 10 > u128::from(before)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_percent_threshold() {
        assert!(!changed(1000, 1000));
        assert!(!changed(1000, 1100));
        assert!(changed(1000, 1101));
        assert!(changed(1000, 899));
        assert!(changed(0, 1));
        assert!(!changed(0, 0));
    }

    #[test]
    fn protection_covers_ancestors_and_subtrees() {
        let protected = vec![
            ProtectedPath {
                path: PathBuf::from("/u/me/Documents"),
                scope: Scope::Exact,
            },
            ProtectedPath {
                path: PathBuf::from("/u/me/.ssh"),
                scope: Scope::Subtree,
            },
        ];
        assert!(protection_for(Path::new("/u/me/Documents"), &protected).is_some());
        assert!(protection_for(Path::new("/u/me"), &protected).is_some());
        assert!(protection_for(Path::new("/u/me/Documents/old.zip"), &protected).is_none());
        assert!(protection_for(Path::new("/u/me/.ssh/id_ed25519"), &protected).is_some());
        assert!(protection_for(Path::new("/u/me/.sshx"), &protected).is_none());
    }
}
