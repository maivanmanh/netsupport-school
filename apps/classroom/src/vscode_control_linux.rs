//! VS Code extension control for Ubuntu Students, without administrator rights.
//!
//! Windows writes VS Code's machine policy (`AllowedExtensions`) into HKLM.
//! The equivalent machine file on Linux needs root, so this applies the same
//! allowlist through VS Code's own user setting `extensions.allowed` in every
//! `User/settings.json` found (deb/tarball, snap, Flatpak) and makes the file
//! read-only. That is **best effort**: the Student owns the file and can undo it,
//! unlike the Windows policy. Copilot/Chat extensions are never allowed and
//! `chat.disableAIFeatures` is set. A state file lets the Student report what is
//! applied after a restart, and a block lifts itself when the local day changes.
use super::{VsCodePolicy, COPILOT_IDS};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

const STATE_FILE: &str = "vscode-policy.json";
const KEY_ALLOWED: &str = "extensions.allowed";
const KEY_AI: &str = "chat.disableAIFeatures";

#[derive(Default, Serialize, Deserialize)]
struct Stored {
    policy: VsCodePolicy,
    day: u32,
    /// Files this policy changed, so a clear touches only them.
    files: Vec<PathBuf>,
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

fn user_dirs() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    [
        ".config/Code/User",
        "snap/code/current/.config/Code/User",
        ".var/app/com.visualstudio.code/config/Code/User",
    ]
    .iter()
    .map(|p| home.join(p))
    .filter(|p| p.is_dir())
    .collect()
}

/// Removes `//` and `/* */` comments and trailing commas from JSONC outside strings.
fn strip_jsonc(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let (mut i, mut in_string) = (0, false);
    while i < chars.len() {
        let c = chars[i];
        if in_string {
            out.push(c);
            if c == '\\' && i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 1;
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
            out.push(c);
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            continue;
        } else {
            out.push(c);
        }
        i += 1;
    }
    // Trailing commas before } or ].
    let bytes: Vec<char> = out.chars().collect();
    let mut cleaned = String::with_capacity(bytes.len());
    let mut in_str = false;
    for (idx, &c) in bytes.iter().enumerate() {
        if in_str {
            cleaned.push(c);
            if c == '"' && bytes[idx.saturating_sub(1)] != '\\' {
                in_str = false;
            }
            continue;
        }
        if c == '"' {
            in_str = true;
        }
        if c == ',' {
            let next = bytes[idx + 1..].iter().find(|n| !n.is_whitespace());
            if matches!(next, Some('}' | ']')) {
                continue;
            }
        }
        cleaned.push(c);
    }
    cleaned
}

fn load_settings(path: &Path) -> Result<Map<String, Value>> {
    match fs::read_to_string(path) {
        Ok(text) if text.trim().is_empty() => Ok(Map::new()),
        Ok(text) => match serde_json::from_str::<Value>(&strip_jsonc(&text)) {
            Ok(Value::Object(map)) => Ok(map),
            _ => anyhow::bail!("{} is not valid VS Code settings; left unchanged", path.display()),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Map::new()),
        Err(error) => Err(error).with_context(|| format!("Cannot read {}", path.display())),
    }
}

fn write_settings(path: &Path, map: &Map<String, Value>, readonly: bool) -> Result<()> {
    if path.exists() {
        let mut mode = fs::metadata(path)?.permissions();
        mode.set_mode(0o644);
        fs::set_permissions(path, mode)?;
    }
    let tmp = path.with_extension("json.classroom-tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(&Value::Object(map.clone()))?)?;
    fs::set_permissions(&tmp, fs::Permissions::from_mode(if readonly { 0o444 } else { 0o644 }))?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn allowlist(policy: &VsCodePolicy) -> Value {
    let mut map = Map::new();
    for id in &policy.allowed {
        let id = id.to_ascii_lowercase();
        if !COPILOT_IDS.contains(&id.as_str()) {
            map.insert(id, Value::Bool(true));
        }
    }
    Value::Object(map)
}

pub fn apply(policy: &VsCodePolicy) -> Result<()> {
    policy.validate()?;
    let state = state_path().context("Student data directory unavailable")?;
    let previous = read_stored();
    // Undo the earlier application before applying a new one.
    for file in &previous.files {
        if let Ok(mut map) = load_settings(file) {
            map.remove(KEY_ALLOWED);
            map.remove(KEY_AI);
            let _ = write_settings(file, &map, false);
        }
    }
    if !policy.enabled {
        let _ = fs::remove_file(&state);
        return Ok(());
    }
    let dirs = user_dirs();
    anyhow::ensure!(!dirs.is_empty(), "VS Code (deb, snap or Flatpak) is not set up for this user yet");
    let mut files = Vec::new();
    for dir in dirs {
        let path = dir.join("settings.json");
        let mut map = load_settings(&path)?;
        if path.exists() {
            let backup = path.with_extension("json.classroom-backup");
            if !backup.exists() {
                let _ = fs::copy(&path, backup);
            }
        }
        map.insert(KEY_ALLOWED.into(), allowlist(policy));
        map.insert(KEY_AI.into(), json!(true));
        write_settings(&path, &map, true)?;
        files.push(path);
    }
    let stored = Stored { policy: policy.clone(), day: crate::local_day::today(), files };
    fs::write(state, serde_json::to_vec(&stored)?)?;
    Ok(())
}

pub fn supported() -> bool {
    true
}

pub fn applied_fingerprint() -> String {
    read_stored().policy.fingerprint()
}

pub fn is_applied() -> bool {
    read_stored().policy.enabled
}

/// Lifts a block left over from an earlier local day.
pub fn expire_if_stale() -> Result<()> {
    let stored = read_stored();
    if stored.policy.enabled && stored.day != crate::local_day::today() {
        tracing::info!("VS Code policy is from an earlier day; lifting it");
        apply(&VsCodePolicy::default())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn jsonc_comments_and_trailing_commas_are_removed_outside_strings() {
        let text = "{\n // note\n \"a\": \"http://x // not a comment\", /* c */\n \"b\": [1,2,],\n}";
        let value: Value = serde_json::from_str(&strip_jsonc(text)).unwrap();
        assert_eq!(value["a"], "http://x // not a comment");
        assert_eq!(value["b"], json!([1, 2]));
    }
    #[test]
    fn allowlist_never_contains_copilot() {
        let policy = VsCodePolicy {
            enabled: true,
            allowed: vec!["ms-python.python".into(), "GitHub.Copilot".into()],
        };
        let list = allowlist(&policy);
        assert_eq!(list["ms-python.python"], true);
        assert!(list.get("github.copilot").is_none());
    }
}
