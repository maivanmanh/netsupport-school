//! Turns successive activity snapshots into "what has been open, since when,
//! for how long, and how many times" — built entirely from scans the Teacher
//! actually requested (opening/refreshing the Activity window), so there are
//! gaps in coverage whenever nobody was watching; this is documented to the
//! Teacher, not hidden. Lives for the Student process's lifetime, in memory
//! only (never written to disk).
use super::{now_ms, AppEntry, DownloadEntry, TabEntry};
use std::collections::HashMap;
use std::sync::Mutex;

/// Closed entries beyond this count are dropped, oldest-closed first; an
/// open entry is never evicted.
const MAX_TRACKED: usize = 400;

struct Track {
    title: String,
    url: Option<String>,
    favicon_png_base64: Option<String>,
    open: bool,
    opened_ms: i64,
    closed_ms: i64,
    /// Committed duration from completed streaks; the live streak is added
    /// on read.
    total_open_ms: i64,
    reopened: u32,
}
impl Track {
    fn start(now: i64, title: String) -> Self {
        Self {
            title,
            url: None,
            favicon_png_base64: None,
            open: true,
            opened_ms: now,
            closed_ms: 0,
            total_open_ms: 0,
            reopened: 0,
        }
    }
}

#[derive(Default)]
pub struct History {
    apps: Mutex<HashMap<String, Track>>,
    tabs: Mutex<HashMap<String, Track>>,
    /// Last successful downloads scan (that step runs less often than the
    /// window/tab scan; keeping the last result here means a cycle that
    /// skips it doesn't make downloads flicker away in the meantime).
    downloads: Mutex<Vec<DownloadEntry>>,
}

/// One freshly observed item: `(identity, display title, url, favicon)`.
/// Identity is the stable key tracked across scans (process name for apps,
/// tab title for tabs — titles are the only stable-ish signal available for
/// a background tab; a page that renavigates under the same tab is seen as
/// closing one entry and opening another, a known, documented limitation).
pub type Seen = (String, String, Option<String>, Option<String>);

fn update(map: &mut HashMap<String, Track>, seen: &[Seen], now: i64) {
    let mut present: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for (key, title, url, favicon) in seen {
        present.insert(key.as_str());
        let entry = map
            .entry(key.clone())
            .or_insert_with(|| Track::start(now, title.clone()));
        title.clone_into(&mut entry.title);
        if url.is_some() {
            entry.url.clone_from(url);
        }
        if favicon.is_some() {
            entry.favicon_png_base64.clone_from(favicon);
        }
        if !entry.open {
            if entry.closed_ms != 0 {
                entry.reopened += 1;
            }
            entry.open = true;
            entry.opened_ms = now;
        }
    }
    for (key, entry) in &mut *map {
        if entry.open && !present.contains(key.as_str()) {
            entry.open = false;
            entry.total_open_ms += (now - entry.opened_ms).max(0);
            entry.closed_ms = now;
        }
    }
    if map.len() > MAX_TRACKED {
        let mut closed: Vec<(String, i64)> = map
            .iter()
            .filter(|(_, t)| !t.open)
            .map(|(k, t)| (k.clone(), t.closed_ms))
            .collect();
        closed.sort_by_key(|(_, closed_ms)| *closed_ms);
        for (key, _) in closed.into_iter().take(map.len() - MAX_TRACKED) {
            map.remove(&key);
        }
    }
}

/// Newest activity first: open items before closed ones, each group by most
/// recent activity.
fn order(a_open: bool, a_ms: i64, b_open: bool, b_ms: i64) -> std::cmp::Ordering {
    b_open.cmp(&a_open).then(b_ms.cmp(&a_ms))
}

fn app_entries(map: &HashMap<String, Track>, now: i64) -> Vec<AppEntry> {
    let mut entries: Vec<AppEntry> = map
        .iter()
        .map(|(key, t)| AppEntry {
            app: key.clone(),
            title: t.title.clone(),
            open: t.open,
            opened_ms: t.opened_ms,
            closed_ms: t.closed_ms,
            total_open_ms: t.total_open_ms + if t.open { (now - t.opened_ms).max(0) } else { 0 },
            reopened: t.reopened,
        })
        .collect();
    entries.sort_by(|a, b| order(a.open, a.closed_ms.max(a.opened_ms), b.open, b.closed_ms.max(b.opened_ms)));
    entries
}
fn tab_entries(map: &HashMap<String, Track>, now: i64) -> Vec<TabEntry> {
    let mut entries: Vec<TabEntry> = map
        .values()
        .map(|t| TabEntry {
            title: t.title.clone(),
            url: t.url.clone(),
            favicon_png_base64: t.favicon_png_base64.clone(),
            open: t.open,
            opened_ms: t.opened_ms,
            closed_ms: t.closed_ms,
            total_open_ms: t.total_open_ms + if t.open { (now - t.opened_ms).max(0) } else { 0 },
            reopened: t.reopened,
            active: false,
        })
        .collect();
    entries.sort_by(|a, b| order(a.open, a.closed_ms.max(a.opened_ms), b.open, b.closed_ms.max(b.opened_ms)));
    entries
}

impl History {
    /// Records a fresh scan and returns the merged, up-to-date history
    /// (every item ever seen this session, open or closed).
    pub fn record(&self, apps_seen: &[Seen], tabs_seen: &[Seen]) -> (Vec<AppEntry>, Vec<TabEntry>) {
        let now = now_ms();
        let mut apps = self.apps.lock().unwrap();
        update(&mut apps, apps_seen, now);
        let apps_out = app_entries(&apps, now);
        drop(apps);
        let mut tabs = self.tabs.lock().unwrap();
        update(&mut tabs, tabs_seen, now);
        let tabs_out = tab_entries(&tabs, now);
        (apps_out, tabs_out)
    }
    /// The current merged history without recording a new scan — a cheap,
    /// read-only snapshot of whatever the background collector last saw.
    pub fn snapshot(&self) -> (Vec<AppEntry>, Vec<TabEntry>) {
        let now = now_ms();
        let apps = self.apps.lock().unwrap();
        let tabs = self.tabs.lock().unwrap();
        (app_entries(&apps, now), tab_entries(&tabs, now))
    }
    pub fn set_downloads(&self, downloads: Vec<DownloadEntry>) {
        *self.downloads.lock().unwrap() = downloads;
    }
    pub fn downloads(&self) -> Vec<DownloadEntry> {
        self.downloads.lock().unwrap().clone()
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn seen(items: &[(&str, &str)]) -> Vec<Seen> {
        items
            .iter()
            .map(|(k, t)| ((*k).to_owned(), (*t).to_owned(), None, None))
            .collect()
    }

    #[test]
    fn tracks_open_close_and_reopen_with_accumulated_duration() {
        let history = History::default();
        let (apps, _) = history.record(&seen(&[("notepad.exe", "a.txt")]), &[]);
        assert_eq!((apps[0].open, apps[0].reopened), (true, 0));
        std::thread::sleep(std::time::Duration::from_millis(10));
        let (apps, _) = history.record(&[], &[]);
        assert!(!apps[0].open);
        assert!(apps[0].total_open_ms >= 10);
        let closed_total = apps[0].total_open_ms;
        std::thread::sleep(std::time::Duration::from_millis(10));
        let (apps, _) = history.record(&seen(&[("notepad.exe", "b.txt")]), &[]);
        assert_eq!((apps[0].open, apps[0].reopened), (true, 1));
        assert_eq!(apps[0].title, "b.txt");
        assert!(apps[0].total_open_ms >= closed_total);
    }

    #[test]
    fn eviction_keeps_open_entries_and_drops_oldest_closed_first() {
        let history = History::default();
        let mut keep = vec![("kept.exe", "kept")];
        for i in 0..MAX_TRACKED + 10 {
            let app = format!("app{i}.exe");
            history.record(&seen(&[(&app, "x")]), &[]);
            history.record(&[], &[]); // close it immediately
        }
        keep.push(("still-open.exe", "open"));
        let (apps, _) = history.record(&seen(&keep), &[]);
        assert!(apps.len() <= MAX_TRACKED);
        assert!(apps.iter().any(|a| a.app == "still-open.exe" && a.open));
    }
}
