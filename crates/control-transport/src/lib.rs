pub mod room;

/// A live presentation must never hold the desktop indefinitely after a silent
/// network failure. Students probe once per second, including static screens.
const CONTROL_LIVENESS_TIMEOUT: Duration = Duration::from_secs(5);
use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use prost::Message;
use protocol::{
    envelope::Payload, Envelope, HelloStudent, Nack, ReceiverReport, RequestKeyframe,
    StartPresentation, MAX_CONTROL_FRAME_BYTES, PROTOCOL_VERSION,
};
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tracing::{info, warn};

#[derive(Clone, Debug)]
pub struct StudentRegistration {
    pub hostname: String,
    pub instance_id: String,
    pub selected_local_ip: Ipv4Addr,
    pub media_udp_port: u16,
    pub feedback_udp_port: u16,
    /// This receiver renders the Teacher pointer channel (`HelloStudent.pointer_overlay`).
    pub pointer_overlay: bool,
}

#[derive(Clone, Debug)]
pub struct RegisteredStudent {
    pub client_id: u64,
    pub endpoint: SocketAddrV4,
    pub hostname: String,
    pub pointer_overlay: bool,
}

#[derive(Clone, Debug)]
pub enum TeacherMediaEvent {
    Registered(RegisteredStudent),
    Disconnected {
        client_id: u64,
        endpoint: SocketAddrV4,
    },
    Nack {
        client_id: u64,
        endpoint: SocketAddrV4,
        message: Nack,
    },
    KeyframeRequested {
        client_id: u64,
        message: RequestKeyframe,
    },
    ReceiverReport {
        client_id: u64,
        report: ReceiverReport,
    },
}

struct RegisteredStudentGuard {
    client_id: u64,
    endpoint: SocketAddrV4,
    events: std::sync::mpsc::Sender<TeacherMediaEvent>,
}

impl Drop for RegisteredStudentGuard {
    fn drop(&mut self) {
        let _ = self.events.send(TeacherMediaEvent::Disconnected {
            client_id: self.client_id,
            endpoint: self.endpoint,
        });
    }
}

#[derive(Clone, Debug)]
pub struct StudentControl {
    sender: tokio::sync::mpsc::Sender<Envelope>,
    rtt: Arc<Mutex<ControlRtt>>,
    connected: Arc<AtomicBool>,
    presentation: Arc<Mutex<Option<StartPresentation>>>,
    input_status: Arc<Mutex<Option<protocol::ObserveMessage>>>,
    cursor_status: Arc<Mutex<Option<protocol::ObserveMessage>>>,
    _tasks: Arc<StudentControlTasks>,
}

#[derive(Debug)]
struct StudentControlTasks {
    reader: tokio::task::AbortHandle,
    writer: tokio::task::AbortHandle,
    connected: Arc<AtomicBool>,
}
impl Drop for StudentControlTasks {
    fn drop(&mut self) {
        self.connected.store(false, Ordering::Release);
        self.reader.abort();
        self.writer.abort();
    }
}

#[derive(Debug, Default)]
struct ControlRtt {
    next_nonce: u64,
    pending: Option<(u64, Instant)>,
    last_micros: u64,
    samples: u64,
    clock: Option<ClockEstimate>,
    clock_at: Option<Instant>,
}

#[derive(Clone, Copy, Debug)]
pub struct ClockEstimate {
    pub teacher_minus_student_us: i64,
    pub uncertainty_us: u64,
}
impl ClockEstimate {
    #[must_use]
    pub fn source_age(self, teacher_us: u64) -> Option<Duration> {
        if teacher_us == 0 {
            return None;
        }
        let age = i128::from(media_core::unix_us()) + i128::from(self.teacher_minus_student_us)
            - i128::from(teacher_us);
        u64::try_from(age).ok().map(Duration::from_micros)
    }
}

impl ControlRtt {
    fn receive_pong(&mut self, nonce: u64, now: Instant) {
        if let Some((expected, sent_at)) = self.pending {
            if nonce == expected {
                self.last_micros =
                    u64::try_from(now.saturating_duration_since(sent_at).as_micros())
                        .unwrap_or(u64::MAX);
                self.samples += 1;
                self.pending = None;
            }
        }
    }
}

impl StudentControl {
    /// Requests one TCP control round-trip sample. A stalled response cannot
    /// accumulate probes: at most one is outstanding for three seconds.
    ///
    /// # Errors
    /// Returns an error when the control connection has closed.
    pub fn probe_rtt(&self) -> Result<(), ControlError> {
        let now = Instant::now();
        let mut state = self.rtt.lock().map_err(|_| ControlError::ControlClosed)?;
        if state
            .pending
            .is_some_and(|(_, sent)| now.saturating_duration_since(sent) < Duration::from_secs(3))
        {
            return Ok(());
        }
        state.next_nonce = state.next_nonce.wrapping_add(1);
        let nonce = state.next_nonce;
        let mut ping = Envelope::control_ping(nonce);
        if let Some(Payload::ControlPing(message)) = &mut ping.payload {
            message.sender_unix_us = media_core::unix_us();
        }
        self.send(ping)?;
        state.pending = Some((nonce, now));
        Ok(())
    }

    #[must_use]
    pub fn rtt_sample(&self) -> (u64, u64) {
        self.rtt
            .lock()
            .map_or((0, 0), |state| (state.samples, state.last_micros))
    }

    #[must_use]
    pub fn clock_estimate(&self) -> Option<ClockEstimate> {
        let state = self.rtt.lock().ok()?;
        if state
            .clock_at
            .is_some_and(|at| at.elapsed() < Duration::from_secs(10))
        {
            state.clock
        } else {
            None
        }
    }

    #[must_use]
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    #[must_use]
    pub fn presentation(&self) -> Option<StartPresentation> {
        self.presentation
            .lock()
            .ok()
            .and_then(|value| value.clone())
    }

    #[must_use]
    pub fn is_presenting(&self, stream_id: u64) -> bool {
        self.is_connected()
            && self
                .presentation
                .lock()
                .is_ok_and(|value| value.as_ref().is_some_and(|p| p.stream_id == stream_id))
    }

    /// Queues one receiver-health report for the teacher control connection.
    ///
    /// # Errors
    /// Returns an error when the teacher control connection has closed.
    pub fn report(&self, report: ReceiverReport) -> Result<(), ControlError> {
        self.send(Envelope::receiver_report(report))
    }

    /// Queues an RTP/H.264 NACK for recent packets that are still useful.
    ///
    /// # Errors
    /// Returns an error when the teacher control connection has closed.
    pub fn nack(&self, nack: Nack) -> Result<(), ControlError> {
        self.send(Envelope::nack(nack))
    }

    /// Requests an independently decodable H.264 access unit from the teacher.
    ///
    /// # Errors
    /// Returns an error when the teacher control connection has closed.
    pub fn request_keyframe(&self, request: RequestKeyframe) -> Result<(), ControlError> {
        self.send(Envelope::request_keyframe(request))
    }

    #[must_use]
    pub fn take_cursor_status(&self) -> Option<protocol::ObserveMessage> {
        self.cursor_status.lock().ok()?.take()
    }
    pub fn take_input_status(&self) -> Option<protocol::ObserveMessage> {
        self.input_status.lock().ok()?.take()
    }
    /// Send bounded input to a session-authorized reverse publisher.
    /// # Errors
    /// Returns an error when the connection/queue is unavailable.
    pub fn observe_input(&self, input: protocol::ObserveMessage) -> Result<(), ControlError> {
        self.send(Envelope {
            protocol_version: PROTOCOL_VERSION,
            payload: Some(Payload::Observe(input)),
        })
    }

    fn send(&self, envelope: Envelope) -> Result<(), ControlError> {
        if !self.is_connected() {
            return Err(ControlError::ControlClosed);
        }
        self.sender
            .try_send(envelope)
            .map_err(|_| ControlError::ControlClosed)
    }
}

#[derive(Debug, Error)]
pub enum ControlError {
    #[error("control I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("control message decode failed: {0}")]
    Decode(#[from] prost::DecodeError),
    #[error("control frame of {0} bytes exceeds the limit")]
    FrameTooLarge(usize),
    #[error("unsupported protocol version {0}")]
    Version(u32),
    #[error("unexpected control message")]
    UnexpectedMessage,
    #[error("control connection closed")]
    ControlClosed,
}

/// Runs the M3 teacher control listener and forwards each registered UDP endpoint.
///
/// # Errors
/// Returns an error when the TCP listener cannot bind, accept, or observe shutdown.
pub async fn run_teacher_media_control(
    bind_ip: Ipv4Addr,
    port: u16,
    presentation: StartPresentation,
    events: std::sync::mpsc::Sender<TeacherMediaEvent>,
) -> Result<(), ControlError> {
    let listener = TcpListener::bind(SocketAddrV4::new(bind_ip, port)).await?;
    info!(address = %listener.local_addr()?, "teacher media control listener ready");
    let next_client_id = Arc::new(AtomicU64::new(1));
    loop {
        tokio::select! {
            result = listener.accept() => {
                let (stream, peer) = result?;
                stream.set_nodelay(true)?;
                let client_id = next_client_id.fetch_add(1, Ordering::Relaxed);
                let presentation = presentation.clone();
                let events = events.clone();
                tokio::spawn(async move {
                    if let Err(error) = handle_media_student(stream, peer, client_id, presentation, events).await {
                        warn!(%peer, %error, "student media control connection ended with an error");
                    }
                });
            }
            result = tokio::signal::ctrl_c() => { result?; info!("teacher media control listener stopping"); return Ok(()); }
        }
    }
}

/// Registers a student, returning the active stream or an idle room connection.
///
/// # Panics
/// Panics if shared presentation state was poisoned by another thread.
///
/// # Errors
/// Returns an error for connection, framing, version, or stream-config failures.
#[allow(clippy::too_many_lines)]
pub async fn register_student_for_presentation(
    teacher: SocketAddrV4,
    registration: StudentRegistration,
) -> Result<(StudentControl, u64, Option<StartPresentation>), ControlError> {
    let mut stream = connect_teacher(teacher, registration.selected_local_ip).await?;
    stream.set_nodelay(true)?;
    let routed_local_ip = match stream.local_addr()? {
        SocketAddr::V4(address) => *address.ip(),
        SocketAddr::V6(_) => registration.selected_local_ip,
    };
    let hello = Envelope::hello(HelloStudent {
        hostname: registration.hostname,
        instance_id: registration.instance_id,
        selected_local_ip: routed_local_ip.to_string(),
        media_udp_port: u32::from(registration.media_udp_port),
        feedback_udp_port: u32::from(registration.feedback_udp_port),
        pointer_overlay: registration.pointer_overlay,
    });
    info!(%teacher, local_addr = %stream.local_addr()?, "student registration TCP connected; sending Hello");
    write_envelope(&mut stream, &hello).await.map_err(|error| {
        warn!(%teacher, %error, "student registration Hello write failed");
        error
    })?;
    let welcome = tokio::time::timeout(CONTROL_LIVENESS_TIMEOUT, read_envelope(&mut stream))
        .await
        .map_err(|_| {
            warn!(%teacher, "student registration Welcome read timed out");
            ControlError::ControlClosed
        })?
        .map_err(|error| {
            warn!(%teacher, %error, "student registration Welcome read failed");
            error
        })?;
    info!(%teacher, "student registration Welcome received");
    check_version(&welcome)?;
    let (client_id, active) = match welcome.payload {
        Some(Payload::WelcomeStudent(value)) => (value.client_id, value.presentation_active),
        _ => return Err(ControlError::UnexpectedMessage),
    };
    let presentation = if active {
        let start = tokio::time::timeout(CONTROL_LIVENESS_TIMEOUT, read_envelope(&mut stream))
            .await
            .map_err(|_| ControlError::ControlClosed)??;
        check_version(&start)?;
        match start.payload {
            Some(Payload::StartPresentation(value)) => Some(value),
            _ => return Err(ControlError::UnexpectedMessage),
        }
    } else {
        None
    };
    let presentation_state = Arc::new(Mutex::new(presentation.clone()));
    let (mut reader, mut writer) = stream.into_split();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(256);
    let rtt = Arc::new(Mutex::new(ControlRtt::default()));
    let connected = Arc::new(AtomicBool::new(true));
    let writer_connected = Arc::clone(&connected);
    let writer_task = tokio::spawn(async move {
        while let Some(envelope) = receiver.recv().await {
            if write_envelope(&mut writer, &envelope).await.is_err() {
                break;
            }
        }
        writer_connected.store(false, Ordering::Release);
    });
    let input_status = Arc::new(Mutex::new(None));
    let read_status = Arc::clone(&input_status);
    let cursor_status = Arc::new(Mutex::new(None));
    let read_cursor = Arc::clone(&cursor_status);
    let reader_presentation = Arc::clone(&presentation_state);
    let reader_rtt = Arc::clone(&rtt);
    let reader_connected = Arc::clone(&connected);
    let reader_task = tokio::spawn(async move {
        loop {
            // On timeout discard this connection: never resume a canceled partial
            // length-prefixed read on the same stream.
            let envelope =
                match tokio::time::timeout(CONTROL_LIVENESS_TIMEOUT, read_envelope(&mut reader))
                    .await
                {
                    Ok(Ok(envelope)) => envelope,
                    Ok(Err(_)) => break,
                    Err(_) => {
                        warn!(
                            timeout_ms = CONTROL_LIVENESS_TIMEOUT.as_secs() * 1000,
                            "teacher control liveness timeout; releasing Student presentation"
                        );
                        break;
                    }
                };
            if check_version(&envelope).is_err() {
                break;
            }
            let pong = match envelope.payload {
                Some(Payload::Observe(status)) => {
                    if status.action == "cursor" {
                        *read_cursor.lock().unwrap() = Some(status);
                    } else {
                        *read_status.lock().unwrap() = Some(status);
                    }
                    continue;
                }
                Some(Payload::ControlPong(pong)) => pong,
                Some(Payload::StartPresentation(value)) => {
                    info!(
                        stream_id = value.stream_id,
                        "student received presentation start"
                    );
                    *reader_presentation.lock().unwrap() = Some(value);
                    continue;
                }
                Some(Payload::StopPresentation(value)) => {
                    let mut state = reader_presentation.lock().unwrap();
                    if state
                        .as_ref()
                        .is_some_and(|p| p.stream_id == value.stream_id)
                    {
                        *state = None;
                        info!(
                            stream_id = value.stream_id,
                            "student received presentation stop"
                        );
                    }
                    continue;
                }
                _ => break,
            };
            if let Ok(mut state) = reader_rtt.lock() {
                let now = Instant::now();
                let received_us = media_core::unix_us();
                let matches = state.pending.is_some_and(|(nonce, _)| nonce == pong.nonce);
                state.receive_pong(pong.nonce, now);
                if matches && pong.sender_unix_us > 0 && pong.teacher_received_unix_us > 0 {
                    let offset = ((i128::from(pong.teacher_received_unix_us)
                        - i128::from(pong.sender_unix_us))
                        + (i128::from(pong.teacher_sent_unix_us) - i128::from(received_us)))
                        / 2;
                    let wire_rtt = (i128::from(received_us) - i128::from(pong.sender_unix_us))
                        - (i128::from(pong.teacher_sent_unix_us)
                            - i128::from(pong.teacher_received_unix_us));
                    if let (Ok(offset), Ok(rtt)) = (i64::try_from(offset), u64::try_from(wire_rtt))
                    {
                        if rtt <= state.last_micros.saturating_add(10_000)
                            && (state
                                .clock_at
                                .is_none_or(|at| at.elapsed() > Duration::from_secs(5))
                                || state.clock.is_none_or(|old| rtt / 2 < old.uncertainty_us)
                                || state.clock.is_some_and(|old| {
                                    old.teacher_minus_student_us.abs_diff(offset) > 10_000
                                }))
                        {
                            state.clock = Some(ClockEstimate {
                                teacher_minus_student_us: offset,
                                uncertainty_us: rtt / 2 + 1000,
                            });
                            state.clock_at = Some(now);
                        }
                    }
                }
            }
        }
        reader_connected.store(false, Ordering::Release);
    });
    Ok((
        StudentControl {
            sender,
            rtt,
            connected: Arc::clone(&connected),
            presentation: presentation_state,
            input_status,
            cursor_status,
            _tasks: Arc::new(StudentControlTasks {
                reader: reader_task.abort_handle(),
                writer: writer_task.abort_handle(),
                connected: Arc::clone(&connected),
            }),
        },
        client_id,
        presentation,
    ))
}

async fn connect_teacher(
    teacher: SocketAddrV4,
    local_ip: Ipv4Addr,
) -> Result<TcpStream, ControlError> {
    let socket = tokio::net::TcpSocket::new_v4()?;
    socket.bind(SocketAddrV4::new(local_ip, 0).into())?;
    let connect_result =
        tokio::time::timeout(Duration::from_secs(2), socket.connect(teacher.into()))
            .await
            .map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::TimedOut, "Teacher TCP connect deadline")
            })?;
    match connect_result {
        Ok(stream) => Ok(stream),
        Err(error) if is_macos_host_unreachable(&error) => {
            // On some macOS routes, the nonblocking connect can report EHOSTUNREACH
            // while the system's blocking connect succeeds after route/neighbor
            // resolution. Keep it off the async workers and bound it to two seconds.
            warn!(%teacher, %error, "Tokio TCP connect reported unreachable; retrying through system connect");
            let fallback = tokio::task::spawn_blocking(move || -> std::io::Result<_> {
                let socket = socket2::Socket::new(
                    socket2::Domain::IPV4,
                    socket2::Type::STREAM,
                    Some(socket2::Protocol::TCP),
                )?;
                socket.bind(&SocketAddrV4::new(local_ip, 0).into())?;
                socket.connect_timeout(
                    &SocketAddrV4::new(*teacher.ip(), teacher.port()).into(),
                    Duration::from_secs(2),
                )?;
                socket.set_nonblocking(true)?;
                Ok(std::net::TcpStream::from(socket))
            })
            .await
            .map_err(|join_error| {
                std::io::Error::other(format!("blocking Teacher connect worker: {join_error}"))
            })?;
            let fallback = fallback.map_err(|fallback_error| {
                warn!(%teacher, %fallback_error, "system TCP connect fallback failed");
                fallback_error
            })?;
            fallback.set_nonblocking(true)?;
            TcpStream::from_std(fallback).map_err(Into::into)
        }
        Err(error) => {
            warn!(%teacher, %error, "student registration TCP connect failed");
            Err(error.into())
        }
    }
}

fn is_macos_host_unreachable(error: &std::io::Error) -> bool {
    // ErrorKind::HostUnreachable was stabilized after this workspace's Rust MSRV.
    // Darwin's stable errno for EHOSTUNREACH is 65; only apply this recovery there.
    #[cfg(target_os = "macos")]
    {
        error.raw_os_error() == Some(65)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = error;
        false
    }
}

/// Runs the teacher-side TCP control listener until Ctrl+C.
///
/// # Errors
///
/// Returns an error when the listener cannot bind, accept a connection, or observe shutdown.
pub async fn run_teacher(bind_ip: Ipv4Addr, port: u16) -> Result<(), ControlError> {
    let listener = TcpListener::bind(SocketAddrV4::new(bind_ip, port)).await?;
    info!(address = %listener.local_addr()?, "teacher control listener ready");
    let next_client_id = Arc::new(AtomicU64::new(1));

    loop {
        tokio::select! {
            result = listener.accept() => {
                let (stream, peer) = result?;
                let client_id = next_client_id.fetch_add(1, Ordering::Relaxed);
                tokio::spawn(async move {
                    if let Err(error) = handle_student(stream, peer, client_id).await {
                        warn!(%peer, %error, "student control connection ended with an error");
                    }
                });
            }
            result = tokio::signal::ctrl_c() => {
                result?;
                info!("teacher control listener stopping");
                return Ok(());
            }
        }
    }
}

/// Connects a student to the teacher and completes the initial registration handshake.
///
/// # Errors
///
/// Returns an error for connection failures, invalid framing, incompatible protocol versions,
/// or an unexpected response.
pub async fn register_student(
    teacher: SocketAddrV4,
    registration: StudentRegistration,
) -> Result<(TcpStream, u64), ControlError> {
    let mut stream = TcpStream::connect(teacher).await?;
    let hello = Envelope::hello(HelloStudent {
        hostname: registration.hostname,
        instance_id: registration.instance_id,
        selected_local_ip: registration.selected_local_ip.to_string(),
        media_udp_port: u32::from(registration.media_udp_port),
        feedback_udp_port: u32::from(registration.feedback_udp_port),
        pointer_overlay: registration.pointer_overlay,
    });
    write_envelope(&mut stream, &hello).await?;
    let response = read_envelope(&mut stream).await?;
    check_version(&response)?;
    match response.payload {
        Some(Payload::WelcomeStudent(welcome)) => Ok((stream, welcome.client_id)),
        _ => Err(ControlError::UnexpectedMessage),
    }
}

async fn handle_student(
    mut stream: TcpStream,
    peer: SocketAddr,
    client_id: u64,
) -> Result<(), ControlError> {
    let envelope = read_envelope(&mut stream).await?;
    check_version(&envelope)?;
    let Some(Payload::HelloStudent(hello)) = envelope.payload else {
        return Err(ControlError::UnexpectedMessage);
    };
    info!(
        client_id,
        %peer,
        hostname = %hello.hostname,
        local_ip = %hello.selected_local_ip,
        media_port = hello.media_udp_port,
        "student registered"
    );
    write_envelope(&mut stream, &Envelope::welcome(client_id)).await?;
    let mut byte = [0_u8; 1];
    while stream.read(&mut byte).await? != 0 {}
    info!(client_id, %peer, "student disconnected");
    Ok(())
}

async fn handle_media_student(
    mut stream: TcpStream,
    peer: SocketAddr,
    client_id: u64,
    presentation: StartPresentation,
    events: std::sync::mpsc::Sender<TeacherMediaEvent>,
) -> Result<(), ControlError> {
    let envelope = read_envelope(&mut stream).await?;
    check_version(&envelope)?;
    let Some(Payload::HelloStudent(hello)) = envelope.payload else {
        return Err(ControlError::UnexpectedMessage);
    };
    let advertised_ip = hello.selected_local_ip.parse::<Ipv4Addr>().ok();
    let endpoint_ip = media_endpoint_ip(peer, advertised_ip);
    if advertised_ip.is_some_and(|advertised| advertised != endpoint_ip) {
        warn!(client_id, %peer, %endpoint_ip, advertised_ip = %hello.selected_local_ip, "student media address differed from control route; using TCP peer IP");
    }
    let endpoint = SocketAddrV4::new(
        endpoint_ip,
        u16::try_from(hello.media_udp_port).map_err(|_| ControlError::UnexpectedMessage)?,
    );
    let mut welcome = Envelope::welcome(client_id);
    if let Some(Payload::WelcomeStudent(value)) = &mut welcome.payload {
        value.presentation_active = true;
    }
    write_envelope(&mut stream, &welcome).await?;
    write_envelope(&mut stream, &Envelope::start_presentation(presentation)).await?;
    events
        .send(TeacherMediaEvent::Registered(RegisteredStudent {
            client_id,
            endpoint,
            hostname: hello.hostname,
            pointer_overlay: hello.pointer_overlay,
        }))
        .map_err(|_| ControlError::ControlClosed)?;
    let _registration = RegisteredStudentGuard {
        client_id,
        endpoint,
        events: events.clone(),
    };
    info!(client_id, %peer, %endpoint, "student registered for media");
    loop {
        let message = read_envelope(&mut stream).await?;
        check_version(&message)?;
        match message.payload {
            Some(Payload::ReceiverReport(report)) => events
                .send(TeacherMediaEvent::ReceiverReport { client_id, report })
                .map_err(|_| ControlError::ControlClosed)?,
            Some(Payload::Nack(message)) => events
                .send(TeacherMediaEvent::Nack {
                    client_id,
                    endpoint,
                    message,
                })
                .map_err(|_| ControlError::ControlClosed)?,
            Some(Payload::RequestKeyframe(message)) => events
                .send(TeacherMediaEvent::KeyframeRequested { client_id, message })
                .map_err(|_| ControlError::ControlClosed)?,
            Some(Payload::ControlPing(ping)) => {
                let received = media_core::unix_us();
                let mut response = Envelope::control_pong(ping.nonce);
                if let Some(Payload::ControlPong(message)) = &mut response.payload {
                    message.sender_unix_us = ping.sender_unix_us;
                    message.teacher_received_unix_us = received;
                    message.teacher_sent_unix_us = media_core::unix_us();
                }
                write_envelope(&mut stream, &response).await?;
            }
            _ => return Err(ControlError::UnexpectedMessage),
        }
    }
}

fn media_endpoint_ip(peer: SocketAddr, advertised: Option<Ipv4Addr>) -> Ipv4Addr {
    match peer {
        SocketAddr::V4(address) => *address.ip(),
        SocketAddr::V6(_) => advertised.unwrap_or(Ipv4Addr::LOCALHOST),
    }
}

async fn write_envelope(
    stream: &mut (impl AsyncWrite + Unpin),
    envelope: &Envelope,
) -> Result<(), ControlError> {
    let body = envelope.encode_to_vec();
    if body.len() > MAX_CONTROL_FRAME_BYTES {
        return Err(ControlError::FrameTooLarge(body.len()));
    }
    let length = u32::try_from(body.len()).map_err(|_| ControlError::FrameTooLarge(body.len()))?;
    let mut frame = Vec::with_capacity(body.len() + 4);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&body);
    stream.write_all(&frame).await?;
    stream.flush().await?;
    Ok(())
}

async fn read_envelope(stream: &mut (impl AsyncRead + Unpin)) -> Result<Envelope, ControlError> {
    let length = stream.read_u32().await? as usize;
    if length > MAX_CONTROL_FRAME_BYTES {
        return Err(ControlError::FrameTooLarge(length));
    }
    let mut body = vec![0_u8; length];
    stream.read_exact(&mut body).await?;
    Ok(Envelope::decode(body.as_slice())?)
}

fn check_version(envelope: &Envelope) -> Result<(), ControlError> {
    if envelope.protocol_version == PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(ControlError::Version(envelope.protocol_version))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn registration_handshake_uses_length_prefixed_protobuf() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let teacher = tokio::spawn(async move {
            let (stream, peer) = listener.accept().await.unwrap();
            handle_student(stream, peer, 42).await.unwrap();
        });

        let (stream, client_id) = register_student(
            match address {
                SocketAddr::V4(value) => value,
                SocketAddr::V6(_) => unreachable!(),
            },
            StudentRegistration {
                hostname: "student-01".to_owned(),
                instance_id: "test-instance".to_owned(),
                selected_local_ip: Ipv4Addr::new(10, 0, 55, 11),
                media_udp_port: 47_810,
                feedback_udp_port: 47_811,
                pointer_overlay: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(client_id, 42);
        drop(stream);
        teacher.await.unwrap();
    }

    #[tokio::test]
    async fn media_control_measures_rtt_and_reports_server_disconnect() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let (events, _received) = std::sync::mpsc::channel();
        let teacher = tokio::spawn(async move {
            let (stream, peer) = listener.accept().await.unwrap();
            let presentation = StartPresentation {
                stream_id: 1,
                width: 640,
                height: 360,
                max_fps: 30,
                bitrate_bps: 1_000_000,
                rtp_payload_type: 96,
                rtp_clock_hz: 90_000,
                teacher_media_udp_port: 47_811,
            };
            let _ = handle_media_student(stream, peer, 1, presentation, events).await;
        });
        let (student, _, _) = register_student_for_presentation(
            match address {
                SocketAddr::V4(value) => value,
                SocketAddr::V6(_) => unreachable!(),
            },
            StudentRegistration {
                hostname: "student-01".to_owned(),
                instance_id: "test-instance".to_owned(),
                selected_local_ip: Ipv4Addr::LOCALHOST,
                media_udp_port: 47_810,
                feedback_udp_port: 47_811,
                pointer_overlay: false,
            },
        )
        .await
        .unwrap();
        student.probe_rtt().unwrap();
        student.probe_rtt().unwrap();
        tokio::time::timeout(Duration::from_secs(1), async {
            while student.rtt_sample().0 == 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(student.rtt_sample().0, 1);
        let clock = student.clock_estimate().unwrap();
        assert!(clock.teacher_minus_student_us.unsigned_abs() <= clock.uncertainty_us + 1000);
        assert!(
            clock
                .source_age(media_core::unix_us().saturating_sub(100_000))
                .unwrap()
                >= Duration::from_millis(90)
        );
        assert!(student.is_connected());
        teacher.abort();
        let _ = teacher.await;
        tokio::time::timeout(Duration::from_secs(1), async {
            while student.is_connected() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert!(student.probe_rtt().is_err());
    }

    #[tokio::test]
    async fn silent_teacher_with_partial_frame_expires_without_tcp_eof() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let SocketAddr::V4(address) = listener.local_addr().unwrap() else {
            unreachable!()
        };
        let teacher = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            read_envelope(&mut stream).await.unwrap();
            write_envelope(&mut stream, &Envelope::welcome(1))
                .await
                .unwrap();
            // A half-written next frame must not keep the desktop hostage.
            stream.write_all(&[0, 0]).await.unwrap();
            tokio::time::sleep(Duration::from_secs(30)).await;
        });
        let (student, _, _) = register_student_for_presentation(
            address,
            StudentRegistration {
                hostname: "silent-test".into(),
                instance_id: "silent-test".into(),
                selected_local_ip: Ipv4Addr::LOCALHOST,
                media_udp_port: 47810,
                feedback_udp_port: 47811,
                pointer_overlay: false,
            },
        )
        .await
        .unwrap();
        let started = Instant::now();
        assert!(student.is_connected());
        tokio::time::timeout(CONTROL_LIVENESS_TIMEOUT + Duration::from_secs(1), async {
            while student.is_connected() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(started.elapsed() >= CONTROL_LIVENESS_TIMEOUT);
        assert!(student.probe_rtt().is_err());
        teacher.abort();
    }

    #[test]
    fn media_endpoint_follows_the_proven_control_route() {
        let peer = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 168, 10, 102), 50_718));
        assert_eq!(
            media_endpoint_ip(peer, Some(Ipv4Addr::new(192, 168, 100, 254))),
            Ipv4Addr::new(192, 168, 10, 102)
        );
    }
}
