#![allow(unsafe_code)]
//! Application control for Ubuntu Students, with no root helper.
//!
//! * **Inventory**: the desktop entries (`*.desktop`) a user sees in the
//!   application launcher, collapsed to one id per normalized name exactly like
//!   the Windows inventory so one program on many computers is one Teacher entry.
//! * **Enforcement**: a watcher inside the Student session ends processes of
//!   blocked programs shortly after they start (SIGTERM, then SIGKILL). Unlike
//!   Windows Image File Execution Options this cannot stop a program from
//!   starting; it limits how long one stays up (about one scan interval). It
//!   only touches processes of the Student's own user.
use super::{app_id, is_protected, normalize_name, AppInfo, AppPolicy, AppReport, ListRequest, MODE_ALLOW_ONLY, MODE_BLOCK};
use anyhow::{Context, Result};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

const SCAN_INTERVAL: Duration = Duration::from_millis(700);
const STATE_FILE: &str = "app-policy.json";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Stored {
    policy: AppPolicy,
    day: u32,
}

struct Entry {
    name: String,
    executables: BTreeSet<String>,
    icon: Option<PathBuf>,
}

fn application_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        PathBuf::from("/var/lib/snapd/desktop/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
    ];
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.push(home.join(".local/share/applications"));
        dirs.push(home.join(".local/share/flatpak/exports/share/applications"));
    }
    dirs
}

/// First real program in an `Exec=` line: skips `env VAR=x`, field codes and quotes.
fn exec_program(exec: &str) -> Option<String> {
    let mut parts = exec.split_whitespace().peekable();
    while let Some(part) = parts.next() {
        let part = part.trim_matches('"');
        if part == "env" || (part.contains('=') && !part.starts_with('/')) || part.starts_with('%') {
            continue;
        }
        let name = Path::new(part).file_name()?.to_str()?.to_owned();
        // Wrappers such as `flatpak run <app-id>` are not the program to match.
        if name == "flatpak" || name == "snap" {
            return parts.find(|p| !p.starts_with('-') && *p != "run").map(|p| p.trim_matches('"').to_owned());
        }
        return Some(name);
    }
    None
}

fn parse_desktop(text: &str) -> Option<(String, String, Option<String>)> {
    let mut in_entry = false;
    let (mut name, mut exec, mut icon, mut hidden, mut kind) = (None, None, None, false, String::from("Application"));
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            match key {
                "Name" => name = Some(value.to_owned()),
                "Exec" => exec = Some(value.to_owned()),
                "Icon" => icon = Some(value.to_owned()),
                "NoDisplay" | "Hidden" => hidden |= value.eq_ignore_ascii_case("true"),
                "Type" => kind = value.to_owned(),
                _ => {}
            }
        }
    }
    (kind == "Application" && !hidden).then_some(())?;
    Some((name?, exec?, icon))
}

fn icon_path(icon: &str) -> Option<PathBuf> {
    if icon.starts_with('/') {
        return Path::new(icon).is_file().then(|| PathBuf::from(icon));
    }
    let bases = ["/usr/share/icons/hicolor", "/usr/share/icons/Yaru", "/usr/share/icons/Adwaita"];
    for base in bases {
        for size in ["64x64", "48x48", "128x128", "256x256", "32x32"] {
            let path = Path::new(base).join(size).join("apps").join(format!("{icon}.png"));
            if path.is_file() {
                return Some(path);
            }
        }
    }
    let pixmap = Path::new("/usr/share/pixmaps").join(format!("{icon}.png"));
    pixmap.is_file().then_some(pixmap)
}

fn inventory() -> BTreeMap<String, Entry> {
    let mut apps: BTreeMap<String, Entry> = BTreeMap::new();
    for dir in application_dirs() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "desktop") {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else { continue };
            let Some((name, exec, icon)) = parse_desktop(&text) else { continue };
            let Some(program) = exec_program(&exec) else { continue };
            if is_protected(&program) {
                continue;
            }
            let id = app_id(&name);
            let item = apps.entry(id).or_insert_with(|| Entry {
                name: name.clone(),
                executables: BTreeSet::new(),
                icon: None,
            });
            item.executables.insert(program);
            if item.icon.is_none() {
                item.icon = icon.as_deref().and_then(icon_path);
            }
        }
    }
    apps
}

pub fn list_apps(request: &ListRequest) -> AppReport {
    let wanted: BTreeSet<&str> = request.icons_for.iter().map(String::as_str).collect();
    let apps = inventory()
        .into_iter()
        .map(|(id, entry)| {
            let icon = wanted
                .contains(id.as_str())
                .then(|| entry.icon.as_ref().and_then(|p| fs::read(p).ok()))
                .flatten()
                .filter(|bytes| bytes.starts_with(b"\x89PNG") && bytes.len() < 256 * 1024)
                .map(|bytes| base64::engine::general_purpose::STANDARD.encode(bytes));
            AppInfo {
                id,
                name: normalize_name(&entry.name),
                publisher: String::new(),
                version: String::new(),
                executables: entry.executables.len(),
                icon_png_base64: icon,
            }
        })
        .collect();
    AppReport { supported: true, apps, notes: vec!["Ubuntu: blocked programs are ended within about a second of starting".into()] }
}

fn state_path() -> Option<PathBuf> {
    Some(crate::launcher::settings_path()?.parent()?.join(STATE_FILE))
}

fn read_stored() -> Stored {
    state_path()
        .and_then(|p| fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Program names (process basename) to end, resolved against this computer's inventory.
fn blocked_programs(policy: &AppPolicy) -> BTreeSet<String> {
    if !policy.enabled {
        return BTreeSet::new();
    }
    let inventory = inventory();
    let chosen: BTreeSet<&str> = policy.ids.iter().map(String::as_str).collect();
    let mut out = BTreeSet::new();
    for (id, entry) in &inventory {
        let listed = chosen.contains(id.as_str());
        let blocked = if policy.mode == MODE_ALLOW_ONLY { !listed } else { policy.mode == MODE_BLOCK && listed };
        if blocked {
            out.extend(entry.executables.iter().cloned());
        }
    }
    out.retain(|name| !is_protected(name));
    out
}

fn process_names(pid: u32) -> Option<(String, String)> {
    let exe = fs::read_link(format!("/proc/{pid}/exe")).ok()?;
    let exe_name = exe.file_name()?.to_str()?.to_owned();
    let cmdline = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let first = cmdline.split(|b| *b == 0).next().map(|b| String::from_utf8_lossy(b).into_owned()).unwrap_or_default();
    let argv0 = Path::new(&first).file_name()?.to_str()?.to_owned();
    Some((exe_name, argv0))
}

fn scan(blocked: &BTreeSet<String>, pending: &mut HashMap<u32, u8>) {
    let own = std::process::id();
    let Ok(entries) = fs::read_dir("/proc") else { return };
    // SAFETY: getuid has no preconditions.
    let uid = unsafe { libc::getuid() };
    let mut seen = BTreeSet::new();
    for entry in entries.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
        if pid == own {
            continue;
        }
        let owned = fs::metadata(entry.path()).is_ok_and(|m| std::os::unix::fs::MetadataExt::uid(&m) == uid);
        if !owned {
            continue;
        }
        let Some((exe_name, argv0)) = process_names(pid) else { continue };
        if !(blocked.contains(&exe_name) || blocked.contains(&argv0)) {
            continue;
        }
        seen.insert(pid);
        let stage = pending.entry(pid).or_insert(0);
        // SAFETY: signalling a pid owned by this user; failure (already gone) is fine.
        unsafe {
            match *stage {
                0 => {
                    libc::kill(pid as i32, libc::SIGTERM);
                }
                _ => {
                    libc::kill(pid as i32, libc::SIGKILL);
                }
            }
        }
        *stage = stage.saturating_add(1);
    }
    pending.retain(|pid, _| seen.contains(pid));
}

#[derive(Clone)]
pub struct Enforcer {
    blocked: Arc<Mutex<BTreeSet<String>>>,
}

impl Enforcer {
    pub fn start() -> Self {
        let blocked = Arc::new(Mutex::new(blocked_programs(&read_stored().policy)));
        let shared = Arc::clone(&blocked);
        let _ = std::thread::Builder::new().name("app-enforcer".into()).spawn(move || {
            let mut pending = HashMap::new();
            loop {
                let current = shared.lock().unwrap().clone();
                if current.is_empty() {
                    pending.clear();
                } else {
                    scan(&current, &mut pending);
                }
                std::thread::sleep(SCAN_INTERVAL);
            }
        });
        Self { blocked }
    }

    pub fn apply(&self, policy: &AppPolicy) -> Result<()> {
        let path = state_path().context("Student data directory unavailable")?;
        if policy.active() {
            let stored = Stored { policy: policy.clone(), day: crate::local_day::today() };
            fs::write(&path, serde_json::to_vec(&stored)?)?;
        } else {
            let _ = fs::remove_file(&path);
        }
        *self.blocked.lock().unwrap() = blocked_programs(policy);
        Ok(())
    }

    /// A block lasts until the Teacher changes it or the local day ends.
    pub fn expire_if_stale(&self) -> Result<()> {
        let stored = read_stored();
        if stored.policy.active() && stored.day != crate::local_day::today() {
            tracing::info!("application block is from an earlier day; lifting it");
            self.apply(&AppPolicy::default())?;
        }
        Ok(())
    }
}

pub fn applied_hash() -> String {
    super::policy_hash(&read_stored().policy)
}

pub fn supported() -> bool {
    true
}

pub fn is_applied() -> bool {
    read_stored().policy.active()
}

pub fn early_operation() -> Option<i32> {
    None
}
