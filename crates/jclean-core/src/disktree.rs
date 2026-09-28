//! The full scan's map of the disk (spec §4.1, §15): totals per folder, plus
//! individual files above a size threshold. Also collects large-file
//! candidates on the way, so the home folder is walked once.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::cancel::CancelToken;
use crate::platform::fs::{allocated_bytes, device, hard_link_id, mtime_secs};
use crate::sizing::{InodeSet, MeasureOptions, measure};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    pub name: String,
    pub allocated: u64,
    pub is_dir: bool,
    /// Folders, and files at or above the threshold. Largest first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<TreeNode>,
    /// Space used by files below the threshold, not listed individually.
    pub small_files: u64,
    /// Contents weren't walked (cloud folders); only the total is known.
    pub summarized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LargeFile {
    pub path: PathBuf,
    pub allocated: u64,
    pub mtime: i64,
}

pub struct TreeOptions<'a> {
    /// Files at or above this size get their own node (1 MB by default).
    pub file_threshold: u64,
    /// Folders measured as a total but never listed (cloud folders).
    pub summarize: &'a [PathBuf],
    /// Large-file search: minimum size, and folders to leave out.
    pub large_min_bytes: Option<u64>,
    pub large_exclude: &'a [PathBuf],
}

pub const DEFAULT_FILE_THRESHOLD: u64 = 1_000_000;

/// Package folders that macOS shows as one file. Large files inside them are
/// never offered individually: removing part of a library breaks it.
const PACKAGE_EXTENSIONS: &[&str] = &[
    "app",
    "photoslibrary",
    "photolibrary",
    "musiclibrary",
    "tvlibrary",
    "imovielibrary",
    "fcpbundle",
    "logicx",
    "band",
    "aplibrary",
    "xcarchive",
    "bundle",
    "framework",
    "vmwarevm",
    "pvm",
    "utm",
    "sparsebundle",
];

pub fn build(
    root: &Path,
    opts: &TreeOptions<'_>,
    inodes: &InodeSet,
    cancel: &CancelToken,
) -> Option<(TreeNode, Vec<LargeFile>, u64)> {
    let meta = fs::symlink_metadata(root).ok()?;
    if !meta.is_dir() {
        return None;
    }
    let walk = Walk {
        dev: device(&meta),
        opts,
        summarize: opts.summarize.iter().map(PathBuf::as_path).collect(),
        large_exclude: opts.large_exclude.iter().map(PathBuf::as_path).collect(),
        inodes,
        cancel,
        large: Mutex::new(Vec::new()),
        unreadable: Mutex::new(0),
    };
    let mut node = walk.dir(root, &meta, opts.large_min_bytes.is_some());
    node.name = root.display().to_string();
    let mut large = walk.large.into_inner().unwrap_or_default();
    large.sort_by_key(|f| std::cmp::Reverse(f.allocated));
    let unreadable = walk.unreadable.into_inner().unwrap_or_default();
    Some((node, large, unreadable))
}

struct Walk<'a> {
    dev: u64,
    opts: &'a TreeOptions<'a>,
    summarize: HashSet<&'a Path>,
    large_exclude: HashSet<&'a Path>,
    inodes: &'a InodeSet,
    cancel: &'a CancelToken,
    large: Mutex<Vec<LargeFile>>,
    unreadable: Mutex<u64>,
}

impl Walk<'_> {
    fn dir(&self, dir: &Path, meta: &fs::Metadata, collect_large: bool) -> TreeNode {
        let mut node = TreeNode {
            name: dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            allocated: allocated_bytes(meta),
            is_dir: true,
            children: Vec::new(),
            small_files: 0,
            summarized: false,
        };
        if self.cancel.is_cancelled() {
            return node;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            if let Ok(mut n) = self.unreadable.lock() {
                *n += 1;
            }
            return node;
        };

        let mut subdirs = Vec::new();
        for entry in entries.filter_map(Result::ok) {
            let Ok(meta) = entry.metadata() else { continue };
            let path = entry.path();
            if meta.is_dir() {
                if device(&meta) != self.dev {
                    continue;
                }
                subdirs.push((path, meta));
                continue;
            }
            let size = if hard_link_id(&meta).is_none_or(|id| self.inodes.first_sighting(id)) {
                allocated_bytes(&meta)
            } else {
                0
            };
            node.allocated += size;
            if size >= self.opts.file_threshold {
                node.children.push(TreeNode {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    allocated: size,
                    is_dir: false,
                    children: Vec::new(),
                    small_files: 0,
                    summarized: false,
                });
            } else {
                node.small_files += size;
            }
            if collect_large
                && meta.is_file()
                && self.opts.large_min_bytes.is_some_and(|min| size >= min)
                && let Ok(mut large) = self.large.lock()
            {
                large.push(LargeFile {
                    path,
                    allocated: size,
                    mtime: mtime_secs(&meta),
                });
            }
        }

        let children: Vec<TreeNode> = subdirs
            .par_iter()
            .map(|(path, meta)| {
                if self.summarize.contains(path.as_path()) {
                    return self.summary(path);
                }
                let large_here = collect_large
                    && !self.large_exclude.contains(path.as_path())
                    && !is_package(path);
                self.dir(path, meta, large_here)
            })
            .collect();
        for child in children {
            node.allocated += child.allocated;
            node.children.push(child);
        }
        node.children
            .sort_by_key(|c| std::cmp::Reverse(c.allocated));
        node
    }

    /// Total size only: stat calls, no listing kept (spec §4.2 cloud folders).
    fn summary(&self, path: &Path) -> TreeNode {
        let allocated = measure(path, &MeasureOptions::default(), self.inodes, self.cancel)
            .map(|m| m.allocated)
            .unwrap_or(0);
        TreeNode {
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            allocated,
            is_dir: true,
            children: Vec::new(),
            small_files: 0,
            summarized: true,
        }
    }
}

fn is_package(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| PACKAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}
