//! Reverse publishing uses the exact presentation publisher and receiver, with a
//! native latest-frame texture sink. Flutter/Dart never transports video pixels.
#![allow(unsafe_code)]
use super::{
    info, register_student_for_presentation, thread, AtomicBool, Config, DecodedFrame, Duration,
    HashSet, Instant, Ipv4Addr, Mutex, Ordering, Result, SocketAddrV4, StudentRegistration,
};
use base64::Engine;
use control_transport::room::{RoomPresentation, RoomRoster};
use fs2::FileExt;
use memmap2::MmapMut;
use std::{
    fs::{File, OpenOptions},
    io::BufRead,
    path::Path,
    sync::{mpsc, Arc},
};
use tokio::sync::watch;
const HEADER: usize = 32;
const CAPACITY: usize = HEADER + 1920 * 1080 * 4;

/// Latest interval only; no video data or history crosses the UI control pipe.
#[derive(Clone, Default)]
pub struct ReceiverMetrics {
    pub receive_mbps: f64,
    pub sampled_unix_us: u64,
}

pub struct Publisher {
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<Result<()>>>,
    pub port: u16,
    pub token: String,
    /// Negotiated session profile label, e.g. "h265@60".
    pub profile: String,
}
impl Publisher {
    pub fn is_finished(&self) -> bool {
        self.worker
            .as_ref()
            .is_none_or(thread::JoinHandle::is_finished)
    }
    #[allow(clippy::too_many_lines)]
    pub fn start(
        config: &Config,
        request: &protocol::ObserveMessage,
        ip: Ipv4Addr,
        handle: &tokio::runtime::Handle,
        presenting: Arc<AtomicBool>,
        observer_ip: Ipv4Addr,
    ) -> Result<Self> {
        let listener = std::net::TcpListener::bind((ip, 0))?;
        let port = listener.local_addr()?.port();
        listener.set_nonblocking(true)?;
        let listener = {
            let _entered = handle.enter();
            tokio::net::TcpListener::from_std(listener)?
        };
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("Observe nonce: {e}"))?;
        let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
        let recipients = Arc::new(HashSet::from([token.clone()]));
        let (state, updates) = watch::channel::<Option<RoomPresentation>>(None);
        let roster = Arc::new(Mutex::new(RoomRoster::default()));
        let server = handle.spawn(control_transport::room::run_authorized_listener(
            listener,
            updates,
            Arc::clone(&roster),
            observer_ip,
            token.clone(),
        ));
        let stop = Arc::new(AtomicBool::new(false));
        let cancel = Arc::clone(&stop);
        let cursor = Arc::new(AtomicBool::new(true));
        let mut settings = config.clone();
        settings.network.feedback_port = 0; // OS allocates one UDP source per publisher.
                                            // Pointer-only changes otherwise select the static-slide 5 FPS policy.
                                            // Reverse desktop interaction needs fresh menus even at low activity.
        settings.video.adaptive_idle_fps = settings.video.max_fps.min(20);
        // One-to-one sessions may use a richer profile when both ends have the
        // hardware for it; otherwise this is exactly the broadcast profile.
        let profile = super::stream_profile::choose(
            &request.codecs,
            request.max_fps,
            config,
            super::stream_profile::local_encodable,
            super::stream_profile::hardware_encode,
        );
        profile.apply(&mut settings);
        // Observe/Control manage the source cursor themselves (local Teacher
        // pointer during Control); the class pointer channel is not used here.
        settings.video.pointer_channel = false;
        info!(
            idle_fps = settings.video.adaptive_idle_fps,
            offered = ?request.codecs,
            offered_fps = request.max_fps,
            profile = %profile.label(),
            enhanced = profile.enhanced,
            bitrate_kbps = settings.video.target_bitrate_kbps,
            "Observe interactive freshness policy"
        );
        let ids = super::presentation_ids::PresentationIds::new()?;
        let mut ids = ids;
        let stream_id = ids.next()?;
        let worker = thread::spawn(move || {
            let media_stop = Arc::clone(&cancel);
            let sender = state.clone();
            let source_cursor = Arc::clone(&cursor);
            let source = thread::spawn(move || {
                super::media_session::publish_with_cursor_policy(
                    &settings,
                    ip,
                    0,
                    stream_id,
                    &media_stop,
                    &sender,
                    Some(&recipients),
                    Some(source_cursor),
                )
            });
            let mut input = super::remote_input::Controller::default();
            let mut had_viewer = false;
            let began = Instant::now();
            while !cancel.load(Ordering::Acquire)
                && !source.is_finished()
                && !presenting.load(Ordering::Acquire)
            {
                let online = roster.lock().unwrap().snapshot().iter().any(|r| r.online);
                had_viewer |= online;
                if (had_viewer && !online)
                    || (!had_viewer && began.elapsed() > Duration::from_secs(15))
                {
                    break;
                }
                for _ in 0..64 {
                    let message = roster.lock().unwrap().take_input();
                    let Some(message) = message else { break };
                    let result = input.apply(&message);
                    if result.is_err() {
                        input.release();
                    }
                    // Avoid a lagging second pointer in the H.264 image during
                    // Control; Observe still shows the actual Student pointer.
                    let visible = !input.enabled();
                    if cursor.swap(visible, Ordering::AcqRel) != visible {
                        info!(
                            source_cursor_visible = visible,
                            "Observe cursor policy changed"
                        );
                    }
                    if ["observe", "control", "exclusive"].contains(&message.action.as_str())
                        || result.is_err()
                    {
                        roster
                            .lock()
                            .unwrap()
                            .input_status(protocol::ObserveMessage {
                                action: "status".into(),
                                key: if result.is_ok() {
                                    message.action.clone()
                                } else {
                                    "observe".into()
                                },
                                error: result.err().map(|e| format!("{e:#}")).unwrap_or_default(),
                                ..Default::default()
                            });
                    }
                }
                if input.expire() {
                    cursor.store(true, Ordering::Release);
                    roster
                        .lock()
                        .unwrap()
                        .input_status(protocol::ObserveMessage {
                            action: "status".into(),
                            key: "observe".into(),
                            error: "Input lease expired; Student input released".into(),
                            ..Default::default()
                        });
                }
                thread::sleep(Duration::from_millis(5));
            }
            drop(input); // release injected keys/buttons and exclusive input before join
            cancel.store(true, Ordering::Release);
            state.send_replace(None);
            server.abort();
            let result = source
                .join()
                .map_err(|_| anyhow::anyhow!("Observe publisher panicked"))?;
            info!(stream_id, "Observe publisher ended");
            result
        });
        Ok(Self {
            stop,
            worker: Some(worker),
            port,
            token,
            profile: profile.label(),
        })
    }
}
impl Drop for Publisher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

pub struct Output {
    file: File,
    map: MmapMut,
    inputs: mpsc::Receiver<Option<protocol::ObserveMessage>>,
    ended: bool,
    sequence: u64,
    delay: media_core::diagnostics::StageMetrics,
    /// Negotiated codec and fps ceiling, reported with metrics for the UI.
    profile: Option<(media_core::VideoCodec, u32)>,
}
impl Output {
    fn new(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .read(true)
            .write(true)
            .open(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        file.set_len(CAPACITY as u64)?;
        // SAFETY: fixed-size private IPC file; never truncated while this map lives.
        // Both producer and native consumer use a nonblocking exclusive flock.
        let map = unsafe { MmapMut::map_mut(&file)? };
        let (tx, inputs) = mpsc::sync_channel(64);
        thread::spawn(move || {
            for line in std::io::stdin().lock().lines() {
                let Ok(line) = line else { break };
                if line.len() > 4096 {
                    break;
                }
                let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
                    break;
                };
                let event = protocol::ObserveMessage {
                    action: v["action"].as_str().unwrap_or("").to_owned(),
                    x: v["x"].as_f64().unwrap_or(0.0),
                    y: v["y"].as_f64().unwrap_or(0.0),
                    key: v["key"].as_str().unwrap_or("").to_owned(),
                    down: v["down"].as_bool().unwrap_or(false),
                    ..Default::default()
                };
                if tx.send(Some(event)).is_err() {
                    return;
                }
            }
            let _ = tx.send(None);
        });
        Ok(Self {
            file,
            map,
            inputs,
            ended: false,
            sequence: 0,
            delay: media_core::diagnostics::StageMetrics::default(),
            profile: None,
        })
    }
    pub fn record_delay(&mut self, age: Duration) {
        self.delay.record("capture_to_surface", age);
    }
    pub fn report_metrics(&mut self, receiver: &ReceiverMetrics, uncertainty_us: Option<u64>) {
        let delay = std::mem::take(&mut self.delay).summaries().pop();
        let delay_us = uncertainty_us.and_then(|_| delay.as_ref().map(|s| s.p50_us));
        println!(
            "{}",
            serde_json::json!({"metrics":{
                "sampled_unix_us": receiver.sampled_unix_us,
                "receive_mbps": receiver.receive_mbps,
                "capture_to_surface_ms_p50": delay_us.map(|us| Duration::from_micros(us).as_secs_f64() * 1000.0),
                "delay_samples":delay.as_ref().map_or(0, |s| s.samples),
                "codec": self.profile.map(|(codec, _)| codec.to_string()),
                "fps_cap": self.profile.map(|(_, fps)| fps),
                "clock_uncertainty_ms":uncertainty_us.map(|us| Duration::from_micros(us).as_secs_f64() * 1000.0)
            }})
        );
    }
    pub fn closed(&self) -> bool {
        self.ended
    }
    pub fn pump(
        &mut self,
        control: &control_transport::StudentControl,
        stream_id: u64,
    ) -> Result<()> {
        if let Some(status) = control.take_input_status() {
            println!(
                "{}",
                serde_json::json!({"mode":status.key,"error":status.error})
            );
        }
        for _ in 0..64 {
            match self.inputs.try_recv() {
                Ok(Some(mut event)) => {
                    if event.action == "close" {
                        self.ended = true;
                        break;
                    }
                    if stream_id != 0 {
                        event.stream_id = stream_id;
                        control.observe_input(event)?;
                    }
                }
                Ok(None) | Err(mpsc::TryRecvError::Disconnected) => {
                    self.ended = true;
                    break;
                }
                Err(mpsc::TryRecvError::Empty) => break,
            }
        }
        Ok(())
    }
    pub fn present(&mut self, frame: &DecodedFrame) -> Result<bool> {
        anyhow::ensure!(
            frame.format == media_core::DecodedPixelFormat::Bgra,
            "Native Observe texture requires BGRA"
        );
        anyhow::ensure!(
            frame.width <= 1920
                && frame.height <= 1080
                && frame.bytes.len() == frame.width as usize * frame.height as usize * 4,
            "Invalid Observe pixels"
        );
        if self.file.try_lock_exclusive().is_err() {
            return Ok(false);
        } // latest frame, no UI wait
        self.sequence += 1;
        self.map[0..4].copy_from_slice(&frame.width.to_le_bytes());
        self.map[4..8].copy_from_slice(&frame.height.to_le_bytes());
        self.map[8..16].copy_from_slice(&self.sequence.to_le_bytes());
        self.map[16..24].copy_from_slice(&media_core::unix_us().to_le_bytes());
        self.map[HEADER..HEADER + frame.bytes.len()].copy_from_slice(&frame.bytes);
        FileExt::unlock(&self.file)?;
        Ok(true)
    }
}

pub async fn receive(
    config: &Config,
    address: SocketAddrV4,
    token: &str,
    path: &Path,
) -> Result<()> {
    let mut output = Output::new(path)?;
    let socket = super::bind_media_socket(0)?;
    let port = socket.local_addr()?.port();
    let (control, id, initial) = register_student_for_presentation(
        address,
        StudentRegistration {
            hostname: "Teacher Observe".into(),
            instance_id: token.into(),
            selected_local_ip: Ipv4Addr::UNSPECIFIED,
            media_udp_port: port,
            feedback_udp_port: port,
            pointer_overlay: false,
        },
    )
    .await?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut initial = initial;
    loop {
        output.pump(&control, 0)?;
        if output.closed() || !control.is_connected() {
            return Ok(());
        }
        if let Some(p) = initial.take().or_else(|| control.presentation()) {
            let endpoint =
                SocketAddrV4::new(*address.ip(), u16::try_from(p.teacher_media_udp_port)?);
            output.profile = media_core::VideoCodec::from_rtp_payload_type(p.rtp_payload_type)
                .map(|codec| (codec, p.max_fps));
            info!(
                stream_id = p.stream_id,
                width = p.width,
                height = p.height,
                payload_type = p.rtp_payload_type,
                max_fps = p.max_fps,
                "Observe live receiver ready"
            );
            let result = super::run_student_media_loop(
                config,
                &p,
                &control,
                id,
                endpoint,
                None,
                &socket,
                false,
                &mut None,
                Some(&mut output),
            );
            return result.map(|_| ());
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "Observe publisher did not become ready"
        );
        let _ = control.probe_rtt();
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
