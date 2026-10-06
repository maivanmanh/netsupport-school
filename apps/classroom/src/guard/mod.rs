//! Student-side policy enforcement ("Guard"). One place owns what the Teacher
//! may impose on a Student computer; each concern is an [`Enforcer`] behind
//! it. Internet control is the first one; tamper protection (keep the Student
//! alive, deny termination, respawn) is the planned next, and the whole Guard
//! is written to move unchanged into a privileged Windows service whose
//! unprivileged Student process sends it the same [`Policy`] documents.
//! See docs/21_GUARD_ARCHITECTURE.md.
pub mod dns;
pub mod internet;
pub mod rules;
#[cfg(windows)]
pub mod windows;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tracing::{info, warn};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Everything allowed (no restriction).
    #[default]
    Open,
    /// Only the local machine and the classroom LAN are reachable.
    BlockAll,
    /// Only the listed sites (plus local/LAN) are reachable.
    AllowOnly,
    /// Everything except the listed sites is reachable.
    BlockOnly,
}
impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::BlockAll => "block_all",
            Self::AllowOnly => "allow_only",
            Self::BlockOnly => "block_only",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct InternetPolicy {
    pub mode: Mode,
    pub rules: Vec<String>,
}

/// The complete desired state; the Teacher always sends the whole document.
/// New concerns are new optional fields, so old documents stay valid.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Policy {
    pub internet: InternetPolicy,
    /// Minutes the policy survives without hearing from the Teacher.
    pub lease_minutes: u32,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            internet: InternetPolicy::default(),
            lease_minutes: 240,
        }
    }
}
impl Policy {
    pub fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            (1..=1440).contains(&self.lease_minutes),
            "lease must be 1–1440 minutes"
        );
        anyhow::ensure!(self.internet.rules.len() <= 400, "At most 400 rules");
        rules::RuleSet::parse(&self.internet.rules)?;
        if matches!(self.internet.mode, Mode::AllowOnly | Mode::BlockOnly) {
            anyhow::ensure!(
                self.internet.rules.iter().any(|r| {
                    matches!(rules::parse_line(r), Ok(Some(_)))
                }),
                "Add at least one site to the list"
            );
        }
        Ok(())
    }
}

/// One enforced concern.
pub trait Enforcer: Send {
    fn name(&self) -> &'static str;
    /// Makes the system match `policy` (idempotent).
    fn apply(&mut self, policy: &Policy) -> Result<()>;
    /// Removes everything this enforcer ever installed.
    fn clear(&mut self);
}

#[derive(Serialize, Deserialize)]
struct Saved {
    policy: Policy,
    last_seen_unix: u64,
}
fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

struct State {
    enforcers: Vec<Box<dyn Enforcer>>,
    applied: Option<Policy>,
    last_seen: std::time::Instant,
    last_saved: u64,
}
pub struct Guard {
    state: Mutex<State>,
    file: PathBuf,
    supported: bool,
}
impl Guard {
    pub fn new(state_dir: &std::path::Path, enforcers: Vec<Box<dyn Enforcer>>, supported: bool) -> Self {
        Self {
            state: Mutex::new(State {
                enforcers,
                applied: None,
                last_seen: std::time::Instant::now(),
                last_saved: 0,
            }),
            file: state_dir.join("guard-policy.json"),
            supported,
        }
    }
    /// Whether this Student can enforce anything (Windows, administrator).
    pub fn supported(&self) -> bool {
        self.supported
    }
    /// Internet mode currently enforced, for the Teacher's overview.
    pub fn mode(&self) -> &'static str {
        self.state
            .lock()
            .unwrap()
            .applied
            .as_ref()
            .map_or("open", |p| p.internet.mode.name())
    }
    fn save(&self, state: &mut State) {
        let Some(policy) = &state.applied else {
            let _ = std::fs::remove_file(&self.file);
            return;
        };
        let saved = Saved {
            policy: policy.clone(),
            last_seen_unix: unix_secs(),
        };
        if let Ok(bytes) = serde_json::to_vec(&saved) {
            let temporary = self.file.with_extension("tmp");
            if std::fs::write(&temporary, bytes)
                .and_then(|()| std::fs::rename(&temporary, &self.file))
                .is_ok()
            {
                state.last_saved = saved.last_seen_unix;
            }
        }
    }
    pub fn apply(&self, policy: Policy) -> Result<()> {
        anyhow::ensure!(
            self.supported,
            "This Student cannot enforce policies (needs Windows and administrator rights)"
        );
        policy.validate()?;
        let mut state = self.state.lock().unwrap();
        let open = policy.internet.mode == Mode::Open;
        if open {
            for e in &mut state.enforcers {
                e.clear();
            }
            state.applied = None;
        } else {
            for e in &mut state.enforcers {
                e.apply(&policy)
                    .with_context(|| format!("{} enforcement failed", e.name()))?;
            }
            state.applied = Some(policy);
        }
        state.last_seen = std::time::Instant::now();
        self.save(&mut state);
        info!(mode = state.applied.as_ref().map_or("open", |p| p.internet.mode.name()), "Guard policy applied");
        Ok(())
    }
    /// Start-up: resume a still-valid persisted policy, otherwise remove any
    /// leftovers of a crashed run.
    pub fn restore(&self) {
        if !self.supported {
            return;
        }
        let saved: Option<Saved> = std::fs::read(&self.file)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok());
        let live = saved.filter(|s| {
            unix_secs().saturating_sub(s.last_seen_unix)
                < u64::from(s.policy.lease_minutes) * 60
        });
        match live {
            Some(saved) => {
                if let Err(error) = self.apply(saved.policy) {
                    warn!(%error, "Could not resume the saved Guard policy; clearing");
                    self.clear();
                }
            }
            None => self.clear(),
        }
    }
    pub fn clear(&self) {
        let mut state = self.state.lock().unwrap();
        for e in &mut state.enforcers {
            e.clear();
        }
        state.applied = None;
        self.save(&mut state);
    }
    /// The Teacher answered a poll: the lease restarts (saved at most once a minute).
    pub fn keepalive(&self) {
        let mut state = self.state.lock().unwrap();
        state.last_seen = std::time::Instant::now();
        if state.applied.is_some() && unix_secs().saturating_sub(state.last_saved) >= 60 {
            self.save(&mut state);
        }
    }
    /// Called while the Teacher is unreachable: lifts a policy whose lease ran out.
    pub fn tick(&self) {
        let mut state = self.state.lock().unwrap();
        let Some(policy) = &state.applied else { return };
        let lease = Duration::from_secs(u64::from(policy.lease_minutes) * 60);
        if state.last_seen.elapsed() > lease {
            warn!("Teacher unreachable for the whole lease; lifting Guard policy");
            for e in &mut state.enforcers {
                e.clear();
            }
            state.applied = None;
            self.save(&mut state);
        }
    }
    pub fn status(&self) -> Value {
        json!({"supported":self.supported,"mode":self.mode()})
    }
}
impl Drop for Guard {
    /// A normal exit hands the network back; a crash is repaired by `restore`.
    fn drop(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            for e in &mut state.enforcers {
                e.clear();
            }
        }
    }
}

/// The Guard for this Student: real enforcement on an elevated Windows
/// Student, otherwise a Guard that reports itself unsupported.
#[cfg(windows)]
pub fn for_student(state_dir: &std::path::Path, teacher_ip: std::net::Ipv4Addr) -> Guard {
    let internet = internet::InternetEnforcer::new(
        Box::new(windows::WfpFirewall::new()),
        Box::new(windows::NrptRedirect),
        std::net::SocketAddr::from(([127, 0, 0, 1], 53)),
        teacher_ip,
    );
    Guard::new(state_dir, vec![Box::new(internet)], windows::is_elevated())
}
#[cfg(not(windows))]
pub fn for_student(state_dir: &std::path::Path, _teacher_ip: std::net::Ipv4Addr) -> Guard {
    Guard::new(state_dir, Vec::new(), false)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub struct Recorder(pub std::sync::Arc<Mutex<Vec<String>>>);
    impl Enforcer for Recorder {
        fn name(&self) -> &'static str {
            "recorder"
        }
        fn apply(&mut self, policy: &Policy) -> Result<()> {
            self.0.lock().unwrap().push(format!("apply {}", policy.internet.mode.name()));
            Ok(())
        }
        fn clear(&mut self) {
            self.0.lock().unwrap().push("clear".into());
        }
    }
    fn temp() -> PathBuf {
        let p = std::env::temp_dir().join(format!("guard-test-{}-{}", std::process::id(), media_core::unix_us()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    fn policy(mode: Mode, rules: &[&str], lease: u32) -> Policy {
        Policy {
            internet: InternetPolicy { mode, rules: rules.iter().map(|s| (*s).into()).collect() },
            lease_minutes: lease,
        }
    }

    #[test]
    fn policies_validate_and_documents_stay_forward_compatible() {
        assert!(policy(Mode::AllowOnly, &[], 60).validate().is_err());
        assert!(policy(Mode::AllowOnly, &["# only a comment"], 60).validate().is_err());
        assert!(policy(Mode::BlockOnly, &["bad host"], 60).validate().is_err());
        assert!(policy(Mode::BlockAll, &[], 0).validate().is_err());
        assert!(policy(Mode::BlockOnly, &["a.com", "*.b.com"], 60).validate().is_ok());
        let future: Policy = serde_json::from_str(r#"{"internet":{"mode":"block_all"},"protect":{"x":1}}"#).unwrap();
        assert_eq!(future.internet.mode, Mode::BlockAll);
    }

    #[test]
    fn apply_persist_restore_and_unblock() {
        let dir = temp();
        let log = std::sync::Arc::new(Mutex::new(Vec::new()));
        let guard = Guard::new(&dir, vec![Box::new(Recorder(log.clone()))], true);
        guard.apply(policy(Mode::BlockAll, &[], 60)).unwrap();
        assert_eq!(guard.mode(), "block_all");
        assert!(dir.join("guard-policy.json").exists());
        // A new process within the lease resumes it...
        let again = Guard::new(&dir, vec![Box::new(Recorder(log.clone()))], true);
        again.restore();
        assert_eq!(again.mode(), "block_all");
        // ...Unblock all removes it and the saved copy.
        again.apply(Policy::default()).unwrap();
        assert_eq!(again.mode(), "open");
        assert!(!dir.join("guard-policy.json").exists());
        assert!(log.lock().unwrap().contains(&"apply block_all".to_string()));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn expired_saved_policy_is_cleared_and_unsupported_students_refuse() {
        let dir = temp();
        let saved = Saved { policy: policy(Mode::BlockAll, &[], 1), last_seen_unix: unix_secs() - 3600 };
        std::fs::write(dir.join("guard-policy.json"), serde_json::to_vec(&saved).unwrap()).unwrap();
        let log = std::sync::Arc::new(Mutex::new(Vec::new()));
        let guard = Guard::new(&dir, vec![Box::new(Recorder(log.clone()))], true);
        guard.restore();
        assert_eq!(guard.mode(), "open");
        assert_eq!(log.lock().unwrap().as_slice(), ["clear"]);
        let unsupported = Guard::new(&dir, vec![], false);
        assert!(unsupported.apply(policy(Mode::BlockAll, &[], 60)).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
