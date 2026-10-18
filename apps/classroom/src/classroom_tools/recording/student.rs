use super::{
    atomic_json, configure, epoch, packet, read, send, sha256_file, RecordingOptions, BLOCK,
    MAX_CHUNK_BYTES, MAX_SECONDS, PACE,
};
use anyhow::{Context, Result};
use protocol::ToolPacket;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{Read, Write},
    net::{SocketAddrV4, TcpStream},
    path::{Path, PathBuf},
    process::Child,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    sync::{
        mpsc::{self, SyncSender},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
#[derive(Serialize, Deserialize)]
pub(super) struct Metadata {
    pub id: String,
    pub token: String,
    pub identity: String,
    pub address: SocketAddrV4,
    pub expires: u64,
    /// How to record; absent in sessions started before the Teacher chose these.
    #[serde(default)]
    pub options: RecordingOptions,
}
/// Largest Teacher-clock lead over this Student's clock that is still accepted.
const CLOCK_SKEW_SECONDS: u64 = 600;
pub(crate) struct Student {
    tx: SyncSender<ToolPacket>,
    contact: Arc<AtomicU64>,
    reports: Arc<Mutex<HashMap<String, ToolPacket>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Student {
    pub fn new(root: PathBuf, address: SocketAddrV4, identity: String) -> Self {
        let (tx, rx) = mpsc::sync_channel::<ToolPacket>(4);
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let reports = Arc::new(Mutex::new(HashMap::new()));
        let worker_reports = Arc::clone(&reports);
        let contact = Arc::new(AtomicU64::new(epoch()));
        let worker_contact = Arc::clone(&contact);
        let worker = thread::spawn(move || {
            let _ = fs::create_dir_all(&root);
            let mut active: Option<(String, PathBuf, Child)> = None;
            // Seconds of silence from the Teacher after which the capture ends (0 = never).
            let mut gone_seconds = RecordingOptions::default().gone_seconds;
            // A restarted Agent resumes transfers, never silently restarts capture.
            if let Ok(dirs) = fs::read_dir(&root) {
                for dir in dirs.flatten().take(32) {
                    let path = dir.path();
                    let status = load_json(&path.join("status.json")).unwrap_or_default();
                    if matches!(status["phase"].as_str(), Some("recording" | "starting")) {
                        let _ = fs::write(path.join("stop"), b"");
                        let _ = atomic_json(
                            &path.join("status.json"),
                            &json!({"phase":"interrupted","error":"Student restarted; completed chunks remain queued"}),
                        );
                    }
                }
            }
            let sender_flag = Arc::clone(&flag);
            let sender_root = root.clone();
            let sender_identity = identity.clone();
            let error_reports = Arc::clone(&worker_reports);
            let sender = thread::spawn(move || {
                transfer_loop(
                    &sender_root,
                    address,
                    &sender_identity,
                    &sender_flag,
                    worker_reports,
                );
            });
            let mut heartbeat = Instant::now()
                .checked_sub(Duration::from_secs(2))
                .unwrap_or_else(Instant::now);
            while !flag.load(Ordering::Acquire) {
                if let Ok(p) = rx.recv_timeout(Duration::from_millis(100)) {
                    let result = (|| -> Result<()> {
                        anyhow::ensure!(
                            p.id.len() == 32
                                && p.id.bytes().all(|b| b.is_ascii_hexdigit())
                                && p.token.len() == 32,
                            "Invalid recording command"
                        );
                        let dir = root.join(&p.id);
                        if dir.join("meta.json").exists() {
                            let meta: Metadata =
                                serde_json::from_slice(&fs::read(dir.join("meta.json"))?)?;
                            anyhow::ensure!(
                                meta.token == p.token
                                    && meta.identity == identity
                                    && meta.address == address,
                                "Recording command belongs to another session"
                            );
                            if p.action == "recording_stop" || p.action == "recording_discard" {
                                fs::write(dir.join("stop"), b"")?;
                            }
                            if p.action == "recording_discard" {
                                // Removed (with anything not yet uploaded) once capture has ended.
                                fs::write(dir.join("discard"), b"")?;
                            }
                            return Ok(()); // Retried Start never creates a second helper.
                        }
                        if p.action == "recording_discard" {
                            return Ok(()); // Nothing of it here.
                        }
                        if p.action == "recording_stop" {
                            fs::create_dir_all(&dir)?;
                            atomic_json(
                                &dir.join("meta.json"),
                                &Metadata {
                                    id: p.id.clone(),
                                    token: p.token.clone(),
                                    identity: identity.clone(),
                                    address,
                                    expires: epoch(),
                                    options: RecordingOptions::default(),
                                },
                            )?;
                            atomic_json(
                                &dir.join("status.json"),
                                &json!({"phase":"stopped","completed_chunks":0}),
                            )?;
                            return Ok(()); // Stop reached Student before Start.
                        }
                        anyhow::ensure!(active.is_none(), "Another recording is still running");
                        prune_completed(&root)?;
                        anyhow::ensure!(
                            fs::read_dir(&root)?.count() < 32,
                            "Student recording spool limit reached"
                        );
                        let v: Value = serde_json::from_str(&p.text)?;
                        let expires = v["expires"]
                            .as_u64()
                            .context("Missing recording deadline")?;
                        let options = serde_json::from_value::<RecordingOptions>(v["options"].clone())
                            .unwrap_or_default()
                            .sanitized();
                        // The deadline is the Teacher's clock plus the longest recording.
                        // Student clocks drift by seconds (or minutes on an unsynced lab
                        // PC), so the upper bound must not reject a Teacher that is
                        // merely ahead of this Student.
                        anyhow::ensure!(
                            expires > epoch() && expires <= epoch() + MAX_SECONDS + CLOCK_SKEW_SECONDS,
                            "Invalid recording deadline"
                        );
                        fs::create_dir(&dir)?;
                        atomic_json(
                            &dir.join("meta.json"),
                            &Metadata {
                                id: p.id.clone(),
                                token: p.token.clone(),
                                identity: identity.clone(),
                                address,
                                expires,
                                options: options.clone(),
                            },
                        )?;
                        gone_seconds = options.gone_seconds;
                        atomic_json(&dir.join("status.json"), &json!({"phase":"starting"}))?;
                        atomic_json(&dir.join("heartbeat.json"), &epoch())?;
                        match spawn(&dir) {
                            Ok(child) => active = Some((p.id.clone(), dir, child)),
                            Err(e) => {
                                atomic_json(
                                    &dir.join("status.json"),
                                    &json!({"phase":"error","error":format!("{e:#}")}),
                                )?;
                            }
                        }
                        Ok(())
                    })();
                    if let Err(e) = result {
                        tracing::warn!(error=%e,"Recording command failed");
                        let mut status = p.clone();
                        status.action = "recording_status".into();
                        status.student.clone_from(&identity);
                        status.text = json!({"phase":"error","error":format!("{e:#}").chars().take(300).collect::<String>()}).to_string();
                        error_reports.lock().unwrap().insert(p.id.clone(), status);
                    }
                }
                if heartbeat.elapsed() >= Duration::from_secs(1) {
                    heartbeat = Instant::now();
                    if let Some((_, dir, _)) = &active {
                        let _ = atomic_json(&dir.join("heartbeat.json"), &epoch());
                        if gone_seconds > 0
                            && epoch().saturating_sub(worker_contact.load(Ordering::Acquire))
                                > gone_seconds
                            && !dir.join("stop").exists()
                        {
                            let _ = fs::write(dir.join("stop"), b"teacher_unreachable");
                        }
                    }
                }
                if let Some((_, dir, child)) = active.as_mut() {
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            if !status.success() {
                                let _ = atomic_json(
                                    &dir.join("status.json"),
                                    &json!({"phase":"error","error":"Recording helper exited unexpectedly; completed chunks remain queued"}),
                                );
                            }
                            active = None;
                        }
                        Err(e) => {
                            tracing::warn!(error=%e,"Recording helper status failed");
                        }
                        Ok(None) => {}
                    }
                }
            }
            if let Some((_, dir, mut child)) = active {
                let _ = fs::write(dir.join("stop"), b"");
                let until = Instant::now() + Duration::from_secs(10);
                while matches!(child.try_wait(), Ok(None)) && Instant::now() < until {
                    thread::sleep(Duration::from_millis(50));
                }
                if matches!(child.try_wait(), Ok(None)) {
                    let _ = child.kill();
                }
                let _ = child.wait();
            }
            let _ = sender.join();
        });
        Self {
            tx,
            contact,
            reports,
            stop,
            worker: Some(worker),
        }
    }
    /// Called while the Teacher connection is alive; resets the unreachable timer.
    pub fn note_contact(&self) {
        self.contact.store(epoch(), Ordering::Release);
    }
    pub fn statuses(&self) -> Vec<ToolPacket> {
        self.reports
            .lock()
            .unwrap()
            .drain()
            .map(|(_, p)| p)
            .collect()
    }
    pub fn command(&self, p: ToolPacket) -> Result<()> {
        self.tx.try_send(p).context("Recording command queue full")
    }
}
impl Drop for Student {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
#[cfg(windows)]
fn spawn(dir: &Path) -> Result<Child> {
    use std::{
        os::windows::process::CommandExt,
        process::{Command, Stdio},
    };
    Command::new(std::env::current_exe()?)
        .arg("--record-screen-internal")
        .arg(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x0800_0000)
        .spawn()
        .context("Cannot start recording helper")
}
#[cfg(not(windows))]
fn spawn(_: &Path) -> Result<Child> {
    anyhow::bail!("Screen recording is supported on Windows Students only")
}
pub(super) fn load_json(path: &Path) -> Result<Value> {
    anyhow::ensure!(
        fs::metadata(path)?.len() <= 16 * 1024,
        "Recording metadata too large"
    );
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
fn report(meta: &Metadata, dir: &Path, error: &str) -> Result<ToolPacket> {
    let mut status = load_json(&dir.join("status.json"))?;
    status["upload_error"] = json!(error.chars().take(300).collect::<String>());
    status["queued_chunks"] = json!(fs::read_dir(dir)?
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|v| v == "mp4"))
        .count());
    let mut p = packet("recording_status");
    p.id.clone_from(&meta.id);
    p.token.clone_from(&meta.token);
    p.student.clone_from(&meta.identity);
    p.text = status.to_string();
    Ok(p)
}
pub(super) fn transfer(
    meta: &Metadata,
    path: &Path,
    index: u64,
    stop: &AtomicBool,
) -> Result<bool> {
    let size = fs::metadata(path)?.len();
    anyhow::ensure!(size <= MAX_CHUNK_BYTES, "Recording chunk too large");
    let hash = sha256_file(path, stop)?;
    let mut s = TcpStream::connect_timeout(&meta.address.into(), Duration::from_secs(2))?;
    configure(&s)?;
    s.set_read_timeout(Some(Duration::from_secs(3)))?;
    s.set_write_timeout(Some(Duration::from_secs(3)))?;
    let mut p = packet("recording_upload");
    p.id.clone_from(&meta.id);
    p.token.clone_from(&meta.token);
    p.student.clone_from(&meta.identity);
    p.done = index;
    p.size = size;
    p.hash = hash;
    send(&mut s, &p)?;
    match read(&mut s)?.action.as_str() {
        "complete" => return Ok(true),
        "busy" => return Ok(false),
        "ready" => {}
        _ => anyhow::bail!("Teacher rejected recording chunk"),
    }
    let mut f = File::open(path)?;
    let mut buffer = vec![0; BLOCK];
    loop {
        anyhow::ensure!(
            !stop.load(Ordering::Acquire),
            "Recording upload interrupted"
        );
        let n = f.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        let start = Instant::now();
        s.write_all(&buffer[..n])?;
        thread::sleep(PACE.saturating_sub(start.elapsed()));
    }
    anyhow::ensure!(
        read(&mut s)?.action == "complete",
        "Recording was not acknowledged"
    );
    Ok(true)
}
fn transfer_loop(
    root: &Path,
    address: SocketAddrV4,
    identity: &str,
    stop: &AtomicBool,
    reports: Arc<Mutex<HashMap<String, ToolPacket>>>,
) {
    // A latest-status slot per session feeds the existing tools poll. No extra
    // status connections and no status backlog when the Teacher disconnects.
    let status_stop = Arc::new(AtomicBool::new(false));
    let status_flag = Arc::clone(&status_stop);
    let status_root = root.to_owned();
    let status_identity = identity.to_owned();
    let status_worker = thread::spawn(move || {
        let mut round = 0u64;
        while !status_flag.load(Ordering::Acquire) {
            round += 1;
            if let Ok(entries) = fs::read_dir(&status_root) {
                for e in entries.flatten().take(32) {
                    if status_flag.load(Ordering::Acquire) {
                        break;
                    }
                    let dir = e.path();
                    let Ok(meta) = fs::read(dir.join("meta.json")).and_then(|b| {
                        serde_json::from_slice::<Metadata>(&b).map_err(std::io::Error::other)
                    }) else {
                        continue;
                    };
                    if meta.address != address || meta.identity != status_identity {
                        continue;
                    }
                    let error =
                        fs::read_to_string(dir.join("upload-error.txt")).unwrap_or_default();
                    if let Ok(p) = report(&meta, &dir, &error) {
                        let terminal = load_json(&dir.join("status.json")).is_ok_and(|r| {
                            matches!(
                                r["phase"].as_str(),
                                Some("stopped" | "error" | "interrupted")
                            )
                        });
                        if !terminal || round % 10 == 1 {
                            reports.lock().unwrap().insert(meta.id.clone(), p);
                        }
                    }
                }
            }
            for _ in 0..10 {
                if status_flag.load(Ordering::Acquire) {
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }
        }
    });
    while !stop.load(Ordering::Acquire) {
        if let Ok(entries) = fs::read_dir(root) {
            for e in entries.flatten().take(32) {
                if stop.load(Ordering::Acquire) {
                    break;
                }
                let dir = e.path();
                let Ok(meta) = fs::read(dir.join("meta.json")).and_then(|b| {
                    serde_json::from_slice::<Metadata>(&b).map_err(std::io::Error::other)
                }) else {
                    continue;
                };
                if meta.address != address || meta.identity != identity {
                    continue;
                }
                if dir.join("discard").exists() {
                    // The Teacher deleted this recording. Keep nothing, upload nothing,
                    // but never pull the directory out from under a running capture.
                    let phase = load_json(&dir.join("status.json")).unwrap_or_default();
                    if matches!(
                        phase["phase"].as_str(),
                        Some("stopped" | "error" | "interrupted")
                    ) {
                        let _ = fs::remove_dir_all(&dir);
                    }
                    continue;
                }
                let mut chunks: Vec<_> = fs::read_dir(&dir)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|e| e == "mp4"))
                    .collect();
                chunks.sort();
                // Round robin between sessions. Pending files remain on disk across
                // disconnect/restart; failed chunks retry from zero, memory is bounded.
                if let Some(path) = chunks.first() {
                    let index = path
                        .file_stem()
                        .and_then(|n| n.to_str())
                        .and_then(|n| n.strip_prefix("Part-"))
                        .and_then(|n| n.parse::<u64>().ok());
                    if let Some(index) = index {
                        match transfer(&meta, path, index, stop) {
                            Ok(true) => {
                                let _ = fs::remove_file(path);
                                let _ = fs::remove_file(dir.join("upload-error.txt"));
                            }
                            Ok(false) => {}
                            Err(e) => {
                                let _ = fs::write(
                                    dir.join("upload-error.txt"),
                                    format!("{e:#}").chars().take(300).collect::<String>(),
                                );
                            }
                        }
                    }
                }
            }
        }
        for _ in 0..20 {
            if stop.load(Ordering::Acquire) {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    }
    status_stop.store(true, Ordering::Release);
    let _ = status_worker.join();
}

fn prune_completed(root: &Path) -> Result<()> {
    let pressure = fs::read_dir(root)?.count() >= 32;
    for entry in fs::read_dir(root)?.flatten() {
        let dir = entry.path();
        let Ok(data) = fs::read(dir.join("meta.json")) else {
            continue;
        };
        let Ok(meta) = serde_json::from_slice::<Metadata>(&data) else {
            continue;
        };
        let terminal_age = fs::metadata(dir.join("status.json"))
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .unwrap_or_default();
        if epoch() < meta.expires.saturating_add(24 * 60 * 60)
            && !(pressure && terminal_age >= Duration::from_secs(60))
        {
            continue;
        }
        let report = load_json(&dir.join("status.json")).unwrap_or_default();
        if !matches!(
            report["phase"].as_str(),
            Some("stopped" | "error" | "interrupted")
        ) {
            continue;
        }
        let pending = fs::read_dir(&dir)?.flatten().any(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x == "mp4" || x == "partial")
        });
        if !pending {
            fs::remove_dir_all(dir)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root() -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("record-student-{}", super::super::nonce().unwrap()));
        fs::create_dir_all(&path).unwrap();
        path
    }
    #[test]
    fn stop_before_start_is_acknowledged_and_later_start_is_idempotent() {
        let root = root();
        let address = "127.0.0.1:1".parse().unwrap();
        let student = Student::new(root.clone(), address, "s".into());
        let mut p = packet("recording_stop");
        p.id = super::super::nonce().unwrap();
        p.token = super::super::nonce().unwrap();
        student.command(p.clone()).unwrap();
        let status = root.join(&p.id).join("status.json");
        for _ in 0..100 {
            if status.exists() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(load_json(&status).unwrap()["phase"], "stopped");
        p.action = "recording_start".into();
        p.text = json!({"expires":epoch()+MAX_SECONDS}).to_string();
        student.command(p).unwrap();
        thread::sleep(Duration::from_millis(150));
        assert_eq!(load_json(&status).unwrap()["phase"], "stopped");
        drop(student);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn restarted_agent_preserves_completed_and_partial_files_without_restarting_capture() {
        let root = root();
        let id = super::super::nonce().unwrap();
        let dir = root.join(&id);
        fs::create_dir(&dir).unwrap();
        atomic_json(
            &dir.join("meta.json"),
            &Metadata {
                id: id.clone(),
                token: super::super::nonce().unwrap(),
                identity: "s".into(),
                address: "127.0.0.1:1".parse().unwrap(),
                expires: epoch() + MAX_SECONDS,
                options: RecordingOptions::default(),
            },
        )
        .unwrap();
        atomic_json(&dir.join("status.json"), &json!({"phase":"recording"})).unwrap();
        fs::write(dir.join("Part-0001.mp4"), b"completed").unwrap();
        fs::write(dir.join("Part-0002.partial"), b"incomplete").unwrap();
        let student = Student::new(root.clone(), "127.0.0.1:1".parse().unwrap(), "s".into());
        for _ in 0..100 {
            if load_json(&dir.join("status.json")).unwrap()["phase"] == "interrupted" {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            load_json(&dir.join("status.json")).unwrap()["phase"],
            "interrupted"
        );
        assert!(dir.join("stop").exists());
        assert_eq!(fs::read(dir.join("Part-0001.mp4")).unwrap(), b"completed");
        assert!(dir.join("Part-0002.partial").exists());
        drop(student);
        fs::remove_dir_all(root).unwrap();
    }
}
