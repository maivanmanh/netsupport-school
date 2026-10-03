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
    ["complete", "replied", "dismissed", "cancelled", "error"].contains(&s)
}
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
        "file".into()
    } else {
        format!("received-{name}")
    }
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
        json!({"ready":s.peers.keys().collect::<Vec<_>>(),"jobs":s.jobs.iter().map(|j|json!({"id":j.packet.id,"kind":j.packet.action,"name":j.packet.name,"size":j.packet.size,"targets":j.targets.iter().map(|(id,t)|json!({"id":id,"status":t.status,"done":t.done,"reply":t.reply,"error":t.error})).collect::<Vec<_>>()})).collect::<Vec<_>>()})
    }
    pub fn submit(
        &self,
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
        let targets = recipients
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    Target {
                        status: "queued".into(),
                        done: 0,
                        reply: String::new(),
                        error: String::new(),
                    },
                )
            })
            .collect();
        for id in recipients {
            s.queues.entry(id.clone()).or_default().push_back(p.clone());
        }
        info!(job_id=%p.id,kind=%action,recipients=recipients.len(),bytes=p.size,"Classroom tools job queued");
        s.jobs.push_back(Job {
            packet: p,
            source,
            targets,
            cancel: Arc::new(AtomicBool::new(false)),
        });
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
    pub fn retry(&self, id: &str, outbox: &Path) -> Result<()> {
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
                "error",
                "cancelled"
            ]
            .contains(&p.status.as_str()),
            "Invalid tools status"
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
        if j.targets.values().all(|t| t.status == "complete") {
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
#[derive(Default)]
pub struct Inbox {
    messages: Mutex<VecDeque<ToolPacket>>,
    events: Mutex<VecDeque<ToolPacket>>,
}
impl Inbox {
    pub fn take(&self) -> Option<ToolPacket> {
        self.messages.lock().unwrap().pop_front()
    }
    pub fn event(&self, id: &str, status: &str, text: &str, done: u64) {
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
    receive_file_to_folder(address, student, p, inbox, cancel, stop, &folder)
}
fn receive_file_to_folder(
    address: SocketAddrV4,
    student: &str,
    p: &ToolPacket,
    inbox: &Inbox,
    cancel: &AtomicBool,
    stop: &AtomicBool,
    folder: &Path,
) -> Result<()> {
    anyhow::ensure!(p.size <= MAX_FILE, "File exceeds 4 GiB limit");
    anyhow::ensure!(
        p.id.len() == 32 && p.id.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid file job id"
    );
    fs::create_dir_all(folder)?;
    let safe_name = safe_name(&p.name);
    let requested = Path::new(&safe_name);
    let prefix = format!(
        "{}-{}",
        &p.id[..12],
        requested.file_stem().unwrap_or_default().to_string_lossy()
    );
    let extension = requested
        .extension()
        .map(|v| v.to_string_lossy().into_owned());
    let (_dest, partial_path) = (0_u32..)
        .find_map(|suffix| {
            let base = if suffix == 0 {
                prefix.clone()
            } else {
                format!("{prefix}-{suffix}")
            };
            let name = extension
                .as_ref()
                .map_or_else(|| base.clone(), |ext| format!("{base}.{ext}"));
            let dest = folder.join(name);
            let partial = folder.join(format!(".{}.part", dest.file_name()?.to_string_lossy()));
            (!dest.exists() && !partial.exists()).then_some((dest, partial))
        })
        .context("Could not choose a unique Desktop filename")?;
    let partial = Partial(partial_path);
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
        .create_new(true)
        .open(&partial.0)?;
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
    // Publish with create_new + copy instead of hard links: redirected Windows
    // Desktop folders and some macOS-managed folders reject hard-link creation.
    // create_new guarantees that a same-named file is never overwritten.
    let mut suffix = 0_u32;
    loop {
        let base = if suffix == 0 {
            prefix.clone()
        } else {
            format!("{prefix}-{suffix}")
        };
        let name = extension
            .as_ref()
            .map_or_else(|| base.clone(), |ext| format!("{base}.{ext}"));
        let candidate = folder.join(name);
        let mut output = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                suffix = suffix
                    .checked_add(1)
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
        fs::remove_file(&partial.0)?;
        break;
    }
    let mut ack = packet("stored");
    ack.hash = digest;
    send(&mut s, &ack)?;
    info!(job_id=%p.id,bytes=done,"Student classroom file saved and verified");
    Ok(())
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
    fn teacher_to_student_file_is_checksummed_and_published_without_partial_file() {
        let temp = TestFolder(test_folder());
        let outbox = temp.0.join("outbox");
        let desktop = temp.0.join("Desktop");
        fs::create_dir_all(&outbox).unwrap();
        fs::create_dir_all(&desktop).unwrap();
        let payload: Vec<u8> = (0..190_000).map(|n| (n % 251) as u8).collect();
        let source = outbox.join("teacher-source");
        fs::write(&source, &payload).unwrap();
        let hub = Arc::new(Hub::default());
        hub.0.lock().unwrap().peers.insert("student-a".into(), 1);
        hub.submit(
            "file",
            &["student-a".into()],
            &json!({"path":source,"name":"lesson notes.txt"}),
            &outbox,
        )
        .unwrap();
        let offer = hub
            .0
            .lock()
            .unwrap()
            .queues
            .get_mut("student-a")
            .unwrap()
            .pop_front()
            .unwrap();
        let preexisting = desktop.join(format!("{}-received-lesson notes.txt", &offer.id[..12]));
        fs::write(&preexisting, b"keep the existing Desktop file").unwrap();
        let listener =
            TcpListener::bind(SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = SocketAddrV4::new(
            std::net::Ipv4Addr::LOCALHOST,
            listener.local_addr().unwrap().port(),
        );
        let stop = Arc::new(AtomicBool::new(false));
        let slots = Arc::new(AtomicUsize::new(0));
        let server_hub = Arc::clone(&hub);
        let server_stop = Arc::clone(&stop);
        let server_slots = Arc::clone(&slots);
        let client_offer = offer.clone();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            configure(&stream).unwrap();
            let request = read(&mut stream).unwrap();
            serve_file(
                &mut stream,
                &request,
                &server_hub,
                &server_stop,
                &server_slots,
            )
            .unwrap();
        });
        let inbox = Inbox::default();
        receive_file_to_folder(
            address,
            "student-a",
            &client_offer,
            &inbox,
            &AtomicBool::new(false),
            &stop,
            &desktop,
        )
        .unwrap();
        server.join().unwrap();
        let entries = fs::read_dir(&desktop)
            .unwrap()
            .collect::<std::io::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            fs::read(&preexisting).unwrap(),
            b"keep the existing Desktop file"
        );
        assert!(entries
            .iter()
            .any(|entry| fs::read(entry.path()).unwrap() == payload));
        assert_eq!(
            hub.snapshot()["jobs"][0]["targets"][0]["status"],
            "complete"
        );
    }

    #[test]
    fn rejected_file_checksum_removes_partial_desktop_file() {
        let temp = TestFolder(test_folder());
        let desktop = temp.0.join("Desktop");
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
        );
        server.join().unwrap();
        assert!(result.is_err());
        assert_eq!(fs::read_dir(desktop).unwrap().count(), 0);
    }

    #[test]
    fn a_student_reply_is_accepted_only_once() {
        let temp = TestFolder(test_folder());
        let outbox = temp.0.join("outbox");
        fs::create_dir_all(&outbox).unwrap();
        let hub = Hub::default();
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
        let hub = Hub::default();
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
