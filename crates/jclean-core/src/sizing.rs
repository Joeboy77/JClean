//! Measuring what a location occupies on disk (spec §4.2).
//!
//! - Allocated size, not logical length.
//! - Hard links count once per scan, via a shared [`InodeSet`].
//! - Symlinks are measured as links and never followed.
//! - Mount points are not crossed unless the rule opts in.
//! - Metadata only: no file is ever opened.
//! - Unreadable folders are counted, not treated as failures.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

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
    /// Remembered folder totals from the last scan (quick scans only). Never
    /// used by the SafetyGuard, which always measures fresh.
    pub cache: Option<Arc<SizeCache>>,
}

/// What one folder holds directly (its files, not its subfolders), as of
/// its modification time. Adding, removing or renaming anything in a folder
/// changes that time, so an unchanged time means the same entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirRecord {
    pub mtime: i64,
    pub allocated: u64,
    pub files: u64,
    pub newest: Option<i64>,
    pub git: bool,
    pub keep: bool,
    pub subdirs: Vec<String>,
}

/// Folder totals remembered between quick scans, so an unchanged folder
/// costs one `stat` instead of one per file. Sizes of files rewritten in
/// place can lag until the folder changes; cleaning always re-measures.
#[derive(Debug, Default)]
pub struct SizeCache {
    previous: HashMap<PathBuf, DirRecord>,
    current: Mutex<HashMap<PathBuf, DirRecord>>,
}

const CACHE_HEADER: &str = "jclean-size-cache 1";

impl SizeCache {
    pub fn new(previous: HashMap<PathBuf, DirRecord>) -> Self {
        Self {
            previous,
            current: Mutex::new(HashMap::new()),
        }
    }

    fn get(&self, dir: &Path) -> Option<&DirRecord> {
        self.previous.get(dir)
    }

    fn put(&self, dir: &Path, record: DirRecord) {
        if let Ok(mut current) = self.current.lock() {
            current.insert(dir.to_path_buf(), record);
        }
    }

    /// The folders seen in this scan, to save for the next one.
    pub fn fresh(&self) -> HashMap<PathBuf, DirRecord> {
        self.current.lock().map(|c| c.clone()).unwrap_or_default()
    }

    /// Reads a saved cache; anything unreadable just means starting fresh.
    pub fn load(path: &Path) -> Self {
        let Ok(text) = fs::read_to_string(path) else {
            return Self::default();
        };
        let mut lines = text.lines();
        if lines.next() != Some(CACHE_HEADER) {
            return Self::default();
        }
        let mut previous = HashMap::new();
        let mut last = String::new();
        for line in lines {
            let f: Vec<&str> = line.split('\t').collect();
            let [
                prefix,
                suffix,
                mtime,
                allocated,
                files,
                newest,
                flags,
                subdirs,
            ] = f.as_slice()
            else {
                return Self::default();
            };
            let Ok(prefix) = prefix.parse::<usize>() else {
                return Self::default();
            };
            let Some(head) = last.get(..prefix) else {
                return Self::default();
            };
            let path = format!("{head}{suffix}");
            let record = DirRecord {
                mtime: mtime.parse().unwrap_or(i64::MIN),
                allocated: allocated.parse().unwrap_or(0),
                files: files.parse().unwrap_or(0),
                newest: newest.parse().ok(),
                git: flags.contains('g'),
                keep: flags.contains('k'),
                subdirs: if subdirs.is_empty() {
                    Vec::new()
                } else {
                    subdirs.split('\u{1f}').map(str::to_string).collect()
                },
            };
            previous.insert(PathBuf::from(&path), record);
            last = path;
        }
        Self::new(previous)
    }

    /// Writes the cache with front-coded paths (sorted, each storing only
    /// what differs from the one before), which keeps it small.
    pub fn save(entries: &HashMap<PathBuf, DirRecord>, path: &Path) -> std::io::Result<()> {
        let mut rows: Vec<(&str, &DirRecord)> = entries
            .iter()
            .filter_map(|(p, r)| p.to_str().map(|s| (s, r)))
            .filter(|(p, r)| !has_separator(p) && !r.subdirs.iter().any(|n| has_separator(n)))
            .collect();
        rows.sort_by(|a, b| a.0.cmp(b.0));
        let mut out = String::with_capacity(rows.len() * 48);
        out.push_str(CACHE_HEADER);
        out.push('\n');
        let mut last = "";
        for (p, r) in rows {
            let prefix = common_prefix(last, p);
            let newest = r.newest.map(|n| n.to_string()).unwrap_or_default();
            let flags = format!(
                "{}{}",
                if r.git { "g" } else { "" },
                if r.keep { "k" } else { "" }
            );
            out.push_str(&format!(
                "{prefix}\t{}\t{}\t{}\t{}\t{newest}\t{flags}\t{}\n",
                &p[prefix..],
                r.mtime,
                r.allocated,
                r.files,
                r.subdirs.join("\u{1f}")
            ));
            last = p;
        }
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, out)?;
        fs::rename(tmp, path)
    }
}

fn has_separator(s: &str) -> bool {
    s.contains(['\t', '\n', '\r', '\u{1f}'])
}

/// Length of the shared prefix, on a character boundary.
fn common_prefix(a: &str, b: &str) -> usize {
    a.char_indices()
        .zip(b.chars())
        .take_while(|((_, x), y)| x == y)
        .last()
        .map_or(0, |((i, c), _)| i + c.len_utf8())
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
        if hard_link_id(path, &meta).is_none_or(|id| inodes.first_sighting(id)) {
            m.allocated = allocated_bytes(path, &meta);
        }
        m.saw_mtime(own_mtime);
        return Ok(m);
    }

    let ctx = Walk {
        root_dev: device(&meta),
        opts,
        inodes,
        cancel,
        excluded_parents: opts
            .exclude
            .iter()
            .filter_map(|e| e.parent().map(Path::to_path_buf))
            .collect(),
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
    /// Folders with an excluded path directly inside: never cached, since
    /// their totals depend on what's excluded.
    excluded_parents: HashSet<PathBuf>,
}

impl Walk<'_> {
    fn dir(&self, dir: &Path, meta: &fs::Metadata) -> Measure {
        let mut m = Measure {
            allocated: allocated_bytes(dir, meta),
            dirs: 1,
            ..Measure::default()
        };
        m.saw_mtime(mtime_secs(meta));
        if self.cancel.is_cancelled() {
            return m;
        }

        let mtime = mtime_secs(meta);
        let cacheable = self.opts.cache.is_some() && !self.excluded_parents.contains(dir);
        let mut subdirs = Vec::new();

        // Unchanged since last time: reuse its own totals, still walk its subfolders.
        let cached = cacheable
            .then(|| self.opts.cache.as_ref().and_then(|c| c.get(dir)))
            .flatten()
            .filter(|r| r.mtime == mtime)
            .cloned();
        if let (Some(record), Some(cache)) = (cached, &self.opts.cache) {
            m.allocated += record.allocated;
            m.files += record.files;
            if let Some(t) = record.newest {
                m.saw_mtime(t);
            }
            m.contains_git |= record.git;
            m.contains_keep_marker |= record.keep;
            for name in &record.subdirs {
                let path = dir.join(name);
                let Ok(meta) = fs::symlink_metadata(&path) else {
                    continue;
                };
                if !meta.is_dir() {
                    continue;
                }
                if device(&meta) != self.root_dev && !self.opts.cross_filesystems {
                    m.crosses_filesystem = true;
                    continue;
                }
                subdirs.push((path, meta));
            }
            cache.put(dir, record);
        } else {
            let entries = match fs::read_dir(dir) {
                Ok(entries) => entries,
                Err(_) => {
                    m.unreadable_dirs += 1;
                    return m;
                }
            };
            let mut own = DirRecord {
                mtime,
                allocated: 0,
                files: 0,
                newest: None,
                git: false,
                keep: false,
                subdirs: Vec::new(),
            };
            let mut shared_files = false;
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
                        own.git = true;
                    }
                    if let Some(n) = name.to_str() {
                        own.subdirs.push(n.to_string());
                    } else {
                        shared_files = true;
                    }
                    if device(&meta) != self.root_dev && !self.opts.cross_filesystems {
                        m.crosses_filesystem = true;
                        continue;
                    }
                    subdirs.push((path, meta));
                } else {
                    if name == KEEP_MARKER {
                        m.contains_keep_marker = true;
                        own.keep = true;
                    }
                    let t = mtime_secs(&meta);
                    m.files += 1;
                    m.saw_mtime(t);
                    own.files += 1;
                    own.newest = Some(own.newest.map_or(t, |n| n.max(t)));
                    let link = hard_link_id(&path, &meta);
                    // Hard-linked files must be counted once per scan, so
                    // their folder is always listed fresh.
                    shared_files |= link.is_some();
                    if link.is_none_or(|id| self.inodes.first_sighting(id)) {
                        let size = allocated_bytes(&path, &meta);
                        m.allocated += size;
                        own.allocated += size;
                    }
                }
            }
            if cacheable
                && !shared_files
                && m.unreadable_dirs == 0
                && let Some(cache) = &self.opts.cache
            {
                cache.put(dir, own);
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
