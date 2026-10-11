//! Class-wide desired policy on the existing tools poll/ack channel.
use super::{nonce, packet, Hub};
use crate::hardware_control::{failure, Policy, GROUPS};
use anyhow::{Context, Result};
use protocol::ToolPacket;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Default)]
pub(super) struct Room {
    policy: Option<Policy>,
    path: Option<PathBuf>,
    reports: HashMap<String, Value>,
    sent: HashMap<String, Instant>,
    acknowledged: HashMap<String, Instant>,
}
impl Room {
    pub(super) fn snapshot(
        &self,
        peers: &HashMap<String, u64>,
        caps: &HashMap<String, Value>,
    ) -> Value {
        let mut reports = self.reports.clone();
        for (id, report) in &mut reports {
            if !peers.contains_key(id) {
                *report = failure(
                    "pending",
                    "Offline; latest policy will be sent on reconnect",
                );
            } else if caps.get(id).is_none_or(|c| c["hardware"] != 1) {
                *report = failure(
                    "unsupported",
                    "Requires updated elevated Windows Student and Guard service",
                );
            } else if self
                .acknowledged
                .get(id)
                .is_some_and(|at| at.elapsed() > Duration::from_secs(70))
            {
                *report = failure(
                    "failed",
                    "Hardware status is stale; waiting for the Student OS reconciler",
                );
            } else if self
                .sent
                .get(id)
                .is_some_and(|at| at.elapsed() > Duration::from_secs(60))
                && report["revision"] != self.policy.as_ref().map_or("", |p| p.revision.as_str())
            {
                *report=failure("failed", "No matching hardware acknowledgement within 60 seconds; retry/reconnect will reconcile");
            }
        }
        json!({"policy":self.policy,"reports":reports})
    }
}
impl Hub {
    pub fn load_hardware(&self, path: PathBuf) -> Result<()> {
        let policy: Option<Policy> = match fs::read(&path) {
            Ok(bytes) => Some(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        if let Some(p) = &policy {
            p.validate()?;
        }
        let mut s = self.0.lock().unwrap();
        s.hardware.policy = policy;
        s.hardware.path = Some(path);
        Ok(())
    }
    pub fn set_hardware(&self, input: &Value, ids: &[String], end: bool) -> Result<()> {
        let mut s = self.0.lock().unwrap();
        let mut p = s.hardware.policy.clone().unwrap_or_default();
        if end {
            if !p.restore_on_session_end {
                anyhow::bail!("Enable restore on explicit session end first");
            }
            p = Policy {
                restore_on_session_end: true,
                ..Default::default()
            };
        } else {
            p.edit(
                input["group"].as_str().context("Missing hardware group")?,
                input["blocked"]
                    .as_bool()
                    .context("Missing hardware switch")?,
            )?;
        }
        p.revision = nonce()?;
        let path = s
            .hardware
            .path
            .as_ref()
            .context("Hardware persistence unavailable")?;
        let tmp = path.with_extension("hardware.tmp");
        let mut f = File::create(&tmp)?;
        f.write_all(&serde_json::to_vec(&p)?)?;
        f.sync_all()?;
        drop(f);
        crate::monitoring_badge::atomic_replace(&tmp, path)?;
        s.hardware.policy = Some(p);
        s.hardware.sent.clear();
        s.hardware.acknowledged.clear();
        let known: Vec<_> = s
            .hardware
            .reports
            .keys()
            .cloned()
            .chain(ids.iter().cloned())
            .chain(s.peers.keys().cloned())
            .collect();
        for id in known {
            s.hardware.reports.insert(
                id,
                failure("pending", "Waiting for Student acknowledgement"),
            );
        }
        Ok(())
    }
    pub(super) fn hardware_ack(&self, student: &str, p: &ToolPacket) -> Result<()> {
        anyhow::ensure!(p.text.len() <= 6000, "Hardware report too large");
        let report: Value = serde_json::from_str(&p.text)?;
        let mut s = self.0.lock().unwrap();
        if s.hardware
            .policy
            .as_ref()
            .is_some_and(|policy| policy.revision != p.id)
        {
            return Ok(());
        }
        anyhow::ensure!(
            p.id.len() == 32 && p.id.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid hardware report revision"
        );
        anyhow::ensure!(report["revision"] == p.id, "Hardware revision mismatch");
        let policy = s
            .hardware
            .policy
            .as_ref()
            .map(serde_json::to_value)
            .transpose()?;
        for group in GROUPS {
            let r = &report["groups"][group];
            anyhow::ensure!(
                matches!(
                    r["status"].as_str(),
                    Some("pending" | "applied" | "failed" | "unsupported")
                ) && r["error"].as_str().is_some_and(|v| v.len() <= 1000)
                    && r["blocked"].is_boolean()
                    && policy.as_ref().is_none_or(|p| r["blocked"] == p[group]),
                "Invalid hardware group report"
            );
        }
        s.hardware.reports.insert(student.into(), report);
        s.hardware
            .acknowledged
            .insert(student.into(), Instant::now());
        Ok(())
    }
    pub(super) fn hardware_poll(
        &self,
        student: &str,
        supported: bool,
        last: &mut Option<String>,
    ) -> Result<Option<ToolPacket>> {
        let mut s = self.0.lock().unwrap();
        let Some(policy) = s.hardware.policy.clone() else {
            return Ok(None);
        };
        if !supported {
            s.hardware.reports.insert(
                student.into(),
                failure(
                    "unsupported",
                    "Requires updated elevated Windows Student and Guard service",
                ),
            );
            return Ok(None);
        }
        // Re-send current intent on every reconnect and retry unanswered commands.
        let retry = s
            .hardware
            .sent
            .get(student)
            .is_some_and(|t| t.elapsed() > Duration::from_secs(65))
            && s.hardware
                .reports
                .get(student)
                .is_none_or(|r| r["revision"] != policy.revision);
        if last.as_ref() == Some(&policy.revision) && !retry {
            return Ok(None);
        }
        let mut p = packet("hardware_policy");
        p.id.clone_from(&policy.revision);
        p.text = serde_json::to_string(&policy)?;
        *last = Some(policy.revision);
        s.hardware.sent.insert(student.into(), Instant::now());
        s.hardware.acknowledged.remove(student);
        s.hardware
            .reports
            .insert(student.into(), failure("pending", "Applying latest policy"));
        Ok(Some(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn group_ack_errors_timeout_and_offline_are_not_success() {
        let path = std::env::temp_dir().join(format!("hardware-{}.json", nonce().unwrap()));
        let hub = Hub::default();
        hub.load_hardware(path.clone()).unwrap();
        hub.set_hardware(
            &json!({"group":"camera_microphone","blocked":true}),
            &["pc".into()],
            false,
        )
        .unwrap();
        {
            let mut s = hub.0.lock().unwrap();
            s.peers.insert("pc".into(), 1);
            s.capabilities.insert("pc".into(), json!({"hardware":1}));
        }
        let command = hub.hardware_poll("pc", true, &mut None).unwrap().unwrap();
        {
            let mut s = hub.0.lock().unwrap();
            s.hardware
                .sent
                .insert("pc".into(), Instant::now() - Duration::from_secs(61));
        }
        assert_eq!(
            hub.snapshot()["hardware"]["reports"]["pc"]["groups"]["wifi"]["status"],
            "failed"
        );
        let mut report = failure("applied", "");
        let policy: Value = serde_json::from_str(&command.text).unwrap();
        report["revision"] = json!(command.id);
        for g in GROUPS {
            report["groups"][g]["blocked"] = policy[g].clone();
        }
        report["groups"]["camera_microphone"]["status"] = json!("failed");
        report["groups"]["camera_microphone"]["error"] = json!("Driver refused disable");
        let mut ack = packet("hardware_ack");
        ack.id = command.id;
        ack.text = report.to_string();
        hub.hardware_ack("pc", &ack).unwrap();
        assert_eq!(
            hub.snapshot()["hardware"]["reports"]["pc"]["groups"]["camera_microphone"]["error"],
            "Driver refused disable"
        );
        report["groups"]["wifi"]["blocked"] = json!(true);
        ack.text = report.to_string();
        assert!(hub.hardware_ack("pc", &ack).is_err());
        hub.0.lock().unwrap().peers.remove("pc");
        assert_eq!(
            hub.snapshot()["hardware"]["reports"]["pc"]["groups"]["camera_microphone"]["status"],
            "pending"
        );
        fs::remove_file(path).unwrap();
    }
    #[test]
    fn class_policy_persists_reconnects_and_rejects_stale_ack() {
        let path = std::env::temp_dir().join(format!("hardware-{}.json", nonce().unwrap()));
        let hub = Hub::default();
        hub.load_hardware(path.clone()).unwrap();
        hub.set_hardware(
            &json!({"group":"wifi","blocked":true}),
            &["offline".into()],
            false,
        )
        .unwrap();
        let mut last = None;
        let first = hub
            .hardware_poll("late-join", true, &mut last)
            .unwrap()
            .unwrap();
        assert!(hub
            .hardware_poll("late-join", true, &mut last)
            .unwrap()
            .is_none());
        let mut reconnect = None;
        assert_eq!(
            hub.hardware_poll("late-join", true, &mut reconnect)
                .unwrap()
                .unwrap()
                .id,
            first.id
        );
        hub.set_hardware(&json!({"group":"wifi","blocked":false}), &[], false)
            .unwrap();
        let mut stale = packet("hardware_ack");
        stale.id = first.id;
        stale.text = "{}".into();
        hub.hardware_ack("late-join", &stale).unwrap();
        assert!(hub.0.lock().unwrap().hardware.reports["late-join"]["revision"].is_null());
        let restored = Hub::default();
        restored.load_hardware(path.clone()).unwrap();
        assert!(
            !restored
                .0
                .lock()
                .unwrap()
                .hardware
                .policy
                .as_ref()
                .unwrap()
                .wifi
        );
        assert!(restored.set_hardware(&json!({}), &[], true).is_err());
        restored
            .set_hardware(
                &json!({"group":"restore_on_session_end","blocked":true}),
                &[],
                false,
            )
            .unwrap();
        restored.set_hardware(&json!({}), &[], true).unwrap();
        fs::remove_file(path).unwrap();
    }
}
