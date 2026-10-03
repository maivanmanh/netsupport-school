//! Bounded classroom messages and separately paced, checksummed file transfers.
//! No media/control/input thread performs file I/O or waits for this service.
#![allow(clippy::too_many_lines)]
use anyhow::{Context, Result};
use control_transport::room::RoomRoster;
use prost::Message;
use protocol::ToolPacket;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, VecDeque},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::{Shutdown, SocketAddr, SocketAddrV4, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use tracing::{info, warn};
const VERSION: u32 = 1;
const MAX_PACKET: usize = 8192;
const CHUNK: usize = 64 * 1024;
pub const MAX_FILE: u64 = 4 * 1024 * 1024 * 1024;
const PACE: Duration = Duration::from_millis(64); // ~1 MiB/s per transfer, at most two
fn packet(action: &str) -> ToolPacket {
    ToolPacket {
        version: VERSION,
        action: action.into(),
        ..Default::default()
    }
}
fn send(s: &mut TcpStream, value: &ToolPacket) -> Result<()> {
    let data = value.encode_to_vec();
    anyhow::ensure!(data.len() <= MAX_PACKET, "Tools packet exceeds limit");
    s.write_all(&u32::try_from(data.len())?.to_be_bytes())?;
    s.write_all(&data)?;
    Ok(())
}
fn read(s: &mut TcpStream) -> Result<ToolPacket> {
    let mut p = [0; 4];
    s.read_exact(&mut p)?;
    let n = u32::from_be_bytes(p) as usize;
    anyhow::ensure!((1..=MAX_PACKET).contains(&n), "Invalid tools packet length");
    let mut data = vec![0; n];
    s.read_exact(&mut data)?;
    let p = ToolPacket::decode(&*data)?;
    anyhow::ensure!(p.version == VERSION, "Tools version mismatch");
    Ok(p)
}
fn configure(s: &TcpStream) -> Result<()> {
    s.set_nonblocking(false)?;
    s.set_nodelay(true)?;
    s.set_read_timeout(Some(Duration::from_secs(4)))?;
    s.set_write_timeout(Some(Duration::from_secs(4)))?;
    Ok(())
}
fn nonce() -> Result<String> {
    let mut b = [0; 16];
    getrandom::fill(&mut b).map_err(|e| anyhow::anyhow!("Tools nonce: {e}"))?;
    let hex = b"0123456789abcdef";
    let mut result = String::with_capacity(32);
    for value in b {
        result.push(char::from(hex[usize::from(value >> 4)]));
        result.push(char::from(hex[usize::from(value & 0x0f)]));
    }
    Ok(result)
}
fn terminal(s: &str) -> bool {
    [
        "complete",
        "skipped",
        "replied",
        "dismissed",
        "cancelled",
        "error",
    ]
    .contains(&s)
}
/// The Teacher's original filename, made safe for every Student OS: path
/// separators/reserved characters are replaced and Windows device names
/// (CON, NUL, COM1…) get a trailing underscore. Nothing is prefixed.
pub fn safe_name(s: &str) -> String {
    let name: String = s
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(140)
        .collect();
    let name = name.trim_matches([' ', '.']);
    if name.is_empty() {
        return "file".into();
    }
    let (stem, extension) = split_name(name);
    let device = stem.trim_end().to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL"].contains(&device.as_str())
        || (device.len() == 4
            && (device.starts_with("COM") || device.starts_with("LPT"))
            && device.as_bytes()[3].is_ascii_digit());
    if reserved {
        join_name(&format!("{stem}_"), extension)
    } else {
        name.into()
    }
}
/// Splits `lesson.notes.pdf` into (`lesson.notes`, Some(`pdf`)); dotfiles keep
/// their leading dot in the stem.
fn split_name(name: &str) -> (&str, Option<&str>) {
    match name.rfind('.') {
        Some(i) if i > 0 && i + 1 < name.len() => (&name[..i], Some(&name[i + 1..])),
        _ => (name, None),
    }
}
fn join_name(stem: &str, extension: Option<&str>) -> String {
    extension.map_or_else(|| stem.to_owned(), |ext| format!("{stem}.{ext}"))
}
/// `name.ext`, then `name-1.ext`, `name-2.ext`, …
fn numbered_name(name: &str, index: u32) -> String {
    if index == 0 {
        return name.to_owned();
    }
    let (stem, extension) = split_name(name);
    join_name(&format!("{stem}-{index}"), extension)
}
fn sha256_file(path: &Path, cancel: &AtomicBool) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; CHUNK];
    loop {
        anyhow::ensure!(!cancel.load(Ordering::Acquire), "Hashing cancelled");
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
/// True when `path` is a regular file with exactly `size` bytes and `hash`.
fn same_content(path: &Path, size: u64, hash: &str, cancel: &AtomicBool) -> bool {
    !hash.is_empty()
        && fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() == size)
        && sha256_file(path, cancel).is_ok_and(|h| h == hash)
}
/// Another receiver sharing the folder (e.g. a VM's redirected Desktop) may
/// still be writing the same name; wait briefly for its size to settle.
fn same_content_settled(path: &Path, size: u64, hash: &str, cancel: &AtomicBool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut last = None;
    loop {
        let len = fs::metadata(path).map(|m| m.len()).ok();
        if len == Some(size) || (len.is_some() && len == last) || Instant::now() >= deadline {
            return same_content(path, size, hash, cancel);
        }
        last = len;
        thread::sleep(Duration::from_millis(200));
    }
}
/// Where the received file will be published, or that an identical copy is
/// already on the Desktop (`Ok(None)`): `name`, `name-1`, … are checked in order.
enum Destination {
    /// First free `name` / `name-N` index.
    Free(u32),
    /// An identical file is already present here.
    Identical(PathBuf),
}
fn choose_destination(folder: &Path, p: &ToolPacket, cancel: &AtomicBool) -> Result<Destination> {
    let name = safe_name(&p.name);
    for index in 0..10_000 {
        let candidate = folder.join(numbered_name(&name, index));
        match fs::symlink_metadata(&candidate) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Destination::Free(index));
            }
            Err(e) => return Err(e).context("Could not inspect the Desktop folder"),
            Ok(_) => {}
        }
        if same_content(&candidate, p.size, &p.hash, cancel) {
            return Ok(Destination::Identical(candidate));
        }
    }
    anyhow::bail!("Could not choose a unique Desktop filename")
}
struct Source(PathBuf);
impl Drop for Source {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
struct Target {
    status: String,
    done: u64,
    reply: String,
    error: String,
}
struct Job {
    packet: ToolPacket,
    source: Option<Arc<Source>>,
    targets: HashMap<String, Target>,
    cancel: Arc<AtomicBool>,
}
#[derive(Default)]
struct State {
    jobs: VecDeque<Job>,
    queues: HashMap<String, VecDeque<ToolPacket>>,
    peers: HashMap<String, u64>,
    generation: u64,
}
#[derive(Default)]
pub struct Hub(Mutex<State>);
impl Hub {
    pub fn snapshot(&self) -> Value {
        let s = self.0.lock().unwrap();
        json!({"ready":s.peers.keys().collect::<Vec<_>>(),"jobs":s.jobs.iter().map(|j|json!({"id":j.packet.id,"kind":j.packet.action,"name":j.packet.name,"size":j.packet.size,"text":j.packet.text.chars().take(240).collect::<String>(),"targets":j.targets.iter().map(|(id,t)|json!({"id":id,"status":t.status,"done":t.done,"reply":t.reply,"error":t.error})).collect::<Vec<_>>()})).collect::<Vec<_>>()})
    }
    /// Hashes a staged file off the caller's thread, then offers it to every
    /// recipient that has not been cancelled meanwhile. The hash lets Students
    /// skip files they already hold with identical content.
    fn prepare_file(self: &Arc<Self>, id: String, path: PathBuf, cancel: Arc<AtomicBool>) {
        let hub = Arc::clone(self);
        thread::spawn(move || {
            let result = sha256_file(&path, &cancel);
            let mut s = hub.0.lock().unwrap();
            let State { jobs, queues, .. } = &mut *s;
            let Some(job) = jobs.iter_mut().find(|j| j.packet.id == id) else {
                return;
            };
            match result {
                Ok(hash) => {
                    job.packet.hash = hash;
                    for (student, target) in &mut job.targets {
                        if target.status == "preparing" {
                            target.status = "queued".into();
                            queues
                                .entry(student.clone())
                                .or_default()
                                .push_back(job.packet.clone());
                        }
                    }
                    info!(job_id=%id,"Classroom file prepared");
                }
                Err(error) => {
                    for target in job.targets.values_mut() {
                        if target.status == "preparing" {
                            target.status = "error".into();
                            target.error = format!("Could not read file: {error:#}");
                        }
                    }
                }
            }
        });
    }
    pub fn submit(
        self: &Arc<Self>,
        action: &str,
        recipients: &[String],
        input: &Value,
        outbox: &Path,
    ) -> Result<()> {
        let mut p = packet(action);
        p.id = nonce()?;
        let source = if action == "file" {
            let path = PathBuf::from(input["path"].as_str().context("File path missing")?);
            let root = outbox
                .canonicalize()
                .context("Outgoing file folder missing")?;
            let path = path.canonicalize()?;
            anyhow::ensure!(
                path.parent() == Some(root.as_path()),
                "File must come from the Teacher file picker"
            );
            let source = Arc::new(Source(path));
            p.size = fs::metadata(&source.0)?.len();
            anyhow::ensure!(p.size <= MAX_FILE, "File exceeds 4 GiB limit");
            p.name = input["name"]
                .as_str()
                .unwrap_or("file")
                .chars()
                .take(140)
                .collect();
            p.token = nonce()?;
            Some(source)
        } else {
            input["text"]
                .as_str()
                .unwrap_or("")
                .trim()
                .clone_into(&mut p.text);
            anyhow::ensure!(
                !p.text.is_empty() && p.text.chars().count() <= 2000,
                "Message must contain 1–2000 characters"
            );
            p.allow_reply = input["allow_reply"].as_bool().unwrap_or(true);
            None
        };
        let mut s = self.0.lock().unwrap();
        anyhow::ensure!(
            !recipients.is_empty() && recipients.len() <= 64,
            "Select online Students"
        );
        anyhow::ensure!(
            recipients.iter().all(|id| s.peers.contains_key(id)),
            "A selected Student is not ready for classroom tools"
        );
        anyhow::ensure!(
            recipients
                .iter()
                .all(|id| s.queues.get(id).is_none_or(|q| q.len() < 16)),
            "Student has too many pending tools"
        );
        if s.jobs.len() >= 32 {
            let i = s
                .jobs
                .iter()
                .position(|j| j.targets.values().all(|t| terminal(&t.status)))
                .context("Too many active classroom jobs")?;
            s.jobs.remove(i);
        }
        let retained: u64 = s
            .jobs
            .iter()
            .filter(|j| j.source.is_some())
            .map(|j| j.packet.size)
            .sum();
        anyhow::ensure!(
            retained.saturating_add(p.size) <= 8 * 1024 * 1024 * 1024,
            "Outgoing files exceed 8 GiB queue budget"
        );
        let initial = if source.is_some() {
            "preparing"
        } else {
            "queued"
        };
        let targets = recipients
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    Target {
                        status: initial.into(),
                        done: 0,
                        reply: String::new(),
                        error: String::new(),
                    },
                )
            })
            .collect();
        if source.is_none() {
            for id in recipients {
                s.queues.entry(id.clone()).or_default().push_back(p.clone());
            }
        }
        info!(job_id=%p.id,kind=%action,recipients=recipients.len(),bytes=p.size,"Classroom tools job queued");
        let cancel = Arc::new(AtomicBool::new(false));
        let prepare = source
            .as_ref()
            .map(|src| (p.id.clone(), src.0.clone(), Arc::clone(&cancel)));
        s.jobs.push_back(Job {
            packet: p,
            source,
            targets,
            cancel,
        });
        drop(s);
        if let Some((id, path, cancel)) = prepare {
            self.prepare_file(id, path, cancel);
        }
        Ok(())
    }
    pub fn cancel(&self, id: &str) -> Result<()> {
        let mut s = self.0.lock().unwrap();
        let j = s
            .jobs
            .iter_mut()
            .find(|j| j.packet.id == id)
            .context("Job not found")?;
        j.cancel.store(true, Ordering::Release);
        let targets = j.targets.keys().cloned().collect::<Vec<_>>();
        for t in j.targets.values_mut().filter(|t| !terminal(&t.status)) {
            t.status = "cancelled".into();
        }
        for student in targets {
            let q = s.queues.entry(student).or_default();
            q.retain(|p| p.id != id);
            if q.len() < 16 {
                let mut p = packet("cancel");
                p.id = id.into();
                q.push_back(p);
            }
        }
        info!(job_id = id, "Classroom tools job cancelled");
        Ok(())
    }
    pub fn retry(self: &Arc<Self>, id: &str, outbox: &Path) -> Result<()> {
        let (targets, input) = {
            let s = self.0.lock().unwrap();
            let j = s
                .jobs
                .iter()
                .find(|j| j.packet.id == id && j.packet.action == "file")
                .context("File job not found")?;
            let src = j
                .source
                .as_ref()
                .context("Outgoing file no longer available")?;
            (
                j.targets
                    .iter()
                    .filter(|(_, t)| ["error", "cancelled"].contains(&t.status.as_str()))
                    .map(|(id, _)| id.clone())
                    .collect::<Vec<_>>(),
                json!({"path":src.0,"name":j.packet.name}),
            )
        };
        // Retrying uses a new staged file/job identity; the previous attempt cannot overwrite it.
        let old = PathBuf::from(input["path"].as_str().unwrap());
        let next = outbox.join(nonce()?);
        fs::copy(old, &next)?;
        self.submit(
            "file",
            &targets,
            &json!({"path":next,"name":input["name"]}),
            outbox,
        )
    }
    fn event(&self, student: &str, p: &ToolPacket) -> Result<()> {
        let mut s = self.0.lock().unwrap();
        let j = s
            .jobs
            .iter_mut()
            .find(|j| j.packet.id == p.id)
            .context("Unknown classroom job")?;
        let t = j
            .targets
            .get_mut(student)
            .context("Student is not a recipient")?;
        if terminal(&t.status) {
            return Ok(());
        }
        anyhow::ensure!(
            [
                "delivered",
                "dismissed",
                "replied",
                "waiting",
                "receiving",
                "complete",
                "skipped",
                "error",
                "cancelled"
            ]
            .contains(&p.status.as_str()),
            "Invalid tools status"
        );
        anyhow::ensure!(
            p.status != "skipped" || j.packet.action == "file",
            "Only files can be skipped"
        );
        anyhow::ensure!(
            p.done <= j.packet.size && p.text.chars().count() <= 2000,
            "Invalid tools event size"
        );
        if p.status == "replied" {
            anyhow::ensure!(
                j.packet.action == "message" && j.packet.allow_reply,
                "Reply not allowed"
            );
            t.reply.clone_from(&p.text);
        }
        if p.status == "error" {
            t.error = p.text.chars().take(300).collect();
        }
        t.status.clone_from(&p.status);
        t.done = t.done.max(p.done);
        if terminal(&t.status) {
            info!(job_id=%p.id,student_id=student,status=%t.status,bytes=t.done,"Classroom tools recipient finished");
        }
        if j.targets
            .values()
            .all(|t| t.status == "complete" || t.status == "skipped")
        {
            j.source = None;
        }
        Ok(())
    }
    fn disconnected(&self, student: &str, generation: u64) {
        let mut state = self.0.lock().unwrap();
        if state.peers.get(student) != Some(&generation) {
            return;
        }
        state.peers.remove(student);
        for job in &mut state.jobs {
            if let Some(target) = job.targets.get_mut(student) {
                if !terminal(&target.status) {
                    target.status = "error".into();
                    target.error = "Student classroom tools connection interrupted".into();
                    info!(job_id=%job.packet.id,student_id=student,status=%target.status,"Classroom tools recipient finished");
                }
            }
        }
    }
}
pub struct Server {
    stop: Arc<AtomicBool>,
    sockets: Arc<Mutex<Vec<TcpStream>>>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        for s in self.sockets.lock().unwrap().iter() {
            let _ = s.shutdown(Shutdown::Both);
        }
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}
pub fn teacher(
    address: SocketAddrV4,
    roster: Arc<Mutex<RoomRoster>>,
    hub: Arc<Hub>,
) -> Result<Server> {
    let listener = TcpListener::bind(address)?;
    listener.set_nonblocking(true)?;
    let stop = Arc::new(AtomicBool::new(false));
    let cancel = Arc::clone(&stop);
    let sockets = Arc::new(Mutex::new(Vec::<TcpStream>::new()));
    let active = Arc::clone(&sockets);
    let worker = thread::spawn(move || {
        let slots = Arc::new(AtomicUsize::new(0));
        let mut workers = Vec::<thread::JoinHandle<()>>::new();
        info!(%address,"Teacher classroom tools listener ready");
        while !cancel.load(Ordering::Acquire) {
            for i in (0..workers.len()).rev() {
                if workers[i].is_finished() {
                    let _ = workers.swap_remove(i).join();
                }
            }
            active
                .lock()
                .unwrap()
                .retain(|s| s.take_error().is_ok_and(|e| e.is_none()));
            match listener.accept() {
                Ok((mut s, peer)) if workers.len() < 66 => {
                    if let Ok(copy) = s.try_clone() {
                        active.lock().unwrap().push(copy);
                    }
                    let h = Arc::clone(&hub);
                    let r = Arc::clone(&roster);
                    let c = Arc::clone(&cancel);
                    let slots = Arc::clone(&slots);
                    let active = Arc::clone(&active);
                    workers.push(thread::spawn(move || {
                        let key = s.local_addr().ok().zip(s.peer_addr().ok());
                        let result = serve(&mut s, peer, &r, &h, &c, &slots);
                        if let Err(error) = result {
                            info!(%error,"Classroom tools connection ended");
                        }
                        active
                            .lock()
                            .unwrap()
                            .retain(|v| v.local_addr().ok().zip(v.peer_addr().ok()) != key);
                    }));
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(20));
                }
                Err(e) => {
                    warn!(%e,"Classroom tools listener failed");
                    break;
                }
            }
        }
        for w in workers {
            let _ = w.join();
        }
    });
    Ok(Server {
        stop,
        sockets,
        worker: Some(worker),
    })
}
fn registered(roster: &Arc<Mutex<RoomRoster>>, id: &str, peer: SocketAddr) -> bool {
    roster.lock().unwrap().snapshot().iter().any(|r| {
        r.online && r.instance_id == id && std::net::IpAddr::V4(*r.endpoint.ip()) == peer.ip()
    })
}
fn serve(
    s: &mut TcpStream,
    peer: SocketAddr,
    roster: &Arc<Mutex<RoomRoster>>,
    hub: &Arc<Hub>,
    stop: &AtomicBool,
    slots: &Arc<AtomicUsize>,
) -> Result<()> {
    configure(s)?;
    let hello = read(s)?;
    anyhow::ensure!(
        registered(roster, &hello.student, peer),
        "Tools peer not registered in classroom"
    );
    if hello.action == "download" {
        return serve_file(s, &hello, hub, stop, slots);
    }
    anyhow::ensure!(hello.action == "hello", "Invalid tools handshake");
    let generation = {
        let mut st = hub.0.lock().unwrap();
        st.generation += 1;
        let g = st.generation;
        st.peers.insert(hello.student.clone(), g);
        g
    };
    let result = (|| {
        send(s, &packet("ok"))?;
        while !stop.load(Ordering::Acquire) {
            let p = read(s)?;
            anyhow::ensure!(
                registered(roster, &hello.student, peer),
                "Student left classroom"
            );
            if p.action == "event" {
                hub.event(&hello.student, &p)?;
                send(s, &packet("ok"))?;
            } else {
                anyhow::ensure!(p.action == "poll", "Invalid tools action");
                let next = hub
                    .0
                    .lock()
                    .unwrap()
                    .queues
                    .get_mut(&hello.student)
                    .and_then(VecDeque::pop_front)
                    .unwrap_or_else(|| packet("idle"));
                send(s, &next)?;
            }
        }
        Ok(())
    })();
    hub.disconnected(&hello.student, generation);
    result
}
struct Slot(Arc<AtomicUsize>);
impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
fn serve_file(
    s: &mut TcpStream,
    p: &ToolPacket,
    hub: &Hub,
    stop: &AtomicBool,
    slots: &Arc<AtomicUsize>,
) -> Result<()> {
    let (source, cancel, total) = {
        let st = hub.0.lock().unwrap();
        let j = st
            .jobs
            .iter()
            .find(|j| {
                j.packet.id == p.id
                    && j.packet.action == "file"
                    && j.packet.token == p.token
                    && j.targets.contains_key(&p.student)
            })
            .context("Invalid file request")?;
        anyhow::ensure!(!j.cancel.load(Ordering::Acquire), "File transfer cancelled");
        (
            Arc::clone(j.source.as_ref().context("File source unavailable")?),
            Arc::clone(&j.cancel),
            j.packet.size,
        )
    };
    if slots
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| {
            (v < 2).then_some(v + 1)
        })
        .is_err()
    {
        send(s, &packet("busy"))?;
        return Ok(());
    }
    let _slot = Slot(Arc::clone(slots));
    let mut file = File::open(&source.0)?;
    anyhow::ensure!(file.metadata()?.len() == total, "Outgoing file changed");
    send(s, &packet("ready"))?;
    let mut done = 0;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; CHUNK];
    while done < total {
        let start = Instant::now();
        anyhow::ensure!(
            !cancel.load(Ordering::Acquire) && !stop.load(Ordering::Acquire),
            "File transfer cancelled"
        );
        let n = file.read(&mut buffer[..usize::try_from((total - done).min(CHUNK as u64))?])?;
        anyhow::ensure!(n > 0, "Outgoing file truncated");
        hash.update(&buffer[..n]);
        s.write_all(&buffer[..n])?;
        done += n as u64;
        thread::sleep(PACE.saturating_sub(start.elapsed()));
    }
    let mut finish = packet("checksum");
    finish.hash = format!("{:x}", hash.finalize());
    send(s, &finish)?;
    // The Student copies the verified file onto its Desktop before acking;
    // a large file (or a first-use OS folder-access prompt) takes far longer
    // than the 4-second control timeout.
    s.set_read_timeout(Some(publish_timeout(total)))?;
    let ack = read(s)?;
    anyhow::ensure!(
        ack.action == "stored" && ack.hash == finish.hash,
        "Student did not confirm file integrity"
    );
    let mut event = packet("event");
    event.id.clone_from(&p.id);
    event.status = "complete".into();
    event.done = total;
    hub.event(&p.student, &event)?;
    Ok(())
}
/// 2 minutes plus 10 MiB/s of local copy time.
fn publish_timeout(size: u64) -> Duration {
    Duration::from_secs(120 + size / (10 * 1024 * 1024))
}
#[derive(Default)]
pub struct Inbox {
    messages: Mutex<VecDeque<ToolPacket>>,
    events: Mutex<VecDeque<ToolPacket>>,
    transfers: Mutex<VecDeque<Transfer>>,
}
/// Student-side view of one incoming file, for the local progress window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransferState {
    Waiting,
    Receiving,
    Saved(PathBuf),
    AlreadyPresent(PathBuf),
    Failed(String),
    Cancelled,
}
impl TransferState {
    pub fn active(&self) -> bool {
        matches!(self, Self::Waiting | Self::Receiving)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transfer {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub done: u64,
    pub state: TransferState,
}
const MAX_TRANSFERS: usize = 32;
impl Inbox {
    pub fn take(&self) -> Option<ToolPacket> {
        self.messages.lock().unwrap().pop_front()
    }
    /// Snapshot, oldest first.
    pub fn transfers(&self) -> Vec<Transfer> {
        self.transfers.lock().unwrap().iter().cloned().collect()
    }
    /// Removes finished transfers the Student has acknowledged.
    pub fn forget_finished(&self) {
        self.transfers.lock().unwrap().retain(|t| t.state.active());
    }
    fn track_file(&self, p: &ToolPacket) {
        let mut q = self.transfers.lock().unwrap();
        if q.iter().any(|t| t.id == p.id) {
            return;
        }
        while q.len() >= MAX_TRANSFERS {
            match q.iter().position(|t| !t.state.active()) {
                Some(i) => {
                    q.remove(i);
                }
                None => return,
            }
        }
        q.push_back(Transfer {
            id: p.id.clone(),
            name: safe_name(&p.name),
            size: p.size,
            done: 0,
            state: TransferState::Waiting,
        });
    }
    fn set_transfer(&self, id: &str, state: TransferState, done: Option<u64>) {
        if let Some(t) = self
            .transfers
            .lock()
            .unwrap()
            .iter_mut()
            .find(|t| t.id == id)
        {
            if let Some(done) = done {
                t.done = done.min(t.size);
            }
            t.state = state;
        }
    }
    pub fn event(&self, id: &str, status: &str, text: &str, done: u64) {
        match status {
            "waiting" => self.set_transfer(id, TransferState::Waiting, None),
            "receiving" => self.set_transfer(id, TransferState::Receiving, Some(done)),
            "error" => self.set_transfer(
                id,
                TransferState::Failed(text.chars().take(200).collect()),
                None,
            ),
            "cancelled" => self.set_transfer(id, TransferState::Cancelled, None),
            _ => {}
        }
        let mut q = self.events.lock().unwrap();
        let mut p = packet("event");
        p.id = id.into();
        p.status = status.into();
        p.text = text.chars().take(2000).collect();
        p.done = done;
        if status == "receiving" {
            q.retain(|v| !(v.id == id && v.status == "receiving"));
        }
        if q.len() < 64 {
            q.push_back(p);
        }
    }
}
pub struct Client {
    stop: Arc<AtomicBool>,
    socket: Arc<Mutex<Option<TcpStream>>>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Drop for Client {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(s) = self.socket.lock().unwrap().as_ref() {
            let _ = s.shutdown(Shutdown::Both);
        }
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}
struct Download {
    id: String,
    cancel: Arc<AtomicBool>,
    worker: thread::JoinHandle<()>,
}
fn start_download(
    address: SocketAddrV4,
    student: String,
    p: ToolPacket,
    inbox: Arc<Inbox>,
    cancel_all: Arc<AtomicBool>,
) -> Download {
    let canceled = Arc::new(AtomicBool::new(false));
    let local_cancel = Arc::clone(&canceled);
    let id = p.id.clone();
    let worker = thread::spawn(move || {
        if let Err(e) = receive_file(address, &student, &p, &inbox, &local_cancel, &cancel_all) {
            inbox.event(
                &p.id,
                if local_cancel.load(Ordering::Acquire) || cancel_all.load(Ordering::Acquire) {
                    "cancelled"
                } else {
                    "error"
                },
                &format!("{e:#}"),
                0,
            );
        }
    });
    Download {
        id,
        cancel: canceled,
        worker,
    }
}
pub fn student(address: SocketAddrV4, identity: String, inbox: Arc<Inbox>) -> Client {
    let stop = Arc::new(AtomicBool::new(false));
    let cancel = Arc::clone(&stop);
    let socket = Arc::new(Mutex::new(None));
    let active = Arc::clone(&socket);
    let worker = thread::spawn(move || {
        let mut download: Option<Download> = None;
        let mut pending_downloads = VecDeque::<ToolPacket>::new();
        while !cancel.load(Ordering::Acquire) {
            if let Ok(mut s) = TcpStream::connect_timeout(&address.into(), Duration::from_secs(2)) {
                *active.lock().unwrap() = s.try_clone().ok();
                let result = (|| -> Result<()> {
                    configure(&s)?;
                    let mut hello = packet("hello");
                    hello.student.clone_from(&identity);
                    send(&mut s, &hello)?;
                    anyhow::ensure!(read(&mut s)?.action == "ok", "Tools handshake rejected");
                    info!("Student classroom tools ready");
                    while !cancel.load(Ordering::Acquire) {
                        for _ in 0..16 {
                            let p = inbox.events.lock().unwrap().pop_front();
                            let Some(p) = p else { break };
                            send(&mut s, &p)?;
                            anyhow::ensure!(read(&mut s)?.action == "ok", "Tools event rejected");
                        }
                        send(&mut s, &packet("poll"))?;
                        let p = read(&mut s)?;
                        if download.as_ref().is_some_and(|d| d.worker.is_finished()) {
                            let _ = download.take().unwrap().worker.join();
                        }
                        if download.is_none() {
                            if let Some(pending) = pending_downloads.pop_front() {
                                download = Some(start_download(
                                    address,
                                    identity.clone(),
                                    pending,
                                    Arc::clone(&inbox),
                                    Arc::clone(&cancel),
                                ));
                            }
                        }
                        match p.action.as_str() {
                            "message" => {
                                let mut q = inbox.messages.lock().unwrap();
                                if q.len() < 16 {
                                    inbox.event(&p.id, "delivered", "", 0);
                                    q.push_back(p);
                                } else {
                                    inbox.event(&p.id, "error", "Student message inbox is full", 0);
                                }
                            }
                            "file" => {
                                inbox.track_file(&p);
                                if download.is_some() {
                                    if pending_downloads.len() < 8 {
                                        inbox.event(&p.id, "waiting", "", 0);
                                        pending_downloads.push_back(p);
                                    } else {
                                        inbox.event(
                                            &p.id,
                                            "error",
                                            "Student file queue is full",
                                            0,
                                        );
                                    }
                                } else {
                                    download = Some(start_download(
                                        address,
                                        identity.clone(),
                                        p,
                                        Arc::clone(&inbox),
                                        Arc::clone(&cancel),
                                    ));
                                }
                            }
                            "cancel" => {
                                if let Some(d) = download.as_ref().filter(|d| d.id == p.id) {
                                    d.cancel.store(true, Ordering::Release);
                                }
                                if pending_downloads.iter().any(|queued| queued.id == p.id) {
                                    inbox.set_transfer(&p.id, TransferState::Cancelled, None);
                                }
                                pending_downloads.retain(|queued| queued.id != p.id);
                                inbox.messages.lock().unwrap().retain(|m| m.id != p.id);
                            }
                            "idle" => {}
                            _ => anyhow::bail!("Invalid classroom tool"),
                        }
                        for _ in 0..10 {
                            if cancel.load(Ordering::Acquire) {
                                break;
                            }
                            thread::sleep(Duration::from_millis(20));
                        }
                    }
                    Ok(())
                })();
                if let Err(e) = result {
                    info!(%e,"Student tools disconnected; retrying");
                }
                *active.lock().unwrap() = None;
            }
            for _ in 0..20 {
                if cancel.load(Ordering::Acquire) {
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
        }
        if let Some(d) = download {
            d.cancel.store(true, Ordering::Release);
            let _ = d.worker.join();
        }
    });
    Client {
        stop,
        socket,
        worker: Some(worker),
    }
}
struct Partial(PathBuf);
impl Drop for Partial {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn receive_file(
    address: SocketAddrV4,
    student: &str,
    p: &ToolPacket,
    inbox: &Inbox,
    cancel: &AtomicBool,
    stop: &AtomicBool,
) -> Result<()> {
    let folder = desktop_folder()?;
    // Stage outside the Desktop so a transfer in progress never shows up there.
    let staging = std::env::temp_dir().join("Classroom-incoming");
    receive_file_to_folder(address, student, p, inbox, cancel, stop, &folder, &staging)
}
#[allow(clippy::too_many_arguments)]
fn receive_file_to_folder(
    address: SocketAddrV4,
    student: &str,
    p: &ToolPacket,
    inbox: &Inbox,
    cancel: &AtomicBool,
    stop: &AtomicBool,
    folder: &Path,
    staging: &Path,
) -> Result<()> {
    anyhow::ensure!(p.size <= MAX_FILE, "File exceeds 4 GiB limit");
    anyhow::ensure!(
        p.id.len() == 32 && p.id.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid file job id"
    );
    fs::create_dir_all(folder)?;
    // Touch the Desktop before downloading: on macOS this is where a first-use
    // folder-access prompt appears, and a denial fails here, not after transfer.
    fs::read_dir(folder).context("Desktop folder is not accessible")?;
    fs::create_dir_all(staging)?;
    let name = safe_name(&p.name);
    // An identical file (same name or numbered copy, same SHA-256) means
    // there is nothing to transfer.
    let first_free = match choose_destination(folder, p, cancel)? {
        Destination::Free(index) => index,
        Destination::Identical(path) => {
            inbox.event(
                &p.id,
                "skipped",
                "Identical file already on Desktop",
                p.size,
            );
            inbox.set_transfer(&p.id, TransferState::AlreadyPresent(path), Some(p.size));
            info!(job_id=%p.id,"Student classroom file already present; skipped");
            return Ok(());
        }
    };
    let partial = Partial(staging.join(format!("{}.part", p.id)));
    let mut s = loop {
        anyhow::ensure!(
            !stop.load(Ordering::Acquire) && !cancel.load(Ordering::Acquire),
            "File cancelled"
        );
        let mut s = TcpStream::connect_timeout(&address.into(), Duration::from_secs(2))?;
        configure(&s)?;
        let mut request = p.clone();
        request.action = "download".into();
        request.student = student.into();
        send(&mut s, &request)?;
        let r = read(&mut s)?;
        if r.action == "ready" {
            break s;
        }
        anyhow::ensure!(r.action == "busy", "File offer rejected");
        inbox.event(&p.id, "waiting", "", 0);
        for _ in 0..20 {
            if stop.load(Ordering::Acquire) || cancel.load(Ordering::Acquire) {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
    };
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&partial.0)
        .context("Could not create temporary download file")?;
    let mut done = 0;
    let mut hash = Sha256::new();
    let mut b = vec![0; CHUNK];
    let mut reported = Instant::now();
    inbox.event(&p.id, "receiving", "", 0);
    while done < p.size {
        anyhow::ensure!(
            !stop.load(Ordering::Acquire) && !cancel.load(Ordering::Acquire),
            "File cancelled"
        );
        let n = s.read(&mut b[..usize::try_from((p.size - done).min(CHUNK as u64))?])?;
        anyhow::ensure!(n > 0, "File transfer interrupted");
        file.write_all(&b[..n])?;
        hash.update(&b[..n]);
        done += n as u64;
        if reported.elapsed() >= Duration::from_millis(250) {
            inbox.event(&p.id, "receiving", "", done);
            reported = Instant::now();
        }
    }
    let checksum = read(&mut s)?;
    let digest = format!("{:x}", hash.finalize());
    anyhow::ensure!(
        checksum.action == "checksum" && checksum.hash == digest,
        "File checksum mismatch"
    );
    anyhow::ensure!(
        !stop.load(Ordering::Acquire) && !cancel.load(Ordering::Acquire),
        "File cancelled"
    );
    file.sync_all()?;
    drop(file);
    // Publish with create_new + copy instead of hard links/rename: redirected
    // Windows Desktops (e.g. a Parallels shared folder) reject hard links with
    // error 87, and rename could overwrite. create_new never replaces a file.
    let mut index = first_free;
    let published = loop {
        let candidate = folder.join(numbered_name(&name, index));
        let mut output = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                // Possibly a peer sharing this folder publishing the same file.
                if same_content_settled(&candidate, p.size, &digest, cancel) {
                    info!(job_id=%p.id,"Identical file appeared on Desktop; not duplicated");
                    break TransferState::AlreadyPresent(candidate);
                }
                index = index
                    .checked_add(1)
                    .filter(|i| *i < 10_000)
                    .context("No unique Desktop filename remains")?;
                continue;
            }
            Err(error) => return Err(error).context("Could not create received file on Desktop"),
        };
        let copy_result = (|| -> Result<()> {
            let mut verified = File::open(&partial.0)?;
            std::io::copy(&mut verified, &mut output)?;
            output.sync_all()?;
            Ok(())
        })();
        drop(output);
        if let Err(error) = copy_result {
            let _ = fs::remove_file(&candidate);
            return Err(error).context("Could not publish verified file to Desktop");
        }
        break TransferState::Saved(candidate);
    };
    drop(partial);
    let mut ack = packet("stored");
    ack.hash = digest;
    // The file is on the Desktop either way; a lost ack only affects the
    // Teacher's status, so show the Student the local result first.
    inbox.set_transfer(&p.id, published, Some(p.size));
    send(&mut s, &ack)?;
    info!(job_id=%p.id,bytes=done,"Student classroom file saved and verified");
    Ok(())
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn desktop_folder() -> Result<PathBuf> {
    use windows::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{FOLDERID_Desktop, SHGetKnownFolderPath, KF_FLAG_DEFAULT},
    };
    // SAFETY: Shell returns a CoTaskMem-allocated UTF-16 path for this user's
    // Desktop (including OneDrive redirection). Copy it and free exactly once.
    unsafe {
        let path = SHGetKnownFolderPath(&FOLDERID_Desktop, KF_FLAG_DEFAULT, None)?;
        let text = path.to_string();
        CoTaskMemFree(Some(path.0.cast()));
        Ok(PathBuf::from(text?))
    }
}
#[cfg(not(windows))]
fn desktop_folder() -> Result<PathBuf> {
    Ok(PathBuf::from(std::env::var_os("HOME").context("User home missing")?).join("Desktop"))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn test_folder() -> PathBuf {
        let path = std::env::temp_dir().join(format!("classroom-tools-{}", nonce().unwrap()));
        fs::create_dir_all(&path).unwrap();
        path
    }
    struct TestFolder(PathBuf);
    impl Drop for TestFolder {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn filenames_do_not_escape_received_folder() {
        for s in ["../../secret", "CON", "a:b\\c", "", "héllo.txt"] {
            let n = safe_name(s);
            assert!(!n.contains(['/', '\\', ':']));
            assert!(!n.is_empty());
        }
    }
    #[test]
    fn progress_events_are_bounded_and_coalesced() {
        let i = Inbox::default();
        for n in 0..10000 {
            i.event("job", "receiving", "", n);
        }
        assert_eq!(i.events.lock().unwrap().len(), 1);
        i.event("job", "error", "failed", 0);
        assert_eq!(i.events.lock().unwrap().len(), 2);
    }
    #[test]
    fn protobuf_roundtrip_and_version() {
        let mut p = packet("message");
        p.text = "Tiếng Việt".into();
        assert_eq!(ToolPacket::decode(&*p.encode_to_vec()).unwrap(), p);
    }

    #[test]
    fn received_names_keep_the_original_and_number_collisions() {
        assert_eq!(safe_name("Meeting 29 Sept.pdf"), "Meeting 29 Sept.pdf");
        assert_eq!(safe_name("con.txt"), "con_.txt");
        assert_eq!(safe_name("COM1"), "COM1_");
        assert_eq!(safe_name("COMPUTER.txt"), "COMPUTER.txt");
        assert_eq!(numbered_name("lesson.notes.pdf", 0), "lesson.notes.pdf");
        assert_eq!(numbered_name("lesson.notes.pdf", 2), "lesson.notes-2.pdf");
        assert_eq!(numbered_name("README", 1), "README-1");
        assert_eq!(numbered_name(".env", 1), ".env-1");
    }

    fn staged_offer(hub: &Arc<Hub>, outbox: &Path, name: &str, payload: &[u8]) -> ToolPacket {
        let source = outbox.join(nonce().unwrap());
        fs::write(&source, payload).unwrap();
        hub.0.lock().unwrap().peers.insert("student-a".into(), 1);
        hub.submit(
            "file",
            &["student-a".into()],
            &json!({"path":source,"name":name}),
            outbox,
        )
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let offer = hub
                .0
                .lock()
                .unwrap()
                .queues
                .get_mut("student-a")
                .and_then(VecDeque::pop_front);
            if let Some(offer) = offer {
                return offer;
            }
            assert!(Instant::now() < deadline, "file was never prepared");
            thread::sleep(Duration::from_millis(5));
        }
    }

    /// Runs one Teacher download server (if the Student connects) and the
    /// Student receive path; returns the Student result and its events.
    fn transfer(
        hub: &Arc<Hub>,
        offer: &ToolPacket,
        desktop: &Path,
        staging: &Path,
    ) -> (Result<()>, Vec<String>) {
        let listener =
            TcpListener::bind(SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = SocketAddrV4::new(
            std::net::Ipv4Addr::LOCALHOST,
            listener.local_addr().unwrap().port(),
        );
        let stop = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicBool::new(false));
        let server_hub = Arc::clone(hub);
        let (server_stop, server_done) = (Arc::clone(&stop), Arc::clone(&done));
        let server = thread::spawn(move || {
            while !server_done.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        configure(&stream).unwrap();
                        let request = read(&mut stream).unwrap();
                        let slots = Arc::new(AtomicUsize::new(0));
                        serve_file(&mut stream, &request, &server_hub, &server_stop, &slots)
                            .unwrap();
                    }
                    Err(_) => thread::sleep(Duration::from_millis(5)),
                }
            }
        });
        let inbox = Inbox::default();
        inbox.track_file(offer);
        let result = receive_file_to_folder(
            address,
            "student-a",
            offer,
            &inbox,
            &AtomicBool::new(false),
            &stop,
            desktop,
            staging,
        );
        done.store(true, Ordering::Release);
        server.join().unwrap();
        let mut events: Vec<String> = inbox
            .events
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.status.clone())
            .collect();
        // Append the Student-visible final state for assertions.
        events.push(match &inbox.transfers()[0].state {
            TransferState::Saved(p) => {
                format!("saved:{}", p.file_name().unwrap().to_string_lossy())
            }
            TransferState::AlreadyPresent(p) => {
                format!("present:{}", p.file_name().unwrap().to_string_lossy())
            }
            other => format!("{other:?}"),
        });
        (result, events)
    }

    #[test]
    fn file_keeps_its_name_and_skips_identical_copies() {
        let temp = TestFolder(test_folder());
        let (outbox, desktop, staging) = (
            temp.0.join("outbox"),
            temp.0.join("Desktop"),
            temp.0.join("staging"),
        );
        fs::create_dir_all(&outbox).unwrap();
        let payload: Vec<u8> = (0..190_000_u32).map(|n| (n % 251) as u8).collect();
        let hub = Arc::new(Hub::default());

        let offer = staged_offer(&hub, &outbox, "lesson notes.txt", &payload);
        assert_eq!(offer.hash.len(), 64);
        let (result, _) = transfer(&hub, &offer, &desktop, &staging);
        result.unwrap();
        assert_eq!(fs::read(desktop.join("lesson notes.txt")).unwrap(), payload);
        assert_eq!(
            fs::read_dir(&staging).unwrap().count(),
            0,
            "partial removed"
        );
        assert_eq!(
            hub.snapshot()["jobs"][0]["targets"][0]["status"],
            "complete"
        );

        // Same name, same content: nothing is transferred or duplicated.
        let again = staged_offer(&hub, &outbox, "lesson notes.txt", &payload);
        let (result, events) = transfer(&hub, &again, &desktop, &staging);
        result.unwrap();
        assert_eq!(events, ["skipped", "present:lesson notes.txt"]);
        assert_eq!(fs::read_dir(&desktop).unwrap().count(), 1);

        // Same name, different content: published as name-1, original kept.
        let mut changed = payload.clone();
        changed[0] ^= 1;
        let v2 = staged_offer(&hub, &outbox, "lesson notes.txt", &changed);
        let (result, events) = transfer(&hub, &v2, &desktop, &staging);
        result.unwrap();
        assert_eq!(events.last().unwrap(), "saved:lesson notes-1.txt");
        assert_eq!(fs::read(desktop.join("lesson notes.txt")).unwrap(), payload);
        assert_eq!(
            fs::read(desktop.join("lesson notes-1.txt")).unwrap(),
            changed
        );

        // The changed version is now present as name-1: skipped too.
        let v2_again = staged_offer(&hub, &outbox, "lesson notes.txt", &changed);
        let (_, events) = transfer(&hub, &v2_again, &desktop, &staging);
        assert_eq!(events, ["skipped", "present:lesson notes-1.txt"]);
        assert_eq!(fs::read_dir(&desktop).unwrap().count(), 2);

        // A third version takes the next free number.
        changed[1] ^= 1;
        let v3 = staged_offer(&hub, &outbox, "lesson notes.txt", &changed);
        transfer(&hub, &v3, &desktop, &staging).0.unwrap();
        assert_eq!(
            fs::read(desktop.join("lesson notes-2.txt")).unwrap(),
            changed
        );
    }

    #[test]
    fn identical_file_published_by_a_folder_peer_is_not_duplicated() {
        let temp = TestFolder(test_folder());
        let desktop = temp.0.join("Desktop");
        fs::create_dir_all(&desktop).unwrap();
        let payload = b"shared desktop".to_vec();
        let digest = format!("{:x}", Sha256::digest(&payload));
        let path = desktop.join("notes.txt");
        // Peer published while this receiver was downloading.
        fs::write(&path, &payload).unwrap();
        assert!(same_content_settled(
            &path,
            payload.len() as u64,
            &digest,
            &AtomicBool::new(false)
        ));
        assert!(!same_content_settled(
            &path,
            payload.len() as u64,
            "other",
            &AtomicBool::new(false)
        ));
    }

    #[test]
    fn rejected_file_checksum_removes_partial_desktop_file() {
        let temp = TestFolder(test_folder());
        let desktop = temp.0.join("Desktop");
        let staging = temp.0.join("staging");
        let listener =
            TcpListener::bind(SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = SocketAddrV4::new(
            std::net::Ipv4Addr::LOCALHOST,
            listener.local_addr().unwrap().port(),
        );
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            configure(&stream).unwrap();
            let _request = read(&mut stream).unwrap();
            send(&mut stream, &packet("ready")).unwrap();
            stream.write_all(b"data").unwrap();
            let mut checksum = packet("checksum");
            checksum.hash = "invalid-checksum".into();
            send(&mut stream, &checksum).unwrap();
        });
        let mut offer = packet("file");
        offer.id = "0123456789abcdef0123456789abcdef".into();
        offer.name = "bad.txt".into();
        offer.size = 4;
        offer.token = "secret-token".into();
        let result = receive_file_to_folder(
            address,
            "student-a",
            &offer,
            &Inbox::default(),
            &AtomicBool::new(false),
            &AtomicBool::new(false),
            &desktop,
            &staging,
        );
        server.join().unwrap();
        assert!(result.is_err());
        assert_eq!(fs::read_dir(desktop).unwrap().count(), 0);
        assert_eq!(fs::read_dir(staging).unwrap().count(), 0);
    }

    #[test]
    fn a_student_reply_is_accepted_only_once() {
        let temp = TestFolder(test_folder());
        let outbox = temp.0.join("outbox");
        fs::create_dir_all(&outbox).unwrap();
        let hub = Arc::new(Hub::default());
        hub.0.lock().unwrap().peers.insert("student-a".into(), 1);
        hub.submit(
            "message",
            &["student-a".into()],
            &json!({"text":"Please respond","allow_reply":true}),
            &outbox,
        )
        .unwrap();
        let id = hub.0.lock().unwrap().jobs[0].packet.id.clone();
        let mut first = packet("event");
        first.id = id.clone();
        first.status = "replied".into();
        first.text = "first response".into();
        hub.event("student-a", &first).unwrap();
        first.text = "second response".into();
        hub.event("student-a", &first).unwrap();
        assert_eq!(
            hub.snapshot()["jobs"][0]["targets"][0]["reply"],
            "first response"
        );
    }

    #[test]
    fn disconnect_marks_inflight_job_retryable() {
        let temp = TestFolder(test_folder());
        let outbox = temp.0.join("outbox");
        fs::create_dir_all(&outbox).unwrap();
        let hub = Arc::new(Hub::default());
        hub.0.lock().unwrap().peers.insert("student-a".into(), 9);
        hub.submit(
            "message",
            &["student-a".into()],
            &json!({"text":"notice"}),
            &outbox,
        )
        .unwrap();
        hub.disconnected("student-a", 8);
        assert_eq!(hub.snapshot()["jobs"][0]["targets"][0]["status"], "queued");
        hub.disconnected("student-a", 9);
        assert_eq!(hub.snapshot()["jobs"][0]["targets"][0]["status"], "error");
    }
}
