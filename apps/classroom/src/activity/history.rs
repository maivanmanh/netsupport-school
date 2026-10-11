//! Turns successive activity snapshots into "what has been open, since when,
//! for how long, and how many times" — built entirely from scans the Teacher
//! actually requested (opening/refreshing the Activity window), so there are
//! gaps in coverage whenever nobody was watching; this is documented to the
//! Teacher, not hidden. Lives for the Student process's lifetime, in memory
//! only (never written to disk).
use super::{now_ms, AppEntry, ClipboardEntry, DownloadEntry, TabEntry, MAX_CLIPBOARD};
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
    /// Results of expensive lookups (executable icons, tab addresses from a
    /// browser's history database, site icons), so a 6-second scan does not
    /// re-copy databases for something it already resolved.
    lookups: Mutex<HashMap<String, (Option<String>, i64)>>,
    /// Newest first; capped at [`MAX_CLIPBOARD`].
    clipboard: Mutex<std::collections::VecDeque<ClipboardEntry>>,
}

/// A failed lookup is retried after this long (a page may simply not be in
/// the browser's history database yet).
const RETRY_MISSING_MS: i64 = 60_000;
const MAX_LOOKUPS: usize = 2_000;

pub enum Lookup {
    /// Already resolved (or recently tried and found nothing).
    Known(Option<String>),
    /// Never tried, or the negative result has expired.
    Needed,
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
/// Collapsed-whitespace, lowercased form used only to compare two copies —
/// never stored or shown.
fn normalize_for_compare(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}
/// Character bigrams of `text`, capped so a very long copy cannot make
/// every later comparison expensive; single-character strings have none.
fn bigrams(text: &str) -> std::collections::HashSet<(char, char)> {
    const MAX_COMPARE_CHARS: usize = 4_000;
    let chars: Vec<char> = text.chars().take(MAX_COMPARE_CHARS).collect();
    chars.windows(2).map(|w| (w[0], w[1])).collect()
}
/// Whether `a` and `b` are "the same copy" for the Teacher's purposes: equal
/// once whitespace-normalized, or at least 90% similar by Dice coefficient
/// over character bigrams (robust to a trailing space, a re-copy that picked
/// up one extra character, etc., without needing exact equality).
pub(crate) fn similar(a: &str, b: &str) -> bool {
    let (a, b) = (normalize_for_compare(a), normalize_for_compare(b));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    // Dice's coefficient is at most 2*min(|A|,|B|)/(|A|+|B|); below this
    // length ratio it cannot reach 0.9, so skip the set work entirely.
    let (shorter, longer) = if a.len() <= b.len() { (a.len(), b.len()) } else { (b.len(), a.len()) };
    if (shorter as f64) < (longer as f64) * 0.8 {
        return false;
    }
    let (sa, sb) = (bigrams(&a), bigrams(&b));
    if sa.is_empty() || sb.is_empty() {
        return false;
    }
    let common = sa.intersection(&sb).count();
    let dice = 2.0 * common as f64 / (sa.len() + sb.len()) as f64;
    dice >= 0.9
}

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
            icon_png_base64: t.favicon_png_base64.clone(),
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
    pub fn lookup(&self, key: &str) -> Lookup {
        match self.lookups.lock().unwrap().get(key) {
            Some((Some(value), _)) => Lookup::Known(Some(value.clone())),
            Some((None, tried)) if now_ms() - tried < RETRY_MISSING_MS => Lookup::Known(None),
            _ => Lookup::Needed,
        }
    }
    pub fn remember(&self, key: &str, value: Option<String>) {
        let mut map = self.lookups.lock().unwrap();
        if map.len() >= MAX_LOOKUPS {
            map.clear();
        }
        map.insert(key.to_owned(), (value, now_ms()));
    }
    pub fn set_downloads(&self, downloads: Vec<DownloadEntry>) {
        *self.downloads.lock().unwrap() = downloads;
    }
    pub fn downloads(&self) -> Vec<DownloadEntry> {
        self.downloads.lock().unwrap().clone()
    }
    /// Called from the clipboard watcher's own thread, every time something
    /// is copied while monitoring is on. A copy that is at least 90%
    /// similar (see [`similar`]) to one already logged today is treated as
    /// the same copy repeated: it is not listed again, and the existing
    /// entry moves to the front with an updated time and a bumped
    /// duplicate count, so a URL (or anything else) copied many times
    /// shows once.
    pub fn push_clipboard(&self, entry: ClipboardEntry) {
        let mut log = self.clipboard.lock().unwrap();
        if let Some(index) = log.iter().position(|e| similar(&e.text, &entry.text)) {
            let mut existing = log.remove(index).unwrap();
            existing.unix_ms = entry.unix_ms;
            existing.app = entry.app;
            existing.duplicates += 1;
            log.push_front(existing);
            return;
        }
        log.push_front(entry);
        log.truncate(MAX_CLIPBOARD);
    }
    pub fn clipboard(&self) -> Vec<ClipboardEntry> {
        self.clipboard.lock().unwrap().iter().cloned().collect()
    }
    /// Switching monitoring off ends collection and discards what was
    /// logged, matching the rest of this history.
    pub fn clear_clipboard(&self) {
        self.clipboard.lock().unwrap().clear();
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
    fn lookups_keep_hits_and_retry_misses_later() {
        let history = History::default();
        assert!(matches!(history.lookup("k"), Lookup::Needed));
        history.remember("k", Some("v".into()));
        assert!(matches!(history.lookup("k"), Lookup::Known(Some(v)) if v == "v"));
        history.remember("miss", None);
        assert!(matches!(history.lookup("miss"), Lookup::Known(None)));
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
#[cfg(test)]
mod clipboard_tests {
    use super::*;
    use crate::activity::ClipboardEntry;

    fn entry(text: &str, unix_ms: i64) -> ClipboardEntry {
        ClipboardEntry { text: text.into(), truncated: false, app: "notepad.exe".into(), unix_ms, duplicates: 0 }
    }

    #[test]
    fn clipboard_log_is_newest_first_and_bounded() {
        let history = History::default();
        // Knuth multiplicative hashing scrambles consecutive indices into
        // unrelated-looking hex strings, so no two pushed copies are
        // near-duplicates of each other.
        let scrambled = |i: usize| format!("{:08x}", (i as u32).wrapping_mul(2_654_435_761));
        for i in 0..MAX_CLIPBOARD + 5 {
            history.push_clipboard(entry(&scrambled(i), i as i64));
        }
        let log = history.clipboard();
        assert_eq!(log.len(), MAX_CLIPBOARD);
        assert_eq!(log[0].text, scrambled(MAX_CLIPBOARD + 4));
        history.clear_clipboard();
        assert!(history.clipboard().is_empty());
    }

    #[test]
    fn a_url_copied_many_times_is_shown_once_with_a_duplicate_count() {
        let history = History::default();
        history.push_clipboard(entry("https://example.com/some/path?x=1", 1));
        history.push_clipboard(entry("https://example.com/some/path?x=1", 2));
        history.push_clipboard(entry("https://example.com/some/path?x=1 ", 3)); // trailing space
        let log = history.clipboard();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].duplicates, 2);
        assert_eq!(log[0].unix_ms, 3); // moved to front with the latest time
    }

    #[test]
    fn similar_strings_merge_but_different_ones_do_not() {
        assert!(similar("Hello, world!", "hello,   world! "));
        assert!(similar(
            "function add(a, b) { return a + b; }",
            "function add(a, b) { return a + b }",
        ));
        assert!(!similar("https://example.com/a", "https://example.com/b/totally/different/path"));
        assert!(!similar("short", "a completely unrelated and much longer piece of text"));
        assert!(!similar("", ""));
        assert!(!similar("a", ""));
    }
}
