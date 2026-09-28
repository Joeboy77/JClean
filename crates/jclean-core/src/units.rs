//! Size formatting in decimal units (GB = 10⁹ bytes), matching Finder.

const KB: u64 = 1_000;
const MB: u64 = 1_000_000;
const GB: u64 = 1_000_000_000;
const TB: u64 = 1_000_000_000_000;

/// "6.1 GB", "824 MB", "12 KB", "512 bytes". One decimal place from 1 GB up.
pub fn format_bytes(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let tenths = |unit: u64| (bytes as f64 / unit as f64 * 10.0).round() / 10.0;
    if bytes >= TB {
        format!("{:.1} TB", tenths(TB))
    } else if bytes >= GB {
        format!("{:.1} GB", tenths(GB))
    } else if bytes >= MB {
        format!("{} MB", (bytes + MB / 2) / MB)
    } else if bytes >= KB {
        format!("{} KB", (bytes + KB / 2) / KB)
    } else if bytes == 1 {
        "1 byte".to_string()
    } else {
        format!("{bytes} bytes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_decimal_units() {
        assert_eq!(format_bytes(0), "0 bytes");
        assert_eq!(format_bytes(999), "999 bytes");
        assert_eq!(format_bytes(12_400), "12 KB");
        assert_eq!(format_bytes(824_000_000), "824 MB");
        assert_eq!(format_bytes(6_120_000_000), "6.1 GB");
        assert_eq!(format_bytes(38_650_000_000), "38.7 GB");
        assert_eq!(format_bytes(2_000_000_000_000), "2.0 TB");
    }
}
