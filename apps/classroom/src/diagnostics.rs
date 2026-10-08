//! Bounded diagnostic log collection, separate from media/control connections.
use anyhow::{Context, Result};
use classroom_config::Config;
use control_transport::room::RoomStudent;
use protocol::ToolPacket;
use serde_json::{json, Value};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

pub const MAX_LOG_BYTES: usize = 2 * 1024 * 1024;
pub const CHUNK: usize = 16 * 1024;
pub const PACE: Duration = Duration::from_millis(32); // 512 KiB/s; one Student at a time

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
fn active_log(config: &Config) -> Result<PathBuf> {
    let args: Vec<_> = std::env::args_os().collect();
    let explicit = args.iter().enumerate().find_map(|(i, v)| {
        if v == "--log-file" {
            args.get(i + 1).map(PathBuf::from)
        } else {
            v.to_str()
                .and_then(|v| v.strip_prefix("--log-file="))
                .map(PathBuf::from)
        }
    });
    let identity = if config.app.role == "teacher" {
        format!("teacher-{}", config.network.control_port)
    } else {
        format!("student-{}", config.network.media_port)
    };
    super::compact_log::path_for(&identity, explicit.as_deref())
}
pub fn log_folder(config: &Config) -> Result<PathBuf> {
    active_log(config)?
        .parent()
        .map(Path::to_path_buf)
        .context("log folder unavailable")
}

// Only compact runtime logs are read. Endpoint descriptors, UI caches, message/
// file payloads and captured pixels are never traversed or added to an archive.
pub fn sanitize_log(input: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(input);
    let mut output = String::new();
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        if [
            "token",
            "password",
            "secret",
            "clipboard",
            " text=",
            "reply=",
            "jpeg=",
            "screenshot",
            "kind=message",
            "tool_message",
            "tool_file",
        ]
        .iter()
        .any(|key| lower.contains(key))
        {
            continue;
        }
        output.push_str(line);
        output.push('\n');
    }
    output.into_bytes()
}
fn safe_value(mut value: Value) -> Value {
    if let Value::Object(values) = &mut value {
        values.retain(|key, _| {
            !["token", "password", "secret", "auth"]
                .iter()
                .any(|word| key.to_ascii_lowercase().contains(word))
        });
        for child in values.values_mut() {
            *child = safe_value(child.take());
        }
    } else if let Value::Array(values) = &mut value {
        for child in values {
            *child = safe_value(child.take());
        }
    }
    value
}
#[derive(Clone)]
pub struct Source {
    metadata: Value,
    log: PathBuf,
}
impl Source {
    pub fn new(config: &Config, hostname: &str) -> Result<Self> {
        Ok(Self {
            metadata: json!({
                "version":env!("CARGO_PKG_VERSION"), "build_id":env!("CLASSROOM_BUILD_ID"),
                "os":std::env::consts::OS, "architecture":std::env::consts::ARCH,
                "hostname":hostname, "role":config.app.role, "configuration":safe_value(serde_json::to_value(config)?),
                "id":super::startup::student_identity(&super::startup::profile_path(None)?, config.network.media_port)?,
            }),
            log: active_log(config)?,
        })
    }
    pub fn capture(&self) -> Result<(Value, Vec<u8>)> {
        let mut file = File::open(&self.log).context("runtime log unavailable")?;
        let length = file.metadata()?.len();
        let truncated = length > MAX_LOG_BYTES as u64;
        if truncated {
            file.seek(SeekFrom::Start(length - MAX_LOG_BYTES as u64))?;
        }
        let mut bytes =
            Vec::with_capacity(MAX_LOG_BYTES.min(usize::try_from(length).unwrap_or(MAX_LOG_BYTES)));
        file.take(MAX_LOG_BYTES as u64).read_to_end(&mut bytes)?;
        if truncated {
            // Avoid retaining the tail of a potentially sensitive split line.
            if let Some(at) = bytes.iter().position(|b| *b == b'\n') {
                bytes.drain(..=at);
            } else {
                bytes.clear();
            }
        }
        let mut metadata = self.metadata.clone();
        metadata["captured_unix_ms"] = json!(unix_ms());
        metadata["log_tail_limit_bytes"] = json!(MAX_LOG_BYTES);
        metadata["log_truncated"] = json!(truncated);
        Ok((metadata, sanitize_log(&bytes)))
    }
}
struct Upload {
    metadata: Value,
    log: Vec<u8>,
}
struct Expected {
    id: String,
    token: String,
    student: String,
    response: Option<std::result::Result<Upload, String>>,
}
#[derive(Default)]
pub struct Service {
    expected: Mutex<Option<Expected>>,
}
impl Service {
    pub fn receive(
        &self,
        socket: &mut TcpStream,
        packet: &ToolPacket,
        stop: &AtomicBool,
    ) -> Result<()> {
        {
            let slot = self.expected.lock().unwrap();
            let expected = slot.as_ref().context("no diagnostic request pending")?;
            anyhow::ensure!(
                expected.id == packet.id
                    && expected.token == packet.token
                    && expected.student == packet.student
                    && expected.response.is_none(),
                "diagnostic request expired or does not match Student"
            );
        }
        let result = (|| -> Result<Upload> {
            anyhow::ensure!(packet.size <= MAX_LOG_BYTES as u64, "diagnostic log exceeds bound");
            let supplied: Value = serde_json::from_str(&packet.text)?;
            // Do not persist arbitrary additional metadata fields from a peer.
            let metadata = safe_value(json!({
                "version":supplied["version"], "build_id":supplied["build_id"],
                "os":supplied["os"], "architecture":supplied["architecture"],
                "role":supplied["role"], "configuration":supplied["configuration"],
                "captured_unix_ms":supplied["captured_unix_ms"], "log_truncated":supplied["log_truncated"],
                "log_tail_limit_bytes":supplied["log_tail_limit_bytes"],
            }));
            let mut log = vec![0; usize::try_from(packet.size)?];
            for chunk in log.chunks_mut(CHUNK) {
                anyhow::ensure!(!stop.load(Ordering::Acquire), "diagnostic upload cancelled");
                anyhow::ensure!(self.expected.lock().unwrap().as_ref().is_some_and(|v| v.id == packet.id && v.token == packet.token), "diagnostic request expired");
                socket.read_exact(chunk)?;
            }
            Ok(Upload { metadata, log: sanitize_log(&log) })
        })().map_err(|e| e.to_string());
        let mut slot = self.expected.lock().unwrap();
        if let Some(expected) = slot
            .as_mut()
            .filter(|v| v.id == packet.id && v.student == packet.student)
        {
            expected.response = Some(result);
        }
        Ok(())
    }
}

pub struct Collector {
    pub service: Arc<Service>,
    source: Source,
    folder: PathBuf,
    state: Arc<Mutex<Value>>,
    cancel: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Collector {
    pub fn new(config: &Config, folder: &Path) -> Result<Self> {
        let hostname = super::host_name();
        Ok(Self {
            service: Arc::new(Service::default()),
            source: Source::new(config, &hostname)?,
            folder: folder.join("diagnostics"),
            state: Arc::new(Mutex::new(json!({"phase":"idle"}))),
            cancel: Arc::new(AtomicBool::new(false)),
            worker: None,
        })
    }
    pub fn snapshot(&self) -> Value {
        self.state.lock().unwrap().clone()
    }
    pub fn poll(&mut self) {
        if self
            .worker
            .as_ref()
            .is_some_and(thread::JoinHandle::is_finished)
        {
            let _ = self.worker.take().unwrap().join();
        }
    }
    pub fn cancel(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.service.expected.lock().unwrap().take();
        *self.state.lock().unwrap() = json!({"phase":"cancelled"});
    }
    pub fn start(
        &mut self,
        hub: Arc<super::classroom_tools::Hub>,
        rows: Vec<RoomStudent>,
        selected: &[String],
        stream_id: u64,
        teacher_ip: Ipv4Addr,
    ) -> Result<()> {
        self.poll();
        anyhow::ensure!(
            self.worker.is_none(),
            "diagnostics collection is already running"
        );
        let rows: Vec<_> = rows
            .into_iter()
            .filter(|r| selected.is_empty() || selected.contains(&r.instance_id))
            .take(128)
            .collect();
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = Arc::clone(&self.cancel);
        let service = Arc::clone(&self.service);
        let source = self.source.clone();
        let folder = self.folder.clone();
        let state = Arc::clone(&self.state);
        *state.lock().unwrap() =
            json!({"phase":"collecting", "collected":0,"total":rows.len(),"stream_id":stream_id});
        self.worker = Some(thread::spawn(move || {
            let result = collect(
                &source, &folder, &hub, &service, &rows, stream_id, teacher_ip, &cancel, &state,
            );
            if let Err(error) = result {
                *state.lock().unwrap() = json!({"phase":"error", "error":error.to_string()});
            }
        }));
        Ok(())
    }
}
impl Drop for Collector {
    fn drop(&mut self) {
        self.cancel();
    }
}

use std::net::Ipv4Addr;
fn zip_entry(zip: &mut ZipWriter<File>, name: &str, bytes: &[u8]) -> Result<()> {
    zip.start_file(
        name,
        SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(3)),
    )?;
    zip.write_all(bytes)?;
    Ok(())
}
struct Partial(PathBuf);
impl Drop for Partial {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
#[allow(clippy::too_many_arguments, clippy::too_many_lines)] // one bounded collector owns the ZIP and serial requests
fn collect(
    source: &Source,
    folder: &Path,
    hub: &Arc<super::classroom_tools::Hub>,
    service: &Service,
    rows: &[RoomStudent],
    stream_id: u64,
    teacher_ip: Ipv4Addr,
    stop: &AtomicBool,
    state: &Mutex<Value>,
) -> Result<()> {
    fs::create_dir_all(folder)?;
    let stamp = unix_ms();
    let destination = folder.join(format!(
        "Classroom-diagnostics-{stamp}-session-{stream_id}.zip"
    ));
    let partial = destination.with_extension("zip.partial");
    let _partial = Partial(partial.clone());
    let mut zip = ZipWriter::new(File::create(&partial)?);
    // Let final Stop telemetry reach the bounded log writer before snapshotting.
    for _ in 0..25 {
        if stop.load(Ordering::Acquire) {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let (teacher_metadata, log) = source.capture()?;
    zip_entry(&mut zip, "teacher/log.txt", &log)?;
    zip_entry(
        &mut zip,
        "teacher/metadata.json",
        &serde_json::to_vec_pretty(&teacher_metadata)?,
    )?;
    let mut outcomes = Vec::new();
    let mut collected = 0;
    for (index, row) in rows.iter().enumerate() {
        let mut record = json!({"id":row.instance_id,"name":row.hostname,"ip":row.endpoint.ip().to_string(),"status":"not_collected","reason":"offline"});
        if stop.load(Ordering::Acquire) {
            record["reason"] = json!("collection cancelled or new presentation started");
        } else if row.online {
            let id = super::classroom_tools::nonce()?;
            let token = super::classroom_tools::nonce()?;
            *service.expected.lock().unwrap() = Some(Expected {
                id: id.clone(),
                token: token.clone(),
                student: row.instance_id.clone(),
                response: None,
            });
            if hub
                .request_diagnostics(&row.instance_id, &id, &token)
                .is_err()
            {
                record["reason"] = json!("Student diagnostics connection is unavailable");
            } else {
                let deadline = Instant::now() + Duration::from_secs(15);
                let response = loop {
                    if let Some(value) = service
                        .expected
                        .lock()
                        .unwrap()
                        .as_mut()
                        .and_then(|v| v.response.take())
                    {
                        break Some(value);
                    }
                    if stop.load(Ordering::Acquire) || Instant::now() >= deadline {
                        break None;
                    }
                    thread::sleep(Duration::from_millis(20));
                };
                match response {
                    Some(Ok(mut upload)) => {
                        upload.metadata["id"] = json!(row.instance_id);
                        upload.metadata["hostname"] = json!(row.hostname);
                        upload.metadata["ip"] = json!(row.endpoint.ip());
                        let prefix = format!("students/{index:03}");
                        zip_entry(&mut zip, &format!("{prefix}/log.txt"), &upload.log)?;
                        zip_entry(
                            &mut zip,
                            &format!("{prefix}/metadata.json"),
                            &serde_json::to_vec_pretty(&upload.metadata)?,
                        )?;
                        record["status"] = json!("collected");
                        record["reason"] = Value::Null;
                        record["directory"] = json!(prefix);
                        collected += 1;
                    }
                    Some(Err(_)) => {
                        record["reason"] = json!("Student log could not be read or transferred");
                    }
                    None => {
                        record["reason"] = json!(
                            "Student disconnected, request timed out, or collection cancelled"
                        );
                    }
                }
            }
            service.expected.lock().unwrap().take();
        }
        outcomes.push(record);
        *state.lock().unwrap() = json!({"phase":"collecting","collected":collected,"total":rows.len(),"completed":outcomes.len(),"stream_id":stream_id});
    }
    let manifest = json!({"schema":1,"created_unix_ms":stamp,"stream_id":stream_id,"teacher_ip":teacher_ip,
        "teacher":teacher_metadata,"students":outcomes,"transfer_limit_kib_per_second":512,"log_tail_limit_bytes":MAX_LOG_BYTES,
        "excluded":["IPC descriptors and tokens","screenshots and thumbnails","message and file contents"],"cancelled":stop.load(Ordering::Acquire)});
    zip_entry(
        &mut zip,
        "manifest.json",
        &serde_json::to_vec_pretty(&manifest)?,
    )?;
    zip.finish()?.sync_all()?;
    fs::rename(&partial, &destination)?;
    *state.lock().unwrap() = json!({"phase":"complete","path":destination,"collected":collected,"total":rows.len(),"missing":rows.len()-collected,"students":outcomes,"stream_id":stream_id});
    tracing::info!(
        students_collected = collected,
        students_missing = rows.len() - collected,
        stream_id,
        "Classroom diagnostic ZIP completed"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_wins_over_worker_completion_and_clears_upload_request() {
        let state = Arc::new(Mutex::new(json!({"phase":"collecting"})));
        let cancel = Arc::new(AtomicBool::new(false));
        let service = Arc::new(Service::default());
        *service.expected.lock().unwrap() = Some(Expected {
            id: "fixture".into(), token: "fixture".into(), student: "fixture".into(), response: None,
        });
        let worker_state = Arc::clone(&state);
        let worker_cancel = Arc::clone(&cancel);
        let worker = thread::spawn(move || {
            while !worker_cancel.load(Ordering::Acquire) { thread::yield_now(); }
            *worker_state.lock().unwrap() = json!({"phase":"complete"});
        });
        let mut collector = Collector {
            service, source: Source { metadata: json!({}), log: PathBuf::new() },
            folder: PathBuf::new(), state, cancel, worker: Some(worker),
        };
        collector.cancel();
        assert_eq!(collector.snapshot()["phase"], "cancelled");
        assert!(collector.service.expected.lock().unwrap().is_none());
        assert!(collector.worker.is_none());
    }
    #[test]
    fn archive_records_missing_students_and_contains_only_diagnostics() {
        let folder = std::env::temp_dir().join(format!(
            "classroom-diagnostics-{}",
            super::super::classroom_tools::nonce().unwrap()
        ));
        fs::create_dir_all(&folder).unwrap();
        let log = folder.join("runtime.txt");
        fs::write(&log, b"INFO decode_fps=25 render_fps=25\nINFO ipc_token=private\nINFO kind=message text=private\n").unwrap();
        let source = Source {
            metadata: json!({"version":"test","build_id":"test","hostname":"teacher"}),
            log,
        };
        let row = RoomStudent {
            client_id: 1,
            instance_id: "student-id".into(),
            hostname: "offline-student".into(),
            endpoint: std::net::SocketAddrV4::new(Ipv4Addr::LOCALHOST, 48710),
            online: false,
            thumbnail: None,
            monitoring_state: "offline".into(),
            media: None,
        };
        let state = Mutex::new(Value::Null);
        collect(
            &source,
            &folder,
            &Arc::new(super::super::classroom_tools::Hub::default()),
            &Service::default(),
            &[row],
            42,
            Ipv4Addr::LOCALHOST,
            &AtomicBool::new(false),
            &state,
        )
        .unwrap();
        let path = PathBuf::from(state.lock().unwrap()["path"].as_str().unwrap());
        let mut zip = zip::ZipArchive::new(File::open(path).unwrap()).unwrap();
        assert_eq!(zip.len(), 3);
        let mut text = String::new();
        zip.by_name("teacher/log.txt")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert!(text.contains("decode_fps=25"));
        assert!(!text.contains("private"));
        text.clear();
        zip.by_name("manifest.json")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        let manifest: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(manifest["students"][0]["status"], "not_collected");
        assert_eq!(manifest["students"][0]["reason"], "offline");
        assert_eq!(manifest["stream_id"], 42);
        drop(zip);
        fs::remove_dir_all(folder).unwrap();
    }
    #[test]
    fn diagnostic_privacy_keeps_recovery_metrics_and_excludes_sensitive_data() {
        let log = b"INFO student UDP recovery totals ingress_dropped_packets=3\nINFO token=private\nINFO clipboard=private\nINFO kind=message text=private\nINFO screenshot jpeg=private\nWARN decoder failed error=bad_frame\n";
        let filtered = String::from_utf8(sanitize_log(log)).unwrap();
        assert!(filtered.contains("ingress_dropped_packets=3"));
        assert!(filtered.contains("decoder failed"));
        assert!(!filtered.contains("private"));
        let metadata = safe_value(
            json!({"network":{"token":"private","teacher_ip":"192.168.50.94"},"password":"private"}),
        );
        assert!(!metadata.to_string().contains("private"));
        assert_eq!(metadata["network"]["teacher_ip"], "192.168.50.94");
    }
}
