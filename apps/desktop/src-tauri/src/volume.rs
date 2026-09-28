//! Disk capacity for the status card (spec §4.3). On macOS "available" is
//! `NSURLVolumeAvailableCapacityForImportantUsageKey`, which counts purgeable
//! space and matches what Finder shows. On Windows it's the system drive's
//! free space, as File Explorer shows it; Windows has no purgeable space.

use serde::Serialize;
use specta::Type;

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VolumeInfo {
    pub name: String,
    #[specta(type = u32)]
    pub total: f64,
    /// What Finder calls available: includes purgeable space.
    #[specta(type = u32)]
    pub available: f64,
    /// Space macOS frees on its own when it needs it.
    #[specta(type = u32)]
    pub purgeable: f64,
}

#[cfg(target_os = "macos")]
pub fn main_volume() -> Option<VolumeInfo> {
    macos::main_volume()
}

#[cfg(windows)]
pub fn main_volume() -> Option<VolumeInfo> {
    windows::main_volume()
}

#[cfg(target_os = "linux")]
pub fn main_volume() -> Option<VolumeInfo> {
    linux::main_volume()
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub fn main_volume() -> Option<VolumeInfo> {
    None
}

/// The filesystem the home folder is on: it's often its own partition on
/// Linux, and it's what a scan measures.
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
mod linux {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    use super::VolumeInfo;

    #[allow(clippy::cast_precision_loss)]
    pub fn main_volume() -> Option<VolumeInfo> {
        let home = std::env::var_os("HOME").unwrap_or_else(|| "/".into());
        let path = CString::new(home.as_bytes()).ok()?;
        // SAFETY: an all-zero statvfs is a valid value to fill in.
        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        // SAFETY: `path` is NUL-terminated and `stat` is a valid out pointer.
        if unsafe { libc::statvfs(path.as_ptr(), &raw mut stat) } != 0 {
            return None;
        }
        let block = stat.f_frsize as f64;
        Some(VolumeInfo {
            name: "Main disk".to_string(),
            total: stat.f_blocks as f64 * block,
            // What an ordinary user can still write (excludes root's reserve).
            available: stat.f_bavail as f64 * block,
            purgeable: 0.0,
        })
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod windows {
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetVolumeInformationW};

    use super::VolumeInfo;

    #[allow(clippy::cast_precision_loss)]
    pub fn main_volume() -> Option<VolumeInfo> {
        let drive = std::env::var_os("SystemDrive").unwrap_or_else(|| "C:".into());
        let mut root: Vec<u16> = drive.encode_wide().collect();
        root.extend("\\".encode_utf16());
        root.push(0);

        let (mut available, mut total, mut free) = (0u64, 0u64, 0u64);
        // SAFETY: `root` is NUL-terminated; the out pointers are valid u64s.
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                root.as_ptr(),
                &raw mut available,
                &raw mut total,
                &raw mut free,
            )
        };
        if ok == 0 {
            return None;
        }

        let mut label = [0u16; 261];
        // SAFETY: `root` is NUL-terminated and `label` holds its stated length;
        // the optional out pointers are null.
        let named = unsafe {
            GetVolumeInformationW(
                root.as_ptr(),
                label.as_mut_ptr(),
                261,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                0,
            )
        } != 0;
        let len = label.iter().position(|&c| c == 0).unwrap_or(0);
        let letter = drive.to_string_lossy().into_owned();
        let name = if named && len > 0 {
            format!("{} ({letter})", String::from_utf16_lossy(&label[..len]))
        } else {
            format!("Local Disk ({letter})")
        };
        Some(VolumeInfo {
            name,
            total: total as f64,
            available: available as f64,
            purgeable: 0.0,
        })
    }
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod macos {
    use objc2::rc::Retained;
    use objc2_foundation::{
        NSArray, NSNumber, NSString, NSURL, NSURLResourceKey,
        NSURLVolumeAvailableCapacityForImportantUsageKey, NSURLVolumeAvailableCapacityKey,
        NSURLVolumeNameKey, NSURLVolumeTotalCapacityKey,
    };

    use super::VolumeInfo;

    #[allow(clippy::cast_precision_loss)]
    pub fn main_volume() -> Option<VolumeInfo> {
        let url = NSURL::fileURLWithPath(&NSString::from_str("/"));
        // SAFETY: these are immutable NSString constants exported by Foundation.
        let (total_key, important_key, plain_key, name_key): (
            &NSURLResourceKey,
            &NSURLResourceKey,
            &NSURLResourceKey,
            &NSURLResourceKey,
        ) = unsafe {
            (
                NSURLVolumeTotalCapacityKey,
                NSURLVolumeAvailableCapacityForImportantUsageKey,
                NSURLVolumeAvailableCapacityKey,
                NSURLVolumeNameKey,
            )
        };
        let keys = NSArray::from_slice(&[total_key, important_key, plain_key, name_key]);
        let values = url.resourceValuesForKeys_error(&keys).ok()?;

        let number = |key: &NSURLResourceKey| -> Option<f64> {
            let obj = values.objectForKey(key)?;
            let n: Retained<NSNumber> = obj.downcast().ok()?;
            Some(n.longLongValue() as f64)
        };
        let total = number(total_key)?;
        let available = number(important_key)?;
        let plain = number(plain_key).unwrap_or(available);
        let name = values
            .objectForKey(name_key)
            .and_then(|o| o.downcast::<NSString>().ok())
            .map_or_else(|| "Macintosh HD".to_string(), |s| s.to_string());
        Some(VolumeInfo {
            name,
            total,
            available,
            purgeable: (available - plain).max(0.0),
        })
    }
}
