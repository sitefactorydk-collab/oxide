//! The computer's time zone, for scheduled tasks (cron times are local wall-clock times).

use chrono::{Local, TimeZone as _};
use oxide_core::{LocalZone, Timestamp};

/// Local time as the operating system reports it (daylight saving time included).
pub struct SystemZone;

impl LocalZone for SystemZone {
    fn offset_seconds(&self, utc_ms: Timestamp) -> i32 {
        // A moment in UTC has exactly one local time; out of range falls back to UTC.
        Local
            .timestamp_millis_opt(utc_ms)
            .earliest()
            .map_or(0, |t| t.offset().local_minus_utc())
    }
}
