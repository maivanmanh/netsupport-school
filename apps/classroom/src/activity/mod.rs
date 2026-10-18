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
pub mod evidence;
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

/// Largest program icon (base64) put into a live summary; the Teacher accepts
/// summaries up to [`MAX_SUMMARY_BYTES`], enough for [`LIVE_APPS`] such icons.
pub const MAX_ICON_BASE64: usize = 8 * 1024;
pub const MAX_SUMMARY_BYTES: usize = 56 * 1024;
/// Most distinct site names reported per Student for AI-site recognition.
pub const MAX_HOSTS: usize = 24;
/// Most distinct tab titles reported per Student.
pub const MAX_TITLES: usize = 24;

/// Most apps shown per Student on the Teacher's roster tiles (upper bound of
/// [`ActivityOptions::live_apps`]).
pub const LIVE_APPS: usize = 5;

/// How a Student collects activity, chosen by the Teacher (Settings → Theo dõi
/// hoạt động) and sent whenever monitoring is switched on or the choice changes.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ActivityOptions {
    pub scan_seconds: u32,
    pub live_apps: u32,
    pub collect_clipboard: bool,
    pub collect_downloads: bool,
    /// Take screenshots as evidence of AI-site use (see [`evidence`]).
    pub evidence: bool,
    pub evidence_per_site: u32,
    pub evidence_first_seconds: u32,
    pub evidence_gap_seconds: u32,
    /// The AI sites to watch for; empty while they are blocked (nothing to see)
    /// or when the Teacher turned evidence off.
    pub watch: Vec<String>,
}
impl Default for ActivityOptions {
    fn default() -> Self {
        Self {
            scan_seconds: 6,
            live_apps: 3,
            collect_clipboard: true,
            collect_downloads: true,
            evidence: true,
            evidence_per_site: 3,
            evidence_first_seconds: 60,
            evidence_gap_seconds: 180,
            watch: Vec::new(),
        }
    }
}
impl ActivityOptions {
    pub fn from_config(c: &classroom_config::ActivityConfig) -> Self {
        Self {
            scan_seconds: c.scan_seconds,
            live_apps: c.live_apps,
            collect_clipboard: c.collect_clipboard,
            collect_downloads: c.collect_downloads,
            evidence: c.evidence,
            evidence_per_site: c.evidence_per_site,
            evidence_first_seconds: c.evidence_first_seconds,
            evidence_gap_seconds: c.evidence_gap_seconds,
            watch: Vec::new(),
        }
    }
    /// A Student pulls whatever it is sent back into the supported range.
    pub fn sanitized(mut self) -> Self {
        self.scan_seconds = self.scan_seconds.clamp(3, 120);
        self.live_apps = self.live_apps.clamp(1, 5);
        self.evidence_per_site = self.evidence_per_site.clamp(1, 10);
        self.evidence_first_seconds = self.evidence_first_seconds.clamp(20, 600);
        self.evidence_gap_seconds = self.evidence_gap_seconds.clamp(60, 1800);
        self.watch.truncate(200);
        self
    }
}
/// Options shared by the collector, the clipboard watcher and the live summary.
pub type SharedOptions = std::sync::Arc<std::sync::Mutex<ActivityOptions>>;

/// `host/path` of an address (no scheme, port, query or fragment), lower-case.
/// This is what the Teacher compares with its AI list.
pub fn address_of(url: &str) -> Option<String> {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let rest = rest.split(['?', '#']).next().unwrap_or("");
    let (authority, path) = rest.split_once('/').map_or((rest, ""), |(a, p)| (a, p));
    let host = authority.rsplit('@').next().unwrap_or("").split(':').next().unwrap_or("");
    let host = host.trim().to_ascii_lowercase();
    if host.is_empty() || host.len() > 120 {
        return None;
    }
    let path: String = path.trim_end_matches('/').chars().take(100).collect();
    Some(if path.is_empty() { host } else { format!("{host}/{}", path.to_ascii_lowercase()) })
}

fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// A few hundred bytes describing what a Student is doing right now, for the
/// Teacher's roster tiles: the front-most applications (window order, most
/// recent first), and, when the front-most one is a browser, its active tab's
/// address (the Teacher fetches the site icon itself). Program icons are sent
/// once per connection (`sent_icons`) and cached by the Teacher, so repeats
/// stay tiny. `None` while monitoring is off or nothing is open.
pub fn live_summary(
    report: &Report,
    sent_icons: &mut std::collections::HashSet<String>,
    max_apps: usize,
) -> Option<serde_json::Value> {
    if !report.monitoring_enabled {
        return None;
    }
    let mut seen: Vec<&WindowEntry> = Vec::new();
    for window in &report.windows {
        if !window.app.is_empty()
            && !seen
                .iter()
                .any(|w| w.app.eq_ignore_ascii_case(&window.app))
        {
            seen.push(window);
        }
        if seen.len() == max_apps.clamp(1, LIVE_APPS) {
            break;
        }
    }
    let front = seen.first()?;
    let apps: Vec<_> = seen
        .iter()
        .map(|w| {
            let mut entry = serde_json::json!({"app": clip(&w.app, 64), "title": clip(&w.title, 80)});
            if let Some(icon) = report
                .apps
                .iter()
                .find(|a| a.app.eq_ignore_ascii_case(&w.app))
                .and_then(|a| a.icon_png_base64.as_deref())
            {
                if icon.len() <= MAX_ICON_BASE64 && sent_icons.insert(w.app.to_ascii_lowercase()) {
                    entry["icon"] = serde_json::json!(icon);
                }
            }
            entry
        })
        .collect();
    let mut summary = serde_json::json!({"apps": apps});
    // The address of the page a browser is showing. A browser that is open but
    // not in front still counts: the front one wins, else the most recently
    // used (window order).
    let shown = |browser: &BrowserWindow| {
        browser
            .tabs
            .iter()
            .find(|t| t.active && t.url.is_some())
            .map(|t| (browser.app.clone(), t.clone()))
    };
    let chosen = report
        .browsers
        .iter()
        .filter(|b| b.app.eq_ignore_ascii_case(&front.app))
        .find_map(shown)
        .or_else(|| {
            // Window order is front-to-back, so go through the windows and
            // pick the first browser that has a page to show.
            report
                .windows
                .iter()
                .filter_map(|w| {
                    report
                        .browsers
                        .iter()
                        .find(|b| b.app.eq_ignore_ascii_case(&w.app))
                })
                .find_map(shown)
        })
        .or_else(|| report.browsers.iter().find_map(shown));
    if let Some((app, tab)) = chosen {
        summary["url"] = serde_json::json!(clip(tab.url.as_deref().unwrap_or(""), 300));
        summary["tab_title"] = serde_json::json!(clip(&tab.title, 80));
        summary["url_app"] = serde_json::json!(clip(&app, 64));
        summary["url_in_front"] = serde_json::json!(app.eq_ignore_ascii_case(&front.app));
    }
    // Every site open in any tab of any browser, for the Teacher to recognise
    // (AI sites) whether or not that browser is in front.
    let mut hosts: Vec<String> = Vec::new();
    for tab in report.browsers.iter().flat_map(|b| b.tabs.iter()) {
        let Some(url) = tab.url.as_deref() else { continue };
        if let Some(address) = address_of(url) {
            if !hosts.contains(&address) && hosts.len() < MAX_HOSTS {
                hosts.push(address);
            }
        }
    }
    if !hosts.is_empty() {
        summary["hosts"] = serde_json::json!(hosts);
    }
    // Page titles of every open tab: a tab whose address is not known yet (a
    // page opened a moment ago is not in the history) can still be recognised.
    let mut titles: Vec<String> = Vec::new();
    for tab in report.browsers.iter().flat_map(|b| b.tabs.iter()) {
        let title = clip(tab.title.trim(), 90);
        if !title.is_empty() && !titles.contains(&title) && titles.len() < MAX_TITLES {
            titles.push(title);
        }
    }
    if !titles.is_empty() {
        summary["titles"] = serde_json::json!(titles);
    }
    Some(summary)
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

#[cfg(test)]
mod live_tests {
    use super::*;
    use std::collections::HashSet;

    fn window(app: &str, title: &str) -> WindowEntry {
        WindowEntry { app: app.into(), title: title.into() }
    }
    fn sample() -> Report {
        Report {
            supported: true,
            monitoring_enabled: true,
            windows: vec![
                window("chrome.exe", "Docs - Chrome"),
                window("Code.exe", "main.rs"),
                window("chrome.exe", "Other window"),
                window("notepad.exe", "todo"),
                window("calc.exe", "Calculator"),
            ],
            browsers: vec![BrowserWindow {
                app: "chrome.exe".into(),
                tabs: vec![
                    TabEntry { title: "Background".into(), ..Default::default() },
                    TabEntry {
                        title: "Docs".into(),
                        url: Some("https://docs.rs/x".into()),
                        active: true,
                        ..Default::default()
                    },
                ],
                unavailable: false,
            }],
            apps: vec![AppEntry {
                app: "chrome.exe".into(),
                icon_png_base64: Some("AAAA".into()),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn three_distinct_front_apps_and_the_browser_address() {
        let mut sent = HashSet::new();
        let summary = live_summary(&sample(), &mut sent, 3).unwrap();
        let apps: Vec<_> = summary["apps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["app"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(apps, ["chrome.exe", "Code.exe", "notepad.exe"]);
        assert_eq!(summary["url"], "https://docs.rs/x");
        assert_eq!(summary["tab_title"], "Docs");
        assert_eq!(summary["apps"][0]["icon"], "AAAA");
    }

    #[test]
    fn a_browser_that_is_open_but_not_in_front_still_reports_its_page() {
        let mut sent = HashSet::new();
        let mut editor_front = sample();
        editor_front.windows.rotate_left(1); // Code first, Chrome behind it
        let summary = live_summary(&editor_front, &mut sent, 3).unwrap();
        assert_eq!(summary["apps"][0]["app"], "Code.exe");
        assert_eq!(summary["url"], "https://docs.rs/x");
        assert_eq!(summary["url_app"], "chrome.exe");
        assert_eq!(summary["url_in_front"], false);
        // In front, it says so.
        let front = live_summary(&sample(), &mut HashSet::new(), 3).unwrap();
        assert_eq!(front["url_in_front"], true);
    }

    #[test]
    fn addresses_are_reduced_to_host_and_path() {
        assert_eq!(address_of("https://Chatgpt.com/c/123?x=1#y").as_deref(), Some("chatgpt.com/c/123"));
        assert_eq!(address_of("http://user@localhost:3000/").as_deref(), Some("localhost"));
        assert_eq!(address_of("https://a.dev/x/").as_deref(), Some("a.dev/x"));
        assert_eq!(address_of("edge://newtab").as_deref(), Some("newtab"));
        assert_eq!(address_of("").as_deref(), None);
    }

    #[test]
    fn every_open_tab_site_is_reported_for_recognition() {
        let mut report = sample();
        report.browsers[0].tabs.push(TabEntry {
            title: "Chat".into(),
            url: Some("https://Chatgpt.com/c/123?x=1".into()),
            ..Default::default()
        });
        report.browsers.push(BrowserWindow {
            app: "msedge.exe".into(),
            tabs: vec![TabEntry {
                url: Some("http://localhost:3000/".into()),
                active: true,
                ..Default::default()
            }],
            unavailable: false,
        });
        let summary = live_summary(&report, &mut HashSet::new(), 3).unwrap();
        let hosts: Vec<_> = summary["hosts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| h.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(hosts, ["docs.rs/x", "chatgpt.com/c/123", "localhost"]);
    }

    #[test]
    fn icons_are_sent_once() {
        let mut sent = HashSet::new();
        let first = live_summary(&sample(), &mut sent, 3).unwrap();
        assert!(first["apps"][0].get("icon").is_some());
        let again = live_summary(&sample(), &mut sent, 3).unwrap();
        assert!(again["apps"][0].get("icon").is_none());

        let mut editor_front = sample();
        editor_front.windows.rotate_left(1);
        let summary = live_summary(&editor_front, &mut sent, 3).unwrap();
        assert_eq!(summary["apps"][0]["app"], "Code.exe");
        // The browser behind it still reports its page (marked as not in front).
        assert_eq!(summary["url_in_front"], false);
    }

    #[test]
    fn the_largest_possible_summary_fits_what_the_teacher_accepts() {
        // (tab titles and hosts add to the icons: see the sizes below)
        let mut report = Report { supported: true, monitoring_enabled: true, ..Default::default() };
        for i in 0..LIVE_APPS {
            let name = format!("app{i}.exe");
            report.windows.push(window(&name, &"t".repeat(200)));
            report.apps.push(AppEntry {
                app: name,
                icon_png_base64: Some("A".repeat(MAX_ICON_BASE64)),
                ..Default::default()
            });
        }
        // ...and a browser with as many long titles and addresses as are reported.
        report.browsers.push(BrowserWindow {
            app: "app0.exe".into(),
            tabs: (0..40)
                .map(|i| TabEntry {
                    title: format!("{i}{}", "t".repeat(200)),
                    url: Some(format!("https://site{i}{}.example/{}", "h".repeat(100), "p".repeat(400))),
                    active: i == 0,
                    ..Default::default()
                })
                .collect(),
            unavailable: false,
        });
        let mut sent = HashSet::new();
        let summary = live_summary(&report, &mut sent, LIVE_APPS).unwrap();
        assert!(summary.to_string().len() < MAX_SUMMARY_BYTES, "{} bytes", summary.to_string().len());
        assert!(MAX_SUMMARY_BYTES < 64 * 1024, "one tools packet is at most 64 KiB");
        // An icon over the cap is dropped instead of sent.
        report.apps[0].icon_png_base64 = Some("A".repeat(MAX_ICON_BASE64 + 1));
        let mut fresh = HashSet::new();
        let again = live_summary(&report, &mut fresh, 1).unwrap();
        assert!(again["apps"][0].get("icon").is_none());
    }

    #[test]
    fn the_number_of_apps_follows_the_setting() {
        let mut sent = HashSet::new();
        let one = live_summary(&sample(), &mut sent, 1).unwrap();
        assert_eq!(one["apps"].as_array().unwrap().len(), 1);
        let five = live_summary(&sample(), &mut sent, 5).unwrap();
        assert_eq!(five["apps"].as_array().unwrap().len(), 4, "only 4 distinct apps are open");
        assert_eq!(ActivityOptions { scan_seconds: 1, live_apps: 99, ..Default::default() }.sanitized().live_apps, 5);
        assert_eq!(ActivityOptions { scan_seconds: 1, ..Default::default() }.sanitized().scan_seconds, 3);
        assert_eq!(ActivityOptions::default().scan_seconds, 6, "today's behaviour is the default");
    }

    #[test]
    fn nothing_is_reported_while_monitoring_is_off_or_empty() {
        let mut sent = HashSet::new();
        assert!(live_summary(&Report::default(), &mut sent, 3).is_none());
        let mut report = sample();
        report.windows.clear();
        assert!(live_summary(&report, &mut sent, 3).is_none());
    }
}
