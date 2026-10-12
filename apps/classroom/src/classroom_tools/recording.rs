//! Local Student recording and separately paced, authenticated chunk collection.
//! No frames or file bytes traverse Flutter, the tools poll, or the media runtime.
mod mux;
mod student;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub(crate) use self::windows::early_operation;
pub(super) use student::Student;

use super::{configure, nonce, packet, read, safe_name, send, sha256_file, Hub};
use anyhow::{Context, Result};
use protocol::ToolPacket;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

pub(super) const CHUNK_SECONDS: u64 = 600;
pub(super) const MAX_SECONDS: u64 = 8 * 60 * 60;
pub(super) const MAX_CHUNKS: u64 = MAX_SECONDS / CHUNK_SECONDS + 1;
const MAX_CHUNK_BYTES: u64 = 1024 * 1024 * 1024;
const BLOCK: usize = 64 * 1024;
const PACE: Duration = Duration::from_millis(6); // <=10.42 MiB/s per slot, two globally.

#[derive(Default)]
pub(super) struct TeacherState {
    path: Option<PathBuf>,
    sessions: Vec<Session>,
    active_uploads: usize,
}
#[derive(Serialize, Deserialize)]
struct Session {
    id: String,
    folder: PathBuf,
    created: u64,
    stop: bool,
    targets: HashMap<String, Target>,
}
#[derive(Serialize, Deserialize)]
struct Target {
    name: String,
    folder: PathBuf,
    token: String,
    #[serde(default)]
    report: Value,
    #[serde(default)]
    received: HashMap<u64, Receipt>,
    #[serde(skip)]
    seen: Option<Instant>,
    #[serde(skip)]
    requested: Option<Instant>,
    #[serde(skip)]
    uploading: bool,
    #[serde(skip)]
    progress: u64,
}
#[derive(Clone, Serialize, Deserialize)]
struct Receipt {
    size: u64,
    hash: String,
}

pub(super) fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    let mut file = File::create(&tmp)?;
    file.write_all(&serde_json::to_vec(value)?)?;
    file.sync_all()?;
    drop(file);
    replace(&tmp, path)?;
    Ok(())
}
#[cfg(not(windows))]
fn replace(from: &Path, to: &Path) -> Result<()> {
    fs::rename(from, to)?;
    Ok(())
}
#[cfg(windows)]
#[allow(unsafe_code)]
fn replace(from: &Path, to: &Path) -> Result<()> {
    use ::windows::core::PCWSTR;
    use ::windows::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    use std::os::windows::ffi::OsStrExt;
    let a: Vec<_> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let b: Vec<_> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: both paths are owned NUL-terminated buffers alive for this call.
    unsafe {
        MoveFileExW(
            PCWSTR(a.as_ptr()),
            PCWSTR(b.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )?;
    }
    Ok(())
}
fn save(state: &TeacherState) -> Result<()> {
    if let Some(path) = &state.path {
        atomic_json(path, &state.sessions)?;
    }
    Ok(())
}
fn epoch() -> u64 {
    media_core::unix_us() / 1_000_000
}
fn chunk_name(index: u64) -> Result<String> {
    anyhow::ensure!((1..=MAX_CHUNKS).contains(&index), "Invalid recording chunk");
    Ok(format!("Part-{index:04}.mp4"))
}
fn authenticated<'a>(state: &'a mut TeacherState, p: &ToolPacket) -> Result<&'a mut Target> {
    let session = state
        .sessions
        .iter_mut()
        .find(|s| s.id == p.id)
        .context("Unknown recording")?;
    let target = session
        .targets
        .get_mut(&p.student)
        .context("Unselected Student")?;
    anyhow::ensure!(
        !p.token.is_empty() && target.token == p.token,
        "Invalid recording token"
    );
    Ok(target)
}
impl Hub {
    pub fn load_recordings(&self, path: PathBuf) -> Result<()> {
        let sessions: Vec<Session> = if path.exists() {
            anyhow::ensure!(
                fs::metadata(&path)?.len() <= 32 * 1024 * 1024,
                "Recording journal too large"
            );
            serde_json::from_slice(&fs::read(&path)?)?
        } else {
            Vec::new()
        };
        anyhow::ensure!(
            sessions.len() <= 32 && sessions.iter().all(|s| s.targets.len() <= 64),
            "Invalid recording journal"
        );
        let mut state = self.7.lock().unwrap();
        state.path = Some(path);
        state.sessions = sessions;
        Ok(())
    }
    pub fn recording_start(&self, recipients: &[(String, String)]) -> Result<String> {
        let desktop = super::user_folder("Desktop")?;
        self.recording_start_at(recipients, &desktop)
    }
    fn recording_start_at(
        &self,
        recipients: &[(String, String)],
        desktop: &Path,
    ) -> Result<String> {
        anyhow::ensure!(
            !recipients.is_empty() && recipients.len() <= 64,
            "Select 1–64 online Students"
        );
        let peers = self.0.lock().unwrap();
        anyhow::ensure!(
            recipients.iter().all(|(id, _)| peers.peers.contains_key(id)
                && peers
                    .capabilities
                    .get(id)
                    .is_some_and(|c| c["recording"] == 1)),
            "Recording requires an updated Windows Student on every selected computer"
        );
        let mut state = self.7.lock().unwrap();
        if state.sessions.len() >= 32 {
            let old = state
                .sessions
                .iter()
                .position(|s| {
                    s.targets.values().all(|t| {
                        matches!(
                            t.report["phase"].as_str(),
                            Some("stopped" | "error" | "interrupted")
                        ) && !t.uploading
                            && t.report["queued_chunks"].as_u64() == Some(0)
                            && t.report["completed_chunks"].as_u64().unwrap_or(0)
                                <= t.received.len() as u64
                    })
                })
                .context("Too many pending recording sessions; finish existing transfers first")?;
            state.sessions.remove(old); // Recordings on Desktop are retained.
        }
        anyhow::ensure!(
            !state.sessions.iter().any(|s| !s.stop
                && epoch() < s.created + MAX_SECONDS
                && recipients
                    .iter()
                    .any(|(id, _)| s.targets.get(id).is_some_and(|t| !matches!(
                        t.report["phase"].as_str(),
                        Some("stopped" | "error" | "interrupted")
                    )))),
            "A selected Student already has a recording session; stop it first"
        );
        let id = nonce()?;
        let root = if recipients.len() > 1 {
            desktop.join(format!("Classroom Recordings-{}-{}", epoch(), &id[..8]))
        } else {
            desktop.to_owned()
        };
        let mut used = std::collections::HashSet::new();
        let mut targets = HashMap::new();
        for (student, name) in recipients {
            let mut folder = safe_name(name);
            if !used.insert(folder.to_ascii_lowercase()) {
                folder = format!("{folder}-{}", targets.len() + 1);
                while !used.insert(folder.to_ascii_lowercase()) {
                    folder.push('_');
                }
            }
            // A single session lives inside Desktop/<Student>/<session> so repeated
            // recordings cannot overwrite Part-0001 from an earlier session.
            let folder = if recipients.len() == 1 {
                root.join(folder)
                    .join(format!("Recording-{}-{}", epoch(), &id[..8]))
            } else {
                root.join(folder)
            };
            fs::create_dir_all(&folder)
                .context("Cannot create recording folder on Teacher Desktop")?;
            targets.insert(
                student.clone(),
                Target {
                    name: name.clone(),
                    folder,
                    token: nonce()?,
                    report: json!({"phase":"pending"}),
                    received: HashMap::new(),
                    seen: None,
                    requested: None,
                    uploading: false,
                    progress: 0,
                },
            );
        }
        let folder = if recipients.len() == 1 {
            targets.values().next().unwrap().folder.clone()
        } else {
            root
        };
        state.sessions.push(Session {
            id: id.clone(),
            folder,
            created: epoch(),
            stop: false,
            targets,
        });
        if let Err(e) = save(&state) {
            state.sessions.pop();
            return Err(e);
        }
        Ok(id)
    }
    pub fn recording_stop(&self, id: &str) -> Result<()> {
        let mut state = self.7.lock().unwrap();
        let session = state
            .sessions
            .iter_mut()
            .find(|s| s.id == id)
            .context("Unknown recording")?;
        session.stop = true;
        for target in session.targets.values_mut() {
            target.requested = None;
        }
        save(&state)
    }
    pub fn recording_folder(&self, id: &str) -> Result<PathBuf> {
        self.7
            .lock()
            .unwrap()
            .sessions
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.folder.clone())
            .context("Unknown recording")
    }
    pub(super) fn recording_poll(&self, student: &str) -> Option<ToolPacket> {
        let mut state = self.7.lock().unwrap();
        for session in &mut state.sessions {
            let Some(target) = session.targets.get_mut(student) else {
                continue;
            };
            let expired = epoch() >= session.created + MAX_SECONDS;
            let stop = session.stop || expired;
            let phase = target.report["phase"].as_str().unwrap_or("pending");
            let terminal = matches!(phase, "stopped" | "error" | "interrupted");
            // Do not restart acknowledged capture after a transient disconnect.
            if terminal || (!stop && phase == "recording") {
                continue;
            }
            if target
                .requested
                .is_some_and(|t| t.elapsed() < Duration::from_secs(5))
            {
                continue;
            }
            target.requested = Some(Instant::now());
            let mut p = packet(if stop {
                "recording_stop"
            } else {
                "recording_start"
            });
            p.id.clone_from(&session.id);
            p.token.clone_from(&target.token);
            p.text = json!({"expires":session.created + MAX_SECONDS}).to_string();
            return Some(p);
        }
        None
    }
    pub(super) fn recording_snapshot(&self, peers: &HashMap<String, u64>) -> Value {
        let state = self.7.lock().unwrap();
        json!(state.sessions.iter().rev().map(|s| json!({"id":s.id,"folder":s.folder,"created":s.created,"stop":s.stop,
            "targets":s.targets.iter().map(|(id,t)| json!({"id":id,"name":t.name,"folder":t.folder,
                "online":peers.contains_key(id),"stale":t.seen.is_none_or(|at|at.elapsed()>Duration::from_secs(10)),
                "report":t.report,"received":t.received.len(),"uploading":t.uploading,"upload_bytes":t.progress,
                "received_bytes":t.received.values().map(|r|r.size).sum::<u64>()})).collect::<Vec<_>>() })).collect::<Vec<_>>())
    }
}

pub(super) fn serve(s: &mut TcpStream, p: &ToolPacket, hub: &Hub) -> Option<Result<()>> {
    Some(match p.action.as_str() {
        "recording_status" => status(s, p, hub),
        "recording_upload" => upload(s, p, hub),
        _ => return None,
    })
}
impl Hub {
    pub(super) fn recording_status(&self, student: &str, p: &ToolPacket) -> Result<bool> {
        anyhow::ensure!(p.text.len() <= 4096, "Recording status too large");
        let report: Value = serde_json::from_str(&p.text)?;
        anyhow::ensure!(
            matches!(
                report["phase"].as_str(),
                Some("recording" | "stopped" | "error" | "interrupted" | "starting")
            ),
            "Invalid recording phase"
        );
        let mut trusted = p.clone();
        student.clone_into(&mut trusted.student);
        let mut state = self.7.lock().unwrap();
        if !state.sessions.iter().any(|s| s.id == p.id) {
            return Ok(true);
        }
        let target = authenticated(&mut state, &trusted)?;
        target.report = report;
        target.seen = Some(Instant::now());
        let session = state.sessions.iter().find(|s| s.id == p.id).unwrap();
        Ok(session.stop || epoch() >= session.created + MAX_SECONDS)
    }
}
fn status(s: &mut TcpStream, p: &ToolPacket, hub: &Hub) -> Result<()> {
    let stop = hub.recording_status(&p.student, p)?;
    let mut ack = packet("ok");
    if stop {
        ack.status = "stop".into();
    }
    send(s, &ack)
}
struct UploadSlot<'a> {
    hub: &'a Hub,
    id: String,
    student: String,
}
impl Drop for UploadSlot<'_> {
    fn drop(&mut self) {
        let mut state = self.hub.7.lock().unwrap();
        state.active_uploads = state.active_uploads.saturating_sub(1);
        if let Some(t) = state
            .sessions
            .iter_mut()
            .find(|s| s.id == self.id)
            .and_then(|s| s.targets.get_mut(&self.student))
        {
            t.uploading = false;
        }
    }
}
fn upload(s: &mut TcpStream, p: &ToolPacket, hub: &Hub) -> Result<()> {
    let index = p.done;
    let name = chunk_name(index)?;
    anyhow::ensure!(
        p.size > 0
            && p.size <= MAX_CHUNK_BYTES
            && p.hash.len() == 64
            && p.hash.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid recording chunk metadata"
    );
    let (folder, prior) = {
        let mut state = hub.7.lock().unwrap();
        let t = authenticated(&mut state, p)?;
        let prior = t.received.get(&index).cloned();
        if let Some(receipt) = &prior {
            anyhow::ensure!(
                receipt.size == p.size && receipt.hash == p.hash,
                "Recording chunk conflicts with received file"
            );
        }
        let folder = t.folder.clone();
        let busy = t.uploading || state.active_uploads >= 2;
        if busy {
            drop(state);
            return send(s, &packet("busy"));
        }
        state.active_uploads += 1;
        let t = authenticated(&mut state, p)?;
        t.uploading = true;
        t.progress = 0;
        (folder, prior)
    };
    let _slot = UploadSlot {
        hub,
        id: p.id.clone(),
        student: p.student.clone(),
    };
    if prior.is_some()
        && super::same_content(
            &folder.join(&name),
            p.size,
            &p.hash,
            &AtomicBool::new(false),
        )
    {
        return send(s, &packet("complete"));
    }
    anyhow::ensure!(
        fs2::available_space(&folder)? > p.size + 256 * 1024 * 1024,
        "Teacher Desktop disk is full"
    );
    let destination = folder.join(&name);
    let temporary = folder.join(format!(".{name}.partial"));
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)?;
    send(s, &packet("ready"))?;
    let outcome = (|| -> Result<()> {
        let mut left = p.size;
        let mut hash = Sha256::new();
        let mut buffer = vec![0; BLOCK];
        while left > 0 {
            let n = usize::try_from(left.min(BLOCK as u64))?;
            s.read_exact(&mut buffer[..n])?;
            file.write_all(&buffer[..n])?;
            hash.update(&buffer[..n]);
            left -= n as u64;
            authenticated(&mut hub.7.lock().unwrap(), p)?.progress = p.size - left;
        }
        anyhow::ensure!(
            format!("{:x}", hash.finalize()) == p.hash,
            "Recording checksum mismatch"
        );
        file.sync_all()?;
        drop(file);
        // Owned session paths only; a retry can replace its own unacknowledged chunk.
        replace(&temporary, &destination)?;
        let mut state = hub.7.lock().unwrap();
        let previous = authenticated(&mut state, p)?.received.insert(
            index,
            Receipt {
                size: p.size,
                hash: p.hash.clone(),
            },
        );
        if let Err(error) = save(&state) {
            let target = authenticated(&mut state, p)?;
            if let Some(previous) = previous {
                target.received.insert(index, previous);
            } else {
                target.received.remove(&index);
            }
            return Err(error);
        }
        drop(state);
        send(s, &packet("complete"))
    })();
    if outcome.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_auth_stop_and_reconnect() {
        let hub = Hub::default();
        let root = std::env::temp_dir().join(format!("recording-test-{}", nonce().unwrap()));
        fs::create_dir_all(&root).unwrap();
        assert!(hub
            .recording_start_at(&[("a".into(), "PC".into())], &root)
            .is_err());
        {
            let mut st = hub.0.lock().unwrap();
            for id in ["a", "b"] {
                st.peers.insert(id.into(), 1);
                st.capabilities.insert(id.into(), json!({"recording":1}));
            }
        }
        hub.load_recordings(root.join("journal.json")).unwrap();
        let id = hub
            .recording_start_at(
                &[("a".into(), "PC".into()), ("b".into(), "pc".into())],
                &root,
            )
            .unwrap();
        let mut request = hub.recording_poll("a").unwrap();
        assert_eq!(request.action, "recording_start");
        request.student = "a".into();
        assert!(authenticated(&mut hub.7.lock().unwrap(), &request).is_ok());
        request.token = "bad".into();
        assert!(authenticated(&mut hub.7.lock().unwrap(), &request).is_err());
        let snapshot = hub.recording_snapshot(&HashMap::new());
        assert_ne!(
            snapshot[0]["targets"][0]["folder"],
            snapshot[0]["targets"][1]["folder"]
        );
        assert!(snapshot[0]["targets"][0]["stale"].as_bool().unwrap());
        assert!(hub
            .recording_start_at(&[("a".into(), "PC".into())], &root)
            .is_err());
        hub.recording_stop(&id).unwrap();
        assert_eq!(hub.recording_poll("a").unwrap().action, "recording_stop");
        let reloaded = Hub::default();
        reloaded.load_recordings(root.join("journal.json")).unwrap();
        assert_eq!(
            reloaded.recording_poll("a").unwrap().action,
            "recording_stop"
        );
        assert!(chunk_name(0).is_err());
        assert!(chunk_name(MAX_CHUNKS + 1).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    fn transport_fixture() -> (std::sync::Arc<Hub>, PathBuf, student::Metadata) {
        let hub = std::sync::Arc::new(Hub::default());
        let root = std::env::temp_dir().join(format!("record-transport-{}", nonce().unwrap()));
        fs::create_dir_all(&root).unwrap();
        {
            let mut state = hub.0.lock().unwrap();
            state.peers.insert("s".into(), 1);
            state
                .capabilities
                .insert("s".into(), json!({"recording":1}));
        }
        hub.load_recordings(root.join("journal.json")).unwrap();
        let id = hub
            .recording_start_at(&[("s".into(), "PC01".into())], &root)
            .unwrap();
        let p = hub.recording_poll("s").unwrap();
        let meta = student::Metadata {
            id,
            token: p.token,
            identity: "s".into(),
            address: "127.0.0.1:1".parse().unwrap(),
            expires: epoch() + 600,
        };
        (hub, root, meta)
    }
    fn endpoint(
        hub: std::sync::Arc<Hub>,
        meta: &mut student::Metadata,
    ) -> std::thread::JoinHandle<Result<()>> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        meta.address = match listener.local_addr().unwrap() {
            std::net::SocketAddr::V4(a) => a,
            std::net::SocketAddr::V6(_) => unreachable!(),
        };
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept()?;
            configure(&s)?;
            let p = read(&mut s)?;
            serve(&mut s, &p, &hub).unwrap()
        })
    }
    #[test]
    fn chunk_disconnect_retry_dedup_and_persisted_receipt() {
        let (hub, root, mut meta) = transport_fixture();
        let src = root.join("source.mp4");
        let bytes = vec![42u8; 128 * 1024];
        fs::write(&src, &bytes).unwrap();
        let server = endpoint(std::sync::Arc::clone(&hub), &mut meta);
        let mut socket = TcpStream::connect(meta.address).unwrap();
        configure(&socket).unwrap();
        let mut hello = packet("recording_upload");
        hello.id = meta.id.clone();
        hello.student = meta.identity.clone();
        hello.token = meta.token.clone();
        hello.done = 1;
        hello.size = bytes.len() as u64;
        hello.hash = sha256_file(&src, &AtomicBool::new(false)).unwrap();
        send(&mut socket, &hello).unwrap();
        assert_eq!(read(&mut socket).unwrap().action, "ready");
        socket.write_all(&bytes[..7]).unwrap();
        // Status and snapshots stay live while an upload is blocked on receive.
        let mut status = hello.clone();
        status.text = json!({"phase":"recording","fps":28.2}).to_string();
        hub.recording_status("s", &status).unwrap();
        assert_eq!(
            hub.recording_snapshot(&HashMap::new())[0]["targets"][0]["report"]["fps"],
            28.2
        );
        drop(socket);
        assert!(server.join().unwrap().is_err());
        let dest = hub
            .recording_folder(&meta.id)
            .unwrap()
            .join("Part-0001.mp4");
        assert!(!dest.exists());
        assert!(!dest.with_file_name(".Part-0001.mp4.partial").exists());
        assert!(src.exists());
        let server = endpoint(std::sync::Arc::clone(&hub), &mut meta);
        assert!(student::transfer(&meta, &src, 1, &AtomicBool::new(false)).unwrap());
        server.join().unwrap().unwrap();
        assert_eq!(fs::read(&dest).unwrap(), bytes);
        // Simulate lost final ACK + Teacher restart. A matching received chunk is
        // acknowledged idempotently without retransmitting or duplicating it.
        let reloaded = std::sync::Arc::new(Hub::default());
        reloaded.load_recordings(root.join("journal.json")).unwrap();
        let server = endpoint(std::sync::Arc::clone(&reloaded), &mut meta);
        assert!(student::transfer(&meta, &src, 1, &AtomicBool::new(false)).unwrap());
        server.join().unwrap().unwrap();
        assert_eq!(
            reloaded.recording_snapshot(&HashMap::new())[0]["targets"][0]["received"],
            1
        );
        // Same length alone cannot authorize Student cleanup if destination was damaged.
        fs::write(&dest, vec![7u8; bytes.len()]).unwrap();
        let server = endpoint(std::sync::Arc::clone(&reloaded), &mut meta);
        assert!(student::transfer(&meta, &src, 1, &AtomicBool::new(false)).unwrap());
        server.join().unwrap().unwrap();
        assert_eq!(fs::read(dest).unwrap(), bytes);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn checksum_failure_and_global_slots_preserve_student_file() {
        let (hub, root, mut meta) = transport_fixture();
        let src = root.join("source.mp4");
        fs::write(&src, b"abc").unwrap();
        hub.7.lock().unwrap().active_uploads = 2;
        let server = endpoint(std::sync::Arc::clone(&hub), &mut meta);
        assert!(!student::transfer(&meta, &src, 1, &AtomicBool::new(false)).unwrap());
        server.join().unwrap().unwrap();
        assert_eq!(fs::read(&src).unwrap(), b"abc");
        hub.7.lock().unwrap().active_uploads = 0;
        let server = endpoint(std::sync::Arc::clone(&hub), &mut meta);
        let mut s = TcpStream::connect(meta.address).unwrap();
        configure(&s).unwrap();
        let mut p = packet("recording_upload");
        p.id = meta.id.clone();
        p.token = meta.token.clone();
        p.student = meta.identity.clone();
        p.size = 3;
        p.done = 1;
        p.hash = "0".repeat(64);
        p.name = "../../untrusted.mp4".into(); // Never used as a Teacher destination.
        send(&mut s, &p).unwrap();
        assert_eq!(read(&mut s).unwrap().action, "ready");
        s.write_all(b"abc").unwrap();
        drop(s);
        assert!(server
            .join()
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("checksum"));
        assert_eq!(
            hub.recording_snapshot(&HashMap::new())[0]["targets"][0]["received"],
            0
        );
        assert!(src.exists());
        assert!(!hub
            .recording_folder(&meta.id)
            .unwrap()
            .join("Part-0001.mp4")
            .exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn receipt_persistence_failure_never_authorizes_source_cleanup() {
        let (hub, root, mut meta) = transport_fixture();
        let src = root.join("source.mp4");
        fs::write(&src, b"video").unwrap();
        hub.7.lock().unwrap().path = Some(root.join("missing/journal.json"));
        let server = endpoint(std::sync::Arc::clone(&hub), &mut meta);
        assert!(student::transfer(&meta, &src, 1, &AtomicBool::new(false)).is_err());
        assert!(server.join().unwrap().is_err());
        assert!(src.exists());
        assert_eq!(
            hub.recording_snapshot(&HashMap::new())[0]["targets"][0]["received"],
            0
        );
        hub.7.lock().unwrap().path = Some(root.join("journal.json"));
        let server = endpoint(std::sync::Arc::clone(&hub), &mut meta);
        assert!(student::transfer(&meta, &src, 1, &AtomicBool::new(false)).unwrap());
        server.join().unwrap().unwrap();
        assert_eq!(
            hub.recording_snapshot(&HashMap::new())[0]["targets"][0]["received"],
            1
        );
        fs::remove_dir_all(root).unwrap();
    }
}
