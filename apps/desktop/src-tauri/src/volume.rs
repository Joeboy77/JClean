//! Disk capacity for the status card (spec §4.3). On macOS "available" is
//! `NSURLVolumeAvailableCapacityForImportantUsageKey`, which counts purgeable
//! space and matches what Finder shows.

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

#[cfg(not(target_os = "macos"))]
pub fn main_volume() -> Option<VolumeInfo> {
    None
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
