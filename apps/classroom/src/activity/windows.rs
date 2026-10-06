//! Windows implementation: `EnumWindows` for the open-window list, UI
//! Automation for browser tab titles and the active tab's address, and the
//! Chromium `History` database for recent downloads.
#![allow(unsafe_code)]
use super::{history::History, BrowserWindow, DownloadEntry, Report, TabEntry, WindowEntry, MAX_DOWNLOADS, MAX_TABS, MAX_WINDOWS};
use base64::{engine::general_purpose::STANDARD, Engine};
use std::{
    collections::HashMap,
    mem::ManuallyDrop,
    path::{Path, PathBuf},
    sync::mpsc,
    time::Duration,
};
use windows::{
    core::BSTR,
    Win32::{
        Foundation::{CloseHandle, HWND, LPARAM, MAX_PATH},
        System::{
            Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED},
            Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION},
            Variant::{VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_BSTR, VT_I4},
        },
        UI::{
            Accessibility::{
                CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationSelectionItemPattern,
                IUIAutomationValuePattern, TreeScope_Descendants, UIA_ClassNamePropertyId,
                UIA_ControlTypePropertyId, UIA_SelectionItemPatternId, UIA_TabControlTypeId,
                UIA_TabItemControlTypeId, UIA_ValuePatternId,
            },
            WindowsAndMessaging::{
                EnumWindows, GetWindow, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW,
                GetWindowThreadProcessId, IsWindowVisible, GWL_EXSTYLE, GW_OWNER, WS_EX_TOOLWINDOW,
            },
        },
    },
};

/// `(app process name, title, pid)`.
struct Window {
    app: String,
    title: String,
    pid: u32,
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> windows::core::BOOL {
    // SAFETY: `lparam` was set to a live `&mut Vec<HWND>` by `list_window_handles`
    // for the single synchronous `EnumWindows` call below.
    let out = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
    if out.len() < MAX_WINDOWS * 4 {
        out.push(hwnd);
    }
    windows::core::BOOL(1)
}
fn list_window_handles() -> Vec<HWND> {
    let mut handles = Vec::new();
    // SAFETY: `enum_proc` only ever reads `lparam` as the `Vec<HWND>` we pass
    // here, for the duration of this synchronous call.
    unsafe {
        let _ = EnumWindows(Some(enum_proc), LPARAM(std::ptr::addr_of_mut!(handles) as isize));
    }
    handles
}
fn window_text(hwnd: HWND) -> String {
    // SAFETY: `hwnd` is a window handle obtained from `EnumWindows` moments ago.
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        let mut buffer = vec![0u16; usize::try_from(len).unwrap_or(0) + 1];
        let copied = GetWindowTextW(hwnd, &mut buffer);
        buffer.truncate(usize::try_from(copied.max(0)).unwrap_or(0));
        String::from_utf16_lossy(&buffer)
    }
}
fn process_name(pid: u32, cache: &mut HashMap<u32, String>) -> String {
    if let Some(name) = cache.get(&pid) {
        return name.clone();
    }
    // SAFETY: a short-lived, limited-access handle; closed right after use.
    let name = unsafe {
        OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
            .ok()
            .and_then(|handle| {
                let mut buffer = vec![0u16; MAX_PATH as usize];
                let mut len = u32::try_from(buffer.len()).unwrap_or(0);
                let path = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, windows::core::PWSTR(buffer.as_mut_ptr()), &mut len)
                    .ok()
                    .map(|()| String::from_utf16_lossy(&buffer[..usize::try_from(len).unwrap_or(0)]));
                let _ = CloseHandle(handle);
                path
            })
            .and_then(|path| Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_default()
    };
    cache.insert(pid, name.clone());
    name
}

/// Windows shell infrastructure that hosts/frames other visible windows
/// rather than being an app a Student opened: reporting it is either a
/// meaningless duplicate (`ApplicationFrameHost.exe` fronts a UWP app that is
/// already reported under its own process, e.g. `SystemSettings.exe`) or
/// never something a Teacher cares about on its own.
const EXCLUDED: [&str; 6] = [
    "applicationframehost.exe",
    "textinputhost.exe",
    "shellexperiencehost.exe",
    "startmenuexperiencehost.exe",
    "searchhost.exe",
    "lockapp.exe",
];

const BROWSERS: [&str; 5] = ["chrome.exe", "msedge.exe", "brave.exe", "firefox.exe", "opera.exe"];
/// Chromium-based browsers only: Firefox and others do not expose tabs the
/// same way through accessibility.
const CHROMIUM_BROWSERS: [&str; 3] = ["chrome.exe", "msedge.exe", "brave.exe"];

fn list_windows() -> (Vec<Window>, bool) {
    let own_pid = std::process::id();
    let mut cache = HashMap::new();
    let mut windows = Vec::new();
    for hwnd in list_window_handles() {
        // SAFETY: `hwnd` came from `EnumWindows`; these are read-only queries.
        let (visible, owned, ex_style) = unsafe {
            (
                IsWindowVisible(hwnd).as_bool(),
                GetWindow(hwnd, GW_OWNER).unwrap_or_default().0 as usize != 0,
                GetWindowLongPtrW(hwnd, GWL_EXSTYLE),
            )
        };
        if !visible || owned || ex_style & isize::try_from(WS_EX_TOOLWINDOW.0).unwrap_or(0) != 0 {
            continue;
        }
        let title = window_text(hwnd);
        if title.trim().is_empty() {
            continue;
        }
        let mut pid = 0u32;
        // SAFETY: `hwnd` is live; `pid` is a valid out-parameter for this call.
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
        }
        if pid == own_pid || pid == 0 {
            continue; // never report the Classroom Student's own windows
        }
        let app = process_name(pid, &mut cache);
        if app.is_empty() || EXCLUDED.contains(&app.to_ascii_lowercase().as_str()) {
            continue;
        }
        windows.push(Window { app, title, pid });
    }
    let truncated = windows.len() > MAX_WINDOWS;
    windows.truncate(MAX_WINDOWS);
    (windows, truncated)
}

// ------------------------------------------------------------- UI Automation

fn variant_i32(value: i32) -> VARIANT {
    VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                vt: VT_I4,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: VARIANT_0_0_0 { lVal: value },
            }),
        },
    }
}
/// Runs `with` on a `VT_BSTR` variant holding `text`. The variant's
/// `bstrVal` is a non-owning bit-copy of a `BSTR` kept alive in `bstr` below
/// for the whole call; `bstr` frees the string normally on return, and the
/// copy inside the variant (wrapped in `ManuallyDrop`) is never separately
/// dropped, so the allocation is freed exactly once.
fn with_bstr_variant<R>(text: &str, with: impl FnOnce(&VARIANT) -> R) -> R {
    let bstr = BSTR::from(text);
    let variant = VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                vt: VT_BSTR,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                // SAFETY: bitwise copy of `bstr`'s pointer; see the doc comment.
                Anonymous: VARIANT_0_0_0 { bstrVal: ManuallyDrop::new(unsafe { std::mem::transmute_copy(&bstr) }) },
            }),
        },
    };
    with(&variant)
}

/// Chromium puts the active tab's title in the window title, and for more
/// than one tab appends `" and N more pages"` before the profile/browser
/// name (observed directly from a real multi-tab Edge window; used here only
/// as a sanity check on the UIA scan below, never as the data source).
fn expected_tab_count(title: &str) -> Option<usize> {
    let (before, after) = title.split_once(" and ")?;
    let _ = before;
    let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    if after[digits.len()..].starts_with(" more page") {
        digits.parse::<usize>().ok().map(|n| n + 1)
    } else {
        None
    }
}

/// Reads the tabs of one Chromium browser window. Runs entirely on a fresh,
/// short-lived thread with its own COM apartment, bounded by `timeout`: a
/// busy/unresponsive browser never blocks the rest of the report.
fn browser_tabs(hwnd_value: isize, window_title: &str, timeout: Duration) -> Option<Vec<TabEntry>> {
    let (tx, rx) = mpsc::channel();
    let title = window_title.to_owned();
    std::thread::spawn(move || {
        let _ = tx.send(read_tabs(HWND(hwnd_value as *mut core::ffi::c_void), &title));
    });
    rx.recv_timeout(timeout).ok().flatten()
}
/// Finds the actual tab strip first (a `Tab`-typed control) and only looks
/// for `TabItem`s inside it — searching the whole window for `TabItem`
/// elements also picks up unrelated UI that happens to reuse the same
/// control type (e.g. a browser's news-feed sidebar), which was observed
/// producing dozens of bogus "tabs". [`expected_tab_count`] is a second,
/// independent check: a scan that disagrees with it is discarded rather than
/// shown, since wrong data is worse than none here.
fn read_tabs(hwnd: HWND, window_title: &str) -> Option<Vec<TabEntry>> {
    // SAFETY: a fresh apartment for this short-lived thread, released below
    // regardless of the outcome.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    let result = (|| -> windows::core::Result<Vec<TabEntry>> {
        // SAFETY: standard COM activation of the system UI Automation engine.
        let automation: IUIAutomation = unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }?;
        // SAFETY: `hwnd` names a currently-open top-level window.
        let root: IUIAutomationElement = unsafe { automation.ElementFromHandle(hwnd) }?;
        let strip_condition = variant_i32(UIA_TabControlTypeId.0);
        // SAFETY: `automation`/`root` are live COM objects; `strip_condition` is
        // a valid property-condition value for the duration of this call.
        let strip = unsafe {
            let condition = automation.CreatePropertyCondition(UIA_ControlTypePropertyId, &strip_condition)?;
            root.FindFirst(TreeScope_Descendants, &condition)?
        };
        let tab_condition = variant_i32(UIA_TabItemControlTypeId.0);
        // SAFETY: `strip` is the live element found above; `tab_condition` is a
        // valid property-condition value for the duration of this call.
        let found = unsafe {
            let condition = automation.CreatePropertyCondition(UIA_ControlTypePropertyId, &tab_condition)?;
            strip.FindAll(TreeScope_Descendants, &condition)?
        };
        // SAFETY: `found` is the live array returned above.
        let count = unsafe { found.Length() }.unwrap_or(0).max(0);
        let mut tabs = Vec::new();
        for i in 0..count.min(i32::try_from(MAX_TABS).unwrap_or(i32::MAX)) {
            // SAFETY: `i` is within the array's reported length.
            let Ok(tab) = (unsafe { found.GetElement(i) }) else {
                continue;
            };
            // SAFETY: reading cached/current properties of a live element.
            let name = unsafe { tab.CurrentName() }.map(|n| n.to_string()).unwrap_or_default();
            let active = unsafe { selection_active(&tab) };
            tabs.push(TabEntry { title: name, active, ..Default::default() });
        }
        if let Some(expected) = expected_tab_count(window_title) {
            check_tab_count(tabs.len(), expected)?;
        }
        // The active tab's address, read from the omnibox (Chromium's stable
        // accessible class name for it); background tabs have no such field.
        if let Some(active) = tabs.iter_mut().find(|t| t.active) {
            active.url = with_bstr_variant("OmniboxViewViews", |value| {
                // SAFETY: `root` is the same live element as above.
                unsafe {
                    let condition = automation.CreatePropertyCondition(UIA_ClassNamePropertyId, value).ok()?;
                    let edit = root.FindFirst(TreeScope_Descendants, &condition).ok()?;
                    let pattern: IUIAutomationValuePattern = edit.GetCurrentPatternAs(UIA_ValuePatternId).ok()?;
                    Some(pattern.CurrentValue().ok()?.to_string()).filter(|s| !s.is_empty())
                }
            });
        }
        Ok(tabs)
    })();
    // SAFETY: matches the `CoInitializeEx` above; always released once.
    unsafe {
        CoUninitialize();
    }
    result.ok()
}
/// `Err` (handled as "scan failed, fall back") when the UIA scan's tab count
/// disagrees with the window title's own count.
fn check_tab_count(found: usize, expected: usize) -> windows::core::Result<()> {
    if found == expected {
        Ok(())
    } else {
        Err(windows::core::Error::from(windows::Win32::Foundation::E_FAIL))
    }
}
/// `SelectionItemPattern.CurrentIsSelected`, or `false` if the tab does not
/// expose selection state.
unsafe fn selection_active(tab: &IUIAutomationElement) -> bool {
    // SAFETY: `tab` is a live element from the same call tree as the caller.
    unsafe {
        tab.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(UIA_SelectionItemPatternId)
            .and_then(|pattern| pattern.CurrentIsSelected())
            .map(|v| v.as_bool())
            .unwrap_or(false)
    }
}

// ----------------------------------------------------------------- Downloads

/// Chrome epoch (1601-01-01) to Unix epoch, in microseconds.
const CHROME_EPOCH_OFFSET_US: i64 = 11_644_473_600_000_000;

fn chrome_time_to_unix_ms(value: i64) -> i64 {
    (value - CHROME_EPOCH_OFFSET_US) / 1000
}

/// `User Data` folders of the browsers we can read downloads from.
fn profile_roots() -> Vec<(&'static str, PathBuf)> {
    let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) else {
        return Vec::new();
    };
    [
        ("Chrome", local.join("Google").join("Chrome").join("User Data")),
        ("Edge", local.join("Microsoft").join("Edge").join("User Data")),
    ]
    .into_iter()
    .filter(|(_, path)| path.is_dir())
    .collect()
}
fn profiles_in(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut profiles: Vec<PathBuf> = entries
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("History").is_file())
        .collect();
    profiles.sort();
    profiles.truncate(5);
    profiles
}
fn read_downloads(browser: &str, profile: &Path, notes: &mut Vec<String>) -> Vec<DownloadEntry> {
    let staged = match stage_history_copy(profile) {
        Ok(path) => path,
        Err(error) => {
            notes.push(format!("{browser} ({}): {error:#}", profile.display()));
            return Vec::new();
        }
    };
    let result = (|| -> rusqlite::Result<Vec<DownloadEntry>> {
        let conn = rusqlite::Connection::open_with_flags(&staged.path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let mut statement = conn.prepare(
            "SELECT d.target_path, d.start_time, d.received_bytes, \
             (SELECT url FROM downloads_url_chains c WHERE c.id = d.id ORDER BY c.chain_index DESC LIMIT 1) \
             FROM downloads d ORDER BY d.start_time DESC LIMIT 30",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (path, start_time, bytes, url) = row?;
            let name = Path::new(&path)
                .file_name()
                .map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned());
            if name.is_empty() {
                continue;
            }
            out.push(DownloadEntry {
                name,
                url: url.unwrap_or_default(),
                browser: browser.to_owned(),
                unix_ms: chrome_time_to_unix_ms(start_time),
                bytes,
            });
        }
        Ok(out)
    })();
    match result {
        Ok(rows) => rows,
        Err(error) => {
            notes.push(format!("{browser} ({}): {error}", profile.display()));
            Vec::new()
        }
    }
}
struct Staged {
    path: PathBuf,
}
impl Drop for Staged {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.path.parent().unwrap_or(&self.path));
    }
}
/// Copies `name` (and its `-wal`/`-journal` companions, if present) out of a
/// profile folder so a running browser's file lock never blocks or corrupts
/// the read.
fn stage_copy(profile: &Path, name: &str) -> anyhow::Result<Staged> {
    let folder = std::env::temp_dir().join(format!("classroom-activity-{}", media_core::unix_us()));
    std::fs::create_dir_all(&folder)?;
    std::fs::copy(profile.join(name), folder.join(name))?;
    for suffix in ["-wal", "-journal"] {
        let _ = std::fs::copy(profile.join(format!("{name}{suffix}")), folder.join(format!("{name}{suffix}")));
    }
    Ok(Staged { path: folder.join(name) })
}
fn stage_history_copy(profile: &Path) -> anyhow::Result<Staged> {
    stage_copy(profile, "History")
}
// ------------------------------------------------------------------ Favicons

/// `scheme://host[:port]` prefix of a URL, used to match a page to its
/// favicon regardless of the exact path Chromium recorded the mapping under.
fn origin_of(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    let end = rest.find('/').unwrap_or(rest.len());
    Some(format!("{scheme}://{}", &rest[..end]))
}
fn read_favicon(profile: &Path, origin: &str) -> Option<Vec<u8>> {
    let staged = stage_copy(profile, "Favicons").ok()?;
    let conn = rusqlite::Connection::open_with_flags(&staged.path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    conn.query_row(
        "SELECT fb.image_data FROM icon_mapping im          JOIN favicon_bitmaps fb ON fb.icon_id = im.icon_id          WHERE im.page_url LIKE ?1 ORDER BY fb.width DESC LIMIT 1",
        [format!("{origin}%")],
        |row| row.get::<_, Vec<u8>>(0),
    )
    .ok()
}
/// Best-effort base64 PNG for `url`'s site icon, tried across every profile
/// of the matching Chromium browser; `None` if it cannot be found quickly.
fn favicon_for(app: &str, url: &str) -> Option<String> {
    let origin = origin_of(url)?;
    let browser = if app.eq_ignore_ascii_case("chrome.exe") {
        "Chrome"
    } else if app.eq_ignore_ascii_case("msedge.exe") {
        "Edge"
    } else {
        "Brave"
    };
    for (name, root) in profile_roots() {
        if name != browser {
            continue;
        }
        for profile in profiles_in(&root) {
            if let Some(bytes) = read_favicon(&profile, &origin) {
                if !bytes.is_empty() && bytes.len() <= 64 * 1024 {
                    return Some(STANDARD.encode(bytes));
                }
            }
        }
    }
    None
}

fn downloads() -> (Vec<DownloadEntry>, bool, Vec<String>) {
    let mut all = Vec::new();
    let mut notes = Vec::new();
    for (browser, root) in profile_roots() {
        for profile in profiles_in(&root) {
            all.extend(read_downloads(browser, &profile, &mut notes));
        }
    }
    all.sort_by(|a, b| b.unix_ms.cmp(&a.unix_ms));
    let truncated = all.len() > MAX_DOWNLOADS;
    all.truncate(MAX_DOWNLOADS);
    (all, truncated, notes)
}

pub fn build(history: &History, include_downloads: bool) -> Report {
    let (listed, windows_truncated) = list_windows();
    let mut browsers = Vec::new();
    for window in &listed {
        let key = window.app.to_ascii_lowercase();
        if !BROWSERS.contains(&key.as_str()) {
            continue;
        }
        if !CHROMIUM_BROWSERS.contains(&key.as_str()) {
            browsers.push(BrowserWindow {
                app: window.app.clone(),
                tabs: vec![TabEntry { title: window.title.clone(), active: true, ..Default::default() }],
                unavailable: false,
            });
            continue;
        }
        // Re-enumerate this HWND specifically (we only kept the handle value
        // transiently in `list_window_handles`); cheap, so just re-scan.
        let hwnd = list_window_handles().into_iter().find(|h| {
            window_text(*h) == window.title && {
                let mut pid = 0u32;
                // SAFETY: `h` was just returned by `EnumWindows`.
                unsafe {
                    GetWindowThreadProcessId(*h, Some(&mut pid));
                }
                pid == window.pid
            }
        });
        match hwnd.and_then(|h| browser_tabs(h.0 as isize, &window.title, Duration::from_millis(1500))) {
            Some(mut tabs) if !tabs.is_empty() => {
                if let Some(active) = tabs.iter_mut().find(|t| t.active) {
                    if let Some(url) = active.url.clone() {
                        active.favicon_png_base64 = favicon_for(&window.app, &url);
                    }
                }
                browsers.push(BrowserWindow { app: window.app.clone(), tabs, unavailable: false });
            }
            _ => browsers.push(BrowserWindow {
                app: window.app.clone(),
                tabs: vec![TabEntry { title: window.title.clone(), active: true, ..Default::default() }],
                unavailable: true,
            }),
        }
    }
    let mut notes = Vec::new();
    if include_downloads {
        let (fresh, truncated, download_notes) = downloads();
        notes.extend(download_notes);
        if truncated {
            notes.push(format!("Only the {MAX_DOWNLOADS} most recent downloads are shown"));
        }
        history.set_downloads(fresh);
    }
    let downloads = history.downloads();

    let mut apps_seen: Vec<(String, String, Option<String>, Option<String>)> = Vec::new();
    for window in &listed {
        if !apps_seen.iter().any(|(app, ..)| *app == window.app) {
            apps_seen.push((window.app.clone(), window.title.clone(), None, None));
        }
    }
    let tabs_seen: Vec<(String, String, Option<String>, Option<String>)> = browsers
        .iter()
        .flat_map(|b| b.tabs.iter())
        .filter(|t| !t.title.is_empty())
        .map(|t| (t.title.clone(), t.title.clone(), t.url.clone(), t.favicon_png_base64.clone()))
        .collect();
    let (apps, tabs) = history.record(&apps_seen, &tabs_seen);

    Report {
        supported: true,
        monitoring_enabled: true,
        windows: listed.into_iter().map(|w| WindowEntry { app: w.app, title: w.title }).collect(),
        windows_truncated,
        browsers,
        downloads,
        apps,
        tabs,
        notes,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tab_count_parses_from_the_window_title_chromium_actually_sends() {
        assert_eq!(expected_tab_count("Wikipedia and 2 more pages - Profile 1 - Microsoft\u{200b} Edge"), Some(3));
        assert_eq!(expected_tab_count("Example Domain - Profile 1 - Microsoft\u{200b} Edge"), None);
        assert_eq!(expected_tab_count("GitHub - octocat/Hello-World - Google Chrome"), None);
    }

    #[test]
    fn chrome_timestamps_convert_to_unix_milliseconds() {
        // 2024-01-01T00:00:00Z in Chrome's 1601-epoch microseconds.
        assert_eq!(chrome_time_to_unix_ms(13_348_944_000_000_000), 1_704_067_200_000);
        assert_eq!(chrome_time_to_unix_ms(CHROME_EPOCH_OFFSET_US), 0);
    }
}
