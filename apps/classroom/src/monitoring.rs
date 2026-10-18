//! Bounded, pull-based snapshots on a separate TCP connection. Never queues video.
use super::PlatformCapture;
use anyhow::{Context, Result};
use classroom_config::MonitoringConfig;
use control_transport::room::{RoomRoster, RoomThumbnail};
use media_core::{CaptureConfig, CaptureSource, VideoFrame};
use prost::Message;
use protocol::{
    envelope::Payload, Envelope, ThumbnailFrame, ThumbnailHello, ThumbnailRequest, PROTOCOL_VERSION,
};
use std::{
    io::{Read, Write},
    net::{Shutdown, SocketAddr, SocketAddrV4, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Condvar, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use tracing::{info, warn};

pub type ObserveJobs = Arc<ObserveRequests>;
#[derive(Default)]
pub struct ObserveRequests {
    pub entries: Mutex<std::collections::HashMap<String, ObserveJob>>,
    wake: Condvar,
    thumbnail_width: AtomicU32,
    /// Teacher's enhanced-profile offer (codecs it decodes, fps ceiling). Behind a
    /// lock because switching the performance mode changes it while running.
    offer: Mutex<(Vec<String>, u32)>,
    /// Thumbnail settings in force now; `None` until a performance-mode switch,
    /// then the connection threads use these instead of the ones they started with.
    monitoring: Mutex<Option<MonitoringConfig>>,
}

struct BroadcastBadgeSuppression {
    active: Arc<AtomicBool>,
    applied: Arc<AtomicBool>,
}
impl BroadcastBadgeSuppression {
    fn start(state: &Arc<AtomicBool>, applied: &Arc<AtomicBool>) -> Self {
        applied.store(false, Ordering::Release);
        state.store(true, Ordering::Release);
        Self {
            active: Arc::clone(state),
            applied: Arc::clone(applied),
        }
    }
}
impl Drop for BroadcastBadgeSuppression {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
        self.applied.store(false, Ordering::Release);
    }
}
fn wait_badge_hidden(applied: &AtomicBool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(2);
    while !applied.load(Ordering::Acquire) {
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(10));
    }
    true
}
impl ObserveRequests {
    pub fn with_offer(offer: (Vec<String>, u32)) -> Self {
        Self {
            offer: Mutex::new(offer),
            ..Self::default()
        }
    }
    pub fn offer(&self) -> (Vec<String>, u32) {
        self.offer.lock().unwrap().clone()
    }
    pub fn set_offer(&self, offer: (Vec<String>, u32)) {
        *self.offer.lock().unwrap() = offer;
    }
    pub fn monitoring_override(&self) -> Option<MonitoringConfig> {
        self.monitoring.lock().unwrap().clone()
    }
    pub fn set_monitoring(&self, settings: MonitoringConfig) {
        *self.monitoring.lock().unwrap() = Some(settings);
        self.wake.notify_all(); // a longer or shorter interval takes effect now
    }
    pub fn set_thumbnail_width(&self, width: u32) -> Result<()> {
        anyhow::ensure!([240, 360, 480].contains(&width), "Invalid thumbnail tier");
        self.thumbnail_width.store(width, Ordering::Release);
        info!(width, "Teacher thumbnail tier changed");
        Ok(())
    }
    pub fn request(&self, id: String) {
        self.request_with_offer(id, None);
    }
    pub fn request_with_offer(&self, id: String, offer: Option<(Vec<String>, u32)>) {
        self.request_profile(id, offer, false);
    }
    pub fn request_broadcast(&self, id: String) {
        self.request_profile(id, Some((vec!["h264".into()], 30)), true);
    }
    fn request_profile(
        &self,
        id: String,
        offer: Option<(Vec<String>, u32)>,
        class_broadcast: bool,
    ) {
        let mut entries = self.entries.lock().unwrap();
        entries.insert(
            id,
            ObserveJob {
                request: true,
                requested_at: Some(Instant::now()),
                offer,
                class_broadcast,
                ..Default::default()
            },
        );
        self.wake.notify_all();
    }
    /// The old fullscreen receiver may be finishing teardown when the new
    /// class-source request arrives on this separate monitoring connection.
    /// Retry only this transient class handoff, with the original request budget.
    fn complete_start(
        &self,
        id: &str,
        queued_at: Instant,
        reply: protocol::ObserveMessage,
        now: Instant,
    ) -> Result<()> {
        let port = u16::try_from(reply.port)?;
        let mut entries = self.entries.lock().unwrap();
        if let Some(job) = entries
            .get_mut(id)
            .filter(|j| j.requested_at == Some(queued_at))
        {
            if job.class_broadcast
                && reply.error == "Student is watching a presentation"
                && now.duration_since(queued_at) < Duration::from_secs(4)
            {
                job.request = true;
                info!(
                    student_id = id,
                    wait_ms = now.duration_since(queued_at).as_millis(),
                    "Class source waiting for previous receiver teardown"
                );
                self.wake.notify_all();
            } else {
                job.port = port;
                job.token = reply.token;
                job.error = reply.error;
                job.profile = reply.codec;
            }
        }
        Ok(())
    }
    fn wait(&self, stop: &AtomicBool, id: &str, duration: Duration) {
        let entries = self.entries.lock().unwrap();
        let _guard = self
            .wake
            .wait_timeout_while(entries, duration, |entries| {
                !stop.load(Ordering::Acquire) && !entries.get(id).is_some_and(|job| job.request)
            })
            .unwrap();
    }
    fn wake_all(&self) {
        let _guard = self.entries.lock().unwrap();
        self.wake.notify_all();
    }
}
#[derive(Clone, Default)]
pub struct ObserveJob {
    pub request: bool,
    pub port: u16,
    pub token: String,
    pub error: String,
    pub requested_at: Option<Instant>,
    /// Student-chosen session profile, e.g. "h265@60".
    pub profile: String,
    pub offer: Option<(Vec<String>, u32)>,
    pub class_broadcast: bool,
}

const MAX_JPEG: usize = 192 * 1024;
const LEGACY_MAX_JPEG: usize = 48 * 1024;
// Only the dedicated snapshot socket grows; control-plane framing stays 64 KiB.
const MAX_MONITOR_FRAME: usize = MAX_JPEG + 1024;
const MAX_PEERS: usize = 64;

pub struct Worker {
    stop: Arc<AtomicBool>,
    socket: Arc<Mutex<Option<TcpStream>>>,
    handle: Option<thread::JoinHandle<()>>,
    wake: Option<ObserveJobs>,
}
// At most one old thumbnail capture retires off the request thread. Its slow
// native teardown must not delay the Observe handshake, and Exit still joins it.
#[derive(Default)]
struct CaptureRetirement(Option<thread::JoinHandle<()>>);
impl CaptureRetirement {
    fn retire(&mut self, capture: PlatformCapture) -> Result<()> {
        if let Some(worker) = self.0.take() {
            let _ = worker.join();
        }
        self.0 = Some(
            thread::Builder::new()
                .name("thumbnail-release".into())
                .spawn(move || {
                    let began = Instant::now();
                    drop(capture);
                    info!(
                        retirement_ms = began.elapsed().as_secs_f64() * 1000.0,
                        "Student old thumbnail capture retired"
                    );
                })?,
        );
        Ok(())
    }
}
impl Drop for CaptureRetirement {
    fn drop(&mut self) {
        if let Some(worker) = self.0.take() {
            let _ = worker.join();
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(wake) = &self.wake {
            wake.wake_all();
        }
        if let Some(socket) = self.socket.lock().unwrap().as_ref() {
            let _ = socket.shutdown(Shutdown::Both);
        }
        if let Some(worker) = self.handle.take() {
            let _ = worker.join();
        }
    }
}
fn envelope(payload: Payload) -> Envelope {
    Envelope {
        protocol_version: PROTOCOL_VERSION,
        payload: Some(payload),
    }
}
fn send(socket: &mut TcpStream, payload: Payload) -> Result<()> {
    let data = envelope(payload).encode_to_vec();
    anyhow::ensure!(
        data.len() <= MAX_MONITOR_FRAME,
        "monitoring frame too large"
    );
    socket.write_all(&u32::try_from(data.len())?.to_be_bytes())?;
    socket.write_all(&data)?;
    Ok(())
}
fn read(socket: &mut TcpStream) -> Result<Payload> {
    let mut prefix = [0; 4];
    socket.read_exact(&mut prefix)?;
    let size = usize::try_from(u32::from_be_bytes(prefix))?;
    anyhow::ensure!(
        size > 0 && size <= MAX_MONITOR_FRAME,
        "invalid monitoring frame length"
    );
    let mut bytes = vec![0; size];
    socket.read_exact(&mut bytes)?;
    let frame = Envelope::decode(bytes.as_slice())?;
    anyhow::ensure!(
        frame.protocol_version == PROTOCOL_VERSION,
        "monitoring version mismatch"
    );
    frame.payload.context("empty monitoring frame")
}
fn configure(socket: &TcpStream) -> Result<()> {
    // macOS accepted sockets inherit a nonblocking listener setting.
    socket.set_nonblocking(false)?;
    socket.set_nodelay(true)?;
    socket.set_read_timeout(Some(Duration::from_secs(8)))?;
    socket.set_write_timeout(Some(Duration::from_secs(3)))?;
    Ok(())
}
/// True when a peer message is waiting. A closed peer also reports readable so
/// the following framed read returns its normal error. Restores the 8 s budget.
fn readable_within(socket: &TcpStream, wait: Duration) -> Result<bool> {
    socket.set_read_timeout(Some(wait))?;
    let result = match socket.peek(&mut [0; 1]) {
        Ok(_) => Ok(true),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) =>
        {
            Ok(false)
        }
        Err(error) => Err(error.into()),
    };
    socket.set_read_timeout(Some(Duration::from_secs(8)))?;
    result
}
fn take_observe_request(jobs: &ObserveJobs, id: &str) -> Option<Instant> {
    jobs.entries.lock().unwrap().get_mut(id).and_then(|j| {
        let r = j
            .request
            .then(|| j.requested_at.unwrap_or_else(Instant::now));
        j.request = false;
        r
    })
}
fn send_observe_start(
    socket: &mut TcpStream,
    jobs: &ObserveJobs,
    id: &str,
    client_id: u64,
    queued_at: Instant,
) -> Result<Instant> {
    let (offer, class_broadcast) = {
        let entries = jobs.entries.lock().unwrap();
        let job = entries.get(id);
        (
            job.and_then(|j| j.offer.clone())
                .unwrap_or_else(|| jobs.offer()),
            job.is_some_and(|j| j.class_broadcast),
        )
    };
    let handshake = Instant::now();
    info!(
        client_id,
        queue_wait_ms = queued_at.elapsed().as_secs_f64() * 1000.0,
        "Observe request dispatched to Student"
    );
    send(
        socket,
        Payload::Observe(protocol::ObserveMessage {
            action: "start".into(),
            codecs: offer.0,
            max_fps: offer.1,
            class_broadcast,
            ..Default::default()
        }),
    )?;
    Ok(handshake)
}
fn finish_observe_start(
    socket: &mut TcpStream,
    jobs: &ObserveJobs,
    id: &str,
    client_id: u64,
    queued_at: Instant,
    handshake: Instant,
) -> Result<()> {
    let Payload::Observe(reply) = read(socket)? else {
        anyhow::bail!("Expected Observe response")
    };
    info!(
        client_id,
        handshake_ms = handshake.elapsed().as_secs_f64() * 1000.0,
        offered = ?jobs.offer(),
        profile = %reply.codec,
        "Observe source handshake complete"
    );
    jobs.complete_start(id, queued_at, reply, Instant::now())
}
/// Wait out `duration`, returning early with true when the Teacher has sent a
/// message (an Observe command) that the next read would consume.
fn wait_for_command(socket: &TcpStream, stop: &AtomicBool, duration: Duration) -> Result<bool> {
    let end = Instant::now() + duration;
    while !stop.load(Ordering::Acquire) && Instant::now() < end {
        if readable_within(socket, Duration::from_millis(10))? {
            return Ok(true);
        }
    }
    Ok(false)
}
fn sleep(stop: &AtomicBool, duration: Duration) {
    let end = Instant::now() + duration;
    while !stop.load(Ordering::Acquire) && Instant::now() < end {
        thread::sleep(Duration::from_millis(50));
    }
}

pub fn teacher(
    address: SocketAddrV4,
    settings: MonitoringConfig,
    roster: Arc<Mutex<RoomRoster>>,
    jobs: ObserveJobs,
) -> Result<Worker> {
    let listener = TcpListener::bind(address)?;
    listener.set_nonblocking(true)?;
    let stop = Arc::new(AtomicBool::new(false));
    let canceled = Arc::clone(&stop);
    let wake = Arc::clone(&jobs);
    let worker = thread::spawn(move || {
        info!(%address,interval_ms=settings.interval_ms,"teacher monitoring listener ready");
        let mut peers: Vec<(TcpStream, thread::JoinHandle<()>)> = Vec::new();
        while !canceled.load(Ordering::Acquire) {
            let mut index = 0;
            while index < peers.len() {
                if peers[index].1.is_finished() {
                    let (_, worker) = peers.swap_remove(index);
                    let _ = worker.join();
                } else {
                    index += 1;
                }
            }
            match listener.accept() {
                Ok((mut socket, SocketAddr::V4(peer))) if peers.len() < MAX_PEERS => {
                    let Ok(clone) = socket.try_clone() else {
                        continue;
                    };
                    let settings = settings.clone();
                    let jobs = Arc::clone(&jobs);
                    let roster = Arc::clone(&roster);
                    let stop = Arc::clone(&canceled);
                    let worker = thread::spawn(move || {
                        if let Err(error) =
                            serve(&mut socket, *peer.ip(), &settings, &roster, &stop, &jobs)
                        {
                            info!(%peer,%error,"teacher monitoring peer ended");
                        }
                    });
                    peers.push((clone, worker));
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(50));
                }
                Err(error) => {
                    warn!(%error,"teacher monitoring accept failed");
                    break;
                }
            }
        }
        for (socket, _) in &peers {
            let _ = socket.shutdown(Shutdown::Both);
        }
        for (_, worker) in peers {
            let _ = worker.join();
        }
    });
    Ok(Worker {
        stop,
        socket: Arc::new(Mutex::new(None)),
        handle: Some(worker),
        wake: Some(wake),
    })
}
#[allow(clippy::too_many_lines)] // One serialized framed exchange owns this TCP stream.
fn serve(
    socket: &mut TcpStream,
    peer: std::net::Ipv4Addr,
    settings: &MonitoringConfig,
    roster: &Mutex<RoomRoster>,
    stop: &AtomicBool,
    jobs: &ObserveJobs,
) -> Result<()> {
    configure(socket)?;
    let Payload::ThumbnailHello(hello) = read(socket)? else {
        anyhow::bail!("expected monitoring hello");
    };
    let client_id = roster
        .lock()
        .unwrap()
        .monitoring_client(&hello.instance_id, peer)
        .context("monitoring peer has no active room registration")?;
    info!(client_id,%peer,"teacher monitoring Student connected");
    while !stop.load(Ordering::Acquire) {
        // The performance mode can change while this connection lives.
        let live = jobs.monitoring_override();
        let settings = live.as_ref().unwrap_or(settings);
        if roster
            .lock()
            .unwrap()
            .monitoring_client(&hello.instance_id, peer)
            != Some(client_id)
        {
            break;
        }
        if let Some(queued_at) = take_observe_request(jobs, &hello.instance_id) {
            let handshake =
                send_observe_start(socket, jobs, &hello.instance_id, client_id, queued_at)?;
            finish_observe_start(
                socket,
                jobs,
                &hello.instance_id,
                client_id,
                queued_at,
                handshake,
            )?;
        }
        let tier = jobs.thumbnail_width.load(Ordering::Acquire);
        let width = match tier {
            0 => settings.width,
            width => width,
        };
        // Negotiate sharper snapshots without sending unsupported requests to
        // legacy Students. Logical UI tiers remain 240/360/480.
        let width = if settings.hidpi && hello.max_width >= 960 {
            (width * 2).min(960)
        } else {
            width.min(settings.width)
        };
        let height = if tier == 0 {
            (u64::from(width) * u64::from(settings.height) / u64::from(settings.width)) as u32
        } else {
            width * 9 / 16
        };
        let request_at = Instant::now();
        send(
            socket,
            Payload::ThumbnailRequest(ThumbnailRequest {
                width,
                height,
                interval_ms: settings.interval_ms,
                quality: u32::from(settings.jpeg_quality.max(settings.quality_floor)),
            }),
        )?;
        // The Student may hold this reply until its snapshot interval ends. Do not
        // let an Observe click wait for it: dispatch the start as soon as it is
        // requested and collect both replies in the order the Student sends them.
        let mut early_start = None;
        let wait_until = Instant::now() + Duration::from_secs(9);
        while !stop.load(Ordering::Acquire)
            && Instant::now() < wait_until
            && !readable_within(socket, Duration::from_millis(10))?
        {
            if early_start.is_none() {
                if let Some(queued_at) = take_observe_request(jobs, &hello.instance_id) {
                    let handshake =
                        send_observe_start(socket, jobs, &hello.instance_id, client_id, queued_at)?;
                    early_start = Some((queued_at, handshake));
                }
            }
        }
        let Payload::ThumbnailFrame(frame) = read(socket)? else {
            anyhow::bail!("expected thumbnail response");
        };
        if let Some((queued_at, handshake)) = early_start {
            finish_observe_start(
                socket,
                jobs,
                &hello.instance_id,
                client_id,
                queued_at,
                handshake,
            )?;
        }
        anyhow::ensure!(
            frame.jpeg.len() <= MAX_JPEG
                && ["ready", "paused", "unavailable", "observing"].contains(&frame.state.as_str()),
            "invalid thumbnail payload"
        );
        let started = Instant::now();
        let thumbnail = if frame.state == "ready" {
            Some(decode(&frame)?)
        } else {
            anyhow::ensure!(frame.jpeg.is_empty(), "inactive thumbnail contains pixels");
            None
        };
        let decode_ms = started.elapsed().as_secs_f64() * 1000.0;
        if !roster
            .lock()
            .unwrap()
            .update_thumbnail(client_id, &frame.state, thumbnail)
        {
            break;
        }
        info!(client_id,sequence=frame.sequence,width=frame.width,height=frame.height,bytes=frame.jpeg.len(),
            state=%frame.state,decode_ms,round_trip_ms=request_at.elapsed().as_secs_f64()*1000.0,
            "teacher thumbnail received");
        jobs.wait(
            stop,
            &hello.instance_id,
            Duration::from_millis(u64::from(settings.interval_ms))
                .saturating_sub(request_at.elapsed()),
        );
    }
    Ok(())
}
fn decode(frame: &ThumbnailFrame) -> Result<RoomThumbnail> {
    anyhow::ensure!(
        frame.width > 0 && frame.width <= 960 && frame.height > 0 && frame.height <= 540,
        "thumbnail dimensions exceed budget"
    );
    let mut decoder = jpeg_decoder::Decoder::new(frame.jpeg.as_slice());
    decoder.set_max_decoding_buffer_size(960 * 540 * 4);
    decoder.read_info()?;
    let header = decoder.info().context("JPEG header missing")?;
    anyhow::ensure!(
        u32::from(header.width) == frame.width
            && u32::from(header.height) == frame.height
            && header.pixel_format == jpeg_decoder::PixelFormat::RGB24,
        "thumbnail JPEG layout mismatch"
    );
    let bytes = decoder.decode()?;
    anyhow::ensure!(
        bytes.len() == usize::from(header.width) * usize::from(header.height) * 3,
        "invalid decoded thumbnail length"
    );
    let pixels = bytes
        .chunks_exact(3)
        .map(|p| (u32::from(p[0]) << 16) | (u32::from(p[1]) << 8) | u32::from(p[2]))
        .collect();
    Ok(RoomThumbnail {
        jpeg: frame.jpeg.clone(),
        width: usize::from(header.width),
        height: usize::from(header.height),
        pixels,
        received_at: Instant::now(),
        sequence: frame.sequence,
    })
}

pub fn student(
    address: SocketAddrV4,
    identity: String,
    presenting: Arc<AtomicBool>,
    badge_broadcast_suppressed: Arc<AtomicBool>,
    badge_broadcast_suppression_applied: Arc<AtomicBool>,
    config: classroom_config::Config,
) -> Worker {
    let runtime = tokio::runtime::Handle::current();
    let stop = Arc::new(AtomicBool::new(false));
    let canceled = Arc::clone(&stop);
    let socket = Arc::new(Mutex::new(None));
    let active_socket = Arc::clone(&socket);
    let worker = thread::spawn(move || {
        info!(%address,"student background monitoring ready");
        while !canceled.load(Ordering::Acquire) {
            if let Ok(mut stream) =
                TcpStream::connect_timeout(&SocketAddr::V4(address), Duration::from_secs(2))
            {
                *active_socket.lock().unwrap() = stream.try_clone().ok();
                if let Err(error) = capture_requests(
                    &mut stream,
                    &identity,
                    &presenting,
                    &badge_broadcast_suppressed,
                    &badge_broadcast_suppression_applied,
                    &canceled,
                    &config,
                    &runtime,
                ) {
                    info!(%error,"student monitoring connection ended; retrying");
                }
                *active_socket.lock().unwrap() = None;
            }
            sleep(&canceled, Duration::from_secs(2));
        }
    });
    Worker {
        stop,
        socket,
        handle: Some(worker),
        wake: None,
    }
}
#[allow(clippy::too_many_lines)]
fn capture_requests(
    socket: &mut TcpStream,
    identity: &str,
    presenting: &Arc<AtomicBool>,
    badge_broadcast_suppressed: &Arc<AtomicBool>,
    badge_broadcast_suppression_applied: &Arc<AtomicBool>,
    stop: &AtomicBool,
    config: &classroom_config::Config,
    runtime: &tokio::runtime::Handle,
) -> Result<()> {
    configure(socket)?;
    send(
        socket,
        Payload::ThumbnailHello(ThumbnailHello {
            instance_id: identity.to_owned(),
            max_width: 960,
        }),
    )?;
    let mut publisher: Option<super::observation::Publisher> = None;
    let mut badge_suppression: Option<BroadcastBadgeSuppression> = None;
    let mut capture: Option<PlatformCapture> = None;
    let mut retiring = CaptureRetirement::default();
    let mut sequence = 0_u64;
    let mut retry_at = Instant::now();
    let mut previous: Option<VideoFrame> = None;
    let mut thumbnail_due = Instant::now();
    while !stop.load(Ordering::Acquire) {
        let message = read(socket)?;
        if let Payload::Observe(command) = message {
            let requested_at = Instant::now();
            let mut response = protocol::ObserveMessage::default();
            if command.action == "start" && !presenting.load(Ordering::Acquire) {
                badge_suppression.take();
                publisher.take();
                if let Some(old) = capture.take() {
                    retiring.retire(old)?;
                }
                previous = None;
                let SocketAddr::V4(local) = socket.local_addr()? else {
                    anyhow::bail!("IPv4 required")
                };
                if command.class_broadcast {
                    badge_suppression = Some(BroadcastBadgeSuppression::start(
                        badge_broadcast_suppressed,
                        badge_broadcast_suppression_applied,
                    ));
                }
                let can_start = !command.class_broadcast
                    || wait_badge_hidden(badge_broadcast_suppression_applied);
                if !can_start {
                    response.error = "Could not confirm monitoring badge hidden".into();
                } else {
                    match super::observation::Publisher::start(
                        config,
                        &command,
                        *local.ip(),
                        runtime,
                        Arc::clone(presenting),
                        match socket.peer_addr()? {
                            SocketAddr::V4(p) => *p.ip(),
                            SocketAddr::V6(_) => anyhow::bail!("IPv4 Observe peer required"),
                        },
                    ) {
                        Ok(p) => {
                            response.port = u32::from(p.port);
                            response.token.clone_from(&p.token);
                            response.codec.clone_from(&p.profile);
                            publisher = Some(p);
                        }
                        Err(e) => response.error = format!("{e:#}"),
                    }
                }
                if publisher.is_none() {
                    badge_suppression.take();
                }
            } else {
                response.error = "Student is watching a presentation".into();
            }
            send(socket, Payload::Observe(response))?;
            info!(
                response_ms = requested_at.elapsed().as_secs_f64() * 1000.0,
                "Student Observe start response sent"
            );
            continue;
        }
        let Payload::ThumbnailRequest(request) = message else {
            anyhow::bail!("expected snapshot request")
        };
        anyhow::ensure!(
            (2..=960).contains(&request.width)
                && (2..=540).contains(&request.height)
                && (1000..=10000).contains(&request.interval_ms)
                && (30..=85).contains(&request.quality),
            "invalid thumbnail request budget"
        );
        if publisher
            .as_ref()
            .is_some_and(super::observation::Publisher::is_finished)
        {
            publisher.take();
            badge_suppression.take();
        }
        // Read control immediately after each response. Rate-limit only snapshots,
        // never the Observe command waiting on this connection.
        // A command that arrives while the snapshot interval is pending ends the
        // wait: answer this request at once (no pixels; the Teacher keeps its
        // previous snapshot) so the command is read without the interval delay.
        let interval_left = thumbnail_due.saturating_duration_since(Instant::now());
        if !interval_left.is_zero() && wait_for_command(socket, stop, interval_left)? {
            sequence = sequence.wrapping_add(1);
            send(
                socket,
                Payload::ThumbnailFrame(ThumbnailFrame {
                    width: 0,
                    height: 0,
                    jpeg: Vec::new(),
                    state: if presenting.load(Ordering::Acquire) {
                        "paused"
                    } else {
                        "observing"
                    }
                    .into(),
                    sequence,
                }),
            )?;
            continue;
        }
        let started = Instant::now();
        thumbnail_due = started + Duration::from_millis(u64::from(request.interval_ms));
        sequence = sequence.wrapping_add(1);
        let mut frame = ThumbnailFrame {
            width: 0,
            height: 0,
            jpeg: Vec::new(),
            state: "unavailable".into(),
            sequence,
        };
        if presenting.load(Ordering::Acquire) {
            publisher.take();
            badge_suppression.take();
            capture = None;
            previous = None;
            frame.state = "paused".into();
        } else if publisher.is_some() {
            frame.state = "observing".into();
            capture = None;
            previous = None;
        } else {
            if capture.is_none() && Instant::now() >= retry_at {
                let mut source = PlatformCapture::new();
                match source.start_thumbnail(CaptureConfig {
                    display_index: 0,
                    width: 960,
                    height: 540,
                    max_fps: 1,
                }) {
                    Ok(()) => {
                        capture = Some(source);
                    }
                    Err(error) => {
                        warn!(%error,"student thumbnail capture unavailable");
                        retry_at = Instant::now() + Duration::from_secs(10);
                    }
                }
            }
            if let Some(source) = capture.as_mut() {
                // Once cached, take the latest available snapshot instead of
                // blocking the command socket for a fresh static-screen frame.
                let budget = if previous.is_some() { 20 } else { 1200 };
                match source.next_frame(Instant::now() + Duration::from_millis(budget)) {
                    Ok(Some(new)) => previous = Some(new),
                    Ok(None) => {}
                    Err(error) => {
                        warn!(%error,"student thumbnail capture failed");
                        capture = None;
                        previous = None;
                        retry_at = Instant::now() + Duration::from_secs(10);
                    }
                }
            }
            if let Some(pixels) = previous.as_ref() {
                let jpg_at = Instant::now();
                frame = encode(
                    pixels,
                    u8::try_from(request.quality)?,
                    sequence,
                    (request.width, request.height),
                )?;
                info!(
                    sequence,
                    width = frame.width,
                    height = frame.height,
                    bytes = frame.jpeg.len(),
                    jpeg_ms = jpg_at.elapsed().as_secs_f64() * 1000.0,
                    capture_api_ms = pixels.capture_latency.as_secs_f64() * 1000.0,
                    frame_age_ms = pixels.captured_at.elapsed().as_secs_f64() * 1000.0,
                    "student thumbnail captured"
                );
            }
        }
        // A share may have begun while a snapshot was being prepared.
        if presenting.load(Ordering::Acquire) {
            frame.jpeg.clear();
            frame.state = "paused".into();
        }
        send(socket, Payload::ThumbnailFrame(frame))?;
        info!(
            sequence,
            total_ms = started.elapsed().as_secs_f64() * 1000.0,
            "student thumbnail response complete"
        );
    }
    Ok(())
}
// One stable native capture per thumbnail session. Tier changes only resample a
// bounded 960×540 snapshot; no teardown/startup or blank initial capture frame.
fn resize_thumbnail(source: Vec<u8>, sw: u32, sh: u32, limit: (u32, u32)) -> (Vec<u8>, u32, u32) {
    let scale = (f64::from(limit.0) / f64::from(sw))
        .min(f64::from(limit.1) / f64::from(sh))
        .min(1.0);
    let w = ((f64::from(sw) * scale).floor() as u32 & !1).max(2).min(sw);
    let h = ((f64::from(sh) * scale).floor() as u32 & !1).max(2).min(sh);
    if w == sw && h == sh {
        return (source, w, h);
    }
    let mut out = vec![0; w as usize * h as usize * 4];
    for y in 0..h {
        let sy = ((f64::from(y) + 0.5) * f64::from(sh) / f64::from(h) - 0.5).max(0.0);
        let y0 = sy.floor() as u32;
        let y1 = (y0 + 1).min(sh - 1);
        let fy = sy - f64::from(y0);
        for x in 0..w {
            let sx = ((f64::from(x) + 0.5) * f64::from(sw) / f64::from(w) - 0.5).max(0.0);
            let x0 = sx.floor() as u32;
            let x1 = (x0 + 1).min(sw - 1);
            let fx = sx - f64::from(x0);
            for c in 0..4usize {
                let p = |x: u32, y: u32| f64::from(source[(y * sw + x) as usize * 4 + c]);
                out[(y * w + x) as usize * 4 + c] = ((p(x0, y0) * (1.0 - fx) + p(x1, y0) * fx)
                    * (1.0 - fy)
                    + (p(x0, y1) * (1.0 - fx) + p(x1, y1) * fx) * fy)
                    .round() as u8;
            }
        }
    }
    (out, w, h)
}
fn encode(
    pixels: &VideoFrame,
    quality: u8,
    sequence: u64,
    limits: (u32, u32),
) -> Result<ThumbnailFrame> {
    anyhow::ensure!(
        pixels.width > 0 && pixels.width <= 960 && pixels.height > 0 && pixels.height <= 540,
        "capture thumbnail bounds invalid"
    );
    let pitch = usize::try_from(pixels.row_pitch)?;
    let row = usize::try_from(pixels.width)? * 4;
    anyhow::ensure!(
        pitch >= row && pixels.bytes.len() >= pitch * usize::try_from(pixels.height)?,
        "capture thumbnail storage invalid"
    );
    let packed: Vec<u8> = pixels
        .bytes
        .chunks(pitch)
        .take(usize::try_from(pixels.height)?)
        .flat_map(|r| r[..row].iter().copied())
        .collect();
    let (packed, width, height) = resize_thumbnail(packed, pixels.width, pixels.height, limits);
    let mut jpeg = Vec::new();
    let mut encoder = jpeg_encoder::Encoder::new(&mut jpeg, quality);
    encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::F_1_1);
    encoder.encode(
        &packed,
        u16::try_from(width)?,
        u16::try_from(height)?,
        jpeg_encoder::ColorType::Bgra,
    )?;
    // Legacy Teachers accept only 48 KiB. Larger requests prove the Teacher
    // understands the expanded bounded budget; never queue multiple snapshots.
    let byte_budget = if limits.0 > 480 || limits.1 > 270 {
        MAX_JPEG
    } else {
        LEGACY_MAX_JPEG
    };
    for fallback in [75, 65, 50, 30] {
        if jpeg.len() <= byte_budget {
            break;
        }
        if fallback >= quality {
            continue;
        }
        jpeg.clear();
        jpeg_encoder::Encoder::new(&mut jpeg, fallback).encode(
            &packed,
            u16::try_from(width)?,
            u16::try_from(height)?,
            jpeg_encoder::ColorType::Bgra,
        )?;
    }
    anyhow::ensure!(
        jpeg.len() <= byte_budget,
        "thumbnail JPEG exceeds byte budget"
    );
    Ok(ThumbnailFrame {
        width,
        height,
        jpeg,
        state: "ready".into(),
        sequence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn class_source_retry_waits_for_receiver_teardown_with_original_deadline() {
        let jobs = ObserveRequests::default();
        jobs.request_broadcast("A".into());
        let began = jobs.entries.lock().unwrap()["A"].requested_at.unwrap();
        jobs.entries.lock().unwrap().get_mut("A").unwrap().request = false;
        let busy = protocol::ObserveMessage {
            error: "Student is watching a presentation".into(),
            ..Default::default()
        };
        jobs.complete_start("A", began, busy.clone(), began + Duration::from_millis(10))
            .unwrap();
        {
            let all = jobs.entries.lock().unwrap();
            assert!(all["A"].request);
            assert!(all["A"].error.is_empty());
            assert_eq!(all["A"].requested_at, Some(began));
        }
        jobs.entries.lock().unwrap().get_mut("A").unwrap().request = false;
        jobs.complete_start("A", began, busy, began + Duration::from_secs(4))
            .unwrap();
        let all = jobs.entries.lock().unwrap();
        assert!(!all["A"].request);
        assert_eq!(all["A"].error, "Student is watching a presentation");
    }
    #[test]
    fn ordinary_observe_and_real_class_errors_are_not_retried() {
        let jobs = ObserveRequests::default();
        for (class, error) in [
            (false, "Student is watching a presentation"),
            (true, "Screen Recording denied"),
        ] {
            jobs.request_profile("A".into(), None, class);
            let began = jobs.entries.lock().unwrap()["A"].requested_at.unwrap();
            jobs.entries.lock().unwrap().get_mut("A").unwrap().request = false;
            jobs.complete_start(
                "A",
                began,
                protocol::ObserveMessage {
                    error: error.into(),
                    ..Default::default()
                },
                began,
            )
            .unwrap();
            let all = jobs.entries.lock().unwrap();
            assert!(!all["A"].request);
            assert_eq!(all["A"].error, error);
        }
    }
    #[test]
    fn stale_source_reply_cannot_replace_a_newer_job() {
        let jobs = ObserveRequests::default();
        jobs.request_broadcast("A".into());
        let old = jobs.entries.lock().unwrap()["A"].requested_at.unwrap();
        jobs.request_broadcast("A".into());
        let newer = old + Duration::from_millis(1);
        jobs.entries
            .lock()
            .unwrap()
            .get_mut("A")
            .unwrap()
            .requested_at = Some(newer);
        jobs.complete_start(
            "A",
            old,
            protocol::ObserveMessage {
                port: 12345,
                token: "old".into(),
                ..Default::default()
            },
            newer,
        )
        .unwrap();
        let all = jobs.entries.lock().unwrap();
        assert_eq!(all["A"].port, 0);
        assert!(all["A"].token.is_empty());
        assert!(all["A"].request);
    }
    #[test]
    fn observe_bypasses_student_thumbnail_cadence_on_same_socket() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut student, _) = listener.accept().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let canceled = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let _ = capture_requests(
                &mut student,
                "test",
                &Arc::new(AtomicBool::new(true)),
                &Arc::new(AtomicBool::new(false)),
                &Arc::new(AtomicBool::new(false)),
                &canceled,
                &classroom_config::Config::load(None).unwrap().0,
                runtime.handle(),
            );
        });
        let _ = read(&mut peer).unwrap();
        send(
            &mut peer,
            Payload::ThumbnailRequest(ThumbnailRequest {
                width: 240,
                height: 135,
                interval_ms: 2000,
                quality: 65,
            }),
        )
        .unwrap();
        assert!(matches!(
            read(&mut peer).unwrap(),
            Payload::ThumbnailFrame(_)
        ));
        let began = Instant::now();
        send(
            &mut peer,
            Payload::Observe(protocol::ObserveMessage {
                action: "start".into(),
                ..Default::default()
            }),
        )
        .unwrap();
        assert!(matches!(read(&mut peer).unwrap(), Payload::Observe(_)));
        assert!(began.elapsed() < Duration::from_millis(500));
        stop.store(true, Ordering::Release);
        peer.shutdown(Shutdown::Both).unwrap();
        worker.join().unwrap();
    }
    #[test]
    fn observe_command_interrupts_pending_student_snapshot_interval() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut student, _) = listener.accept().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let canceled = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().unwrap();
            let _ = capture_requests(
                &mut student,
                "test",
                &Arc::new(AtomicBool::new(true)),
                &Arc::new(AtomicBool::new(false)),
                &Arc::new(AtomicBool::new(false)),
                &canceled,
                &classroom_config::Config::load(None).unwrap().0,
                runtime.handle(),
            );
        });
        let _ = read(&mut peer).unwrap();
        let request = || {
            Payload::ThumbnailRequest(ThumbnailRequest {
                width: 240,
                height: 135,
                interval_ms: 2000,
                quality: 65,
            })
        };
        send(&mut peer, request()).unwrap();
        assert!(matches!(
            read(&mut peer).unwrap(),
            Payload::ThumbnailFrame(_)
        ));
        // The Student now owes a 2 s snapshot interval. A command sent while that
        // request is pending must not wait for it.
        send(&mut peer, request()).unwrap();
        thread::sleep(Duration::from_millis(50));
        let began = Instant::now();
        send(
            &mut peer,
            Payload::Observe(protocol::ObserveMessage {
                action: "start".into(),
                ..Default::default()
            }),
        )
        .unwrap();
        let Payload::ThumbnailFrame(frame) = read(&mut peer).unwrap() else {
            panic!("pending snapshot request must be answered first");
        };
        assert!(frame.jpeg.is_empty());
        assert!(matches!(read(&mut peer).unwrap(), Payload::Observe(_)));
        assert!(
            began.elapsed() < Duration::from_millis(500),
            "Observe waited {:?}",
            began.elapsed()
        );
        stop.store(true, Ordering::Release);
        peer.shutdown(Shutdown::Both).unwrap();
        worker.join().unwrap();
    }
    #[test]
    fn observe_request_wakes_target_thumbnail_wait_without_waiting_for_interval() {
        let jobs = Arc::new(ObserveRequests::default());
        let stop = Arc::new(AtomicBool::new(false));
        let (started, ready) = std::sync::mpsc::channel();
        let (done, completed) = std::sync::mpsc::channel();
        let waiting_jobs = Arc::clone(&jobs);
        let waiting_stop = Arc::clone(&stop);
        let worker = thread::spawn(move || {
            started.send(()).unwrap();
            waiting_jobs.wait(&waiting_stop, "selected", Duration::from_secs(5));
            done.send(()).unwrap();
        });
        ready.recv_timeout(Duration::from_secs(1)).unwrap();
        jobs.request("other".into());
        assert!(completed.recv_timeout(Duration::from_millis(30)).is_err());
        jobs.request("selected".into());
        completed.recv_timeout(Duration::from_millis(300)).unwrap();
        worker.join().unwrap();
    }
    #[test]
    fn observe_wait_handles_preexisting_request_and_shutdown() {
        let jobs = Arc::new(ObserveRequests::default());
        jobs.request("selected".into());
        let stop = Arc::new(AtomicBool::new(false));
        let began = Instant::now();
        jobs.wait(&stop, "selected", Duration::from_secs(5));
        assert!(began.elapsed() < Duration::from_millis(300));
        let waiting_jobs = Arc::clone(&jobs);
        let waiting_stop = Arc::clone(&stop);
        let (done, completed) = std::sync::mpsc::channel();
        let worker = thread::spawn(move || {
            waiting_jobs.wait(&waiting_stop, "idle", Duration::from_secs(5));
            done.send(()).unwrap();
        });
        stop.store(true, Ordering::Release);
        jobs.wake_all();
        completed.recv_timeout(Duration::from_millis(300)).unwrap();
        worker.join().unwrap();
    }
    #[test]
    fn accepted_monitoring_socket_handles_delayed_framed_response() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut tx = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let (mut rx, _) = loop {
            match listener.accept() {
                Ok(value) => break value,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < deadline =>
                {
                    thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("accept deadline: {error}"),
            }
        };
        configure(&rx).unwrap();
        let writer = thread::spawn(move || {
            thread::sleep(Duration::from_millis(30));
            send(
                &mut tx,
                Payload::ThumbnailHello(ThumbnailHello {
                    instance_id: "registered".into(),
                    ..Default::default()
                }),
            )
            .unwrap();
        });
        assert!(
            matches!(read(&mut rx).unwrap(),Payload::ThumbnailHello(value) if value.instance_id=="registered")
        );
        writer.join().unwrap();
    }
    #[test]
    fn monitoring_rejects_oversized_frame_before_body_allocation() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut tx = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut rx, _) = listener.accept().unwrap();
        configure(&rx).unwrap();
        tx.write_all(&u32::MAX.to_be_bytes()).unwrap();
        assert!(read(&mut rx).is_err());
    }
    #[test]
    fn thumbnail_tiers_preserve_pixels_and_do_not_upscale() {
        let original = [24, 48, 192, 255].repeat(432 * 270);
        for (limit, expected) in [
            ((240, 135), (216, 134)),
            ((360, 202), (322, 202)),
            ((480, 270), (432, 270)),
        ] {
            let (pixels, w, h) = resize_thumbnail(original.clone(), 432, 270, limit);
            assert_eq!((w, h), expected);
            assert!(pixels.chunks_exact(4).all(|p| p == [24, 48, 192, 255]));
        }
        let (_, w, h) = resize_thumbnail(vec![255; 64 * 32 * 4], 64, 32, (480, 270));
        assert_eq!((w, h), (64, 32));
    }
    #[test]
    fn jpeg_roundtrip_respects_bgra_dimensions_and_rejects_mismatched_header() {
        let pixels = VideoFrame {
            bytes: [0_u8, 0, 240, 255].repeat(64 * 32),
            width: 64,
            height: 32,
            row_pitch: 256,
            captured_at: Instant::now(),
            dirty_region_ratio: 1.0,
            accumulated_frames: 1,
            pointer_visible: false,
            pointer_composited: false,
            protected_content_masked_out: false,
            capture_latency: Duration::ZERO,
        };
        let mut frame = encode(&pixels, 65, 1, (480, 270)).unwrap();
        let decoded = decode(&frame).unwrap();
        assert_eq!((decoded.width, decoded.height), (64, 32));
        assert!(decoded.pixels[0] >> 16 > 220);
        assert!(decoded.pixels[0] & 255 < 20);
        frame.width = 480;
        assert!(decode(&frame).is_err());
        frame.width = 481;
        assert!(decode(&frame).is_err());
    }
}
