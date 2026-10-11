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
        let mut ids: Vec<String> = self.allowed.iter().map(|id| id.to_ascii_lowercase()).collect();
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
                        name: value["displayName"].as_str().unwrap_or(name).to_owned(),
                        description: value["description"]
                            .as_str()
                            .unwrap_or("")
                            .chars()
                            .take(300)
                            .collect(),
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
        let state = serde_json::json!({"hash": policy.fingerprint(), "day": crate::local_day::today()}).to_string();
        reg(&["add", &format!("HKLM\\{RULE_KEY}"), "/v", "State", "/t", "REG_SZ", "/d", &state, "/f"])
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
        rule_state().and_then(|v| v["hash"].as_str().map(str::to_owned)).unwrap_or_default()
    }
    /// A policy lasts until the Teacher changes it or the local day ends —
    /// not until the connection drops. Lifts one left over from an earlier
    /// day; a policy from before day stamps existed is stamped with today.
    pub fn expire_if_stale() -> Result<()> {
        if !is_applied() {
            return Ok(());
        }
        match rule_state() {
            Some(state) if state["day"].as_u64() == Some(u64::from(crate::local_day::today())) => Ok(()),
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
pub use windows_impl::{applied_fingerprint, apply, expire_if_stale, is_applied, list_extensions, supported};

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
            name: v["displayName"].as_str().unwrap_or(name).to_owned(),
            description: v["description"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(300)
                .collect(),
            publisher: publisher.to_owned(),
        });
    }
    report.extensions = by_id.into_values().collect();
    report
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
}
