//! One on-demand reverse view per Teacher; independent of presentation scope.
use super::monitoring::ObserveJobs;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    io::{BufRead, Read, Seek, SeekFrom, Write},
    path::Path,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use tracing::{info, warn};
struct Status {
    value: Value,
    lease: Instant,
}
pub struct View {
    pub id: String,
    status: Arc<Mutex<Status>>,
    stop: Arc<AtomicBool>,
    input: Arc<Mutex<VecDeque<Value>>>,
    worker: Option<thread::JoinHandle<()>>,
}
impl View {
    pub fn start(id: String, ip: String, parent: &Path, jobs: ObserveJobs) -> Result<Self> {
        let surface = parent.join(format!("observe-frame-{}.bin", media_core::unix_us()));
        let executable = std::env::current_exe()?;
        let status = Arc::new(Mutex::new(Status {
            value: json!({"id":id,"phase":"starting","mode":"observe","surface":surface}),
            lease: Instant::now(),
        }));
        let state = Arc::clone(&status);
        let stop = Arc::new(AtomicBool::new(false));
        let cancel = Arc::clone(&stop);
        let input = Arc::new(Mutex::new(VecDeque::new()));
        let worker_input = Arc::clone(&input);
        let identity = id.clone();
        jobs.request(id.clone());
        let worker = thread::spawn(move || {
            let result = run(
                &identity,
                &ip,
                &surface,
                &executable,
                &jobs,
                &state,
                &cancel,
                &worker_input,
            );
            jobs.entries.lock().unwrap().remove(&identity);
            let mut state = state.lock().unwrap();
            state.value["phase"] = json!(if result.is_ok() { "ended" } else { "error" });
            if let Err(e) = result {
                warn!(error=%e,"Observe session failed");
                state.value["error"] = json!(format!("{e:#}"));
            }
            let _ = std::fs::remove_file(&surface);
            info!(student_id=%identity,"Observe view ended");
        });
        Ok(Self {
            id,
            status,
            stop,
            input,
            worker: Some(worker),
        })
    }
    pub fn state(&self) -> Value {
        self.status.lock().unwrap().value.clone()
    }
    pub fn cursor_serial(&self) -> Option<u64> {
        self.status.lock().unwrap().value["cursor"]["serial"].as_u64()
    }
    pub fn is_live(&self) -> bool {
        self.status.lock().unwrap().value["phase"] == "live"
    }
    pub fn send(&self, v: Value) -> Result<()> {
        if v["action"] == "keepalive" {
            self.status.lock().unwrap().lease = Instant::now();
        }
        let mut input = self.input.lock().unwrap();
        enqueue_input(&mut input, v)
    }
}
fn enqueue_input(input: &mut VecDeque<Value>, v: Value) -> Result<()> {
    if v["action"] == "move" && input.back().is_some_and(|last| last["action"] == "move") {
        input.pop_back();
    }
    anyhow::ensure!(input.len() < 64, "Observe input queue unavailable");
    input.push_back(v);
    Ok(())
}

/// Read only the bounded log tail after the receiver has flushed and exited.
fn receiver_failure(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(size.saturating_sub(8192))).ok()?;
    let mut bytes = Vec::with_capacity(8192);
    file.take(8192).read_to_end(&mut bytes).ok()?;
    String::from_utf8_lossy(&bytes)
        .lines()
        .rev()
        .find(|line| line.contains("ERROR Classroom terminated with error"))
        .and_then(|line| line.split_once(" error="))
        .map(|(_, detail)| detail.chars().take(512).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receiver_exit_uses_only_error_from_bounded_log_tail() {
        let path = std::env::temp_dir().join(format!(
            "classroom-receiver-error-{}.log",
            std::process::id()
        ));
        std::fs::write(&path,format!("{}\nunix_ms=1 ERROR Classroom terminated with error error=\"No route to host (os error 65)\"\n", "info\n".repeat(3000))).unwrap();
        assert!(receiver_failure(&path)
            .unwrap()
            .contains("No route to host"));
        std::fs::write(&path, "INFO receiver ready\n").unwrap();
        assert!(receiver_failure(&path).is_none());
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn motion_is_coalesced_without_crossing_click_barriers() {
        let mut events = VecDeque::new();
        enqueue_input(&mut events, json!({"action":"move","x":0.1})).unwrap();
        enqueue_input(&mut events, json!({"action":"move","x":0.2})).unwrap();
        enqueue_input(
            &mut events,
            json!({"action":"button_at","x":0.2,"down":true}),
        )
        .unwrap();
        enqueue_input(&mut events, json!({"action":"move","x":0.3})).unwrap();
        enqueue_input(&mut events, json!({"action":"move","x":0.4})).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0]["x"], json!(0.2));
        assert_eq!(events[1]["action"], "button_at");
        assert_eq!(events[2]["x"], json!(0.4));
    }
}
impl Drop for View {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}
struct Child(std::process::Child);
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[allow(clippy::too_many_arguments)]
fn run(
    id: &str,
    ip: &str,
    surface: &Path,
    exe: &Path,
    jobs: &ObserveJobs,
    status: &Arc<Mutex<Status>>,
    stop: &AtomicBool,
    events: &Mutex<VecDeque<Value>>,
) -> Result<()> {
    let began = Instant::now();
    let job = loop {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        anyhow::ensure!(
            began.elapsed() < Duration::from_secs(12),
            "Student did not answer Observe request"
        );
        let job = jobs
            .entries
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .context("Observe request canceled")?;
        anyhow::ensure!(job.error.is_empty(), "{}", job.error);
        if job.port != 0 && !job.token.is_empty() {
            break job;
        }
        thread::sleep(Duration::from_millis(25));
    };
    info!(
        student_id = id,
        source_ready_wait_ms = began.elapsed().as_secs_f64() * 1000.0,
        "Observe source ready; starting receiver"
    );
    let mut child = Child(
        Command::new(exe)
            .args([
                "--observe-connect",
                &format!("{ip}:{}", job.port),
                "--observe-token",
                &job.token,
                "--observe-surface",
                surface.to_str().context("Invalid surface path")?,
                "--log-file",
                surface
                    .with_extension("log.txt")
                    .to_str()
                    .context("Invalid log path")?,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let mut input = child.0.stdin.take().context("Observe input pipe missing")?;
    let output = child
        .0
        .stdout
        .take()
        .context("Observe status pipe missing")?;
    let state = Arc::clone(status);
    let reader = thread::spawn(move || {
        for line in std::io::BufReader::new(output).lines() {
            let Ok(line) = line else { break };
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                let mut s = state.lock().unwrap();
                if v["mode"].is_string() {
                    s.value["mode"] = v["mode"].clone();
                    s.value["error"] = v["error"].clone();
                }
                if v["cursor"].is_object() {
                    s.value["cursor"] = v["cursor"].clone();
                }
                if v["metrics"].is_object() {
                    s.value["metrics"] = v["metrics"].clone();
                }
            }
        }
    });
    status.lock().unwrap().value["phase"] = json!("live");
    while !stop.load(Ordering::Acquire) {
        if status.lock().unwrap().lease.elapsed() > Duration::from_secs(5) {
            break;
        }
        if let Some(exit) = child.0.try_wait()? {
            if !exit.success() {
                let detail = receiver_failure(&surface.with_extension("log.txt"));
                anyhow::bail!(
                    "Observe receiver exited: {exit}{}",
                    detail.map_or(String::new(), |message| format!(" · {message}"))
                );
            }
            break;
        }
        for _ in 0..64 {
            let Some(v) = events.lock().unwrap().pop_front() else {
                break;
            };
            let bytes = serde_json::to_vec(&v)?;
            anyhow::ensure!(bytes.len() < 1024, "Observe input too large");
            input.write_all(&bytes)?;
            input.write_all(b"\n")?;
        }
        thread::sleep(Duration::from_millis(5));
    }
    let _ = input.write_all(b"{\"action\":\"close\"}\n");
    drop(input);
    drop(child);
    let _ = reader.join();
    Ok(())
}
