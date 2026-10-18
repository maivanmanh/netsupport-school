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
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};
use tracing::{info, warn};
const VERSION: u32 = 1;
const MAX_PACKET: usize = 65536;
const CHUNK: usize = 64 * 1024;
pub const MAX_FILE: u64 = 4 * 1024 * 1024 * 1024;
mod activity_jobs;
mod app_jobs;
mod file_tree;
mod fs_jobs;
mod hardware;
mod launch;
mod power;
pub(crate) mod recording;
mod evidence;
pub(crate) mod screenshot;
mod vscode_jobs;
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
pub(crate) fn nonce() -> Result<String> {
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
    changed: Instant,
}
struct Job {
    packet: ToolPacket,
    source: Option<Arc<Source>>,
    targets: HashMap<String, Target>,
    cancel: Arc<AtomicBool>,
}
/// Settings the Teacher can change while running; the defaults are how each
/// behaved before it was adjustable.
#[derive(Clone)]
struct Tuning {
    power_grace: u32,
    alert_cooldown: Duration,
    activity: crate::activity::ActivityOptions,
    /// File sending speed per transfer, KiB/s.
    rate_kib_s: u32,
}
impl Default for Tuning {
    fn default() -> Self {
        Self {
            power_grace: 15,
            alert_cooldown: Duration::from_secs(30),
            activity: crate::activity::ActivityOptions::default(),
            rate_kib_s: 1000,
        }
    }
}
#[derive(Default)]
struct State {
    tuning: Tuning,
    /// Screenshots of Students on AI websites, kept for the day.
    evidence: evidence::Store,
    /// Listed AI entries (as typed) and whether they are currently blocked.
    ai_entries: Vec<String>,
    ai_blocking: bool,
    /// Recognises hosts of AI/chatbot websites for the roster highlight.
    ai_matcher: crate::ai_sites::Matcher,
    hardware: hardware::Room,
    badge: HashMap<String, BadgeState>,
    badge_doc: Option<classroom_store::Doc>,
    jobs: VecDeque<Job>,
    queues: HashMap<String, VecDeque<ToolPacket>>,
    peers: HashMap<String, u64>,
    generation: u64,
    capabilities: HashMap<String, Value>,
    /// Students whose classroom lock screen is currently shown.
    locked: std::collections::HashSet<String>,
    /// Students whose clipboard the Teacher is currently clearing on copy.
    clipboard_blocked: std::collections::HashSet<String>,
    /// Internet mode enforced on each Student (absent = open).
    internet: HashMap<String, String>,
    /// Students whose VS Code extension allowlist is currently enforced.
    vscode_blocked: std::collections::HashSet<String>,
    apps_blocked: std::collections::HashSet<String>,
    copilot_blocked: std::collections::HashSet<String>,
    /// Bounded log of "a monitored Student disconnected" events, newest last;
    /// the Teacher UI tracks which it has already shown.
    activity_alerts: VecDeque<Value>,
    /// Suppresses a burst of alerts for the same Student reconnecting
    /// repeatedly in a short window (a flapping connection is one event to
    /// the Teacher, not one per retry).
    last_alert: HashMap<String, Instant>,
    /// Latest compact "what is on this Student's screen" summary, and when it
    /// arrived; shown on the roster tiles while activity monitoring is on.
    activity_live: HashMap<String, (Instant, Value)>,
    /// Program icons (PNG base64) per Student and app, sent only once.
    activity_icons: HashMap<String, HashMap<String, String>>,
}
struct BadgeState {
    enabled: bool,
    actual: Option<bool>,
    revision: String,
    status: String,
    error: String,
    changed: Instant,
}
impl BadgeState {
    fn new(enabled: bool) -> Result<Self> {
        Ok(Self {
            enabled,
            actual: None,
            revision: nonce()?,
            status: "pending".into(),
            error: String::new(),
            changed: Instant::now(),
        })
    }
    fn snapshot(&self) -> Value {
        json!({"enabled":self.enabled,"actual":self.actual,"status":self.status,"error":self.error})
    }
}
#[derive(Default)]
pub struct Hub(
    Mutex<State>,
    AtomicBool,
    OnceLock<String>,
    Mutex<fs_jobs::FsState>,
    AtomicBool,
    Mutex<HashMap<String, Value>>,
    pub(crate) crate::remote_terminal::teacher::Manager,
    Mutex<recording::TeacherState>,
);
impl Hub {
    pub fn load_badges(&self, doc: classroom_store::Doc) -> Result<()> {
        let saved: HashMap<String, bool> = doc.read()?.unwrap_or_default();
        anyhow::ensure!(saved.len() <= 256, "Too many saved badge preferences");
        let mut s = self.0.lock().unwrap();
        s.badge_doc = Some(doc);
        for (id, on) in saved {
            s.badge.insert(id, BadgeState::new(on)?);
        }
        Ok(())
    }
    pub fn set_badges(&self, ids: &[String], enabled: bool) -> Result<()> {
        anyhow::ensure!(
            !ids.is_empty() && ids.len() <= 256,
            "Select at least one computer"
        );
        let mut s = self.0.lock().unwrap();
        anyhow::ensure!(
            s.badge.len() + ids.iter().filter(|id| !s.badge.contains_key(*id)).count() <= 256,
            "Badge preference limit reached"
        );
        let mut saved: HashMap<String, bool> = s
            .badge
            .iter()
            .map(|(id, b)| (id.clone(), b.enabled))
            .collect();
        for id in ids {
            saved.insert(id.clone(), enabled);
        }
        if let Some(doc) = &s.badge_doc {
            doc.write(&saved)?;
        }
        for id in ids {
            let mut b = BadgeState::new(enabled)?;
            if s.capabilities
                .get(id)
                .is_none_or(|c| c["monitoring_badge"] != 1)
            {
                b.status = if s.peers.contains_key(id) {
                    "unsupported"
                } else {
                    "offline"
                }
                .into();
            }
            s.badge.insert(id.clone(), b);
        }
        Ok(())
    }
    fn badge_ack(&self, student: &str, p: &ToolPacket) -> Result<()> {
        anyhow::ensure!(
            matches!(p.status.as_str(), "complete" | "error")
                && p.done <= 1
                && p.text.len() <= 2000,
            "Invalid badge acknowledgement"
        );
        let mut s = self.0.lock().unwrap();
        if let Some(b) = s.badge.get_mut(student).filter(|b| b.revision == p.id) {
            anyhow::ensure!(
                p.status != "complete" || (p.done == 1) == b.enabled,
                "Badge state mismatch"
            );
            b.actual = Some(p.done == 1);
            b.status = if p.status == "complete" {
                "applied"
            } else {
                "error"
            }
            .into();
            b.error = p.text.chars().take(300).collect();
        }
        Ok(())
    }
    pub fn terminal_start(&self, student: &str, id: &str, compatibility: bool, authorization: String) -> Result<()> {
        let mut state = self.0.lock().unwrap();
        anyhow::ensure!(
            state.peers.contains_key(student),
            "Student tools unavailable"
        );
        let queue = state.queues.entry(student.into()).or_default();
        anyhow::ensure!(queue.len() < 16, "Student tools queue full");
        let mut p = packet("terminal_start");
        p.id = id.into();
        p.status = if compatibility { "compat" } else { "native" }.into();
        p.text = authorization;
        queue.push_back(p);
        Ok(())
    }
    pub fn set_connection_popup(&self, enabled: bool) {
        self.1.store(enabled, Ordering::Release);
    }
    pub fn connection_popup(&self) -> bool {
        self.1.load(Ordering::Acquire)
    }
    /// Class-wide switch: while on, every capable Student collects activity
    /// data in the background (see `activity::collector`) regardless of
    /// whether the Teacher is currently looking; while off, none of them do.
    pub fn set_activity_monitoring(&self, enabled: bool) {
        self.4.store(enabled, Ordering::Release);
    }
    pub fn activity_monitoring(&self) -> bool {
        self.4.load(Ordering::Acquire)
    }
    /// The most recently received report for `student`, if any — shown
    /// instantly (no round trip to the Student) while a background refresh
    /// (triggered by the caller) brings it up to date for next time.
    pub fn activity_cached(&self, student: &str) -> Option<Value> {
        self.5.lock().unwrap().get(student).cloned()
    }
    pub(super) fn activity_ingest(&self, student: &str, report: Value) {
        self.5.lock().unwrap().insert(student.to_owned(), report);
    }
    /// Stores a Student's compact live summary (see `activity::live_summary`).
    pub(super) fn activity_live(&self, student: &str, p: &ToolPacket) -> Result<()> {
        anyhow::ensure!(
            p.text.len() <= crate::activity::MAX_SUMMARY_BYTES,
            "Live activity summary too large"
        );
        let mut summary: Value = serde_json::from_str(&p.text)?;
        anyhow::ensure!(summary["apps"].is_array(), "Invalid live activity summary");
        let mut state = self.0.lock().unwrap();
        let icons = state.activity_icons.entry(student.to_owned()).or_default();
        if let Some(apps) = summary["apps"].as_array_mut() {
            apps.truncate(crate::activity::LIVE_APPS);
            for app in apps {
                let Some(icon) = app.as_object_mut().and_then(|o| o.remove("icon")) else {
                    continue;
                };
                if let (Some(name), Some(icon)) = (app["app"].as_str(), icon.as_str()) {
                    if icons.len() < 40 || icons.contains_key(&name.to_ascii_lowercase()) {
                        icons.insert(name.to_ascii_lowercase(), icon.to_owned());
                    }
                }
            }
        }
        state
            .activity_live
            .insert(student.to_owned(), (Instant::now(), summary));
        Ok(())
    }
    /// Live summaries newer than 30 s, with cached program icons filled back
    /// in. Empty while activity monitoring is off.
    fn activity_live_snapshot(&self, state: &State) -> Value {
        if !self.activity_monitoring() {
            return json!({});
        }
        let mut out = serde_json::Map::new();
        for (student, (at, summary)) in &state.activity_live {
            if at.elapsed() > Duration::from_secs(30) || !state.peers.contains_key(student) {
                continue;
            }
            let mut summary = summary.clone();
            // Which listed AI sites are open in any of the Student's tabs; the
            // raw host names stay here.
            let hosts: Vec<String> = summary["hosts"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|h| h.as_str().map(str::to_owned))
                .collect();
            if let Some(map) = summary.as_object_mut() {
                map.remove("hosts");
            }
            let titles: Vec<String> = summary["titles"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| t.as_str().map(str::to_owned))
                .collect();
            if let Some(map) = summary.as_object_mut() {
                map.remove("titles");
            }
            let mut hits = state.ai_matcher.hits(&hosts);
            for host in state.ai_matcher.hits_in_titles(&titles) {
                if !hits.contains(&host) {
                    hits.push(host);
                }
            }
            if !hits.is_empty() {
                summary["ai"] = json!(hits);
            }
            if let (Some(apps), Some(icons)) = (
                summary["apps"].as_array_mut(),
                state.activity_icons.get(student),
            ) {
                for app in apps {
                    let icon = app["app"]
                        .as_str()
                        .and_then(|n| icons.get(&n.to_ascii_lowercase()))
                        .cloned();
                    if let Some(icon) = icon {
                        app["icon"] = json!(icon);
                    }
                }
            }
            out.insert(student.clone(), summary);
        }
        Value::Object(out)
    }
    /// Takes the adjustable settings from `config`; used at start and whenever
    /// the Teacher applies Settings.
    pub fn set_tuning(&self, config: &classroom_config::Config) {
        let mut state = self.0.lock().unwrap();
        state.tuning = Tuning {
            power_grace: config.class.power_grace_seconds,
            alert_cooldown: Duration::from_secs(u64::from(config.activity.alert_cooldown_seconds)),
            activity: crate::activity::ActivityOptions::from_config(&config.activity),
            rate_kib_s: config.transfer.rate_kib_s,
        };
    }
    /// The AI-website list used to highlight Students who visit those sites.
    pub fn set_ai_sites(&self, sites: &[String]) {
        let mut state = self.0.lock().unwrap();
        state.ai_matcher = crate::ai_sites::Matcher::new(sites);
        state.ai_entries = sites.to_vec();
    }
    /// While AI websites are blocked no Student can visit them, so evidence stops.
    pub fn set_ai_blocking(&self, blocking: bool) {
        self.0.lock().unwrap().ai_blocking = blocking;
    }
    /// Midnight: yesterday's evidence is deleted.
    pub fn evidence_roll(&self) {
        self.0.lock().unwrap().evidence.roll_day(crate::local_day::today());
    }
    /// What the Students are told to collect: the Teacher's choices, plus the AI
    /// sites to watch for while they are not blocked.
    fn activity_options_to_send(&self) -> crate::activity::ActivityOptions {
        let state = self.0.lock().unwrap();
        let mut options = state.tuning.activity.clone();
        options.evidence = options.evidence && !state.ai_blocking;
        options.watch = if options.evidence { state.ai_entries.clone() } else { Vec::new() };
        options
    }
    fn tuning(&self) -> Tuning {
        self.0.lock().unwrap().tuning.clone()
    }
    pub fn activity_forget(&self, student: &str) {
        self.5.lock().unwrap().remove(student);
    }
    /// Alerts not yet returned to the caller, oldest first; each is handed
    /// out exactly once — call this from the one place that publishes the
    /// Teacher snapshot, never from an incidental `snapshot()`/lookup call.
    pub fn activity_alerts(&self) -> Vec<Value> {
        let mut state = self.0.lock().unwrap();
        state.activity_alerts.drain(..).collect()
    }
}
impl Hub {
    pub fn request_diagnostics(&self, student: &str, id: &str, token: &str) -> Result<()> {
        let mut state = self.0.lock().unwrap();
        anyhow::ensure!(
            state.peers.contains_key(student),
            "Student tools unavailable"
        );
        let queue = state.queues.entry(student.to_owned()).or_default();
        anyhow::ensure!(queue.len() < 16, "Student tools queue full");
        let mut request = packet("collect_diagnostics");
        id.clone_into(&mut request.id);
        token.clone_into(&mut request.token);
        queue.push_back(request);
        Ok(())
    }
    pub fn snapshot(&self) -> Value {
        let mut s = self.0.lock().unwrap();
        for job in &mut s.jobs {
            for target in job.targets.values_mut() {
                if job.packet.action == "update"
                    && target.status == "restarting"
                    && target.changed.elapsed() > Duration::from_secs(120)
                {
                    target.status = "error".into();
                    target.error = "Updated Student did not confirm reconnect; inspect updater.log or retry after rollback".into();
                }
            }
        }
        for b in s.badge.values_mut() {
            if b.status == "pending" && b.changed.elapsed() > Duration::from_secs(10) {
                b.status = "timeout".into();
                b.error = "Student did not confirm badge display within 10 seconds".into();
            }
        }
        let badges: HashMap<_, _> = s.badge.iter().map(|(id, b)| (id, b.snapshot())).collect();
        let fs_jobs = self.fs_snapshot();
        let mut locked: Vec<&String> = s.locked.iter().collect();
        locked.sort();
        let mut clipboard_blocked: Vec<&String> = s.clipboard_blocked.iter().collect();
        clipboard_blocked.sort();
        let mut vscode_blocked: Vec<&String> = s.vscode_blocked.iter().collect();
        vscode_blocked.sort();
        let mut apps_blocked: Vec<&String> = s.apps_blocked.iter().collect();
        apps_blocked.sort();
        let copilot_blocked: Vec<&String> = s.copilot_blocked.iter().collect();
        json!({"evidence":s.evidence.snapshot(),"activity_live":self.activity_live_snapshot(&s),"recordings":self.recording_snapshot(&s.peers),"hardware":s.hardware.snapshot(&s.peers, &s.capabilities),"monitoring_badges":badges,"connection_popup":self.connection_popup(),"fs_jobs":fs_jobs,"locked":locked,"clipboard_blocked":clipboard_blocked,"vscode_blocked":vscode_blocked,"apps_blocked":apps_blocked,"copilot_blocked":copilot_blocked,"internet":s.internet,"capabilities":s.capabilities,"ready":s.peers.keys().collect::<Vec<_>>(),"jobs":s.jobs.iter().map(|j|json!({"id":j.packet.id,"kind":j.packet.action,"name":j.packet.name,"size":j.packet.size,"text":j.packet.text.chars().take(240).collect::<String>(),"targets":j.targets.iter().map(|(id,t)|json!({"id":id,"status":t.status,"done":t.done,"reply":t.reply,"error":t.error})).collect::<Vec<_>>()})).collect::<Vec<_>>()})
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
    ) -> Result<String> {
        self.submit_with_limit(action, recipients, input, outbox, MAX_FILE)
    }
    pub fn submit_with_limit(
        self: &Arc<Self>,
        action: &str,
        recipients: &[String],
        input: &Value,
        outbox: &Path,
        max_file: u64,
    ) -> Result<String> {
        let mut p = packet(action);
        p.id = nonce()?;
        let source = if action == "file" || action == "update" {
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
            anyhow::ensure!(
                action != "file" || p.size <= max_file,
                "File exceeds Teacher transfer limit ({} MB)",
                max_file / (1024 * 1024)
            );
            p.name = input["name"]
                .as_str()
                .unwrap_or("file")
                .chars()
                .take(140)
                .collect();
            p.token = nonce()?;
            if action == "update" {
                let manifest = super::dev_update::manifest(&source.0)?;
                p.text = serde_json::to_string(&manifest)?;
                anyhow::ensure!(
                    p.size <= super::dev_update::MAX_PACKAGE,
                    "Update exceeds 512 MiB"
                );
            }
            Some(source)
        } else if action == "policy" {
            let policy: crate::guard::Policy =
                serde_json::from_value(input["policy"].clone()).context("Invalid access policy")?;
            policy.validate()?;
            p.text = serde_json::to_string(&policy)?;
            anyhow::ensure!(
                p.text.len() <= 6500,
                "The site list is too long to send (keep it under about 150 entries)"
            );
            p.name = policy.internet.mode.name().into();
            None
        } else if action == "copilot_policy" {
            let enabled = input["enabled"]
                .as_bool()
                .context("Missing Copilot block selection")?;
            p.text = json!({"enabled":enabled}).to_string();
            p.name = if enabled { "block" } else { "open" }.into();
            None
        } else if action == "vscode_policy" {
            let policy: crate::vscode_control::VsCodePolicy =
                serde_json::from_value(input["policy"].clone())
                    .context("Invalid VS Code policy")?;
            policy.validate()?;
            p.text = serde_json::to_string(&policy)?;
            anyhow::ensure!(
                p.text.len() <= 8000,
                "The allowed extension list is too long to send (keep it under about 300 entries)"
            );
            p.name = if policy.enabled {
                "vscode_block"
            } else {
                "vscode_open"
            }
            .into();
            None
        } else if action == "app_policy" {
            let policy: crate::app_control::AppPolicy =
                serde_json::from_value(input["policy"].clone())
                    .context("Invalid application policy")?;
            policy.validate()?;
            p.text = serde_json::to_string(&policy)?;
            anyhow::ensure!(
                p.text.len() <= 48_000,
                "The application list is too long to send (keep it under about 400 entries)"
            );
            p.name = if policy.active() { "app_block" } else { "app_open" }.into();
            None
        } else if action == "power" {
            let op = input["op"].as_str().unwrap_or("");
            anyhow::ensure!(
                [
                    "lock",
                    "unlock",
                    "reboot",
                    "shutdown",
                    "logout",
                    "clipboard_block",
                    "clipboard_allow"
                ]
                .contains(&op),
                "Unknown power action"
            );
            let message: String = input["message"]
                .as_str()
                .unwrap_or("")
                .trim()
                .chars()
                .take(200)
                .collect();
            p.text = json!({"op":op,"message":message,"grace":self.tuning().power_grace})
                .to_string();
            p.name = op.into();
            None
        } else if action == "website" {
            let url = input["url"].as_str().unwrap_or("").trim();
            launch::validate_website(url)?;
            p.text = json!({"url":url}).to_string();
            p.name = "Website".into();
            None
        } else if action == "launch" {
            let path = input["path"].as_str().unwrap_or("").trim();
            let args = input["args"].as_str().unwrap_or("").trim();
            anyhow::ensure!(
                !path.is_empty() && path.chars().count() <= 500,
                "Enter the path of the program to launch (up to 500 characters)"
            );
            anyhow::ensure!(args.chars().count() <= 1500, "Arguments are too long");
            p.text = json!({"path":path,"args":args}).to_string();
            p.name = launch::display_name(path);
            None
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

        if action == "power" {
            for id in recipients {
                anyhow::ensure!(
                    s.capabilities.get(id).is_some_and(|c| c["power"] == 1),
                    "A selected Student runs an older build without power controls; update it via Classroom first"
                );
            }
        }
        if action == "website" {
            for id in recipients {
                anyhow::ensure!(
                    s.capabilities.get(id).is_some_and(|c| c["website"] == 1),
                    "Update selected Students to enable Open website"
                );
            }
        }
        if action == "launch" {
            for id in recipients {
                anyhow::ensure!(
                    s.capabilities.get(id).is_some_and(|c| c["launch"] == 1),
                    "A selected Student runs an older build without remote launch; update it via Classroom first"
                );
            }
        }
        if action == "update" {
            let m: super::dev_update::Manifest = serde_json::from_str(&p.text)?;
            for id in recipients {
                let capability = s
                    .capabilities
                    .get(id)
                    .context("Student has no update capability; bootstrap once manually")?;
                anyhow::ensure!(
                    capability["dev_update"] == 1
                        && capability["os"] == m.os
                        && capability["arch"] == m.arch,
                    "Student {id} needs an updater-enabled build matching {}/{}",
                    m.os,
                    m.arch
                );
                anyhow::ensure!(
                    !s.jobs.iter().any(|j| j.packet.action == "update"
                        && j.targets.get(id).is_some_and(|t| !terminal(&t.status))),
                    "Student {id} already has an active update"
                );
            }
        }
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
                        changed: Instant::now(),
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
        let job_id = p.id.clone();
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
        Ok(job_id)
    }
    pub fn cancel(&self, id: &str) -> Result<()> {
        let mut s = self.0.lock().unwrap();
        let j = s
            .jobs
            .iter_mut()
            .find(|j| j.packet.id == id)
            .context("Job not found")?;
        anyhow::ensure!(
            j.packet.action != "update" || !j.targets.values().any(|t| t.status == "restarting"),
            "Update already committed; wait for reconnect or automatic rollback"
        );
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
        self.retry_with_limit(id, outbox, MAX_FILE)
    }
    pub fn retry_with_limit(
        self: &Arc<Self>,
        id: &str,
        outbox: &Path,
        max_file: u64,
    ) -> Result<()> {
        let (targets, input, action) = {
            let s = self.0.lock().unwrap();
            let j = s
                .jobs
                .iter()
                .find(|j| {
                    j.packet.id == id && (j.packet.action == "file" || j.packet.action == "update")
                })
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
                j.packet.action.clone(),
            )
        };
        // Retrying uses a new staged file/job identity; the previous attempt cannot overwrite it.
        let old = PathBuf::from(input["path"].as_str().unwrap());
        let next = outbox.join(nonce()?);
        fs::copy(old, &next)?;
        self.submit_with_limit(
            &action,
            &targets,
            &json!({"path":next,"name":input["name"]}),
            outbox,
            max_file,
        )
        .map(|_| ())
    }
    fn event(&self, student: &str, p: &ToolPacket) -> Result<()> {
        if p.status == "error" && self.6.failed(&p.id, student, &p.text) {
            return Ok(());
        }
        let mut s = self.0.lock().unwrap();
        let State {
            jobs,
            locked,
            clipboard_blocked,
            internet,
            vscode_blocked,
            apps_blocked,
            copilot_blocked,
            capabilities,
            ..
        } = &mut *s;
        let Some(j) = jobs.iter_mut().find(|j| j.packet.id == p.id) else {
            // A Student can finish an old popup after Teacher restart/history
            // eviction. Acknowledge/discard that event, never poison current jobs.
            info!(job_id=%p.id, student_id=student, status=%p.status, "Stale classroom tools event discarded");
            return Ok(());
        };
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
                "cancelled",
                "restarting"
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
        t.changed = Instant::now();
        if j.packet.action == "policy" && p.status == "complete" {
            if j.packet.name == "open" {
                internet.remove(student);
            } else {
                internet.insert(student.to_owned(), j.packet.name.clone());
            }
        }
        if j.packet.action == "vscode_policy" && p.status == "complete" {
            if j.packet.name == "vscode_open" {
                vscode_blocked.remove(student);
            } else {
                vscode_blocked.insert(student.to_owned());
            }
        }
        if j.packet.action == "app_policy" && p.status == "complete" {
            if j.packet.name == "app_open" {
                apps_blocked.remove(student);
            } else {
                apps_blocked.insert(student.to_owned());
            }
            // Remember what the Student now enforces, so the Teacher's
            // reconcile loop does not resend a rule that is already in force.
            let applied = serde_json::from_str::<crate::app_control::AppPolicy>(&j.packet.text)
                .map(|policy| crate::app_control::policy_hash(&policy))
                .unwrap_or_default();
            if let Some(entry) = capabilities.get_mut(student) {
                entry["apps_policy"] = json!(applied);
                entry["apps_blocked"] = json!(u8::from(j.packet.name != "app_open"));
            }
        }
        if p.status == "complete" {
            // Mirror the new state into the capability record, so the
            // Teacher's reconcile loop sees it as in step without waiting
            // for the Student to reconnect and say so.
            if let Some(entry) = capabilities.get_mut(student) {
                match j.packet.action.as_str() {
                    "policy" => {
                        if let Ok(policy) = serde_json::from_str::<crate::guard::Policy>(&j.packet.text) {
                            entry["internet"] = json!(policy.internet.mode.name());
                            entry["internet_fp"] = json!(crate::guard::fingerprint(&policy.internet));
                        }
                    }
                    "copilot_policy" => {
                        let on = serde_json::from_str::<Value>(&j.packet.text)
                            .is_ok_and(|v| v["enabled"] == true);
                        entry["copilot_blocked"] = json!(u8::from(on));
                    }
                    "vscode_policy" => {
                        if let Ok(policy) =
                            serde_json::from_str::<crate::vscode_control::VsCodePolicy>(&j.packet.text)
                        {
                            entry["vscode_fp"] = json!(policy.fingerprint());
                        }
                    }
                    "power" => {
                        match serde_json::from_str::<Value>(&j.packet.text).ok().and_then(|v| v["op"].as_str().map(str::to_owned)).as_deref() {
                            Some("clipboard_block") => entry["clipboard_blocked"] = json!(1),
                            Some("clipboard_allow") => entry["clipboard_blocked"] = json!(0),
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
        }
        if j.packet.action == "copilot_policy" && p.status == "complete" {
            if j.packet.name == "block" {
                copilot_blocked.insert(student.to_owned());
            } else {
                copilot_blocked.remove(student);
            }
        }
        if j.packet.action == "power" && p.status == "complete" {
            match serde_json::from_str::<Value>(&j.packet.text)
                .ok()
                .and_then(|v| v["op"].as_str().map(str::to_owned))
                .as_deref()
            {
                Some("lock") => {
                    locked.insert(student.to_owned());
                }
                Some("unlock") => {
                    locked.remove(student);
                }
                Some("clipboard_block") => {
                    clipboard_blocked.insert(student.to_owned());
                }
                Some("clipboard_allow") => {
                    clipboard_blocked.remove(student);
                }
                _ => {}
            }
        }
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
    fn commit_update(&self, student: &str, id: &str, size: u64) -> Result<()> {
        let mut state = self.0.lock().unwrap();
        let job = state
            .jobs
            .iter_mut()
            .find(|j| j.packet.id == id)
            .context("Update job disappeared")?;
        anyhow::ensure!(
            !job.cancel.load(Ordering::Acquire),
            "Update cancelled before commit"
        );
        let target = job
            .targets
            .get_mut(student)
            .context("Update recipient missing")?;
        anyhow::ensure!(
            !terminal(&target.status),
            "Update recipient no longer active"
        );
        target.status = "restarting".into();
        target.done = size;
        target.changed = Instant::now();
        Ok(())
    }
    fn confirm_update(&self, student: &str, capability: &Value) -> bool {
        let receipt = &capability["last_update"];
        let mut s = self.0.lock().unwrap();
        let Some(job) = s.jobs.iter_mut().find(|j| {
            j.packet.action == "update"
                && receipt["id"] == j.packet.id
                && receipt["token"] == j.packet.token
        }) else {
            return false;
        };
        let Ok(m) = serde_json::from_str::<super::dev_update::Manifest>(&job.packet.text) else {
            return false;
        };
        if receipt["sha256"] != m.sha256 || receipt["version"] != m.version {
            return false;
        }
        let mut confirmed = false;
        if let Some(target) = job.targets.get_mut(student) {
            confirmed = target.status == "complete";
            if target.status == "restarting" {
                if receipt["outcome"] == "rolled_back" {
                    target.status = "error".into();
                    target.error =
                        "New build could not reconnect; previous build automatically restored"
                            .into();
                    return false;
                }
                confirmed = true;
                target.status = "complete".into();
                target.done = job.packet.size;
                target.reply = format!("{} ({})", m.version, capability["build"]);
                info!(job_id=%job.packet.id,student_id=student,version=%m.version,"Student dev update reconnected and confirmed");
            }
        }
        if job.targets.values().all(|t| terminal(&t.status)) {
            job.source = None;
        }
        confirmed
    }
    fn disconnected(&self, student: &str, generation: u64) {
        let mut state = self.0.lock().unwrap();
        if state.peers.get(student) != Some(&generation) {
            return;
        }
        let alert_cooldown = state.tuning.alert_cooldown;
        if self.activity_monitoring()
            && state
                .last_alert
                .get(student)
                .is_none_or(|at| at.elapsed() >= alert_cooldown)
        {
            state.last_alert.insert(student.to_owned(), Instant::now());
            if state.activity_alerts.len() >= 50 {
                state.activity_alerts.pop_front();
            }
            state.activity_alerts.push_back(json!({
                "id": format!("{student}-{generation}"),
                "student": student,
                "unix_ms": media_core::unix_us() / 1000,
            }));
        }
        state.activity_live.remove(student);
        state.activity_icons.remove(student);
        if let Some(b) = state.badge.get_mut(student) {
            b.status = "offline".into();
            b.actual = None;
        }
        state.peers.remove(student);
        state.capabilities.remove(student);
        state.locked.remove(student);
        state.clipboard_blocked.remove(student);
        state.internet.remove(student);
        state.vscode_blocked.remove(student);
        state.apps_blocked.remove(student);
        state.copilot_blocked.remove(student);
        for job in &mut state.jobs {
            if let Some(target) = job.targets.get_mut(student) {
                if !(terminal(&target.status)
                    || job.packet.action == "update" && target.status == "restarting")
                {
                    target.status = "error".into();
                    target.error = "Student classroom tools connection interrupted".into();
                    info!(job_id=%job.packet.id,student_id=student,status=%target.status,"Classroom tools recipient finished");
                }
            }
        }
        drop(state);
        self.fs_disconnected(student);
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
    diagnostics: Arc<super::diagnostics::Service>,
) -> Result<Server> {
    let listener = TcpListener::bind(address)?;
    let _ = hub.2.set(nonce()?);
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
                    let diagnostics = Arc::clone(&diagnostics);
                    workers.push(thread::spawn(move || {
                        let key = s.local_addr().ok().zip(s.peer_addr().ok());
                        let result = serve(&mut s, peer, &r, &h, &c, &slots, &diagnostics);
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
#[allow(clippy::too_many_arguments)]
fn serve(
    s: &mut TcpStream,
    peer: SocketAddr,
    roster: &Arc<Mutex<RoomRoster>>,
    hub: &Arc<Hub>,
    stop: &AtomicBool,
    slots: &Arc<AtomicUsize>,
    diagnostics: &super::diagnostics::Service,
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
    if hello.action == "terminal_connect" {
        return hub.6.serve(s.try_clone()?, &hello.id, &hello.student);
    }
    if let Some(result) = recording::serve(s, &hello, hub) {
        return result;
    }
    if let Some(result) = evidence::serve(s, &hello, hub) {
        return result;
    }
    if let Some(result) = fs_jobs::serve(s, &hello, hub) {
        return result;
    }
    if hello.action == "diagnostics_upload" {
        diagnostics.receive(s, &hello, stop)?;
        return send(s, &packet("ok"));
    }
    anyhow::ensure!(hello.action == "hello", "Invalid tools handshake");
    let capability = serde_json::from_str::<Value>(&hello.text).ok();
    let generation = {
        let mut st = hub.0.lock().unwrap();
        st.generation += 1;
        let g = st.generation;
        st.peers.insert(hello.student.clone(), g);
        if capability
            .as_ref()
            .is_some_and(|c| c["clipboard_blocked"] == 1)
        {
            st.clipboard_blocked.insert(hello.student.clone());
        } else {
            st.clipboard_blocked.remove(&hello.student);
        }
        match capability.as_ref().and_then(|c| c["internet"].as_str()) {
            Some(mode) if mode != "open" => {
                st.internet.insert(hello.student.clone(), mode.to_owned());
            }
            _ => {
                st.internet.remove(&hello.student);
            }
        }
        if capability.as_ref().is_some_and(|c| c["locked"] == 1) {
            st.locked.insert(hello.student.clone());
        } else {
            st.locked.remove(&hello.student);
        }
        if capability
            .as_ref()
            .is_some_and(|c| c["vscode_blocked"] == 1)
        {
            st.vscode_blocked.insert(hello.student.clone());
        } else {
            st.vscode_blocked.remove(&hello.student);
        }
        if capability.as_ref().is_some_and(|c| c["apps_blocked"] == 1) {
            st.apps_blocked.insert(hello.student.clone());
        } else {
            st.apps_blocked.remove(&hello.student);
        }
        if capability
            .as_ref()
            .is_some_and(|c| c["copilot_blocked"] == 1)
        {
            st.copilot_blocked.insert(hello.student.clone());
        } else {
            st.copilot_blocked.remove(&hello.student);
        }
        if let Some(c) = &capability {
            if c["monitoring_badge"] == 1 && !st.badge.contains_key(&hello.student) {
                let enabled = c["badge_enabled"].as_bool().unwrap_or(false);
                st.badge
                    .insert(hello.student.clone(), BadgeState::new(enabled)?);
            }
        }
        if capability
            .as_ref()
            .is_none_or(|c| c["monitoring_badge"] != 1)
        {
            if let Some(b) = st.badge.get_mut(&hello.student) {
                b.status = "unsupported".into();
                b.actual = None;
            }
        }
        if let Some(c) = &capability {
            st.capabilities.insert(hello.student.clone(), json!({"hardware":c["hardware"],"monitoring_badge":c["monitoring_badge"],"terminal":c["terminal"],"dev_update":c["dev_update"],"connection_popup":c["connection_popup"],"fs":c["fs"],"screenshot":c["screenshot"],"recording":c["recording"],"website":c["website"],"launch":c["launch"],"power":c["power"],"guard":c["guard"],"copilot_network":c["copilot_network"],"activity":c["activity"],"vscode":c["vscode"],"vscode_list":c["vscode_list"],"apps":c["apps"],"apps_blocked":c["apps_blocked"],"apps_policy":c["apps_policy"],"internet":c["internet"],"internet_fp":c["internet_fp"],"vscode_fp":c["vscode_fp"],"copilot_blocked":c["copilot_blocked"],"os":c["os"],"arch":c["arch"],"build":c["build"],"pid":c["pid"]}));
        }
        g
    };
    let popup_supported = capability
        .as_ref()
        .is_some_and(|c| c["connection_popup"] == 1);
    let badge_supported = capability
        .as_ref()
        .is_some_and(|c| c["monitoring_badge"] == 1);
    let hardware_supported = capability.as_ref().is_some_and(|c| c["hardware"] == 1);
    let mut last_hardware = None;
    let mut last_badge = None;
    let mut last_popup = None;
    let activity_supported = capability.as_ref().is_some_and(|c| c["activity"] == 1);
    let mut last_activity_monitoring = None;
    let mut last_activity_options = None;
    let confirmed = capability
        .as_ref()
        .is_some_and(|c| hub.confirm_update(&hello.student, c));
    let result = (|| {
        let mut ack = packet("ok");
        ack.token = hub.2.get().cloned().unwrap_or_default();
        if confirmed {
            ack.status = "update_confirmed".into();
        }
        send(s, &ack)?;
        while !stop.load(Ordering::Acquire) {
            let p = read(s)?;
            anyhow::ensure!(
                registered(roster, &hello.student, peer),
                "Student left classroom"
            );
            if p.action == "recording_status" {
                let verdict = hub.recording_status(&hello.student, &p)?;
                let mut ack = packet("ok");
                ack.status = verdict.into();
                send(s, &ack)?;
            } else if p.action == "activity_live" {
                // A summary that cannot be used is dropped; it must never cost
                // the Student its tools connection.
                if let Err(error) = hub.activity_live(&hello.student, &p) {
                    tracing::debug!(%error, "Ignored a live activity summary");
                }
                send(s, &packet("ok"))?;
            } else if p.action == "hardware_ack" {
                hub.hardware_ack(&hello.student, &p)?;
                send(s, &packet("ok"))?;
            } else if p.action == "monitoring_badge_ack" {
                hub.badge_ack(&hello.student, &p)?;
                send(s, &packet("ok"))?;
            } else if p.action == "event" {
                hub.event(&hello.student, &p)?;
                send(s, &packet("ok"))?;
            } else {
                anyhow::ensure!(p.action == "poll", "Invalid tools action");
                if let Some(policy) =
                    hub.hardware_poll(&hello.student, hardware_supported, &mut last_hardware)?
                {
                    send(s, &policy)?;
                    continue;
                }
                if let Some(request) = hub.recording_poll(&hello.student) {
                    send(s, &request)?;
                    continue;
                }
                let badge = {
                    let mut st = hub.0.lock().unwrap();
                    if badge_supported {
                        let entry = st.badge.get_mut(&hello.student);
                        entry.and_then(|b| {
                            if last_badge.as_ref() == Some(&b.revision) {
                                return None;
                            }
                            b.status = "pending".into();
                            b.error.clear();
                            b.actual = None;
                            b.changed = Instant::now();
                            let mut p = packet("monitoring_badge");
                            p.id = b.revision.clone();
                            p.status = if b.enabled { "on" } else { "off" }.into();
                            Some(p)
                        })
                    } else {
                        None
                    }
                };
                if let Some(badge) = badge {
                    last_badge = Some(badge.id.clone());
                    send(s, &badge)?;
                    continue;
                }
                let popup = hub.connection_popup();
                if popup_supported && last_popup != Some(popup) {
                    let mut preference = packet("connection_status");
                    preference.status = if popup { "on" } else { "off" }.into();
                    send(s, &preference)?;
                    last_popup = Some(popup);
                    continue;
                }
                let monitoring = hub.activity_monitoring();
                let activity_options = hub.activity_options_to_send();
                if activity_supported
                    && (last_activity_monitoring != Some(monitoring)
                        || (monitoring && last_activity_options.as_ref() != Some(&activity_options)))
                {
                    let mut preference = packet("activity_status");
                    preference.status = if monitoring { "on" } else { "off" }.into();
                    preference.text = serde_json::to_string(&activity_options)?;
                    send(s, &preference)?;
                    last_activity_monitoring = Some(monitoring);
                    last_activity_options = Some(activity_options);
                    continue;
                }
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
    let (source, cancel, total, updating) = {
        let st = hub.0.lock().unwrap();
        let j = st
            .jobs
            .iter()
            .find(|j| {
                j.packet.id == p.id
                    && (j.packet.action == "file" || j.packet.action == "update")
                    && j.packet.token == p.token
                    && j.targets.contains_key(&p.student)
            })
            .context("Invalid file request")?;
        anyhow::ensure!(!j.cancel.load(Ordering::Acquire), "File transfer cancelled");
        (
            Arc::clone(j.source.as_ref().context("File source unavailable")?),
            Arc::clone(&j.cancel),
            j.packet.size,
            j.packet.action == "update",
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
    // One block per interval gives the configured speed (1000 KiB/s = 64 ms per 64 KiB).
    let pace = Duration::from_micros(
        (CHUNK as u64 * 1_000_000 / 1024 / u64::from(hub.tuning().rate_kib_s.max(1))).max(500),
    );
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
        thread::sleep(pace.saturating_sub(start.elapsed()));
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
    event.status = if updating { "restarting" } else { "complete" }.into();
    event.done = total;
    if updating {
        hub.commit_update(&p.student, &p.id, total)?;
        send(s, &packet("ok"))?;
    } else {
        hub.event(&p.student, &event)?;
    }
    Ok(())
}
/// 2 minutes plus 10 MiB/s of local copy time.
fn publish_timeout(size: u64) -> Duration {
    Duration::from_secs(120 + size / (10 * 1024 * 1024))
}
#[derive(Default)]
pub struct Inbox {
    pub(crate) badge: Mutex<crate::monitoring_badge::Request>,
    /// Keep the source Student's badge out of its class-broadcast capture.
    pub(crate) badge_broadcast_suppressed: Arc<AtomicBool>,
    /// UI-thread acknowledgement that the source badge window is hidden.
    pub(crate) badge_broadcast_suppression_applied: Arc<AtomicBool>,
    messages: Mutex<VecDeque<ToolPacket>>,
    message_session: Mutex<String>,
    message_epoch: AtomicU64,
    events: Mutex<VecDeque<ToolPacket>>,
    transfers: Mutex<VecDeque<Transfer>>,
    pub update_exit: AtomicBool,
    pub connection_popup: AtomicBool,
    pub terminal_active: AtomicUsize,
    connection_popup_doc: Mutex<Option<classroom_store::Doc>>,
    lock: Mutex<Option<String>>,
    lock_epoch: AtomicU64,
    last_poll_ms: AtomicU64,
    clipboard_guard: Mutex<Option<super::clipboard_guard::ClipboardGuard>>,
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
    pub(crate) fn badge_ack(&self, p: ToolPacket) {
        let mut events = self.events.lock().unwrap();
        events.retain(|old| old.action != "monitoring_badge_ack");
        events.push_back(p);
    }
    pub(crate) fn hardware_ack(&self, p: ToolPacket) {
        let mut events = self.events.lock().unwrap();
        events.retain(|old| old.action != "hardware_ack");
        if events.len() < 256 {
            events.push_back(p);
        }
    }
    pub fn initialize_connection_popup(&self, profile: &Path, default: bool) {
        let doc = super::startup::profile_doc(profile, "connection_popup")
            .map_err(|error| warn!(%error, "Connection popup storage unavailable"))
            .ok();
        let enabled = doc
            .as_ref()
            .and_then(|doc| doc.read::<bool>().ok().flatten())
            .unwrap_or(default);
        self.connection_popup.store(enabled, Ordering::Release);
        *self.connection_popup_doc.lock().unwrap() = doc;
    }
    fn set_connection_popup(&self, enabled: bool) {
        self.connection_popup.store(enabled, Ordering::Release);
        info!(enabled, "Student connection popup preference received");
        if let Some(doc) = self.connection_popup_doc.lock().unwrap().as_ref() {
            if let Err(error) = doc.write(&enabled) {
                tracing::warn!(%error,"Connection popup preference could not be saved");
            }
        }
    }
    pub fn message_epoch(&self) -> u64 {
        self.message_epoch.load(Ordering::Acquire)
    }
    /// Shows (`Some(message)`) or removes the classroom lock screen.
    pub(crate) fn set_lock(&self, message: Option<String>) {
        *self.lock.lock().unwrap() = message;
        self.lock_epoch.fetch_add(1, Ordering::AcqRel);
    }
    /// `(change counter, lock message)`; `None` message means unlocked.
    pub(crate) fn lock_state(&self) -> (u64, Option<String>) {
        (
            self.lock_epoch.load(Ordering::Acquire),
            self.lock.lock().unwrap().clone(),
        )
    }
    fn touch(&self) {
        self.last_poll_ms
            .store(media_core::unix_us() / 1000, Ordering::Release);
    }
    /// Milliseconds since the Teacher last answered a poll (0 before the first).
    pub(crate) fn silent_ms(&self) -> u64 {
        match self.last_poll_ms.load(Ordering::Acquire) {
            0 => 0,
            last => (media_core::unix_us() / 1000).saturating_sub(last),
        }
    }
    /// Starts (or stops) clearing the clipboard on every copy. Idempotent.
    /// Persists across a Student restart and a Teacher disconnect/restart —
    /// lifted only by the Teacher's own choice or the local day ending (see
    /// `clipboard_guard::state`).
    pub(crate) fn set_clipboard_block(&self, blocked: bool) -> Result<()> {
        let mut guard = self.clipboard_guard.lock().unwrap();
        if blocked {
            if guard.is_none() {
                *guard = Some(super::clipboard_guard::ClipboardGuard::start()?);
            }
            super::clipboard_guard::state::mark_applied()?;
        } else {
            *guard = None;
            super::clipboard_guard::state::clear();
        }
        Ok(())
    }
    /// Startup/periodic: holds a block recorded for today, lifts one left
    /// over from an earlier day. Never turns a block on by itself from
    /// nothing — only resumes what was already recorded.
    pub(crate) fn sync_clipboard_block(&self) {
        let should = super::clipboard_guard::state::should_be_applied();
        let has = self.clipboard_guard.lock().unwrap().is_some();
        if should != has {
            let _ = self.set_clipboard_block(should);
        }
    }
    pub(crate) fn clipboard_blocked(&self) -> bool {
        self.clipboard_guard.lock().unwrap().is_some()
    }
    fn bind_message_session(&self, session: &str) {
        if session.is_empty() {
            return;
        } // Older Teachers do not send a session tag.
        let mut current = self.message_session.lock().unwrap();
        if *current != session {
            self.messages.lock().unwrap().clear();
            self.events
                .lock()
                .unwrap()
                .retain(|p| !matches!(p.status.as_str(), "delivered" | "dismissed" | "replied"));
            session.clone_into(&mut current);
            self.message_epoch.fetch_add(1, Ordering::AcqRel);
            // A different Teacher session cannot unlock this Student later —
            // but clipboard block is a day-scoped class rule like the others
            // (Guard, VS Code, applications) and must survive a Teacher
            // restart/reconnect exactly like them, so it is left alone here.
            self.set_lock(None);
            info!("Student message session changed; obsolete popups cleared");
        }
    }
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
pub fn student(
    address: SocketAddrV4,
    identity: String,
    inbox: Arc<Inbox>,
    source: super::diagnostics::Source,
    presenting: Arc<AtomicBool>,
) -> Client {
    let stop = Arc::new(AtomicBool::new(false));
    let cancel = Arc::clone(&stop);
    let socket = Arc::new(Mutex::new(None));
    let active = Arc::clone(&socket);
    let worker = thread::spawn(move || {
        let mut download: Option<Download> = None;
        let mut pending_downloads = VecDeque::<ToolPacket>::new();
        let mut terminals = Vec::<crate::remote_terminal::Worker>::new();
        let mut upload: Option<thread::JoinHandle<()>> = None;
        let mut diagnostic_request: Option<ToolPacket> = None;
        let state_dir = super::startup::profile_path(None)
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(std::env::temp_dir);
        let guard = Arc::new(crate::guard::for_student(&state_dir, *address.ip()));
        guard.restore();
        if guard.supported() {
            // `address` is the tools port (control_port + 2, see the
            // `classroom_tools::student` call site in main.rs); recover the
            // Student's own --control-port so a freshly installed service
            // relaunches it with the same connection info it already has.
            crate::guard_service::ensure_installed(*address.ip(), address.port().saturating_sub(2));
        }
        let hardware_request =
            crate::hardware_control::start(Arc::clone(&inbox), Arc::clone(&cancel), *address.ip());
        let mut fs_run: Option<fs_jobs::Run> = None;
        let mut fs_pending = VecDeque::<ToolPacket>::new();
        let list_active = Arc::new(AtomicUsize::new(0));
        let activity_active = Arc::new(AtomicUsize::new(0));
        let vscode_active = Arc::new(AtomicUsize::new(0));
        let app_active = Arc::new(AtomicUsize::new(0));
        let app_enforcer = super::app_control::Enforcer::start();
        inbox.sync_clipboard_block();
        let mut activity_lifecycle = activity_jobs::Lifecycle::new();
        // Takes screenshots as evidence of AI-site use when the Teacher asks for them.
        let mut ai_watcher = evidence::Watcher::new(&state_dir);
        let mut last_evidence_check = Instant::now();
        // Collection resumes by itself after a Student restart and keeps
        // going while the Teacher is away; it ends with the local day.
        let activity_doc = classroom_store::open_shared(&state_dir)
            .map_err(|error| warn!(%error, "Activity switch storage unavailable"))
            .ok()
            .map(|store| store.doc(classroom_store::ns::PROFILE, "activity_monitoring"));
        let saved_activity = || {
            activity_doc
                .as_ref()
                .and_then(|doc| doc.read::<Value>().ok().flatten())
        };
        let mut activity_monitoring = false;
        if let Some(day) = saved_activity()
            .filter(|v| v["on"] == true)
            .and_then(|v| v["day"].as_u64())
        {
            if day == u64::from(crate::local_day::today()) {
                activity_monitoring = true;
                activity_lifecycle.set_enabled(true);
            }
        }
        let mut last_expiry = Instant::now();
        // Every rule lasts until the Teacher changes it or the local day
        // ends; nothing lapses because the connection did.
        macro_rules! expire_rules {
            () => {
                if last_expiry.elapsed() >= Duration::from_secs(30) {
                    last_expiry = Instant::now();
                    guard.tick();
                    if let Err(error) = crate::vscode_control::expire_if_stale() {
                        warn!(%error, "Could not lift an earlier day's VS Code policy");
                    }
                    if let Err(error) = app_enforcer.expire_if_stale() {
                        warn!(%error, "Could not lift an earlier day's application block");
                    }
                    inbox.sync_clipboard_block();
                    if activity_monitoring
                        && saved_activity().and_then(|v| v["day"].as_u64())
                            != Some(u64::from(crate::local_day::today()))
                    {
                        activity_monitoring = false;
                        activity_lifecycle.set_enabled(false);
                        if let Some(doc) = &activity_doc {
                            let _ = doc.clear();
                        }
                    }
                }
            };
        }
        expire_rules!();
        let recorder =
            recording::Student::new(state_dir.join("recordings"), address, identity.clone());
        fs_jobs::request_folder_access();
        while !cancel.load(Ordering::Acquire) {
            expire_rules!();
            if let Ok(mut s) = TcpStream::connect_timeout(&address.into(), Duration::from_secs(2)) {
                *active.lock().unwrap() = s.try_clone().ok();
                let result = (|| -> Result<()> {
                    configure(&s)?;
                    let mut hello = packet("hello");
                    hello.student.clone_from(&identity);
                    let mut capabilities = super::dev_update::capabilities();
                    capabilities["locked"] = json!(u8::from(inbox.lock_state().1.is_some()));
                    capabilities["clipboard_blocked"] = json!(u8::from(inbox.clipboard_blocked()));
                    capabilities["hardware"] =
                        json!(u8::from(crate::hardware_control::supported()));
                    capabilities["guard"] = json!(u8::from(guard.supported()));
                    capabilities["copilot_network"] = json!(u8::from(guard.supported()));
                    capabilities["copilot_blocked"] = json!(u8::from(guard.copilot_blocked()));
                    capabilities["internet"] = json!(guard.mode());
                    capabilities["internet_fp"] = json!(guard.internet_fingerprint());
                    capabilities["vscode_fp"] = json!(super::vscode_control::applied_fingerprint());
                    capabilities["activity"] = json!(u8::from(cfg!(windows)));
                    capabilities["vscode"] = json!(u8::from(super::vscode_control::supported()));
                    capabilities["vscode_list"] =
                        json!(u8::from(cfg!(any(windows, target_os = "macos"))));
                    capabilities["vscode_blocked"] =
                        json!(u8::from(super::vscode_control::is_applied()));
                    capabilities["apps"] = json!(u8::from(super::app_control::supported()));
                    capabilities["apps_blocked"] = json!(u8::from(super::app_control::is_applied()));
                    capabilities["apps_policy"] = json!(super::app_control::applied_hash());
                    capabilities["monitoring_badge"] =
                        json!(u8::from(crate::monitoring_badge::supported()));
                    capabilities["badge_enabled"] = json!(inbox.badge.lock().unwrap().enabled);
                    capabilities["screenshot"] = json!(u8::from(cfg!(any(windows, target_os = "macos"))));
                    capabilities["recording"] = json!(u8::from(cfg!(windows)));
                    capabilities["website"] = json!(u8::from(cfg!(any(windows, target_os = "macos"))));
                    capabilities["terminal"] = json!(u8::from(crate::remote_terminal::available()));
                    hello.text = capabilities.to_string();
                    send(&mut s, &hello)?;
                    let ack = read(&mut s)?;
                    anyhow::ensure!(ack.action == "ok", "Tools handshake rejected");
                    if ack.status == "update_confirmed" {
                        super::dev_update::connected();
                    }
                    inbox.bind_message_session(&ack.token);
                    info!("Student classroom tools ready");
                    // Per connection: the Teacher's icon cache starts empty too.
                    let mut sent_icons = std::collections::HashSet::<String>::new();
                    let mut last_live_text = String::new();
                    let mut last_live_sent: Option<Instant> = None;
                    while !cancel.load(Ordering::Acquire) {
                        terminals.retain_mut(|worker| !worker.finished());
                        inbox
                            .terminal_active
                            .store(terminals.len(), Ordering::Release);
                        recorder.note_contact();
                        if activity_monitoring
                            && last_live_sent.is_none_or(|at| at.elapsed() >= Duration::from_secs(3))
                        {
                            let summary = activity_lifecycle
                                .live()
                                .lock()
                                .unwrap()
                                .as_ref()
                                .and_then(|r| {
                                    crate::activity::live_summary(
                                        r,
                                        &mut sent_icons,
                                        activity_lifecycle.options().live_apps as usize,
                                    )
                                });
                            if let Some(summary) = summary {
                                let text = summary.to_string();
                                let heartbeat_due = last_live_sent
                                    .is_none_or(|at| at.elapsed() >= Duration::from_secs(12));
                                if heartbeat_due || last_live_text != text {
                                    let mut p = packet("activity_live");
                                    p.student.clone_from(&identity);
                                    p.text.clone_from(&text);
                                    send(&mut s, &p)?;
                                    anyhow::ensure!(
                                        read(&mut s)?.action == "ok",
                                        "Live activity summary rejected"
                                    );
                                    last_live_text = text;
                                    last_live_sent = Some(Instant::now());
                                }
                            }
                        }
                        if activity_monitoring && last_evidence_check.elapsed() >= Duration::from_secs(2) {
                            last_evidence_check = Instant::now();
                            let plan = activity_lifecycle
                                .live()
                                .lock()
                                .unwrap()
                                .as_ref()
                                .and_then(|report| ai_watcher.observe(report, crate::activity::now_ms()));
                            if let Some(plan) = plan {
                                ai_watcher.capture(
                                    plan,
                                    address,
                                    identity.clone(),
                                    state_dir.clone(),
                                    Arc::clone(&cancel),
                                );
                            }
                        }
                        for p in recorder.statuses() {
                            send(&mut s, &p)?;
                            let ack = read(&mut s)?;
                            anyhow::ensure!(ack.action == "ok", "Recording status rejected");
                            if ack.status == "stop" {
                                let mut stop = p;
                                stop.action = "recording_stop".into();
                                let _ = recorder.command(stop);
                            } else if ack.status == "discard" {
                                // The Teacher deleted this recording: drop what is left here.
                                let mut discard = p;
                                discard.action = "recording_discard".into();
                                let _ = recorder.command(discard);
                            }
                        }
                        for _ in 0..16 {
                            let p = inbox.events.lock().unwrap().pop_front();
                            let Some(p) = p else { break };
                            send(&mut s, &p)?;
                            anyhow::ensure!(read(&mut s)?.action == "ok", "Tools event rejected");
                        }
                        send(&mut s, &packet("poll"))?;
                        let p = read(&mut s)?;
                        // A real command may be followed by more queued work: ask
                        // again at once instead of idling a full interval.
                        let had_work = p.action != "idle";
                        inbox.touch();
                        guard.keepalive();
                        expire_rules!();
                        if upload.as_ref().is_some_and(thread::JoinHandle::is_finished) {
                            let _ = upload.take().unwrap().join();
                        }
                        // Stop is asynchronous at each Student; retain one request until teardown completes.
                        if upload.is_none() && !presenting.load(Ordering::Acquire) {
                            if let Some(p) = diagnostic_request.take() {
                                let source = source.clone();
                                let identity = identity.clone();
                                let cancel = Arc::clone(&cancel);
                                let presenting = Arc::clone(&presenting);
                                upload = Some(thread::spawn(move || {
                                    if upload_diagnostics(
                                        address,
                                        &identity,
                                        &p,
                                        &source,
                                        &cancel,
                                        &presenting,
                                    )
                                    .is_err()
                                    {
                                        info!("Student diagnostic upload unavailable or cancelled");
                                    }
                                }));
                            }
                        }
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
                        if fs_run.as_ref().is_some_and(|r| r.worker.is_finished()) {
                            let _ = fs_run.take().unwrap().worker.join();
                        }
                        if fs_run.is_none() {
                            if let Some(next) = fs_pending.pop_front() {
                                fs_run = Some(fs_jobs::start(
                                    address,
                                    identity.clone(),
                                    next,
                                    Arc::clone(&cancel),
                                ));
                            }
                        }
                        match p.action.as_str() {
                            "hardware_policy" => {
                                let policy: crate::hardware_control::Policy =
                                    serde_json::from_str(&p.text)?;
                                policy.validate()?;
                                anyhow::ensure!(
                                    policy.revision == p.id,
                                    "Hardware revision mismatch"
                                );
                                *hardware_request.lock().unwrap() = Some(policy);
                            }
                            "monitoring_badge" => {
                                anyhow::ensure!(
                                    matches!(p.status.as_str(), "on" | "off") && p.id.len() == 32,
                                    "Invalid badge request"
                                );
                                let mut b = inbox.badge.lock().unwrap();
                                b.enabled = p.status == "on";
                                b.revision = Some(p.id);
                            }
                            "collect_diagnostics" => diagnostic_request = Some(p),
                            "terminal_start" => {
                                let outcome = if terminals.len() >= 4 {
                                    Err(anyhow::anyhow!("Student terminal limit reached"))
                                } else {
                                    crate::remote_terminal::start(
                                        address,
                                        identity.clone(),
                                        p.id.clone(),
                                        p.status != "native",
                                        &p.text,
                                    )
                                };
                                match outcome {
                                    Ok(worker) => terminals.push(worker),
                                    Err(error) => {
                                        inbox.event(&p.id, "error", &format!("{error:#}"), 0)
                                    }
                                }
                            }
                            "fs_list" => {
                                fs_jobs::list(address, identity.clone(), p, &list_active);
                            }
                            "activity" => {
                                activity_jobs::run(
                                    address,
                                    identity.clone(),
                                    p,
                                    &activity_active,
                                    activity_lifecycle.live(),
                                    activity_monitoring,
                                );
                            }
                            "activity_status" => {
                                anyhow::ensure!(
                                    matches!(p.status.as_str(), "on" | "off"),
                                    "Invalid activity monitoring preference"
                                );
                                activity_monitoring = p.status == "on";
                                if let Ok(options) = serde_json::from_str::<crate::activity::ActivityOptions>(&p.text) {
                                    ai_watcher.configure(&options);
                                    activity_lifecycle.set_options(options);
                                }
                                activity_lifecycle.set_enabled(activity_monitoring);
                                if let Some(doc) = &activity_doc {
                                    let _ = if activity_monitoring {
                                        doc.write(&json!({"on": true, "day": crate::local_day::today()}))
                                    } else {
                                        doc.clear()
                                    };
                                }
                            }
                            "launch" | "website" => launch::run(p, Arc::clone(&inbox)),
                            "power" => power::run(p, Arc::clone(&inbox)),
                            "policy" => {
                                let (guard, inbox) = (Arc::clone(&guard), Arc::clone(&inbox));
                                thread::spawn(move || {
                                    let outcome =
                                        serde_json::from_str::<crate::guard::Policy>(&p.text)
                                            .context("Invalid access policy")
                                            .and_then(|policy| guard.apply_internet(policy));
                                    match outcome {
                                        Ok(()) => inbox.event(&p.id, "complete", "", 0),
                                        Err(error) => {
                                            inbox.event(&p.id, "error", &format!("{error:#}"), 0)
                                        }
                                    }
                                });
                            }
                            "copilot_policy" => {
                                let (guard, inbox) = (Arc::clone(&guard), Arc::clone(&inbox));
                                thread::spawn(move || {
                                    let outcome = serde_json::from_str::<Value>(&p.text)
                                        .context("Invalid Copilot policy")
                                        .and_then(|v| {
                                            v["enabled"].as_bool().context("Missing enabled flag")
                                        })
                                        .and_then(|enabled| guard.set_copilot(enabled));
                                    match outcome {
                                        Ok(()) => inbox.event(&p.id, "complete", "", 0),
                                        Err(error) => {
                                            inbox.event(&p.id, "error", &format!("{error:#}"), 0)
                                        }
                                    }
                                });
                            }
                            "vscode_list" => {
                                vscode_jobs::run(address, identity.clone(), p, &vscode_active);
                            }
                            "app_list" => {
                                app_jobs::run(address, identity.clone(), p, &app_active);
                            }
                            "app_policy" => {
                                let inbox = Arc::clone(&inbox);
                                let enforcer = app_enforcer.clone();
                                thread::spawn(move || {
                                    let outcome = serde_json::from_str::<crate::app_control::AppPolicy>(
                                        &p.text,
                                    )
                                    .context("Invalid application policy")
                                    .and_then(|policy| {
                                        policy.validate()?;
                                        enforcer.apply(&policy)
                                    });
                                    match outcome {
                                        Ok(()) => inbox.event(&p.id, "complete", "", 0),
                                        Err(error) => {
                                            inbox.event(&p.id, "error", &format!("{error:#}"), 0)
                                        }
                                    }
                                });
                            }
                            "vscode_policy" => {
                                let inbox = Arc::clone(&inbox);
                                thread::spawn(move || {
                                    let outcome = serde_json::from_str::<
                                        crate::vscode_control::VsCodePolicy,
                                    >(&p.text)
                                    .context("Invalid VS Code policy")
                                    .and_then(|policy| {
                                        policy.validate()?;
                                        crate::vscode_control::apply(&policy)
                                    });
                                    match outcome {
                                        Ok(()) => inbox.event(&p.id, "complete", "", 0),
                                        Err(error) => {
                                            inbox.event(&p.id, "error", &format!("{error:#}"), 0)
                                        }
                                    }
                                });
                            }
                            "recording_start" | "recording_stop" => {
                                recorder.command(p)?;
                            }
                            "fs_send" | "fs_receive" | "screenshot" => {
                                if fs_run.is_none() {
                                    fs_run = Some(fs_jobs::start(
                                        address,
                                        identity.clone(),
                                        p,
                                        Arc::clone(&cancel),
                                    ));
                                } else if fs_pending.len() < 4 {
                                    fs_pending.push_back(p);
                                }
                            }
                            "connection_status" => {
                                anyhow::ensure!(
                                    matches!(p.status.as_str(), "on" | "off"),
                                    "Invalid connection preference"
                                );
                                inbox.set_connection_popup(p.status == "on");
                            }
                            "message" => {
                                let mut q = inbox.messages.lock().unwrap();
                                if q.len() < 16 {
                                    inbox.event(&p.id, "delivered", "", 0);
                                    q.push_back(p);
                                } else {
                                    inbox.event(&p.id, "error", "Student message inbox is full", 0);
                                }
                            }
                            "file" | "update" => {
                                if p.action == "file" {
                                    inbox.track_file(&p);
                                }
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
                                if let Some(r) = fs_run.as_ref().filter(|r| r.id == p.id) {
                                    r.cancel.store(true, Ordering::Release);
                                }
                                fs_pending.retain(|queued| queued.id != p.id);
                                inbox.messages.lock().unwrap().retain(|m| m.id != p.id);
                            }
                            "idle" => {}
                            // Forward compatible: a Teacher newer than this Student may
                            // push a message type this build does not know yet (e.g. a
                            // feature-status toggle). Ignoring it keeps the whole tools
                            // connection alive instead of tearing it down and looping.
                            other => {
                                info!(
                                    action = other,
                                    "Student ignored an unrecognized classroom tool message"
                                );
                            }
                        }
                        for _ in 0..if had_work { 0 } else { 10 } {
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
        if let Some(upload) = upload {
            let _ = upload.join();
        }
        if let Some(run) = fs_run {
            run.cancel.store(true, Ordering::Release);
            let _ = run.worker.join();
        }
    });
    Client {
        stop,
        socket,
        worker: Some(worker),
    }
}
fn upload_diagnostics(
    address: SocketAddrV4,
    identity: &str,
    request: &ToolPacket,
    source: &super::diagnostics::Source,
    stop: &AtomicBool,
    presenting: &AtomicBool,
) -> Result<()> {
    let (metadata, log) = source.capture()?;
    let mut socket = TcpStream::connect_timeout(&address.into(), Duration::from_secs(2))?;
    configure(&socket)?;
    let mut header = packet("diagnostics_upload");
    header.id.clone_from(&request.id);
    header.token.clone_from(&request.token);
    identity.clone_into(&mut header.student);
    header.size = log.len() as u64;
    header.text = serde_json::to_string(&metadata)?;
    send(&mut socket, &header)?;
    for chunk in log.chunks(super::diagnostics::CHUNK) {
        anyhow::ensure!(
            !stop.load(Ordering::Acquire) && !presenting.load(Ordering::Acquire),
            "diagnostic upload cancelled"
        );
        let started = Instant::now();
        socket.write_all(chunk)?;
        thread::sleep(super::diagnostics::PACE.saturating_sub(started.elapsed()));
    }
    anyhow::ensure!(
        read(&mut socket)?.action == "ok",
        "diagnostic upload rejected"
    );
    Ok(())
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
    if p.action == "update" {
        let folder = std::env::temp_dir().join(format!("Classroom-update-download-{}", p.id));
        fs::create_dir_all(&folder)?;
        let result =
            receive_file_to_folder(address, student, p, inbox, cancel, stop, &folder, &folder);
        let _ = fs::remove_dir_all(folder);
        return result;
    }
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
    anyhow::ensure!(
        p.size
            <= if p.action == "update" {
                super::dev_update::MAX_PACKAGE
            } else {
                MAX_FILE
            },
        "File exceeds size limit"
    );
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
        checksum.action == "checksum" && checksum.hash == digest && digest == p.hash,
        "File checksum mismatch"
    );
    anyhow::ensure!(
        !stop.load(Ordering::Acquire) && !cancel.load(Ordering::Acquire),
        "File cancelled"
    );
    file.sync_all()?;
    drop(file);
    if p.action == "update" {
        let plan = super::dev_update::prepare(&partial.0, &p.id, &p.token, cancel)?;
        anyhow::ensure!(
            !cancel.load(Ordering::Acquire) && !stop.load(Ordering::Acquire),
            "Update cancelled before commit"
        );
        let mut ack = packet("stored");
        ack.hash = digest;
        send(&mut s, &ack)?;
        anyhow::ensure!(
            read(&mut s)?.action == "ok",
            "Teacher did not commit update"
        );
        // Spawn first; failure keeps the current Student alive.
        super::dev_update::launch_worker(&plan)?;
        inbox.update_exit.store(true, Ordering::Release);
        info!(job_id=%p.id,plan=%plan.display(),"Student dev update prepared; releasing all workers before swap");
        return Ok(());
    }
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

/// A well-known per-user folder (`Desktop`, `Documents`, `Downloads`, or `""`
/// for the home folder), following Windows shell redirection such as OneDrive.
#[cfg(windows)]
#[allow(unsafe_code)]
pub(crate) fn user_folder(name: &str) -> Result<PathBuf> {
    use windows::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{
            FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_Profile,
            SHGetKnownFolderPath, KF_FLAG_DEFAULT,
        },
    };
    let id = match name {
        "Desktop" => &FOLDERID_Desktop,
        "Documents" => &FOLDERID_Documents,
        "Downloads" => &FOLDERID_Downloads,
        _ => &FOLDERID_Profile,
    };
    // SAFETY: Shell returns a CoTaskMem-allocated UTF-16 path for this user's
    // folder. Copy it and free exactly once.
    unsafe {
        let path = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None)?;
        let text = path.to_string();
        CoTaskMemFree(Some(path.0.cast()));
        Ok(PathBuf::from(text?))
    }
}
/// The account's real home. A sandboxed macOS Teacher helper has `$HOME` set to
/// its container, so the account database is asked first.
#[cfg(not(windows))]
#[allow(unsafe_code)]
fn real_home() -> Result<PathBuf> {
    use std::{ffi::CStr, os::unix::ffi::OsStrExt};
    let mut buffer = vec![0 as libc::c_char; 8192];
    // SAFETY: an all-zero `passwd` is a valid out-parameter; getpwuid_r fills it
    // and `result` only with pointers into `buffer`, which outlives their use.
    let home = unsafe {
        let mut entry: libc::passwd = std::mem::zeroed();
        let mut result: *mut libc::passwd = std::ptr::null_mut();
        let status = libc::getpwuid_r(
            libc::getuid(),
            &mut entry,
            buffer.as_mut_ptr(),
            buffer.len(),
            &mut result,
        );
        (status == 0 && !result.is_null() && !entry.pw_dir.is_null()).then(|| {
            PathBuf::from(std::ffi::OsStr::from_bytes(
                CStr::from_ptr(entry.pw_dir).to_bytes(),
            ))
        })
    };
    home.filter(|h| h.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .context("User home missing")
}
#[cfg(not(windows))]
pub(crate) fn user_folder(name: &str) -> Result<PathBuf> {
    let home = real_home()?;
    Ok(if name.is_empty() {
        home
    } else {
        home.join(name)
    })
}
fn desktop_folder() -> Result<PathBuf> {
    user_folder("Desktop")
}
/// A not-yet-existing folder on the Teacher Desktop for one collection run.
pub fn collection_root(label: &str) -> Result<PathBuf> {
    let desktop = desktop_folder()?;
    let name = safe_name(if label.trim().is_empty() {
        "Collected files"
    } else {
        label.trim()
    });
    for index in 0..1000 {
        let candidate = desktop.join(numbered_name(&name, index));
        if fs::symlink_metadata(&candidate).is_err() {
            return Ok(candidate);
        }
    }
    anyhow::bail!("No free collection folder name on the Desktop")
}
#[cfg(test)]
mod tests {
    #[test]
    fn a_student_on_an_ai_site_is_flagged_in_the_snapshot_even_with_the_browser_in_the_background() {
        let hub = Hub::default();
        hub.0.lock().unwrap().peers.insert("s1".into(), 1);
        hub.set_activity_monitoring(true);
        hub.set_ai_sites(&crate::ai_sites::defaults());
        let mut p = packet("activity_live");
        p.text = json!({"apps":[{"app":"Code.exe","title":"main.rs"}],
                        "url":"https://docs.rs","hosts":["docs.rs","chatgpt.com"]}).to_string();
        hub.activity_live("s1", &p).unwrap();
        let shown = hub.snapshot()["activity_live"]["s1"].clone();
        assert_eq!(shown["ai"], json!(["chatgpt.com"]));
        assert!(shown.get("hosts").is_none(), "raw hosts are not published");
        // A tab whose address is not known yet is still recognised by its title.
        p.text = json!({"apps":[{"app":"Code.exe","title":"x"}],
                        "hosts":["tinteo.vn"],"titles":["tinteo.vn","ChatGPT"]}).to_string();
        hub.activity_live("s1", &p).unwrap();
        let shown = hub.snapshot()["activity_live"]["s1"].clone();
        assert_eq!(shown["ai"], json!(["chatgpt.com"]));
        assert!(shown.get("titles").is_none(), "raw titles are not published");
        // Not flagged when nothing on the list is open.
        p.text = json!({"apps":[{"app":"Code.exe","title":"x"}],"hosts":["docs.rs"]}).to_string();
        hub.activity_live("s1", &p).unwrap();
        assert!(hub.snapshot()["activity_live"]["s1"].get("ai").is_none());
        // The list is the Teacher's: with an empty one nobody is flagged.
        p.text = json!({"apps":[{"app":"Code.exe","title":"x"}],"hosts":["chatgpt.com"]}).to_string();
        hub.activity_live("s1", &p).unwrap();
        hub.set_ai_sites(&[]);
        assert!(hub.snapshot()["activity_live"]["s1"].get("ai").is_none());
    }

    #[test]
    fn adjustable_settings_reach_the_hub_and_default_to_todays_behaviour() {
        let hub = Hub::default();
        let before = hub.tuning();
        assert_eq!(before.power_grace, 15);
        assert_eq!(before.alert_cooldown, Duration::from_secs(30));
        assert_eq!(before.rate_kib_s, 1000);
        assert_eq!(before.activity, crate::activity::ActivityOptions::default());
        let mut config = classroom_config::Config::load(None).unwrap().0;
        config.class.power_grace_seconds = 45;
        config.activity.scan_seconds = 20;
        config.activity.live_apps = 2;
        config.activity.collect_clipboard = false;
        config.activity.alert_cooldown_seconds = 120;
        config.transfer.rate_kib_s = 4096;
        hub.set_tuning(&config);
        let now = hub.tuning();
        assert_eq!(now.power_grace, 45);
        assert_eq!(now.alert_cooldown, Duration::from_secs(120));
        assert_eq!(now.rate_kib_s, 4096);
        assert_eq!(now.activity.scan_seconds, 20);
        assert_eq!(now.activity.live_apps, 2);
        assert!(!now.activity.collect_clipboard);
    }
    #[test]
    fn live_activity_reaches_the_snapshot_with_cached_icons_only_while_monitoring() {
        let hub = Hub::default();
        hub.0.lock().unwrap().peers.insert("s1".into(), 1);
        let mut first = packet("activity_live");
        first.text = json!({"apps":[{"app":"Chrome.exe","title":"Docs","icon":"AAAA"}],"url":"https://a.dev"}).to_string();
        hub.activity_live("s1", &first).unwrap();
        // Off by default: nothing is published.
        assert_eq!(hub.snapshot()["activity_live"], json!({}));
        hub.set_activity_monitoring(true);
        let shown = hub.snapshot()["activity_live"]["s1"].clone();
        assert_eq!(shown["apps"][0]["icon"], "AAAA");
        assert_eq!(shown["url"], "https://a.dev");
        // A later summary without the icon still shows it, from the cache.
        let mut next = packet("activity_live");
        next.text = json!({"apps":[{"app":"chrome.exe","title":"Other"}]}).to_string();
        hub.activity_live("s1", &next).unwrap();
        let shown = hub.snapshot()["activity_live"]["s1"].clone();
        assert_eq!(shown["apps"][0]["icon"], "AAAA");
        assert_eq!(shown["apps"][0]["title"], "Other");
        assert!(shown.get("url").is_none());
        // Malformed or oversized input is refused.
        let mut bad = packet("activity_live");
        bad.text = "{\"apps\":1}".into();
        assert!(hub.activity_live("s1", &bad).is_err());
        bad.text = "x".repeat(crate::activity::MAX_SUMMARY_BYTES + 1);
        assert!(hub.activity_live("s1", &bad).is_err());
        // A Student that is no longer connected disappears.
        hub.0.lock().unwrap().peers.clear();
        assert_eq!(hub.snapshot()["activity_live"], json!({}));
    }
    use super::*;
    #[test]
    fn teacher_file_limit_rejects_before_hashing_and_removes_staging() {
        let dir = std::env::temp_dir().join(format!("classroom-limit-{}", nonce().unwrap()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("staged");
        fs::write(&file, [0; 17]).unwrap();
        let hub = Arc::new(Hub::default());
        let error = hub
            .submit_with_limit(
                "file",
                &["student".into()],
                &json!({"path":file,"name":"file.txt"}),
                &dir,
                16,
            )
            .unwrap_err();
        assert!(error.to_string().contains("Teacher transfer limit"));
        assert!(!file.exists());
        assert!(hub.0.lock().unwrap().jobs.is_empty());
        fs::remove_dir_all(dir).unwrap();
    }
    fn update_job() -> (Hub, Value) {
        let hub = Hub::default();
        let manifest = json!({"format":1,"os":"windows","arch":"x86_64","version":"dev-test",
            "root":"ClassroomStudent.exe","executable":"ClassroomStudent.exe","sha256":"a".repeat(64)});
        let mut offer = packet("update");
        offer.id = "update-id".into();
        offer.token = "private-update-token".into();
        offer.text = manifest.to_string();
        offer.size = 100;
        hub.0.lock().unwrap().jobs.push_back(Job {
            packet: offer,
            source: None,
            targets: HashMap::from([(
                "student".into(),
                Target {
                    status: "receiving".into(),
                    done: 99,
                    reply: String::new(),
                    error: String::new(),
                    changed: Instant::now(),
                },
            )]),
            cancel: Arc::new(AtomicBool::new(false)),
        });
        hub.0.lock().unwrap().peers.insert("student".into(), 1);
        let receipt = json!({"build":"new-build","last_update":{"id":"update-id","token":"private-update-token",
            "version":"dev-test","sha256":"a".repeat(64),"outcome":"installed"}});
        (hub, receipt)
    }
    #[test]
    fn update_success_requires_committed_reconnect_and_matching_receipt() {
        let (hub, receipt) = update_job();
        assert!(!hub.confirm_update("student", &receipt));
        hub.commit_update("student", "update-id", 100).unwrap();
        assert!(hub.cancel("update-id").is_err());
        hub.disconnected("student", 1);
        assert_eq!(
            hub.snapshot()["jobs"][0]["targets"][0]["status"],
            "restarting"
        );
        let mut wrong = receipt.clone();
        wrong["last_update"]["token"] = json!("wrong");
        assert!(!hub.confirm_update("student", &wrong));
        wrong = receipt.clone();
        wrong["last_update"]["sha256"] = json!("b".repeat(64));
        assert!(!hub.confirm_update("student", &wrong));
        assert!(!hub.confirm_update("other-student", &receipt));
        assert!(hub.confirm_update("student", &receipt));
        assert_eq!(
            hub.snapshot()["jobs"][0]["targets"][0]["status"],
            "complete"
        );
        assert!(hub.confirm_update("student", &receipt)); // idempotent reconnect
    }
    #[test]
    fn cancelled_update_cannot_commit_and_rollback_is_reported_as_failure() {
        let (hub, _) = update_job();
        hub.cancel("update-id").unwrap();
        assert!(hub.commit_update("student", "update-id", 100).is_err());
        let (hub, mut receipt) = update_job();
        hub.commit_update("student", "update-id", 100).unwrap();
        receipt["last_update"]["outcome"] = json!("rolled_back");
        assert!(!hub.confirm_update("student", &receipt));
        assert_eq!(hub.snapshot()["jobs"][0]["targets"][0]["status"], "error");
    }
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
    fn stale_message_event_does_not_interrupt_current_job_or_peer() {
        let temp = TestFolder(test_folder());
        let outbox = temp.0.join("outbox");
        fs::create_dir_all(&outbox).unwrap();
        let hub = Arc::new(Hub::default());
        hub.0.lock().unwrap().peers.insert("student-a".into(), 1);
        hub.submit(
            "message",
            &["student-a".into()],
            &json!({"text":"Current message","allow_reply":true}),
            &outbox,
        )
        .unwrap();
        let id = hub.0.lock().unwrap().jobs[0].packet.id.clone();
        let mut delivered = packet("event");
        delivered.id = id;
        delivered.status = "delivered".into();
        hub.event("student-a", &delivered).unwrap();
        let mut stale = packet("event");
        stale.id = "previous-teacher-message".into();
        stale.status = "replied".into();
        stale.text = "obsolete reply".into();
        hub.event("student-a", &stale).unwrap();
        let snapshot = hub.snapshot();
        assert_eq!(snapshot["jobs"][0]["targets"][0]["status"], "delivered");
        assert_eq!(snapshot["jobs"][0]["targets"][0]["reply"], "");
        assert_eq!(snapshot["ready"][0], "student-a");
        delivered.status = "replied".into();
        delivered.text = "current reply".into();
        hub.event("student-a", &delivered).unwrap();
        assert_eq!(
            hub.snapshot()["jobs"][0]["targets"][0]["reply"],
            "current reply"
        );
    }

    #[test]
    fn only_new_teacher_session_clears_message_windows_and_events() {
        let inbox = Inbox::default();
        inbox.bind_message_session("session-a");
        let epoch = inbox.message_epoch();
        let mut message = packet("message");
        message.id = "old".into();
        inbox.messages.lock().unwrap().push_back(message);
        inbox.event("old", "replied", "answer", 0);
        inbox.event("file", "receiving", "", 1);
        inbox.bind_message_session("session-a");
        assert_eq!(inbox.message_epoch(), epoch);
        assert_eq!(inbox.messages.lock().unwrap().len(), 1);
        inbox.bind_message_session(""); // Legacy handshake must not wipe state.
        assert_eq!(inbox.message_epoch(), epoch);
        inbox.bind_message_session("session-b");
        assert!(inbox.take().is_none());
        assert_eq!(inbox.message_epoch(), epoch + 1);
        let events = inbox.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, "file");
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
    fn disconnect_still_reports_offline_and_alerts_to_teacher() {
        let hub = Hub::default();
        hub.set_activity_monitoring(true);
        hub.0.lock().unwrap().peers.insert("student-a".into(), 9);
        hub.disconnected("student-a", 9);
        assert!(hub.snapshot()["ready"].as_array().unwrap().is_empty());
        let alerts = hub.activity_alerts();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0]["student"], "student-a");
        assert_eq!(alerts[0]["id"], "student-a-9");
        hub.disconnected("student-a", 9);
        assert!(hub.activity_alerts().is_empty());
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

#[cfg(test)]
mod monitoring_badge_tests {
    use super::*;
    #[test]
    fn explicit_state_survives_disconnect_and_teacher_restart() {
        let store: classroom_store::Shared =
            std::sync::Arc::new(classroom_store::Store::open_in_memory().unwrap());
        let doc = store.doc(classroom_store::ns::BADGES, "enabled");
        let hub = Hub::default();
        hub.load_badges(doc.clone()).unwrap();
        {
            let mut s = hub.0.lock().unwrap();
            s.peers.insert("pc".into(), 7);
            s.capabilities
                .insert("pc".into(), json!({"monitoring_badge":1}));
        }
        hub.set_badges(&["pc".into()], true).unwrap();
        let revision = hub.0.lock().unwrap().badge["pc"].revision.clone();
        hub.badge_ack(
            "pc",
            &ToolPacket {
                id: revision.clone(),
                status: "complete".into(),
                done: 1,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(hub.snapshot()["monitoring_badges"]["pc"]["actual"], true);
        hub.disconnected("pc", 7);
        assert_eq!(hub.snapshot()["monitoring_badges"]["pc"]["enabled"], true);
        let restored = Hub::default();
        restored.load_badges(doc).unwrap();
        assert_eq!(
            restored.snapshot()["monitoring_badges"]["pc"]["enabled"],
            true
        );
        hub.set_badges(&["pc".into()], false).unwrap();
        hub.badge_ack(
            "pc",
            &ToolPacket {
                id: revision,
                status: "complete".into(),
                done: 1,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(hub.snapshot()["monitoring_badges"]["pc"]["enabled"], false);
        assert!(hub.snapshot()["monitoring_badges"]["pc"]["actual"].is_null());
    }
    #[test]
    fn errors_timeouts_and_mismatched_ack_are_visible() {
        let hub = Hub::default();
        hub.set_badges(&["pc".into()], true).unwrap();
        let revision = hub.0.lock().unwrap().badge["pc"].revision.clone();
        assert!(hub
            .badge_ack(
                "pc",
                &ToolPacket {
                    id: revision.clone(),
                    status: "complete".into(),
                    done: 0,
                    ..Default::default()
                }
            )
            .is_err());
        hub.badge_ack(
            "pc",
            &ToolPacket {
                id: revision,
                status: "error".into(),
                text: "No desktop".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            hub.snapshot()["monitoring_badges"]["pc"]["error"],
            "No desktop"
        );
        {
            let mut s = hub.0.lock().unwrap();
            let b = s.badge.get_mut("pc").unwrap();
            b.status = "pending".into();
            b.changed = Instant::now() - Duration::from_secs(11);
        }
        assert_eq!(
            hub.snapshot()["monitoring_badges"]["pc"]["status"],
            "timeout"
        );
    }
    #[test]
    fn student_preference_survives_restart_and_explicit_off() {
        let root = std::env::temp_dir().join(format!("badge-student-{}", nonce().unwrap()));
        fs::create_dir_all(&root).unwrap();
        let profile = root.join("profile.json");
        let mut r = crate::monitoring_badge::Request::default();
        r.load(&profile).unwrap();
        r.enabled = true;
        r.persist().unwrap();
        let mut restored = crate::monitoring_badge::Request::default();
        restored.load(&profile).unwrap();
        assert!(restored.enabled);
        restored.enabled = false;
        restored.persist().unwrap();
        r.load(&profile).unwrap();
        assert!(!r.enabled);
        fs::remove_dir_all(root).unwrap();
    }
}
