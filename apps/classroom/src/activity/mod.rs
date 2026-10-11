//! Snapshot of what is visibly happening on a Student computer: open
//! application windows, browser windows/tabs (with the active tab's address
//! and favicon), and recent browser downloads — plus, via [`history::History`],
//! how long each has been open across the scans the Teacher actually
//! requested. See `classroom_tools/activity_jobs.rs` for the request/response
//! plumbing and `docs/22_ACTIVITY_MONITORING.md` for the full design and its
//! limits.
//!
//! Windows only. Nothing here reads keystrokes. Plain-text clipboard copies
//! are recorded only while the Teacher has monitoring switched on — see
//! [`clipboard`] — which is distinct from `clipboard_guard.rs`'s
//! blocking-only mode (that one discards every copy and never reads it).
pub mod clipboard;
pub mod collector;
pub mod history;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) use windows::{process_name_for_pid, program_icon};

use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_WINDOWS: usize = 80;
pub const MAX_TABS: usize = 80;
pub const MAX_DOWNLOADS: usize = 50;
pub const MAX_CLIPBOARD: usize = clipboard::MAX_ENTRIES;

pub(crate) fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct WindowEntry {
    pub app: String,
    pub title: String,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct TabEntry {
    pub title: String,
    /// Only ever populated for the browser's foreground tab (the address
    /// bar); a background tab does not expose its URL through accessibility.
    pub url: Option<String>,
    /// PNG bytes, base64-encoded; same availability as `url`.
    pub favicon_png_base64: Option<String>,
    /// Foreground tab of its window right now. Always `false` in the
    /// flattened `Report::tabs` history list (not a tracked identity there).
    pub active: bool,
    pub open: bool,
    pub opened_ms: i64,
    pub closed_ms: i64,
    pub total_open_ms: i64,
    pub reopened: u32,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct BrowserWindow {
    pub app: String,
    pub tabs: Vec<TabEntry>,
    /// The tab list could not be read (e.g. the browser was busy).
    pub unavailable: bool,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct ClipboardEntry {
    pub text: String,
    /// `text` was cut off at [`clipboard::MAX_CHARS`] characters.
    pub truncated: bool,
    /// Foreground process at the moment of the copy, best effort (may be empty).
    pub app: String,
    pub unix_ms: i64,
    /// How many later copies were near-duplicates of this one (merged rather
    /// than listed again) — see `history::similar`.
    pub duplicates: u32,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct DownloadEntry {
    pub name: String,
    pub url: String,
    pub browser: String,
    pub unix_ms: i64,
    pub bytes: i64,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct AppEntry {
    pub app: String,
    /// Most recently observed window title for this app.
    pub title: String,
    pub open: bool,
    pub opened_ms: i64,
    pub closed_ms: i64,
    pub total_open_ms: i64,
    pub reopened: u32,
    /// The program's own icon (PNG, base64), extracted from its executable.
    pub icon_png_base64: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct Report {
    pub supported: bool,
    /// Whether the Teacher has activity monitoring switched on for this
    /// Student; when `false` every other field is empty (nothing was
    /// scanned) rather than stale leftover data.
    pub monitoring_enabled: bool,
    /// Open right now, grouped by window (quick glance).
    pub windows: Vec<WindowEntry>,
    pub windows_truncated: bool,
    pub browsers: Vec<BrowserWindow>,
    pub downloads: Vec<DownloadEntry>,
    /// Every plain-text copy seen while monitoring was on, newest first.
    pub clipboard: Vec<ClipboardEntry>,
    /// Every app seen this tracked session, open or closed, newest first.
    pub apps: Vec<AppEntry>,
    /// Every tab seen this tracked session, open or closed, newest first.
    pub tabs: Vec<TabEntry>,
    /// Non-fatal problems (e.g. a browser profile that could not be read).
    pub notes: Vec<String>,
}

#[cfg(windows)]
pub fn build(history: &history::History, include_downloads: bool) -> Report {
    let mut report = windows::build(history, include_downloads);
    report.clipboard = history.clipboard();
    report
}
#[cfg(not(windows))]
pub fn build(_history: &history::History, _include_downloads: bool) -> Report {
    Report::default()
}
