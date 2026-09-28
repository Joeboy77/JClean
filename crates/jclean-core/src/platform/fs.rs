//! Filesystem metadata that differs between platforms. Metadata only: nothing
//! here reads a file's contents (spec §4.2).

use std::fs::Metadata;
#[cfg(not(windows))]
use std::path::Path;

/// Space the file actually occupies on disk (spec §4.2), not its logical length.
#[cfg(unix)]
pub fn allocated_bytes(_path: &Path, meta: &Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.blocks().saturating_mul(512)
}

/// `(device, inode)` for files with more than one hard link, so they're
/// counted once. `None` when the file has a single link.
#[cfg(unix)]
pub fn hard_link_id(_path: &Path, meta: &Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    (meta.nlink() > 1 && !meta.is_dir()).then(|| (meta.dev(), meta.ino()))
}

/// The filesystem a folder is on, to stay on one (spec §4.2).
#[cfg(unix)]
pub fn device(meta: &Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    meta.dev()
}

#[cfg(windows)]
pub use windows::{allocated_bytes, device, hard_link_id};

#[cfg(not(any(unix, windows)))]
pub fn allocated_bytes(_path: &Path, meta: &Metadata) -> u64 {
    meta.len()
}

#[cfg(not(any(unix, windows)))]
pub fn hard_link_id(_path: &Path, _meta: &Metadata) -> Option<(u64, u64)> {
    None
}

#[cfg(not(any(unix, windows)))]
pub fn device(_meta: &Metadata) -> u64 {
    0
}

/// The real path of `path`, resolving `..` and symlinks in it. On Windows the
/// result is plain `C:\…` rather than `\\?\C:\…`, the form rules, the
/// environment and protected paths use, so the SafetyGuard compares like
/// with like. (The standard library adds the long-path prefix itself
/// whenever a path needs it.)
pub fn canonicalize(path: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    std::fs::canonicalize(path).map(plain)
}

/// Drops the `\\?\` prefix from a Windows drive path; anything else is
/// returned as it is.
pub fn plain(path: std::path::PathBuf) -> std::path::PathBuf {
    if cfg!(windows) {
        let text = path.to_string_lossy();
        if let Some(rest) = text.strip_prefix(r"\\?\")
            && rest.as_bytes().get(1) == Some(&b':')
        {
            return std::path::PathBuf::from(rest.to_string());
        }
    }
    path
}

/// Modification time as Unix seconds (0 if unavailable).
pub fn mtime_secs(meta: &Metadata) -> i64 {
    meta.modified().map(crate::time::unix_secs).unwrap_or(0)
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod windows {
    use std::fs::Metadata;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::MetadataExt;
    use std::path::Path;

    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, INVALID_HANDLE_VALUE, NO_ERROR,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, CreateFileW, FILE_ATTRIBUTE_COMPRESSED, FILE_ATTRIBUTE_OFFLINE,
        FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, FILE_ATTRIBUTE_RECALL_ON_OPEN,
        FILE_ATTRIBUTE_SPARSE_FILE, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, GetCompressedFileSizeW,
        GetFileInformationByHandle, INVALID_FILE_SIZE, OPEN_EXISTING,
    };

    /// NTFS's default cluster size. Files take whole clusters, so this is
    /// closer to Explorer's "Size on disk" than the logical length.
    const CLUSTER: u64 = 4096;

    /// OneDrive and other cloud placeholders: their data isn't on this disk,
    /// and opening them could download it, so they're never touched.
    const PLACEHOLDER: u32 = FILE_ATTRIBUTE_OFFLINE
        | FILE_ATTRIBUTE_RECALL_ON_OPEN
        | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS;

    fn round_up(n: u64) -> u64 {
        n.div_ceil(CLUSTER).saturating_mul(CLUSTER)
    }

    /// NUL-terminated UTF-16, with the `\\?\` prefix long paths need.
    fn wide(path: &Path) -> Vec<u16> {
        let raw = path.as_os_str();
        let long = raw.len() >= 248 && !raw.to_string_lossy().starts_with(r"\\");
        let mut out: Vec<u16> = Vec::with_capacity(raw.len() + 5);
        if long && path.is_absolute() {
            out.extend(r"\\?\".encode_utf16());
        }
        out.extend(raw.encode_wide());
        out.push(0);
        out
    }

    pub fn allocated_bytes(path: &Path, meta: &Metadata) -> u64 {
        if meta.is_dir() || meta.file_type().is_symlink() {
            return 0;
        }
        let attrs = meta.file_attributes();
        if attrs & PLACEHOLDER != 0 {
            return 0;
        }
        if attrs & (FILE_ATTRIBUTE_COMPRESSED | FILE_ATTRIBUTE_SPARSE_FILE) != 0 {
            let name = wide(path);
            let mut high = 0u32;
            // SAFETY: `name` is NUL-terminated and outlives the call; `high` is a valid out pointer.
            let low = unsafe { GetCompressedFileSizeW(name.as_ptr(), &raw mut high) };
            // SAFETY: reads this thread's last-error value; no preconditions.
            if low != INVALID_FILE_SIZE || unsafe { GetLastError() } == NO_ERROR {
                return round_up((u64::from(high) << 32) | u64::from(low));
            }
        }
        round_up(meta.len())
    }

    /// Volume serial and file index for files with more than one link. Opens
    /// the file for attributes only (no data access), which never downloads
    /// a cloud file or trips a lock.
    pub fn hard_link_id(path: &Path, meta: &Metadata) -> Option<(u64, u64)> {
        if meta.is_dir()
            || meta.file_type().is_symlink()
            || meta.len() == 0
            || meta.file_attributes() & PLACEHOLDER != 0
        {
            return None;
        }
        let name = wide(path);
        // SAFETY: `name` is NUL-terminated; null security attributes and template are allowed.
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return None;
        }
        // SAFETY: an all-zero BY_HANDLE_FILE_INFORMATION is a valid value.
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        // SAFETY: `handle` is open and `info` is a valid out pointer.
        let ok = unsafe { GetFileInformationByHandle(handle, &raw mut info) } != 0;
        // SAFETY: `handle` is open and closed exactly once.
        unsafe { CloseHandle(handle) };
        (ok && info.nNumberOfLinks > 1).then(|| {
            (
                u64::from(info.dwVolumeSerialNumber),
                (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
            )
        })
    }

    /// Always the same: on Windows another drive can only appear inside a
    /// folder as a mount point or junction, which is a reparse point and is
    /// never followed, so the walk can't leave the volume anyway.
    pub fn device(_meta: &Metadata) -> u64 {
        0
    }
}
