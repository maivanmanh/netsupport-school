//! Same-host benchmark orchestration. Child processes use the real media path.
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    net::{Ipv4Addr, TcpListener, UdpSocket},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{bail, ensure, Context, Result};
use clap::Args;
use classroom_config::Config;
use tracing::info;

#[derive(Debug, Args)]
pub struct BenchmarkArgs {
    #[arg(long, value_parser = clap::value_parser!(u16).range(1..=40),
        requires = "benchmark_output",
        conflicts_with_all = ["role", "headless", "share_seconds", "teacher_ip", "bind_ip"],
        help = "Run one Teacher and N headless Students locally, save logs and CSV reports")]
    pub benchmark_students: Option<u16>,
    #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u64).range(5..=3600),
        requires = "benchmark_students", help = "Duration of each benchmark Student in seconds")]
    pub benchmark_seconds: u64,
    #[arg(
        long,
        requires = "benchmark_students",
        help = "Parent directory for a new benchmark run directory"
    )]
    pub benchmark_output: Option<PathBuf>,
    #[arg(long, requires = "expected_students", conflicts_with_all = ["role", "headless", "share_seconds"],
        help = "Generate CSV reports from a directory of complete, unmodified M5 logs")]
    pub benchmark_report: Option<PathBuf>,
    #[arg(long, requires = "benchmark_report", value_parser = clap::value_parser!(u16).range(1..=40))]
    pub expected_students: Option<u16>,
}

/// Own only children created by this invocation. Cancellation or an error drops
/// the guard, terminates those children and reaps them; no process-name kills.
struct Children(Vec<Child>);

impl Drop for Children {
    fn drop(&mut self) {
        // Send termination to all children first; then reap each one.
        for child in &mut self.0 {
            if !matches!(child.try_wait(), Ok(Some(_))) {
                let _ = child.kill();
            }
        }
        for child in &mut self.0 {
            let _ = child.wait();
        }
    }
}

pub async fn run(
    args: &BenchmarkArgs,
    students: u16,
    config: &Config,
    display_index: u32,
    drop_every_nth_rtp_packet: u32,
) -> Result<()> {
    let ports = media_ports(config.network.media_port, students)?;
    preflight_ports(config.network.control_port, &ports)?;
    let directory = new_run_directory(
        args.benchmark_output
            .as_deref()
            .context("missing output directory")?,
    )?;
    info!(path = %directory.display(), students, "starting same-host benchmark; logs retained on failure");
    let executable = std::env::current_exe()?.canonicalize()?;
    let mut snapshot = config.clone();
    "127.0.0.1".clone_into(&mut snapshot.network.bind_ip);
    snapshot.network.teacher_ip = Ipv4Addr::LOCALHOST;
    "explicit".clone_into(&mut snapshot.network.teacher_ip_mode);
    "info".clone_into(&mut snapshot.app.log_level);
    let config_path = directory.join("config.toml");
    fs::write(&config_path, toml::to_string_pretty(&snapshot)?)?;
    fs::write(directory.join("run.txt"), format!(
        "mode=same-host-headless\nos={}\narchitecture={}\nversion={}\nexecutable={}\nstudents={students}\nstudent_seconds={}\ndisplay_index={display_index}\ndrop_every_nth_rtp_packet={drop_every_nth_rtp_packet}\nThis run does not validate separate-PC LAN scaling, visible rendering or end-to-end latency.\n",
        std::env::consts::OS, std::env::consts::ARCH, env!("CARGO_PKG_VERSION"),
        executable.display(), args.benchmark_seconds,
    ))?;

    let mut children = Children(Vec::new());
    let teacher_log = directory.join("teacher.log");
    let mut teacher = base_command(&executable, &config_path);
    teacher.args([
        "--role",
        "teacher",
        "--share-seconds",
        &(args.benchmark_seconds + 5).to_string(),
        "--display-index",
        &display_index.to_string(),
        "--drop-every-nth-rtp-packet",
        &drop_every_nth_rtp_packet.to_string(),
    ]);
    children.0.push(spawn_logged(&mut teacher, &teacher_log)?);
    wait_for_teacher(&mut children.0[0], &teacher_log)
        .await
        .with_context(|| {
            format!(
                "Teacher did not become ready; see {}",
                teacher_log.display()
            )
        })?;

    for (index, port) in ports.iter().enumerate() {
        let mut student = base_command(&executable, &config_path);
        student.args([
            "--role",
            "student",
            "--headless",
            "--share-seconds",
            &args.benchmark_seconds.to_string(),
            "--media-port",
            &port.to_string(),
            "--feedback-port",
            &(port + 1).to_string(),
        ]);
        children.0.push(spawn_logged(
            &mut student,
            &directory.join(format!("student-{:02}.log", index + 1)),
        )?);
    }
    info!(
        students,
        seconds = args.benchmark_seconds,
        "benchmark Students launched; move or scroll on the captured display"
    );
    let deadline = Instant::now() + Duration::from_secs(args.benchmark_seconds + 20);
    loop {
        let mut finished = 0;
        for (index, child) in children.0.iter_mut().enumerate() {
            if let Some(status) = child.try_wait()? {
                ensure!(
                    status.success(),
                    "benchmark process {index} exited with {status}; logs: {}",
                    directory.display()
                );
                finished += 1;
            }
        }
        if finished == children.0.len() {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "benchmark timed out; logs: {}",
            directory.display()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    crate::benchmark_report::generate(&directory, students)?;
    info!(path = %directory.display(), "benchmark finished; inspect reports for receiver health");
    Ok(())
}

fn base_command(executable: &Path, config: &Path) -> Command {
    let mut command = Command::new(executable);
    command
        .arg("--config")
        .arg(config)
        .args(["--loopback", "--log-level", "info"]);
    command.stdin(Stdio::null());
    command
}

fn spawn_logged(command: &mut Command, path: &Path) -> Result<Child> {
    let log = OpenOptions::new().write(true).create_new(true).open(path)?;
    command.stdout(log.try_clone()?).stderr(log);
    command
        .spawn()
        .with_context(|| format!("failed to spawn child for {}", path.display()))
}

async fn wait_for_teacher(child: &mut Child, path: &Path) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut reader = File::open(path)?;
    let mut tail = String::new();
    loop {
        if let Some(status) = child.try_wait()? {
            bail!("Teacher exited during startup: {status}");
        }
        let mut chunk = Vec::new();
        reader.by_ref().take(65536).read_to_end(&mut chunk)?;
        tail.push_str(&String::from_utf8_lossy(&chunk));
        if tail.contains("teacher M3 media loop ready") {
            return Ok(());
        }
        // Retain only the tail for a marker split across reads. Startup logs are
        // ASCII here; lossy conversion keeps the boundary safe for UTF-8 paths.
        if tail.len() > 4096 {
            tail = String::from_utf8_lossy(&tail.as_bytes()[tail.len() - 4096..]).into_owned();
        }
        ensure!(
            Instant::now() < deadline,
            "Teacher startup exceeded 15 seconds"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn media_ports(first: u16, students: u16) -> Result<Vec<u16>> {
    ensure!(
        (1..=40).contains(&students),
        "Student count must be in 1..=40"
    );
    ensure!(first > 0, "media port must not be zero");
    first
        .checked_add(students * 2 - 1)
        .context("media/feedback port range exceeds 65535")?;
    Ok((0..students).map(|index| first + index * 2).collect())
}

fn preflight_ports(control: u16, media: &[u16]) -> Result<()> {
    let _listener = TcpListener::bind((Ipv4Addr::LOCALHOST, control))
        .with_context(|| format!("control port {control} is occupied; stop the existing Teacher or select --control-port"))?;
    let mut reservations = Vec::new();
    for port in media {
        reservations.push(
            UdpSocket::bind((Ipv4Addr::UNSPECIFIED, *port)).with_context(|| {
                format!("media port {port} is occupied; select a different --media-port base")
            })?,
        );
    }
    // Release before children bind. A concurrent binder can still win the race;
    // child exit checking and complete-log validation detect that failure.
    Ok(())
}

fn new_run_directory(parent: &Path) -> Result<PathBuf> {
    fs::create_dir_all(parent)?;
    let parent = parent.canonicalize()?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let path = parent.join(format!("run-{stamp}-{}", std::process::id()));
    fs::create_dir(&path).context("cannot create a fresh benchmark directory")?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_range_keeps_media_and_feedback_distinct_and_rejects_overflow() {
        assert_eq!(media_ports(47810, 3).unwrap(), vec![47810, 47812, 47814]);
        assert!(media_ports(65530, 5).is_err());
        assert!(media_ports(47810, 0).is_err());
        assert!(media_ports(47810, 41).is_err());
    }
}
