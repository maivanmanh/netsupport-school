//! Teacher-requested activity report. The Student never scans on demand: it
//! hands over whatever its background collector last produced
//! (`activity::collector`, gated by [`Hub::activity_monitoring`]), so this
//! round trip is cheap and fast regardless of how busy the Student is. The
//! result travels back over the same generic "uploaded JSON" channel
//! directory listings use (`fs_result`); landing it also updates the
//! Teacher's per-student cache ([`Hub::activity_cached`]) so a later request
//! can be answered instantly, before the Student even answers.
use super::{configure, nonce, packet, read, send, Hub};
use anyhow::Result;
use protocol::ToolPacket;
use serde_json::{json, Value};
use std::{
    io::Write,
    net::{SocketAddrV4, TcpStream},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

/// At most this many uploads run at once across all Students.
const MAX_CONCURRENT: usize = 4;

impl Hub {
    /// Queues a refresh from `student` (skipped quietly if one is already
    /// in flight is not tracked here — harmless, the Student just answers
    /// again) and returns any cached report immediately. Callers show the
    /// cached value right away and let the queued refresh update it later.
    pub fn activity_view(&self, student: &str) -> Result<(Option<String>, Option<Value>)> {
        let cached = self.activity_cached(student);
        {
            let st = self.0.lock().unwrap();
            anyhow::ensure!(st.peers.contains_key(student), "Student is offline");
            anyhow::ensure!(
                st.capabilities.get(student).is_some_and(|c| c["activity"] == 1),
                "A selected Student runs an older build without activity monitoring, or is not on Windows; update it via Classroom first"
            );
        }
        if !self.activity_monitoring() {
            return Ok((None, cached.or(Some(json!({"supported":true,"monitoring_enabled":false})))));
        }
        let id = nonce()?;
        let token = nonce()?;
        self.fs_register_slot(&id, student, &token, None, true);
        let mut request = packet("activity");
        request.id.clone_from(&id);
        request.token = token;
        self.fs_enqueue(student, request);
        Ok((Some(id), cached))
    }
}

struct Slot(Arc<AtomicUsize>);
impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
/// Uploads whatever the background collector most recently produced (or a
/// "monitoring is off" sentinel); never scans synchronously here.
pub(super) fn run(
    address: SocketAddrV4,
    identity: String,
    p: ToolPacket,
    active: &Arc<AtomicUsize>,
    live: &super::super::activity::collector::Live,
    monitoring: bool,
) -> bool {
    if active.fetch_add(1, Ordering::AcqRel) >= MAX_CONCURRENT {
        active.fetch_sub(1, Ordering::AcqRel);
        return false;
    }
    let slot = Slot(Arc::clone(active));
    let live = Arc::clone(live);
    thread::spawn(move || {
        let _slot = slot;
        let outcome = (|| -> Result<()> {
            let report = if monitoring {
                live.lock().unwrap().clone()
            } else {
                None
            };
            let body = serde_json::to_string(&report.unwrap_or_else(|| {
                let mut r = super::super::activity::Report {
                    supported: true,
                    ..Default::default()
                };
                r.monitoring_enabled = monitoring;
                r
            }))?;
            let mut s = TcpStream::connect_timeout(&address.into(), Duration::from_secs(2))?;
            configure(&s)?;
            let mut hello = packet("fs_result");
            hello.id.clone_from(&p.id);
            hello.token.clone_from(&p.token);
            hello.student = identity;
            hello.size = u64::try_from(body.len())?;
            send(&mut s, &hello)?;
            anyhow::ensure!(read(&mut s)?.action == "ready", "Activity report rejected");
            s.write_all(body.as_bytes())?;
            anyhow::ensure!(read(&mut s)?.action == "ok", "Activity report not accepted");
            Ok(())
        })();
        if let Err(error) = outcome {
            tracing::info!(%error, "Student activity report failed");
        }
    });
    true
}

/// Per-Student collector lifecycle, owned by the Student's poll loop: starts
/// when the Teacher switches monitoring on, stops when it switches off or the
/// loop ends.
pub(super) struct Lifecycle {
    history: Arc<super::super::activity::history::History>,
    live: super::super::activity::collector::Live,
    collector: Option<super::super::activity::collector::Collector>,
}
impl Lifecycle {
    pub fn new() -> Self {
        Self {
            history: Arc::default(),
            live: Arc::new(Mutex::new(None)),
            collector: None,
        }
    }
    pub fn live(&self) -> &super::super::activity::collector::Live {
        &self.live
    }
    pub fn set_enabled(&mut self, enabled: bool) {
        match (enabled, self.collector.is_some()) {
            (true, false) => {
                self.collector = Some(super::super::activity::collector::Collector::start(
                    Arc::clone(&self.history),
                    Arc::clone(&self.live),
                ));
            }
            (false, true) => {
                self.collector = None;
                *self.live.lock().unwrap() = None;
            }
            _ => {}
        }
    }
}
