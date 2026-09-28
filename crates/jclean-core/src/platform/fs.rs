//! Filesystem metadata that differs between platforms. Metadata only: nothing
//! here opens a file (spec §4.2).

use std::fs::Metadata;

/// Space the file actually occupies on disk (spec §4.2), not its logical length.
#[cfg(unix)]
pub fn allocated_bytes(meta: &Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.blocks().saturating_mul(512)
}

/// `(device, inode)` for files with more than one hard link, so they're
/// counted once. `None` when the file has a single link.
#[cfg(unix)]
pub fn hard_link_id(meta: &Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    (meta.nlink() > 1 && !meta.is_dir()).then(|| (meta.dev(), meta.ino()))
}

#[cfg(unix)]
pub fn device(meta: &Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.dev()
}

// Windows sizing (GetCompressedFileSizeW) and file IDs arrive in phase 8.
#[cfg(not(unix))]
pub fn allocated_bytes(meta: &Metadata) -> u64 {
    meta.len()
}

#[cfg(not(unix))]
pub fn hard_link_id(_meta: &Metadata) -> Option<(u64, u64)> {
    None
}

#[cfg(not(unix))]
pub fn device(_meta: &Metadata) -> u64 {
    0
}

/// Modification time as Unix seconds (0 if unavailable).
pub fn mtime_secs(meta: &Metadata) -> i64 {
    meta.modified().map(crate::time::unix_secs).unwrap_or(0)
}
