//! Complete INFO-level interval telemetry, with bounded asynchronous file writes,
//! rotation and automatic backup. Repeated warning details are rate limited.
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, IsTerminal, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, SyncSender},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context as _, Result};
use tracing::{field::Visit, Event, Level, Subscriber};
use tracing_subscriber::{filter::LevelFilter, layer::Context, prelude::*, EnvFilter, Layer};

const SAMPLE_EVERY: Duration = Duration::from_secs(1);
const REPEAT_WARNING_EVERY: Duration = Duration::from_secs(10);

pub fn path_for(identity: &str, override_path: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = override_path {
        return if path.is_absolute() {
            Ok(path.to_owned())
        } else {
            Ok(std::env::current_dir()?.join(path))
        };
    }
    let executable = std::env::current_exe()?;
    default_path(&executable, identity)
}

fn default_path(executable: &Path, identity: &str) -> Result<PathBuf> {
    let directory = executable
        .parent()
        .context("executable has no parent directory")?;
    // A signed .app is immutable. Writing logs inside Contents invalidates its
    // resource seal and can make macOS reject its privacy identity on relaunch.
    #[cfg(target_os = "macos")]
    if directory.file_name().is_some_and(|v| v == "MacOS")
        && directory
            .parent()
            .is_some_and(|v| v.file_name().is_some_and(|n| n == "Contents"))
    {
        let home = std::env::var_os("HOME").context("Mac user home unavailable")?;
        return Ok(PathBuf::from(home)
            .join("Library/Logs/Classroom")
            .join(identity)
            .join("log.txt"));
    }
    Ok(directory.join("logs").join(identity).join("log.txt"))
}

pub fn init(level: &str, path: &Path) -> Result<LogGuard> {
    let filter =
        EnvFilter::try_new(level).with_context(|| format!("invalid log filter '{level}'"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create log directory {}", parent.display()))?;
    }
    archive_previous_log(path)?;
    remove_previous_log(path)?;
    // Per role/port defaults prevent a same-host Teacher and Student from
    // truncating one another's files. Explicit --log-file is the caller's path.
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .with_context(|| format!("cannot open compact log {}", path.display()))?;
    let console = tracing_subscriber::fmt::layer()
        .with_ansi(!cfg!(windows) && io::stdout().is_terminal())
        .with_target(false)
        .compact()
        .with_filter(filter)
        .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
            metadata.target() != "telemetry"
        }));
    let (compact, guard) = CompactLog::new(file, path.to_owned());
    let compact = compact.with_filter(LevelFilter::INFO);
    tracing_subscriber::registry()
        .with(console)
        .with(compact)
        .try_init()
        .map_err(|error| anyhow::anyhow!("failed to initialize logging: {error}"))?;
    Ok(guard)
}

fn archive_previous_log(path: &Path) -> Result<()> {
    if !path.is_file() || path.metadata()?.len() == 0 {
        return Ok(());
    }
    let directory = path.parent().context("log parent missing")?.join("archive");
    fs::create_dir_all(&directory)?;
    let name = path
        .file_name()
        .context("log name missing")?
        .to_string_lossy();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    fs::rename(path, directory.join(format!("{name}.{stamp}.txt")))?;
    let mut entries: Vec<_> = fs::read_dir(&directory)?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(&format!("{name}."))
        })
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    let remove = entries.len().saturating_sub(5);
    for old in entries.into_iter().take(remove) {
        fs::remove_file(old)?;
    }
    Ok(())
}

fn remove_previous_log(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error)
                .with_context(|| format!("cannot remove old compact log {}", path.display()));
        }
    }
    Ok(())
}

enum Record {
    Line(String),
    Shutdown(mpsc::Sender<()>),
}

pub struct LogGuard {
    sender: SyncSender<Record>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Drop for LogGuard {
    fn drop(&mut self) {
        let (sender, receiver) = mpsc::channel();
        if self.sender.try_send(Record::Shutdown(sender)).is_ok() {
            let _ = receiver.recv_timeout(Duration::from_secs(2));
        }
        if self
            .worker
            .as_ref()
            .is_some_and(thread::JoinHandle::is_finished)
        {
            let _ = self.worker.take().unwrap().join();
        }
    }
}

struct CompactLog {
    state: Mutex<State>,
    sender: SyncSender<Record>,
    dropped: Arc<AtomicU64>,
}
struct State {
    teacher_sample: Option<Instant>,
    student_sample: Option<Instant>,
    warnings: BTreeMap<String, Instant>,
}
impl CompactLog {
    fn new(file: File, path: PathBuf) -> (Self, LogGuard) {
        let (sender, receiver) = mpsc::sync_channel(1024);
        let dropped = Arc::new(AtomicU64::new(0));
        let worker_dropped = Arc::clone(&dropped);
        let worker = thread::spawn(move || {
            let mut file = BufWriter::with_capacity(64 * 1024, file);
            let mut size = 0_u64;
            let mut last_flush = Instant::now();
            loop {
                let record = match receiver.recv_timeout(Duration::from_millis(250)) {
                    Ok(record) => record,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        if let Err(error) = file.flush() {
                            eprintln!("session log flush failed: {error}");
                            break;
                        }
                        last_flush = Instant::now();
                        continue;
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        let _ = file.flush();
                        break;
                    }
                };
                match record {
                    Record::Line(line) => {
                        let lost = worker_dropped.swap(0, Ordering::Relaxed);
                        if lost > 0 {
                            let _ = writeln!(
                                file,
                                "WARN telemetry writer queue overflow dropped_records={lost}"
                            );
                        }
                        if size + line.len() as u64 > 64 * 1024 * 1024 {
                            let _ = file.flush();
                            drop(file);
                            if archive_previous_log(&path).is_err() {
                                break;
                            }
                            match File::create(&path) {
                                Ok(next) => {
                                    file = BufWriter::with_capacity(64 * 1024, next);
                                    size = 0;
                                }
                                Err(_) => break,
                            }
                        }
                        let write = file.write_all(line.as_bytes()).and_then(|()| {
                            if last_flush.elapsed() >= Duration::from_millis(250) {
                                last_flush = Instant::now();
                                file.flush()
                            } else {
                                Ok(())
                            }
                        });
                        if let Err(error) = write {
                            eprintln!("session log write failed: {error}");
                            break;
                        }
                        size += line.len() as u64;
                    }
                    Record::Shutdown(done) => {
                        let _ = file.flush();
                        let _ = done.send(());
                        break;
                    }
                }
            }
        });
        (
            Self {
                state: Mutex::new(State {
                    teacher_sample: None,
                    student_sample: None,
                    warnings: BTreeMap::new(),
                }),
                sender: sender.clone(),
                dropped,
            },
            LogGuard {
                sender,
                worker: Some(worker),
            },
        )
    }
}

impl<S: Subscriber> Layer<S> for CompactLog {
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        let mut visitor = Fields::default();
        event.record(&mut visitor);
        let Some(message) = visitor.0.remove("message") else {
            return;
        };
        let level = *event.metadata().level();
        let kind = classify(&message, level);
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if !state.should_write(kind, &message, Instant::now()) {
            return;
        }
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let mut line = format!("unix_ms={timestamp} {level} {message}");
        for (key, value) in &visitor.0 {
            line.push(' ');
            line.push_str(key);
            line.push('=');
            line.push_str(&compact_value(
                value,
                if key == "config" || key == "report" {
                    4096
                } else if key == "error" {
                    512
                } else {
                    256
                },
            ));
        }
        line.push('\n');
        drop(state);
        if self.sender.try_send(Record::Line(line)).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[derive(Default)]
struct Fields(BTreeMap<String, String>);

impl Visit for Fields {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let value = format!("{value:?}");
        self.0.insert(
            field.name().to_owned(),
            if field.name() == "message" {
                value.trim_matches('"').to_owned()
            } else {
                value
            },
        );
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Event,
    TeacherSample,
    StudentSample,
    TeacherFinal,
    StudentFinal,
    Warning,
}

fn classify(message: &str, level: Level) -> Kind {
    if level == Level::ERROR || level == Level::WARN {
        return Kind::Warning;
    }
    match message {
        "teacher M3 media interval" => Kind::TeacherSample,
        "student M3 media interval" => Kind::StudentSample,
        "teacher M3 media complete" => Kind::TeacherFinal,
        "student M3 media complete" => Kind::StudentFinal,
        "teacher scheduled an immediate VideoToolbox IDR for the joining student"
        | "teacher scheduled an immediate VideoToolbox recovery IDR"
        | "teacher received keyframe request"
        | "teacher rate-limited VideoToolbox session rebuild; scheduled IDR remains available"
        | "teacher handled receiver NACK" => Kind::Warning,
        _ => Kind::Event,
    }
}

impl State {
    fn should_write(&mut self, kind: Kind, message: &str, now: Instant) -> bool {
        match kind {
            Kind::TeacherSample => sample_due(&mut self.teacher_sample, now),
            Kind::StudentSample => sample_due(&mut self.student_sample, now),
            Kind::Warning => {
                // Warning storms (notably H.264 FU-A loss) are useful once,
                // not hundreds of times per access unit.
                if self.warnings.len() > 32 {
                    self.warnings.clear();
                }
                let due = self.warnings.get(message).is_none_or(|previous| {
                    now.saturating_duration_since(*previous) >= REPEAT_WARNING_EVERY
                });
                if due {
                    self.warnings.insert(message.to_owned(), now);
                }
                due
            }
            _ => true,
        }
    }
}

fn sample_due(previous: &mut Option<Instant>, now: Instant) -> bool {
    if previous.is_some_and(|previous| now.saturating_duration_since(previous) < SAMPLE_EVERY) {
        return false;
    }
    *previous = Some(now);
    true
}

fn compact_value(value: &str, max_chars: usize) -> String {
    let clean: String = value
        .chars()
        .filter(|character| *character != '\n' && *character != '\r')
        .take(max_chars)
        .collect();
    if clean.chars().any(char::is_whitespace) {
        format!("{clean:?}")
    } else {
        clean
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    #[test]
    fn bundled_student_logs_do_not_modify_signed_resources() {
        let executable =
            Path::new("/Applications/Classroom Student.app/Contents/MacOS/ClassroomStudent");
        let path = default_path(executable, "student-47810").unwrap();
        assert!(path.ends_with("Library/Logs/Classroom/student-47810/log.txt"));
        assert!(!path.starts_with("/Applications/Classroom Student.app"));
    }
    #[test]
    fn archive_retains_latest_five_complete_logs() {
        let directory =
            std::env::temp_dir().join(format!("classroom-archive-{}", media_core::unix_us()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("log.txt");
        for run in 0..8 {
            fs::write(&path, format!("run={run}")).unwrap();
            archive_previous_log(&path).unwrap();
        }
        let archive = directory.join("archive");
        let entries: Vec<_> = fs::read_dir(archive).unwrap().map(Result::unwrap).collect();
        assert_eq!(entries.len(), 5);
        let contents: Vec<_> = entries
            .iter()
            .map(|entry| fs::read_to_string(entry.path()).unwrap())
            .collect();
        for run in 3..8 {
            assert!(contents.contains(&format!("run={run}")));
        }
        fs::remove_dir_all(directory).unwrap();
    }

    use super::*;

    #[test]
    fn default_logs_are_created_beside_the_executable() {
        let path = default_path(
            Path::new("/shared/Downloads/Classroom.exe"),
            "student-47810",
        )
        .unwrap();
        let expected = PathBuf::from("/shared/Downloads")
            .join("logs")
            .join("student-47810")
            .join("log.txt");
        assert_eq!(path, expected);
    }

    #[test]
    fn default_logs_fall_back_to_executable_directory() {
        let path = default_path(Path::new("/opt/classroom/Classroom"), "student-47810").unwrap();
        assert_eq!(
            path,
            PathBuf::from("/opt/classroom/logs/student-47810/log.txt")
        );
    }

    #[test]
    fn previous_compact_log_is_removed_before_new_run() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("classroom-log-test-{unique}"));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("log.txt");
        fs::write(&path, "old run").unwrap();

        remove_previous_log(&path).unwrap();

        assert!(!path.exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn writer_flushes_and_archives_previous_runs() {
        let directory = std::env::temp_dir().join(format!(
            "classroom-rich-log-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("log.txt");
        fs::write(&path, "previous run").unwrap();
        archive_previous_log(&path).unwrap();
        assert!(!path.exists());
        assert_eq!(fs::read_dir(directory.join("archive")).unwrap().count(), 1);
        let (layer, guard) = CompactLog::new(File::create(&path).unwrap(), path.clone());
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(future_metric = 123, "new pipeline metric");
        });
        drop(guard);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("future_metric=123"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn only_sparse_metrics_and_important_events_are_selected() {
        assert_eq!(
            classify("teacher M3 media interval", Level::INFO),
            Kind::TeacherSample
        );
        assert_eq!(
            classify("student M3 media complete", Level::INFO),
            Kind::StudentFinal
        );
        assert_eq!(
            classify("student receiver report", Level::INFO),
            Kind::Event
        );
        assert_eq!(
            classify("student forced presentation window ready", Level::INFO),
            Kind::Event
        );
        assert_eq!(
            classify(
                "teacher control connection closed; closing Student preview",
                Level::INFO
            ),
            Kind::Event
        );
        assert_eq!(
            classify(
                "student preview closed after Teacher disconnect",
                Level::INFO
            ),
            Kind::Event
        );
        assert_eq!(
            classify("teacher handled receiver NACK", Level::INFO),
            Kind::Warning
        );
        assert_eq!(
            classify("student depacketizer lost an access unit", Level::WARN),
            Kind::Warning
        );
        let now = Instant::now();
        let mut previous = None;
        assert!(sample_due(&mut previous, now));
        assert!(!sample_due(&mut previous, now + Duration::from_millis(500)));
        assert!(sample_due(&mut previous, now + Duration::from_secs(5)));
        assert_eq!(compact_value("one\ntwo", 96), "onetwo");
    }
}
