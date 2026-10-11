//! Class-wide control of installed Windows applications.
//!
//! * **Inventory**: the programs Windows itself lists under Settings → Apps /
//!   Programs and Features (the three `Uninstall` registry views), each with a
//!   stable id derived from its normalized name so the same program on many
//!   computers — even at different versions — collapses to one entry on the
//!   Teacher. Native icons are extracted on request, only for the ids the
//!   Teacher still lacks, and only from the one computer it chose, so a class
//!   of 40 never uploads 40 copies of the same icon.
//! * **Enforcement**: Image File Execution Options. For every executable of a
//!   blocked program a `Debugger` value is written under
//!   `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution
//!   Options\<exe>` pointing at a copy of this very program started with
//!   `--app-blocked`. Windows then runs that stub *instead of* the blocked
//!   program, which tells the Student it is blocked and exits. This is a
//!   documented OS facility, works before the blocked program executes any
//!   code, and — like the VS Code policy — is plain registry state that
//!   survives a Student restart with no lease bookkeeping.
//!
//! The Teacher sends program *ids*, never paths: each Student resolves them
//! against its own inventory, so a program installed in a different folder on
//! one computer is still covered, and a program absent from a computer costs
//! nothing there.
#![cfg_attr(windows, allow(unsafe_code))]
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MODE_BLOCK: &str = "block";
pub const MODE_ALLOW_ONLY: &str = "allow_only";
const MAX_IDS: usize = 400;
const ID_LEN: usize = 12;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AppPolicy {
    /// `false`: remove every restriction this applies.
    pub enabled: bool,
    /// [`MODE_BLOCK`]: the listed programs cannot start.
    /// [`MODE_ALLOW_ONLY`]: every *other* inventoried program cannot start.
    pub mode: String,
    pub ids: Vec<String>,
}
impl AppPolicy {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.mode == MODE_BLOCK || self.mode == MODE_ALLOW_ONLY || !self.enabled,
            "Unknown application control mode"
        );
        anyhow::ensure!(self.ids.len() <= MAX_IDS, "At most {MAX_IDS} applications");
        anyhow::ensure!(
            self.ids
                .iter()
                .all(|id| id.len() == ID_LEN && id.bytes().all(|b| b.is_ascii_hexdigit())),
            "Invalid application id"
        );
        Ok(())
    }
}

/// Fingerprint of the policy a Student enforces (empty when none). The
/// Student reports it on connect and after applying, so the Teacher can tell
/// at a glance whether a computer that just joined or restarted has the
/// class rule — and send it if not.
pub fn policy_hash(policy: &AppPolicy) -> String {
    if !policy.enabled {
        return String::new();
    }
    let mut ids = policy.ids.clone();
    ids.sort();
    let digest = Sha256::digest(format!("{}|{}", policy.mode, ids.join(",")).as_bytes());
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct AppInfo {
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub version: String,
    /// How many executables could be found to block; `0` means this program
    /// cannot be controlled on this computer.
    pub executables: usize,
    /// Native icon (PNG, base64), present only when the Teacher asked for it.
    pub icon_png_base64: Option<String>,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct AppReport {
    pub supported: bool,
    pub apps: Vec<AppInfo>,
    pub notes: Vec<String>,
}

/// Request body of an inventory scan.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ListRequest {
    /// Ids whose icons this computer should attach.
    pub icons_for: Vec<String>,
}

/// Name with architecture/version noise removed and lowercased, so
/// `Python 3.12.1 (64-bit)` and `Python 3.11.9 (64-bit)` are one program.
pub fn normalize_name(name: &str) -> String {
    let mut plain = String::with_capacity(name.len());
    let mut depth = 0usize;
    for c in name.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => plain.push(c),
            _ => {}
        }
    }
    let mut words: Vec<&str> = plain.split_whitespace().collect();
    let noise = |w: &str| {
        let w = w.to_ascii_lowercase();
        let w = w.trim_start_matches('v');
        matches!(w, "x64" | "x86" | "64-bit" | "32-bit" | "64bit" | "32bit")
            || (!w.is_empty() && w.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-'))
    };
    while words.len() > 1 && words.last().is_some_and(|w| noise(w)) {
        words.pop();
    }
    words.join(" ").to_lowercase()
}
/// Stable 12-hex-digit id of a program name.
pub fn app_id(name: &str) -> String {
    let digest = Sha256::digest(normalize_name(name).as_bytes());
    digest[..ID_LEN / 2].iter().map(|b| format!("{b:02x}")).collect()
}

/// Executable names that must never be blocked, whatever the Teacher picks:
/// the Classroom programs themselves and core shells.
pub fn is_protected(file_name: &str) -> bool {
    matches!(
        file_name.to_ascii_lowercase().as_str(),
        "classroomstudent.exe"
            | "classroomguardservice.exe"
            | "classroomblocked.exe"
            | "classroom.exe"
            | "explorer.exe"
            | "cmd.exe"
            | "powershell.exe"
            | "pwsh.exe"
            | "regedit.exe"
            | "msedgewebview2.exe"
            | "systemsettings.exe"
            | "applicationframehost.exe"
            | "runtimebroker.exe"
            | "shellexperiencehost.exe"
            | "startmenuexperiencehost.exe"
            | "searchhost.exe"
            | "textinputhost.exe"
            | "lockapp.exe"
            | "backgroundtaskhost.exe"
            | "dllhost.exe"
            | "sihost.exe"
            | "ctfmon.exe"
            | "securityhealthsystray.exe"
            | "svchost.exe"
            | "services.exe"
            | "winlogon.exe"
            | "csrss.exe"
            | "lsass.exe"
    )
}

#[cfg(windows)]
mod windows_impl {
    use super::{
        app_id, is_protected, normalize_name, policy_hash, AppInfo, AppPolicy, AppReport, ListRequest,
        MODE_ALLOW_ONLY,
    };
    use anyhow::{Context, Result};
    use serde_json::{json, Value};
    use std::{
        collections::{BTreeMap, HashSet},
        os::windows::process::CommandExt,
        path::{Path, PathBuf},
        process::Command,
        sync::{Arc, Mutex},
        time::Duration,
    };
    use windows::{
        core::{w, PCWSTR, PWSTR},
        Win32::{
            Foundation::ERROR_SUCCESS,
            System::Registry::{
                RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER,
                HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY, REG_DWORD, REG_EXPAND_SZ,
                REG_SAM_FLAGS, REG_SZ, REG_VALUE_TYPE,
            },
            UI::Shell::SHLoadIndirectString,
            UI::WindowsAndMessaging::{MessageBoxW, MB_ICONSTOP, MB_OK, MB_SETFOREGROUND, MB_TOPMOST},
        },
    };

    const NO_WINDOW: u32 = 0x0800_0000;
    const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
    const IFEO: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options";
    const STATE_KEY: &str = r"SOFTWARE\Classroom\AppBlock";
    const STUB_NAME: &str = "ClassroomBlocked.exe";
    const MAX_APPS: usize = 600;
    const MAX_EXES_PER_APP: usize = 6;

    // ------------------------------------------------------------ registry
    struct Key(HKEY);
    impl Drop for Key {
        fn drop(&mut self) {
            // SAFETY: the handle was opened by `Key::open` and is closed once.
            unsafe {
                let _ = RegCloseKey(self.0);
            }
        }
    }
    impl Key {
        fn open(root: HKEY, path: &str, view: REG_SAM_FLAGS) -> Option<Self> {
            let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            let mut handle = HKEY::default();
            // SAFETY: `wide` is NUL-terminated and outlives the call.
            let status = unsafe { RegOpenKeyExW(root, PCWSTR(wide.as_ptr()), None, KEY_READ | view, &mut handle) };
            (status == ERROR_SUCCESS).then_some(Self(handle))
        }
        fn child(&self, name: &str) -> Option<Self> {
            let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            let mut handle = HKEY::default();
            // SAFETY: as in `open`.
            let status = unsafe { RegOpenKeyExW(self.0, PCWSTR(wide.as_ptr()), None, KEY_READ, &mut handle) };
            (status == ERROR_SUCCESS).then_some(Self(handle))
        }
        fn subkeys(&self) -> Vec<String> {
            let mut names = Vec::new();
            for index in 0..u32::try_from(MAX_APPS * 3).unwrap_or(u32::MAX) {
                let mut buffer = [0u16; 256];
                let mut len = 256u32;
                // SAFETY: `buffer`/`len` describe a valid 256-unit buffer.
                let status = unsafe {
                    RegEnumKeyExW(self.0, index, Some(PWSTR(buffer.as_mut_ptr())), &mut len, None, None, None, None)
                };
                if status != ERROR_SUCCESS {
                    break;
                }
                names.push(String::from_utf16_lossy(&buffer[..len as usize]));
            }
            names
        }
        fn value(&self, name: &str) -> Option<(REG_VALUE_TYPE, Vec<u8>)> {
            let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            let mut kind = REG_VALUE_TYPE::default();
            let mut size = 0u32;
            // SAFETY: first call only asks for the size.
            let status = unsafe {
                RegQueryValueExW(self.0, PCWSTR(wide.as_ptr()), None, Some(&mut kind), None, Some(&mut size))
            };
            if status != ERROR_SUCCESS || size == 0 || size > 64 * 1024 {
                return None;
            }
            let mut data = vec![0u8; size as usize];
            // SAFETY: `data` holds `size` bytes as reported above.
            let status = unsafe {
                RegQueryValueExW(
                    self.0,
                    PCWSTR(wide.as_ptr()),
                    None,
                    Some(&mut kind),
                    Some(data.as_mut_ptr()),
                    Some(&mut size),
                )
            };
            (status == ERROR_SUCCESS).then(|| {
                data.truncate(size as usize);
                (kind, data)
            })
        }
        fn string(&self, name: &str) -> Option<String> {
            let (kind, data) = self.value(name)?;
            if kind != REG_SZ && kind != REG_EXPAND_SZ {
                return None;
            }
            let units: Vec<u16> = data.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            let text = String::from_utf16_lossy(&units);
            let text = text.trim_end_matches('\0').trim().to_owned();
            if text.is_empty() {
                None
            } else if kind == REG_EXPAND_SZ {
                Some(expand_env(&text))
            } else {
                Some(text)
            }
        }
        fn dword(&self, name: &str) -> Option<u32> {
            let (kind, data) = self.value(name)?;
            (kind == REG_DWORD && data.len() >= 4).then(|| u32::from_le_bytes([data[0], data[1], data[2], data[3]]))
        }
    }
    /// `%VAR%` expansion for the few variables installers use.
    fn expand_env(text: &str) -> String {
        let mut out = String::new();
        let mut rest = text;
        while let Some(start) = rest.find('%') {
            out.push_str(&rest[..start]);
            let after = &rest[start + 1..];
            match after.find('%') {
                Some(end) => {
                    match std::env::var(&after[..end]) {
                        Ok(value) => out.push_str(&value),
                        Err(_) => {
                            out.push('%');
                            out.push_str(&after[..=end]);
                        }
                    }
                    rest = &after[end + 1..];
                }
                None => {
                    out.push('%');
                    rest = after;
                }
            }
        }
        out.push_str(rest);
        out
    }

    // ----------------------------------------------------------- inventory
    struct Found {
        info: AppInfo,
        exes: Vec<PathBuf>,
        /// An executable (or `.ico`) whose shell icon represents the program.
        icon_source: Option<PathBuf>,
        /// A ready-made PNG (Store apps ship their own logos).
        icon_file: Option<PathBuf>,
        /// Store (MSIX/AppX) app: blocked by the watcher, not by IFEO.
        packaged: bool,
    }

    fn lower_name(path: &Path) -> String {
        path.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
    }
    fn is_installer_name(name: &str) -> bool {
        ["unins", "uninst", "setup", "install", "update", "crashpad", "crashreport", "helper", "elevat", "repair"]
            .iter()
            .any(|marker| name.contains(marker))
    }
    /// The executable named by a `DisplayIcon` value (`"C:\x\y.exe",0`).
    fn exe_from_display_icon(value: &str) -> Option<PathBuf> {
        let first = value.split(',').next()?.trim().trim_matches('"').to_owned();
        let path = PathBuf::from(first);
        (path.is_file()).then_some(path)
    }
    fn alnum(text: &str) -> String {
        text.chars().filter(char::is_ascii_alphanumeric).collect::<String>().to_ascii_lowercase()
    }
    fn is_system_path(path: &Path) -> bool {
        let lowered = path.to_string_lossy().to_ascii_lowercase();
        let windir = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()).to_ascii_lowercase();
        lowered.starts_with(&windir)
            || lowered.contains(r"\microsoft\edgewebview\")
            || lowered.contains(r"\microsoft\edgeupdate\")
            || lowered.contains(r"\windowsapps\")
    }
    /// Executables worth blocking for one program: the one its icon points
    /// at, plus top-level executables of its install folder whose name
    /// matches the program (so unrelated tools sharing a folder, or generic
    /// names like `launcher.exe`, are not swept in).
    fn discover(name: &str, display_icon: Option<&str>, location: Option<&str>) -> (Vec<PathBuf>, Option<PathBuf>) {
        let mut exes: Vec<PathBuf> = Vec::new();
        let mut icon_source = None;
        let mut folder = location.map(PathBuf::from).filter(|p| p.is_dir());
        if let Some(icon_exe) = display_icon.and_then(exe_from_display_icon) {
            let file = lower_name(&icon_exe);
            if file.ends_with(".exe") && !is_installer_name(&file) {
                if folder.is_none() {
                    folder = icon_exe.parent().map(Path::to_path_buf);
                }
                icon_source = Some(icon_exe.clone());
                exes.push(icon_exe);
            } else if !file.ends_with(".exe") {
                icon_source = Some(icon_exe); // `.ico` and similar: only for display
            }
        }
        if let Some(dir) = folder.filter(|d| !is_system_path(d)) {
            let wanted = alnum(&normalize_name(name));
            let mut candidates: Vec<PathBuf> = std::fs::read_dir(&dir)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    let file = lower_name(p);
                    file.ends_with(".exe") && !is_installer_name(&file)
                })
                .take(80)
                .collect();
            candidates.sort();
            let only_one = candidates.len() == 1;
            for path in candidates {
                let stem = alnum(path.file_stem().map(|s| s.to_string_lossy()).unwrap_or_default().as_ref());
                let related = stem.len() >= 3 && (wanted.contains(&stem) || stem.contains(&wanted) || only_one);
                if related && !exes.iter().any(|e| lower_name(e) == lower_name(&path)) {
                    exes.push(path);
                }
            }
        }
        exes.retain(|p| !is_system_path(p));
        exes.truncate(MAX_EXES_PER_APP);
        if icon_source.is_none() {
            icon_source = exes.first().cloned();
        }
        (exes, icon_source)
    }
    fn skip_entry(key: &Key, name: &str) -> bool {
        if key.dword("SystemComponent") == Some(1) || key.string("ParentKeyName").is_some() {
            return true;
        }
        if key.string("ReleaseType").is_some_and(|r| matches!(r.as_str(), "Update" | "Hotfix" | "Security Update")) {
            return true;
        }
        let lowered = name.to_ascii_lowercase();
        lowered.starts_with("update for ")
            || lowered.starts_with("security update")
            || lowered.starts_with("hotfix")
            || lowered.contains("redistributable")
            || lowered.starts_with("microsoft .net")
            || lowered.starts_with("microsoft edge webview2")
            || lowered.starts_with("microsoft edge update")
            || lowered.starts_with("microsoft update health")
            || lowered.starts_with("windows software development kit")
    }
    fn scan() -> Vec<Found> {
        let mut by_id: BTreeMap<String, Found> = BTreeMap::new();
        let sources: [(HKEY, REG_SAM_FLAGS); 3] = [
            (HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY),
            (HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY),
            (HKEY_CURRENT_USER, REG_SAM_FLAGS(0)),
        ];
        for (root, view) in sources {
            let Some(parent) = Key::open(root, UNINSTALL, view) else {
                continue;
            };
            for sub in parent.subkeys() {
                let Some(key) = parent.child(&sub) else {
                    continue;
                };
                let Some(name) = key.string("DisplayName") else {
                    continue;
                };
                if skip_entry(&key, &name) || by_id.len() >= MAX_APPS {
                    continue;
                }
                let id = app_id(&name);
                let display_icon = key.string("DisplayIcon");
                let location = key.string("InstallLocation");
                let (exes, icon_source) = discover(&name, display_icon.as_deref(), location.as_deref());
                let entry = by_id.entry(id.clone()).or_insert_with(|| Found {
                    info: AppInfo {
                        id,
                        name: name.chars().take(120).collect(),
                        publisher: key.string("Publisher").unwrap_or_default().chars().take(80).collect(),
                        version: key.string("DisplayVersion").unwrap_or_default().chars().take(40).collect(),
                        ..AppInfo::default()
                    },
                    exes: Vec::new(),
                    icon_source: None,
                    icon_file: None,
                    packaged: false,
                });
                for exe in exes {
                    if !entry.exes.iter().any(|e| lower_name(e) == lower_name(&exe)) {
                        entry.exes.push(exe);
                    }
                }
                if entry.icon_source.is_none() {
                    entry.icon_source = icon_source;
                }
                entry.info.executables = entry.exes.len();
            }
        }
        for pkg in packaged_apps() {
            if by_id.len() >= MAX_APPS && !by_id.contains_key(&pkg.info.id) {
                continue;
            }
            match by_id.get_mut(&pkg.info.id) {
                Some(existing) => {
                    for exe in pkg.exes {
                        if !existing.exes.iter().any(|e| lower_name(e) == lower_name(&exe)) {
                            existing.exes.push(exe);
                        }
                    }
                    existing.icon_file = existing.icon_file.take().or(pkg.icon_file);
                    existing.info.executables = existing.exes.len();
                }
                None => {
                    by_id.insert(pkg.info.id.clone(), pkg);
                }
            }
        }
        // Windows tools Students use to end the Classroom Student itself.
        // Never blocked unless the Teacher ticks them.
        let system = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let task_manager = PathBuf::from(system).join("System32").join("Taskmgr.exe");
        if task_manager.is_file() {
            let name = "Task Manager";
            let id = app_id(name);
            by_id.entry(id.clone()).or_insert_with(|| Found {
                info: AppInfo {
                    id,
                    name: name.into(),
                    publisher: "Microsoft Windows".into(),
                    executables: 1,
                    ..AppInfo::default()
                },
                exes: vec![task_manager.clone()],
                icon_source: Some(task_manager),
                icon_file: None,
                packaged: false,
            });
        }
        // Entries with nothing to block (driver packages, SDK pieces,
        // installers…) are not applications a Teacher can act on.
        by_id.into_values().filter(|f| !f.exes.is_empty()).collect()
    }

    // ------------------------------------------------- Store (MSIX) apps
    const PACKAGES: &str =
        r"Software\Classes\Local Settings\Software\Microsoft\Windows\CurrentVersion\AppModel\Repository\Packages";

    fn unescape_xml(text: &str) -> String {
        text.replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&")
    }
    /// `name="value"` inside one XML start tag.
    fn attr(tag: &str, name: &str) -> Option<String> {
        let pattern = format!("{name}=\"");
        let mut from = 0;
        while let Some(i) = tag[from..].find(&pattern) {
            let at = from + i;
            if at == 0 || tag.as_bytes()[at - 1].is_ascii_whitespace() {
                let rest = &tag[at + pattern.len()..];
                return rest.find('"').map(|end| unescape_xml(&rest[..end]));
            }
            from = at + pattern.len();
        }
        None
    }
    /// First start tag whose element name (ignoring a namespace prefix) is `element`.
    fn first_tag<'a>(xml: &'a str, element: &str) -> Option<&'a str> {
        let mut from = 0;
        while let Some(i) = xml[from..].find('<') {
            let start = from + i;
            let end = xml[start..].find('>')? + start;
            let tag = &xml[start + 1..end];
            let name = tag.split_whitespace().next().unwrap_or("");
            if !tag.starts_with('/') && name.rsplit(':').next() == Some(element) {
                return Some(&xml[start..=end]);
            }
            from = end;
        }
        None
    }
    /// Resolves `ms-resource:` display names the way Settings does.
    fn resolve_resource(full_name: &str, raw: &str) -> Option<String> {
        if !raw.starts_with("ms-resource:") {
            return Some(raw.to_owned());
        }
        let source: Vec<u16> = format!("@{{{full_name}?{raw}}}").encode_utf16().chain(std::iter::once(0)).collect();
        let mut buffer = [0u16; 256];
        // SAFETY: `source` is NUL-terminated; `buffer` is a valid output slice.
        unsafe { SHLoadIndirectString(PCWSTR(source.as_ptr()), &mut buffer, None) }.ok()?;
        let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        let text = String::from_utf16_lossy(&buffer[..len]);
        (!text.trim().is_empty()).then(|| text.trim().to_owned())
    }
    /// The best small PNG among a logo's scale/size variants.
    fn pick_logo(root: &Path, logo: &str) -> Option<PathBuf> {
        let logo_path = root.join(logo.replace('/', "\\"));
        let dir = logo_path.parent()?;
        let stem = logo_path.file_stem()?.to_string_lossy().to_ascii_lowercase();
        let mut best: Option<(u32, PathBuf)> = None;
        for entry in std::fs::read_dir(dir).ok()?.flatten().take(400) {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if !name.starts_with(&stem) || !name.ends_with(".png") || entry.metadata().map_or(true, |m| m.len() > 60_000) {
                continue;
            }
            let score = if name.contains("targetsize-48") && name.contains("unplated") {
                7
            } else if name.contains("targetsize-48") {
                6
            } else if name.contains("scale-200") {
                5
            } else if name.contains("targetsize-64") || name.contains("targetsize-72") || name.contains("targetsize-96") {
                4
            } else if name.contains("scale-100") || name.contains("scale-150") {
                3
            } else {
                1
            };
            if best.as_ref().map_or(true, |(b, _)| score > *b) {
                best = Some((score, entry.path()));
            }
        }
        best.map(|(_, p)| p)
    }
    /// Apps shown under Settings → Installed apps that come from the Store
    /// (Camera, Media Player, Teams, Calculator…), read from the packages
    /// registered for this user and each package's own manifest.
    fn packaged_apps() -> Vec<Found> {
        let Some(parent) = Key::open(HKEY_CURRENT_USER, PACKAGES, REG_SAM_FLAGS(0)) else {
            return Vec::new();
        };
        let windir = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()).to_ascii_lowercase();
        let mut found = Vec::new();
        for full_name in parent.subkeys() {
            let Some(key) = parent.child(&full_name) else {
                continue;
            };
            let Some(root) = key.string("PackageRootFolder").map(PathBuf::from) else {
                continue;
            };
            if root.to_string_lossy().to_ascii_lowercase().starts_with(&windir) {
                continue; // shell/system components, not listed under Installed apps
            }
            let Ok(manifest) = std::fs::read_to_string(root.join("AppxManifest.xml")) else {
                continue;
            };
            if manifest.contains("<Framework>true</Framework>") {
                continue;
            }
            let Some(identity) = first_tag(&manifest, "Identity") else {
                continue;
            };
            let mut exes = Vec::new();
            let mut name = None;
            let mut logo = None;
            for chunk in manifest.split("<Application ").skip(1) {
                let chunk = chunk.split("</Application>").next().unwrap_or(chunk);
                let head = chunk.split('>').next().unwrap_or("");
                let Some(exe) = attr(head, "Executable").filter(|e| e.to_ascii_lowercase().ends_with(".exe")) else {
                    continue;
                };
                let Some(visual) = first_tag(chunk, "VisualElements") else {
                    continue;
                };
                if attr(visual, "AppListEntry").is_some_and(|v| v.eq_ignore_ascii_case("none")) {
                    continue;
                }
                if name.is_none() {
                    name = attr(visual, "DisplayName").and_then(|raw| resolve_resource(&full_name, &raw));
                    logo = attr(visual, "Square44x44Logo");
                }
                exes.push(root.join(exe));
            }
            let Some(name) = name
                .or_else(|| key.string("DisplayName").and_then(|raw| resolve_resource(&full_name, &raw)))
                .filter(|n| !n.starts_with("ms-resource:"))
            else {
                continue;
            };
            if exes.is_empty() {
                continue;
            }
            let publisher = attr(identity, "Publisher")
                .and_then(|p| p.split(',').find_map(|part| part.trim().strip_prefix("CN=").map(str::to_owned)))
                .unwrap_or_default();
            found.push(Found {
                info: AppInfo {
                    id: app_id(&name),
                    name: name.chars().take(120).collect(),
                    publisher: publisher.chars().take(80).collect(),
                    version: attr(identity, "Version").unwrap_or_default(),
                    executables: exes.len(),
                    icon_png_base64: None,
                },
                exes,
                icon_source: None,
                icon_file: logo.and_then(|l| pick_logo(&root, &l)),
                packaged: true,
            });
        }
        found
    }

    pub fn list_apps(request: &ListRequest) -> AppReport {
        let wanted: HashSet<&str> = request.icons_for.iter().map(String::as_str).collect();
        let mut apps = Vec::new();
        for found in scan() {
            let mut info = found.info;
            if wanted.contains(info.id.as_str()) {
                info.icon_png_base64 = found
                    .icon_file
                    .as_deref()
                    .and_then(|p| std::fs::read(p).ok())
                    .map(|bytes| base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes))
                    .or_else(|| {
                        found
                            .icon_source
                            .as_deref()
                            .and_then(|p| crate::activity::program_icon(&p.to_string_lossy()))
                    });
            }
            apps.push(info);
        }
        AppReport { supported: true, apps, notes: Vec::new() }
    }

    // --------------------------------------------------------- enforcement
    fn run(program: &str, args: &[&str]) -> Result<std::process::Output> {
        Command::new(program)
            .args(args)
            .creation_flags(NO_WINDOW)
            .output()
            .with_context(|| format!("Could not run {program}"))
    }
    fn reg(args: &[&str]) -> Result<()> {
        let output = run("reg.exe", args)?;
        anyhow::ensure!(output.status.success(), "reg.exe failed: {}", String::from_utf8_lossy(&output.stderr).trim());
        Ok(())
    }
    fn ifeo_path(exe: &str) -> String {
        format!("HKLM\\{IFEO}\\{exe}")
    }
    fn debugger_value(stub: &Path) -> String {
        format!("\"{}\" --app-blocked", stub.display())
    }
    /// Whether `exe` already has a `Debugger` that is not ours (another tool
    /// or a developer's own setting): those are left untouched.
    fn foreign_debugger(exe: &str) -> bool {
        let Ok(output) = run("reg.exe", &["query", &ifeo_path(exe), "/v", "Debugger"]) else {
            return false;
        };
        output.status.success() && !String::from_utf8_lossy(&output.stdout).contains("--app-blocked")
    }
    fn stub_path() -> Result<PathBuf> {
        let base = std::env::var_os("ProgramData").context("ProgramData is not set")?;
        let dir = PathBuf::from(base).join("Classroom").join("AppBlock");
        std::fs::create_dir_all(&dir)?;
        let stub = dir.join(STUB_NAME);
        let me = std::env::current_exe()?;
        let stale = match (std::fs::metadata(&me), std::fs::metadata(&stub)) {
            (Ok(a), Ok(b)) => a.len() != b.len(),
            _ => true,
        };
        if stale {
            std::fs::copy(&me, &stub).context("Could not install the block notice program")?;
        }
        Ok(stub)
    }
    fn read_state_value() -> Option<Value> {
        let output = run("reg.exe", &["query", &format!("HKLM\\{STATE_KEY}"), "/v", "State"]).ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        text.split_once("REG_SZ").and_then(|(_, v)| serde_json::from_str::<Value>(v.trim()).ok())
    }
    fn read_state() -> Vec<String> {
        read_state_value()
            .and_then(|v| {
                v["exes"].as_array().map(|a| a.iter().filter_map(|e| e.as_str().map(str::to_owned)).collect())
            })
            .unwrap_or_default()
    }
    /// Fingerprint of the policy this computer enforces, or empty.
    pub fn applied_hash() -> String {
        read_state_value().and_then(|v| v["hash"].as_str().map(str::to_owned)).unwrap_or_default()
    }
    fn remove_ifeo(exe: &str) {
        if foreign_debugger(exe) {
            return;
        }
        for view in ["/reg:64", "/reg:32"] {
            let _ = reg(&["delete", &ifeo_path(exe), "/v", "Debugger", "/f", view]);
        }
    }
    /// Programs to block, resolved against this computer's own inventory.
    fn targets(policy: &AppPolicy) -> BTreeMap<String, (String, bool)> {
        let selected: HashSet<&str> = policy.ids.iter().map(String::as_str).collect();
        let allow_only = policy.mode == MODE_ALLOW_ONLY;
        let own_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
        let mut out = BTreeMap::new(); // lowercase file name -> (app name, packaged)
        for found in scan() {
            let chosen = selected.contains(found.info.id.as_str());
            if chosen == allow_only {
                continue; // block: only chosen; allow_only: only the not-chosen
            }
            let packaged = found.packaged;
            for exe in found.exes {
                let file = lower_name(&exe);
                let in_own_dir = own_dir.as_deref().is_some_and(|d| exe.starts_with(d));
                if file.is_empty() || is_protected(&file) || in_own_dir {
                    continue;
                }
                out.entry(file).or_insert_with(|| (found.info.name.clone(), packaged));
            }
        }
        out
    }

    /// Writes the registry side of `policy` and returns the lowercase
    /// executable names that must stay blocked (for the watcher).
    fn apply_registry(policy: &AppPolicy) -> Result<HashSet<String>> {
        if !policy.enabled {
            clear()?;
            return Ok(HashSet::new());
        }
        let blocked = targets(policy);
        let stub = stub_path()?;
        let debugger = debugger_value(&stub);
        let previous = read_state();
        for exe in previous.iter().filter(|e| !blocked.contains_key(e.as_str())) {
            remove_ifeo(exe);
        }
        let mut names: HashSet<String> = HashSet::new();
        for (exe, (_, packaged)) in &blocked {
            names.insert(exe.clone());
            // Store apps are started by the system's app activation, which
            // does not go through Image File Execution Options; the watcher
            // ends them the moment they appear instead.
            if *packaged || foreign_debugger(exe) {
                continue;
            }
            for view in ["/reg:64", "/reg:32"] {
                reg(&["add", &ifeo_path(exe), "/v", "Debugger", "/t", "REG_SZ", "/d", &debugger, "/f", view])
                    .context("Could not write the application block (run as administrator?)")?;
            }
        }
        let mut sorted: Vec<&String> = names.iter().collect();
        sorted.sort();
        let state = json!({
            "mode": policy.mode,
            "exes": sorted,
            "hash": policy_hash(policy),
            "day": crate::local_day::today(),
        })
        .to_string();
        reg(&["add", &format!("HKLM\\{STATE_KEY}"), "/v", "State", "/t", "REG_SZ", "/d", &state, "/f"])
            .context("Could not record the application block")?;
        // Programs already running keep running until they exit; closing them
        // is what the Teacher expects "blocked" to mean.
        for exe in &names {
            let _ = run("taskkill.exe", &["/F", "/IM", exe, "/T"]);
        }
        tracing::info!(count = names.len(), mode = %policy.mode, "application block applied");
        Ok(names)
    }
    fn clear() -> Result<()> {
        for exe in read_state() {
            remove_ifeo(&exe);
        }
        let _ = reg(&["delete", &format!("HKLM\\{STATE_KEY}"), "/f"]);
        Ok(())
    }

    /// Keeps blocked programs from running. Image File Execution Options
    /// stops ordinary programs before they start; this watcher is the
    /// enforcement for Store apps and a second line for the rest. Its list is
    /// read from the registry at start, so a rule applied earlier is back in
    /// force the moment the Student process starts — before it has even
    /// reconnected to the Teacher.
    #[derive(Clone)]
    pub struct Enforcer {
        blocked: Arc<Mutex<HashSet<String>>>,
    }
    impl Enforcer {
        pub fn start() -> Self {
            let blocked: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(read_state().into_iter().collect()));
            let shared = Arc::clone(&blocked);
            // Lives as long as the Student process; there is nothing to join.
            let _ = std::thread::Builder::new().name("app-enforcer".into()).spawn(move || watch(&shared));
            Self { blocked }
        }
        pub fn apply(&self, policy: &AppPolicy) -> Result<()> {
            let names = apply_registry(policy)?;
            *self.blocked.lock().unwrap() = names;
            Ok(())
        }
        /// A block lasts until the Teacher changes it or the local day ends.
        /// Lifts one left over from an earlier day (and stamps one from
        /// before day stamps existed).
        pub fn expire_if_stale(&self) -> Result<()> {
            let Some(state) = read_state_value() else {
                return Ok(());
            };
            match state["day"].as_u64() {
                Some(day) if day == u64::from(crate::local_day::today()) => Ok(()),
                Some(_) => {
                    tracing::info!("application block is from an earlier day; lifting it");
                    self.apply(&AppPolicy::default())
                }
                None => {
                    let mut stamped = state.clone();
                    stamped["day"] = json!(crate::local_day::today());
                    reg(&[
                        "add",
                        &format!("HKLM\\{STATE_KEY}"),
                        "/v",
                        "State",
                        "/t",
                        "REG_SZ",
                        "/d",
                        &stamped.to_string(),
                        "/f",
                    ])
                }
            }
        }
    }
    fn watch(shared: &Mutex<HashSet<String>>) {
        let mut system = sysinfo::System::new();
        loop {
            std::thread::sleep(Duration::from_millis(500));
            let names = shared.lock().unwrap().clone();
            if names.is_empty() {
                std::thread::sleep(Duration::from_millis(1500));
                continue;
            }
            system.refresh_processes_specifics(
                sysinfo::ProcessesToUpdate::All,
                true,
                sysinfo::ProcessRefreshKind::nothing(),
            );
            for process in system.processes().values() {
                if names.contains(&process.name().to_string_lossy().to_ascii_lowercase()) {
                    process.kill();
                }
            }
        }
    }
    pub fn is_applied() -> bool {
        run("reg.exe", &["query", &format!("HKLM\\{STATE_KEY}"), "/v", "State"]).is_ok_and(|o| o.status.success())
    }
    pub fn supported() -> bool {
        true
    }

    /// Entry point of the stub: Windows starts it as
    /// `ClassroomBlocked.exe --app-blocked "<blocked exe>" <its arguments>`.
    pub fn early_operation() -> Option<i32> {
        let mut args = std::env::args_os();
        let _ = args.next();
        if args.next()? != "--app-blocked" {
            return None;
        }
        let target = args.next().map(PathBuf::from).unwrap_or_default();
        let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "This program".into());
        let text: Vec<u16> =
            format!("{name} is blocked by your teacher.").encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: both strings are NUL-terminated and outlive the call.
        unsafe {
            MessageBoxW(
                None,
                PCWSTR(text.as_ptr()),
                w!("Classroom"),
                MB_OK | MB_ICONSTOP | MB_SETFOREGROUND | MB_TOPMOST,
            );
        }
        Some(1)
    }
}

#[cfg(windows)]
pub use windows_impl::{applied_hash, early_operation, is_applied, list_apps, supported, Enforcer};

#[cfg(not(windows))]
#[derive(Clone)]
pub struct Enforcer;
#[cfg(not(windows))]
impl Enforcer {
    pub fn start() -> Self {
        Self
    }
    pub fn apply(&self, _policy: &AppPolicy) -> anyhow::Result<()> {
        anyhow::bail!("Application control is only available on Windows")
    }
    pub fn expire_if_stale(&self) -> anyhow::Result<()> {
        Ok(())
    }
}
#[cfg(not(windows))]
pub fn applied_hash() -> String {
    String::new()
}
#[cfg(not(windows))]
pub fn supported() -> bool {
    false
}
#[cfg(not(windows))]
pub fn is_applied() -> bool {
    false
}
#[cfg(not(windows))]
pub fn early_operation() -> Option<i32> {
    None
}
#[cfg(not(windows))]
pub fn list_apps(_request: &ListRequest) -> AppReport {
    AppReport::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_and_architectures_do_not_split_one_program() {
        assert_eq!(normalize_name("Python 3.12.1 (64-bit)"), "python");
        assert_eq!(app_id("Python 3.12.1 (64-bit)"), app_id("Python 3.11.9 (64-bit)"));
        assert_eq!(normalize_name("7-Zip 24.07 (x64 edition)"), "7-zip");
        assert_eq!(normalize_name("Google Chrome"), "google chrome");
        assert_eq!(normalize_name("Notepad++ (64-bit x64)"), "notepad++");
        assert_ne!(app_id("Google Chrome"), app_id("Mozilla Firefox"));
        assert_eq!(app_id("Google Chrome").len(), 12);
    }
    #[test]
    fn policy_hash_ignores_order_and_is_empty_when_off() {
        let a = AppPolicy { enabled: true, mode: MODE_BLOCK.into(), ids: vec!["b".repeat(12), "a".repeat(12)] };
        let b = AppPolicy { ids: vec!["a".repeat(12), "b".repeat(12)], ..a.clone() };
        assert_eq!(policy_hash(&a), policy_hash(&b));
        assert_eq!(policy_hash(&a).len(), 16);
        assert_ne!(policy_hash(&a), policy_hash(&AppPolicy { mode: MODE_ALLOW_ONLY.into(), ..a.clone() }));
        assert_eq!(policy_hash(&AppPolicy { enabled: false, ..a }), "");
    }
    #[test]
    fn a_name_that_is_only_a_version_is_kept() {
        assert_eq!(normalize_name("7"), "7");
    }
    #[test]
    fn policy_validation_rejects_bad_input() {
        let good = AppPolicy { enabled: true, mode: MODE_BLOCK.into(), ids: vec![app_id("Zoom")] };
        assert!(good.validate().is_ok());
        assert!(AppPolicy { mode: "other".into(), ..good.clone() }.validate().is_err());
        assert!(AppPolicy { ids: vec!["zz".into()], ..good.clone() }.validate().is_err());
        assert!(AppPolicy { ids: vec!["a".repeat(12); 401], ..good }.validate().is_err());
    }
    #[test]
    fn classroom_and_core_programs_are_never_blockable() {
        for name in ["ClassroomStudent.exe", "explorer.exe", "MSEDGEWEBVIEW2.EXE"] {
            assert!(is_protected(name), "{name}");
        }
        // Task Manager is how a Student ends the Classroom agent: the
        // Teacher may choose to block it.
        assert!(!is_protected("taskmgr.exe"));
        assert!(!is_protected("chrome.exe"));
    }
}
