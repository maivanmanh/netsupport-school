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
    /// Teacher's enhanced-profile offer (codecs it decodes, fps ceiling).
    offer: (Vec<String>, u32),
}
impl ObserveRequests {
    pub fn with_offer(offer: (Vec<String>, u32)) -> Self {
        Self {
            offer,
            ..Self::default()
        }
    }
    pub fn set_thumbnail_width(&self, width: u32) -> Result<()> {
        anyhow::ensure!([240, 360, 480].contains(&width), "Invalid thumbnail tier");
        self.thumbnail_width.store(width, Ordering::Release);
        info!(width, "Teacher thumbnail tier changed");
        Ok(())
    }
    pub fn request(&self, id: String) {
        let mut entries = self.entries.lock().unwrap();
        entries.insert(
            id,
            ObserveJob {
                request: true,
                requested_at: Some(Instant::now()),
                ..Default::default()
            },
        );
        self.wake.notify_all();
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
}

const MAX_JPEG: usize = 48 * 1024;
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
        data.len() <= protocol::MAX_CONTROL_FRAME_BYTES,
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
        size > 0 && size <= protocol::MAX_CONTROL_FRAME_BYTES,
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
        if roster
            .lock()
            .unwrap()
            .monitoring_client(&hello.instance_id, peer)
            != Some(client_id)
        {
            break;
        }
        let request = jobs
            .entries
            .lock()
            .unwrap()
            .get_mut(&hello.instance_id)
            .and_then(|j| {
                let r = j
                    .request
                    .then(|| j.requested_at.unwrap_or_else(Instant::now));
                j.request = false;
                r
            });
        if let Some(queued_at) = request {
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
                    codecs: jobs.offer.0.clone(),
                    max_fps: jobs.offer.1,
                    ..Default::default()
                }),
            )?;
            let Payload::Observe(reply) = read(socket)? else {
                anyhow::bail!("Expected Observe response")
            };
            info!(
                client_id,
                handshake_ms = handshake.elapsed().as_secs_f64() * 1000.0,
                offered = ?jobs.offer,
                profile = %reply.codec,
                "Observe source handshake complete"
            );
            let mut all = jobs.entries.lock().unwrap();
            if let Some(j) = all.get_mut(&hello.instance_id) {
                j.port = u16::try_from(reply.port)?;
                j.token = reply.token;
                j.error = reply.error;
                j.profile = reply.codec;
            }
        }
        let tier = jobs.thumbnail_width.load(Ordering::Acquire);
        let width = match tier {
            0 => settings.width,
            width => width,
        };
        let height = if tier == 0 {
            settings.height
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
                quality: u32::from(settings.jpeg_quality),
            }),
        )?;
        let Payload::ThumbnailFrame(frame) = read(socket)? else {
            anyhow::bail!("expected thumbnail response");
        };
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
        frame.width > 0 && frame.width <= 480 && frame.height > 0 && frame.height <= 270,
        "thumbnail dimensions exceed budget"
    );
    let mut decoder = jpeg_decoder::Decoder::new(frame.jpeg.as_slice());
    decoder.set_max_decoding_buffer_size(480 * 270 * 4);
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
    stop: &AtomicBool,
    config: &classroom_config::Config,
    runtime: &tokio::runtime::Handle,
) -> Result<()> {
    configure(socket)?;
    send(
        socket,
        Payload::ThumbnailHello(ThumbnailHello {
            instance_id: identity.to_owned(),
        }),
    )?;
    let mut publisher: Option<super::observation::Publisher> = None;
    let mut capture: Option<PlatformCapture> = None;
    let mut retiring = CaptureRetirement::default();
    let mut dimensions = None;
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
                publisher.take();
                if let Some(old) = capture.take() {
                    retiring.retire(old)?;
                }
                previous = None;
                dimensions = None;
                let SocketAddr::V4(local) = socket.local_addr()? else {
                    anyhow::bail!("IPv4 required")
                };
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
            (2..=480).contains(&request.width)
                && (2..=270).contains(&request.height)
                && (1000..=10000).contains(&request.interval_ms)
                && (30..=85).contains(&request.quality),
            "invalid thumbnail request budget"
        );
        if publisher
            .as_ref()
            .is_some_and(super::observation::Publisher::is_finished)
        {
            publisher.take();
        }
        // Read control immediately after each response. Rate-limit only snapshots,
        // never the Observe command waiting on this connection.
        sleep(
            stop,
            thumbnail_due.saturating_duration_since(Instant::now()),
        );
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
            capture = None;
            previous = None;
            dimensions = None;
            frame.state = "paused".into();
        } else if publisher.is_some() {
            frame.state = "observing".into();
            capture = None;
            previous = None;
            dimensions = None;
        } else {
            let requested = (request.width, request.height);
            if dimensions != Some(requested) {
                capture = None;
                previous = None;
            }
            if capture.is_none() && Instant::now() >= retry_at {
                let mut source = PlatformCapture::new();
                match source.start_thumbnail(CaptureConfig {
                    display_index: 0,
                    width: request.width,
                    height: request.height,
                    max_fps: 1,
                }) {
                    Ok(()) => {
                        capture = Some(source);
                        dimensions = Some(requested);
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
                frame = encode(pixels, u8::try_from(request.quality)?, sequence)?;
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
fn encode(pixels: &VideoFrame, quality: u8, sequence: u64) -> Result<ThumbnailFrame> {
    anyhow::ensure!(
        pixels.width > 0 && pixels.width <= 480 && pixels.height > 0 && pixels.height <= 270,
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
    let mut jpeg = Vec::new();
    jpeg_encoder::Encoder::new(&mut jpeg, quality).encode(
        &packed,
        u16::try_from(pixels.width)?,
        u16::try_from(pixels.height)?,
        jpeg_encoder::ColorType::Bgra,
    )?;
    if jpeg.len() > MAX_JPEG {
        jpeg.clear();
        jpeg_encoder::Encoder::new(&mut jpeg, 30).encode(
            &packed,
            u16::try_from(pixels.width)?,
            u16::try_from(pixels.height)?,
            jpeg_encoder::ColorType::Bgra,
        )?;
    }
    anyhow::ensure!(jpeg.len() <= MAX_JPEG, "thumbnail JPEG exceeds byte budget");
    Ok(ThumbnailFrame {
        width: pixels.width,
        height: pixels.height,
        jpeg,
        state: "ready".into(),
        sequence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let mut frame = encode(&pixels, 65, 1).unwrap();
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
