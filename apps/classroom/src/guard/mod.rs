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
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
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
    pub copilot_block: bool,
    /// Kept for old documents. A policy no longer lapses when the Teacher is
    /// unreachable: it holds until the Teacher changes it or the local day
    /// ends (see [`Guard::tick`]).
    pub lease_minutes: u32,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            internet: InternetPolicy::default(),
            copilot_block: false,
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
                self.internet
                    .rules
                    .iter()
                    .any(|r| { matches!(rules::parse_line(r), Ok(Some(_))) }),
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

/// Short stable hash of an internet policy; empty for "open".
pub fn fingerprint(internet: &InternetPolicy) -> String {
    use sha2::{Digest, Sha256};
    if internet.mode == Mode::Open {
        return String::new();
    }
    let mut rules = internet.rules.clone();
    rules.sort();
    let digest = Sha256::digest(format!("{}|{}", internet.mode.name(), rules.join("\n")).as_bytes());
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Serialize, Deserialize)]
struct Saved {
    policy: Policy,
    last_seen_unix: u64,
    /// Local day (`yyyymmdd`) the policy was applied; void on any other day.
    /// `0` in files written before this existed.
    #[serde(default)]
    day: u32,
}
fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

struct State {
    enforcers: Vec<Box<dyn Enforcer>>,
    applied: Option<Policy>,
    applied_day: u32,
    last_seen: std::time::Instant,
    last_saved: u64,
}
pub struct Guard {
    state: Mutex<State>,
    doc: Option<classroom_store::Doc>,
    supported: bool,
}
impl Guard {
    pub fn new(
        state_dir: &std::path::Path,
        enforcers: Vec<Box<dyn Enforcer>>,
        supported: bool,
    ) -> Self {
        Self {
            state: Mutex::new(State {
                enforcers,
                applied: None,
                applied_day: 0,
                last_seen: std::time::Instant::now(),
                last_saved: 0,
            }),
            doc: classroom_store::open_shared(state_dir)
                .map_err(|error| warn!(%error, "Guard policy storage unavailable"))
                .ok()
                .map(|store| store.doc(classroom_store::ns::PROFILE, "guard_policy")),
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
        let Some(doc) = &self.doc else { return };
        let Some(policy) = &state.applied else {
            let _ = doc.clear();
            return;
        };
        let saved = Saved {
            policy: policy.clone(),
            last_seen_unix: unix_secs(),
            day: state.applied_day,
        };
        if doc.write(&saved).is_ok() {
            state.last_saved = saved.last_seen_unix;
        }
    }
    pub fn apply(&self, policy: Policy) -> Result<()> {
        self.apply_change(policy, None)
    }
    /// Internet UI updates must not lift the independently selected AI block.
    pub fn apply_internet(&self, policy: Policy) -> Result<()> {
        self.apply_change(policy, Some(false))
    }
    pub fn set_copilot(&self, enabled: bool) -> Result<()> {
        let mut policy = Policy::default();
        policy.copilot_block = enabled;
        self.apply_change(policy, Some(true))
    }
    pub fn copilot_blocked(&self) -> bool {
        self.state
            .lock()
            .unwrap()
            .applied
            .as_ref()
            .is_some_and(|p| p.copilot_block)
    }
    fn apply_change(&self, mut policy: Policy, change: Option<bool>) -> Result<()> {
        anyhow::ensure!(
            self.supported,
            "This Student cannot enforce policies (needs Windows and administrator rights)"
        );
        let mut state = self.state.lock().unwrap();
        if let Some(copilot_only) = change {
            let current = state.applied.clone().unwrap_or_default();
            if copilot_only {
                policy.internet = current.internet;
                policy.lease_minutes = current.lease_minutes;
            } else {
                policy.copilot_block = current.copilot_block;
            }
        }
        policy.validate()?;
        let open = policy.internet.mode == Mode::Open && !policy.copilot_block;
        if open {
            for e in &mut state.enforcers {
                e.clear();
            }
            state.applied = None;
        } else {
            let previous = state.applied.clone();
            let mut failure = None;
            for e in &mut state.enforcers {
                if let Err(error) = e
                    .apply(&policy)
                    .with_context(|| format!("{} enforcement failed", e.name()))
                {
                    failure = Some(error);
                    break;
                }
            }
            if let Some(error) = failure {
                let mut restored = true;
                for e in &mut state.enforcers {
                    if let Some(old) = &previous {
                        if let Err(restore_error) = e.apply(old) {
                            warn!(%restore_error, "Could not restore prior Guard policy after failure");
                            restored = false;
                        }
                    } else {
                        e.clear();
                    }
                }
                if !restored {
                    for e in &mut state.enforcers {
                        e.clear();
                    }
                    state.applied = None;
                    self.save(&mut state);
                    return Err(error)
                        .context("Prior Guard policy could not be restored; restrictions cleared");
                }
                return Err(error);
            }
            state.applied = Some(policy);
            state.applied_day = crate::local_day::today();
        }
        state.last_seen = std::time::Instant::now();
        self.save(&mut state);
        info!(
            mode = state
                .applied
                .as_ref()
                .map_or("open", |p| p.internet.mode.name()),
            "Guard policy applied"
        );
        Ok(())
    }
    /// Start-up: resume a still-valid persisted policy, otherwise remove any
    /// leftovers of a crashed run.
    pub fn restore(&self) {
        if !self.supported {
            return;
        }
        let saved: Option<Saved> = self.doc.as_ref().and_then(|doc| doc.read().ok().flatten());
        // Valid for the rest of the local day it was applied — whether or not
        // the Teacher has been reachable since. Files from before day stamps
        // fall back to the old lease once, and are stamped when resumed.
        let live = saved.filter(|s| {
            if s.day == 0 {
                unix_secs().saturating_sub(s.last_seen_unix) < u64::from(s.policy.lease_minutes) * 60
            } else {
                s.day == crate::local_day::today()
            }
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
        // Applying DNS/WFP policy can take seconds on a slow Student. Never
        // hold up tools polling behind that work: apply_change refreshes the
        // lease when it finishes, and the next poll saves the heartbeat.
        let Ok(mut state) = self.state.try_lock() else {
            return;
        };
        state.last_seen = std::time::Instant::now();
        if state.applied.is_some() && unix_secs().saturating_sub(state.last_saved) >= 60 {
            self.save(&mut state);
        }
    }
    /// Lifts a policy left over from an earlier day. Cheap; called often.
    /// Nothing else ends a policy: not a lost connection, not a Teacher
    /// restart, not a Student restart.
    pub fn tick(&self) {
        let Ok(mut state) = self.state.try_lock() else {
            return;
        };
        if state.applied.is_none() || state.applied_day == crate::local_day::today() {
            return;
        }
        warn!("Guard policy is from an earlier day; lifting it");
        for e in &mut state.enforcers {
            e.clear();
        }
        state.applied = None;
        self.save(&mut state);
    }
    /// Fingerprint of the enforced internet policy (empty when open), which the
    /// Student reports so a Teacher can tell whether it is in step.
    pub fn internet_fingerprint(&self) -> String {
        self.state
            .lock()
            .unwrap()
            .applied
            .as_ref()
            .map(|p| fingerprint(&p.internet))
            .unwrap_or_default()
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
    use std::path::PathBuf;

    pub struct Recorder(pub std::sync::Arc<Mutex<Vec<String>>>);
    impl Enforcer for Recorder {
        fn name(&self) -> &'static str {
            "recorder"
        }
        fn apply(&mut self, policy: &Policy) -> Result<()> {
            self.0
                .lock()
                .unwrap()
                .push(format!("apply {}", policy.internet.mode.name()));
            Ok(())
        }
        fn clear(&mut self) {
            self.0.lock().unwrap().push("clear".into());
        }
    }
    fn policy_doc(dir: &std::path::Path) -> classroom_store::Doc {
        classroom_store::open_shared(dir)
            .unwrap()
            .doc(classroom_store::ns::PROFILE, "guard_policy")
    }
    fn saved_policy(dir: &std::path::Path) -> Option<Saved> {
        policy_doc(dir).read().unwrap()
    }
    fn write_saved(dir: &std::path::Path, saved: &Saved) {
        policy_doc(dir).write(saved).unwrap();
    }
    fn temp() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "guard-test-{}-{}",
            std::process::id(),
            media_core::unix_us()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    fn policy(mode: Mode, rules: &[&str], lease: u32) -> Policy {
        Policy {
            internet: InternetPolicy {
                mode,
                rules: rules.iter().map(|s| (*s).into()).collect(),
            },
            lease_minutes: lease,
            copilot_block: false,
        }
    }

    #[test]
    fn policies_validate_and_documents_stay_forward_compatible() {
        assert!(policy(Mode::AllowOnly, &[], 60).validate().is_err());
        assert!(policy(Mode::AllowOnly, &["# only a comment"], 60)
            .validate()
            .is_err());
        assert!(policy(Mode::BlockOnly, &["bad host"], 60)
            .validate()
            .is_err());
        assert!(policy(Mode::BlockAll, &[], 0).validate().is_err());
        assert!(policy(Mode::BlockOnly, &["a.com", "*.b.com"], 60)
            .validate()
            .is_ok());
        let future: Policy =
            serde_json::from_str(r#"{"internet":{"mode":"block_all"},"protect":{"x":1}}"#).unwrap();
        assert_eq!(future.internet.mode, Mode::BlockAll);
    }

    #[test]
    fn heartbeat_does_not_wait_for_policy_application() {
        let dir = temp();
        let guard = Guard::new(&dir, vec![], true);
        let state = guard.state.lock().unwrap();
        guard.keepalive();
        guard.tick();
        drop(state);
        guard.keepalive();
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn apply_persist_restore_and_unblock() {
        let dir = temp();
        let log = std::sync::Arc::new(Mutex::new(Vec::new()));
        let guard = Guard::new(&dir, vec![Box::new(Recorder(log.clone()))], true);
        guard.apply(policy(Mode::BlockAll, &[], 60)).unwrap();
        assert_eq!(guard.mode(), "block_all");
        assert!(saved_policy(&dir).is_some());
        // A new process within the lease resumes it...
        let again = Guard::new(&dir, vec![Box::new(Recorder(log.clone()))], true);
        again.restore();
        assert_eq!(again.mode(), "block_all");
        // ...Unblock all removes it and the saved copy.
        again.apply(Policy::default()).unwrap();
        assert_eq!(again.mode(), "open");
        assert!(saved_policy(&dir).is_none());
        assert!(log.lock().unwrap().contains(&"apply block_all".to_string()));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_policy_outlives_the_teacher_but_not_the_day() {
        let dir = temp();
        let log = std::sync::Arc::new(Mutex::new(Vec::new()));
        // Applied today, last heard from the Teacher long ago: still in force.
        let today = crate::local_day::today();
        let saved = Saved {
            policy: policy(Mode::BlockAll, &[], 1),
            last_seen_unix: unix_secs() - 7 * 24 * 3600,
            day: today,
        };
        write_saved(&dir, &saved);
        let guard = Guard::new(&dir, vec![Box::new(Recorder(log.clone()))], true);
        guard.restore();
        assert_eq!(guard.mode(), "block_all");
        guard.tick();
        assert_eq!(guard.mode(), "block_all");
        // The same file stamped with another day is void.
        let stale = Saved { day: today - 1, ..saved };
        write_saved(&dir, &stale);
        let next = Guard::new(&dir, vec![Box::new(Recorder(log.clone()))], true);
        next.restore();
        assert_eq!(next.mode(), "open");
        // A policy that was live when the day rolled over is lifted by tick.
        guard.state.lock().unwrap().applied_day = today - 1;
        guard.tick();
        assert_eq!(guard.mode(), "open");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn fingerprints_ignore_rule_order_and_open_is_empty() {
        let a = InternetPolicy { mode: Mode::AllowOnly, rules: vec!["a.com".into(), "b.com".into()] };
        let b = InternetPolicy { mode: Mode::AllowOnly, rules: vec!["b.com".into(), "a.com".into()] };
        assert_eq!(fingerprint(&a), fingerprint(&b));
        assert_eq!(fingerprint(&InternetPolicy::default()), "");
        assert_ne!(fingerprint(&a), fingerprint(&InternetPolicy { mode: Mode::BlockOnly, ..a.clone() }));
    }

    #[test]
    fn expired_saved_policy_is_cleared_and_unsupported_students_refuse() {
        let dir = temp();
        let saved = Saved {
            policy: policy(Mode::BlockAll, &[], 1),
            last_seen_unix: unix_secs() - 3600,
            day: 0,
        };
        write_saved(&dir, &saved);
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
