//! Class-wide control of installed VS Code (Desktop, stable — not Insiders,
//! not the portable .zip build, not a WSL-remote install) extensions, using
//! VS Code's own documented machine policy rather than any kind of hook or
//! modified install:
//!
//! * **Extensions**: `AllowedExtensions`
//!   (`HKEY_LOCAL_MACHINE\SOFTWARE\Policies\Microsoft\VSCode`, `REG_SZ`, a
//!   JSON object `{ "publisher.name": true, … }`). VS Code enforces this
//!   itself: anything not listed is disabled and its Install button greyed
//!   out, overriding whatever the Student has in their own settings. Needs
//!   VS Code ≥ 1.96. Source: `extensions.allowed` / policy name
//!   `AllowedExtensions` in microsoft/vscode's own policy registration
//!   (`build/lib/policies/policyData.jsonc`).
//! * **GitHub Copilot / Chat**: always excluded from the allowlist above —
//!   with no entry, VS Code's own default-deny applies, exactly like any
//!   other unlisted extension; there is nothing to "turn on" for it. VS Code
//!   does not yet ship a machine *registry* policy for its built-in
//!   `chat.disableAIFeatures` setting (confirmed against its source: that
//!   setting has no `policy:` registration, and
//!   microsoft/vscode#268012 was closed as not planned), so as a
//!   best-effort second layer this also writes `chat.disableAIFeatures:
//!   true` into the Student's own `User/settings.json` and removes the
//!   Student's write access to that one file (`icacls`) so the Settings UI
//!   can't save over it. Unlike the registry policy above, this layer is
//!   **not** a guaranteed enforcement — document that to the Teacher, don't
//!   imply otherwise.
//!
//! Both effects are plain filesystem/registry state: once applied they
//! persist across a Student restart or crash with no extra bookkeeping (no
//! lease/restore needed, unlike the network Guard).
#![cfg_attr(windows, allow(unsafe_code))]
use serde::{Deserialize, Serialize};

/// Always excluded from the allowlist, regardless of what the Teacher sends.
pub const COPILOT_IDS: [&str; 4] = [
    "github.copilot",
    "github.copilot-chat",
    "github.copilot-nightly",
    "github.copilot-chat-nightly",
];

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct VsCodePolicy {
    /// `false`: remove every restriction this applies.
    pub enabled: bool,
    /// Extension ids (`publisher.name`, lowercase) the Student may use.
    pub allowed: Vec<String>,
}
impl VsCodePolicy {
    /// Short stable hash of what this policy enforces; empty when off. The
    /// Student reports it so the Teacher can tell whether a computer that
    /// joined late or restarted holds the class rule.
    pub fn fingerprint(&self) -> String {
        use sha2::{Digest, Sha256};
        if !self.enabled {
            return String::new();
        }
        let mut ids: Vec<String> = self
            .allowed
            .iter()
            .map(|id| id.to_ascii_lowercase())
            .collect();
        ids.sort();
        ids.dedup();
        let digest = Sha256::digest(ids.join(",").as_bytes());
        digest[..8].iter().map(|b| format!("{b:02x}")).collect()
    }
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(self.allowed.len() <= 300, "At most 300 extensions");
        anyhow::ensure!(
            self.allowed
                .iter()
                .all(|id| id.len() <= 200 && id.contains('.') && !id.contains(['"', '\\'])),
            "Invalid extension id"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct ExtensionInfo {
    /// `publisher.name`, lowercase.
    pub id: String,
    pub name: String,
    pub description: String,
    pub publisher: String,
    /// Marketplace icon as a small PNG (base64); only for ids the Teacher asked for.
    pub icon_png_base64: Option<String>,
    /// Install folder, used to find the icon; never sent.
    #[serde(skip)]
    pub folder: std::path::PathBuf,
}
/// Request body of an extension scan.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ListRequest {
    /// Extension ids whose icons this computer should attach.
    pub icons_for: Vec<String>,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct ExtensionReport {
    pub supported: bool,
    pub extensions: Vec<ExtensionInfo>,
    pub notes: Vec<String>,
}

#[cfg(windows)]
mod windows_impl {
    use super::{ExtensionInfo, ExtensionReport, VsCodePolicy, COPILOT_IDS};
    use anyhow::{Context, Result};
    use serde_json::Value;
    use std::{
        collections::HashMap,
        os::windows::process::CommandExt,
        path::{Path, PathBuf},
        process::Command,
    };

    const NO_WINDOW: u32 = 0x0800_0000;
    const REG_PATH: &str = r"SOFTWARE\Policies\Microsoft\VSCode";
    const REG_VALUE: &str = "AllowedExtensions";
    /// Our own record of what was applied and when (the policy value itself
    /// cannot carry it).
    const RULE_KEY: &str = r"SOFTWARE\Classroom\Rules\VsCode";

    fn extensions_dir() -> Option<PathBuf> {
        Some(
            PathBuf::from(std::env::var_os("USERPROFILE")?)
                .join(".vscode")
                .join("extensions"),
        )
    }
    fn settings_path() -> Option<PathBuf> {
        Some(
            PathBuf::from(std::env::var_os("APPDATA")?)
                .join("Code")
                .join("User")
                .join("settings.json"),
        )
    }

    pub fn list_extensions() -> ExtensionReport {
        let mut notes = Vec::new();
        let Some(dir) = extensions_dir() else {
            return ExtensionReport {
                supported: true,
                extensions: Vec::new(),
                notes: vec!["Could not determine the Student's profile folder".into()],
            };
        };
        let mut byid: HashMap<String, ExtensionInfo> = HashMap::new();
        match std::fs::read_dir(&dir) {
            Ok(entries) => {
                for entry in entries.filter_map(std::result::Result::ok) {
                    let manifest = entry.path().join("package.json");
                    let Ok(bytes) = std::fs::read(&manifest) else {
                        continue;
                    };
                    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
                        continue;
                    };
                    let (Some(name), Some(publisher)) =
                        (value["name"].as_str(), value["publisher"].as_str())
                    else {
                        continue;
                    };
                    let id = format!("{publisher}.{name}").to_ascii_lowercase();
                    byid.entry(id.clone()).or_insert_with(|| ExtensionInfo {
                        id,
                        name: super::manifest_text(&value, "displayName", &entry.path())
                            .unwrap_or_else(|| name.to_owned()),
                        description: super::manifest_text(&value, "description", &entry.path())
                            .unwrap_or_default()
                            .chars()
                            .take(300)
                            .collect(),
                        publisher: publisher.to_owned(),
                        icon_png_base64: None,
                        folder: entry.path(),
                    });
                }
            }
            Err(error) => notes.push(format!("Could not read the extensions folder: {error}")),
        }
        let mut extensions: Vec<ExtensionInfo> = byid
            .into_values()
            .filter(|e| !COPILOT_IDS.contains(&e.id.as_str()))
            .collect();
        extensions.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        });
        ExtensionReport {
            supported: true,
            extensions,
            notes,
        }
    }

    fn reg(args: &[&str]) -> Result<()> {
        let output = Command::new("reg.exe")
            .args(args)
            .creation_flags(NO_WINDOW)
            .output()
            .context("Could not run reg.exe")?;
        anyhow::ensure!(
            output.status.success(),
            "reg.exe failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        Ok(())
    }
    fn icacls(args: &[&str]) {
        // Best-effort: a failure here still leaves the registry-enforced
        // extension allowlist in place, so it is logged, not escalated.
        if let Err(error) = (|| -> Result<()> {
            let output = Command::new("icacls.exe")
                .args(args)
                .creation_flags(NO_WINDOW)
                .output()
                .context("Could not run icacls.exe")?;
            anyhow::ensure!(
                output.status.success(),
                "icacls.exe failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
            Ok(())
        })() {
            tracing::warn!(%error, "Could not lock VS Code settings.json");
        }
    }
    fn username() -> Option<String> {
        std::env::var("USERNAME").ok()
    }

    /// Merges `chat.disableAIFeatures` into the Student's own
    /// `User/settings.json`, preserving every other key, then (if `lock`)
    /// denies that user write/take-ownership access to the file so VS
    /// Code's Settings UI cannot silently overwrite it back.
    fn set_disable_ai(disable: bool) -> Result<()> {
        let Some(path) = settings_path() else {
            anyhow::bail!("Could not determine the Student's VS Code settings path");
        };
        if let Some(user) = username() {
            // Unlock first: editing a locked file (including our own edit
            // below) would otherwise fail.
            icacls(&[path.to_string_lossy().as_ref(), "/remove:d", &user]);
        }
        std::fs::create_dir_all(path.parent().context("settings path has no parent")?)?;
        let mut value: Value = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .context("VS Code settings use comments or invalid JSON; left unchanged to preserve user settings")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound =>
                Value::Object(serde_json::Map::new()),
            Err(error) => return Err(error).context("Could not read VS Code settings"),
        };
        anyhow::ensure!(
            value.is_object(),
            "VS Code settings must be an object; left unchanged"
        );
        if disable {
            value["chat.disableAIFeatures"] = Value::Bool(true);
        } else if let Some(map) = value.as_object_mut() {
            map.remove("chat.disableAIFeatures");
        }
        std::fs::write(&path, serde_json::to_vec_pretty(&value)?)?;
        let written: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
        anyhow::ensure!(
            if disable {
                written["chat.disableAIFeatures"] == true
            } else {
                written.get("chat.disableAIFeatures").is_none()
            },
            "VS Code AI setting readback failed"
        );
        tracing::info!(path = %path.display(), disable, "VS Code AI setting written and verified");
        if disable {
            if let Some(user) = username() {
                // Deny write, delete and "change permissions" for the
                // Student's own account: owning the file no longer lets a
                // standard user grant themselves access back.
                icacls(&[
                    path.to_string_lossy().as_ref(),
                    "/deny",
                    &format!("{user}:(WD,DC,WDAC,WO)"),
                ]);
            }
        }
        Ok(())
    }

    pub fn apply(policy: &VsCodePolicy) -> Result<()> {
        if !policy.enabled {
            return clear();
        }
        let mut map = serde_json::Map::new();
        for id in &policy.allowed {
            let id = id.to_ascii_lowercase();
            if !COPILOT_IDS.contains(&id.as_str()) {
                map.insert(id, Value::Bool(true));
            }
        }
        // Explicit denial avoids treating an empty allowlist as unrestricted
        // and makes Copilot's exclusion unambiguous to VS Code policy readers.
        map.insert("*".into(), Value::Bool(false));
        for id in COPILOT_IDS {
            map.insert(id.into(), Value::Bool(false));
        }
        let json = serde_json::to_string(&Value::Object(map))?;
        reg(&[
            "add",
            &format!("HKLM\\{REG_PATH}"),
            "/v",
            REG_VALUE,
            "/t",
            "REG_SZ",
            "/d",
            &json,
            "/f",
        ])
        .context("Could not write the extension allowlist (run as administrator?)")?;
        let readback = Command::new("reg.exe")
            .args(["query", &format!("HKLM\\{REG_PATH}"), "/v", REG_VALUE])
            .creation_flags(NO_WINDOW)
            .output()
            .context("Could not read back VS Code policy")?;
        let text = String::from_utf8_lossy(&readback.stdout);
        let stored = text
            .split_once("REG_SZ")
            .and_then(|(_, v)| serde_json::from_str::<Value>(v.trim()).ok());
        anyhow::ensure!(
            readback.status.success()
                && stored.as_ref() == Some(&serde_json::from_str::<Value>(&json)?),
            "VS Code machine policy readback did not match the requested allowlist"
        );
        set_disable_ai(true)?;
        write_rule_state(policy)?;
        // VS Code only resolves `AllowedExtensions` at process start (seen
        // directly in its own log: PolicyConfiguration#initialize runs once
        // during startup). A copy the Student already had open before this
        // policy landed — or deliberately keeps open to dodge it — would
        // otherwise keep running unrestricted indefinitely, so force a
        // restart whenever enforcement turns on. Relies on VS Code's default
        // hot-exit to restore unsaved buffers; not restarted on `clear()`
        // since relaxing a restriction is never urgent enough to justify
        // interrupting the Student.
        restart_vscode_if_running()?;
        Ok(())
    }
    pub fn clear() -> Result<()> {
        let _ = reg(&[
            "delete",
            &format!("HKLM\\{REG_PATH}"),
            "/v",
            REG_VALUE,
            "/f",
        ]);
        let _ = reg(&["delete", &format!("HKLM\\{RULE_KEY}"), "/f"]);
        set_disable_ai(false)?;
        Ok(())
    }
    fn write_rule_state(policy: &VsCodePolicy) -> Result<()> {
        let state =
            serde_json::json!({"hash": policy.fingerprint(), "day": crate::local_day::today()})
                .to_string();
        reg(&[
            "add",
            &format!("HKLM\\{RULE_KEY}"),
            "/v",
            "State",
            "/t",
            "REG_SZ",
            "/d",
            &state,
            "/f",
        ])
        .context("Could not record the VS Code policy")
    }
    fn rule_state() -> Option<Value> {
        let output = Command::new("reg.exe")
            .args(["query", &format!("HKLM\\{RULE_KEY}"), "/v", "State"])
            .creation_flags(NO_WINDOW)
            .output()
            .ok()?;
        String::from_utf8_lossy(&output.stdout)
            .split_once("REG_SZ")
            .and_then(|(_, v)| serde_json::from_str::<Value>(v.trim()).ok())
    }
    /// Fingerprint of the enforced policy (empty when none), reported to the
    /// Teacher so it can tell whether this computer holds the class rule.
    pub fn applied_fingerprint() -> String {
        if !is_applied() {
            return String::new();
        }
        rule_state()
            .and_then(|v| v["hash"].as_str().map(str::to_owned))
            .unwrap_or_default()
    }
    /// A policy lasts until the Teacher changes it or the local day ends —
    /// not until the connection drops. Lifts one left over from an earlier
    /// day; a policy from before day stamps existed is stamped with today.
    pub fn expire_if_stale() -> Result<()> {
        if !is_applied() {
            return Ok(());
        }
        match rule_state() {
            Some(state) if state["day"].as_u64() == Some(u64::from(crate::local_day::today())) => {
                Ok(())
            }
            Some(_) => {
                tracing::info!("VS Code policy is from an earlier day; lifting it");
                clear()
            }
            None => write_rule_state(&VsCodePolicy::default()),
        }
    }
    /// A failed restart must reach the Teacher as an error: registry state
    /// alone does not confirm that an already running editor loaded it.
    fn restart_vscode_if_running() -> Result<()> {
        let running = Command::new("tasklist.exe")
            .args(["/FI", "IMAGENAME eq Code.exe", "/FO", "CSV", "/NH"])
            .creation_flags(NO_WINDOW)
            .output()
            .is_ok_and(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .to_ascii_lowercase()
                    .contains("code.exe")
            });
        if !running {
            return Ok(());
        }
        let stopped = Command::new("taskkill.exe")
            .args(["/F", "/IM", "Code.exe", "/T"])
            .creation_flags(NO_WINDOW)
            .output()
            .context("Could not close VS Code for policy enforcement")?;
        anyhow::ensure!(
            stopped.status.success(),
            "VS Code restart failed; policy has not been confirmed for the running editor"
        );
        // Let the OS release the previous process' file locks before the
        // relaunch tries to open the same user profile.
        std::thread::sleep(std::time::Duration::from_millis(800));
        let local =
            std::env::var_os("LOCALAPPDATA").context("Missing LOCALAPPDATA for VS Code restart")?;
        let exe = PathBuf::from(local)
            .join("Programs")
            .join("Microsoft VS Code")
            .join("Code.exe");
        if exe.exists() {
            Command::new(&exe)
                .creation_flags(NO_WINDOW)
                .spawn()
                .context("Could not relaunch VS Code after enforcing a policy")?;
        }
        Ok(())
    }
    pub fn supported() -> bool {
        true
    }
    /// Whether the registry allowlist is currently present, so the Teacher
    /// can learn a Student's real enforced state on reconnect (e.g. after a
    /// Teacher restart) instead of assuming it from its own job history.
    pub fn is_applied() -> bool {
        Command::new("reg.exe")
            .args(["query", &format!("HKLM\\{REG_PATH}"), "/v", REG_VALUE])
            .creation_flags(NO_WINDOW)
            .output()
            .is_ok_and(|o| o.status.success())
    }
    #[allow(dead_code)]
    fn unused(_: &Path) {}
}

#[cfg(windows)]
#[allow(dead_code, unused_imports)]
pub use windows_impl::clear;
#[cfg(windows)]
pub use windows_impl::{
    applied_fingerprint, apply, expire_if_stale, is_applied, list_extensions, supported,
};

#[cfg(not(windows))]
pub fn apply(_policy: &VsCodePolicy) -> anyhow::Result<()> {
    anyhow::bail!("VS Code policy control is only available on Windows")
}
#[cfg(not(windows))]
#[allow(dead_code)]
pub fn clear() -> anyhow::Result<()> {
    Ok(())
}
#[cfg(not(windows))]
pub fn supported() -> bool {
    false
}
#[cfg(not(windows))]
pub fn applied_fingerprint() -> String {
    String::new()
}
#[cfg(not(windows))]
pub fn expire_if_stale() -> anyhow::Result<()> {
    Ok(())
}
#[cfg(not(windows))]
pub fn is_applied() -> bool {
    false
}
#[cfg(not(windows))]
pub fn list_extensions() -> ExtensionReport {
    let mut report = ExtensionReport {
        supported: cfg!(target_os = "macos"),
        ..Default::default()
    };
    let Some(home) = std::env::var_os("HOME") else {
        return report;
    };
    let dir = std::path::PathBuf::from(home).join(".vscode/extensions");
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return report,
        Err(error) => {
            report
                .notes
                .push(format!("Could not read extensions: {error}"));
            return report;
        }
    };
    let mut by_id = std::collections::BTreeMap::new();
    for entry in entries.flatten().take(2000) {
        let Ok(bytes) = std::fs::read(entry.path().join("package.json")) else {
            continue;
        };
        let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            continue;
        };
        let (Some(name), Some(publisher)) = (v["name"].as_str(), v["publisher"].as_str()) else {
            continue;
        };
        let id = format!("{publisher}.{name}").to_ascii_lowercase();
        if COPILOT_IDS.contains(&id.as_str()) {
            continue;
        }
        by_id.entry(id.clone()).or_insert_with(|| ExtensionInfo {
            id,
            name: manifest_text(&v, "displayName", &entry.path())
                .unwrap_or_else(|| name.to_owned()),
            description: manifest_text(&v, "description", &entry.path())
                .unwrap_or_default()
                .chars()
                .take(300)
                .collect(),
            publisher: publisher.to_owned(),
            icon_png_base64: None,
            folder: entry.path(),
        });
    }
    report.extensions = by_id.into_values().collect();
    report
}

/// A manifest text such as `displayName`, with VS Code's `%key%` placeholders
/// resolved from the extension's own `package.nls.json` (English). Extensions
/// that localize their manifest otherwise show up as "%displayName%". Returns
/// `None` when the field is absent or the placeholder cannot be resolved.
#[must_use]
pub fn manifest_text(
    manifest: &serde_json::Value,
    field: &str,
    folder: &std::path::Path,
) -> Option<String> {
    let raw = manifest[field].as_str()?.trim();
    let Some(key) = raw.strip_prefix('%').and_then(|r| r.strip_suffix('%')) else {
        return (!raw.is_empty()).then(|| raw.to_owned());
    };
    let bytes = std::fs::read(folder.join("package.nls.json")).ok()?;
    let table: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    // Values are either a string or {"message": "...", "comment": [...]}.
    let entry = &table[key];
    let text = entry.as_str().or_else(|| entry["message"].as_str())?.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Longest edge of an icon sent to the Teacher. The list shows them at ~32 px.
const ICON_EDGE: u32 = 64;
/// An icon file that already is small enough is forwarded byte for byte.
const ICON_RAW_MAX: usize = 16 * 1024;
const ICON_READ_MAX: u64 = 2 * 1024 * 1024;
const ICON_SEND_MAX: usize = 48 * 1024;
const ICON_PIXELS_MAX: u64 = 4096 * 4096;

/// Attaches icons for just the requested ids. Marketplace icons are PNGs next
/// to `package.json` (`"icon": "images/icon.png"`); a typical one is 128x128
/// or larger, so anything above the raw limit is shrunk before it travels.
pub fn attach_icons(report: &mut ExtensionReport, request: &ListRequest) {
    let wanted: std::collections::HashSet<&str> = request
        .icons_for
        .iter()
        .map(String::as_str)
        .take(200)
        .collect();
    for ext in &mut report.extensions {
        if wanted.contains(ext.id.as_str()) {
            ext.icon_png_base64 = extension_icon(&ext.folder);
        }
    }
}

fn extension_icon(folder: &std::path::Path) -> Option<String> {
    use base64::Engine;
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(folder.join("package.json")).ok()?).ok()?;
    let relative = std::path::Path::new(manifest["icon"].as_str()?);
    // Stay inside the extension's own folder: no absolute path, no `..`, no
    // symlink that leads elsewhere.
    if relative.is_absolute()
        || relative
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        || !relative
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("png"))
    {
        return None;
    }
    let root = folder.canonicalize().ok()?;
    let file = root.join(relative).canonicalize().ok()?;
    if !file.starts_with(&root) || std::fs::metadata(&file).ok()?.len() > ICON_READ_MAX {
        return None;
    }
    let bytes = std::fs::read(&file).ok()?;
    let encoded = if bytes.len() <= ICON_RAW_MAX && png_edge(&bytes)? <= 128 {
        bytes
    } else {
        let small = shrink_png(&bytes, ICON_EDGE)?;
        if small.len() > ICON_SEND_MAX {
            return None;
        }
        small
    };
    Some(base64::engine::general_purpose::STANDARD.encode(encoded))
}

fn png_edge(bytes: &[u8]) -> Option<u32> {
    let reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .ok()?;
    let info = reader.info();
    Some(info.width.max(info.height))
}

/// Decodes any PNG to RGBA8 and box-filters it down so its longest edge is at
/// most `edge`; images already that small are re-encoded unchanged in size.
fn shrink_png(bytes: &[u8], edge: u32) -> Option<Vec<u8>> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let (width, height) = {
        let info = reader.info();
        (info.width, info.height)
    };
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > ICON_PIXELS_MAX {
        return None;
    }
    let mut raw = vec![0; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut raw).ok()?;
    let channels = frame.color_type.samples();
    let pixel = |x: u32, y: u32| -> [u32; 4] {
        let i = (y as usize * width as usize + x as usize) * channels;
        let p = &raw[i..i + channels];
        match frame.color_type {
            png::ColorType::Grayscale => [p[0].into(), p[0].into(), p[0].into(), 255],
            png::ColorType::GrayscaleAlpha => [p[0].into(), p[0].into(), p[0].into(), p[1].into()],
            png::ColorType::Rgb => [p[0].into(), p[1].into(), p[2].into(), 255],
            _ => [p[0].into(), p[1].into(), p[2].into(), p[3].into()],
        }
    };
    let scale = f64::from(width.max(height)) / f64::from(edge.min(width.max(height)));
    let out_w = ((f64::from(width) / scale).round() as u32).max(1);
    let out_h = ((f64::from(height) / scale).round() as u32).max(1);
    let mut out = Vec::with_capacity((out_w * out_h * 4) as usize);
    for oy in 0..out_h {
        let y0 = (f64::from(oy) * scale) as u32;
        let y1 = (((f64::from(oy) + 1.0) * scale).ceil() as u32).clamp(y0 + 1, height);
        for ox in 0..out_w {
            let x0 = (f64::from(ox) * scale) as u32;
            let x1 = (((f64::from(ox) + 1.0) * scale).ceil() as u32).clamp(x0 + 1, width);
            // Colour is weighted by alpha so transparent pixels do not darken edges.
            let (mut r, mut g, mut b, mut a, mut n) = (0u64, 0u64, 0u64, 0u64, 0u64);
            for y in y0.min(height - 1)..y1 {
                for x in x0.min(width - 1)..x1 {
                    let [pr, pg, pb, pa] = pixel(x, y);
                    r += u64::from(pr * pa);
                    g += u64::from(pg * pa);
                    b += u64::from(pb * pa);
                    a += u64::from(pa);
                    n += 1;
                }
            }
            if a == 0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                out.extend_from_slice(&[
                    (r / a) as u8,
                    (g / a) as u8,
                    (b / a) as u8,
                    (a / n.max(1)) as u8,
                ]);
            }
        }
    }
    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, out_w, out_h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header().ok()?.write_image_data(&out).ok()?;
    }
    Some(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copilot_is_never_in_the_validated_or_applied_set() {
        let policy = VsCodePolicy {
            enabled: true,
            allowed: vec!["ms-python.python".into(), "github.copilot".into()],
        };
        assert!(policy.validate().is_ok());
        assert!(COPILOT_IDS.contains(&"github.copilot"));
        assert!(!COPILOT_IDS.contains(&"ms-python.python"));
    }
    #[test]
    fn rejects_malformed_or_excessive_ids() {
        assert!(VsCodePolicy {
            enabled: true,
            allowed: vec!["no-dot".into()]
        }
        .validate()
        .is_err());
        assert!(VsCodePolicy {
            enabled: true,
            allowed: vec!["a\".b".into()]
        }
        .validate()
        .is_err());
        assert!(VsCodePolicy {
            enabled: true,
            allowed: vec!["p.n".into(); 301]
        }
        .validate()
        .is_err());
    }

    fn png_of(width: u32, height: u32, noisy: bool) -> Vec<u8> {
        let mut pixels = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let v = if noisy {
                    ((x * 7 + y * 13) % 251) as u8
                } else {
                    200
                };
                pixels.extend_from_slice(&[v, 255 - v, v / 2, 255]);
            }
        }
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&pixels)
            .unwrap();
        out
    }
    fn extension_folder(
        name: &str,
        icon: Option<&str>,
        files: &[(&str, Vec<u8>)],
    ) -> std::path::PathBuf {
        let folder =
            std::env::temp_dir().join(format!("classroom-ext-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("images")).unwrap();
        let manifest = match icon {
            Some(icon) => serde_json::json!({"name": "x", "publisher": "p", "icon": icon}),
            None => serde_json::json!({"name": "x", "publisher": "p"}),
        };
        std::fs::write(folder.join("package.json"), manifest.to_string()).unwrap();
        for (path, bytes) in files {
            std::fs::write(folder.join(path), bytes).unwrap();
        }
        folder
    }
    fn decode(b64: &str) -> Vec<u8> {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap()
    }
    #[test]
    fn small_icon_is_forwarded_unchanged() {
        let icon = png_of(32, 32, false);
        let folder = extension_folder(
            "small",
            Some("images/icon.png"),
            &[("images/icon.png", icon.clone())],
        );
        assert_eq!(decode(&extension_icon(&folder).unwrap()), icon);
    }
    #[test]
    fn large_icon_is_shrunk_to_a_few_kilobytes() {
        let big = png_of(512, 512, true);
        assert!(big.len() > ICON_RAW_MAX);
        let folder = extension_folder(
            "large",
            Some("images/icon.png"),
            &[("images/icon.png", big)],
        );
        let sent = decode(&extension_icon(&folder).unwrap());
        assert!(sent.len() <= ICON_SEND_MAX);
        assert_eq!(png_edge(&sent), Some(ICON_EDGE));
    }
    #[test]
    fn icon_cannot_leave_the_extension_folder_or_be_a_non_png() {
        let secret = png_of(16, 16, false);
        let folder = extension_folder("escape", Some("../secret.png"), &[]);
        std::fs::write(folder.parent().unwrap().join("secret.png"), &secret).unwrap();
        assert!(extension_icon(&folder).is_none());
        let svg = extension_folder(
            "svg",
            Some("images/icon.svg"),
            &[("images/icon.svg", b"<svg/>".to_vec())],
        );
        assert!(extension_icon(&svg).is_none());
        let absolute = extension_folder("abs", Some("/etc/hosts.png"), &[]);
        assert!(extension_icon(&absolute).is_none());
        assert!(extension_icon(&extension_folder("none", None, &[])).is_none());
        let garbage = extension_folder(
            "garbage",
            Some("images/icon.png"),
            &[("images/icon.png", vec![1, 2, 3])],
        );
        assert!(extension_icon(&garbage).is_none());
    }
    #[test]
    fn only_requested_ids_get_icons() {
        let folder = extension_folder(
            "only",
            Some("images/icon.png"),
            &[("images/icon.png", png_of(24, 24, false))],
        );
        let ext = |id: &str| ExtensionInfo {
            id: id.into(),
            folder: folder.clone(),
            ..Default::default()
        };
        let mut report = ExtensionReport {
            supported: true,
            extensions: vec![ext("p.a"), ext("p.b")],
            notes: vec![],
        };
        attach_icons(
            &mut report,
            &ListRequest {
                icons_for: vec!["p.b".into()],
            },
        );
        assert!(report.extensions[0].icon_png_base64.is_none());
        assert!(report.extensions[1].icon_png_base64.is_some());
    }

    #[test]
    fn localized_manifest_placeholders_resolve_from_the_nls_file() {
        let folder = std::env::temp_dir().join(format!("classroom-nls-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("package.nls.json"),
            r#"{"displayName":"Dev Containers","extension.description":{"message":"Official C# extension","comment":["x"]},"empty":""}"#,
        )
        .unwrap();
        let manifest = serde_json::json!({
            "displayName": "%displayName%",
            "description": "%extension.description%",
            "missing": "%nothing.here%",
            "empty": "%empty%",
            "plain": "  Python  ",
        });
        assert_eq!(
            manifest_text(&manifest, "displayName", &folder).as_deref(),
            Some("Dev Containers")
        );
        assert_eq!(
            manifest_text(&manifest, "description", &folder).as_deref(),
            Some("Official C# extension")
        );
        assert_eq!(
            manifest_text(&manifest, "plain", &folder).as_deref(),
            Some("Python")
        );
        // Unresolvable placeholders and absent fields are None, never "%x%".
        assert_eq!(manifest_text(&manifest, "missing", &folder), None);
        assert_eq!(manifest_text(&manifest, "empty", &folder), None);
        assert_eq!(manifest_text(&manifest, "absent", &folder), None);
        // No nls file at all.
        assert_eq!(
            manifest_text(&manifest, "displayName", &folder.join("nope")),
            None
        );
    }
}
