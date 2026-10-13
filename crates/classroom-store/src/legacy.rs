//! One-time import of the files the Teacher used before the database existed.
//!
//! Each file is imported at most once (recorded in the `meta` namespace in the
//! same transaction as the data) and then renamed to `<name>.migrated`, so the
//! user's old settings are preserved but can never be read again as truth.
use crate::{ns, Change, Result, Store};
use serde_json::{json, Value};
use std::path::Path;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub imported: Vec<String>,
    /// Files that could not be parsed; they were renamed `<name>.invalid`.
    pub invalid: Vec<String>,
}

type Convert = fn(&[u8]) -> std::result::Result<Vec<(&'static str, String, Value)>, String>;

/// `(file, converter, keep_original)`. Teacher files are set aside once
/// imported. Student/profile files are left in place: an updater rollback to a
/// build that predates the database must still find its settings and the
/// Teacher address, or the Student could never reconnect.
const SOURCES: &[(&str, Convert, bool)] = &[
    ("console-settings.json", prefs, false),
    ("teacher-settings.toml", config, false),
    ("known-students.json", roster, false),
    ("app-inventory.json", |b| whole(b, ns::INVENTORY, "cache"), false),
    ("monitoring-badges.json", |b| whole(b, ns::BADGES, "enabled"), false),
    ("recording-sessions.json", |b| whole(b, ns::RECORDINGS, "journal"), false),
    ("hardware-policy.json", |b| whole(b, ns::HARDWARE, "policy"), false),
    ("class-rules.json", |b| whole(b, ns::RULES, "today"), false),
    ("profile.toml", |b| toml_doc(b, "launch"), true),
    ("device-id.txt", device_id, true),
    ("teacher-ip.txt", teacher_ip, true),
    ("ui-preferences.toml", |b| toml_doc(b, "ui_preferences"), true),
    ("student-settings.toml", |b| toml_doc(b, "settings"), true),
    ("connection-popup.txt", connection_popup, true),
    ("monitoring-badge.json", |b| whole(b, ns::PROFILE, "badge"), true),
    ("activity-monitoring.json", |b| whole(b, ns::PROFILE, "activity_monitoring"), true),
    ("guard-policy.json", |b| whole(b, ns::PROFILE, "guard_policy"), true),
];

fn json_value(bytes: &[u8]) -> std::result::Result<Value, String> {
    serde_json::from_slice(bytes).map_err(|e| e.to_string())
}

fn whole(
    bytes: &[u8],
    namespace: &'static str,
    key: &str,
) -> std::result::Result<Vec<(&'static str, String, Value)>, String> {
    Ok(vec![(namespace, key.to_owned(), json_value(bytes)?)])
}

fn prefs(bytes: &[u8]) -> std::result::Result<Vec<(&'static str, String, Value)>, String> {
    let Value::Object(map) = json_value(bytes)? else {
        return Err("preferences must be an object".into());
    };
    Ok(map
        .into_iter()
        .filter(|(_, value)| !value.is_null())
        .map(|(key, value)| (ns::PREFS, key, value))
        .collect())
}

fn config(bytes: &[u8]) -> std::result::Result<Vec<(&'static str, String, Value)>, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    let table: toml::Value = toml::from_str(text).map_err(|e| e.to_string())?;
    let value = serde_json::to_value(table).map_err(|e| e.to_string())?;
    Ok(vec![(ns::CONFIG, "overlay".to_owned(), value)])
}

fn toml_doc(bytes: &[u8], key: &str) -> std::result::Result<Vec<(&'static str, String, Value)>, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    let table: toml::Value = toml::from_str(text).map_err(|e| e.to_string())?;
    Ok(vec![(ns::PROFILE, key.to_owned(), serde_json::to_value(table).map_err(|e| e.to_string())?)])
}

fn connection_popup(bytes: &[u8]) -> std::result::Result<Vec<(&'static str, String, Value)>, String> {
    match std::str::from_utf8(bytes).map_err(|e| e.to_string())?.trim() {
        "on" => Ok(vec![(ns::PROFILE, "connection_popup".to_owned(), json!(true))]),
        "off" => Ok(vec![(ns::PROFILE, "connection_popup".to_owned(), json!(false))]),
        other => Err(format!("unexpected popup preference '{other}'")),
    }
}

fn device_id(bytes: &[u8]) -> std::result::Result<Vec<(&'static str, String, Value)>, String> {
    let id = std::str::from_utf8(bytes).map_err(|e| e.to_string())?.trim();
    if id.is_empty() || id.len() > 64 || !id.chars().all(|c| c.is_ascii_digit() || c == '-') {
        return Err("invalid device identity".into());
    }
    Ok(vec![(ns::PROFILE, "device_id".to_owned(), json!(id))])
}

fn teacher_ip(bytes: &[u8]) -> std::result::Result<Vec<(&'static str, String, Value)>, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?.trim();
    text.parse::<std::net::Ipv4Addr>().map_err(|e| e.to_string())?;
    Ok(vec![(ns::PROFILE, "teacher_ip".to_owned(), json!(text))])
}

fn roster(bytes: &[u8]) -> std::result::Result<Vec<(&'static str, String, Value)>, String> {
    let value = json_value(bytes)?;
    let mut out = Vec::new();
    if let Some(room) = value["room"].as_str() {
        out.push((ns::ROSTER, "__room".to_owned(), json!(room)));
    }
    for student in value["students"].as_array().into_iter().flatten().take(256) {
        if let Some(id) = student["id"].as_str() {
            out.push((ns::ROSTER, id.to_owned(), student.clone()));
        }
    }
    Ok(out)
}

fn set_aside(path: &Path, suffix: &str) {
    let mut target = path.as_os_str().to_owned();
    target.push(suffix);
    let target = std::path::PathBuf::from(target);
    let _ = std::fs::remove_file(&target);
    if let Err(error) = std::fs::rename(path, &target) {
        tracing::warn!(%error, file = %path.display(), "Could not set the legacy file aside");
    }
}

pub(crate) fn import(store: &Store, dir: &Path) -> Result<ImportReport> {
    let mut report = ImportReport::default();
    for (name, convert, keep) in SOURCES {
        let path = dir.join(name);
        let flag = format!("legacy:{name}");
        if !path.is_file() {
            continue;
        }
        if store.meta_flag(&flag)? {
            // Imported earlier; an older build recreated the file. The
            // database stays authoritative, so leave the stray file alone.
            continue;
        }
        let bytes = match std::fs::read(&path) {
            Ok(bytes) if bytes.len() <= crate::MAX_DOCUMENT_BYTES => bytes,
            _ => {
                report.invalid.push((*name).to_owned());
                set_aside(&path, ".invalid");
                continue;
            }
        };
        match convert(&bytes) {
            Ok(entries) => {
                let mut changes: Vec<Change> = entries
                    .into_iter()
                    .map(|(namespace, key, value)| (namespace.to_owned(), key, Some(value)))
                    .collect();
                changes.push((ns::META.to_owned(), flag, Some(json!({"at": crate::now_ms()}))));
                store.apply_many(&changes)?;
                report.imported.push((*name).to_owned());
                if !keep {
                    set_aside(&path, ".migrated");
                }
            }
            Err(error) => {
                tracing::warn!(%error, file = %name, "Legacy Teacher file is invalid; not imported");
                report.invalid.push((*name).to_owned());
                set_aside(&path, ".invalid");
            }
        }
    }
    Ok(report)
}


#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("legacy-{name}-{}", crate::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn imports_every_source_once_and_sets_files_aside() {
        let dir = temp("all");
        std::fs::write(
            dir.join("console-settings.json"),
            r#"{"view":"list","grid_size":380.0,"apps_policy":null,"launch_items":[{"id":"a"}]}"#,
        )
        .unwrap();
        std::fs::write(dir.join("teacher-settings.toml"), "[network]\ncontrol_port = 47800\n[tools]\nmax_file_mb = 300\n").unwrap();
        std::fs::write(
            dir.join("known-students.json"),
            r#"{"room":"10.0.0.1:47800","students":[{"id":"s1","name":"PC","ip":"10.0.0.2"}]}"#,
        )
        .unwrap();
        std::fs::write(dir.join("monitoring-badges.json"), r#"{"s1":true}"#).unwrap();
        std::fs::write(dir.join("recording-sessions.json"), "[]").unwrap();
        std::fs::write(dir.join("hardware-policy.json"), r#"{"revision":"r"}"#).unwrap();
        std::fs::write(dir.join("class-rules.json"), r#"{"day":20261012,"activity":true}"#).unwrap();
        std::fs::write(dir.join("app-inventory.json"), r#"{"saved_ms":1,"apps":[]}"#).unwrap();
        let store = Store::open_in_memory().unwrap();
        let report = store.import_legacy(&dir).unwrap();
        assert_eq!(report.imported.len(), 8);
        assert!(report.invalid.is_empty());
        let prefs = store.list(ns::PREFS).unwrap();
        assert_eq!(prefs["view"], json!("list"));
        assert!(!prefs.contains_key("apps_policy"), "null preference is absent, not stored");
        assert_eq!(store.get(ns::CONFIG, "overlay").unwrap().unwrap()["network"]["control_port"], json!(47800));
        assert_eq!(store.get(ns::ROSTER, "__room").unwrap(), Some(json!("10.0.0.1:47800")));
        assert_eq!(store.get(ns::ROSTER, "s1").unwrap().unwrap()["name"], json!("PC"));
        assert_eq!(store.get(ns::BADGES, "enabled").unwrap(), Some(json!({"s1": true})));
        assert!(!dir.join("console-settings.json").exists());
        assert!(dir.join("console-settings.json.migrated").exists());
        // An older build recreating a file must not override the database.
        std::fs::write(dir.join("console-settings.json"), r#"{"view":"grid"}"#).unwrap();
        let again = store.import_legacy(&dir).unwrap();
        assert!(again.imported.is_empty());
        assert_eq!(store.get(ns::PREFS, "view").unwrap(), Some(json!("list")));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_file_is_preserved_and_does_not_block_others() {
        let dir = temp("bad");
        std::fs::write(dir.join("console-settings.json"), "{ not json").unwrap();
        std::fs::write(dir.join("monitoring-badges.json"), r#"{"s1":false}"#).unwrap();
        let store = Store::open_in_memory().unwrap();
        let report = store.import_legacy(&dir).unwrap();
        assert_eq!(report.invalid, ["console-settings.json"]);
        assert_eq!(report.imported, ["monitoring-badges.json"]);
        assert!(dir.join("console-settings.json.invalid").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod profile_tests {
    use super::*;

    #[test]
    fn student_files_are_imported_but_left_in_place_for_rollback() {
        let dir = std::env::temp_dir().join(format!("legacy-student-{}", crate::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("profile.toml"), "role = \"student\"\nteacher_ip = \"192.168.50.206\"\n").unwrap();
        std::fs::write(dir.join("device-id.txt"), "1790990215628075000-4419\n").unwrap();
        std::fs::write(dir.join("teacher-ip.txt"), "192.168.50.206\n").unwrap();
        std::fs::write(dir.join("student-settings.toml"), "[network]\nteacher_ip = \"192.168.50.206\"\ncontrol_port = 47800\n").unwrap();
        std::fs::write(dir.join("connection-popup.txt"), "off").unwrap();
        std::fs::write(dir.join("monitoring-badge.json"), r#"{"enabled":true}"#).unwrap();
        std::fs::write(dir.join("activity-monitoring.json"), r#"{"on":true,"day":20261012}"#).unwrap();
        std::fs::write(dir.join("guard-policy.json"), r#"{"day":20261012}"#).unwrap();
        let store = Store::open_in_memory().unwrap();
        let report = store.import_legacy(&dir).unwrap();
        assert_eq!(report.imported.len(), 8, "{report:?}");
        assert_eq!(store.get(ns::PROFILE, "connection_popup").unwrap(), Some(json!(false)));
        assert_eq!(store.get(ns::PROFILE, "launch").unwrap().unwrap()["role"], json!("student"));
        assert_eq!(store.get(ns::PROFILE, "device_id").unwrap(), Some(json!("1790990215628075000-4419")));
        assert_eq!(store.get(ns::PROFILE, "teacher_ip").unwrap(), Some(json!("192.168.50.206")));
        assert_eq!(store.get(ns::PROFILE, "settings").unwrap().unwrap()["network"]["control_port"], json!(47800));
        assert_eq!(store.get(ns::PROFILE, "badge").unwrap(), Some(json!({"enabled": true})));
        for file in ["profile.toml", "device-id.txt", "teacher-ip.txt", "student-settings.toml", "guard-policy.json"] {
            assert!(dir.join(file).exists(), "{file} must stay for an older build");
        }
        // Importing again never re-reads (and never overrides) the database.
        std::fs::write(dir.join("monitoring-badge.json"), r#"{"enabled":false}"#).unwrap();
        assert!(store.import_legacy(&dir).unwrap().imported.is_empty());
        assert_eq!(store.get(ns::PROFILE, "badge").unwrap(), Some(json!({"enabled": true})));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bad_device_identity_is_rejected_not_imported() {
        assert!(device_id(b"../etc/passwd").is_err());
        assert!(device_id(b"").is_err());
        assert!(device_id(b"123-45").is_ok());
        assert!(teacher_ip(b"not an ip").is_err());
    }
}
