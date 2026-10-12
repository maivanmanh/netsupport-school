#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
use std::{
    collections::{HashMap, HashSet, VecDeque},
    io::{self, IsTerminal, Write},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
    path::PathBuf,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use app_core::Role;
use clap::Parser;
use classroom_config::{Config, Overrides};
use control_transport::{
    register_student_for_presentation, run_teacher_media_control, StudentRegistration,
    TeacherMediaEvent,
};
use media_core::{
    CaptureConfig, CaptureSource, DecodedFrame, DecodedFrameRef, EncodedAccessUnitRef, H264Config,
    Renderer, VideoDecoder, VideoEncoder, VideoFrame, VideoFrameRef,
};
use media_transport::{
    DepacketizedAccessUnit, H264Depacketizer, H264Packetizer, JitterBuffer, RtpPacket,
    H264_PAYLOAD_TYPE, RTP_VIDEO_CLOCK_HZ,
};
use platform::{infer_teacher_address, select_adapter, AdapterCandidate, SelectedAdapter};
use socket2::{Domain, Protocol, Socket, Type};
use tracing::{info, warn};

mod activity;
mod benchmark;
mod benchmark_report;
mod classroom_tools;
mod clipboard_guard;
mod compact_log;
mod dashboard_view;
mod decode_health;
mod desktop_tray;
mod dev_update;
mod diagnostics;
mod encode_pacer;
mod fanout;
mod guard;
mod guard_service;
mod hardware_control;
mod keyframe_gate;
mod launcher;
mod macos_permissions;
mod media_activity;
mod media_session;
mod monitoring;
mod observation;
mod observe_coordinator;
mod pointer_channel;
mod presentation_ids;
mod presentation_pacer;
mod process_metrics;
mod quality;
mod refinement;
mod remote_input;
mod remote_terminal;
mod startup;
mod stream_profile;
mod student_bootstrap;
mod student_broadcast;
mod student_connection_ui;
mod student_file_ui;
mod student_fps_badge;
mod monitoring_badge;
mod student_lock;
mod student_presentation;
mod student_settings;
mod student_tools_ui;
mod teacher_agent;
mod teacher_dashboard;
mod text_editor;
mod teacher_repair;
mod udp_ingress;
mod udp_sender;
mod vscode_control;
mod app_control;
mod class_rules;
mod local_day;
use process_metrics::ProcessMetrics;
use quality::{QualityController, ReceiverMediaReport};
use student_presentation::StudentPresentation;

const MEDIA_SOCKET_RECEIVE_BUFFER_BYTES: usize = 4 * 1024 * 1024;
const MEDIA_SOCKET_SEND_BUFFER_BYTES: usize = 256 * 1024;
const MEDIA_ROUTE_PROBE_MAGIC: [u8; 4] = *b"CLUP";
const MEDIA_ROUTE_PROBE_BYTES: usize = 12;
const MEDIA_ROUTE_PROBE_INTERVAL: Duration = Duration::from_secs(1);
const MAX_MEDIA_ROUTE_PROBES_PER_TICK: usize = 64;

#[cfg(target_os = "macos")]
use capture_macos_sck::ScreenCaptureKitCapture as PlatformCapture;
#[cfg(windows)]
use capture_windows_dxgi::DxgiCapture as PlatformCapture;
#[cfg(target_os = "macos")]
use codec_macos_vt::{
    VideoToolboxH264Decoder as PlatformH264Decoder, VideoToolboxH264Encoder as PlatformH264Encoder,
};
#[cfg(windows)]
use codec_windows_mf::{
    MediaFoundationRuntime, MfH264Decoder as PlatformH264Decoder,
    MfH264Encoder as PlatformH264Encoder,
};
#[cfg(target_os = "macos")]
use renderer_macos::NativeRenderer as PlatformRenderer;
#[cfg(windows)]
use renderer_windows::NativeRenderer as PlatformRenderer;

#[cfg(not(any(windows, target_os = "macos")))]
use capture_windows_dxgi::DxgiCapture as PlatformCapture;
#[cfg(not(any(windows, target_os = "macos")))]
use codec_windows_mf::{
    MediaFoundationRuntime, MfH264Decoder as PlatformH264Decoder,
    MfH264Encoder as PlatformH264Encoder,
};
#[cfg(not(any(windows, target_os = "macos")))]
use renderer_windows::NativeRenderer as PlatformRenderer;

#[derive(Debug, Parser)]
#[command(name = "Classroom", version, about = "Native classroom screen sharing")]
#[command(group(clap::ArgGroup::new("operation").args([
    "benchmark_students", "benchmark_report", "diagnose",
    "capture_benchmark_seconds", "local_media_benchmark_seconds"
]).multiple(false)))]
#[allow(clippy::struct_excessive_bools)]
struct Cli {
    #[command(flatten)]
    benchmark: benchmark::BenchmarkArgs,
    #[arg(long, value_enum)]
    role: Option<CliRole>,
    #[arg(long)]
    observe_connect: Option<SocketAddrV4>,
    #[arg(long, requires = "observe_connect")]
    observe_token: Option<String>,
    #[arg(long, requires = "observe_connect")]
    observe_surface: Option<PathBuf>,
    /// Open the native Teacher dashboard instead of immediately sharing.
    #[arg(long)]
    teacher_ui: bool,
    /// Run the background Teacher coordinator for the native Flutter console.
    #[arg(long, requires = "ipc_file", conflicts_with = "teacher_ui")]
    teacher_agent: bool,
    /// Private Flutter/Rust IPC endpoint descriptor (never a classroom address).
    #[arg(long, requires = "teacher_agent")]
    ipc_file: Option<PathBuf>,
    /// Native console executable to reopen from the background tray.
    #[arg(long, requires = "teacher_agent")]
    console_path: Option<PathBuf>,
    /// Reopen first-run role/IP setup, ignoring remembered settings.
    #[arg(long)]
    setup: bool,
    /// Save the explicit CLI role and Teacher IP for subsequent double-clicks.
    #[arg(long)]
    remember: bool,
    /// Isolated per-user launch profile override (also useful for automation).
    #[arg(long)]
    launch_profile: Option<PathBuf>,
    /// Diagnostic: run repeated Start/Stop cycles through the Teacher dashboard.
    #[arg(long, requires = "teacher_ui", value_parser = clap::value_parser!(u32).range(1..=10))]
    teacher_test_cycles: Option<u32>,
    /// Diagnostic barrier: wait for this many connected Students before cycling.
    #[arg(long, requires = "teacher_test_cycles", default_value_t = 0, value_parser = clap::value_parser!(u32).range(0..=128))]
    teacher_test_min_students: u32,
    /// Diagnostic barrier for actual received monitoring images.
    #[arg(long, requires = "teacher_test_cycles", default_value_t = 0, value_parser = clap::value_parser!(u32).range(0..=128))]
    teacher_test_min_thumbnails: u32,
    /// Diagnostic: exercise Grid/List/inspection and export console pixels.
    #[arg(long, requires = "teacher_test_cycles")]
    teacher_test_output: Option<PathBuf>,
    /// Diagnostic: second presentation targets the selected Student only.
    #[arg(long, requires = "teacher_test_cycles")]
    teacher_test_selective: bool,
    /// Exercise close-to-tray/reopen and desktop visual states while sharing.
    #[arg(long, requires = "teacher_test_cycles")]
    teacher_test_desktop: bool,
    /// Isolated diagnostic: save Student settings, restart, and log replacement PID.
    #[arg(long, requires_all = ["teacher_ui", "launch_profile"])]
    teacher_test_settings_restart: bool,
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    log_level: Option<String>,
    #[arg(
        long,
        value_name = "PATH",
        help = "Override the compact per-run log.txt path"
    )]
    log_file: Option<PathBuf>,
    #[arg(long)]
    teacher_ip: Option<Ipv4Addr>,
    #[arg(long)]
    bind_ip: Option<String>,
    #[arg(long = "interface")]
    interface_name: Option<String>,
    #[arg(long)]
    control_port: Option<u16>,
    #[arg(long)]
    media_port: Option<u16>,
    #[arg(long)]
    feedback_port: Option<u16>,
    #[arg(long, help = "Validate configuration and adapter selection, then exit")]
    diagnose: bool,
    #[arg(
        long,
        value_name = "SECONDS",
        help = "Run the native screen-capture benchmark, then exit"
    )]
    capture_benchmark_seconds: Option<u64>,
    #[arg(
        long,
        value_name = "SECONDS",
        help = "Run the native local capture/H.264/render benchmark, then exit"
    )]
    local_media_benchmark_seconds: Option<u64>,
    #[arg(
        long,
        value_name = "SECONDS",
        help = "Run teacher or student M3 media for this duration; omit to run until interrupted"
    )]
    share_seconds: Option<u64>,
    #[arg(long, default_value_t = 0, value_parser = clap::value_parser!(u64).range(0..=60_000),
        help = "Test only: delay Student registration after UDP initialization")]
    student_connect_delay_ms: u64,
    #[arg(
        long,
        default_value_t = 0,
        help = "Test only: Teacher drops every Nth original RTP packet (0 disables)"
    )]
    drop_every_nth_rtp_packet: u32,
    #[arg(long, default_value_t = 0, help = "Desktop output index for capture")]
    display_index: u32,
    #[arg(
        long,
        help = "Explicitly use the default 127.0.0.1 same-machine test mode"
    )]
    loopback: bool,
    #[arg(
        long,
        help = "Student only: decode and report health without opening a preview window"
    )]
    headless: bool,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum CliRole {
    Teacher,
    Student,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StudentSessionOutcome {
    Completed,
    Reconnect,
    PresentationStopped,
}

impl From<CliRole> for Role {
    fn from(value: CliRole) -> Self {
        match value {
            CliRole::Teacher => Self::Teacher,
            CliRole::Student => Self::Student,
        }
    }
}

enum RestartRequest {
    Student(std::path::PathBuf),
    CurrentArguments,
}

fn main() -> Result<()> {
    #[cfg(windows)]
    if let Some(result) = classroom_tools::recording::early_operation() {
        return result;
    }
    #[cfg(windows)]
    if let Some(result) = classroom_tools::screenshot::early_operation() {
        return result;
    }
    if std::env::args_os().nth(1).is_some_and(|v| v == "--hardware-reset") {
        return hardware_control::emergency_reset();
    }
    // Started by Windows in place of a blocked program (see `app_control`).
    if let Some(code) = app_control::early_operation() {
        std::process::exit(code);
    }
    if let Some(code) = remote_terminal::compat::early_operation() {
        std::process::exit(code);
    }
    if let Some(result) = macos_permissions::early_operation() {
        return result;
    }
    if env!("CARGO_BIN_NAME") == "ClassroomGuardService" {
        return guard_service::main();
    }
    if let Some(result) = dev_update::early_operation() {
        return result;
    }
    let runtime = tokio::runtime::Runtime::new()?;
    let mut log_guard = None;
    let mut restart_profile = None;
    let result = runtime.block_on(run(&mut log_guard, &mut restart_profile));
    if let Err(error) = &result {
        tracing::error!(error = %format!("{error:#}"), "Classroom terminated with error");
    }
    drop(log_guard);
    // Close sockets/tasks and join workers before replacing the executable.
    drop(runtime);
    if result.is_ok() {
        if let Some(request) = restart_profile {
            match request {
                RestartRequest::Student(profile) => startup::restart(&profile)?,
                RestartRequest::CurrentArguments => startup::restart_current()?,
            }
        }
    }
    result
}

#[allow(clippy::too_many_lines)]
async fn run(
    log_guard: &mut Option<compact_log::LogGuard>,
    restart_profile: &mut Option<RestartRequest>,
) -> Result<()> {
    if std::env::args_os().nth(1).is_some_and(|v| v == "--terminal-worker") {
        let path = std::env::args_os().nth(2).context("Terminal worker context missing")?;
        return remote_terminal::worker::run(std::path::Path::new(&path)).await;
    }
    let mut cli = Cli::parse();
    let student_only = env!("CARGO_BIN_NAME") == "ClassroomStudent";
    if student_only {
        anyhow::ensure!(
            !matches!(cli.role, Some(CliRole::Teacher))
                && !cli.teacher_ui
                && !cli.teacher_agent
                && !cli.setup,
            "ClassroomStudent is a background Student agent; use the Flutter console for Teacher"
        );
        cli.role = Some(CliRole::Student);
    }
    if let Some(directory) = &cli.benchmark.benchmark_report {
        let path = compact_log::path_for("report", cli.log_file.as_deref())?;
        *log_guard = Some(compact_log::init("info", &path)?);
        benchmark_report::generate(directory, cli.benchmark.expected_students.unwrap_or(1))?;
        info!(path = %directory.display(), "benchmark report generated");
        return Ok(());
    }
    if let Some(address) = cli.observe_connect {
        let (config, _) = Config::load(cli.config.as_deref())?;
        let path = compact_log::path_for("observe", cli.log_file.as_deref())?;
        *log_guard = Some(compact_log::init("info", &path)?);
        return observation::receive(
            &config,
            address,
            cli.observe_token
                .as_deref()
                .context("Observe token missing")?,
            cli.observe_surface
                .as_deref()
                .context("Observe surface missing")?,
        )
        .await;
    }
    // A downloaded portable Student uses its adjacent room configuration, not
    // another room remembered by this user. Explicit CLI/config still wins.
    let bootstrap = if student_only && cli.config.is_none() {
        student_bootstrap::Bootstrap::load(
            std::env::current_exe()?
                .parent()
                .context("Student directory missing")?,
        )?
    } else {
        None
    };
    let profile_path = if cli.launch_profile.is_none() {
        if let Some(bootstrap) = &bootstrap {
            let base = startup::profile_path(None)?;
            base.parent()
                .context("Profile directory missing")?
                .join(format!(
                    "bootstrap-room-{}-{}",
                    bootstrap.teacher_ip, bootstrap.control_port
                ))
                .join("profile.toml")
        } else {
            startup::profile_path(None)?
        }
    } else {
        startup::profile_path(cli.launch_profile.as_deref())?
    };
    if student_only && cli.teacher_ip.is_none() {
        cli.teacher_ip = startup::load(&profile_path).map(|profile| profile.teacher_ip);
    }
    let gui_selection = if should_show_launcher(&cli) {
        let (defaults, _) = Config::load(cli.config.as_deref())?;
        let initial_ip = launcher::load_teacher_ip(defaults.network.teacher_ip);
        let selection = if cli.setup {
            None
        } else {
            startup::load(&profile_path)
        };
        match selection.map_or_else(
            || launcher::show(&defaults.app.role, initial_ip),
            |value| Ok(Some(value)),
        )? {
            Some(selection) => {
                startup::save(&profile_path, selection)?;
                Some(selection)
            }
            None => return Ok(()),
        }
    } else {
        None
    };
    let (mut config, config_path) = Config::load(cli.config.as_deref())?;
    if let Some(bootstrap) = &bootstrap {
        config.network.teacher_ip = bootstrap.teacher_ip;
        config.network.control_port = bootstrap.control_port;
        config.tools.connection_popup = bootstrap.connection_popup;
    }
    if student_only || matches!(cli.role, Some(CliRole::Student)) {
        student_settings::apply_saved(&mut config, &profile_path)?;
    }
    let selected_role = cli
        .role
        .map(|role| Role::from(role).to_string())
        .or_else(|| gui_selection.map(|selection| selection.role.to_string()));
    let bind_ip = cli.bind_ip.clone().or_else(|| {
        gui_selection
            .filter(|selection| selection.role == Role::Teacher)
            .map(|_| "auto".to_owned())
    });
    config.apply_overrides(Overrides {
        role: selected_role,
        log_level: cli.log_level,
        teacher_ip: cli
            .teacher_ip
            .or_else(|| gui_selection.map(|s| s.teacher_ip)),
        bind_ip,
        interface: cli.interface_name,
        control_port: cli.control_port,
        media_port: cli.media_port,
        feedback_port: cli.feedback_port,
    })?;
    // Acquire before initializing the compact log: a duplicate launch must
    // neither steal ports nor truncate the active agent's diagnostic log.
    let _student_instance = if config.app.role == "student" {
        let Some(instance) = startup::student_instance(config.network.media_port)? else {
            return Ok(());
        };
        Some(instance)
    } else {
        None
    };
    if cli.remember {
        startup::save(
            &profile_path,
            launcher::LaunchSelection {
                role: config.app.role.parse()?,
                teacher_ip: config.network.teacher_ip,
            },
        )?;
    }
    let log_identity = if cli.benchmark.benchmark_students.is_some() {
        "benchmark".to_owned()
    } else if cli.capture_benchmark_seconds.is_some() {
        "capture".to_owned()
    } else if cli.local_media_benchmark_seconds.is_some() {
        "local-media".to_owned()
    } else {
        let role = cli
            .role
            .map_or(config.app.role.as_str(), |role| match role {
                CliRole::Teacher => "teacher",
                CliRole::Student => "student",
            });
        match role {
            "teacher" => format!("teacher-{}", config.network.control_port),
            "student" => format!("student-{}", config.network.media_port),
            _ => "session".to_owned(),
        }
    };
    let log_path = compact_log::path_for(&log_identity, cli.log_file.as_deref())?;
    *log_guard = Some(compact_log::init(&config.app.log_level, &log_path)?);
    info!(path = %log_path.display(), "compact session log ready");

    if let Some(students) = cli.benchmark.benchmark_students {
        tokio::select! {
            result = benchmark::run(&cli.benchmark, students, &config, cli.display_index,
                cli.drop_every_nth_rtp_packet) => result?,
            signal = tokio::signal::ctrl_c() => {
                signal.context("failed to listen for benchmark interruption")?;
                bail!("benchmark interrupted; child processes stopped, partial logs retained");
            }
        }
        return Ok(());
    }

    if let Some(seconds) = cli.capture_benchmark_seconds {
        if seconds == 0 {
            bail!("--capture-benchmark-seconds must be greater than zero");
        }
        let video = config.video.clone();
        tokio::task::spawn_blocking(move || {
            run_capture_benchmark(seconds, cli.display_index, &video)
        })
        .await
        .context("capture benchmark worker stopped unexpectedly")??;
        return Ok(());
    }
    if let Some(seconds) = cli.local_media_benchmark_seconds {
        if seconds == 0 {
            bail!("--local-media-benchmark-seconds must be greater than zero");
        }
        let video = config.video.clone();
        #[cfg(target_os = "macos")]
        run_local_media_benchmark(seconds, cli.display_index, &video)?;
        #[cfg(not(target_os = "macos"))]
        tokio::task::spawn_blocking(move || {
            run_local_media_benchmark(seconds, cli.display_index, &video)
        })
        .await
        .context("local media benchmark worker stopped unexpectedly")??;
        return Ok(());
    }

    let adapter_started = Instant::now();
    let adapter = if cli.loopback {
        loopback_adapter()
    } else {
        select_adapter(&config.network.interface).with_context(|| {
            format!(
                "failed to select interface using '{}'",
                config.network.interface
            )
        })?
    };
    log_startup(&adapter, config_path.as_deref());
    info!(
        adapter_discovery_ms = adapter_started.elapsed().as_secs_f64() * 1000.0,
        "network adapter discovery complete"
    );
    info!(version = env!("CARGO_PKG_VERSION"), build_id = env!("CLASSROOM_BUILD_ID"),
        protocol_version = protocol::PROTOCOL_VERSION, telemetry_schema = 3,
        config = ?config, log_interval_seconds = 1, "session diagnostic manifest");

    if cli.diagnose {
        info!(
            control_port = config.network.control_port,
            media_port = config.network.media_port,
            feedback_port = config.network.feedback_port,
            width = config.video.width,
            height = config.video.height,
            max_fps = config.video.max_fps,
            target_bitrate_kbps = config.video.target_bitrate_kbps,
            "configuration diagnostics passed"
        );
        return Ok(());
    }

    let role = choose_role(&config.app.role)?;
    match role {
        Role::Teacher => {
            if cli.student_connect_delay_ms != 0 {
                bail!("--student-connect-delay-ms is only valid with --role student");
            }
            if cli.headless {
                bail!("--headless is only valid with --role student");
            }
            let bind_ip = if cli.loopback {
                Ipv4Addr::LOCALHOST
            } else {
                resolve_bind_ip(&config.network.bind_ip, adapter.address)?
            };
            info!(role = %role, bind_ip = %bind_ip, "starting Classroom");
            if cli.teacher_agent {
                if teacher_agent::run(
                    &config,
                    bind_ip,
                    cli.display_index,
                    cli.ipc_file
                        .as_deref()
                        .context("Teacher agent IPC path missing")?,
                    cli.console_path.as_deref(),
                )? {
                    *restart_profile = Some(RestartRequest::CurrentArguments);
                }
                return Ok(());
            }
            if cli.teacher_ui || gui_selection.is_some() {
                return teacher_dashboard::run(
                    &config,
                    bind_ip,
                    cli.display_index,
                    cli.teacher_test_cycles,
                    cli.teacher_test_min_students,
                    cli.teacher_test_min_thumbnails,
                    cli.teacher_test_output.as_deref(),
                    cli.teacher_test_selective,
                    cli.teacher_test_desktop,
                    cli.teacher_test_settings_restart,
                    &profile_path,
                );
            }
            run_teacher_media(
                &config,
                bind_ip,
                cli.display_index,
                cli.share_seconds,
                cli.drop_every_nth_rtp_packet,
            )?;
        }
        Role::Student => {
            let teacher_ip = if cli.loopback {
                Ipv4Addr::LOCALHOST
            } else {
                infer_teacher_address(
                    adapter.address,
                    config.network.teacher_ip,
                    &config.network.teacher_ip_mode,
                )
            };
            info!(role = %role, %teacher_ip, "starting Classroom");
            let media_endpoint_ip = student_media_endpoint_ip(teacher_ip, adapter.address);
            if run_student_media(
                &config,
                teacher_ip,
                media_endpoint_ip,
                cli.share_seconds,
                cli.headless,
                cli.student_connect_delay_ms,
                &profile_path,
            )
            .await?
            {
                *restart_profile = Some(RestartRequest::Student(profile_path.clone()));
            }
        }
    }
    Ok(())
}

fn should_show_launcher(cli: &Cli) -> bool {
    cli.role.is_none()
        && cli.benchmark.benchmark_students.is_none()
        && cli.benchmark.benchmark_report.is_none()
        && !cli.diagnose
        && cli.capture_benchmark_seconds.is_none()
        && cli.local_media_benchmark_seconds.is_none()
}

#[allow(clippy::cast_precision_loss)]
fn run_capture_benchmark(
    seconds: u64,
    display_index: u32,
    video: &classroom_config::VideoConfig,
) -> Result<()> {
    let mut capture = PlatformCapture::new();
    capture
        .start(CaptureConfig {
            display_index,
            width: video.width,
            height: video.height,
            max_fps: video.max_fps,
        })
        .context("failed to start DXGI Desktop Duplication benchmark")?;

    let benchmark_started = Instant::now();
    let benchmark_duration = Duration::from_secs(seconds);
    let frame_interval = Duration::from_secs_f64(1.0 / f64::from(video.max_fps));
    let mut next_frame_at = benchmark_started;
    let mut report_started = benchmark_started;
    let mut report_frames = 0_u64;
    let mut total_dirty_ratio = 0.0_f64;
    let mut latencies = Vec::new();
    let mut last_width = 0_u32;
    let mut last_height = 0_u32;

    info!(
        seconds,
        display_index,
        requested_width = video.width,
        requested_height = video.height,
        max_fps = video.max_fps,
        "capture benchmark started; move the cursor, type, and scroll during the run"
    );

    while benchmark_started.elapsed() < benchmark_duration {
        let now = Instant::now();
        if now < next_frame_at {
            thread::sleep(next_frame_at - now);
        }
        let deadline = (Instant::now() + Duration::from_millis(100))
            .min(benchmark_started + benchmark_duration);
        if let Some(frame) = capture.next_frame(deadline)? {
            last_width = frame.width;
            last_height = frame.height;
            total_dirty_ratio += f64::from(frame.dirty_region_ratio);
            latencies.push(frame.capture_latency);
            report_frames += 1;
        }
        next_frame_at = Instant::now() + frame_interval;

        if report_started.elapsed() >= Duration::from_secs(1) {
            let statistics = capture.statistics();
            info!(
                capture_fps = report_frames as f64 / report_started.elapsed().as_secs_f64(),
                frames = statistics.frames_acquired,
                no_change_timeouts = statistics.no_change_timeouts,
                accumulated_frames = statistics.accumulated_frames,
                access_loss_recoveries = statistics.access_loss_recoveries,
                pointer_frames = statistics.pointer_frames_composited,
                "capture benchmark interval"
            );
            report_started = Instant::now();
            report_frames = 0;
        }
    }

    let elapsed = benchmark_started.elapsed();
    let statistics = capture.statistics();
    latencies.sort_unstable();
    let p50 = percentile(&latencies, 50);
    let p95 = percentile(&latencies, 95);
    let average_dirty_ratio = if statistics.frames_acquired == 0 {
        0.0
    } else {
        total_dirty_ratio / statistics.frames_acquired as f64
    };
    info!(
        elapsed_seconds = elapsed.as_secs_f64(),
        width = last_width,
        height = last_height,
        frames = statistics.frames_acquired,
        capture_fps = statistics.frames_acquired as f64 / elapsed.as_secs_f64(),
        capture_latency_p50_ms = p50.as_secs_f64() * 1_000.0,
        capture_latency_p95_ms = p95.as_secs_f64() * 1_000.0,
        average_dirty_ratio,
        no_change_timeouts = statistics.no_change_timeouts,
        accumulated_frames = statistics.accumulated_frames,
        access_loss_recoveries = statistics.access_loss_recoveries,
        pointer_updates = statistics.pointer_updates,
        pointer_frames_composited = statistics.pointer_frames_composited,
        protected_content_frames = statistics.protected_content_frames,
        "capture benchmark complete"
    );
    if statistics.frames_acquired == 0 {
        bail!("capture benchmark completed without acquiring a frame");
    }
    Ok(())
}

fn percentile(values: &[Duration], percentile: usize) -> Duration {
    if values.is_empty() {
        return Duration::ZERO;
    }
    let index = (values.len() - 1) * percentile / 100;
    values[index]
}

fn take_interval_percentile(values: &mut Vec<Duration>, percentile_value: usize) -> Duration {
    values.sort_unstable();
    let result = percentile(values, percentile_value);
    values.clear();
    result
}

fn take_interval_percentile_and_max(values: &mut Vec<Duration>) -> (Duration, Duration) {
    values.sort_unstable();
    let p95 = percentile(values, 95);
    let max = values.last().copied().unwrap_or_default();
    values.clear();
    (p95, max)
}

#[allow(clippy::cast_precision_loss, clippy::too_many_lines)]
fn run_local_media_benchmark(
    seconds: u64,
    display_index: u32,
    video: &classroom_config::VideoConfig,
) -> Result<()> {
    #[cfg(not(target_os = "macos"))]
    let runtime = MediaFoundationRuntime::new().context("failed to initialize media runtime")?;
    let mut capture = PlatformCapture::new();
    capture
        .start(CaptureConfig {
            display_index,
            width: video.width,
            height: video.height,
            max_fps: video.max_fps,
        })
        .context("failed to start native capture for local media benchmark")?;
    let (output_width, output_height) = capture
        .output_dimensions()
        .context("capture did not report its output dimensions")?;
    let codec_config = H264Config {
        width: output_width,
        height: output_height,
        frames_per_second: video.max_fps,
        bitrate: video.target_bitrate_kbps.saturating_mul(1_000),
        keyframe_interval_frames: video.max_fps.saturating_mul(video.keyframe_interval_ms) / 1_000,
    };
    #[cfg(not(target_os = "macos"))]
    let mut encoder = PlatformH264Encoder::new(&runtime, codec_config)
        .context("failed to initialize native H.264 encoder")?;
    #[cfg(target_os = "macos")]
    let mut encoder = PlatformH264Encoder::new(codec_config)
        .context("failed to initialize native H.264 encoder")?;
    #[cfg(not(target_os = "macos"))]
    let mut decoder = PlatformH264Decoder::new(&runtime, codec_config)
        .context("failed to initialize native H.264 decoder")?;
    #[cfg(target_os = "macos")]
    let mut decoder = PlatformH264Decoder::new(codec_config)
        .context("failed to initialize native H.264 decoder")?;
    let mut renderer = PlatformRenderer::new(output_width, output_height, "Classroom M2 preview")
        .context("failed to create native preview window")?;
    encoder
        .request_keyframe()
        .context("failed to request initial H.264 keyframe")?;

    let encoder_descriptor = encoder.descriptor();
    let decoder_descriptor = decoder.descriptor();
    info!(
        seconds,
        display_index,
        width = output_width,
        height = output_height,
        target_fps = video.max_fps,
        target_bitrate_kbps = video.target_bitrate_kbps,
        encoder = %encoder_descriptor.name,
        encoder_backend = %encoder_descriptor.kind,
        encoder_async = encoder_descriptor.asynchronous,
        decoder = %decoder_descriptor.name,
        "local native-media benchmark started"
    );

    let benchmark_started = Instant::now();
    let benchmark_ends = benchmark_started + Duration::from_secs(seconds);
    let frame_interval = Duration::from_secs_f64(1.0 / f64::from(video.max_fps));
    let mut next_tick = benchmark_started + frame_interval;
    let mut latest_frame: Option<VideoFrame> = None;
    let mut captured_frames = 0_u64;
    let mut repeated_frames = 0_u64;
    let mut encoded_frames = 0_u64;
    let mut decoded_frames = 0_u64;
    let mut rendered_frames = 0_u64;
    let mut keyframes = 0_u64;
    let mut encoded_bytes = 0_u64;
    let mut schedule_overruns = 0_u64;
    let mut encode_latencies = Vec::new();
    let mut decode_latencies = Vec::new();
    let mut render_latencies = Vec::new();
    let mut pipeline_latencies = Vec::new();
    let mut interval_started = benchmark_started;
    let mut interval_encoded = 0_u64;
    let mut interval_rendered = 0_u64;

    while Instant::now() < benchmark_ends && renderer.is_open() {
        let capture_deadline = next_tick.min(benchmark_ends);
        if let Some(frame) = capture.next_frame(capture_deadline)? {
            latest_frame = Some(frame);
            captured_frames += 1;
        } else if latest_frame.is_some() {
            repeated_frames += 1;
        }
        let now = Instant::now();
        if now < next_tick {
            thread::sleep(next_tick - now);
        }
        let Some(frame) = latest_frame.as_ref() else {
            next_tick = Instant::now() + frame_interval;
            continue;
        };

        let pipeline_started = Instant::now();
        let encode_started = Instant::now();
        let access_unit = encoder.encode(VideoFrameRef {
            bytes: &frame.bytes,
            captured_at: frame.captured_at,
            width: frame.width,
            height: frame.height,
        })?;
        bounded_sample(&mut encode_latencies, encode_started.elapsed());
        if let Some(access_unit) = access_unit {
            encoded_frames += 1;
            interval_encoded += 1;
            let access_unit_bytes = u64::try_from(access_unit.bytes.len()).unwrap_or(u64::MAX);
            encoded_bytes += access_unit_bytes;
            if access_unit.is_keyframe {
                keyframes += 1;
            }
            let decode_started = Instant::now();
            let decoded_output = decoder.decode(EncodedAccessUnitRef {
                bytes: &access_unit.bytes,
                timestamp: access_unit.timestamp,
                is_keyframe: access_unit.is_keyframe,
            })?;
            bounded_sample(&mut decode_latencies, decode_started.elapsed());
            decoded_frames += u64::try_from(decoded_output.len()).unwrap_or(u64::MAX);
            for decoded_frame in decoded_output {
                let render_started = Instant::now();
                renderer.render(DecodedFrameRef {
                    format: decoded_frame.format,
                    bytes: &decoded_frame.bytes,
                    width: decoded_frame.width,
                    height: decoded_frame.height,
                })?;
                render_latencies.push(render_started.elapsed());
                pipeline_latencies.push(pipeline_started.elapsed());
                rendered_frames += 1;
                interval_rendered += 1;
            }
        }

        next_tick += frame_interval;
        if next_tick < Instant::now() {
            schedule_overruns += 1;
            next_tick = Instant::now() + frame_interval;
        }
        if interval_started.elapsed() >= Duration::from_secs(1) {
            let interval_seconds = interval_started.elapsed().as_secs_f64();
            log_stages("teacher", encoder.take_diagnostics());
            info!(
                encode_fps = interval_encoded as f64 / interval_seconds,
                render_fps = interval_rendered as f64 / interval_seconds,
                encoded_frames,
                rendered_frames,
                schedule_overruns,
                "local media benchmark interval"
            );
            interval_started = Instant::now();
            interval_encoded = 0;
            interval_rendered = 0;
        }
    }

    let elapsed = benchmark_started.elapsed();
    encode_latencies.sort_unstable();
    decode_latencies.sort_unstable();
    render_latencies.sort_unstable();
    pipeline_latencies.sort_unstable();
    let capture_statistics = capture.statistics();
    info!(
        elapsed_seconds = elapsed.as_secs_f64(),
        width = output_width,
        height = output_height,
        encoder = %encoder_descriptor.name,
        encoder_backend = %encoder_descriptor.kind,
        decoder = %decoder_descriptor.name,
        captured_frames,
        repeated_frames,
        encoded_frames,
        decoded_frames,
        rendered_frames,
        encode_fps = encoded_frames as f64 / elapsed.as_secs_f64(),
        render_fps = rendered_frames as f64 / elapsed.as_secs_f64(),
        encode_latency_p50_ms = percentile(&encode_latencies, 50).as_secs_f64() * 1_000.0,
        encode_latency_p95_ms = percentile(&encode_latencies, 95).as_secs_f64() * 1_000.0,
        decode_latency_p50_ms = percentile(&decode_latencies, 50).as_secs_f64() * 1_000.0,
        decode_latency_p95_ms = percentile(&decode_latencies, 95).as_secs_f64() * 1_000.0,
        render_latency_p50_ms = percentile(&render_latencies, 50).as_secs_f64() * 1_000.0,
        render_latency_p95_ms = percentile(&render_latencies, 95).as_secs_f64() * 1_000.0,
        local_pipeline_p50_ms = percentile(&pipeline_latencies, 50).as_secs_f64() * 1_000.0,
        local_pipeline_p95_ms = percentile(&pipeline_latencies, 95).as_secs_f64() * 1_000.0,
        encoded_bitrate_mbps = encoded_bytes as f64 * 8.0 / elapsed.as_secs_f64() / 1_000_000.0,
        keyframes,
        schedule_overruns,
        capture_timeouts = capture_statistics.no_change_timeouts,
        capture_access_loss_recoveries = capture_statistics.access_loss_recoveries,
        "local native-media benchmark complete"
    );
    if encoded_frames == 0 || rendered_frames == 0 {
        bail!("local media benchmark completed without encoded and rendered frames");
    }
    Ok(())
}

fn log_startup(adapter: &SelectedAdapter, config_path: Option<&std::path::Path>) {
    info!(
        adapter_name = %adapter.candidate.name,
        friendly_name = adapter.candidate.friendly_name.as_deref().unwrap_or("unknown"),
        description = adapter.candidate.description.as_deref().unwrap_or("unknown"),
        kind = %adapter.candidate.kind,
        transmit_speed_bps = ?adapter.candidate.transmit_speed_bps,
        receive_speed_bps = ?adapter.candidate.receive_speed_bps, mtu = ?adapter.candidate.mtu,
        local_ip = %adapter.address,
        default_route = adapter.candidate.is_default_route,
        physical = adapter.candidate.is_physical,
        selection = %adapter.reason,
        config = %config_path.map_or_else(|| "embedded defaults".to_owned(), |path| path.display().to_string()),
        "selected active network adapter"
    );
}

fn choose_role(configured: &str) -> Result<Role> {
    if configured != "ask" {
        return Role::from_str(configured).map_err(Into::into);
    }
    if !io::stdin().is_terminal() {
        bail!("role is not configured; pass --role teacher or --role student");
    }

    print!("Select role: [1] Teacher  [2] Student: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    match input.trim().to_ascii_lowercase().as_str() {
        "1" | "teacher" | "t" => Ok(Role::Teacher),
        "2" | "student" | "s" => Ok(Role::Student),
        _ => bail!("invalid role selection"),
    }
}

fn resolve_bind_ip(configured: &str, selected: Ipv4Addr) -> Result<Ipv4Addr> {
    if configured.eq_ignore_ascii_case("auto") {
        Ok(selected)
    } else {
        configured
            .parse()
            .with_context(|| format!("invalid network.bind_ip '{configured}'"))
    }
}

fn student_media_endpoint_ip(teacher_ip: Ipv4Addr, selected_adapter_ip: Ipv4Addr) -> Ipv4Addr {
    if teacher_ip.is_loopback() {
        Ipv4Addr::LOCALHOST
    } else {
        selected_adapter_ip
    }
}

fn loopback_adapter() -> SelectedAdapter {
    SelectedAdapter {
        candidate: AdapterCandidate {
            index: 0,
            name: "loopback".to_owned(),
            friendly_name: Some("localhost".to_owned()),
            description: Some("M3 same-machine test override".to_owned()),
            kind: "Loopback".to_owned(),
            transmit_speed_bps: None,
            receive_speed_bps: None,
            mtu: None,
            ipv4: vec![Ipv4Addr::LOCALHOST],
            is_up: true,
            is_default_route: false,
            is_physical: false,
            has_gateway: false,
        },
        address: Ipv4Addr::LOCALHOST,
        score: 0,
        reason: "explicit --loopback test override".to_owned(),
    }
}

fn run_teacher_media(
    config: &Config,
    bind_ip: Ipv4Addr,
    display_index: u32,
    share_seconds: Option<u64>,
    drop_every_nth_rtp_packet: u32,
) -> Result<()> {
    let mut capture = PlatformCapture::new();
    capture.start(CaptureConfig {
        display_index,
        width: config.video.width,
        height: config.video.height,
        max_fps: config.video.max_fps,
    })?;
    let (output_width, output_height) = capture
        .output_dimensions()
        .context("capture did not report its output dimensions")?;
    let media_socket = bind_teacher_media_socket(bind_ip, config.network.feedback_port)?;
    let teacher_media_udp_port = u32::from(media_socket.local_addr()?.port());
    let presentation = protocol::StartPresentation {
        stream_id: 1,
        width: output_width,
        height: output_height,
        max_fps: config.video.max_fps,
        bitrate_bps: config.video.target_bitrate_kbps.saturating_mul(1_000),
        rtp_payload_type: u32::from(H264_PAYLOAD_TYPE),
        rtp_clock_hz: RTP_VIDEO_CLOCK_HZ,
        teacher_media_udp_port,
    };
    let (event_sender, event_receiver) = std::sync::mpsc::channel();
    let control_config = presentation.clone();
    let control_port = config.network.control_port;
    tokio::spawn(async move {
        if let Err(error) =
            run_teacher_media_control(bind_ip, control_port, control_config, event_sender).await
        {
            warn!(%error, "teacher media control listener stopped");
        }
    });
    let _summary = run_teacher_media_loop(
        config,
        capture,
        &media_socket,
        (output_width, output_height),
        share_seconds,
        event_receiver,
        drop_every_nth_rtp_packet,
    )?;
    Ok(())
}

#[derive(Debug)]
struct TeacherMediaSummary {
    encoded_frames: u64,
    final_bitrate_bps: u32,
    bitrate_changes: u64,
    bitrate_change_first_access_unit_keyframes: u64,
}

#[allow(clippy::cast_precision_loss, clippy::too_many_lines)]
fn run_teacher_media_loop(
    config: &Config,
    capture: PlatformCapture,
    socket: &UdpSocket,
    output_dimensions: (u32, u32),
    share_seconds: Option<u64>,
    events: std::sync::mpsc::Receiver<TeacherMediaEvent>,
    drop_every_nth_rtp_packet: u32,
) -> Result<TeacherMediaSummary> {
    run_teacher_media_session(
        config,
        capture,
        socket,
        output_dimensions,
        share_seconds,
        events,
        drop_every_nth_rtp_packet,
        1,
        &AtomicBool::new(false),
    )
}

#[allow(clippy::too_many_arguments)]
fn run_teacher_media_session(
    config: &Config,
    capture: PlatformCapture,
    socket: &UdpSocket,
    output_dimensions: (u32, u32),
    share_seconds: Option<u64>,
    events: std::sync::mpsc::Receiver<TeacherMediaEvent>,
    drop_every_nth_rtp_packet: u32,
    stream_id: u64,
    cancelled: &AtomicBool,
) -> Result<TeacherMediaSummary> {
    let _activity = media_activity::MediaActivity::begin();
    let repair = teacher_repair::RepairService::for_stream_paced(
        stream_id,
        socket.try_clone()?,
        Duration::from_millis(u64::from(config.network.retransmission_cache_ms)),
        std::sync::Arc::new(udp_sender::AggregatePacer::new(
            config.network.send_pacing_mbps,
        )),
    )?;
    let stop = AtomicBool::new(false);
    let (forward, receiver) = std::sync::mpsc::sync_channel(256);
    thread::scope(|scope| {
        let service = &repair;
        let stopped = &stop;
        let router = scope.spawn(move || service.route(events, forward, stopped));
        let result = run_teacher_media_inner(
            config,
            capture,
            socket,
            output_dimensions,
            share_seconds,
            &receiver,
            drop_every_nth_rtp_packet,
            &repair,
            stream_id,
            cancelled,
        );
        stop.store(true, Ordering::Release);
        router
            .join()
            .map_err(|_| anyhow::anyhow!("Teacher control/repair dispatcher panicked"))?;
        info!(
            requested = repair.stats.requested.load(Ordering::Relaxed),
            cache_misses = repair.stats.misses.load(Ordering::Relaxed),
            cache_expired_misses = repair.stats.expired_misses.load(Ordering::Relaxed),
            cache_capacity_misses = repair.stats.capacity_misses.load(Ordering::Relaxed),
            cache_invalid_misses = repair.stats.invalid_misses.load(Ordering::Relaxed),
            cache_unknown_misses = repair.stats.unknown_misses.load(Ordering::Relaxed),
            cache_expired_packets = repair.stats.expired_packets.load(Ordering::Relaxed),
            cache_capacity_evictions = repair.stats.capacity_evictions.load(Ordering::Relaxed),
            stale_repairs = repair.stats.stale_repairs.load(Ordering::Relaxed),
            resent_packets = repair.stats.resent.load(Ordering::Relaxed),
            repair_send_bytes = repair.stats.bytes.load(Ordering::Relaxed),
            repair_send_errors = repair.stats.errors.load(Ordering::Relaxed),
            repair_send_would_block = repair.stats.would_block.load(Ordering::Relaxed),
            repair_deadline_exhausted = repair.stats.deadline_exhausted.load(Ordering::Relaxed),
            repair_retry_budget_exhausted =
                repair.stats.retry_budget_exhausted.load(Ordering::Relaxed),
            reports_dropped = repair.stats.reports_dropped.load(Ordering::Relaxed),
            "teacher repair service complete"
        );
        result
    })
}

#[allow(
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    clippy::too_many_arguments
)]
fn run_teacher_media_inner(
    config: &Config,
    mut capture: PlatformCapture,
    socket: &UdpSocket,
    output_dimensions: (u32, u32),
    share_seconds: Option<u64>,
    events: &std::sync::mpsc::Receiver<(Instant, TeacherMediaEvent)>,
    drop_every_nth_rtp_packet: u32,
    repair: &teacher_repair::RepairService,
    stream_id: u64,
    cancelled: &AtomicBool,
) -> Result<TeacherMediaSummary> {
    #[cfg(not(target_os = "macos"))]
    let runtime = MediaFoundationRuntime::new().context("failed to initialize media runtime")?;
    let mut codec_config = h264_config(&config.video);
    (codec_config.width, codec_config.height) = output_dimensions;
    let video_codec = stream_profile::session_codec(&config.video);
    #[cfg(not(target_os = "macos"))]
    let mut encoder = {
        anyhow::ensure!(
            video_codec == media_core::VideoCodec::H264,
            "{video_codec} encoding is not available on this platform"
        );
        PlatformH264Encoder::new(&runtime, codec_config)
            .context("failed to initialize native H.264 encoder")?
    };
    #[cfg(target_os = "macos")]
    let mut encoder = PlatformH264Encoder::with_codec(codec_config, video_codec)
        .with_context(|| format!("failed to initialize native {video_codec} encoder"))?;
    encoder.request_keyframe()?;
    let encoder_descriptor = encoder.descriptor();
    info!(codec = %video_codec, max_fps = config.video.max_fps, bitrate_kbps = config.video.target_bitrate_kbps, encoder = %encoder_descriptor.name, "media session codec profile");
    let mut packetizer =
        H264Packetizer::with_codec(protocol::presentation_ssrc(stream_id), video_codec);
    // O1: separate pointer channel. Capture keeps compositing the pointer until
    // every registered receiver says it draws the channel itself.
    let pointer_composite = std::sync::Arc::new(AtomicBool::new(true));
    let pointer = if config.video.pointer_channel {
        match (capture.pointer_source(), socket.try_clone()) {
            (Some(source), Ok(pointer_socket)) => {
                capture.set_cursor_policy(std::sync::Arc::clone(&pointer_composite));
                Some(pointer_channel::PointerBroadcaster::start(
                    pointer_socket,
                    source,
                    protocol::presentation_ssrc(stream_id) ^ 0x504f_494e,
                    std::sync::Arc::clone(&repair.pacer),
                ))
            }
            (_, Err(error)) => {
                warn!(%error, "pointer channel unavailable; pointer stays in video");
                None
            }
            (None, _) => None,
        }
    } else {
        None
    };
    // O2: one sharper burst of repeat frames after content settles.
    let mut refine = refinement::Refinement::new(
        if config.video.adaptive {
            config.video.refine_bitrate_percent
        } else {
            0
        },
        20_000_000,
    );
    let mut previous_refine = refine.stats;
    let mut refine_qp_supported = None::<bool>;
    let mut overlay_clients = HashSet::<u64>::new();
    let mut pointer_endpoints: Vec<SocketAddrV4> = Vec::new();
    let mut pointer_mode_changes = 0_u64;
    let mut previous_pointer_state_packets = 0_u64;
    let mut previous_pointer_image_packets = 0_u64;
    let mut previous_pointer_moves = 0_u64;
    let mut previous_pointer_skipped = 0_u64;
    let mut endpoints = HashMap::new();
    let mut udp_ready_clients = HashSet::new();
    let mut last_sent_capture = None;
    // O3: one IDR per coalescing window for joins/repairs; deferred, not dropped.
    let mut keyframes = keyframe_gate::KeyframeGate::new(Duration::from_millis(u64::from(
        config.video.keyframe_request_coalesce_ms.max(100),
    )));
    let mut previous_keyframes = keyframes.stats;
    let mut process_metrics = ProcessMetrics::new()?;
    let started = Instant::now();
    let ends = share_seconds.map(|seconds| started + Duration::from_secs(seconds));
    let mut quality = QualityController::new(&config.video, started);
    let mut next_tick = started;
    let mut encode_pacer = encode_pacer::EncodePacer::new(config.video.max_fps);
    let mut interval_aligned_encodes = 0_u64;
    let mut latest_frame: Option<VideoFrame> = None;
    let mut fresh_frame_pending = false;
    let mut media_slot_replacements = 0_u64;
    let mut repeated_frame_submissions = 0_u64;
    let mut frames_consumed = 0_u64;
    let mut capture_latencies = Vec::new();
    let mut dirty_ratio_sum = 0.0_f64;
    let mut encoded_frames = 0_u64;
    let mut bitrate_changes = 0_u64;
    let mut bitrate_change_first_access_unit_keyframes = 0_u64;
    let mut awaiting_bitrate_change_access_unit = false;
    let mut encoded_bytes = 0_u64;
    let mut interval_encoded_bytes = 0_u64;
    let mut interval_max_access_unit_bytes = 0_u64;
    let mut encoded_keyframes = 0_u64;
    // Fan-out runs on worker threads so a forty-Student IDR burst does not
    // stall the next capture/encode (docs/17 §9).
    let fanout = fanout::Fanout::start_paced(
        socket,
        fanout::FanoutConfig {
            workers: config.network.fanout_threads as usize,
            connected: config.network.connected_fanout,
            pacing_mbps: config.network.send_pacing_mbps,
            max_au_age: Duration::from_millis(u64::from(config.network.send_au_deadline_ms)),
            max_keyframe_age: Duration::from_millis(u64::from(
                config.network.send_keyframe_deadline_ms,
            )),
            send_buffer_bytes: MEDIA_SOCKET_SEND_BUFFER_BYTES,
        },
        &repair.pacer,
    )?;
    let mut previous_fanout = FanoutSnapshot::default();
    let mut encode_latencies = Vec::new();
    let mut interval_encode_latencies = Vec::new();
    let mut teacher_stages = media_core::diagnostics::StageMetrics::default();
    let mut interval_started = started;
    let mut previous_repair_bytes = 0_u64;
    let mut interval_encoded_frames = 0_u64;
    let mut interval_capture_frames = capture.statistics().frames_acquired;
    let mut interval_min_students = endpoints.len();
    let mut interval_max_students = endpoints.len();
    let mut original_packet_index = 0_u64;
    let mut intentionally_dropped_packets = 0_u64;
    info!(
        source = %socket.local_addr()?,
        width = codec_config.width,
        height = codec_config.height,
        encoder = %encoder_descriptor.name,
        encoder_backend = %encoder_descriptor.kind,
        "teacher M3 media loop ready; waiting for students"
    );
    loop {
        if cancelled.load(Ordering::Acquire) || ends.is_some_and(|end| Instant::now() >= end) {
            break;
        }
        anyhow::ensure!(
            !repair.stats.router_overflow.load(Ordering::Acquire),
            "Teacher critical control queue exceeded its bound"
        );
        while let Ok((arrived, event)) = events.try_recv() {
            teacher_stages.record("teacher_control_dispatch", arrived.elapsed());
            match event {
                TeacherMediaEvent::Registered(student) => {
                    if student.pointer_overlay {
                        overlay_clients.insert(student.client_id);
                    } else {
                        overlay_clients.remove(&student.client_id);
                    }
                    endpoints.insert(student.client_id, student.endpoint);
                    udp_ready_clients.remove(&student.client_id);
                    quality.register_receiver(student.client_id);
                    info!(client_id = student.client_id, hostname = %student.hostname, endpoint = %student.endpoint, connected_students = endpoints.len(), "teacher registered Student; awaiting UDP route probe");
                }
                TeacherMediaEvent::Disconnected {
                    client_id,
                    endpoint,
                } => {
                    repair.remove(client_id);
                    overlay_clients.remove(&client_id);
                    endpoints.remove(&client_id);
                    udp_ready_clients.remove(&client_id);
                    quality.remove_receiver(client_id);
                    info!(client_id, endpoint = %endpoint, connected_students = endpoints.len(), "teacher removed disconnected UDP media endpoint");
                }
                TeacherMediaEvent::Nack { .. } => {} // Handled exclusively by the repair dispatcher.
                TeacherMediaEvent::KeyframeRequested { client_id, message } => {
                    if message.stream_id == stream_id {
                        info!(
                            client_id,
                            "teacher received keyframe request; dispatcher scheduled IDR"
                        );
                    }
                }
                TeacherMediaEvent::ReceiverReport { client_id, report } => {
                    info!(target: "telemetry", client_id, report = ?report, "teacher per Student receiver report");
                    quality.observe_receiver_media_report(ReceiverMediaReport {
                        id: client_id,
                        received_packets: report.received_packets,
                        missing_packets: report.missing_packets,
                        decoded_frames: report.decoded_frames,
                        rendered_frames: report.rendered_frames,
                        rendering_enabled: report.rendering_enabled,
                        render_latency_p95_us: report.render_latency_p95_us,
                        received_at: Instant::now(),
                    });
                    info!(
                        client_id,
                        received_packets = report.received_packets,
                        missing_packets = report.missing_packets,
                        decoded_frames = report.decoded_frames,
                        rendered_frames = report.rendered_frames,
                        control_rtt_us = report.control_rtt_us,
                        control_rtt_samples = report.control_rtt_samples,
                        "student receiver report"
                    );
                }
            }
            interval_min_students = interval_min_students.min(endpoints.len());
            interval_max_students = interval_max_students.max(endpoints.len());
        }
        for _ in 0..MAX_MEDIA_ROUTE_PROBES_PER_TICK {
            let mut probe = [0_u8; 64];
            match socket.recv_from(&mut probe) {
                Ok((bytes, source)) => {
                    let Some(client_id) = parse_media_route_probe(&probe[..bytes]) else {
                        continue;
                    };
                    let SocketAddr::V4(source) = source else {
                        continue;
                    };
                    let Some(previous_endpoint) = endpoints.get_mut(&client_id) else {
                        continue;
                    };
                    let endpoint_changed = *previous_endpoint != source;
                    *previous_endpoint = source;
                    let first_probe = udp_ready_clients.insert(client_id);
                    repair.ready(client_id, source);
                    if first_probe || endpoint_changed {
                        keyframes.request(Instant::now());
                        info!(client_id, endpoint = %source, endpoint_changed, "teacher established Student UDP media route and requested IDR");
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => {
                    warn!(%error, "teacher could not receive Student UDP route probe");
                    break;
                }
            }
        }
        if let Some(pointer) = pointer.as_ref() {
            let composite =
                !pointer_channel::all_receivers_overlay(endpoints.keys(), &overlay_clients);
            if pointer_composite.swap(composite, Ordering::AcqRel) != composite {
                pointer_mode_changes += 1;
                info!(
                    pointer_mode = if composite { "composited" } else { "overlay" },
                    connected_students = endpoints.len(),
                    overlay_students = overlay_clients.len(),
                    "teacher pointer mode changed"
                );
            }
            let mut ready: Vec<SocketAddrV4> = udp_ready_clients
                .iter()
                .filter(|id| overlay_clients.contains(id))
                .filter_map(|id| endpoints.get(id).copied())
                .collect();
            ready.sort_unstable();
            if ready != pointer_endpoints {
                pointer.set_endpoints(ready.clone());
                pointer_endpoints = ready;
            }
        }
        // Acquire the latest capture only after the previous encoded AU has drained.
        // Large IDRs cannot accumulate a second encoded reference chain behind them.
        if !fanout.wait_idle(cancelled) {
            break;
        }
        let deadline = (Instant::now() + Duration::from_millis(2))
            .min(ends.unwrap_or_else(|| Instant::now() + Duration::from_millis(2)));
        let capture_poll_started = Instant::now();
        let captured = capture.next_frame(deadline)?;
        teacher_stages.record("capture_poll_wait", capture_poll_started.elapsed());
        if let Some(frame) = captured {
            if quality.observe_frame(&frame, Instant::now()) {
                next_tick = Instant::now();
            }
            teacher_stages.record("capture_api", frame.capture_latency);
            bounded_sample(&mut capture_latencies, frame.capture_latency);
            dirty_ratio_sum += f64::from(frame.dirty_region_ratio);
            frames_consumed += 1;
            if fresh_frame_pending {
                media_slot_replacements += 1;
            }
            latest_frame = Some(frame);
            fresh_frame_pending = true;
        }
        let now = Instant::now();
        teacher_stages.record(
            "teacher_tick_lateness",
            now.saturating_duration_since(next_tick),
        );
        let target_fps = quality.target_fps(now);
        let refinement_allowed = quality.refinement_allowed()
            && !fanout.stats.recovery_requested.load(Ordering::Acquire)
            && !keyframes.pending();
        if let Some(action) = refine.constrain(
            refinement_allowed,
            repair
                .pacer
                .bitrate_ceiling(endpoints.len())
                .min(config.video.max_bitrate_kbps.saturating_mul(1_000)),
        ) {
            apply_refinement(&mut encoder, action, &mut refine_qp_supported);
        }
        let refine_action = if refinement_allowed {
            refine.tick(
                target_fps <= config.video.adaptive_idle_fps,
                quality.bitrate_bps(),
                now,
            )
        } else {
            None
        };
        if let Some(action) = refine_action {
            apply_refinement(&mut encoder, action, &mut refine_qp_supported);
            if refine.refining() {
                next_tick = now; // start the refinement burst right away
            }
        }
        let encode_fps = if refine.refining() {
            refinement::REFINE_FPS.max(target_fps)
        } else {
            target_fps
        }
        .min(config.video.max_fps);
        let frame_interval =
            Duration::from_nanos(1_000_000_000_u64.div_ceil(u64::from(encode_fps)));
        // Capture-aligned encode: a fresh frame is encoded as soon as the fps
        // cap allows instead of
        // waiting for a free-running tick that is out of phase with capture.
        // Activity/refinement may advance the requested tick, but cannot bypass
        // the cap or accumulate a catch-up burst after a stalled encode.
        let earliest = encode_pacer.deadline(now, now, frame_interval);
        next_tick = encode_pacer.deadline(now, next_tick, frame_interval);
        if config.video.capture_aligned_encode && fresh_frame_pending {
            let aligned = earliest.max(now);
            if aligned < next_tick {
                next_tick = aligned;
                interval_aligned_encodes += 1;
            }
        }
        if now < next_tick {
            // NACK retransmission runs independently during this wait.
            if config.video.capture_aligned_encode && !fresh_frame_pending {
                // Block on capture until the tick; wake early for a new frame.
                let captured = capture.next_frame(next_tick)?;
                if let Some(frame) = captured {
                    if quality.observe_frame(&frame, Instant::now()) {
                        next_tick = Instant::now();
                    }
                    teacher_stages.record("capture_api", frame.capture_latency);
                    bounded_sample(&mut capture_latencies, frame.capture_latency);
                    dirty_ratio_sum += f64::from(frame.dirty_region_ratio);
                    frames_consumed += 1;
                    latest_frame = Some(frame);
                    fresh_frame_pending = true;
                    let aligned = earliest.max(Instant::now());
                    if aligned < next_tick {
                        next_tick = aligned;
                        interval_aligned_encodes += 1;
                    }
                }
            }
            next_tick = encode_pacer.deadline(Instant::now(), next_tick, frame_interval);
            let remaining = next_tick.saturating_duration_since(Instant::now());
            if !remaining.is_zero() {
                thread::sleep(remaining);
            }
            teacher_stages.record("publisher_pacing_wait", now.elapsed());
            teacher_stages.record(
                "publisher_pacing_overshoot",
                Instant::now().saturating_duration_since(next_tick),
            );
            // Refresh once after pacing; a newer capture can arrive during sleep.
            let capture_poll_started = Instant::now();
            let captured = capture.next_frame(Instant::now())?;
            teacher_stages.record("capture_poll_wait", capture_poll_started.elapsed());
            if let Some(frame) = captured {
                quality.observe_frame(&frame, Instant::now());
                teacher_stages.record("capture_api", frame.capture_latency);
                bounded_sample(&mut capture_latencies, frame.capture_latency);
                dirty_ratio_sum += f64::from(frame.dirty_region_ratio);
                frames_consumed += 1;
                if fresh_frame_pending {
                    media_slot_replacements += 1;
                }
                latest_frame = Some(frame);
                fresh_frame_pending = true;
            }
        }
        if interval_started.elapsed() >= Duration::from_secs(1) {
            let interval_seconds = interval_started.elapsed().as_secs_f64();
            let capture_statistics = capture.statistics();
            let process = process_metrics.sample()?;
            interval_encode_latencies.sort_unstable();
            let encode_p95 = percentile(&interval_encode_latencies, 95);
            let fan = FanoutSnapshot::take(&fanout.stats);
            let sender_ceiling_bps = repair.pacer.bitrate_ceiling(endpoints.len());
            let sender_pressure = fan.abandoned_packets > previous_fanout.abandoned_packets
                || (interval_encoded_bytes as f64 * 8.0 / interval_seconds)
                    > f64::from(sender_ceiling_bps);
            quality.observe_sender(sender_pressure, sender_ceiling_bps);
            if sender_pressure {
                if let Some(action) = refine.constrain(false, sender_ceiling_bps) {
                    apply_refinement(&mut encoder, action, &mut refine_qp_supported);
                }
            }
            let quality_update =
                quality.evaluate_interval(encode_p95, process.cpu_capacity_percent, Instant::now());
            // Adaptive bitrate waits while a refinement burst owns the encoder rate.
            match quality.apply_bitrate(
                &mut encoder,
                quality_update
                    .bitrate_change_bps
                    .filter(|_| !refine.refining()),
                Instant::now(),
            ) {
                Ok(Some(bitrate_bps)) => {
                    bitrate_changes += 1;
                    awaiting_bitrate_change_access_unit = true;
                    info!(bitrate_bps, reason = ?quality_update.bitrate_reason,
                        encode_pressure = quality_update.encode_pressure,
                        cpu_pressure = quality_update.cpu_pressure,
                        process_cpu_capacity_percent = process.cpu_capacity_percent,
                        "teacher adaptive bitrate changed");
                }
                Ok(None) => {}
                Err(error) => {
                    warn!(%error, requested_bitrate_bps = ?quality_update.bitrate_change_bps, "encoder rejected adaptive bitrate; keeping current bitrate");
                }
            }
            let nack_requested_sequences = repair.stats.requested.load(Ordering::Relaxed);
            let nack_cache_misses = repair.stats.misses.load(Ordering::Relaxed);
            let retransmitted_packets = repair.stats.resent.load(Ordering::Relaxed);
            let repair_bytes = repair.stats.bytes.load(Ordering::Relaxed);
            let repair_interval_bytes = repair_bytes.saturating_sub(previous_repair_bytes);
            previous_repair_bytes = repair_bytes;
            log_stages("teacher", repair.diagnostics());
            let interval_packets = fan.sent_packets - previous_fanout.sent_packets;
            let interval_bytes = fan.sent_bytes - previous_fanout.sent_bytes;
            info!(
                connected_students = endpoints.len(),
                connected_students_min = interval_min_students,
                connected_students_max = interval_max_students,
                interval_seconds,
                encoder_instances = 1,
                capture_fps = capture_statistics
                    .frames_acquired
                    .saturating_sub(interval_capture_frames) as f64
                    / interval_seconds,
                capture_frames = capture_statistics.frames_acquired,
                waiting_frames_replaced =
                    capture_statistics.latest_frame_replacements + media_slot_replacements,
                repeated_frame_submissions,
                encode_fps = interval_encoded_frames as f64 / interval_seconds,
                encoded_stream_mbps = interval_encoded_bytes as f64 * 8.0 / interval_seconds / 1_000_000.0,
                interval_max_access_unit_bytes, encoded_keyframes, nack_requested_sequences, nack_cache_misses, retransmitted_packets,
                nack_expired_misses = repair.stats.expired_misses.load(Ordering::Relaxed),
                nack_capacity_misses = repair.stats.capacity_misses.load(Ordering::Relaxed),
                nack_invalid_misses = repair.stats.invalid_misses.load(Ordering::Relaxed),
                nack_unknown_misses = repair.stats.unknown_misses.load(Ordering::Relaxed),
                stale_repairs = repair.stats.stale_repairs.load(Ordering::Relaxed),
                encode_latency_p95_ms = encode_p95.as_secs_f64() * 1_000.0,
                target_fps = quality_update.target_fps,
                target_bitrate_bps = quality.bitrate_bps(),
                adaptive_bitrate_reason = ?quality_update.bitrate_reason,
                adaptive_bitrate_proposal_bps = ?quality_update.bitrate_change_bps,
                adaptive_bitrate_deferred = refine.refining() && quality_update.bitrate_change_bps.is_some(),
                adaptive_encode_pressure = quality_update.encode_pressure,
                adaptive_cpu_pressure = quality_update.cpu_pressure,
                adaptive_pressure_ceiling_fps = quality_update.pressure_ceiling_fps,
                refinement_active = refine.refining(),
                sender_pressure,
                sender_ceiling_bps,
                send_au_deadline_drops = fanout.stats.au_deadline_drops.load(Ordering::Relaxed),
                send_preflight_recipient_drops = fanout.stats.preflight_recipient_drops.load(Ordering::Relaxed),
                connected_send_fallbacks = fanout.stats.connected_send_fallbacks.load(Ordering::Relaxed),
                send_dependency_skipped_packets = fanout.stats.dependency_skipped_packets.load(Ordering::Relaxed),
                aggregate_pacer_denied = repair.pacer.denied.load(Ordering::Relaxed),
                aggregate_pacer_wait_us = repair.pacer.wait_us.load(Ordering::Relaxed),
                activity_ratio = quality.activity_ratio(),
                receiver_loss_percent = ?quality_update.receiver_loss_percent,
                receiver_reporting_students = quality_update.receiver_reporting_students,
                receiver_high_loss_students = quality_update.receiver_high_loss_students,
                receiver_slow_students = quality_update.receiver_slow_students,
                receiver_render_slow_students = quality_update.receiver_render_slow_students,
                receiver_feedback_eligible = quality_update.receiver_feedback_eligible,
                receiver_recovery_eligible = quality_update.receiver_recovery_eligible,
                process_metrics_available = process.available,
                process_cpu_percent = process.cpu_percent,
                process_cpu_capacity_percent = process.cpu_capacity_percent,
                process_working_set_bytes = process.working_set_bytes,
                sent_packets = interval_packets,
                send_errors = fan.send_errors,
                send_would_block = fan.would_block,
                send_resource_backpressure = fan.resource_backpressure,
                send_retry_recovered = fan.retry_recovered,
                send_deadline_exhausted = fan.deadline_exhausted,
                send_retry_budget_exhausted = fan.retry_budget_exhausted,
                send_abandoned_packets = fan.abandoned_packets,
                send_retry_wait_interval_ms = (fan.retry_wait_us - previous_fanout.retry_wait_us) as f64 / 1_000.0,
                send_pacing_wait_interval_ms = (fan.pacing_wait_us - previous_fanout.pacing_wait_us) as f64 / 1_000.0,
                fanout_handoff_wait_interval_ms = (fan.handoff_wait_us - previous_fanout.handoff_wait_us) as f64 / 1_000.0,
                fanout_threads = config.network.fanout_threads,
                fanout_connected_sockets = fanout.stats.connected_sockets.load(Ordering::Relaxed),
                fanout_connect_failures = fanout.stats.connect_failures.load(Ordering::Relaxed),
                repair_send_errors = repair.stats.errors.load(Ordering::Relaxed),
                repair_send_would_block = repair.stats.would_block.load(Ordering::Relaxed),
                repair_deadline_exhausted = repair.stats.deadline_exhausted.load(Ordering::Relaxed),
                repair_retry_budget_exhausted = repair.stats.retry_budget_exhausted.load(Ordering::Relaxed),
                control_reports_dropped = repair.stats.reports_dropped.load(Ordering::Relaxed),
                repair_send_mbps = repair_interval_bytes as f64 * 8.0 / interval_seconds / 1_000_000.0,
                send_mbps = (interval_bytes + repair_interval_bytes) as f64 * 8.0 / interval_seconds / 1_000_000.0,
                "teacher M3 media interval"
            );
            info!(
                target: "telemetry",
                refinements = refine.stats.refinements - previous_refine.refinements,
                refinement_frames = refine.stats.frames - previous_refine.frames,
                refinement_bytes = refine.stats.bytes - previous_refine.bytes,
                refinements_cancelled = refine.stats.cancelled - previous_refine.cancelled,
                refinement_last_ms = refine.last_duration_ms,
                refine_bitrate_percent = config.video.refine_bitrate_percent,
                refine_qp_supported = ?refine_qp_supported,
                "teacher refinement interval"
            );
            previous_refine = refine.stats;
            info!(
                target: "telemetry",
                keyframe_requests = keyframes.stats.requests - previous_keyframes.requests,
                keyframe_requests_coalesced = keyframes.stats.coalesced - previous_keyframes.coalesced,
                keyframes_forced = keyframes.stats.forced - previous_keyframes.forced,
                keyframes_emitted = keyframes.stats.emitted - previous_keyframes.emitted,
                keyframes_periodic_or_rebuild = (keyframes.stats.emitted - previous_keyframes.emitted)
                    .saturating_sub(keyframes.stats.forced - previous_keyframes.forced),
                keyframe_max_bytes = keyframes.interval_max_keyframe_bytes,
                keyframe_interval_ms = config.video.keyframe_interval_ms,
                keyframe_gate_window_ms = keyframes.window().as_millis(),
                "teacher keyframe interval"
            );
            info!(
                target: "telemetry",
                capture_aligned_encode = config.video.capture_aligned_encode,
                aligned_encodes = interval_aligned_encodes,
                "teacher encode scheduling interval"
            );
            interval_aligned_encodes = 0;
            previous_keyframes = keyframes.stats;
            keyframes.interval_max_keyframe_bytes = 0;
            if let Some(pointer) = pointer.as_ref() {
                let state = pointer.stats.state_packets.load(Ordering::Relaxed);
                let image = pointer.stats.image_packets.load(Ordering::Relaxed);
                let moves = pointer.stats.moves.load(Ordering::Relaxed);
                info!(
                    target: "telemetry",
                    pointer_mode = if pointer_composite.load(Ordering::Acquire) {
                        "composited"
                    } else {
                        "overlay"
                    },
                    pointer_receivers = pointer_endpoints.len(),
                    pointer_state_packets = state - previous_pointer_state_packets,
                    pointer_image_packets = image - previous_pointer_image_packets,
                    pointer_moves_per_second = (moves - previous_pointer_moves) as f64 / interval_seconds,
                    pointer_only_updates_skipped = capture_statistics.pointer_only_updates_skipped - previous_pointer_skipped,
                    pointer_mode_changes,
                    pointer_send_errors = pointer.stats.send_errors.load(Ordering::Relaxed),
                    "teacher pointer channel interval"
                );
                previous_pointer_state_packets = state;
                previous_pointer_image_packets = image;
                previous_pointer_moves = moves;
                previous_pointer_skipped = capture_statistics.pointer_only_updates_skipped;
            }
            log_stages("teacher", encoder.take_diagnostics());
            log_stages("teacher", std::mem::take(&mut teacher_stages));
            log_stages("teacher", fanout.take_stages());
            previous_fanout = fan;
            interval_started = Instant::now();
            interval_min_students = endpoints.len();
            interval_max_students = endpoints.len();
            interval_capture_frames = capture_statistics.frames_acquired;
            interval_encoded_frames = 0;
            interval_encoded_bytes = 0;
            interval_max_access_unit_bytes = 0;
            interval_encode_latencies.clear();
        }
        let Some(frame) = latest_frame.as_ref() else {
            next_tick = Instant::now() + frame_interval;
            continue;
        };
        let fresh_input = fresh_frame_pending;
        if !fresh_frame_pending {
            repeated_frame_submissions += 1;
        }
        fresh_frame_pending = false;
        // The dispatcher coalesces recovery requests independently of the media
        // event queue. Only this thread owns/calls the codec.
        if repair.stats.keyframe_pending.swap(false, Ordering::AcqRel)
            | fanout
                .stats
                .recovery_requested
                .swap(false, Ordering::AcqRel)
        {
            keyframes.request(Instant::now());
        }
        let oversized_percent = fanout
            .stats
            .oversized_keyframe_percent
            .swap(0, Ordering::AcqRel);
        if oversized_percent != 0 {
            if let Some(action) = refine.constrain(false, quality.bitrate_bps()) {
                apply_refinement(&mut encoder, action, &mut refine_qp_supported);
            }
            let proposal = quality.oversized_keyframe_bitrate(oversized_percent);
            match quality.apply_bitrate(&mut encoder, proposal, Instant::now()) {
                Ok(Some(bitrate_bps)) => {
                    bitrate_changes += 1;
                    awaiting_bitrate_change_access_unit = true;
                    info!(bitrate_bps, oversized_percent, "teacher reduced bitrate after unsendable keyframe");
                }
                Ok(None) => warn!(oversized_percent, bitrate_bps = quality.bitrate_bps(),
                    "unsendable keyframe: bitrate minimum, opt-out or unsupported backend; recovery remains bounded"),
                Err(error) => warn!(%error, "encoder rejected oversized keyframe bitrate reduction"),
            }
        }
        if keyframes.poll(Instant::now()) {
            encoder.request_keyframe()?;
        }
        let encode_started = Instant::now();
        encode_pacer.admitted(encode_started);
        if fresh_input {
            teacher_stages.record(
                "fresh_capture_to_encode_age",
                encode_started.saturating_duration_since(frame.captured_at),
            );
        }
        teacher_stages.record(
            "capture_to_encode_age",
            encode_started.saturating_duration_since(frame.captured_at),
        );
        if let Some(access_unit) = encoder.encode(VideoFrameRef {
            bytes: &frame.bytes,
            captured_at: frame.captured_at,
            width: frame.width,
            height: frame.height,
        })? {
            if awaiting_bitrate_change_access_unit {
                if access_unit.is_keyframe {
                    bitrate_change_first_access_unit_keyframes += 1;
                }
                awaiting_bitrate_change_access_unit = false;
            }
            bounded_sample(&mut encode_latencies, encode_started.elapsed());
            bounded_sample(&mut interval_encode_latencies, encode_started.elapsed());
            encoded_frames += 1;
            interval_encoded_frames += 1;
            let access_unit_bytes = u64::try_from(access_unit.bytes.len()).unwrap_or(u64::MAX);
            encoded_bytes += access_unit_bytes;
            interval_encoded_bytes += access_unit_bytes;
            interval_max_access_unit_bytes = interval_max_access_unit_bytes.max(access_unit_bytes);
            encoded_keyframes += u64::from(access_unit.is_keyframe);
            if access_unit.is_keyframe {
                keyframes.emitted(access_unit.bytes.len(), Instant::now());
            }
            if let Some(action) = refine.encoded(access_unit.bytes.len(), Instant::now()) {
                apply_refinement(&mut encoder, action, &mut refine_qp_supported);
            }
            let packetize_started = Instant::now();
            let send_us = media_core::unix_us();
            let source = encoder.take_source_timing(access_unit.timestamp);
            if let Some(source) = source {
                teacher_stages.record("encoder_input_to_send", source.submitted_at.elapsed());
                if last_sent_capture != Some(source.captured_at) {
                    teacher_stages
                        .record("fresh_source_capture_to_send", source.captured_at.elapsed());
                }
                last_sent_capture = Some(source.captured_at);
            }
            let timing = media_core::FrameTiming {
                capture_unix_us: source.map_or(0, |source| source.capture_unix_us),
                send_unix_us: send_us,
            };
            let packets = packetizer.packetize_timed(
                EncodedAccessUnitRef {
                    bytes: &access_unit.bytes,
                    timestamp: access_unit.timestamp,
                    is_keyframe: access_unit.is_keyframe,
                },
                timing,
            );
            teacher_stages.record("packetization", packetize_started.elapsed());
            let fanout_started = Instant::now();
            let mut outgoing = Vec::with_capacity(packets.len());
            for packet in packets {
                let sequence_number = RtpPacket::parse(&packet)?.sequence_number;
                repair.cache(sequence_number, &packet, Instant::now());
                original_packet_index += 1;
                if drop_every_nth_rtp_packet != 0
                    && original_packet_index % u64::from(drop_every_nth_rtp_packet) == 0
                {
                    intentionally_dropped_packets +=
                        u64::try_from(endpoints.len()).unwrap_or(u64::MAX);
                    continue;
                }
                outgoing.push(packet);
            }
            fanout.send(fanout::FanoutJob {
                is_keyframe: access_unit.is_keyframe,
                packets: outgoing,
                targets: endpoints
                    .iter()
                    .filter(|(client_id, _)| udp_ready_clients.contains(*client_id))
                    .map(|(&client_id, &endpoint)| (client_id, endpoint))
                    .collect(),
            });
            teacher_stages.record("udp_fanout_handoff", fanout_started.elapsed());
        }
        next_tick += frame_interval;
        if next_tick < Instant::now() {
            next_tick = Instant::now() + frame_interval;
        }
    }
    // Join the workers so the totals include the last AU.
    let fanout_stats = std::sync::Arc::clone(&fanout.stats);
    fanout.wait_idle(&AtomicBool::new(false));
    log_stages("teacher", fanout.take_stages());
    drop(fanout);
    let fan = FanoutSnapshot::take(&fanout_stats);
    capture_latencies.sort_unstable();
    let nack_requested_sequences = repair.stats.requested.load(Ordering::Relaxed);
    let nack_cache_misses = repair.stats.misses.load(Ordering::Relaxed);
    let retransmitted_packets = repair.stats.resent.load(Ordering::Relaxed);
    log_stages("teacher", repair.diagnostics());
    log_stages("teacher", encoder.take_diagnostics());
    log_stages("teacher", teacher_stages);
    encode_latencies.sort_unstable();
    let elapsed = started.elapsed();
    let capture_statistics = capture.statistics();
    let process = process_metrics.sample()?;
    let summary = TeacherMediaSummary {
        encoded_frames,
        final_bitrate_bps: quality.bitrate_bps(),
        bitrate_changes,
        bitrate_change_first_access_unit_keyframes,
    };
    info!(
        elapsed_seconds = elapsed.as_secs_f64(),
        captured_frames = capture_statistics.frames_acquired,
        capture_fps = capture_statistics.frames_acquired as f64 / elapsed.as_secs_f64(),
        capture_latency_p50_ms = percentile(&capture_latencies, 50).as_secs_f64() * 1_000.0,
        capture_latency_p95_ms = percentile(&capture_latencies, 95).as_secs_f64() * 1_000.0,
        average_dirty_ratio = dirty_ratio_sum / frames_consumed.max(1) as f64,
        waiting_frames_replaced =
            capture_statistics.latest_frame_replacements + media_slot_replacements,
        repeated_frame_submissions,
        encoded_frames = summary.encoded_frames,
        encode_fps = encoded_frames as f64 / elapsed.as_secs_f64(),
        target_fps = quality.target_fps(Instant::now()),
        target_bitrate_bps = summary.final_bitrate_bps,
        bitrate_changes = summary.bitrate_changes,
        bitrate_change_first_access_unit_keyframes =
            summary.bitrate_change_first_access_unit_keyframes,
        encoded_stream_mbps = encoded_bytes as f64 * 8.0 / elapsed.as_secs_f64() / 1_000_000.0,
        sent_packets = fan.sent_packets,
        retransmitted_packets,
        encoded_keyframes,
        nack_requested_sequences,
        nack_cache_misses,
        send_errors = fan.send_errors,
        send_deadline_exhausted = fan.deadline_exhausted,
        send_retry_budget_exhausted = fan.retry_budget_exhausted,
        send_would_block = fan.would_block,
        send_resource_backpressure = fan.resource_backpressure,
        send_retry_recovered = fan.retry_recovered,
        send_abandoned_packets = fan.abandoned_packets,
        intentionally_dropped_packets,
        connected_students = endpoints.len(),
        encoder_instances = 1,
        process_metrics_available = process.available,
        process_cpu_percent = process.cpu_percent,
        process_cpu_capacity_percent = process.cpu_capacity_percent,
        process_working_set_bytes = process.working_set_bytes,
        aggregate_send_mbps = (fan.sent_bytes + repair.stats.bytes.load(Ordering::Relaxed)) as f64
            * 8.0
            / elapsed.as_secs_f64()
            / 1_000_000.0,
        encode_latency_p50_ms = percentile(&encode_latencies, 50).as_secs_f64() * 1_000.0,
        encode_latency_p95_ms = percentile(&encode_latencies, 95).as_secs_f64() * 1_000.0,
        "teacher M3 media complete"
    );
    Ok(summary)
}

#[allow(clippy::too_many_lines)]
async fn run_student_media(
    config: &Config,
    teacher_ip: Ipv4Addr,
    media_endpoint_ip: Ipv4Addr,
    share_seconds: Option<u64>,
    headless: bool,
    connect_delay_ms: u64,
    profile_path: &std::path::Path,
) -> Result<bool> {
    let teacher = SocketAddrV4::new(teacher_ip, config.network.control_port);
    // Bind before registration so the route probe and all incoming RTP use the same
    // local UDP port.
    let media_socket = bind_media_socket(config.network.media_port)?;
    if connect_delay_ms != 0 {
        info!(connect_delay_ms, "student test registration delay");
        tokio::time::sleep(Duration::from_millis(connect_delay_ms)).await;
    }
    let registration = StudentRegistration {
        hostname: host_name(),
        instance_id: startup::student_identity(profile_path, config.network.media_port)?,
        selected_local_ip: media_endpoint_ip,
        media_udp_port: config.network.media_port,
        feedback_udp_port: config.network.feedback_port,
        // Visible Students draw the Teacher pointer channel themselves.
        pointer_overlay: !headless && config.video.pointer_channel,
    };
    let mut desktop = if headless {
        None
    } else {
        match desktop_tray::StudentDesktop::new() {
            Ok(value) => Some(value),
            Err(error) => {
                warn!(%error,"Student native background host unavailable");
                None
            }
        }
    };
    let inbox = std::sync::Arc::new(classroom_tools::Inbox::default());
    inbox.initialize_connection_popup(profile_path, config.tools.connection_popup);
    inbox.badge.lock().unwrap().load(profile_path)?;
    if let Some(desktop) = desktop.as_mut() {
        desktop.connection_preference = Some(std::sync::Arc::clone(&inbox));
    }
    if let Some(desktop) = desktop.as_mut() {
        desktop.tools = Some(student_tools_ui::StudentToolsUi::new(
            std::sync::Arc::clone(&inbox),
        ));
    }
    let presenting = std::sync::Arc::new(AtomicBool::new(false));
    let tools = classroom_tools::student(
        SocketAddrV4::new(
            teacher_ip,
            config
                .network
                .control_port
                .checked_add(2)
                .context("Tools port overflow")?,
        ),
        registration.instance_id.clone(),
        std::sync::Arc::clone(&inbox),
        diagnostics::Source::new(config, &registration.hostname)?,
        std::sync::Arc::clone(&presenting),
    );
    let mut restart_requested = false;
    let monitor = config.monitoring.enabled.then(|| {
        monitoring::student(
            SocketAddrV4::new(teacher_ip, config.network.control_port + 1),
            registration.instance_id.clone(),
            std::sync::Arc::clone(&presenting),
            std::sync::Arc::clone(&inbox.badge_broadcast_suppressed),
            std::sync::Arc::clone(&inbox.badge_broadcast_suppression_applied),
            config.clone(),
        )
    });
    info!("student background agent ready; waiting for Teacher presentation");
    let started = Instant::now();
    let ends = share_seconds.map(|seconds| started + Duration::from_secs(seconds));
    'agent: loop {
        if inbox.update_exit.load(Ordering::Acquire) {
            break 'agent;
        }
        if let Some(desktop) = desktop.as_mut() {
            desktop.pump(&format!("Connecting to Teacher {teacher_ip}"), false);
            if desktop.exit_requested {
                break 'agent;
            }
            if desktop.settings_requested {
                desktop.settings_requested = false;
                if let Some(updated) = student_settings::show(config, teacher_ip, desktop)? {
                    student_settings::save(&updated, profile_path)?;
                    startup::save(
                        profile_path,
                        launcher::LaunchSelection {
                            role: Role::Student,
                            teacher_ip: updated.network.teacher_ip,
                        },
                    )?;
                    restart_requested = true;
                    break 'agent;
                }
                if desktop.exit_requested {
                    break 'agent;
                }
            }
        }
        if ends.is_some_and(|end| Instant::now() >= end) {
            return Ok(false);
        }
        let registration_future = register_student_for_presentation(teacher, registration.clone());
        tokio::pin!(registration_future);
        let registered = loop {
            tokio::select! {
                result = &mut registration_future => break result,
                () = tokio::time::sleep(Duration::from_millis(20)) => {
                    if inbox.update_exit.load(Ordering::Acquire) { break 'agent; }
                    if let Some(desktop) = desktop.as_mut() {
                        desktop.pump(&format!("Connecting to Teacher {teacher_ip}"), false);
                        if desktop.exit_requested {
                            break 'agent;
                        }
                        if desktop.settings_requested {
                            continue 'agent;
                        }
                    }
                }
            }
        };
        let (control, client_id, presentation) = match registered {
            Ok(registered) => registered,
            Err(error) => {
                warn!(%teacher, %error, "student control registration failed; retrying");
                tokio::time::sleep(Duration::from_millis(500)).await;
                continue;
            }
        };
        info!(client_id, %teacher, "student room connection ready");
        let mut next = presentation;
        let mut last_probe = Instant::now();
        while control.is_connected() {
            if inbox.update_exit.load(Ordering::Acquire) {
                break 'agent;
            }
            if let Some(desktop) = desktop.as_mut() {
                desktop.pump(&format!("Connected to Teacher {teacher_ip}"), false);
                if desktop.exit_requested {
                    break 'agent;
                }
                // Leave this connection before the modal settings dialog; it must
                // not silently miss presentation commands while editing.
                if desktop.settings_requested {
                    break;
                }
            }
            if ends.is_some_and(|end| Instant::now() >= end) {
                return Ok(false);
            }
            let Some(presentation) = next.take().or_else(|| control.presentation()) else {
                if last_probe.elapsed() >= Duration::from_secs(1) {
                    let _ = control.probe_rtt();
                    last_probe = Instant::now();
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
                continue;
            };
            info!(client_id, %teacher, stream_id = presentation.stream_id, width = presentation.width, height = presentation.height, "student presentation registration complete");
            let port = u16::try_from(presentation.teacher_media_udp_port)
                .ok()
                .filter(|port| *port != 0)
                .context("teacher announced an invalid UDP media port")?;
            presenting.store(true, Ordering::Release);
            let outcome = run_student_media_loop(
                config,
                &presentation,
                &control,
                client_id,
                SocketAddrV4::new(teacher_ip, port),
                ends,
                &media_socket,
                headless,
                &mut desktop,
                None,
                Some(&inbox.update_exit),
            );
            presenting.store(false, Ordering::Release);
            if inbox.update_exit.load(Ordering::Acquire) {
                break 'agent;
            }
            if desktop
                .as_ref()
                .is_some_and(|desktop| desktop.exit_requested)
            {
                break 'agent;
            }
            match outcome? {
                StudentSessionOutcome::Completed => return Ok(false),
                StudentSessionOutcome::PresentationStopped => {
                    info!(client_id, "student waiting for next presentation");
                }
                StudentSessionOutcome::Reconnect => break,
            }
        }
        warn!(%teacher, "student control connection ended; reconnecting");
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    drop(tools);
    drop(monitor);
    drop(media_socket);
    let tray_restart = desktop
        .as_ref()
        .is_some_and(|desktop| desktop.restart_requested);
    restart_requested |= tray_restart;
    drop(desktop);
    if tray_restart {
        startup::save(
            profile_path,
            launcher::LaunchSelection {
                role: Role::Student,
                teacher_ip,
            },
        )?;
    }
    info!(
        restart_requested,
        "Student workers stopped; agent shutdown complete"
    );
    Ok(restart_requested && !inbox.update_exit.load(Ordering::Acquire))
}

/// Cumulative fan-out counters read once per telemetry interval.
#[derive(Clone, Copy, Default)]
struct FanoutSnapshot {
    sent_packets: u64,
    sent_bytes: u64,
    send_errors: u64,
    abandoned_packets: u64,
    would_block: u64,
    resource_backpressure: u64,
    retry_recovered: u64,
    retry_wait_us: u64,
    deadline_exhausted: u64,
    retry_budget_exhausted: u64,
    pacing_wait_us: u64,
    handoff_wait_us: u64,
}

impl FanoutSnapshot {
    fn take(stats: &fanout::FanoutStats) -> Self {
        let load = |counter: &std::sync::atomic::AtomicU64| counter.load(Ordering::Relaxed);
        Self {
            sent_packets: load(&stats.sent_packets),
            sent_bytes: load(&stats.sent_bytes),
            send_errors: load(&stats.send_errors),
            abandoned_packets: load(&stats.abandoned_packets),
            would_block: load(&stats.would_block),
            resource_backpressure: load(&stats.resource_backpressure),
            retry_recovered: load(&stats.retry_recovered),
            retry_wait_us: load(&stats.retry_wait_us),
            deadline_exhausted: load(&stats.deadline_exhausted),
            retry_budget_exhausted: load(&stats.retry_budget_exhausted),
            pacing_wait_us: load(&stats.pacing_wait_us),
            handoff_wait_us: load(&stats.handoff_wait_us),
        }
    }
}

fn bind_media_socket(port: u16) -> Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_recv_buffer_size(MEDIA_SOCKET_RECEIVE_BUFFER_BYTES)?;
    info!(
        actual_receive_buffer = socket.recv_buffer_size()?,
        "student UDP receive buffer ready"
    );
    socket.bind(&SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port).into())?;
    Ok(socket.into())
}

fn bind_teacher_media_socket(bind_ip: Ipv4Addr, port: u16) -> Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_send_buffer_size(MEDIA_SOCKET_SEND_BUFFER_BYTES)?;
    // Per-Student connected fan-out sockets share this address.
    fanout::allow_shared_port(&socket)?;
    socket.bind(&SocketAddrV4::new(bind_ip, port).into())?;
    socket.set_nonblocking(true)?;
    let actual_send_buffer = socket.send_buffer_size()?;
    info!(actual_send_buffer, "teacher UDP send buffer ready");
    Ok(socket.into())
}

#[allow(
    clippy::cast_precision_loss,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]
fn run_student_media_loop(
    config: &Config,
    presentation: &protocol::StartPresentation,
    control: &control_transport::StudentControl,
    client_id: u64,
    teacher_media_endpoint: SocketAddrV4,
    ends: Option<Instant>,
    socket: &UdpSocket,
    headless: bool,
    desktop: &mut Option<desktop_tray::StudentDesktop>,
    mut observation: Option<&mut observation::Output>,
    update_exit: Option<&AtomicBool>,
) -> Result<StudentSessionOutcome> {
    let _activity = media_activity::MediaActivity::begin();
    let shared = Mutex::new(StudentPresentation::new(
        presentation.max_fps,
        Instant::now(),
    ));
    let stop = AtomicBool::new(false);
    let mut renderer = if headless || observation.is_some() {
        None
    } else {
        let renderer = PlatformRenderer::new_student_presentation(
            presentation.width,
            presentation.height,
            "Classroom — Teacher presentation",
        )?;
        info!(
            fullscreen = true,
            topmost = true,
            "student forced presentation window ready"
        );
        Some(renderer)
    };
    // The window stays on its owner thread (also required by macOS). Codec
    // initialization and destruction happen entirely on the receive/decode worker.
    socket.set_read_timeout(Some(Duration::from_millis(5)))?;
    let ingress = Mutex::new(udp_ingress::IngressState::default());
    let observe_metrics = observation
        .as_ref()
        .map(|_| Mutex::new(observation::ReceiverMetrics::default()));
    thread::scope(|scope| {
        let ingest =
            scope.spawn(|| udp_ingress::receive(socket, teacher_media_endpoint, &ingress, &stop));
        let worker = scope.spawn(|| {
            run_student_receiver(
                config,
                presentation,
                control,
                client_id,
                teacher_media_endpoint,
                ends,
                socket,
                headless,
                &shared,
                &stop,
                &ingress,
                observe_metrics.as_ref(),
            )
        });
        let ui_result = (|| -> Result<()> {
            let mut fps_badge = student_fps_badge::FpsBadge::new(
                renderer.is_some() && config.diagnostics.student_fps_badge,
            );
            let mut last_render_metrics = Instant::now();
            let mut render_stages = media_core::diagnostics::StageMetrics::default();
            let mut last_capture_us = 0_u64;
            let preview_started = Instant::now();
            let mut first_present = true;
            // O1: pointer drawn locally over the latest frame.
            let mut pointer_dirty = false;
            let mut pointer_redraws = 0_u64;
            let mut pointer_updates = 0_u64;
            let mut last_present = Instant::now();
            while !worker.is_finished() {
                if update_exit.is_some_and(|flag| flag.load(Ordering::Acquire)) {
                    break;
                }
                if renderer.is_some() {
                    let update = ingress.lock().unwrap().pointer.take_update();
                    if let (Some(overlay), Some(renderer)) = (update, renderer.as_mut()) {
                        renderer.set_pointer(overlay);
                        pointer_dirty = true;
                        pointer_updates += 1;
                    }
                }
                if let Some(desktop) = desktop.as_mut() {
                    desktop.pump("Watching Teacher presentation", true);
                    if desktop.exit_requested {
                        break;
                    }
                }
                if let Some(output) = observation.as_mut() {
                    output.pump(control, presentation.stream_id)?;
                    if output.closed() {
                        break;
                    }
                }
                if !control.is_presenting(presentation.stream_id)
                    || ends.is_some_and(|end| Instant::now() >= end)
                    || renderer
                        .as_mut()
                        .is_some_and(|renderer| !renderer.is_open())
                {
                    break;
                }
                if renderer.is_some() || observation.is_some() {
                    let next = shared.lock().unwrap().take_due(Instant::now());
                    if let Some((frame, ready_at)) = next {
                        let render_started = Instant::now();
                        let mut surface_published = false;
                        render_stages.record("decoded_wait", ready_at.elapsed());
                        if let Some(renderer) = renderer.as_mut() {
                            renderer.render(DecodedFrameRef {
                                format: frame.format,
                                bytes: &frame.bytes,
                                width: frame.width,
                                height: frame.height,
                            })?;
                            fps_badge.frame();
                        } else if let Some(output) = observation.as_mut() {
                            surface_published = output.present(&frame)?;
                        }
                        if first_present {
                            info!(
                                client_id,
                                first_present_ms = preview_started.elapsed().as_secs_f64() * 1000.0,
                                "student first presented frame"
                            );
                            first_present = false;
                        }
                        if let (Some(timing), Some(clock)) =
                            (frame.timing, control.clock_estimate())
                        {
                            if let Some(age) = clock.source_age(timing.send_unix_us) {
                                render_stages.record("source_send_to_present_estimate", age);
                            }
                            if timing.capture_unix_us != last_capture_us {
                                if let Some(age) = clock.source_age(timing.capture_unix_us) {
                                    render_stages.record("source_capture_to_present_estimate", age);
                                    if let Some(output) =
                                        observation.as_mut().filter(|_| surface_published)
                                    {
                                        output.record_delay(age);
                                    }
                                }
                            }
                            last_capture_us = timing.capture_unix_us;
                        }
                        shared.lock().unwrap().note_presented(
                            render_started,
                            ready_at,
                            Instant::now(),
                        );
                        pointer_dirty = false;
                        last_present = Instant::now();
                    }
                }
                // Pointer-only motion: re-present the latest frame, at most
                // once per ~4 ms, without waiting for (or causing) video.
                if pointer_dirty && last_present.elapsed() >= Duration::from_millis(4) {
                    if let Some(renderer) = renderer.as_mut() {
                        if renderer.redraw()? {
                            pointer_redraws += 1;
                        }
                    }
                    pointer_dirty = false;
                    last_present = Instant::now();
                }
                if last_render_metrics.elapsed() >= Duration::from_secs(1) {
                    if renderer.is_some() {
                        let ingress = ingress.lock().unwrap();
                        info!(
                            target: "telemetry",
                            pointer_updates,
                            pointer_redraws,
                            pointer_state_packets = ingress.pointer.state_packets,
                            pointer_image_packets = ingress.pointer.image_packets,
                            pointer_images = ingress.pointer.images_completed,
                            pointer_malformed = ingress.pointer.malformed,
                            "student pointer channel interval"
                        );
                        pointer_updates = 0;
                        pointer_redraws = 0;
                    }
                    if let (Some(output), Some(metrics)) =
                        (observation.as_mut(), observe_metrics.as_ref())
                    {
                        output.report_metrics(
                            &metrics.lock().unwrap().clone(),
                            control.clock_estimate().map(|clock| clock.uncertainty_us),
                        );
                    }
                    if let Some(renderer) = renderer.as_mut() {
                        log_stages("student", renderer.take_diagnostics());
                    }
                    log_stages("student", std::mem::take(&mut render_stages));
                    last_render_metrics = Instant::now();
                }
                fps_badge.pump();
                thread::sleep(Duration::from_millis(1));
            }
            if let Some(renderer) = renderer.as_mut() {
                log_stages("student", renderer.take_diagnostics());
            }
            log_stages("student", render_stages);
            Ok(())
        })();
        stop.store(true, Ordering::Release);
        // Close immediately, before waiting for an in-flight decoder operation.
        drop(renderer.take());
        if !control.is_connected() {
            info!(client_id, "student preview closed after Teacher disconnect");
        } else if !control.is_presenting(presentation.stream_id) {
            info!(
                client_id,
                stream_id = presentation.stream_id,
                "student preview closed after Teacher stop"
            );
        }
        let result = worker
            .join()
            .map_err(|_| anyhow::anyhow!("Student receiver worker panicked"))?;
        ingest
            .join()
            .map_err(|_| anyhow::anyhow!("UDP ingest worker panicked"))?;
        ui_result?;
        result
    })
}

#[allow(
    clippy::cast_precision_loss,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]
fn run_student_receiver(
    config: &Config,
    presentation: &protocol::StartPresentation,
    control: &control_transport::StudentControl,
    client_id: u64,
    teacher_media_endpoint: SocketAddrV4,
    ends: Option<Instant>,
    socket: &UdpSocket,
    headless: bool,
    shared: &Mutex<StudentPresentation>,
    stop: &AtomicBool,
    ingress: &Mutex<udp_ingress::IngressState>,
    observe_metrics: Option<&Mutex<observation::ReceiverMetrics>>,
) -> Result<StudentSessionOutcome> {
    let Some(video_codec) =
        media_core::VideoCodec::from_rtp_payload_type(presentation.rtp_payload_type)
            .filter(|&codec| stream_profile::local_decodable(codec))
    else {
        bail!(
            "publisher announced unsupported RTP payload type {}",
            presentation.rtp_payload_type
        );
    };
    if presentation.rtp_clock_hz != RTP_VIDEO_CLOCK_HZ {
        bail!("publisher announced unsupported RTP clock rate");
    }
    let codec_config = H264Config {
        width: presentation.width,
        height: presentation.height,
        frames_per_second: presentation.max_fps,
        bitrate: presentation.bitrate_bps,
        keyframe_interval_frames: config
            .video
            .max_fps
            .saturating_mul(config.video.keyframe_interval_ms)
            / 1_000,
    };
    #[cfg(not(target_os = "macos"))]
    let runtime = MediaFoundationRuntime::new().context("failed to initialize media runtime")?;
    #[cfg(not(target_os = "macos"))]
    let mut decoder = PlatformH264Decoder::new(&runtime, codec_config)?;
    #[cfg(target_os = "macos")]
    let mut decoder = PlatformH264Decoder::with_codec(codec_config, video_codec)?;
    info!(codec = %video_codec, max_fps = presentation.max_fps, "receiver codec selected");
    socket.set_read_timeout(Some(Duration::from_millis(5)))?;
    let mut jitter = JitterBuffer::with_repair_budget(
        Duration::from_millis(u64::from(config.video.jitter_buffer_ms)),
        Duration::from_millis(u64::from(config.video.nack_repair_ms)),
    );
    let experimental_rtt_nack_retry =
        std::env::var("CLASSROOM_TEST_NACK_RETRY_RTT").is_ok_and(|value| value == "1");
    if experimental_rtt_nack_retry {
        info!("test-only RTT-bounded NACK retry experiment enabled");
    }
    info!(
        policy = if experimental_rtt_nack_retry {
            "rtt"
        } else {
            "half-budget"
        },
        "student NACK retry policy"
    );
    let mut depacketizer = H264Depacketizer::for_codec(video_codec);
    let mut process_metrics = ProcessMetrics::new()?;
    let started = Instant::now();
    let mut received_packets = 0_u64;
    let mut received_bytes = 0_u64;
    let mut decoded_frames = 0_u64;
    let mut previous_rendered_frames = 0_u64;
    let mut decode_errors = 0_u64;
    let mut depacketizer_errors = 0_u64;
    let mut received_access_units = 0_u64;
    let mut received_keyframes = 0_u64;
    let mut nack_sequences_sent = 0_u64;
    let mut pre_keyframe_frames_discarded = 0_u64;
    let mut decode_latencies = Vec::new();
    let mut interval_decode_latencies = Vec::new();
    let mut interval_packet_interarrival_gaps = Vec::new();
    let mut interval_access_unit_gaps = Vec::new();
    let mut interval_started = started;
    let mut interval_received_bytes = 0_u64;
    let mut interval_decoded_frames = 0_u64;
    let mut interval_render_slot_replacements = 0_u64;
    let mut nacks_sent = 0_u64;
    let mut keyframe_requests = 0_u64;
    let mut last_keyframe_request = None;
    let mut waiting_for_keyframe = true;
    let mut recovery_timestamp = None;
    let mut recovery_packets_discarded = 0_u64;
    let mut unrepaired_gap_resets = 0_u64;
    let mut waiting_for_keyframe_since = started;
    let mut udp_route_probes_sent = 0_u64;
    let mut last_udp_route_probe = None;
    let mut last_packet_received_at = None;
    let mut last_access_unit_completed_at = None;
    let mut receiver_stages = media_core::diagnostics::StageMetrics::default();
    let mut frame_timings: VecDeque<(u128, media_core::FrameTiming)> = VecDeque::new();
    let mut previous_capture_us = 0_u64;
    let mut repeated_source_frames = 0_u64;
    let mut unmapped_decoded_frames = 0_u64;
    let mut recovery_episodes = 0_u64;
    let mut stale_backlog_packets = 0_u64;
    let mut observed_ingress_drops = 0_u64;
    let mut ingress_reference_loss_events = 0_u64;
    let mut recovery_peak_ms = 0.0_f64;
    let mut decode_health = decode_health::DecodeHealth::default();
    let mut decoder_no_output_resets = 0_u64;
    let mut incomplete_recovery_units = 0_u64;
    let mut pending_decoded_recovery_since = Some(started);
    let mut decoded_recovery_peak_ms = 0.0_f64;

    info!(address = %socket.local_addr()?, jitter_buffer_ms = config.video.jitter_buffer_ms, headless, "student M3 UDP receiver ready");
    send_media_route_probe(
        socket,
        teacher_media_endpoint,
        client_id,
        &mut udp_route_probes_sent,
        &mut last_udp_route_probe,
    );
    if control.probe_rtt().is_err() {
        return Ok(StudentSessionOutcome::Reconnect);
    }
    let mut reconnect = false;
    'session: while !stop.load(Ordering::Acquire) && ends.is_none_or(|end| Instant::now() < end) {
        if !control.is_presenting(presentation.stream_id) {
            info!(
                client_id,
                "teacher control connection closed; stopping Student receiver"
            );
            break;
        }
        // Repair budget follows the measured control RTT (Wi-Fi RTT can exceed
        // the configured floor several-fold, making every repair "late"). While
        // frozen waiting for a keyframe there is nothing to lose by waiting
        // longer, so the IDR itself gets its retransmissions.
        let rtt_us = control.rtt_sample().1;
        let floor = Duration::from_millis(u64::from(config.video.nack_repair_ms));
        let rtt_budget = Duration::from_micros(rtt_us.saturating_mul(3) / 2 + 5_000)
            .clamp(floor, floor.max(Duration::from_millis(150)));
        jitter.set_repair_budget(if waiting_for_keyframe {
            rtt_budget.max(Duration::from_millis(250))
        } else {
            rtt_budget
        });
        if experimental_rtt_nack_retry {
            jitter.set_retry_delay_ceiling(rtt_budget);
        }
        let mut drained = false;
        // Drain bounded batches independently of codec cost; the ingest worker
        // timestamps arrivals even while decode is busy.
        for _ in 0..512 {
            let (next, ingress_drops) = {
                let mut input = ingress.lock().unwrap();
                anyhow::ensure!(
                    input.socket_errors == 0,
                    "UDP ingest worker stopped after a socket error"
                );
                let packet = input.pop(Instant::now());
                (packet, input.dropped)
            };
            if ingress_drops > observed_ingress_drops {
                // These packets arrived but were deliberately expired locally.
                // Do not ask for their old reference chain through NACK: recover
                // immediately from a fresh IDR while retaining newer ingress data.
                observed_ingress_drops = ingress_drops;
                ingress_reference_loss_events += 1;
                stale_backlog_packets += reset_receiver_reference_chain(
                    &mut jitter,
                    &mut depacketizer,
                    &mut frame_timings,
                ) as u64;
                if !waiting_for_keyframe {
                    waiting_for_keyframe_since = Instant::now();
                }
                waiting_for_keyframe = true;
                recovery_timestamp = None;
                shared.lock().unwrap().clear();
                if !request_recovery_keyframe(
                    control,
                    presentation.stream_id,
                    &mut last_keyframe_request,
                    &mut keyframe_requests,
                ) {
                    reconnect = true;
                    break 'session;
                }
            }
            let Some(received) = next else {
                break;
            };
            drained = true;
            let packet = received.packet;
            if packet.ssrc != protocol::presentation_ssrc(presentation.stream_id) {
                continue;
            }
            let packet_received_at = received.arrived;
            received_packets += 1;
            let bytes = packet.payload.len()
                + media_transport::RTP_HEADER_BYTES
                + if packet.timing.is_some() { 20 } else { 0 };
            received_bytes += bytes as u64;
            interval_received_bytes += bytes as u64;
            if let Some(previous) = last_packet_received_at {
                interval_packet_interarrival_gaps
                    .push(packet_received_at.saturating_duration_since(previous));
            }
            last_packet_received_at = Some(packet_received_at);
            if packet.marker {
                if let (Some(timing), Some(clock)) = (packet.timing, control.clock_estimate()) {
                    if let Some(age) = clock.source_age(timing.send_unix_us) {
                        receiver_stages.record(
                            "source_send_to_udp_marker_estimate",
                            age.saturating_sub(packet_received_at.elapsed()),
                        );
                    }
                }
            }
            if waiting_for_keyframe && recovery_timestamp != Some(packet.timestamp) {
                if !recovery_packet_starts_au(&packet, video_codec) {
                    recovery_packets_discarded += 1;
                    continue;
                }
                stale_backlog_packets += reset_receiver_reference_chain(
                    &mut jitter,
                    &mut depacketizer,
                    &mut frame_timings,
                ) as u64;
                recovery_timestamp = Some(packet.timestamp);
            }
            jitter.push(packet, packet_received_at);
        }
        if !drained {
            thread::sleep(Duration::from_millis(1));
        }
        // P-frames cannot be dropped independently: abandon stale compressed
        // backlog and recover from an IDR rather than decoding old reference chains.
        if jitter.backlog_is_stale(Instant::now()) {
            stale_backlog_packets += jitter.discard_backlog() as u64;
            depacketizer.reset();
            frame_timings.clear();
            if !waiting_for_keyframe {
                waiting_for_keyframe_since = Instant::now();
            }
            waiting_for_keyframe = true;
            recovery_timestamp = None;
            shared.lock().unwrap().clear();
            if !request_recovery_keyframe(
                control,
                presentation.stream_id,
                &mut last_keyframe_request,
                &mut keyframe_requests,
            ) {
                reconnect = true;
                break 'session;
            }
        }
        loop {
            let missing_before = jitter.statistics().missing_sequences;
            let packet = jitter.pop_ready(Instant::now());
            if jitter.statistics().missing_sequences > missing_before {
                unrepaired_gap_resets += 1;
                stale_backlog_packets += reset_receiver_reference_chain(
                    &mut jitter,
                    &mut depacketizer,
                    &mut frame_timings,
                ) as u64;
                if !waiting_for_keyframe {
                    waiting_for_keyframe_since = Instant::now();
                }
                waiting_for_keyframe = true;
                recovery_timestamp = None;
                shared.lock().unwrap().clear();
                if !request_recovery_keyframe(
                    control,
                    presentation.stream_id,
                    &mut last_keyframe_request,
                    &mut keyframe_requests,
                ) {
                    reconnect = true;
                    break 'session;
                }
                break;
            }
            let missing_sequences = jitter.take_missing_sequences();
            if !missing_sequences.is_empty() {
                nacks_sent += 1;
                nack_sequences_sent += missing_sequences.len() as u64;
                if control
                    .nack(protocol::Nack {
                        stream_id: presentation.stream_id,
                        sequence_numbers: missing_sequences.into_iter().map(u32::from).collect(),
                    })
                    .is_err()
                {
                    reconnect = true;
                    break 'session;
                }
            }
            let Some(packet) = packet else {
                break;
            };
            match depacketizer.push(&packet) {
                Ok(Some(access_unit)) => {
                    received_access_units += 1;
                    received_keyframes +=
                        u64::from(access_unit_is_keyframe(&access_unit, video_codec));
                    let access_unit_completed_at = Instant::now();
                    if let Some(previous) = last_access_unit_completed_at {
                        interval_access_unit_gaps
                            .push(access_unit_completed_at.saturating_duration_since(previous));
                    }
                    last_access_unit_completed_at = Some(access_unit_completed_at);
                    if waiting_for_keyframe && !access_unit_is_keyframe(&access_unit, video_codec) {
                        pre_keyframe_frames_discarded += 1;
                        continue;
                    }
                    if waiting_for_keyframe
                        && !decode_health::complete_recovery_unit(&access_unit.bytes, video_codec)
                    {
                        incomplete_recovery_units += 1;
                        recovery_timestamp = None;
                        stale_backlog_packets += reset_receiver_reference_chain(
                            &mut jitter,
                            &mut depacketizer,
                            &mut frame_timings,
                        ) as u64;
                        info!(
                            incomplete_recovery_units,
                            "student rejected recovery AU missing parameter sets"
                        );
                        if !request_recovery_keyframe(
                            control,
                            presentation.stream_id,
                            &mut last_keyframe_request,
                            &mut keyframe_requests,
                        ) {
                            reconnect = true;
                            break 'session;
                        }
                        continue;
                    }
                    if waiting_for_keyframe {
                        pending_decoded_recovery_since = Some(waiting_for_keyframe_since);
                        let recovery = waiting_for_keyframe_since.elapsed();
                        receiver_stages.record("keyframe_recovery", recovery);
                        recovery_peak_ms = recovery_peak_ms.max(recovery.as_secs_f64() * 1000.0);
                        recovery_episodes += 1;
                    }
                    waiting_for_keyframe = false;
                    recovery_timestamp = None;
                    if let Some(timing) = access_unit.timing {
                        if frame_timings.len() == 128 {
                            frame_timings.pop_front();
                        }
                        let hns = u128::from(access_unit.timestamp) * 1_000_000_000
                            / u128::from(RTP_VIDEO_CLOCK_HZ)
                            / 100;
                        frame_timings.push_back((hns, timing));
                        if let Some(clock) = control.clock_estimate() {
                            if let Some(age) = clock.source_age(timing.send_unix_us) {
                                receiver_stages.record("source_send_to_access_unit_estimate", age);
                            }
                        }
                        if timing.capture_unix_us != 0
                            && timing.capture_unix_us == previous_capture_us
                        {
                            repeated_source_frames += 1;
                        }
                        previous_capture_us = timing.capture_unix_us;
                    }
                    let decoded_before = decoded_frames;
                    match decode_remote_access_unit(
                        &mut decoder,
                        &access_unit,
                        video_codec,
                        &mut decode_latencies,
                    ) {
                        Ok(mut frames) => {
                            if decode_health.observe(!frames.is_empty(), Instant::now()) {
                                decoder_no_output_resets += 1;
                                waiting_for_keyframe_since =
                                    pending_decoded_recovery_since.unwrap_or_else(Instant::now);
                                waiting_for_keyframe = true;
                                recovery_timestamp = None;
                                stale_backlog_packets += reset_receiver_reference_chain(
                                    &mut jitter,
                                    &mut depacketizer,
                                    &mut frame_timings,
                                ) as u64;
                                shared.lock().unwrap().clear();
                                info!(decoder_no_output_resets, "student decoder accepted AUs without output; requesting fresh IDR");
                                if !request_recovery_keyframe(
                                    control,
                                    presentation.stream_id,
                                    &mut last_keyframe_request,
                                    &mut keyframe_requests,
                                ) {
                                    reconnect = true;
                                    break 'session;
                                }
                            }
                            if !frames.is_empty() {
                                if let Some(since) = pending_decoded_recovery_since.take() {
                                    let elapsed = since.elapsed();
                                    decoded_recovery_peak_ms = decoded_recovery_peak_ms
                                        .max(elapsed.as_secs_f64() * 1000.0);
                                    receiver_stages.record("keyframe_decode_recovery", elapsed);
                                    info!(
                                        decoded_recovery_ms = elapsed.as_secs_f64() * 1000.0,
                                        "student recovery confirmed by decoded output"
                                    );
                                }
                            }
                            for frame in &mut frames {
                                frame.timing = frame.media_timestamp.and_then(|timestamp| {
                                    frame_timings
                                        .iter()
                                        .find(|(time, _)| *time == timestamp.as_nanos() / 100)
                                        .map(|(_, timing)| *timing)
                                });
                                if frame.timing.is_none() {
                                    unmapped_decoded_frames += 1;
                                }
                            }
                            if decoded_frames == 0 && !frames.is_empty() {
                                info!(
                                    client_id,
                                    first_decode_ms = started.elapsed().as_secs_f64() * 1000.0,
                                    "student first decoded frame"
                                );
                            }
                            decoded_frames += u64::try_from(frames.len()).unwrap_or(u64::MAX);
                            if !headless {
                                let mut output = shared.lock().unwrap();
                                for frame in frames {
                                    output.submit(frame, Instant::now());
                                }
                            }
                            if let Some(latency) = decode_latencies.last() {
                                interval_decode_latencies.push(*latency);
                            }
                        }
                        Err(error) => {
                            decode_errors += 1;
                            if !waiting_for_keyframe {
                                waiting_for_keyframe_since = Instant::now();
                            }
                            waiting_for_keyframe = true;
                            recovery_timestamp = None;
                            stale_backlog_packets += reset_receiver_reference_chain(
                                &mut jitter,
                                &mut depacketizer,
                                &mut frame_timings,
                            ) as u64;
                            shared.lock().unwrap().clear();
                            warn!(%error, "student discarded unrecoverable access unit");
                            if !request_recovery_keyframe(
                                control,
                                presentation.stream_id,
                                &mut last_keyframe_request,
                                &mut keyframe_requests,
                            ) {
                                reconnect = true;
                                break 'session;
                            }
                        }
                    }
                    interval_decoded_frames += decoded_frames - decoded_before;
                }
                Ok(None) => {}
                Err(error) => {
                    depacketizer_errors += 1;
                    if !waiting_for_keyframe {
                        waiting_for_keyframe_since = Instant::now();
                    }
                    waiting_for_keyframe = true;
                    recovery_timestamp = None;
                    stale_backlog_packets += reset_receiver_reference_chain(
                        &mut jitter,
                        &mut depacketizer,
                        &mut frame_timings,
                    ) as u64;
                    shared.lock().unwrap().clear();
                    warn!(%error, "student depacketizer lost an access unit");
                    if !request_recovery_keyframe(
                        control,
                        presentation.stream_id,
                        &mut last_keyframe_request,
                        &mut keyframe_requests,
                    ) {
                        reconnect = true;
                        break 'session;
                    }
                }
            }
        }
        if waiting_for_keyframe
            && waiting_for_keyframe_since.elapsed() >= Duration::from_millis(500)
            && !request_recovery_keyframe(
                control,
                presentation.stream_id,
                &mut last_keyframe_request,
                &mut keyframe_requests,
            )
        {
            reconnect = true;
            break 'session;
        }
        if waiting_for_keyframe
            && last_udp_route_probe.is_none_or(|last| last.elapsed() >= MEDIA_ROUTE_PROBE_INTERVAL)
        {
            send_media_route_probe(
                socket,
                teacher_media_endpoint,
                client_id,
                &mut udp_route_probes_sent,
                &mut last_udp_route_probe,
            );
        }
        if interval_started.elapsed() >= Duration::from_secs(1) {
            let elapsed = interval_started.elapsed();
            if let Some(metrics) = observe_metrics {
                *metrics.lock().unwrap() = observation::ReceiverMetrics {
                    receive_mbps: interval_received_bytes as f64 * 8.0
                        / elapsed.as_secs_f64()
                        / 1_000_000.0,
                    sampled_unix_us: media_core::unix_us(),
                };
            }
            log_stages("student", decoder.take_diagnostics());
            log_stages("student", jitter.take_diagnostics());
            log_stages("student", std::mem::take(&mut receiver_stages));
            let clock = control.clock_estimate();
            info!(target: "telemetry", clock_synchronized = clock.is_some(), clock_offset_us = ?clock.map(|value| value.teacher_minus_student_us),
                clock_uncertainty_us = ?clock.map(|value| value.uncertainty_us),
                jitter_queue_depth = jitter.queued_packets(), repaired_gaps = jitter.statistics().repaired_gaps, nack_retries = jitter.statistics().nack_retries, nack_retry_after_us = jitter.retry_delay().as_micros(), repair_budget_ms = jitter.repair_budget().as_millis(), fast_released_frames = jitter.statistics().fast_released_frames, nack_repair_ms = config.video.nack_repair_ms, late_packets = jitter.statistics().late_packets,
                missing_sequences = jitter.statistics().missing_sequences, duplicate_packets = jitter.statistics().duplicate_packets,
                reordered_packets = jitter.statistics().reordered_packets, jitter_overflow_packets = jitter.statistics().overflow_packets,
                recovery_packets_discarded, unrepaired_gap_resets,
                waiting_for_keyframe, waiting_for_keyframe_ms = if waiting_for_keyframe { waiting_for_keyframe_since.elapsed().as_millis() } else { 0 },
                recovery_episodes, recovery_peak_ms, decoded_recovery_peak_ms, decoder_no_output_resets, incomplete_recovery_units,
                awaiting_decoded_recovery = pending_decoded_recovery_since.is_some(), stale_backlog_packets, ingress_reference_loss_events, repeated_source_frames, unmapped_decoded_frames,
                "student freshness recovery interval");
            {
                let mut wire = ingress.lock().unwrap();
                info!(target: "telemetry",
                    wire_packets = wire.packets,
                    wire_bytes = wire.bytes,
                    ingress_dropped_packets = wire.dropped,
                    injected_dropped_packets = wire.test_dropped,
                    injected_parameter_dropped_packets = wire.test_parameter_dropped,
                    injected_repair_dropped_packets = wire.test_repair_dropped,
                    controlled_repair_accepted_packets = wire.test_repair_accepted,
                    malformed_packets = wire.malformed,
                    wrong_peer_packets = wire.peer_rejected,
                    socket_errors = wire.socket_errors,
                    ingress_queue_depth = wire.depth(),
                    ingress_queue_high_water = wire.high_water,
                    ingress_queue_oldest_ms = wire.oldest_age().as_secs_f64() * 1000.0,
                    "student UDP ingest interval"
                );
                log_stages("student", std::mem::take(&mut wire.diagnostics));
            }
            let stats = jitter.statistics();
            let process = process_metrics.sample()?;
            let render = shared.lock().unwrap().snapshot();
            let rendered_frames = render.rendered_frames;
            let interval_rendered_frames = rendered_frames.saturating_sub(previous_rendered_frames);
            let current_render_slot_replacements = render.replaced_frames;
            let render_gap_interval_p95_ms = render.gap_p95.as_secs_f64() * 1_000.0;
            let render_latency_interval_p95_ms = render.latency_p95.as_secs_f64() * 1_000.0;
            let render_latency_p95_us =
                u64::try_from(render.latency_p95.as_micros()).unwrap_or(u64::MAX);
            let decode_latency_interval_p95 =
                take_interval_percentile(&mut interval_decode_latencies, 95);
            let (packet_interarrival_interval_p95, packet_interarrival_interval_max) =
                take_interval_percentile_and_max(&mut interval_packet_interarrival_gaps);
            let (access_unit_gap_interval_p95, access_unit_gap_interval_max) =
                take_interval_percentile_and_max(&mut interval_access_unit_gaps);
            let (control_rtt_samples, control_rtt_us) = control.rtt_sample();
            if control
                .report(protocol::ReceiverReport {
                    stream_id: presentation.stream_id,
                    received_packets,
                    missing_packets: stats.late_or_missing_packets,
                    decoded_frames,
                    rendered_frames,
                    receive_bitrate_bps: received_bytes
                        .saturating_mul(8)
                        .checked_div(started.elapsed().as_secs().max(1))
                        .unwrap_or(0),
                    jitter_delay_ms: config.video.jitter_buffer_ms,
                    control_rtt_us,
                    control_rtt_samples,
                    rendering_enabled: !headless,
                    render_latency_p95_us,
                    recovering: waiting_for_keyframe || pending_decoded_recovery_since.is_some(),
                    health_version: 1,
                })
                .is_err()
            {
                reconnect = true;
                break 'session;
            }
            info!(
                decode_fps = interval_decoded_frames as f64 / elapsed.as_secs_f64(),
                render_fps = interval_rendered_frames as f64 / elapsed.as_secs_f64(),
                receive_mbps =
                    interval_received_bytes as f64 * 8.0 / elapsed.as_secs_f64() / 1_000_000.0,
                process_metrics_available = process.available,
                process_cpu_percent = process.cpu_percent,
                process_working_set_bytes = process.working_set_bytes,
                received_packets,
                udp_route_probes_sent,
                waiting_for_keyframe,
                last_packet_age_ms = last_packet_received_at
                    .map_or(-1.0, |last| last.elapsed().as_secs_f64() * 1_000.0),
                missing_packets = stats.late_or_missing_packets,
                nacks_sent,
                keyframe_requests,
                decode_errors,
                depacketizer_errors,
                received_access_units,
                received_keyframes,
                nack_sequences_sent,
                pre_keyframe_frames_discarded,
                render_slot_replacements = render.replaced_frames,
                render_slot_replacements_interval = current_render_slot_replacements
                    .saturating_sub(interval_render_slot_replacements),
                packet_interarrival_interval_p95_ms =
                    packet_interarrival_interval_p95.as_secs_f64() * 1_000.0,
                packet_interarrival_interval_max_ms =
                    packet_interarrival_interval_max.as_secs_f64() * 1_000.0,
                access_unit_gap_interval_p95_ms =
                    access_unit_gap_interval_p95.as_secs_f64() * 1_000.0,
                access_unit_gap_interval_max_ms =
                    access_unit_gap_interval_max.as_secs_f64() * 1_000.0,
                render_gap_interval_p95_ms,
                render_latency_interval_p95_ms,
                decode_latency_interval_p95_ms =
                    decode_latency_interval_p95.as_secs_f64() * 1_000.0,
                decoded_frame_wait_interval_p95_ms = render.wait_p95.as_secs_f64() * 1_000.0,
                pending_render_frames = render.pending_frames,
                receiver_render_decoupled = true,
                render_gap_p95_ms = render.total_gap_p95.as_secs_f64() * 1_000.0,
                jitter_delay_ms = config.video.jitter_buffer_ms,
                control_rtt_ms = control_rtt_us as f64 / 1_000.0,
                control_rtt_samples,
                interval_seconds = elapsed.as_secs_f64(),
                "student M3 media interval"
            );
            if control.probe_rtt().is_err() {
                reconnect = true;
                break 'session;
            }
            interval_started = Instant::now();
            interval_received_bytes = 0;
            interval_decoded_frames = 0;
            previous_rendered_frames = rendered_frames;
            interval_render_slot_replacements = current_render_slot_replacements;
        }
    }
    log_stages("student", decoder.take_diagnostics());
    log_stages("student", jitter.take_diagnostics());
    log_stages("student", receiver_stages);
    log_stages(
        "student",
        std::mem::take(&mut ingress.lock().unwrap().diagnostics),
    );
    {
        let wire = ingress.lock().unwrap();
        info!(
            wire_packets = wire.packets,
            wire_bytes = wire.bytes,
            ingress_dropped_packets = wire.dropped,
            injected_dropped_packets = wire.test_dropped,
            injected_parameter_dropped_packets = wire.test_parameter_dropped,
            ingress_reference_loss_events,
            malformed_packets = wire.malformed,
            wrong_peer_packets = wire.peer_rejected,
            socket_errors = wire.socket_errors,
            ingress_queue_high_water = wire.high_water,
            stale_backlog_packets,
            recovery_episodes,
            recovery_peak_ms,
            "student UDP recovery totals"
        );
    }
    decode_latencies.sort_unstable();
    let render = shared.lock().unwrap().snapshot();
    let rendered_frames = render.rendered_frames;
    let elapsed = started.elapsed();
    let stats = jitter.statistics();
    let process = process_metrics.sample()?;
    let (control_rtt_samples, control_rtt_us) = control.rtt_sample();
    info!(
        elapsed_seconds = elapsed.as_secs_f64(),
        received_packets,
        udp_route_probes_sent,
        missing_packets = stats.late_or_missing_packets,
        nacks_sent,
        keyframe_requests,
        decode_errors,
        depacketizer_errors,
        received_access_units,
        received_keyframes,
        nack_sequences_sent,
        pre_keyframe_frames_discarded,
        render_slot_replacements = render.replaced_frames,
        render_gap_p95_ms = render.total_gap_p95.as_secs_f64() * 1_000.0,
        jitter_delay_ms = config.video.jitter_buffer_ms,
        control_rtt_ms = control_rtt_us as f64 / 1_000.0,
        control_rtt_samples,
        decoded_frames,
        rendered_frames,
        decode_fps = decoded_frames as f64 / elapsed.as_secs_f64(),
        render_fps = rendered_frames as f64 / elapsed.as_secs_f64(),
        process_metrics_available = process.available,
        process_cpu_percent = process.cpu_percent,
        process_working_set_bytes = process.working_set_bytes,
        receive_mbps = received_bytes as f64 * 8.0 / elapsed.as_secs_f64() / 1_000_000.0,
        decode_latency_p50_ms = percentile(&decode_latencies, 50).as_secs_f64() * 1_000.0,
        decode_latency_p95_ms = percentile(&decode_latencies, 95).as_secs_f64() * 1_000.0,
        render_latency_p50_ms = render.total_latency_p50.as_secs_f64() * 1_000.0,
        render_latency_p95_ms = render.total_latency_p95.as_secs_f64() * 1_000.0,
        "student M3 media complete"
    );
    Ok(
        if !reconnect && control.is_connected() && !control.is_presenting(presentation.stream_id) {
            StudentSessionOutcome::PresentationStopped
        } else if !reconnect && control.is_connected() {
            StudentSessionOutcome::Completed
        } else {
            StudentSessionOutcome::Reconnect
        },
    )
}

fn log_stages(role: &str, metrics: media_core::diagnostics::StageMetrics) {
    for value in metrics.summaries() {
        info!(target: "telemetry",
            role,
            stage = value.stage,
            samples = value.samples,
            retained_samples = value.retained_samples,
            mean_us = value.mean_us,
            p50_us = value.p50_us,
            p95_us = value.p95_us,
            p99_us = value.p99_us,
            max_us = value.max_us,
            "media stage interval"
        );
    }
}

fn media_route_probe(client_id: u64) -> [u8; MEDIA_ROUTE_PROBE_BYTES] {
    let mut probe = [0_u8; MEDIA_ROUTE_PROBE_BYTES];
    probe[..MEDIA_ROUTE_PROBE_MAGIC.len()].copy_from_slice(&MEDIA_ROUTE_PROBE_MAGIC);
    probe[MEDIA_ROUTE_PROBE_MAGIC.len()..].copy_from_slice(&client_id.to_be_bytes());
    probe
}

fn parse_media_route_probe(bytes: &[u8]) -> Option<u64> {
    if bytes.len() != MEDIA_ROUTE_PROBE_BYTES
        || bytes[..MEDIA_ROUTE_PROBE_MAGIC.len()] != MEDIA_ROUTE_PROBE_MAGIC
    {
        return None;
    }
    let client_id = u64::from_be_bytes(bytes[MEDIA_ROUTE_PROBE_MAGIC.len()..].try_into().ok()?);
    (client_id != 0).then_some(client_id)
}

fn send_media_route_probe(
    socket: &UdpSocket,
    teacher: SocketAddrV4,
    client_id: u64,
    probes_sent: &mut u64,
    last_probe: &mut Option<Instant>,
) {
    *last_probe = Some(Instant::now());
    match socket.send_to(&media_route_probe(client_id), teacher) {
        Ok(_) => {
            *probes_sent += 1;
            if *probes_sent == 1 {
                info!(client_id, endpoint = %teacher, "student sent UDP media route probe");
            }
        }
        Err(error) => {
            warn!(client_id, endpoint = %teacher, %error, "student could not send UDP media route probe");
        }
    }
}

fn decode_remote_access_unit(
    decoder: &mut PlatformH264Decoder,
    access_unit: &DepacketizedAccessUnit,
    codec: media_core::VideoCodec,
    decode_latencies: &mut Vec<Duration>,
) -> Result<Vec<DecodedFrame>> {
    let decode_started = Instant::now();
    let timestamp = Duration::from_nanos(
        u64::from(access_unit.timestamp) * 1_000_000_000 / u64::from(RTP_VIDEO_CLOCK_HZ),
    );
    let frames = decoder.decode(EncodedAccessUnitRef {
        bytes: &access_unit.bytes,
        timestamp,
        is_keyframe: access_unit_is_keyframe(access_unit, codec),
    })?;
    bounded_sample(decode_latencies, decode_started.elapsed());
    Ok(frames)
}

/// O2: applies a refinement begin/end to the encoder (bitrate + optional QP cap).
fn apply_refinement<E: VideoEncoder>(
    encoder: &mut E,
    action: refinement::Action,
    qp_supported: &mut Option<bool>,
) {
    let (bitrate_bps, active) = match action {
        refinement::Action::Begin(bps) => (bps, true),
        refinement::Action::End(bps) => (bps, false),
    };
    if let Err(error) = encoder.set_bitrate(bitrate_bps) {
        warn!(%error, bitrate_bps, active, "encoder rejected refinement bitrate");
    }
    if (active && *qp_supported != Some(false)) || (!active && *qp_supported == Some(true)) {
        match encoder.set_refinement(active) {
            Ok(supported) => *qp_supported = Some(supported),
            Err(error) => warn!(%error, active, "encoder refinement QP change failed"),
        }
    }
    info!(bitrate_bps, active, qp_cap = ?qp_supported, "teacher refinement");
}

fn access_unit_is_keyframe(
    access_unit: &DepacketizedAccessUnit,
    codec: media_core::VideoCodec,
) -> bool {
    annex_b_contains_random_access(&access_unit.bytes, codec)
}

#[cfg(test)]
fn annex_b_contains_idr(bytes: &[u8]) -> bool {
    annex_b_contains_random_access(bytes, media_core::VideoCodec::H264)
}

/// True when any NAL unit after a 3- or 4-byte start code begins decoding
/// (H.264 IDR, H.265 IRAP).
fn annex_b_contains_random_access(bytes: &[u8], codec: media_core::VideoCodec) -> bool {
    bytes
        .windows(3)
        .enumerate()
        .any(|(i, w)| w == [0, 0, 1] && codec.is_random_access(&bytes[i + 3..]))
}

fn h264_config(video: &classroom_config::VideoConfig) -> H264Config {
    H264Config {
        width: video.width,
        height: video.height,
        frames_per_second: video.max_fps,
        bitrate: video.target_bitrate_kbps.saturating_mul(1_000),
        keyframe_interval_frames: video.max_fps.saturating_mul(video.keyframe_interval_ms) / 1_000,
    }
}

/// Re-anchor only at a decodable recovery AU boundary. Continuation FU packets
/// and ordinary P frames must not restart NACK work for an obsolete reference chain.
fn recovery_packet_starts_au(packet: &RtpPacket, codec: media_core::VideoCodec) -> bool {
    let p = &packet.payload;
    if packet.payload_type != codec.rtp_payload_type() {
        return false;
    }
    match codec {
        media_core::VideoCodec::H264 => match p.first().map(|b| b & 0x1f) {
            Some(5 | 7 | 8) => true,
            Some(28) if p.len() >= 3 => p[1] & 0x80 != 0 && matches!(p[1] & 0x1f, 5 | 7 | 8),
            _ => false,
        },
        media_core::VideoCodec::H265 => match codec.nal_type(p) {
            Some(16..=21 | 32..=34) => true,
            Some(49) if p.len() >= 4 => {
                p[2] & 0x80 != 0 && matches!(p[2] & 0x3f, 16..=21 | 32..=34)
            }
            _ => false,
        },
    }
}

/// An intentional local drop invalidates compressed references, not the UDP
/// route. Forget old gap repair state so a fresh IDR starts a new sequence base.
fn reset_receiver_reference_chain(
    jitter: &mut JitterBuffer,
    depacketizer: &mut H264Depacketizer,
    timings: &mut VecDeque<(u128, media_core::FrameTiming)>,
) -> usize {
    let discarded = jitter.discard_backlog();
    depacketizer.reset();
    timings.clear();
    discarded
}

fn request_recovery_keyframe(
    control: &control_transport::StudentControl,
    stream_id: u64,
    last_request: &mut Option<Instant>,
    requests: &mut u64,
) -> bool {
    if last_request.is_some_and(|instant| instant.elapsed() < Duration::from_millis(250)) {
        return true;
    }
    if control
        .request_keyframe(protocol::RequestKeyframe { stream_id })
        .is_err()
    {
        return false;
    }
    *last_request = Some(Instant::now());
    *requests += 1;
    true
}

fn host_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .or_else(sysinfo::System::host_name)
        .unwrap_or_else(|| "unknown-host".to_owned())
}

// Session summaries use a bounded recent window; interval telemetry preserves
// every interval and reports its sample count for offline aggregation.
fn bounded_sample(samples: &mut Vec<Duration>, value: Duration) {
    if samples.len() == 4096 {
        samples.remove(0);
    }
    samples.push(value);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_expiration_discards_old_gap_repairs_and_accepts_a_fresh_idr() {
        let now = Instant::now();
        let mut jitter =
            JitterBuffer::with_repair_budget(Duration::from_millis(20), Duration::from_millis(25));
        let mut depacketizer = H264Depacketizer::default();
        let mut timings = VecDeque::new();
        let packet = |sequence_number, timestamp| RtpPacket {
            timing: None,
            marker: true,
            payload_type: H264_PAYLOAD_TYPE,
            sequence_number,
            timestamp,
            ssrc: protocol::presentation_ssrc(1),
            payload: vec![0x65, 0x88],
        };
        let mut probe = packet(1, 1);
        assert!(recovery_packet_starts_au(
            &probe,
            media_core::VideoCodec::H264
        ));
        probe.payload = vec![0x61, 0];
        assert!(!recovery_packet_starts_au(
            &probe,
            media_core::VideoCodec::H264
        ));
        probe.payload = vec![0x7c, 0x85, 1];
        assert!(recovery_packet_starts_au(
            &probe,
            media_core::VideoCodec::H264
        ));
        probe.payload[1] = 5;
        assert!(!recovery_packet_starts_au(
            &probe,
            media_core::VideoCodec::H264
        ));
        probe.payload = vec![0x67, 1];
        assert!(recovery_packet_starts_au(
            &probe,
            media_core::VideoCodec::H264
        ));
        probe.payload_type = 98;
        probe.payload = vec![49 << 1, 1, 0x80 | 19, 1];
        assert!(recovery_packet_starts_au(
            &probe,
            media_core::VideoCodec::H265
        ));
        probe.payload[2] = 19;
        assert!(!recovery_packet_starts_au(
            &probe,
            media_core::VideoCodec::H265
        ));
        jitter.push(packet(10, 1), now);
        let ready = now + Duration::from_millis(20);
        let first = jitter.pop_ready(ready).unwrap();
        assert!(depacketizer.push(&first).unwrap().is_some());
        jitter.push(packet(12, 2), ready);
        assert!(jitter.pop_ready(ready).is_none());
        // Sequence11 was intentionally expired in the ingress queue. Waiting
        // for its retransmission would play an obsolete reference chain.
        assert_eq!(
            reset_receiver_reference_chain(&mut jitter, &mut depacketizer, &mut timings),
            1
        );
        assert!(jitter.take_missing_sequences().is_empty());
        jitter.push(packet(100, 200), ready);
        let idr = jitter.pop_ready(ready + Duration::from_millis(20)).unwrap();
        assert_eq!(idr.sequence_number, 100);
        assert!(depacketizer.push(&idr).unwrap().is_some());
        assert!(jitter.take_missing_sequences().is_empty());
        assert_eq!(jitter.statistics().missing_sequences, 0);
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[allow(clippy::too_many_lines)] // One end-to-end diagnostic scenario owns its setup and assertions.
    #[ignore = "requires macOS Screen Recording permission and runs a real six-second capture/encode/decode loop"]
    fn student_decodes_across_real_teacher_adaptive_bitrate_change() {
        let mut config = Config::load(None).unwrap().0;
        config.video.width = 640;
        config.video.height = 360;
        config.video.target_bitrate_kbps = 1_200;
        config.video.min_bitrate_kbps = 600;
        config.video.max_bitrate_kbps = 1_800;
        config.video.keyframe_interval_ms = 10_000;
        config.video.adaptive_bitrate_cooldown_ms = 1_000;
        let sink = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).unwrap();
        let std::net::SocketAddr::V4(endpoint) = sink.local_addr().unwrap() else {
            unreachable!("IPv4 socket")
        };
        sink.set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        let teacher_socket = bind_teacher_media_socket(Ipv4Addr::LOCALHOST, 0).unwrap();
        sink.send_to(&media_route_probe(1), teacher_socket.local_addr().unwrap())
            .unwrap();
        let decoder_config = h264_config(&config.video);
        let (receiver_done_sender, receiver_done) = std::sync::mpsc::channel();
        let student = thread::spawn(move || {
            let mut decoder = PlatformH264Decoder::new(decoder_config).unwrap();
            let mut depacketizer = H264Depacketizer::default();
            let mut packet_bytes = [0_u8; 1_500];
            let mut keyframes = 0_u64;
            let mut decoded_frames = 0_u64;
            let mut decoded_after_second_keyframe = 0_u64;
            loop {
                match sink.recv_from(&mut packet_bytes) {
                    Ok((size, _)) => {
                        let packet = RtpPacket::parse(&packet_bytes[..size]).unwrap();
                        if let Some(access_unit) = depacketizer.push(&packet).unwrap() {
                            let is_keyframe = annex_b_contains_idr(&access_unit.bytes);
                            if is_keyframe {
                                keyframes += 1;
                            }
                            let frames = decoder
                                .decode(EncodedAccessUnitRef {
                                    bytes: &access_unit.bytes,
                                    timestamp: Duration::from_millis(
                                        u64::from(access_unit.timestamp) / 90,
                                    ),
                                    is_keyframe,
                                })
                                .unwrap();
                            let frame_count = u64::try_from(frames.len()).unwrap();
                            decoded_frames += frame_count;
                            if keyframes >= 2 {
                                decoded_after_second_keyframe += frame_count;
                            }
                        }
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                        ) =>
                    {
                        if receiver_done.try_recv().is_ok() {
                            break;
                        }
                    }
                    Err(error) => panic!("synthetic Student UDP receive failed: {error}"),
                }
            }
            (keyframes, decoded_frames, decoded_after_second_keyframe)
        });
        let (sender, receiver) = std::sync::mpsc::channel();
        sender
            .send(TeacherMediaEvent::Registered(
                control_transport::RegisteredStudent {
                    client_id: 1,
                    endpoint,
                    hostname: "synthetic-loss-test".to_owned(),
                    pointer_overlay: false,
                },
            ))
            .unwrap();
        let report = |received_packets, missing_packets| TeacherMediaEvent::ReceiverReport {
            client_id: 1,
            report: protocol::ReceiverReport {
                stream_id: 1,
                received_packets,
                missing_packets,
                decoded_frames: 0,
                rendered_frames: 0,
                receive_bitrate_bps: 0,
                jitter_delay_ms: 40,
                control_rtt_us: 0,
                control_rtt_samples: 0,
                rendering_enabled: false,
                render_latency_p95_us: 0,
            ..Default::default()
            },
        };
        sender.send(report(0, 0)).unwrap();
        let feeder = thread::spawn(move || {
            for interval in 1..=3 {
                thread::sleep(Duration::from_millis(1_000));
                sender.send(report(interval * 100, interval * 5)).unwrap();
            }
        });
        let mut capture = PlatformCapture::new();
        capture
            .start(CaptureConfig {
                display_index: 0,
                width: config.video.width,
                height: config.video.height,
                max_fps: config.video.max_fps,
            })
            .unwrap();
        let output_dimensions = capture.output_dimensions().unwrap();
        let result = run_teacher_media_loop(
            &config,
            capture,
            &teacher_socket,
            output_dimensions,
            Some(6),
            receiver,
            0,
        );
        feeder.join().unwrap();
        receiver_done_sender.send(()).unwrap();
        let (keyframes, decoded_frames, decoded_after_second_keyframe) = student.join().unwrap();
        let summary = result.unwrap();
        assert!(summary.encoded_frames > 0);
        assert_eq!(summary.bitrate_changes, 1);
        assert_eq!(summary.final_bitrate_bps, 1_020_000);
        assert_eq!(summary.bitrate_change_first_access_unit_keyframes, 1);
        assert!(
            keyframes >= 2,
            "initial and bitrate-switch IDRs are required"
        );
        assert!(decoded_frames > 0);
        assert!(decoded_after_second_keyframe > 0);
    }

    #[test]
    fn media_route_probe_round_trips_client_id_and_rejects_other_datagrams() {
        assert_eq!(parse_media_route_probe(&media_route_probe(42)), Some(42));
        assert_eq!(parse_media_route_probe(&media_route_probe(0)), None);
        assert_eq!(parse_media_route_probe(b"not-an-rtp-probe"), None);
    }

    #[test]
    fn media_route_probe_reveals_the_students_actual_udp_endpoint() {
        let teacher = bind_teacher_media_socket(Ipv4Addr::LOCALHOST, 0).unwrap();
        let student = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).unwrap();
        let expected_student = student.local_addr().unwrap();
        student
            .send_to(&media_route_probe(7), teacher.local_addr().unwrap())
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(1);
        let mut bytes = [0_u8; MEDIA_ROUTE_PROBE_BYTES];
        loop {
            match teacher.recv_from(&mut bytes) {
                Ok((size, source)) => {
                    assert_eq!(parse_media_route_probe(&bytes[..size]), Some(7));
                    assert_eq!(source, expected_student);
                    break;
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "UDP route probe timed out");
                    thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("UDP route probe receive failed: {error}"),
            }
        }
    }

    #[test]
    fn benchmark_modes_validate_arguments_without_changing_role_commands() {
        assert!(Cli::try_parse_from(["Classroom", "--role", "student"]).is_ok());
        assert!(Cli::try_parse_from([
            "Classroom",
            "--benchmark-students",
            "5",
            "--benchmark-output",
            "logs"
        ])
        .is_ok());
        assert!(Cli::try_parse_from([
            "Classroom",
            "--benchmark-report",
            "logs",
            "--expected-students",
            "5"
        ])
        .is_ok());
        assert!(Cli::try_parse_from([
            "Classroom",
            "--benchmark-students",
            "41",
            "--benchmark-output",
            "logs"
        ])
        .is_err());
        assert!(Cli::try_parse_from(["Classroom", "--benchmark-report", "logs"]).is_err());
        assert!(Cli::try_parse_from([
            "Classroom",
            "--role",
            "teacher",
            "--benchmark-students",
            "1",
            "--benchmark-output",
            "logs"
        ])
        .is_err());
    }

    #[test]
    fn native_launcher_is_default_but_explicit_roles_and_diagnostics_bypass_it() {
        let default = Cli::try_parse_from(["Classroom"]).unwrap();
        assert!(should_show_launcher(&default));

        let student = Cli::try_parse_from(["Classroom", "--role", "student"]).unwrap();
        assert!(!should_show_launcher(&student));

        let diagnose = Cli::try_parse_from(["Classroom", "--diagnose"]).unwrap();
        assert!(!should_show_launcher(&diagnose));

        let benchmark = Cli::try_parse_from([
            "Classroom",
            "--benchmark-students",
            "1",
            "--benchmark-output",
            "logs",
        ])
        .unwrap();
        assert!(!should_show_launcher(&benchmark));
    }

    #[test]
    fn automatic_bind_uses_selected_adapter() {
        let selected = Ipv4Addr::new(10, 0, 55, 200);
        assert_eq!(resolve_bind_ip("auto", selected).unwrap(), selected);
    }

    #[test]
    fn explicit_bind_is_supported() {
        assert_eq!(
            resolve_bind_ip("0.0.0.0", Ipv4Addr::LOCALHOST).unwrap(),
            Ipv4Addr::UNSPECIFIED
        );
    }

    #[test]
    fn localhost_teacher_uses_localhost_media_endpoint() {
        assert_eq!(
            student_media_endpoint_ip(Ipv4Addr::LOCALHOST, Ipv4Addr::new(10, 0, 55, 7)),
            Ipv4Addr::LOCALHOST
        );
    }

    #[test]
    fn lan_teacher_uses_selected_adapter_as_media_endpoint() {
        let selected = Ipv4Addr::new(10, 0, 55, 7);
        assert_eq!(
            student_media_endpoint_ip(Ipv4Addr::new(10, 0, 55, 200), selected),
            selected
        );
    }

    #[test]
    fn detects_idr_with_any_valid_nal_priority_bits() {
        assert!(annex_b_contains_idr(&[0, 0, 0, 1, 0x25, 1, 2]));
        assert!(annex_b_contains_idr(&[0, 0, 1, 0x45, 1, 2]));
        assert!(!annex_b_contains_idr(&[0, 0, 0, 1, 0x61, 1, 2]));
    }
}
