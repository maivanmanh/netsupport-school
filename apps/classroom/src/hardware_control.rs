//! Durable machine policy. No disconnect/day expiry and no media dependencies.
#[cfg(windows)]
use anyhow::Context;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const GROUPS: [&str; 5] = [
    "usb_storage",
    "wifi",
    "bluetooth",
    "usb_tethering",
    "camera_microphone",
];
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools)] // Five independent owner-requested switches.
pub struct Policy {
    pub revision: String,
    pub usb_storage: bool,
    pub wifi: bool,
    pub bluetooth: bool,
    pub usb_tethering: bool,
    pub camera_microphone: bool,
    /// Only an explicit End hardware session command triggers this option.
    pub restore_on_session_end: bool,
}
impl Policy {
    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.revision.len() == 32 && self.revision.bytes().all(|c| c.is_ascii_hexdigit()),
            "Invalid hardware revision"
        );
        Ok(())
    }
    pub fn edit(&mut self, group: &str, blocked: bool) -> Result<()> {
        match group {
            "usb_storage" => self.usb_storage = blocked,
            "wifi" => self.wifi = blocked,
            "bluetooth" => self.bluetooth = blocked,
            "usb_tethering" => self.usb_tethering = blocked,
            "camera_microphone" => self.camera_microphone = blocked,
            "restore_on_session_end" => self.restore_on_session_end = blocked,
            _ => anyhow::bail!("Unknown hardware group"),
        }
        Ok(())
    }
}
pub fn supported() -> bool {
    #[cfg(windows)]
    {
        crate::guard::windows::is_elevated()
    }
    #[cfg(not(windows))]
    {
        false
    }
}
pub fn failure(status: &str, error: &str) -> Value {
    json!({"groups":GROUPS.into_iter().map(|g| (g, json!({"status":status,"error":error}))).collect::<std::collections::BTreeMap<_,_>>()})
}

#[cfg(windows)]
pub fn execute(policy: Option<&Policy>, teacher: Option<std::net::Ipv4Addr>) -> Result<Value> {
    run_windows(policy, teacher, false, false)
}
#[cfg(windows)]
fn run_windows(
    policy: Option<&Policy>,
    teacher: Option<std::net::Ipv4Addr>,
    recovery: bool,
    service: bool,
) -> Result<Value> {
    use std::{
        io::{Read, Write},
        os::windows::process::CommandExt,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    if let Some(p) = policy {
        p.validate()?;
    }
    // The reconciler is a fixed, readable script passed as the -Command text
    // (policy is JSON on stdin, never interpolated into it). Plain text rather
    // than a compressed/encoded loader: encoded fileless loaders are a classic
    // malware pattern that antivirus heuristics flag. -Command is not subject
    // to the PowerShell execution policy, so no policy override is needed.
    let script = include_str!("hardware_control/windows.ps1");
    anyhow::ensure!(
        script.encode_utf16().count() < 30000,
        "Hardware script exceeds Windows command limit"
    );
    let powershell =
        std::path::PathBuf::from(std::env::var_os("SystemRoot").context("SystemRoot missing")?)
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    let mut child = Command::new(powershell)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
        .creation_flags(0x0800_0000)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .context("Hardware stdin unavailable")?
        .write_all(&serde_json::to_vec(
            &json!({"policy":policy,"teacher":teacher.map(|v|v.to_string()),"recovery":recovery,"service":service}),
        )?)?;
    // Drain pipes concurrently so a diagnostic cannot deadlock the worker.
    let out = child.stdout.take().unwrap();
    let err = child.stderr.take().unwrap();
    fn drain(mut r: impl std::io::Read) -> Vec<u8> {
        let mut b = Vec::new();
        let _ = r.by_ref().take(65536).read_to_end(&mut b);
        b
    }
    let stdout = std::thread::spawn(move || drain(out));
    let stderr = std::thread::spawn(move || drain(err));
    let start = Instant::now();
    while child.try_wait()?.is_none() {
        if start.elapsed() > Duration::from_secs(45) {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("Hardware enforcement timed out; durable journal retained for retry");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let output = stdout.join().unwrap_or_default();
    let error = stderr.join().unwrap_or_default();
    anyhow::ensure!(
        child.wait()?.success(),
        "Hardware enforcement: {}",
        String::from_utf8_lossy(&error)
    );
    serde_json::from_slice(&output).context("Invalid hardware enforcement report")
}

/// Explicit local administrator recovery, usable even when the service is down.
pub fn emergency_reset() -> Result<()> {
    #[cfg(windows)]
    {
        let policy = Policy {
            revision: crate::classroom_tools::nonce()?,
            ..Default::default()
        };
        let report = run_windows(Some(&policy), None, true, false)?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        anyhow::ensure!(
            !GROUPS
                .iter()
                .any(|g| report["groups"][g]["status"] == "failed"),
            "Some devices could not be restored; see group errors above"
        );
        Ok(())
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("Hardware recovery is available on Windows only")
    }
}
#[cfg(not(windows))]
#[allow(clippy::unnecessary_wraps)] // Same fallible interface as the Windows backend.
pub fn execute(_policy: Option<&Policy>, _teacher: Option<std::net::Ipv4Addr>) -> Result<Value> {
    Ok(failure(
        "unsupported",
        "Hardware Control requires Windows 10/11 and an elevated Student",
    ))
}

/// Service executes the same reconciler even before logon and while Teacher is absent.
#[cfg(windows)]
pub fn reconcile() {
    if let Err(error) = run_windows(None, None, false, true) {
        tracing::warn!(%error, "Hardware policy reconcile failed");
    }
}

/// One bounded latest-request slot; OS operations never block tools/media I/O.
pub fn start(
    inbox: std::sync::Arc<crate::classroom_tools::Inbox>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    teacher: std::net::Ipv4Addr,
) -> std::sync::Arc<std::sync::Mutex<Option<Policy>>> {
    use std::{
        sync::{atomic::Ordering, Arc, Mutex},
        time::{Duration, Instant},
    };
    let pending = Arc::new(Mutex::new(None::<Policy>));
    let slot = Arc::clone(&pending);
    std::thread::spawn(move || {
        let mut last: Option<Instant> = None;
        let mut active: Option<Policy> = None;
        let mut retry_apply = false;
        while !cancel.load(Ordering::Acquire) {
            let request = slot.lock().unwrap().take();
            if let Some(p) = &request {
                active = Some(p.clone());
            }
            if request.is_some() || last.is_none_or(|at| at.elapsed() >= Duration::from_secs(10)) {
                last = Some(Instant::now());
                let revision = active.as_ref().map(|p| p.revision.clone());
                let to_apply = if request.is_some() || retry_apply {
                    active.as_ref()
                } else {
                    None
                };
                let mut report = match execute(to_apply, Some(teacher)) {
                    Ok(v) => {
                        retry_apply = false;
                        v
                    }
                    Err(error) => {
                        retry_apply = true;
                        let mut report = failure(
                            "failed",
                            &format!("{error:#}").chars().take(200).collect::<String>(),
                        );
                        if let Some(p) = &active {
                            report["revision"] = json!(p.revision);
                            let policy = serde_json::to_value(p).unwrap();
                            for group in GROUPS {
                                report["groups"][group]["blocked"] = policy[group].clone();
                            }
                        }
                        report
                    }
                };
                if let Some(p) = request.as_ref() {
                    report["revision"] = json!(p.revision);
                    let policy = serde_json::to_value(p).unwrap();
                    for group in GROUPS {
                        report["groups"][group]["blocked"] = policy[group].clone();
                    }
                }
                if !report["groups"].is_object() {
                    report = failure(
                        "failed",
                        "Durable hardware policy/report is missing; waiting for policy retry",
                    );
                    if let Some(p) = &active {
                        report["revision"] = json!(p.revision);
                        let policy = serde_json::to_value(p).unwrap();
                        for group in GROUPS {
                            report["groups"][group]["blocked"] = policy[group].clone();
                        }
                        retry_apply = true;
                    }
                }
                let id = report["revision"].as_str().map(str::to_owned).or(revision);
                if let Some(id) = id {
                    let ack = protocol::ToolPacket {
                        version: 1,
                        action: "hardware_ack".into(),
                        id,
                        text: report.to_string(),
                        ..Default::default()
                    };
                    inbox.hardware_ack(ack);
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    pending
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn policy_roundtrip_and_independent_switches() {
        let mut p = Policy {
            revision: "a".repeat(32),
            ..Default::default()
        };
        p.edit("wifi", true).unwrap();
        p.edit("usb_storage", true).unwrap();
        p.edit("wifi", false).unwrap();
        assert!(p.usb_storage);
        assert!(!p.wifi);
        assert!(!p.bluetooth);
        assert_eq!(
            serde_json::from_slice::<Policy>(&serde_json::to_vec(&p).unwrap()).unwrap(),
            p
        );
        p.validate().unwrap();
        assert!(p.edit("disk", true).is_err());
        p.revision = "bad".into();
        assert!(p.validate().is_err());
    }
}
