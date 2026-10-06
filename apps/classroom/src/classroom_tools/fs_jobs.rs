//! File collection and file-manager jobs: Teacher-side bookkeeping/endpoints and
//! the Student-side workers. Students always connect *to* the Teacher (like
//! file offers and diagnostics), so no inbound Student port is ever needed.
use super::{
    configure,
    file_tree::{self, Filter, Progress, Sink},
    nonce, packet, read, safe_name, send, terminal, Hub, Slot,
};
use anyhow::{Context, Result};
use protocol::ToolPacket;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    io::{Read, Write},
    net::{SocketAddrV4, TcpStream},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

const MAX_JOBS: usize = 24;
const MAX_LISTS: usize = 32;
const MAX_STREAMS: usize = 6;
const MAX_BODY: u64 = 4 * 1024 * 1024;
const QUEUED_TIMEOUT: Duration = Duration::from_secs(60);
const LIST_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_PATHS: usize = 32;
const MAX_REQUEST_TEXT: usize = 6000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Bulk: Students send filtered files into a new Teacher folder.
    Collect,
    /// File manager: Student paths → a Teacher folder.
    Fetch,
    /// File manager: Teacher paths → a Student folder.
    Push,
}
impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Collect => "collect",
            Self::Fetch => "fetch",
            Self::Push => "push",
        }
    }
}
struct FsTarget {
    name: String,
    folder: PathBuf,
    status: String,
    progress: Progress,
    error: String,
    changed: Instant,
}
struct FsJob {
    id: String,
    token: String,
    kind: Kind,
    cancel: Arc<AtomicBool>,
    flat: bool,
    dest: PathBuf,
    local: Vec<PathBuf>,
    targets: HashMap<String, FsTarget>,
}
struct ListSlot {
    student: String,
    token: String,
    body: Option<Value>,
    at: Instant,
    /// Whether a landed body should also be merged into the Teacher's
    /// per-student activity cache (`Hub::activity_ingest`), not just handed
    /// to the one caller polling this slot.
    is_activity: bool,
}
#[derive(Default)]
pub(super) struct FsState {
    jobs: VecDeque<FsJob>,
    lists: HashMap<String, ListSlot>,
    active: usize,
}
struct Stream<'a>(&'a Hub);
impl Drop for Stream<'_> {
    fn drop(&mut self) {
        let mut st = self.0 .3.lock().unwrap();
        st.active = st.active.saturating_sub(1);
    }
}

fn exts_json(filter: &Filter) -> Value {
    json!({"exts":filter.exts,"recursive":filter.recursive,"hidden":filter.hidden,
        "max_file_mb":filter.max_file_mb,"max_total_mb":filter.max_total_mb})
}
fn bounded(text: String) -> Result<String> {
    anyhow::ensure!(
        text.len() <= MAX_REQUEST_TEXT,
        "Too many or too long paths in one request"
    );
    Ok(text)
}

impl Hub {
    fn fs_student_ready(&self, ids: &[String]) -> Result<()> {
        let st = self.0.lock().unwrap();
        for id in ids {
            anyhow::ensure!(
                st.peers.contains_key(id),
                "Student tools unavailable; the Student may be offline"
            );
            anyhow::ensure!(
                st.capabilities.get(id).is_some_and(|c| c["fs"] == 1),
                "A Student runs an older build without file collection; update it via Classroom first"
            );
            anyhow::ensure!(
                st.queues.get(id).is_none_or(|q| q.len() < 16),
                "Student has too many pending tools"
            );
        }
        Ok(())
    }
    pub(super) fn fs_enqueue(&self, student: &str, request: ToolPacket) {
        self.0
            .lock()
            .unwrap()
            .queues
            .entry(student.to_owned())
            .or_default()
            .push_back(request);
    }
    fn fs_add_job(&self, job: FsJob) -> Result<()> {
        let mut st = self.3.lock().unwrap();
        if st.jobs.len() >= MAX_JOBS {
            let i = st
                .jobs
                .iter()
                .position(|j| j.targets.values().all(|t| terminal(&t.status)))
                .context("Too many active file jobs")?;
            st.jobs.remove(i);
        }
        st.jobs.push_back(job);
        Ok(())
    }

    /// Bulk collection from `recipients` (`(id, display name)`) into one new
    /// Teacher folder `root`, one sub-folder per Student.
    pub fn fs_collect(&self, recipients: &[(String, String)], input: &Value, root: &Path) -> Result<String> {
        anyhow::ensure!(
            !recipients.is_empty() && recipients.len() <= 64,
            "Select online Students"
        );
        let ids: Vec<String> = recipients.iter().map(|(id, _)| id.clone()).collect();
        self.fs_student_ready(&ids)?;
        let path = input["path"].as_str().unwrap_or("~desktop").trim();
        anyhow::ensure!(path.chars().count() <= 1000, "Path too long");
        let filter = Filter {
            exts: file_tree::parse_exts(input["exts"].as_str().unwrap_or("")),
            recursive: input["recursive"].as_bool().unwrap_or(true),
            hidden: false,
            max_file_mb: input["max_file_mb"].as_u64().unwrap_or(200),
            max_total_mb: input["max_total_mb"].as_u64().unwrap_or(4096),
        };
        let text = bounded(
            json!({"paths":[path],"keep_root":false,"fast":false,"filter":exts_json(&filter)})
                .to_string(),
        )?;
        std::fs::create_dir_all(root)
            .with_context(|| format!("Cannot create collection folder {}", root.display()))?;
        let mut used = std::collections::HashSet::new();
        let id = nonce()?;
        let token = nonce()?;
        let targets = recipients
            .iter()
            .map(|(student, name)| {
                let mut folder = safe_name(name);
                if !used.insert(folder.to_ascii_lowercase()) {
                    folder = format!("{folder}-{}", &student[..student.len().min(4)]);
                    used.insert(folder.to_ascii_lowercase());
                }
                (student.clone(), FsTarget {
                    name: name.clone(),
                    folder: root.join(folder),
                    status: "queued".into(),
                    progress: Progress::default(),
                    error: String::new(),
                    changed: Instant::now(),
                })
            })
            .collect();
        self.fs_add_job(FsJob {
            id: id.clone(),
            token: token.clone(),
            kind: Kind::Collect,
            cancel: Arc::new(AtomicBool::new(false)),
            flat: input["flat"].as_bool().unwrap_or(false),
            dest: root.to_path_buf(),
            local: Vec::new(),
            targets,
        })?;
        for student in ids {
            let mut request = packet("fs_send");
            request.id.clone_from(&id);
            request.token.clone_from(&token);
            request.text.clone_from(&text);
            self.fs_enqueue(&student, request);
        }
        tracing::info!(job_id=%id,students=recipients.len(),"Classroom file collection queued");
        Ok(id)
    }

    /// File manager transfer with one Student. `push`: `paths` are Teacher
    /// paths copied into the Student folder `dest`; otherwise `paths` are
    /// Student paths copied into the Teacher folder `dest`.
    pub fn fs_transfer(
        &self,
        push: bool,
        student: (&str, &str),
        paths: &[String],
        dest: &str,
    ) -> Result<String> {
        anyhow::ensure!(
            !paths.is_empty() && paths.len() <= MAX_PATHS,
            "Select 1–{MAX_PATHS} items"
        );
        anyhow::ensure!(!dest.trim().is_empty(), "Choose a destination folder");
        self.fs_student_ready(&[student.0.to_owned()])?;
        let id = nonce()?;
        let token = nonce()?;
        let everything = Filter {
            exts: Vec::new(),
            recursive: true,
            hidden: true,
            max_file_mb: 4096,
            max_total_mb: 32 * 1024,
        };
        let (kind, request, local, folder) = if push {
            let local: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
            anyhow::ensure!(
                local.iter().all(|p| p.is_absolute() && p.is_file()),
                "Only existing files can be sent here, not folders"
            );
            let mut request = packet("fs_receive");
            request.text = bounded(json!({"dest":dest}).to_string())?;
            (Kind::Push, request, local, PathBuf::from(dest))
        } else {
            let folder = PathBuf::from(dest);
            anyhow::ensure!(folder.is_absolute(), "Destination must be an absolute path");
            let mut request = packet("fs_send");
            request.text = bounded(
                json!({"paths":paths,"keep_root":true,"fast":true,"files_only":true,"filter":exts_json(&everything)})
                    .to_string(),
            )?;
            (Kind::Fetch, request, Vec::new(), folder.clone())
        };
        let mut request = request;
        request.id.clone_from(&id);
        request.token.clone_from(&token);
        self.fs_add_job(FsJob {
            id: id.clone(),
            token,
            kind,
            cancel: Arc::new(AtomicBool::new(false)),
            flat: false,
            dest: folder.clone(),
            local,
            targets: HashMap::from([(
                student.0.to_owned(),
                FsTarget {
                    name: student.1.to_owned(),
                    folder,
                    status: "queued".into(),
                    progress: Progress::default(),
                    error: String::new(),
                    changed: Instant::now(),
                },
            )]),
        })?;
        self.fs_enqueue(student.0, request);
        tracing::info!(job_id=%id,kind=kind.name(),"Classroom file transfer queued");
        Ok(id)
    }

    pub fn fs_cancel(&self, id: &str) -> Result<()> {
        let students: Vec<String> = {
            let mut st = self.3.lock().unwrap();
            let job = st
                .jobs
                .iter_mut()
                .find(|j| j.id == id)
                .context("File job not found")?;
            job.cancel.store(true, Ordering::Release);
            for target in job.targets.values_mut().filter(|t| !terminal(&t.status)) {
                target.status = "cancelled".into();
                target.changed = Instant::now();
            }
            job.targets.keys().cloned().collect()
        };
        let mut st = self.0.lock().unwrap();
        for student in students {
            let queue = st.queues.entry(student).or_default();
            queue.retain(|p| p.id != id);
            if queue.len() < 16 {
                let mut cancel = packet("cancel");
                cancel.id = id.into();
                queue.push_back(cancel);
            }
        }
        Ok(())
    }

    /// Asks a Student for a directory listing; poll [`Hub::fs_list_result`].
    pub fn fs_list_remote(&self, student: &str, path: &str, hidden: bool) -> Result<String> {
        self.fs_student_ready(&[student.to_owned()])?;
        let id = nonce()?;
        let token = nonce()?;
        self.fs_register_list(&id, student, &token, None);
        let mut request = packet("fs_list");
        request.id.clone_from(&id);
        request.token = token;
        request.text = bounded(json!({"path":path,"hidden":hidden}).to_string())?;
        self.fs_enqueue(student, request);
        Ok(id)
    }
    /// Lists a Teacher folder off the caller's thread.
    pub fn fs_list_local(self: &Arc<Self>, path: &str, hidden: bool) -> Result<String> {
        let id = nonce()?;
        self.fs_register_list(&id, "", "", None);
        let hub = Arc::clone(self);
        let path = path.to_owned();
        let key = id.clone();
        thread::spawn(move || {
            let body = file_tree::list(&path, hidden);
            if let Some(slot) = hub.3.lock().unwrap().lists.get_mut(&key) {
                slot.body = Some(body);
            }
        });
        Ok(id)
    }
    pub(super) fn fs_register_list(&self, id: &str, student: &str, token: &str, body: Option<Value>) {
        self.fs_register_slot(id, student, token, body, false);
    }
    pub(super) fn fs_register_slot(&self, id: &str, student: &str, token: &str, body: Option<Value>, is_activity: bool) {
        let mut st = self.3.lock().unwrap();
        if st.lists.len() >= MAX_LISTS {
            if let Some(oldest) = st
                .lists
                .iter()
                .min_by_key(|(_, slot)| slot.at)
                .map(|(key, _)| key.clone())
            {
                st.lists.remove(&oldest);
            }
        }
        st.lists.insert(
            id.to_owned(),
            ListSlot {
                student: student.to_owned(),
                token: token.to_owned(),
                body,
                at: Instant::now(),
                is_activity,
            },
        );
    }
    /// `{pending:true}` until the listing arrives; consumed on first read.
    pub fn fs_list_result(&self, id: &str) -> Value {
        let mut st = self.3.lock().unwrap();
        let Some(slot) = st.lists.get(id) else {
            return json!({"error":"Unknown or expired listing"});
        };
        if slot.body.is_some() {
            return st
                .lists
                .remove(id)
                .and_then(|s| s.body)
                .unwrap_or(Value::Null);
        }
        if slot.at.elapsed() > LIST_TIMEOUT {
            st.lists.remove(id);
            return json!({"error":"The Student did not answer in time"});
        }
        json!({"pending":true})
    }

    pub(super) fn fs_snapshot(&self) -> Value {
        let mut st = self.3.lock().unwrap();
        for job in &mut st.jobs {
            for target in job.targets.values_mut() {
                if target.status == "queued" && target.changed.elapsed() > QUEUED_TIMEOUT {
                    target.status = "error".into();
                    target.error = "The Student did not start the transfer".into();
                }
            }
        }
        Value::Array(
            st.jobs
                .iter()
                .map(|job| {
                    json!({"id":job.id,"kind":job.kind.name(),"dest":job.dest.to_string_lossy(),
                    "flat":job.flat,
                    "targets":job.targets.iter().map(|(id, t)| json!({
                        "id":id,"name":t.name,"status":t.status,"folder":t.folder.to_string_lossy(),
                        "files_done":t.progress.files_done,"files_total":t.progress.files_total,
                        "bytes_done":t.progress.bytes_done,"bytes_total":t.progress.bytes_total,
                        "skipped":t.progress.skipped,"failed":t.progress.failed,
                        "current":t.progress.current,"error":t.error,"notes":t.progress.notes,
                    })).collect::<Vec<_>>()})
                })
                .collect(),
        )
    }
    /// A Student whose tools connection dropped before it started a request.
    pub(super) fn fs_disconnected(&self, student: &str) {
        let mut st = self.3.lock().unwrap();
        for job in &mut st.jobs {
            if let Some(t) = job.targets.get_mut(student) {
                if t.status == "queued" {
                    t.status = "error".into();
                    t.error = "Student connection interrupted".into();
                }
            }
        }
    }
    fn fs_acquire(&self) -> Option<Stream<'_>> {
        let mut st = self.3.lock().unwrap();
        (st.active < MAX_STREAMS).then(|| {
            st.active += 1;
            Stream(self)
        })
    }
    fn fs_progress(&self, id: &str, student: &str, progress: &Progress) {
        let mut st = self.3.lock().unwrap();
        if let Some(t) = st
            .jobs
            .iter_mut()
            .find(|j| j.id == id)
            .and_then(|j| j.targets.get_mut(student))
        {
            if !terminal(&t.status) {
                t.status = "transferring".into();
                t.changed = Instant::now();
                t.progress = progress.clone();
            }
        }
    }
    fn fs_finish(&self, id: &str, student: &str, result: Result<Progress>, cancel: &AtomicBool) {
        let mut st = self.3.lock().unwrap();
        let Some(t) = st
            .jobs
            .iter_mut()
            .find(|j| j.id == id)
            .and_then(|j| j.targets.get_mut(student))
        else {
            return;
        };
        if terminal(&t.status) {
            return;
        }
        t.changed = Instant::now();
        match result {
            Ok(progress) => {
                t.progress = progress;
                t.status = "complete".into();
                tracing::info!(job_id=%id,student_id=student,files=t.progress.files_done,failed=t.progress.failed,"Classroom file job finished");
            }
            Err(_) if cancel.load(Ordering::Acquire) => t.status = "cancelled".into(),
            Err(error) => {
                t.status = "error".into();
                t.error = file_tree::describe(&error).chars().take(300).collect();
                tracing::info!(job_id=%id,student_id=student,%error,"Classroom file job failed");
            }
        }
    }
    fn fs_find(
        &self,
        hello: &ToolPacket,
        allowed: fn(Kind) -> bool,
    ) -> Result<(Kind, Arc<AtomicBool>, Sink, Vec<PathBuf>)> {
        let st = self.3.lock().unwrap();
        let job = st
            .jobs
            .iter()
            .find(|j| {
                j.id == hello.id
                    && j.token == hello.token
                    && allowed(j.kind)
                    && j.targets.contains_key(&hello.student)
            })
            .context("Invalid file request")?;
        let target = &job.targets[&hello.student];
        anyhow::ensure!(
            !job.cancel.load(Ordering::Acquire) && !terminal(&target.status),
            "File job is no longer active"
        );
        Ok((
            job.kind,
            Arc::clone(&job.cancel),
            Sink {
                root: target.folder.clone(),
                flat: job.flat,
                tag: job.id.chars().take(8).collect(),
            },
            job.local.clone(),
        ))
    }
}

/// Teacher-side endpoint dispatch; `None` when `hello` is not a file request.
pub(super) fn serve(s: &mut TcpStream, hello: &ToolPacket, hub: &Hub) -> Option<Result<()>> {
    Some(match hello.action.as_str() {
        "fs_upload" => upload(s, hello, hub),
        "fs_download" => download(s, hello, hub),
        "fs_result" => result(s, hello, hub),
        _ => return None,
    })
}
fn upload(s: &mut TcpStream, hello: &ToolPacket, hub: &Hub) -> Result<()> {
    let (_, cancel, sink, _) = hub.fs_find(hello, |k| k != Kind::Push)?;
    let Some(_stream) = hub.fs_acquire() else {
        return send(s, &packet("busy"));
    };
    send(s, &packet("ready"))?;
    hub.fs_progress(&hello.id, &hello.student, &Progress::default());
    let outcome = file_tree::receive_tree(s, &sink, &cancel, &mut |p| {
        hub.fs_progress(&hello.id, &hello.student, p);
    });
    hub.fs_finish(&hello.id, &hello.student, outcome, &cancel);
    Ok(())
}
fn download(s: &mut TcpStream, hello: &ToolPacket, hub: &Hub) -> Result<()> {
    let (_, cancel, _, local) = hub.fs_find(hello, |k| k == Kind::Push)?;
    let Some(_stream) = hub.fs_acquire() else {
        return send(s, &packet("busy"));
    };
    send(s, &packet("ready"))?;
    hub.fs_progress(&hello.id, &hello.student, &Progress::default());
    let everything = Filter {
        exts: Vec::new(),
        recursive: true,
        hidden: true,
        max_file_mb: 4096,
        max_total_mb: 32 * 1024,
    };
    let outcome = match file_tree::plan(&local, &everything, true, &cancel) {
        Ok(plan) => file_tree::send_tree(
            s,
            &plan,
            file_tree::INTERACTIVE_DELAY,
            &cancel,
            &mut |p| hub.fs_progress(&hello.id, &hello.student, p),
        ),
        Err(error) => {
            let mut abort = packet("abort");
            abort.text = file_tree::describe(&error);
            let _ = send(s, &abort);
            Err(error)
        }
    };
    hub.fs_finish(&hello.id, &hello.student, outcome, &cancel);
    Ok(())
}
fn result(s: &mut TcpStream, hello: &ToolPacket, hub: &Hub) -> Result<()> {
    anyhow::ensure!(hello.size <= MAX_BODY, "Listing too large");
    {
        let st = hub.3.lock().unwrap();
        let slot = st.lists.get(&hello.id).context("Unknown listing request")?;
        anyhow::ensure!(
            slot.student == hello.student && slot.token == hello.token && slot.body.is_none(),
            "Invalid listing request"
        );
    }
    send(s, &packet("ready"))?;
    let mut body = vec![0; usize::try_from(hello.size)?];
    s.read_exact(&mut body)?;
    let value: Value = serde_json::from_slice(&body).context("Invalid listing")?;
    let is_activity = {
        let mut st = hub.3.lock().unwrap();
        let slot = st.lists.get_mut(&hello.id);
        let is_activity = slot.as_ref().is_some_and(|s| s.is_activity);
        if let Some(slot) = slot {
            slot.body = Some(value.clone());
        }
        is_activity
    };
    if is_activity {
        hub.activity_ingest(&hello.student, value);
    }
    send(s, &packet("ok"))
}

// ---------------------------------------------------------------- Student ---

pub(super) struct Run {
    pub id: String,
    pub cancel: Arc<AtomicBool>,
    pub worker: thread::JoinHandle<()>,
}
/// Opens a stream connection and waits for the Teacher's `ready` (retrying
/// while its stream slots are busy).
fn connect(
    address: SocketAddrV4,
    action: &str,
    p: &ToolPacket,
    identity: &str,
    cancel: &AtomicBool,
    stop: &AtomicBool,
) -> Result<TcpStream> {
    for _ in 0..600 {
        anyhow::ensure!(
            !cancel.load(Ordering::Acquire) && !stop.load(Ordering::Acquire),
            "Cancelled"
        );
        let mut s = TcpStream::connect_timeout(&address.into(), Duration::from_secs(2))?;
        configure(&s)?;
        let mut hello = packet(action);
        hello.id.clone_from(&p.id);
        hello.token.clone_from(&p.token);
        identity.clone_into(&mut hello.student);
        send(&mut s, &hello)?;
        match read(&mut s)?.action.as_str() {
            "ready" => return Ok(s),
            "busy" => thread::sleep(Duration::from_millis(500 + u64::from(p.id.as_bytes()[0]) * 4)),
            _ => anyhow::bail!("Teacher rejected the file request"),
        }
    }
    anyhow::bail!("Teacher stayed busy")
}
pub(super) fn start(
    address: SocketAddrV4,
    identity: String,
    p: ToolPacket,
    stop: Arc<AtomicBool>,
) -> Run {
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    let id = p.id.clone();
    let worker = thread::spawn(move || {
        if let Err(error) = run(address, &identity, &p, &flag, &stop) {
            tracing::info!(job_id=%p.id,%error,"Student file job ended");
        }
    });
    Run { id, cancel, worker }
}
fn run(
    address: SocketAddrV4,
    identity: &str,
    p: &ToolPacket,
    cancel: &AtomicBool,
    stop: &AtomicBool,
) -> Result<()> {
    let params: Value = serde_json::from_str(&p.text).context("Invalid file request")?;
    if p.action == "fs_receive" {
        let dest = file_tree::resolve(params["dest"].as_str().unwrap_or(""))?;
        let mut s = connect(address, "fs_download", p, identity, cancel, stop)?;
        let sink = Sink {
            root: dest,
            flat: false,
            tag: p.id.chars().take(8).collect(),
        };
        file_tree::receive_tree(&mut s, &sink, cancel, &mut |_| {})?;
        return Ok(());
    }
    let filter: Filter = serde_json::from_value(params["filter"].clone()).unwrap_or_default();
    let roots = params["paths"]
        .as_array()
        .context("No paths requested")?
        .iter()
        .take(MAX_PATHS)
        .filter_map(Value::as_str)
        .map(file_tree::resolve)
        .collect::<Result<Vec<_>>>();
    let files_only = params["files_only"].as_bool().unwrap_or(false);
    let roots = roots.and_then(|roots| {
        if files_only && roots.iter().any(|r| !r.is_file()) {
            anyhow::bail!("Only files can be transferred here, not folders");
        }
        Ok(roots)
    });
    let plan = roots.and_then(|roots| {
        file_tree::plan(
            &roots,
            &filter,
            params["keep_root"].as_bool().unwrap_or(false),
            cancel,
        )
    });
    let mut s = connect(address, "fs_upload", p, identity, cancel, stop)?;
    match plan {
        Ok(plan) => {
            let delay = if params["fast"].as_bool().unwrap_or(false) {
                file_tree::INTERACTIVE_DELAY
            } else {
                file_tree::BULK_DELAY
            };
            file_tree::send_tree(&mut s, &plan, delay, cancel, &mut |_| {})?;
        }
        Err(error) => {
            let mut abort = packet("abort");
            abort.text = file_tree::describe(&error);
            send(&mut s, &abort)?;
        }
    }
    Ok(())
}
/// Answers a Teacher listing request on its own short-lived thread; returns
/// `false` when too many listings are already running.
pub(super) fn list(
    address: SocketAddrV4,
    identity: String,
    p: ToolPacket,
    active: &Arc<AtomicUsize>,
) -> bool {
    if active.fetch_add(1, Ordering::AcqRel) >= 3 {
        active.fetch_sub(1, Ordering::AcqRel);
        return false;
    }
    let slot = Slot(Arc::clone(active));
    thread::spawn(move || {
        let _slot = slot;
        let outcome = (|| -> Result<()> {
            let params: Value = serde_json::from_str(&p.text)?;
            let body = file_tree::list(
                params["path"].as_str().unwrap_or(""),
                params["hidden"].as_bool().unwrap_or(false),
            )
            .to_string();
            let mut s = TcpStream::connect_timeout(&address.into(), Duration::from_secs(2))?;
            configure(&s)?;
            let mut hello = packet("fs_result");
            hello.id.clone_from(&p.id);
            hello.token.clone_from(&p.token);
            hello.student = identity;
            hello.size = body.len() as u64;
            send(&mut s, &hello)?;
            anyhow::ensure!(read(&mut s)?.action == "ready", "Listing rejected");
            s.write_all(body.as_bytes())?;
            anyhow::ensure!(read(&mut s)?.action == "ok", "Listing not accepted");
            Ok(())
        })();
        if let Err(error) = outcome {
            tracing::info!(%error,"Student directory listing failed");
        }
    });
    true
}
/// Touches the protected folders once so macOS shows its folder-access prompts
/// at start-up, not in the middle of the first collection.
pub(super) fn request_folder_access() {
    if cfg!(target_os = "macos") {
        thread::spawn(|| {
            for name in ["Desktop", "Documents", "Downloads"] {
                if let Ok(path) = super::user_folder(name) {
                    let _ = std::fs::read_dir(path);
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, net::TcpListener};

    fn temp(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "classroom-fs-{name}-{}-{}",
            std::process::id(),
            media_core::unix_us()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }
    fn ready_student(hub: &Hub, id: &str) {
        let mut st = hub.0.lock().unwrap();
        st.peers.insert(id.into(), 1);
        st.capabilities.insert(id.into(), json!({"fs":1}));
    }

    #[test]
    fn collect_end_to_end_over_the_teacher_endpoint() {
        let hub = Arc::new(Hub::default());
        ready_student(&hub, "stu1");
        let src = temp("src");
        fs::write(src.join("essay.docx"), "words").unwrap();
        fs::write(src.join("notes.txt"), "skip me").unwrap();
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::write(src.join("sub/lab.docx"), "lab").unwrap();
        let root = temp("root").join("Collected");
        let job = hub
            .fs_collect(
                &[("stu1".into(), "PC 01".into())],
                &json!({"path":src.to_string_lossy(),"exts":"docx","recursive":true}),
                &root,
            )
            .unwrap();
        // The queued Student request carries a token for this job only.
        let request = hub.0.lock().unwrap().queues["stu1"][0].clone();
        assert_eq!((request.action.as_str(), request.id.as_str()), ("fs_send", job.as_str()));

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = match listener.local_addr().unwrap() {
            std::net::SocketAddr::V4(a) => a,
            std::net::SocketAddr::V6(_) => unreachable!(),
        };
        let server_hub = Arc::clone(&hub);
        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            configure(&s).unwrap();
            let hello = read(&mut s).unwrap();
            serve(&mut s, &hello, &server_hub).unwrap().unwrap();
        });
        let cancel = AtomicBool::new(false);
        run(address, "stu1", &request, &cancel, &AtomicBool::new(false)).unwrap();
        server.join().unwrap();

        let folder = root.join("PC 01");
        assert_eq!(fs::read_to_string(folder.join("essay.docx")).unwrap(), "words");
        assert_eq!(fs::read_to_string(folder.join("sub/lab.docx")).unwrap(), "lab");
        assert!(!folder.join("notes.txt").exists());
        let snapshot = hub.fs_snapshot();
        let target = &snapshot[0]["targets"][0];
        assert_eq!(target["status"], "complete");
        assert_eq!(target["files_done"], 2);
        // A second connection with the finished job's token is refused.
        assert!(hub.fs_find(&request, |_| true).is_err());
        fs::remove_dir_all(src).unwrap();
        fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn requests_need_capability_token_and_known_student() {
        let hub = Hub::default();
        let root = temp("deny").join("c");
        assert!(hub
            .fs_collect(&[("ghost".into(), "G".into())], &json!({}), &root)
            .is_err());
        hub.0.lock().unwrap().peers.insert("old".into(), 1);
        let error = hub
            .fs_collect(&[("old".into(), "O".into())], &json!({}), &root)
            .unwrap_err();
        assert!(error.to_string().contains("older build"));
        ready_student(&hub, "new");
        hub.fs_collect(&[("new".into(), "N".into())], &json!({}), &root)
            .unwrap();
        let mut forged = packet("fs_upload");
        forged.student = "new".into();
        forged.token = "wrong".into();
        forged.id = hub.0.lock().unwrap().queues["new"][0].id.clone();
        assert!(hub.fs_find(&forged, |_| true).is_err());
        fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn file_manager_refuses_folders() {
        let hub = Hub::default();
        ready_student(&hub, "s");
        let dir = temp("folder");
        let error = hub
            .fs_transfer(true, ("s", "S"), &[dir.to_string_lossy().into_owned()], "~desktop")
            .unwrap_err();
        assert!(error.to_string().contains("Only existing files"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn duplicate_names_get_distinct_folders_and_cancel_is_terminal() {
        let hub = Hub::default();
        ready_student(&hub, "aaaa1");
        ready_student(&hub, "bbbb2");
        let root = temp("dup").join("c");
        let job = hub
            .fs_collect(
                &[("aaaa1".into(), "PC".into()), ("bbbb2".into(), "pc".into())],
                &json!({}),
                &root,
            )
            .unwrap();
        let snapshot = hub.fs_snapshot();
        let mut folders: Vec<_> = snapshot[0]["targets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["folder"].as_str().unwrap().to_owned())
            .collect();
        folders.sort();
        assert_ne!(folders[0], folders[1]);
        hub.fs_cancel(&job).unwrap();
        assert!(hub.fs_snapshot()[0]["targets"]
            .as_array()
            .unwrap()
            .iter()
            .all(|t| t["status"] == "cancelled"));
        let queue = &hub.0.lock().unwrap().queues["aaaa1"];
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].action, "cancel");
        fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }

    #[test]
    fn listing_results_are_consumed_once_and_local_listing_works() {
        let hub = Arc::new(Hub::default());
        let dir = temp("listing");
        fs::write(dir.join("a.txt"), "a").unwrap();
        let id = hub.fs_list_local(dir.to_str().unwrap(), false).unwrap();
        let mut value = json!({"pending":true});
        for _ in 0..100 {
            value = hub.fs_list_result(&id);
            if value["pending"] != true {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(value["entries"][0]["n"], "a.txt");
        assert!(hub.fs_list_result(&id)["error"].is_string());
        fs::remove_dir_all(dir).unwrap();
    }
}
