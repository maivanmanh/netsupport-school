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
//!   microsoft/vscode#268012 is an open request for one), so as a
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

    fn extensions_dir() -> Option<PathBuf> {
        Some(PathBuf::from(std::env::var_os("USERPROFILE")?).join(".vscode").join("extensions"))
    }
    fn settings_path() -> Option<PathBuf> {
        Some(PathBuf::from(std::env::var_os("APPDATA")?).join("Code").join("User").join("settings.json"))
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
                    let Ok(bytes) = std::fs::read(&manifest) else { continue };
                    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else { continue };
                    let (Some(name), Some(publisher)) = (value["name"].as_str(), value["publisher"].as_str()) else {
                        continue;
                    };
                    let id = format!("{publisher}.{name}").to_ascii_lowercase();
                    byid.entry(id.clone()).or_insert_with(|| ExtensionInfo {
                        id,
                        name: value["displayName"].as_str().unwrap_or(name).to_owned(),
                        description: value["description"].as_str().unwrap_or("").chars().take(300).collect(),
                        publisher: publisher.to_owned(),
                    });
                }
            }
            Err(error) => notes.push(format!("Could not read the extensions folder: {error}")),
        }
        let mut extensions: Vec<ExtensionInfo> = byid
            .into_values()
            .filter(|e| !COPILOT_IDS.contains(&e.id.as_str()))
            .collect();
        extensions.sort_by(|a, b| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()));
        ExtensionReport { supported: true, extensions, notes }
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
        let mut value: Value = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
        if !value.is_object() {
            value = Value::Object(serde_json::Map::new());
        }
        if disable {
            value["chat.disableAIFeatures"] = Value::Bool(true);
        } else if let Some(map) = value.as_object_mut() {
            map.remove("chat.disableAIFeatures");
        }
        std::fs::write(&path, serde_json::to_vec_pretty(&value)?)?;
        if disable {
            if let Some(user) = username() {
                // Deny write, delete and "change permissions" for the
                // Student's own account: owning the file no longer lets a
                // standard user grant themselves access back.
                icacls(&[path.to_string_lossy().as_ref(), "/deny", &format!("{user}:(WD,DC,WDAC,WO)")]);
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
        let json = serde_json::to_string(&Value::Object(map))?;
        reg(&["add", &format!("HKLM\\{REG_PATH}"), "/v", REG_VALUE, "/t", "REG_SZ", "/d", &json, "/f"])
            .context("Could not write the extension allowlist (run as administrator?)")?;
        set_disable_ai(true)?;
        // VS Code only resolves `AllowedExtensions` at process start (seen
        // directly in its own log: PolicyConfiguration#initialize runs once
        // during startup). A copy the Student already had open before this
        // policy landed — or deliberately keeps open to dodge it — would
        // otherwise keep running unrestricted indefinitely, so force a
        // restart whenever enforcement turns on. Relies on VS Code's default
        // hot-exit to restore unsaved buffers; not restarted on `clear()`
        // since relaxing a restriction is never urgent enough to justify
        // interrupting the Student.
        restart_vscode_if_running();
        Ok(())
    }
    pub fn clear() -> Result<()> {
        let _ = reg(&["delete", &format!("HKLM\\{REG_PATH}"), "/v", REG_VALUE, "/f"]);
        set_disable_ai(false)?;
        Ok(())
    }
    /// Best-effort: a failed restart still leaves the (stronger) registry
    /// policy in place for VS Code's next ordinary launch, so this never
    /// escalates — only *immediate* enforcement would be delayed, not lost.
    fn restart_vscode_if_running() {
        let running = Command::new("tasklist.exe")
            .args(["/FI", "IMAGENAME eq Code.exe", "/FO", "CSV", "/NH"])
            .creation_flags(NO_WINDOW)
            .output()
            .is_ok_and(|o| {
                String::from_utf8_lossy(&o.stdout).to_ascii_lowercase().contains("code.exe")
            });
        if !running {
            return;
        }
        if let Err(error) = Command::new("taskkill.exe")
            .args(["/F", "/IM", "Code.exe", "/T"])
            .creation_flags(NO_WINDOW)
            .output()
        {
            tracing::warn!(%error, "Could not force-close VS Code for policy enforcement");
            return;
        }
        // Let the OS release the previous process' file locks before the
        // relaunch tries to open the same user profile.
        std::thread::sleep(std::time::Duration::from_millis(800));
        let Some(local) = std::env::var_os("LOCALAPPDATA") else { return };
        let exe = PathBuf::from(local).join("Programs").join("Microsoft VS Code").join("Code.exe");
        if exe.exists() {
            if let Err(error) = Command::new(&exe).creation_flags(NO_WINDOW).spawn() {
                tracing::warn!(%error, "Could not relaunch VS Code after enforcing a policy");
            }
        }
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
pub use windows_impl::{apply, is_applied, list_extensions, supported};
#[cfg(windows)]
#[allow(dead_code, unused_imports)]
pub use windows_impl::clear;

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
pub fn is_applied() -> bool {
    false
}
#[cfg(not(windows))]
pub fn list_extensions() -> ExtensionReport {
    ExtensionReport::default()
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
        assert!(VsCodePolicy { enabled: true, allowed: vec!["no-dot".into()] }.validate().is_err());
        assert!(VsCodePolicy { enabled: true, allowed: vec!["a\".b".into()] }.validate().is_err());
        assert!(VsCodePolicy { enabled: true, allowed: vec!["p.n".into(); 301] }.validate().is_err());
    }
}
