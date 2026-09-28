//! Measuring what a location occupies on disk (spec §4.2).
//!
//! - Allocated size, not logical length.
//! - Hard links count once per scan, via a shared [`InodeSet`].
//! - Symlinks are measured as links and never followed.
//! - Mount points are not crossed unless the rule opts in.
//! - Metadata only: no file is ever opened.
//! - Unreadable folders are counted, not treated as failures.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rayon::prelude::*;
use serde::Serialize;

use crate::cancel::CancelToken;
use crate::platform::fs::{allocated_bytes, device, hard_link_id, mtime_secs};

/// Name of the marker file that protects a folder from cleaning (spec §7.2).
pub const KEEP_MARKER: &str = ".jclean-keep";

/// `(device, inode)` pairs already counted in this scan.
#[derive(Debug, Default)]
pub struct InodeSet(Mutex<HashSet<(u64, u64)>>);

impl InodeSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// True the first time an ID is seen.
    pub(crate) fn first_sighting(&self, id: (u64, u64)) -> bool {
        match self.0.lock() {
            Ok(mut set) => set.insert(id),
            // A poisoned lock means another thread panicked mid-insert;
            // counting the file is the conservative choice.
            Err(poisoned) => poisoned.into_inner().insert(id),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MeasureOptions {
    pub cross_filesystems: bool,
    /// Paths inside the target claimed by more specific rules; skipped.
    pub exclude: HashSet<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Measure {
    pub allocated: u64,
    pub files: u64,
    pub dirs: u64,
    /// Newest modification time of anything inside, as Unix seconds.
    pub newest_mtime: Option<i64>,
    /// The target's own modification time.
    pub own_mtime: i64,
    pub is_symlink: bool,
    pub is_dir: bool,
    pub unreadable_dirs: u64,
    /// A mount point was found inside and skipped.
    pub crosses_filesystem: bool,
    /// A `.git` folder exists somewhere inside.
    pub contains_git: bool,
    /// A `.jclean-keep` file exists somewhere inside.
    pub contains_keep_marker: bool,
}

impl Measure {
    fn merge(&mut self, other: &Self) {
        self.allocated += other.allocated;
        self.files += other.files;
        self.dirs += other.dirs;
        self.newest_mtime = self.newest_mtime.max(other.newest_mtime);
        self.unreadable_dirs += other.unreadable_dirs;
        self.crosses_filesystem |= other.crosses_filesystem;
        self.contains_git |= other.contains_git;
        self.contains_keep_marker |= other.contains_keep_marker;
    }

    fn saw_mtime(&mut self, t: i64) {
        self.newest_mtime = Some(self.newest_mtime.map_or(t, |n| n.max(t)));
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SizeError {
    #[error("{0} doesn't exist")]
    NotFound(PathBuf),
    #[error("couldn't read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cancelled")]
    Cancelled,
}

/// Measures a file, symlink or folder tree.
pub fn measure(
    path: &Path,
    opts: &MeasureOptions,
    inodes: &InodeSet,
    cancel: &CancelToken,
) -> Result<Measure, SizeError> {
    let meta = fs::symlink_metadata(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            SizeError::NotFound(path.to_path_buf())
        } else {
            SizeError::Io {
                path: path.to_path_buf(),
                source: e,
            }
        }
    })?;

    let own_mtime = mtime_secs(&meta);
    let ft = meta.file_type();
    if !ft.is_dir() {
        let mut m = Measure {
            own_mtime,
            is_symlink: ft.is_symlink(),
            files: 1,
            ..Measure::default()
        };
        if hard_link_id(&meta).is_none_or(|id| inodes.first_sighting(id)) {
            m.allocated = allocated_bytes(&meta);
        }
        m.saw_mtime(own_mtime);
        return Ok(m);
    }

    let ctx = Walk {
        root_dev: device(&meta),
        opts,
        inodes,
        cancel,
    };
    let mut m = ctx.dir(path, &meta);
    if cancel.is_cancelled() {
        return Err(SizeError::Cancelled);
    }
    m.own_mtime = own_mtime;
    m.is_dir = true;
    Ok(m)
}

struct Walk<'a> {
    root_dev: u64,
    opts: &'a MeasureOptions,
    inodes: &'a InodeSet,
    cancel: &'a CancelToken,
}

impl Walk<'_> {
    fn dir(&self, dir: &Path, meta: &fs::Metadata) -> Measure {
        let mut m = Measure {
            allocated: allocated_bytes(meta),
            dirs: 1,
            ..Measure::default()
        };
        m.saw_mtime(mtime_secs(meta));
        if self.cancel.is_cancelled() {
            return m;
        }

        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(_) => {
                m.unreadable_dirs += 1;
                return m;
            }
        };

        let mut subdirs = Vec::new();
        for entry in entries {
            let Ok(entry) = entry else {
                m.unreadable_dirs += 1;
                continue;
            };
            let path = entry.path();
            if self.opts.exclude.contains(&path) {
                continue;
            }
            // DirEntry::metadata doesn't follow symlinks.
            let Ok(meta) = entry.metadata() else {
                m.unreadable_dirs += 1;
                continue;
            };
            let name = entry.file_name();
            if meta.is_dir() {
                if name == ".git" {
                    m.contains_git = true;
                }
                if device(&meta) != self.root_dev && !self.opts.cross_filesystems {
                    m.crosses_filesystem = true;
                    continue;
                }
                subdirs.push((path, meta));
            } else {
                if name == KEEP_MARKER {
                    m.contains_keep_marker = true;
                }
                m.files += 1;
                m.saw_mtime(mtime_secs(&meta));
                if hard_link_id(&meta).is_none_or(|id| self.inodes.first_sighting(id)) {
                    m.allocated += allocated_bytes(&meta);
                }
            }
        }

        let children: Vec<Measure> = subdirs
            .par_iter()
            .map(|(p, meta)| self.dir(p, meta))
            .collect();
        for child in &children {
            m.merge(child);
        }
        m
    }
}
