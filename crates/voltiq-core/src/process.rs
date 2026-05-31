//! Small cross-platform process/runtime helpers shared across crates.
//! Heavy spawning/attach logic lives in `voltiq-perf`; this is just the basics.

use std::time::{SystemTime, UNIX_EPOCH};

/// Current wall-clock time as unix epoch milliseconds (computed at runtime — never
/// hardcode timestamps). Returns 0 if the clock is before the epoch.
pub fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The OS we're running on: `"linux"`, `"macos"`, `"windows"`, …
pub fn current_os() -> &'static str {
    std::env::consts::OS
}
