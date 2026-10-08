//! Background Teacher coordinator. Private framed loopback IPC, never HTTP/video.
use super::presentation_ids::PresentationIds;
use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use classroom_config::Config;
use control_transport::room::{RoomPresentation, RoomRoster, RoomStudent};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::Write;
use std::{
    collections::{HashMap, HashSet},
    fs,
    net::{Ipv4Addr, SocketAddrV4},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{mpsc, watch},
};
use tracing::{info, warn};

const IPC_VERSION: u32 = 1;
const MAX_FRAME: usize = 8 * 1024 * 1024;
const MAX_COMMAND: usize = 64 * 1024;
const MAX_CLIENTS: usize = 6;
const MAX_OBSERVE: usize = 3;
const MAX_FILE_WINDOWS: usize = 2;
const MAX_ACTIVITY_WINDOWS: usize = 2;
const MAX_INTERNET_WINDOWS: usize = 2;
const MAX_VSCODE_WINDOWS: usize = 2;

#[derive(Clone)]
struct Snapshot {
    rows: Vec<RoomStudent>,
    ip: Ipv4Addr,
    port: u16,
    phase: String,
    scope: String,
    recipients: usize,
    stream_id: u64,
    error: Option<String>,
    tray: bool,
    ui_request: u64,
    observe_focus: (String, u64, u32),
    open_settings: bool,
    observe: Value,
    tools: Value,
    lifecycle: &'static str,
    settings: Value,
    diagnostics: Value,
    student_download: Value,
    presentation: Option<RoomPresentation>,
    source_id: Option<String>,
}
#[derive(Deserialize)]
struct Command {
    #[serde(default)]
    id: u64,
    action: String,
    #[serde(default)]
    selected: Vec<String>,
    #[serde(default)]
    input: Value,
}
struct Request {
    command: Command,
    reply: tokio::sync::oneshot::Sender<Value>,
}
struct Session {
    stop: Arc<AtomicBool>,
    worker: thread::JoinHandle<Result<()>>,
}
struct Endpoint {
    path: PathBuf,
    token: String,
}
impl Drop for Endpoint {
    fn drop(&mut self) {
        if fs::read(&self.path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .is_some_and(|v| v["token"].as_str() == Some(&self.token))
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}
struct ClientCount(Arc<AtomicUsize>);
impl Drop for ClientCount {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

async fn read_frame(socket: &mut TcpStream, max: usize) -> Result<Value> {
    let n = usize::try_from(socket.read_u32().await?)?;
    anyhow::ensure!(n > 0 && n <= max, "invalid local IPC frame length");
    let mut bytes = vec![0; n];
    socket.read_exact(&mut bytes).await?;
    Ok(serde_json::from_slice(&bytes)?)
}
async fn send_frame(socket: &mut TcpStream, value: &Value) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    anyhow::ensure!(bytes.len() <= MAX_FRAME, "local IPC state exceeds bound");
    tokio::time::timeout(Duration::from_secs(3), async {
        socket.write_u32(u32::try_from(bytes.len())?).await?;
        socket.write_all(&bytes).await?;
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("local IPC client write deadline")??;
    Ok(())
}
fn state_json(state: &Snapshot, sent: &mut HashMap<String, u64>) -> Value {
    let students: Vec<_> = state.rows.iter().map(|row| {
        let thumbnail = row.thumbnail.as_ref().filter(|t| sent.get(&row.instance_id) != Some(&t.sequence))
            .map(|t| { sent.insert(row.instance_id.clone(), t.sequence);
                json!({"sequence":t.sequence,"width":t.width,"height":t.height,"jpeg":STANDARD.encode(&t.jpeg)}) });
        let expected = state.presentation.as_ref().is_some_and(|p| p.includes(&row.instance_id));
        let media = row.media.as_ref().filter(|m| expected && m.report.stream_id == state.stream_id);
        let health = if !row.online { "offline" } else if state.source_id.as_deref() == Some(row.instance_id.as_str()) { "presenting_source" }
            else if !expected { "idle" } else { media.map_or("waiting_for_report", |m| m.state()) };
        json!({"id":row.instance_id,"name":row.hostname,"ip":row.endpoint.ip().to_string(),
            "online":row.online,"monitoring":row.monitoring_state,"thumbnail":thumbnail,
            "media":{"state":health,"decode_fps":media.map(|m|m.decode_fps),"render_fps":media.map(|m|m.render_fps),
                "report_age_ms":media.map(|m|m.received_at.elapsed().as_millis()),"receive_mbps":media.map(|m|m.report.receive_bitrate_bps as f64/1_000_000.0),
                "missing_packets":media.map(|m|m.report.missing_packets)},
            "thumbnail_age_ms":row.thumbnail.as_ref().map(|t|t.received_at.elapsed().as_millis())})
    }).collect();
    sent.retain(|id, _| state.rows.iter().any(|r| &r.instance_id == id));
    json!({"type":"state","protocol":IPC_VERSION,"teacher_ip":state.ip.to_string(),
        "control_port":state.port,"phase":state.phase,"scope":state.scope,
        "recipients":state.recipients,"stream_id":state.stream_id,"error":state.error,
        "tray_available":state.tray,"ui_request":state.ui_request,"open_settings":state.open_settings,
        "students":students,"observe":state.observe,"tools":state.tools,"lifecycle":state.lifecycle,
        "settings":state.settings,"diagnostics":state.diagnostics,"student_download":state.student_download,
        "observe_focus":state.observe_focus,"source_id":state.source_id})
}
async fn client(
    mut socket: TcpStream,
    token: Arc<String>,
    mut state: watch::Receiver<Snapshot>,
    commands: mpsc::Sender<Request>,
    clients: Arc<AtomicUsize>,
    consoles: Arc<AtomicUsize>,
) -> Result<()> {
    let hello = tokio::time::timeout(Duration::from_secs(3), read_frame(&mut socket, MAX_COMMAND))
        .await??;
    anyhow::ensure!(
        hello["type"] == "hello"
            && hello["protocol"] == IPC_VERSION
            && hello["token"].as_str() == Some(token.as_str()),
        "local IPC handshake rejected"
    );
    let _owned = ClientCount(Arc::clone(&clients));
    let count = clients.fetch_add(1, Ordering::AcqRel);
    anyhow::ensure!(count < MAX_CLIENTS, "too many console clients");
    // Observe is an independent IPC client, not a main console. Its presence
    // must not prevent the tray/Finder from reopening a closed Teacher window.
    let _console_owned = if hello["ui_role"] == "observe" || hello["ui_role"] == "automation" {
        None
    } else {
        consoles.fetch_add(1, Ordering::AcqRel);
        Some(ClientCount(consoles))
    };
    socket.set_nodelay(true)?;
    let mut sent = HashMap::new();
    let initial = state.borrow_and_update().clone();
    send_frame(&mut socket, &state_json(&initial, &mut sent)).await?;
    info!("Flutter console attached to Teacher agent");
    // One in-flight command per client. A dedicated reader owns partial frames so
    // watch updates never cancel read_exact and desynchronize the socket framing.
    let (mut input, output) = socket.into_split();
    let (tx, mut rx) = mpsc::channel::<Command>(8);
    let reader = tokio::spawn(async move {
        loop {
            let n = input.read_u32().await? as usize;
            anyhow::ensure!(n > 0 && n <= MAX_COMMAND, "invalid IPC command length");
            let mut bytes = vec![0; n];
            input.read_exact(&mut bytes).await?;
            let c = serde_json::from_slice::<Command>(&bytes)?;
            tx.send(c).await.context("IPC coordinator closed")?;
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    });
    let mut output = output;
    let result = async {
        loop {
            let payload = tokio::select! {
                changed = state.changed() => { changed?; state_json(&state.borrow_and_update().clone(), &mut sent) }
                command = rx.recv() => {
                    let Some(command) = command else { break; };
                    let (reply, response) = tokio::sync::oneshot::channel();
                    commands.send(Request {command,reply}).await?;
                    response.await?
                }
            };
            let bytes = serde_json::to_vec(&payload)?;
            anyhow::ensure!(bytes.len() <= MAX_FRAME, "IPC state too large");
            tokio::time::timeout(Duration::from_secs(3), async {
                output.write_u32(u32::try_from(bytes.len())?).await?;
                output.write_all(&bytes).await?;
                Ok::<_, anyhow::Error>(())
            }).await??;
            if payload["lifecycle"].as_str().is_some_and(|v| v != "running") {
                break;
            }
        }
        Ok::<_, anyhow::Error>(())
    }.await;
    reader.abort();
    info!("Flutter console detached; Teacher session continues");
    result
}

#[allow(clippy::too_many_lines)]
pub(super) fn run(
    config: &Config,
    bind_ip: Ipv4Addr,
    display_index: u32,
    path: &Path,
    console: Option<&Path>,
) -> Result<bool> {
    let mut presentation_ids = PresentationIds::new()?;
    info!("Teacher presentation ID epoch initialized");
    let parent = path.parent().context("IPC endpoint parent missing")?;
    fs::create_dir_all(parent)?;
    anyhow::ensure!(
        !path.exists(),
        "IPC endpoint already exists; attach to the running agent"
    );
    let mut random = [0_u8; 32];
    getrandom::fill(&mut random).map_err(|e| anyhow::anyhow!("IPC nonce: {e}"))?;
    let token = Arc::new(STANDARD.encode(random));
    let std_listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let port = std_listener.local_addr()?.port();
    std_listener.set_nonblocking(true)?;
    let listener = TcpListener::from_std(std_listener)?;
    let roster = Arc::new(Mutex::new(RoomRoster::default()));
    let (presentation, updates) = watch::channel::<Option<RoomPresentation>>(None);
    let server = tokio::spawn(control_transport::room::run(
        bind_ip,
        config.network.control_port,
        updates,
        Arc::clone(&roster),
    ));
    let tools_hub = Arc::new(super::classroom_tools::Hub::default());
    tools_hub.set_connection_popup(config.tools.connection_popup);
    let mut diagnostics = super::diagnostics::Collector::new(config, parent)?;
    let tools_server = super::classroom_tools::teacher(
        SocketAddrV4::new(
            bind_ip,
            config
                .network
                .control_port
                .checked_add(2)
                .context("Tools port overflow")?,
        ),
        Arc::clone(&roster),
        Arc::clone(&tools_hub),
        Arc::clone(&diagnostics.service),
    )?;
    let outbox = parent.join("outbox");
    fs::create_dir_all(&outbox)?;
    cleanup_staged_files(&outbox);
    let offer = super::stream_profile::teacher_offer(
        &config.observe,
        super::stream_profile::local_decodable,
    );
    info!(offered = ?offer, "Observe enhanced-profile offer");
    let observe_jobs = Arc::new(super::monitoring::ObserveRequests::with_offer(offer));
    let monitor = config
        .monitoring
        .enabled
        .then(|| {
            super::monitoring::teacher(
                SocketAddrV4::new(bind_ip, config.network.control_port + 1),
                config.monitoring.clone(),
                Arc::clone(&roster),
                Arc::clone(&observe_jobs),
            )
        })
        .transpose()?;
    // Windows tray menus run a nested modal message loop. Keep that loop off
    // the coordinator so opening the menu cannot suspend Stop or Observe leases.
    #[cfg(windows)]
    let mut tray = super::desktop_tray::WindowsTeacherTray::new().ok();
    #[cfg(not(windows))]
    let mut host = minifb::Window::new(
        "Classroom Teacher agent",
        1,
        1,
        minifb::WindowOptions {
            visible: false,
            background: true,
            borderless: true,
            title: false,
            ..Default::default()
        },
    )?;
    #[cfg(not(windows))]
    host.set_target_fps(0);
    #[cfg(not(windows))]
    host.update();
    #[cfg(not(windows))]
    let mut tray = super::desktop_tray::DesktopTray::new(true).ok();
    let mut snapshot = Snapshot {
        rows: vec![],
        ip: bind_ip,
        port: config.network.control_port,
        phase: "idle".into(),
        scope: "class".into(),
        recipients: 0,
        stream_id: 0,
        error: None,
        tray: tray.is_some(),
        ui_request: 0,
        observe_focus: (String::new(), 0, 0),
        open_settings: false,
        observe: Value::Null,
        tools: tools_hub.snapshot(),
        lifecycle: "running",
        settings: serde_json::to_value(config)?,
        diagnostics: diagnostics.snapshot(),
        student_download: json!({"phase":"off"}),
        presentation: None,
        source_id: None,
    };
    let (states, receiver) = watch::channel(snapshot.clone());
    let (commands, mut requests) = mpsc::channel::<Request>(16);
    let clients = Arc::new(AtomicUsize::new(0));
    let connections = Arc::clone(&clients);
    let consoles = Arc::new(AtomicUsize::new(0));
    let console_connections = Arc::clone(&consoles);
    let auth = Arc::clone(&token);
    let ipc = tokio::spawn(async move {
        let mut tasks = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                accepted = listener.accept() => {
                    let (socket,_) = accepted?;
                    // Pending handshake sockets are bounded as well as authenticated clients.
                    if tasks.len() < MAX_CLIENTS { tasks.spawn(client(socket,Arc::clone(&auth),receiver.clone(),commands.clone(),Arc::clone(&connections),Arc::clone(&console_connections))); }
                }
                Some(_) = tasks.join_next(), if !tasks.is_empty() => {}
            }
        }
        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    });
    let endpoint = Endpoint {
        path: path.to_owned(),
        token: (*token).clone(),
    };
    let descriptor =
        json!({"protocol":IPC_VERSION,"port":port,"token":*token,"pid":std::process::id()});
    let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(&serde_json::to_vec(&descriptor)?)?;
    file.sync_all()?;
    drop(file);
    // A freshly launched console must never see half a JSON descriptor.
    fs::rename(&temporary, path)?;
    info!(ipc_port=port,bind_ip=%bind_ip,control_port=config.network.control_port,"Flutter Teacher agent ready");
    let mut student_download: Option<super::student_bootstrap::Share> = None;
    let mut session: Option<Session> = None;
    let mut observe: Option<super::observe_coordinator::View> = None;
    // The persistent coordinator owns UI launches, including windows that are
    // still starting or outlive a reopened main console.
    let mut observe_windows: HashMap<String, std::process::Child> = HashMap::new();
    let mut files_windows: HashMap<String, std::process::Child> = HashMap::new();
    let mut activity_windows: HashMap<String, std::process::Child> = HashMap::new();
    let mut internet_windows: Vec<std::process::Child> = Vec::new();
    let mut vscode_windows: Vec<std::process::Child> = Vec::new();
    let mut publish_due = Instant::now();
    let mut metrics = Instant::now();
    let mut restart_requested = false;
    let mut pending_diagnostics: Option<Vec<String>> = None;
    let result = (|| -> Result<()> {
        'agent: loop {
            diagnostics.poll();
            if let Some(share) = &student_download {
                snapshot.student_download = share.state.lock().unwrap().clone();
            }
            #[cfg(not(windows))]
            host.update();
            if server.is_finished() || ipc.is_finished() {
                anyhow::bail!("Teacher listener stopped; inspect log.txt for bind errors");
            }
            if let Some(tray) = tray.as_mut() {
                tray.update(
                    &format!("Teacher {bind_ip} • {}", snapshot.phase),
                    session.is_none(),
                );
                if let Some(command) = tray.poll() {
                    match command {
                        super::desktop_tray::Command::Exit => break,
                        super::desktop_tray::Command::Restart => {
                            restart_requested = true;
                            break;
                        }
                        super::desktop_tray::Command::Show
                        | super::desktop_tray::Command::Settings => {
                            snapshot.ui_request += 1;
                            snapshot.open_settings =
                                command == super::desktop_tray::Command::Settings;
                            if consoles.load(Ordering::Acquire) == 0 {
                                if let Some(console) = console {
                                    std::process::Command::new(console)
                                        .arg("--ipc-file")
                                        .arg(path)
                                        .spawn()?;
                                }
                            }
                            publish_due = Instant::now();
                        }
                    }
                }
            }
            if session.as_ref().is_some_and(|s| s.worker.is_finished()) {
                let done = session.take().unwrap();
                presentation.send_replace(None);
                snapshot.phase = "idle".into();
                snapshot.source_id = None;
                match done.worker.join() {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        snapshot.error = Some(format!("{e:#}"));
                        warn!(error=?snapshot.error,"Teacher agent presentation failed");
                    }
                    Err(_) => snapshot.error = Some("Media worker stopped unexpectedly".into()),
                }
                if let Some(selected) = pending_diagnostics
                    .take()
                    .or_else(|| config.diagnostics.auto_collect.then(Vec::new))
                {
                    if let Err(error) = diagnostics.start(
                        Arc::clone(&tools_hub),
                        roster.lock().unwrap().snapshot(),
                        &selected,
                        snapshot.stream_id,
                        bind_ip,
                    ) {
                        snapshot.error = Some(format!("{error:#}"));
                    }
                }
                publish_due = Instant::now();
            }
            for _ in 0..32 {
                let Ok(request) = requests.try_recv() else {
                    break;
                };
                let started = Instant::now();
                let command = request.command;
                let id = command.id;
                let mut job_id: Option<String> = None;
                let mut payload: Option<Value> = None;
                let response = match command.action.as_str() {
                    "thumbnail_size" => command.input["width"]
                        .as_u64()
                        .filter(|v| [240, 360, 480].contains(v))
                        .ok_or("Invalid thumbnail size")
                        .and_then(|width| {
                            observe_jobs
                                .set_thumbnail_width(width as u32)
                                .map_err(|_| "Invalid thumbnail size")
                        }),
                    "connection_popup" => {
                        if let Some(enabled) = command.input["enabled"].as_bool() {
                            tools_hub.set_connection_popup(enabled);
                            snapshot.settings["tools"]["connection_popup"] = enabled.into();
                            publish_due = Instant::now();
                            Ok(())
                        } else {
                            Err("Missing popup preference")
                        }
                    }
                    "student_download_stop" => {
                        student_download = None;
                        snapshot.student_download = json!({"phase":"off"});
                        publish_due = Instant::now();
                        Ok(())
                    }
                    "student_download_start" => {
                        if student_download.is_some() {
                            Err("Stop the current download share first")
                        } else {
                            let source = command.input["path"]
                                .as_str()
                                .map(PathBuf::from)
                                .or_else(super::student_bootstrap::bundled_package);
                            let port = command.input["port"].as_u64().unwrap_or(80);
                            if let Some(source) = source {
                                match u16::try_from(port).ok().filter(|p| *p != 0).map(|port| {
                                    super::student_bootstrap::Share::start(
                                        source,
                                        parent.join("student-downloads"),
                                        bind_ip,
                                        port,
                                        config.network.control_port,
                                        tools_hub.connection_popup(),
                                    )
                                }) {
                                    Some(Ok(share)) => {
                                        snapshot.student_download =
                                            share.state.lock().unwrap().clone();
                                        student_download = Some(share);
                                        publish_due = Instant::now();
                                        Ok(())
                                    }
                                    Some(Err(error)) => {
                                        snapshot.error = Some(format!("{error:#}"));
                                        Err("Student download could not start; inspect error or choose another port")
                                    }
                                    None => Err("Download port must be 1..65535"),
                                }
                            } else {
                                Err("Choose a Windows Student dev-update ZIP or bundle student-packages/student-windows-x64.zip")
                            }
                        }
                    }
                    "collect_diagnostics" => {
                        if session.is_some() {
                            pending_diagnostics = Some(command.selected.clone());
                            snapshot.diagnostics = json!({"phase":"waiting_for_stop"});
                            Ok(())
                        } else {
                            match diagnostics.start(
                                Arc::clone(&tools_hub),
                                roster.lock().unwrap().snapshot(),
                                &command.selected,
                                snapshot.stream_id,
                                bind_ip,
                            ) {
                                Ok(()) => Ok(()),
                                Err(error) => {
                                    snapshot.error = Some(format!("{error:#}"));
                                    Err("Diagnostics collection could not start")
                                }
                            }
                        }
                    }
                    "cancel_diagnostics" => {
                        pending_diagnostics = None;
                        diagnostics.cancel();
                        snapshot.diagnostics = diagnostics.snapshot();
                        Ok(())
                    }
                    "observe_window" if snapshot.source_id.is_none() => {
                        observe_windows.retain(|_, child| matches!(child.try_wait(), Ok(None)));
                        let rows = roster.lock().unwrap().snapshot();
                        if let Some(row) = rows
                            .iter()
                            .find(|r| r.online && command.selected.first() == Some(&r.instance_id))
                        {
                            if observe_windows.contains_key(&row.instance_id) {
                                snapshot.observe_focus.0 = row.instance_id.clone();
                                snapshot.observe_focus.1 += 1;
                                snapshot.observe_focus.2 = observe_windows[&row.instance_id].id();
                                publish_due = Instant::now();
                                Ok(())
                            } else if let Some(console) = console {
                                if observe_windows.len() >= MAX_OBSERVE {
                                    Err("Close an Observe window before opening another")
                                } else {
                                    let mut launch = std::process::Command::new(console);
                                    launch
                                        .arg("--observe-id")
                                        .arg(&row.instance_id)
                                        .arg("--observe-name")
                                        .arg(&row.hostname)
                                        .arg("--ipc-file")
                                        .arg(path);
                                    for (input, flag) in [
                                        ("center_x", "--teacher-center-x"),
                                        ("center_y", "--teacher-center-y"),
                                    ] {
                                        if let Some(value) =
                                            command.input[input].as_f64().filter(|v| v.is_finite())
                                        {
                                            launch.arg(flag).arg(value.to_string());
                                        }
                                    }
                                    match launch.spawn() {
                                        Ok(child) => {
                                            snapshot.observe_focus.0 = row.instance_id.clone();
                                            snapshot.observe_focus.1 += 1;
                                            snapshot.observe_focus.2 = child.id();
                                            publish_due = Instant::now();
                                            info!(student_id=%row.instance_id,pid=child.id(),"Observe window launched");
                                            observe_windows.insert(row.instance_id.clone(), child);
                                            Ok(())
                                        }
                                        Err(error) => {
                                            snapshot.error = Some(error.to_string());
                                            Err("Could not open Observe window")
                                        }
                                    }
                                }
                            } else {
                                Err("Teacher console executable unavailable")
                            }
                        } else {
                            Err("Student is offline")
                        }
                    }
                    "focus_observe" => {
                        snapshot.observe_focus.0 =
                            command.selected.first().cloned().unwrap_or_default();
                        snapshot.observe_focus.1 += 1;
                        snapshot.observe_focus.2 = observe_windows
                            .get(&snapshot.observe_focus.0)
                            .map_or(0, std::process::Child::id);
                        publish_due = Instant::now();
                        Ok(())
                    }
                    "show_console" => {
                        snapshot.ui_request += 1;
                        snapshot.open_settings = false;
                        if consoles.load(Ordering::Acquire) == 0 {
                            if let Some(console) = console {
                                std::process::Command::new(console)
                                    .arg("--ipc-file")
                                    .arg(path)
                                    .spawn()?;
                            }
                        }
                        publish_due = Instant::now();
                        Ok(())
                    }
                    "tool_message" | "tool_file" | "tool_update" | "tool_launch" | "tool_power" | "tool_policy" | "tool_vscode_policy" => {
                        if command.action == "tool_update"
                            && (session.is_some() || observe.is_some())
                        {
                            let _ = request.reply.send(json!({"type":"ack","id":id,"ok":false,"error":"Stop sharing and close Observe before dev updates"}));
                            continue;
                        }
                        let rows = roster.lock().unwrap().snapshot();
                        let mut recipients: Vec<String> = rows
                            .iter()
                            .filter(|r| {
                                r.online
                                    && (command.selected.is_empty()
                                        || command.selected.contains(&r.instance_id))
                            })
                            .map(|r| r.instance_id.clone())
                            .collect();
                        // Enforcement (Internet rules, VS Code policy) needs an elevated,
                        // up-to-date Student; apply to every computer that qualifies rather
                        // than refusing the whole batch for one that does not, and report
                        // exactly which were skipped and why.
                        if command.action == "tool_policy" || command.action == "tool_vscode_policy" {
                            let caps = tools_hub.snapshot()["capabilities"].clone();
                            let mut skipped = Vec::new();
                            recipients.retain(|id| {
                                if caps[id]["guard"] == 1 {
                                    true
                                } else {
                                    if let Some(row) = rows.iter().find(|r| &r.instance_id == id) {
                                        skipped.push(json!({"name":row.hostname,"ip":row.endpoint.ip().to_string()}));
                                    }
                                    false
                                }
                            });
                            if !skipped.is_empty() {
                                payload = Some(json!({"skipped": skipped}));
                            }
                            if recipients.is_empty() && !skipped.is_empty() {
                                let _ = request.reply.send(json!({
                                    "type":"ack","id":id,"ok":false,
                                    "error":"No selected computer can enforce this: all need the updated build running as administrator on Windows",
                                    "result":payload,
                                }));
                                continue;
                            }
                        }
                        if command.action == "tool_update" && command.selected.is_empty() {
                            let package = command.input["path"].as_str().map(Path::new);
                            if let Some(m) =
                                package.and_then(|path| super::dev_update::manifest(path).ok())
                            {
                                let caps = tools_hub.snapshot()["capabilities"].clone();
                                recipients.retain(|id| {
                                    caps[id]["dev_update"] == 1
                                        && caps[id]["os"] == m.os
                                        && caps[id]["arch"] == m.arch
                                });
                            }
                        }
                        match tools_hub.submit_with_limit(
                            if command.action == "tool_update" {
                                "update"
                            } else if command.action == "tool_file" {
                                "file"
                            } else if command.action == "tool_launch" {
                                "launch"
                            } else if command.action == "tool_power" {
                                "power"
                            } else if command.action == "tool_policy" {
                                "policy"
                            } else if command.action == "tool_vscode_policy" {
                                "vscode_policy"
                            } else {
                                "message"
                            },
                            &recipients,
                            &command.input,
                            &outbox,
                            u64::from(config.tools.max_file_mb) * 1024 * 1024,
                        ) {
                            Ok(job) => {
                                job_id = Some(job);
                                publish_due = Instant::now();
                                Ok(())
                            }
                            Err(e) => {
                                snapshot.error = Some(format!("{e:#}"));
                                Err("Classroom tool could not be sent; see details")
                            }
                        }
                    }
                    "tool_cancel" | "tool_retry" => {
                        let job = command.input["id"].as_str().unwrap_or("");
                        let result = if command.action == "tool_cancel" {
                            tools_hub.cancel(job)
                        } else {
                            tools_hub.retry_with_limit(
                                job,
                                &outbox,
                                u64::from(config.tools.max_file_mb) * 1024 * 1024,
                            )
                        };
                        match result {
                            Ok(()) => {
                                publish_due = Instant::now();
                                Ok(())
                            }
                            Err(e) => {
                                snapshot.error = Some(format!("{e:#}"));
                                Err("Classroom job could not be updated")
                            }
                        }
                    }
                    "fs_collect" | "fs_list" | "fs_result" | "fs_transfer" | "fs_cancel" | "fs_open" | "activity_request" | "activity_monitoring_set" | "vscode_list" => {
                        let rows = roster.lock().unwrap().snapshot();
                        let input = &command.input;
                        let outcome: Result<Option<Value>> = (|| match command.action.as_str() {
                            "fs_collect" => {
                                let recipients: Vec<(String, String)> = rows
                                    .iter()
                                    .filter(|r| {
                                        r.online
                                            && (command.selected.is_empty()
                                                || command.selected.contains(&r.instance_id))
                                    })
                                    .map(|r| (r.instance_id.clone(), r.hostname.clone()))
                                    .collect();
                                let root = super::classroom_tools::collection_root(
                                    input["label"].as_str().unwrap_or(""),
                                )?;
                                job_id = Some(tools_hub.fs_collect(&recipients, input, &root)?);
                                Ok(Some(json!({"folder":root,"students":recipients.len()})))
                            }
                            "fs_list" => {
                                let hidden = input["hidden"].as_bool().unwrap_or(false);
                                let path = input["path"].as_str().unwrap_or("");
                                let list = if input["side"] == "student" {
                                    let student = input["student"].as_str().unwrap_or("");
                                    anyhow::ensure!(
                                        rows.iter().any(|r| r.online && r.instance_id == student),
                                        "Student is offline"
                                    );
                                    tools_hub.fs_list_remote(student, path, hidden)?
                                } else {
                                    tools_hub.fs_list_local(path, hidden)?
                                };
                                job_id = Some(list);
                                Ok(None)
                            }
                            "fs_result" => Ok(Some(
                                tools_hub.fs_list_result(input["id"].as_str().unwrap_or("")),
                            )),
                            "activity_request" => {
                                let student = input["student"].as_str().unwrap_or("");
                                anyhow::ensure!(
                                    rows.iter().any(|r| r.online && r.instance_id == student),
                                    "Student is offline"
                                );
                                let (queued, cached) = tools_hub.activity_view(student)?;
                                job_id = queued;
                                Ok(Some(json!({"cached": cached})))
                            }
                            "fs_transfer" => {
                                let student = input["student"].as_str().unwrap_or("");
                                let row = rows
                                    .iter()
                                    .find(|r| r.online && r.instance_id == student)
                                    .context("Student is offline")?;
                                let paths: Vec<String> = input["paths"]
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                                    .filter_map(|p| p.as_str().map(str::to_owned))
                                    .collect();
                                job_id = Some(tools_hub.fs_transfer(
                                    input["push"].as_bool().unwrap_or(false),
                                    (&row.instance_id, &row.hostname),
                                    &paths,
                                    input["dest"].as_str().unwrap_or(""),
                                )?);
                                Ok(None)
                            }
                            "fs_cancel" => {
                                tools_hub.fs_cancel(input["id"].as_str().unwrap_or(""))?;
                                Ok(None)
                            }
                            "activity_monitoring_set" => {
                                tools_hub.set_activity_monitoring(input["enabled"].as_bool().unwrap_or(false));
                                publish_due = Instant::now();
                                Ok(None)
                            }
                            "vscode_list" => {
                                let student = input["student"].as_str().unwrap_or("");
                                anyhow::ensure!(
                                    rows.iter().any(|r| r.online && r.instance_id == student),
                                    "Student is offline"
                                );
                                job_id = Some(tools_hub.vscode_list(student)?);
                                Ok(None)
                            }
                            _ => {
                                // Only folders a file job created or targets may be revealed.
                                let wanted = input["path"].as_str().unwrap_or("");
                                let known = tools_hub.snapshot()["fs_jobs"]
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                                    .any(|job| {
                                        job["dest"] == wanted
                                            || job["targets"].as_array().into_iter().flatten().any(
                                                |t| t["folder"] == wanted,
                                            )
                                    });
                                anyhow::ensure!(known, "Unknown folder");
                                std::process::Command::new(if cfg!(windows) {
                                    "explorer"
                                } else {
                                    "open"
                                })
                                .arg(wanted)
                                .spawn()?;
                                Ok(None)
                            }
                        })();
                        match outcome {
                            Ok(value) => {
                                payload = value;
                                publish_due = Instant::now();
                                Ok(())
                            }
                            Err(error) => {
                                payload = Some(json!({"error_detail":format!("{error:#}")}));
                                Err("File operation failed")
                            }
                        }
                    }
                    "internet_window" => {
                        internet_windows.retain_mut(|child| matches!(child.try_wait(), Ok(None)));
                        let rows = roster.lock().unwrap().snapshot();
                        let ids: Vec<&str> = rows
                            .iter()
                            .filter(|r| r.online && command.selected.contains(&r.instance_id))
                            .map(|r| r.instance_id.as_str())
                            .collect();
                        if ids.is_empty() {
                            Err("Select at least one online Student")
                        } else if let Some(child) = internet_windows.first() {
                            // An Internet access window is already open: bring
                            // it forward instead of silently doing nothing (it
                            // does not retarget to a different audience — close
                            // it first for that, same as before).
                            snapshot.observe_focus.0 = "internet".into();
                            snapshot.observe_focus.1 += 1;
                            snapshot.observe_focus.2 = child.id();
                            publish_due = Instant::now();
                            Ok(())
                        } else if internet_windows.len() >= MAX_INTERNET_WINDOWS {
                            Err("Close an Internet access window before opening another")
                        } else if let Some(console) = console {
                            let audience = command.input["audience"].as_str().unwrap_or("the class");
                            match std::process::Command::new(console)
                                .arg("--internet-ids")
                                .arg(ids.join(","))
                                .arg("--internet-audience")
                                .arg(audience)
                                .arg("--ipc-file")
                                .arg(path)
                                .spawn()
                            {
                                Ok(child) => {
                                    info!(students = ids.len(), pid = child.id(), "Internet access window launched");
                                    internet_windows.push(child);
                                    publish_due = Instant::now();
                                    Ok(())
                                }
                                Err(error) => {
                                    snapshot.error = Some(error.to_string());
                                    Err("Could not open Internet access window")
                                }
                            }
                        } else {
                            Err("Teacher console executable unavailable")
                        }
                    }
                    "vscode_window" => {
                        vscode_windows.retain_mut(|child| matches!(child.try_wait(), Ok(None)));
                        let rows = roster.lock().unwrap().snapshot();
                        let ids: Vec<&str> = rows
                            .iter()
                            .filter(|r| r.online && command.selected.contains(&r.instance_id))
                            .map(|r| r.instance_id.as_str())
                            .collect();
                        if ids.is_empty() {
                            Err("Select at least one online Student")
                        } else if let Some(child) = vscode_windows.first() {
                            // Already open: bring it forward (same simplification
                            // as internet_window — does not retarget audience).
                            snapshot.observe_focus.0 = "vscode".into();
                            snapshot.observe_focus.1 += 1;
                            snapshot.observe_focus.2 = child.id();
                            publish_due = Instant::now();
                            Ok(())
                        } else if vscode_windows.len() >= MAX_VSCODE_WINDOWS {
                            Err("Close a VS Code control window before opening another")
                        } else if let Some(console) = console {
                            let audience = command.input["audience"].as_str().unwrap_or("the class");
                            match std::process::Command::new(console)
                                .arg("--vscode-ids")
                                .arg(ids.join(","))
                                .arg("--vscode-audience")
                                .arg(audience)
                                .arg("--ipc-file")
                                .arg(path)
                                .spawn()
                            {
                                Ok(child) => {
                                    info!(students = ids.len(), pid = child.id(), "VS Code control window launched");
                                    vscode_windows.push(child);
                                    publish_due = Instant::now();
                                    Ok(())
                                }
                                Err(error) => {
                                    snapshot.error = Some(error.to_string());
                                    Err("Could not open VS Code control window")
                                }
                            }
                        } else {
                            Err("Teacher console executable unavailable")
                        }
                    }
                    "activity_window" => {
                        activity_windows.retain(|_, child| matches!(child.try_wait(), Ok(None)));
                        let rows = roster.lock().unwrap().snapshot();
                        let Some(row) = rows
                            .iter()
                            .find(|r| r.online && command.selected.first() == Some(&r.instance_id))
                        else {
                            let _ = request.reply.send(json!({"type":"ack","id":id,"ok":false,"error":"Student is offline"}));
                            continue;
                        };
                        if let Some(child) = activity_windows.get(&row.instance_id) {
                            snapshot.observe_focus.0 = format!("activity:{}", row.instance_id);
                            snapshot.observe_focus.1 += 1;
                            snapshot.observe_focus.2 = child.id();
                            publish_due = Instant::now();
                            Ok(())
                        } else if activity_windows.len() >= MAX_ACTIVITY_WINDOWS {
                            Err("Close an Activity window before opening another")
                        } else if let Some(console) = console {
                            match std::process::Command::new(console)
                                .arg("--activity-id")
                                .arg(&row.instance_id)
                                .arg("--activity-name")
                                .arg(&row.hostname)
                                .arg("--ipc-file")
                                .arg(path)
                                .spawn()
                            {
                                Ok(child) => {
                                    snapshot.observe_focus.0 = format!("activity:{}", row.instance_id);
                                    snapshot.observe_focus.1 += 1;
                                    snapshot.observe_focus.2 = child.id();
                                    publish_due = Instant::now();
                                    info!(student_id=%row.instance_id,pid=child.id(),"Activity window launched");
                                    activity_windows.insert(row.instance_id.clone(), child);
                                    Ok(())
                                }
                                Err(error) => {
                                    snapshot.error = Some(error.to_string());
                                    Err("Could not open Activity window")
                                }
                            }
                        } else {
                            Err("Teacher console executable unavailable")
                        }
                    }
                    "files_window" => {
                        files_windows.retain(|_, child| matches!(child.try_wait(), Ok(None)));
                        let rows = roster.lock().unwrap().snapshot();
                        let Some(row) = rows
                            .iter()
                            .find(|r| r.online && command.selected.first() == Some(&r.instance_id))
                        else {
                            let _ = request.reply.send(json!({"type":"ack","id":id,"ok":false,"error":"Student is offline"}));
                            continue;
                        };
                        if let Some(child) = files_windows.get(&row.instance_id) {
                            snapshot.observe_focus.0 = format!("files:{}", row.instance_id);
                            snapshot.observe_focus.1 += 1;
                            snapshot.observe_focus.2 = child.id();
                            publish_due = Instant::now();
                            Ok(())
                        } else if files_windows.len() >= MAX_FILE_WINDOWS {
                            Err("Close a file window before opening another")
                        } else if let Some(console) = console {
                            match std::process::Command::new(console)
                                .arg("--files-id")
                                .arg(&row.instance_id)
                                .arg("--files-name")
                                .arg(&row.hostname)
                                .arg("--ipc-file")
                                .arg(path)
                                .spawn()
                            {
                                Ok(child) => {
                                    snapshot.observe_focus.0 = format!("files:{}", row.instance_id);
                                    snapshot.observe_focus.1 += 1;
                                    snapshot.observe_focus.2 = child.id();
                                    publish_due = Instant::now();
                                    info!(student_id=%row.instance_id,pid=child.id(),"File window launched");
                                    files_windows.insert(row.instance_id.clone(), child);
                                    Ok(())
                                }
                                Err(error) => {
                                    snapshot.error = Some(error.to_string());
                                    Err("Could not open file window")
                                }
                            }
                        } else {
                            Err("Teacher console executable unavailable")
                        }
                    }
                    "observe_open" if snapshot.source_id.is_none() => {
                        let rows = roster.lock().unwrap().snapshot();
                        if let Some(row) = rows
                            .iter()
                            .find(|r| r.online && command.selected.first() == Some(&r.instance_id))
                        {
                            observe.take();
                            match super::observe_coordinator::View::start(
                                row.instance_id.clone(),
                                row.endpoint.ip().to_string(),
                                parent,
                                Arc::clone(&observe_jobs),
                            ) {
                                Ok(view) => {
                                    observe = Some(view);
                                    // Publish the surface path immediately with
                                    // the command acknowledgement. Observe's
                                    // Flutter window can otherwise receive the
                                    // ack before the next 500 ms state tick and
                                    // miss the one-shot texture attachment.
                                    snapshot.observe = observe.as_ref().map_or(
                                        Value::Null,
                                        super::observe_coordinator::View::state,
                                    );
                                    Ok(())
                                }
                                Err(e) => {
                                    snapshot.error = Some(format!("{e:#}"));
                                    Err("Cannot start Observe")
                                }
                            }
                        } else {
                            Err("Student is offline")
                        }
                    }
                    "observe_input" => {
                        if let Some(view) = observe
                            .as_ref()
                            .filter(|v| command.selected.first() == Some(&v.id))
                        {
                            if view.send(command.input).is_ok() {
                                Ok(())
                            } else {
                                observe.take();
                                Err("Observe input queue closed")
                            }
                        } else {
                            Err("Observe session is no longer active")
                        }
                    }
                    "observe_close" => {
                        if observe.as_ref().is_some_and(|v| {
                            command.selected.first() == Some(&v.id)
                                && command
                                    .input
                                    .get("surface")
                                    .is_none_or(|surface| surface == &v.state()["surface"])
                        }) {
                            observe.take();
                        }
                        Ok(())
                    }
                    "share_student" if session.is_none() => {
                        observe_windows.retain(|_, child| matches!(child.try_wait(), Ok(None)));
                        let rows = roster.lock().unwrap().snapshot();
                        let source = rows.iter().find(|r| r.online && command.selected.len() == 1 && command.selected[0] == r.instance_id);
                        if !config.monitoring.enabled {
                            Err("Enable monitoring to request Student screen publishing")
                        } else if observe.is_some() || !observe_windows.is_empty() {
                            Err("Close Observe/Control windows before presenting a Student")
                        } else if let Some(source) = source {
                            diagnostics.cancel();
                            let recipients = Arc::new(rows.iter().filter(|r| r.online && r.instance_id != source.instance_id)
                                .map(|r| r.instance_id.clone()).collect::<HashSet<_>>());
                            snapshot.stream_id = presentation_ids.next()?;
                            snapshot.source_id = Some(source.instance_id.clone());
                            snapshot.scope = "student_class".into();
                            snapshot.recipients = recipients.len();
                            snapshot.phase = "starting".into();
                            snapshot.error = None;
                            let stop = Arc::new(AtomicBool::new(false));
                            let cancel = Arc::clone(&stop);
                            let settings = config.clone();
                            let state = presentation.clone();
                            let stream = snapshot.stream_id;
                            let source_id = source.instance_id.clone();
                            let source_ip = *source.endpoint.ip();
                            let jobs = Arc::clone(&observe_jobs);
                            let runtime = tokio::runtime::Handle::current();
                            session = Some(Session { stop, worker: thread::spawn(move || {
                                super::student_broadcast::run(&settings, bind_ip, source_id, source_ip, stream,
                                    &cancel, &state, jobs, runtime)
                            }) });
                            Ok(())
                        } else { Err("Select exactly one online Student as the presentation source") }
                    }
                    "share_class" | "share_selected" if session.is_none() => {
                        diagnostics.cancel();
                        let rows = roster.lock().unwrap().snapshot();
                        let selected: HashSet<_> = command.selected.into_iter().collect();
                        let recipients = (command.action == "share_selected").then(|| {
                            Arc::new(
                                rows.iter()
                                    .filter(|r| r.online && selected.contains(&r.instance_id))
                                    .map(|r| r.instance_id.clone())
                                    .collect::<HashSet<_>>(),
                            )
                        });
                        if recipients.as_ref().is_some_and(|ids| ids.is_empty()) {
                            Err("Select an online Student first")
                        } else {
                            snapshot.stream_id = presentation_ids.next()?;
                            snapshot.scope = if recipients.is_some() {
                                "selected"
                            } else {
                                "class"
                            }
                            .into();
                            snapshot.recipients = recipients.as_ref().map_or_else(
                                || rows.iter().filter(|r| r.online).count(),
                                |ids| ids.len(),
                            );
                            snapshot.phase = "starting".into();
                            snapshot.source_id = None;
                            snapshot.error = None;
                            let stop = Arc::new(AtomicBool::new(false));
                            let cancel = Arc::clone(&stop);
                            let settings = config.clone();
                            let state = presentation.clone();
                            let stream = snapshot.stream_id;
                            session = Some(Session {
                                stop,
                                worker: thread::spawn(move || {
                                    super::teacher_dashboard::present(
                                        &settings,
                                        bind_ip,
                                        display_index,
                                        stream,
                                        &cancel,
                                        &state,
                                        recipients.as_ref(),
                                    )
                                }),
                            });
                            info!(stream_id=snapshot.stream_id,scope=%snapshot.scope,recipient_count=snapshot.recipients,"teacher dashboard Start Sharing");
                            Ok(())
                        }
                    }
                    "stop" => {
                        if let Some(active) = session.as_ref() {
                            active.stop.store(true, Ordering::Release);
                            presentation.send_replace(None);
                            snapshot.phase = "stopping".into();
                            info!(
                                stream_id = snapshot.stream_id,
                                "teacher dashboard Stop Sharing"
                            );
                        }
                        Ok(())
                    }
                    "quit" | "restart" | "reconfigure" => {
                        restart_requested = command.action == "restart";
                        if command.action == "reconfigure" {
                            snapshot.lifecycle = "reconfigure";
                        }
                        let _ = request.reply.send(json!({"type":"ack","id":id,"ok":true}));
                        break 'agent;
                    }
                    "rescan" => {
                        // Registration and heartbeat are continuously active. Refresh the
                        // authoritative roster without interrupting an existing stream.
                        snapshot.rows = roster.lock().unwrap().snapshot();
                        snapshot.tools = tools_hub.snapshot();
                        info!(
                            online = snapshot.rows.iter().filter(|r| r.online).count(),
                            "Teacher roster rescan"
                        );
                        Ok(())
                    }
                    "clear_error" => {
                        snapshot.error = None;
                        Ok(())
                    }
                    _ => Err("Command unavailable in the current presentation state"),
                };
                let _ = request.reply.send(
                    json!({"type":"ack","id":id,"ok":response.is_ok(),"error":response.err(),"job_id":job_id,"result":payload}),
                );
                if command.action != "observe_input" {
                    info!(id,action=%command.action,elapsed_ms=started.elapsed().as_secs_f64()*1000.0,"Flutter agent command processed");
                    publish_due = Instant::now();
                }
            }
            if snapshot.phase == "starting" && presentation.borrow().is_some() {
                snapshot.phase = "sharing".into();
                publish_due = Instant::now();
            }
            if observe
                .as_ref()
                .is_some_and(super::observe_coordinator::View::is_live)
                && snapshot.observe["phase"] != "live"
            {
                publish_due = Instant::now();
            }
            if observe
                .as_ref()
                .and_then(super::observe_coordinator::View::cursor_serial)
                != snapshot.observe["cursor"]["serial"].as_u64()
            {
                publish_due = Instant::now();
            }
            if Instant::now() >= publish_due {
                snapshot.observe = observe
                    .as_ref()
                    .map_or(Value::Null, super::observe_coordinator::View::state);
                snapshot.rows = roster.lock().unwrap().snapshot();
                snapshot.presentation = presentation.borrow().clone();
                snapshot.tools = tools_hub.snapshot();
                snapshot.tools["activity_monitoring"] = json!(tools_hub.activity_monitoring());
                let new_alerts = tools_hub.activity_alerts();
                if !new_alerts.is_empty() {
                    snapshot.tools["new_alerts"] = json!(new_alerts);
                }
                if pending_diagnostics.is_none() {
                    snapshot.diagnostics = diagnostics.snapshot();
                }
                states.send_replace(snapshot.clone());
                publish_due = Instant::now() + Duration::from_millis(500);
            }
            if metrics.elapsed() >= Duration::from_secs(5) {
                info!(ipc_clients=clients.load(Ordering::Acquire),online=snapshot.rows.iter().filter(|r|r.online).count(),phase=%snapshot.phase,"Flutter agent interval");
                metrics = Instant::now();
            }
            // Interactive input must not wait for the roster's 500 ms refresh
            // or a 20 ms coordinator poll. All work and queues remain bounded.
            thread::sleep(Duration::from_millis(2));
        }
        Ok(())
    })();
    if snapshot.lifecycle == "running" {
        snapshot.lifecycle = if restart_requested { "restart" } else { "exit" };
    }
    if let Some(active) = session {
        active.stop.store(true, Ordering::Release);
        presentation.send_replace(None);
        let _ = active.worker.join();
    }
    drop(observe);
    drop(diagnostics);
    server.abort();
    drop(tools_server);
    drop(monitor);
    let deadline = Instant::now() + Duration::from_secs(1);
    while !server.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    drop(endpoint);
    // Notify dependent windows after releasing media/ports and removing the
    // descriptor, so an immediate double-click starts a fresh, usable room.
    // EOF alone is not Exit: the console recovers from an unexpected crash.
    snapshot.phase = "stopping".into();
    states.send_replace(snapshot);
    let deadline = Instant::now() + Duration::from_secs(1);
    while clients.load(Ordering::Acquire) != 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    // Also cover a freshly spawned UI that has not attached to IPC yet.
    for child in observe_windows.values_mut() {
        if matches!(child.try_wait(), Ok(None)) {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    ipc.abort();
    info!("Flutter Teacher agent stopped; room and presentation released");
    result.map(|()| restart_requested)
}

fn cleanup_staged_files(outbox: &Path) {
    let Ok(entries) = fs::read_dir(outbox) else {
        return;
    };
    let mut removed = 0_u32;
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if kind.is_file()
            && metadata
                .modified()
                .ok()
                .and_then(|time| time.elapsed().ok())
                .is_some_and(|age| age > Duration::from_secs(24 * 60 * 60))
            && fs::remove_file(entry.path()).is_ok()
        {
            removed += 1;
        }
    }
    if removed > 0 {
        info!(removed, "Removed stale staged classroom files");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn rejects_oversized_prefix_before_allocating_body() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let sender = tokio::spawn(async move {
            let mut s = TcpStream::connect(address).await.unwrap();
            s.write_u32(u32::try_from(MAX_COMMAND + 1).unwrap())
                .await
                .unwrap();
        });
        let (mut socket, _) = listener.accept().await.unwrap();
        assert!(read_frame(&mut socket, MAX_COMMAND)
            .await
            .unwrap_err()
            .to_string()
            .contains("invalid local IPC"));
        sender.await.unwrap();
    }
}
