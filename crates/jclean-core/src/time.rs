use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Seconds since the Unix epoch; negative for times before it.
pub fn unix_secs(t: SystemTime) -> i64 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
        Err(e) => -i64::try_from(e.duration().as_secs()).unwrap_or(i64::MAX),
    }
}

pub fn now_secs() -> i64 {
    unix_secs(SystemTime::now())
}

pub const DAY_SECS: i64 = 86_400;

pub fn days(n: u32) -> Duration {
    Duration::from_secs(u64::from(n) * 86_400)
}
