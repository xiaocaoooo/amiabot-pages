use chrono::{DateTime, Local};

/// Format unix seconds as local `YYYY-MM-DD HH:MM:SS`.
pub fn format_unix_secs(secs: i64) -> String {
    if secs <= 0 {
        return String::new();
    }
    match DateTime::from_timestamp(secs, 0) {
        Some(dt) => dt.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S").to_string(),
        None => String::new(),
    }
}

/// Format unix milliseconds as local `YYYY-MM-DD HH:MM:SS`.
pub fn format_unix_millis(ms: i64) -> String {
    if ms <= 0 {
        return String::new();
    }
    format_unix_secs(ms / 1000)
}
