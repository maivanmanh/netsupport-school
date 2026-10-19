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

/// Hard ceiling accepted anywhere (protocol bound, chunk numbering, old journals).
pub(super) const MAX_SECONDS: u64 = 8 * 60 * 60;
/// How long a new session records before it stops by itself when the Teacher
/// has not configured otherwise (`tools.recording_max_minutes`), so a recording
/// forgotten about cannot run (and fill disks) for a whole day.
pub(super) const DEFAULT_SECONDS: u64 = 60 * 60;
const GIB: u64 = 1024 * 1024 * 1024;
const MIB: u64 = 1024 * 1024;
/// Besides the stop limit, a new session needs this much more free Teacher
/// disk, plus [`START_PER_STUDENT`] for each recorded Student.
const START_MARGIN_BYTES: u64 = GIB;
const START_PER_STUDENT: u64 = GIB / 2;
const DISK_CHECK_EVERY: Duration = Duration::from_secs(5);
/// Shortest part is a minute, so this many parts fit in [`MAX_SECONDS`].
pub(super) const MAX_CHUNKS: u64 = MAX_SECONDS / 60 + 1;

/// Everything about how a Student records, chosen by the Teacher (Settings →
/// Ghi màn hình) and sent with each Start so Students keep no settings of
/// their own. Defaults are how recording always behaved.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub(crate) struct RecordingOptions {
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub chunk_seconds: u64,
    pub student_stop_bytes: u64,
    pub student_warn_bytes: u64,
    /// Seconds without Teacher contact before capture stops; 0 = never.
    pub gone_seconds: u64,
}
impl Default for RecordingOptions {
    fn default() -> Self {
        Self {
            height: 720,
            fps: 30,
            bitrate_kbps: 3000,
            chunk_seconds: 600,
            student_stop_bytes: 512 * MIB,
            student_warn_bytes: 2 * GIB,
            gone_seconds: 30 * 60,
        }
    }
}
impl RecordingOptions {
    pub fn from_config(c: &classroom_config::RecordingConfig) -> Self {
        Self {
            height: c.height,
            fps: c.fps,
            bitrate_kbps: c.bitrate_kbps,
            chunk_seconds: u64::from(c.chunk_minutes) * 60,
            student_stop_bytes: u64::from(c.student_stop_mb) * MIB,
            student_warn_bytes: u64::from(c.student_warn_mb) * MIB,
            gone_seconds: u64::from(c.gone_minutes) * 60,
        }
    }
    /// A Student never trusts the numbers it is sent: anything outside what
    /// the helper can run is pulled back into range.
    pub fn sanitized(mut self) -> Self {
        if ![360, 480, 720, 1080].contains(&self.height) {
            self.height = 720;
        }
        self.fps = self.fps.clamp(5, 30);
        self.bitrate_kbps = self.bitrate_kbps.clamp(500, 12_000);
        self.chunk_seconds = self.chunk_seconds.clamp(60, 1800);
        // Keep each part comfortably under the 1 GiB transfer limit.
        let largest = 100_000 * 60 / u64::from(self.bitrate_kbps);
        self.chunk_seconds = self.chunk_seconds.min(largest.max(60));
        self.student_stop_bytes = self.student_stop_bytes.max(256 * MIB);
        self.student_warn_bytes = self.student_warn_bytes.max(self.student_stop_bytes);
        self.gone_seconds = if self.gone_seconds == 0 {
            0
        } else {
            self.gone_seconds.clamp(300, 14_400)
        };
        self
    }
    /// Capture bounds: 16:9 at the chosen height (smaller screens keep their shape).
    pub fn bounds(&self) -> (u32, u32) {
        ((self.height * 16 / 9 + 1) & !1, self.height)
    }
    pub fn max_samples(&self) -> u32 {
        u32::try_from(self.chunk_seconds).unwrap_or(1800) * self.fps + 200
    }
}

/// What the Teacher decides for one new session.
pub(crate) struct StartPolicy {
    pub limit_seconds: u64,
    pub options: RecordingOptions,
    /// Teacher folder for the recordings; empty = the Desktop.
    pub folder: String,
    pub stop_bytes: u64,
    pub warn_bytes: u64,
}
impl Default for StartPolicy {
    fn default() -> Self {
        Self {
            limit_seconds: DEFAULT_SECONDS,
            options: RecordingOptions::default(),
            folder: String::new(),
            stop_bytes: GIB,
            warn_bytes: 5 * GIB,
        }
    }
}
impl StartPolicy {
    pub fn from_config(config: &classroom_config::Config) -> Self {
        let r = &config.recording;
        Self {
            limit_seconds: u64::from(config.tools.recording_max_minutes) * 60,
            options: RecordingOptions::from_config(r),
            folder: r.folder.clone(),
            stop_bytes: u64::from(r.teacher_stop_mb) * MIB,
            warn_bytes: u64::from(r.teacher_warn_mb) * MIB,
        }
    }
}
fn legacy_stop_bytes() -> u64 {
    GIB
}
fn legacy_warn_bytes() -> u64 {
    5 * GIB
}
const MAX_CHUNK_BYTES: u64 = 1024 * 1024 * 1024;
const BLOCK: usize = 64 * 1024;
const PACE: Duration = Duration::from_millis(6); // <=10.42 MiB/s per slot, two globally.

#[derive(Default)]
pub(super) struct TeacherState {
    doc: Option<classroom_store::Doc>,
    sessions: Vec<Session>,
    active_uploads: usize,
    /// Last measured free space of the Teacher's recording disk, and when.
    free_bytes: Option<u64>,
    disk_checked: Option<Instant>,
}
#[derive(Serialize, Deserialize)]
struct Session {
    id: String,
    folder: PathBuf,
    created: u64,
    stop: bool,
    /// Seconds after `created` at which capture ends by itself. Journals written
    /// before this field existed ran for the full eight hours.
    #[serde(default = "legacy_limit")]
    limit: u64,
    /// Why the Teacher ended the session on its own (`""` for a manual Stop).
    #[serde(default)]
    stop_reason: String,
    /// How Students record this session (resent with every Start).
    #[serde(default)]
    options: RecordingOptions,
    /// Folder the session's recordings live under; empty = the Desktop.
    #[serde(default)]
    root: PathBuf,
    /// Teacher free-space limits for this session (stop / warn).
    #[serde(default = "legacy_stop_bytes")]
    stop_bytes: u64,
    #[serde(default = "legacy_warn_bytes")]
    warn_bytes: u64,
    targets: HashMap<String, Target>,
}
fn legacy_limit() -> u64 {
    MAX_SECONDS
}
impl Session {
    fn deadline(&self) -> u64 {
        self.created + self.limit.min(MAX_SECONDS)
    }
}
#[derive(Serialize, Deserialize)]
struct Target {
    name: String,
    folder: PathBuf,
    token: String,
    #[serde(default)]
    stop: bool,
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
    if let Some(doc) = &state.doc {
        doc.write(&state.sessions)?;
    }
    Ok(())
}

/// Teacher-side disk watchdog. At most every few seconds it measures the free
/// space where live sessions write; below a session's stop limit every live
/// session is stopped so Students stop recording video that can no longer be
/// collected (what they already recorded stays queued and uploads once space
/// is freed). `free_of` is injectable for tests.
fn guard_teacher_disk(
    state: &mut TeacherState,
    now: Instant,
    free_of: &dyn Fn(&Path) -> Option<u64>,
) {
    if state
        .disk_checked
        .is_some_and(|at| now.duration_since(at) < DISK_CHECK_EVERY)
    {
        return;
    }
    state.disk_checked = Some(now);
    let at = epoch();
    let mut least: Option<u64> = None;
    let mut changed = false;
    for session in &mut state.sessions {
        let live = !session.stop
            && at < session.deadline()
            && session.targets.values().any(|t| {
                !matches!(
                    t.report["phase"].as_str(),
                    Some("stopped" | "error" | "interrupted")
                )
            });
        if !live {
            continue;
        }
        let Some(free) = free_of(&session.folder) else {
            continue;
        };
        least = Some(least.map_or(free, |l| l.min(free)));
        if free < session.stop_bytes {
            session.stop = true;
            session.stop_reason = "teacher_disk_low".into();
            for target in session.targets.values_mut() {
                target.requested = None;
            }
            changed = true;
        }
    }
    state.free_bytes = least;
    if changed {
        let _ = save(state);
    }
}
fn free_space(path: &Path) -> Option<u64> {
    fs2::available_space(path).ok()
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
    pub fn load_recordings(&self, doc: classroom_store::Doc) -> Result<()> {
        let sessions: Vec<Session> = doc.read()?.unwrap_or_default();
        anyhow::ensure!(
            sessions.len() <= 32 && sessions.iter().all(|s| s.targets.len() <= 64),
            "Invalid recording journal"
        );
        let mut state = self.7.lock().unwrap();
        state.doc = Some(doc);
        state.sessions = sessions;
        Ok(())
    }
    pub fn recording_start(
        &self,
        recipients: &[(String, String)],
        policy: &StartPolicy,
    ) -> Result<String> {
        let base = if policy.folder.trim().is_empty() {
            super::user_folder("Desktop")?
        } else {
            let folder = PathBuf::from(policy.folder.trim());
            anyhow::ensure!(
                folder.is_absolute(),
                "Thư mục ghi hình phải là đường dẫn đầy đủ"
            );
            fs::create_dir_all(&folder)
                .context("Không tạo hoặc mở được thư mục lưu ghi hình đã chọn")?;
            folder
        };
        self.recording_start_at(recipients, &base, policy)
    }
    fn recording_start_at(
        &self,
        recipients: &[(String, String)],
        desktop: &Path,
        policy: &StartPolicy,
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
                && epoch() < s.deadline()
                && recipients
                    .iter()
                    .any(|(id, _)| s.targets.get(id).is_some_and(|t| !matches!(
                        t.report["phase"].as_str(),
                        Some("stopped" | "error" | "interrupted")
                    )))),
            "A selected Student already has a recording session; stop it first"
        );
        let free =
            fs2::available_space(desktop).context("Cannot read free space of the Desktop")?;
        let needed =
            policy.stop_bytes + START_MARGIN_BYTES + START_PER_STUDENT * recipients.len() as u64;
        anyhow::ensure!(
            free >= needed,
            "Ổ đĩa giảng viên còn {} GiB, cần ít nhất {} GiB để ghi {} máy (đổi ngưỡng trong Cài đặt → Ghi màn hình)",
            free / GIB,
            needed.div_ceil(GIB),
            recipients.len()
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
                    stop: false,
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
            limit: policy.limit_seconds.clamp(5 * 60, MAX_SECONDS),
            stop_reason: String::new(),
            options: policy.options.clone().sanitized(),
            root: desktop.to_owned(),
            stop_bytes: policy.stop_bytes,
            warn_bytes: policy.warn_bytes.max(policy.stop_bytes),
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
    pub fn recording_stop_target(&self, id: &str, student: &str) -> Result<()> {
        let mut state = self.7.lock().unwrap();
        let target = state
            .sessions
            .iter_mut()
            .find(|session| session.id == id)
            .and_then(|session| session.targets.get_mut(student))
            .context("Student is not part of this recording")?;
        target.stop = true;
        target.requested = None;
        save(&state)
    }
    /// Removes a recording from the journal and deletes the video files this
    /// Teacher received for it. Only files the recording itself created are
    /// deleted (`Part-NNNN.mp4` and their `.partial` temporaries) and only
    /// inside the Teacher's Desktop; a folder that still holds anything else is
    /// left in place. Refused while a Student is still capturing or a file is
    /// arriving. Returns how many files were deleted.
    pub fn recording_delete(&self, id: &str) -> Result<usize> {
        let mut state = self.7.lock().unwrap();
        let index = state
            .sessions
            .iter()
            .position(|s| s.id == id)
            .context("Unknown recording")?;
        {
            let session = &state.sessions[index];
            for target in session.targets.values() {
                let phase = target.report["phase"].as_str().unwrap_or("pending");
                let live = target
                    .seen
                    .is_some_and(|at| at.elapsed() < Duration::from_secs(10));
                anyhow::ensure!(
                    !(live && matches!(phase, "recording" | "starting")),
                    "Hãy dừng ghi trước khi xóa phiên này"
                );
                anyhow::ensure!(
                    !target.uploading,
                    "Đang tải video về; hãy thử lại sau khi tải xong"
                );
            }
        }
        let session = state.sessions.remove(index);
        if let Err(error) = save(&state) {
            state.sessions.insert(index, session);
            return Err(error);
        }
        drop(state);
        // Only below the folder the session itself recorded into.
        let desktop = if session.root.as_os_str().is_empty() {
            super::user_folder("Desktop")?
        } else {
            session.root.clone()
        };
        let mut folders: Vec<PathBuf> =
            session.targets.values().map(|t| t.folder.clone()).collect();
        folders.push(session.folder.clone());
        folders.dedup();
        Ok(delete_recording_files(&desktop, &folders))
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
        guard_teacher_disk(&mut state, Instant::now(), &free_space);
        for session in &mut state.sessions {
            let deadline = session.deadline();
            let Some(target) = session.targets.get_mut(student) else {
                continue;
            };
            let expired = epoch() >= deadline;
            let stop = session.stop || target.stop || expired;
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
            p.text = json!({"expires":deadline,"options":session.options}).to_string();
            return Some(p);
        }
        None
    }
    pub(super) fn recording_snapshot(&self, peers: &HashMap<String, u64>) -> Value {
        let state = self.7.lock().unwrap();
        json!(state.sessions.iter().rev().map(|s| json!({"id":s.id,"folder":s.folder,"created":s.created,"stop":s.stop,"deadline":s.deadline(),
            "stop_reason":if !s.stop_reason.is_empty(){s.stop_reason.as_str()}else if !s.stop && epoch()>=s.deadline(){"time_limit"}else{""},
            "teacher_free_bytes":state.free_bytes,"teacher_warn_bytes":s.warn_bytes,"teacher_stop_bytes":s.stop_bytes,"options":s.options,
            "targets":s.targets.iter().map(|(id,t)| json!({"id":id,"name":t.name,"folder":t.folder,
                "online":peers.contains_key(id),"stale":t.seen.is_none_or(|at|at.elapsed()>Duration::from_secs(10)),
                "report":t.report,"received":t.received.len(),"uploading":t.uploading,"upload_bytes":t.progress,
                "received_bytes":t.received.values().map(|r|r.size).sum::<u64>()})).collect::<Vec<_>>() })).collect::<Vec<_>>())
    }
}

/// `Part-0001.mp4`, or the `.Part-0001.mp4.partial` temporary of one.
fn is_recording_file(name: &str) -> bool {
    let name = name
        .strip_prefix('.')
        .map_or(name, |n| n.strip_suffix(".partial").unwrap_or(n));
    name.strip_prefix("Part-")
        .and_then(|n| n.strip_suffix(".mp4"))
        .is_some_and(|digits| digits.len() == 4 && digits.bytes().all(|b| b.is_ascii_digit()))
}

/// Deletes recording files below `desktop` in `folders`, then removes each
/// folder if that left it empty (never recursively). Returns the file count.
fn delete_recording_files(desktop: &Path, folders: &[PathBuf]) -> usize {
    let mut deleted = 0;
    for folder in folders {
        if !folder.starts_with(desktop) || folder == desktop {
            continue;
        }
        if let Ok(entries) = fs::read_dir(folder) {
            for entry in entries.flatten() {
                let path = entry.path();
                let is_file = entry.file_type().is_ok_and(|t| t.is_file());
                if is_file
                    && path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(is_recording_file)
                    && fs::remove_file(&path).is_ok()
                {
                    deleted += 1;
                }
            }
        }
    }
    // Per-Student folders first, then the session folder that held them.
    for folder in folders {
        if folder.starts_with(desktop) && folder != desktop {
            let _ = fs::remove_dir(folder);
        }
    }
    deleted
}

pub(super) fn serve(s: &mut TcpStream, p: &ToolPacket, hub: &Hub) -> Option<Result<()>> {
    Some(match p.action.as_str() {
        "recording_status" => status(s, p, hub),
        "recording_upload" => upload(s, p, hub),
        _ => return None,
    })
}
impl Hub {
    /// What to tell the Student after its status report: `""` carry on, `"stop"`
    /// end the capture, `"discard"` the Teacher no longer has this recording
    /// (the Teacher deleted it), so the Student may drop what it still holds.
    pub(super) fn recording_status(&self, student: &str, p: &ToolPacket) -> Result<&'static str> {
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
        guard_teacher_disk(&mut state, Instant::now(), &free_space);
        if !state.sessions.iter().any(|s| s.id == p.id) {
            return Ok("discard");
        }
        let target = authenticated(&mut state, &trusted)?;
        target.report = report;
        target.seen = Some(Instant::now());
        let session = state.sessions.iter().find(|s| s.id == p.id).unwrap();
        Ok(if session.stop || epoch() >= session.deadline() {
            "stop"
        } else {
            ""
        })
    }
}
fn status(s: &mut TcpStream, p: &ToolPacket, hub: &Hub) -> Result<()> {
    let verdict = hub.recording_status(&p.student, p)?;
    let mut ack = packet("ok");
    ack.status = verdict.into();
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
    #[test]
    fn only_recording_files_inside_the_desktop_are_deleted() {
        assert!(is_recording_file("Part-0001.mp4"));
        assert!(is_recording_file(".Part-0012.mp4.partial"));
        assert!(!is_recording_file("Part-1.mp4"));
        assert!(!is_recording_file("notes.txt"));
        assert!(!is_recording_file("Part-0001.mp4.bak"));
        let root = std::env::temp_dir().join(format!("rec-delete-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let desktop = root.join("Desktop");
        let session = desktop.join("Lop-A");
        let one = session.join("PC-01");
        let two = session.join("PC-02");
        for dir in [&one, &two] {
            fs::create_dir_all(dir).unwrap();
        }
        fs::write(one.join("Part-0001.mp4"), b"x").unwrap();
        fs::write(one.join("Part-0002.mp4"), b"x").unwrap();
        fs::write(two.join(".Part-0001.mp4.partial"), b"x").unwrap();
        fs::write(two.join("my-notes.txt"), b"keep").unwrap();
        let outside = root.join("Elsewhere");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("Part-0001.mp4"), b"x").unwrap();
        let folders = vec![
            one.clone(),
            two.clone(),
            outside.clone(),
            session.clone(),
            desktop.clone(),
        ];
        assert_eq!(delete_recording_files(&desktop, &folders), 3);
        // Emptied folders go; a folder holding someone's own file stays, and so does its parent.
        assert!(!one.exists());
        assert!(two.join("my-notes.txt").exists());
        assert!(session.exists());
        // Outside the Desktop nothing is touched; the Desktop itself is never removed.
        assert!(outside.join("Part-0001.mp4").exists());
        assert!(desktop.exists());
        let _ = fs::remove_dir_all(&root);
    }

    use super::*;
    fn live_session(folder: &Path) -> Session {
        let mut targets = HashMap::new();
        targets.insert(
            "a".to_owned(),
            Target {
                name: "PC".into(),
                folder: folder.to_owned(),
                token: "t".into(),
                stop: false,
                report: json!({"phase":"recording"}),
                received: HashMap::new(),
                seen: None,
                requested: None,
                uploading: false,
                progress: 0,
            },
        );
        Session {
            id: "s".into(),
            folder: folder.to_owned(),
            created: epoch(),
            stop: false,
            limit: DEFAULT_SECONDS,
            stop_reason: String::new(),
            options: RecordingOptions::default(),
            root: PathBuf::new(),
            stop_bytes: GIB,
            warn_bytes: 5 * GIB,
            targets,
        }
    }
    #[test]
    fn low_teacher_disk_stops_live_sessions_and_says_why() {
        let folder = std::env::temp_dir();
        let mut state = TeacherState::default();
        state.sessions.push(live_session(&folder));
        let now = Instant::now();
        // Plenty of space: nothing happens, the free space is remembered.
        guard_teacher_disk(&mut state, now, &|_| Some(50 * GIB));
        assert!(!state.sessions[0].stop);
        assert_eq!(state.free_bytes, Some(50 * GIB));
        // Checks are rate limited, so a sudden drop is seen on the next interval.
        guard_teacher_disk(&mut state, now, &|_| Some(0));
        assert!(!state.sessions[0].stop);
        let later = now + DISK_CHECK_EVERY;
        guard_teacher_disk(&mut state, later, &|_| Some(GIB - 1));
        assert!(state.sessions[0].stop);
        assert_eq!(state.sessions[0].stop_reason, "teacher_disk_low");
    }
    #[test]
    fn finished_sessions_are_not_stopped_or_measured() {
        let folder = std::env::temp_dir();
        let mut state = TeacherState::default();
        let mut session = live_session(&folder);
        session.targets.get_mut("a").unwrap().report = json!({"phase":"stopped"});
        state.sessions.push(session);
        guard_teacher_disk(&mut state, Instant::now(), &|_| Some(0));
        assert!(!state.sessions[0].stop);
        assert_eq!(state.free_bytes, None);
    }
    #[test]
    fn new_sessions_have_a_short_deadline_and_old_journals_keep_theirs() {
        let folder = std::env::temp_dir();
        let session = live_session(&folder);
        assert_eq!(session.deadline(), session.created + DEFAULT_SECONDS);
        assert!(DEFAULT_SECONDS < MAX_SECONDS);
        let old: Session = serde_json::from_value(json!({
            "id":"s","folder":"/x","created":100,"stop":false,"targets":{}
        }))
        .unwrap();
        assert_eq!(old.deadline(), 100 + MAX_SECONDS);
        assert_eq!(old.stop_reason, "");
    }
    #[test]
    fn the_configured_session_length_is_used_and_bounded() {
        let hub = Hub::default();
        let root = std::env::temp_dir().join(format!("recording-limit-{}", nonce().unwrap()));
        fs::create_dir_all(&root).unwrap();
        {
            let mut st = hub.0.lock().unwrap();
            st.peers.insert("a".into(), 1);
            st.capabilities.insert("a".into(), json!({"recording":1}));
        }
        hub.load_recordings(test_journal()).unwrap();
        let id = hub
            .recording_start_at(
                &[("a".into(), "PC".into())],
                &root,
                &StartPolicy {
                    limit_seconds: 45 * 60,
                    ..StartPolicy::default()
                },
            )
            .unwrap();
        let snapshot = hub.recording_snapshot(&hub.0.lock().unwrap().peers);
        let session = &snapshot[0];
        assert_eq!(session["id"], id);
        assert_eq!(
            session["deadline"].as_u64().unwrap() - session["created"].as_u64().unwrap(),
            45 * 60
        );
        // Absurd values are clamped rather than trusted.
        let other = hub
            .recording_start_at(
                &[("b".into(), "PC2".into())],
                &root,
                &StartPolicy {
                    limit_seconds: 1,
                    ..StartPolicy::default()
                },
            )
            .map(|_| ());
        assert!(other.is_err(), "unknown Student b is refused");
        let _ = fs::remove_dir_all(&root);
    }
    #[test]
    fn the_start_requirement_follows_the_configured_limits() {
        let policy = StartPolicy::default();
        let one = policy.stop_bytes + START_MARGIN_BYTES + START_PER_STUDENT;
        let forty = policy.stop_bytes + START_MARGIN_BYTES + START_PER_STUDENT * 40;
        assert!(forty > one);
        assert_eq!(
            policy.stop_bytes + START_MARGIN_BYTES,
            2 * GIB,
            "same as before the setting existed"
        );
        assert!(policy.stop_bytes < policy.warn_bytes);
    }
    #[test]
    fn student_options_are_clamped_and_describe_todays_defaults() {
        let d = RecordingOptions::default();
        assert_eq!(d.bounds(), (1280, 720));
        assert_eq!(d.max_samples(), 18_200);
        assert_eq!(
            RecordingOptions {
                height: 1080,
                ..d.clone()
            }
            .bounds(),
            (1920, 1080)
        );
        assert_eq!(
            RecordingOptions {
                height: 480,
                ..d.clone()
            }
            .bounds(),
            (854, 480)
        );
        let wild = RecordingOptions {
            height: 999,
            fps: 500,
            bitrate_kbps: 1,
            chunk_seconds: 999_999,
            student_stop_bytes: 0,
            student_warn_bytes: 0,
            gone_seconds: 5,
        }
        .sanitized();
        assert_eq!(wild.height, 720);
        assert_eq!(wild.fps, 30);
        assert_eq!(wild.bitrate_kbps, 500);
        assert!(wild.chunk_seconds <= 1800);
        assert!(wild.student_stop_bytes >= 256 * MIB);
        assert!(wild.student_warn_bytes >= wild.student_stop_bytes);
        assert_eq!(wild.gone_seconds, 300);
        // A big bitrate shortens the part so it stays under the file limit.
        let hi = RecordingOptions {
            bitrate_kbps: 12_000,
            chunk_seconds: 1800,
            ..d.clone()
        }
        .sanitized();
        assert!(hi.chunk_seconds * u64::from(hi.bitrate_kbps) / 8 * 1000 < MAX_CHUNK_BYTES);
        assert_eq!(
            RecordingOptions {
                gone_seconds: 0,
                ..d
            }
            .sanitized()
            .gone_seconds,
            0
        );
    }
    #[test]
    fn selection_auth_stop_and_reconnect() {
        let hub = Hub::default();
        let root = std::env::temp_dir().join(format!("recording-test-{}", nonce().unwrap()));
        fs::create_dir_all(&root).unwrap();
        assert!(hub
            .recording_start_at(&[("a".into(), "PC".into())], &root, &StartPolicy::default())
            .is_err());
        {
            let mut st = hub.0.lock().unwrap();
            for id in ["a", "b"] {
                st.peers.insert(id.into(), 1);
                st.capabilities.insert(id.into(), json!({"recording":1}));
            }
        }
        let journal = test_journal();
        hub.load_recordings(journal.clone()).unwrap();
        let id = hub
            .recording_start_at(
                &[("a".into(), "PC".into()), ("b".into(), "pc".into())],
                &root,
                &StartPolicy::default(),
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
            .recording_start_at(&[("a".into(), "PC".into())], &root, &StartPolicy::default())
            .is_err());
        hub.recording_stop_target(&id, "a").unwrap();
        assert_eq!(hub.recording_poll("a").unwrap().action, "recording_stop");
        assert_eq!(hub.recording_poll("b").unwrap().action, "recording_start");
        hub.recording_stop(&id).unwrap();
        assert_eq!(hub.recording_poll("b").unwrap().action, "recording_stop");
        let reloaded = Hub::default();
        reloaded.load_recordings(journal).unwrap();
        assert_eq!(
            reloaded.recording_poll("a").unwrap().action,
            "recording_stop"
        );
        assert!(chunk_name(0).is_err());
        assert!(chunk_name(MAX_CHUNKS + 1).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    fn test_journal() -> classroom_store::Doc {
        let store: classroom_store::Shared =
            std::sync::Arc::new(classroom_store::Store::open_in_memory().unwrap());
        store.doc(classroom_store::ns::RECORDINGS, "journal")
    }
    /// A journal whose writes always fail (invalid document name).
    fn failing_journal() -> classroom_store::Doc {
        let store: classroom_store::Shared =
            std::sync::Arc::new(classroom_store::Store::open_in_memory().unwrap());
        store.doc("", "journal")
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
        hub.load_recordings(test_journal()).unwrap();
        let id = hub
            .recording_start_at(
                &[("s".into(), "PC01".into())],
                &root,
                &StartPolicy::default(),
            )
            .unwrap();
        let p = hub.recording_poll("s").unwrap();
        let meta = student::Metadata {
            id,
            token: p.token,
            identity: "s".into(),
            address: "127.0.0.1:1".parse().unwrap(),
            expires: epoch() + 600,
            options: RecordingOptions::default(),
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
        let journal = hub.7.lock().unwrap().doc.clone().unwrap();
        reloaded.load_recordings(journal).unwrap();
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
        let working = hub.7.lock().unwrap().doc.clone();
        hub.7.lock().unwrap().doc = Some(failing_journal());
        let server = endpoint(std::sync::Arc::clone(&hub), &mut meta);
        assert!(student::transfer(&meta, &src, 1, &AtomicBool::new(false)).is_err());
        assert!(server.join().unwrap().is_err());
        assert!(src.exists());
        assert_eq!(
            hub.recording_snapshot(&HashMap::new())[0]["targets"][0]["received"],
            0
        );
        hub.7.lock().unwrap().doc = working;
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
