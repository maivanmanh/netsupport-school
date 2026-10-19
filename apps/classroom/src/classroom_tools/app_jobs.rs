//! Teacher-requested "which installed applications does this Student have"
//! inventory. Scanned fresh on each request, then uploaded over the same
//! generic channel directory listings and activity reports use (`fs_result`).
//! The request body names the ids whose native icons the Teacher still lacks,
//! so icons travel once, from one computer, instead of from every Student.
use super::{configure, packet, read, send, Hub};
use anyhow::Result;
use protocol::ToolPacket;
use std::{
    io::Write,
    net::{SocketAddrV4, TcpStream},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

const MAX_CONCURRENT: usize = 4;

impl Hub {
    pub fn app_list(&self, student: &str, icons_for: &[String]) -> Result<String> {
        {
            let st = self.0.lock().unwrap();
            anyhow::ensure!(st.peers.contains_key(student), "Student is offline");
            anyhow::ensure!(
                st.capabilities.get(student).is_some_and(|c| c["apps"] == 1),
                "A selected Student runs an older build without application control, or is not on Windows; update it via Classroom first"
            );
        }
        let id = super::nonce()?;
        let token = super::nonce()?;
        self.fs_register_slot(&id, student, &token, None, false);
        let mut request = packet("app_list");
        request.text = serde_json::json!({"icons_for": icons_for}).to_string();
        request.id.clone_from(&id);
        request.token = token;
        self.fs_enqueue(student, request);
        Ok(id)
    }
}

struct Slot(Arc<AtomicUsize>);
impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
pub(super) fn run(
    address: SocketAddrV4,
    identity: String,
    p: ToolPacket,
    active: &Arc<AtomicUsize>,
) -> bool {
    if active.fetch_add(1, Ordering::AcqRel) >= MAX_CONCURRENT {
        active.fetch_sub(1, Ordering::AcqRel);
        return false;
    }
    let slot = Slot(Arc::clone(active));
    thread::spawn(move || {
        let _slot = slot;
        let outcome = (|| -> Result<()> {
            let request: crate::app_control::ListRequest =
                serde_json::from_str(&p.text).unwrap_or_default();
            let report = crate::app_control::list_apps(&request);
            let body = serde_json::to_string(&report)?;
            let mut s = TcpStream::connect_timeout(&address.into(), Duration::from_secs(2))?;
            configure(&s)?;
            let mut hello = packet("fs_result");
            hello.id.clone_from(&p.id);
            hello.token.clone_from(&p.token);
            hello.student = identity;
            hello.size = u64::try_from(body.len())?;
            send(&mut s, &hello)?;
            anyhow::ensure!(read(&mut s)?.action == "ready", "Application list rejected");
            s.write_all(body.as_bytes())?;
            anyhow::ensure!(
                read(&mut s)?.action == "ok",
                "Application list not accepted"
            );
            Ok(())
        })();
        if let Err(error) = outcome {
            tracing::info!(%error, "Student application list upload failed");
        }
    });
    true
}
