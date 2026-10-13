//! Student encodes/packetizes once; Teacher relays compressed RTP unchanged
//! except the session SSRC. No decoder, encoder, frame queue or disk video.
use anyhow::{Context, Result};
use control_transport::{room::RoomPresentation, StudentRegistration, TeacherMediaEvent};
use media_transport::RtpPacket;
use std::{
    collections::{HashMap, HashSet},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};
use tokio::sync::watch;

struct RequestGuard {
    id: String,
    jobs: super::monitoring::ObserveJobs,
}
impl Drop for RequestGuard {
    fn drop(&mut self) {
        self.jobs.entries.lock().unwrap().remove(&self.id);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    config: &classroom_config::Config,
    bind_ip: Ipv4Addr,
    source_id: String,
    source_ip: Ipv4Addr,
    stream_id: u64,
    stop: &AtomicBool,
    state: &watch::Sender<Option<RoomPresentation>>,
    jobs: super::monitoring::ObserveJobs,
    runtime: tokio::runtime::Handle,
) -> Result<()> {
    let _request = RequestGuard {
        id: source_id.clone(),
        jobs: Arc::clone(&jobs),
    };
    jobs.request_broadcast(source_id.clone());
    let began = Instant::now();
    let job = loop {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        anyhow::ensure!(
            began.elapsed() < Duration::from_secs(12),
            "Student broadcast source did not answer"
        );
        let job = jobs
            .entries
            .lock()
            .unwrap()
            .get(&source_id)
            .cloned()
            .context("Student broadcast canceled")?;
        anyhow::ensure!(job.error.is_empty(), "{}", job.error);
        if job.port != 0 && !job.token.is_empty() {
            break job;
        }
        thread::sleep(Duration::from_millis(20));
    };
    let upstream = super::bind_media_socket(0)?;
    upstream.set_nonblocking(true)?;
    let port = upstream.local_addr()?.port();
    let (control, client_id, initial) =
        runtime.block_on(control_transport::register_student_for_presentation(
            SocketAddrV4::new(source_ip, job.port),
            StudentRegistration {
                hostname: "Classroom relay".into(),
                instance_id: job.token,
                selected_local_ip: bind_ip,
                media_udp_port: port,
                feedback_udp_port: port,
                pointer_overlay: false,
            },
        ))?;
    let mut initial = initial;
    let source = loop {
        if stop.load(Ordering::Acquire) {
            return Ok(());
        }
        anyhow::ensure!(
            control.is_connected() && began.elapsed() < Duration::from_secs(20),
            "Student broadcast source did not start"
        );
        if let Some(p) = initial.take().or_else(|| control.presentation()) {
            break p;
        }
        let _ = control.probe_rtt();
        thread::sleep(Duration::from_millis(20));
    };
    anyhow::ensure!(
        source.rtp_payload_type == u32::from(media_transport::H264_PAYLOAD_TYPE)
            && source.rtp_clock_hz == media_transport::RTP_VIDEO_CLOCK_HZ
            && source.width > 0
            && source.width <= 1920
            && source.height > 0
            && source.height <= 1080
            && source.max_fps <= 30,
        "Student broadcast requires bounded H.264/30 profile"
    );
    let source_endpoint =
        SocketAddrV4::new(source_ip, u16::try_from(source.teacher_media_udp_port)?);
    upstream
        .send_to(&super::media_route_probe(client_id), source_endpoint)
        .context("Student source initial UDP route probe failed")?;
    let socket = super::bind_teacher_media_socket(bind_ip, 0)?;
    let pacer = Arc::new(super::udp_sender::AggregatePacer::new(
        config.network.send_pacing_mbps,
    ));
    let repair = super::teacher_repair::RepairService::for_stream_paced(
        stream_id,
        socket.try_clone()?,
        Duration::from_millis(u64::from(config.network.retransmission_cache_ms)),
        Arc::clone(&pacer),
    )?;
    let (events, receiver) = mpsc::sync_channel(256);
    let mut presentation = source.clone();
    presentation.preserve_aspect = config.video.presentation_preserve_aspect;
    presentation.stream_id = stream_id;
    presentation.teacher_media_udp_port = u32::from(socket.local_addr()?.port());
    if stop.load(Ordering::Acquire) {
        return Ok(());
    }
    state.send_replace(Some(RoomPresentation {
        config: presentation,
        events,
        recipients: None,
        excluded_source: Some(source_id.clone()),
    }));
    let router_stop = AtomicBool::new(false);
    let (forward, incoming) = mpsc::sync_channel(256);
    let result = thread::scope(|scope| {
        let router = scope.spawn(|| repair.route(receiver, forward, &router_stop));
        let result = (|| -> Result<()> {
            let mut endpoints = HashMap::new();
            let mut ready = HashSet::new();
            let mut gate = super::keyframe_gate::KeyframeGate::new(Duration::from_millis(400));
            let mut buffer = [0u8; 2048];
            let mut last_packet = Instant::now();
            let mut last_ping = Instant::now() - Duration::from_secs(1);
            let mut highest = None;
            let mut packets = 0u64;
            let mut sent = 0u64;
            let mut errors = 0u64;
            let mut send_deadline_errors = 0u64;
            let mut last_send_error = String::new();
            let mut last_metrics = Instant::now();
            while !stop.load(Ordering::Acquire) {
                anyhow::ensure!(
                    control.is_presenting(source.stream_id),
                    "Student broadcast source disconnected/stopped"
                );
                anyhow::ensure!(
                    last_packet.elapsed() < Duration::from_secs(10),
                    "Student broadcast source media timed out"
                );
                for _ in 0..64 {
                    let Ok((_, event)) = incoming.try_recv() else {
                        break;
                    };
                    match event {
                        TeacherMediaEvent::Registered(r) => {
                            endpoints.insert(r.client_id, r.endpoint);
                            gate.request(Instant::now());
                        }
                        TeacherMediaEvent::Disconnected { client_id, .. } => {
                            endpoints.remove(&client_id);
                            ready.remove(&client_id);
                        }
                        TeacherMediaEvent::KeyframeRequested { .. } => gate.request(Instant::now()),
                        _ => {}
                    }
                }
                for _ in 0..64 {
                    match socket.recv_from(&mut buffer) {
                        Ok((n, SocketAddr::V4(peer))) => {
                            if let Some(id) = super::parse_media_route_probe(&buffer[..n]) {
                                if let Some(endpoint) = endpoints
                                    .get_mut(&id)
                                    .filter(|endpoint| endpoint.ip() == peer.ip())
                                {
                                    *endpoint = peer;
                                    repair.ready(id, peer);
                                    if ready.insert(id) {
                                        tracing::info!(client_id = id, endpoint = %peer, "Student relay established class UDP route");
                                        gate.request(Instant::now());
                                    }
                                }
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                        Err(e) => {
                            // Match the Teacher publisher: an ICMP/route error
                            // from one endpoint must not stop the whole class.
                            tracing::warn!(%e, "Student relay class UDP route receive failed");
                            break;
                        }
                        _ => {}
                    }
                }
                if repair.stats.keyframe_pending.swap(false, Ordering::AcqRel) {
                    gate.request(Instant::now());
                }
                if gate.poll(Instant::now()) {
                    control
                        .request_keyframe(protocol::RequestKeyframe {
                            stream_id: source.stream_id,
                            ..Default::default()
                        })
                        .context("Student source keyframe control request failed")?;
                }
                if last_ping.elapsed() >= Duration::from_secs(1) {
                    control.probe_rtt()?;
                    source_route_keepalive(
                        upstream.send_to(&super::media_route_probe(client_id), source_endpoint),
                    );
                    last_ping = Instant::now();
                }
                for _ in 0..256 {
                    let n = match upstream.recv_from(&mut buffer) {
                        Ok((n, peer)) if peer == SocketAddr::V4(source_endpoint) => n,
                        Ok(_) => continue,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                        Err(e) => return Err(e).context("Student source UDP receive failed"),
                    };
                    let Ok(packet) = RtpPacket::parse(&buffer[..n]) else {
                        continue;
                    };
                    if packet.ssrc != protocol::presentation_ssrc(source.stream_id)
                        || packet.payload_type != media_transport::H264_PAYLOAD_TYPE
                    {
                        continue;
                    }
                    let sequence = packet.sequence_number;
                    if let Some(previous) = highest {
                        let delta = sequence.wrapping_sub(previous);
                        if delta > 1 && delta < 32768 {
                            if delta <= 33 {
                                control.nack(protocol::Nack {
                                    stream_id: source.stream_id,
                                    sequence_numbers: (1..delta)
                                        .map(|i| u32::from(previous.wrapping_add(i)))
                                        .collect(),
                                })?;
                            } else {
                                gate.request(Instant::now());
                            }
                        }
                        if delta > 0 && delta < 32768 {
                            highest = Some(sequence);
                        }
                    } else {
                        highest = Some(sequence);
                    }
                    let now = Instant::now();
                    last_packet = now;
                    if packets == 0 {
                        tracing::info!(endpoint = %source_endpoint, "Student relay received first source RTP packet");
                    }
                    packets += 1;
                    rewrite_ssrc(&mut buffer[..n], stream_id)?;
                    repair.cache(sequence, &buffer[..n], now);
                    let deadline = now + Duration::from_millis(2);
                    let mut budget =
                        super::udp_sender::SendBudget::new(now, Duration::from_millis(2));
                    for (id, endpoint) in &endpoints {
                        if ready.contains(id) {
                            match budget.send_to_paced(
                                &socket,
                                &buffer[..n],
                                *endpoint,
                                &pacer,
                                deadline,
                            ) {
                                Ok(_) => sent += 1,
                                Err(error) => {
                                    errors += 1;
                                    send_deadline_errors +=
                                        u64::from(error.kind() == std::io::ErrorKind::TimedOut);
                                    last_send_error = error.to_string().chars().take(160).collect();
                                }
                            }
                        }
                    }
                }
                if last_metrics.elapsed() >= Duration::from_secs(5) {
                    tracing::info!(
                        stream_id,
                        source_id,
                        received_packets = packets,
                        sent_packets = sent,
                        send_errors = errors,
                        send_deadline_errors,
                        last_send_error,
                        recipients = ready.len(),
                        active_encoder_instances = 0,
                        "Student broadcast relay interval"
                    );
                    last_metrics = Instant::now();
                }
                thread::sleep(Duration::from_millis(1));
            }
            Ok(())
        })();
        router_stop.store(true, Ordering::Release);
        let _ = router.join();
        result
    });
    state.send_replace(None);
    tracing::info!(stream_id, "Student broadcast relay ended");
    result
}

fn rewrite_ssrc(packet: &mut [u8], stream_id: u64) -> Result<()> {
    RtpPacket::parse(packet)?;
    packet[8..12].copy_from_slice(&protocol::presentation_ssrc(stream_id).to_be_bytes());
    Ok(())
}

/// Route refresh is advisory after initial registration. Its failure must not
/// discard fresh video already arriving from the source. TCP liveness and the
/// independent ten-second media watchdog still stop a genuinely dead source.
fn source_route_keepalive(result: std::io::Result<usize>) {
    if let Err(error) = result {
        tracing::warn!(%error, "Student source UDP route keepalive failed; retaining live media and retrying next tick");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Worker {
        stop: Arc<AtomicBool>,
        thread: Option<thread::JoinHandle<Result<()>>>,
    }
    impl Worker {
        fn finish(mut self) -> Result<()> {
            self.stop.store(true, Ordering::Release);
            self.thread.take().unwrap().join().unwrap()
        }
    }
    impl Drop for Worker {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            if let Some(worker) = self.thread.take() {
                let _ = worker.join();
            }
        }
    }
    fn worker(
        state: watch::Sender<Option<RoomPresentation>>,
        jobs: super::super::monitoring::ObserveJobs,
        runtime: tokio::runtime::Handle,
    ) -> Worker {
        let stop = Arc::new(AtomicBool::new(false));
        let cancel = Arc::clone(&stop);
        Worker {
            stop,
            thread: Some(thread::spawn(move || {
                let config = classroom_config::Config::load(None)?.0;
                run(
                    &config,
                    Ipv4Addr::LOCALHOST,
                    "source".into(),
                    Ipv4Addr::LOCALHOST,
                    100,
                    &cancel,
                    &state,
                    jobs,
                    runtime,
                )
            })),
        }
    }
    async fn requested(jobs: &super::super::monitoring::ObserveJobs) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while !jobs.entries.lock().unwrap().contains_key("source") {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }

    #[test]
    fn failed_advisory_keepalive_does_not_return_a_session_error() {
        source_route_keepalive(Ok(12));
        source_route_keepalive(Err(std::io::Error::from_raw_os_error(65)));
        source_route_keepalive(Err(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "fixture",
        )));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancel_before_source_reply_cleans_job_without_starting_class() {
        let jobs = Arc::new(super::super::monitoring::ObserveRequests::default());
        let (state, updates) = watch::channel(None);
        let worker = worker(state, Arc::clone(&jobs), tokio::runtime::Handle::current());
        requested(&jobs).await;
        assert!(updates.borrow().is_none());
        worker.finish().unwrap();
        assert!(jobs.entries.lock().unwrap().is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn source_failure_before_registration_cleans_job_and_class() {
        let jobs = Arc::new(super::super::monitoring::ObserveRequests::default());
        let (state, updates) = watch::channel(None);
        let mut worker = worker(state, Arc::clone(&jobs), tokio::runtime::Handle::current());
        requested(&jobs).await;
        jobs.entries
            .lock()
            .unwrap()
            .get_mut("source")
            .unwrap()
            .error = "fixture capture unavailable".into();
        let result =
            tokio::task::spawn_blocking(move || worker.thread.take().unwrap().join().unwrap())
                .await
                .unwrap();
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("capture unavailable"));
        assert!(updates.borrow().is_none());
        assert!(jobs.entries.lock().unwrap().is_empty());
    }

    // Real loopback TCP/UDP, fake RTP source: tests control/relay/repair/lifecycle,
    // not capture, codec, renderer or physical classroom performance.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn loopback_late_join_repair_and_stop_keep_rtp_payload_unchanged() {
        use control_transport::room::{run_authorized_listener, RoomRoster};
        use std::sync::Mutex;
        let source_udp = tokio::net::UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let source_tcp = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let source_port = source_tcp.local_addr().unwrap().port();
        let (source_events, _source_messages) = mpsc::sync_channel(128);
        let (source_state, source_updates) = watch::channel(Some(RoomPresentation {
            config: protocol::StartPresentation {
                stream_id: 17,
                width: 640,
                height: 480,
                max_fps: 30,
                bitrate_bps: 6_000_000,
                rtp_payload_type: 96,
                rtp_clock_hz: 90_000,
                teacher_media_udp_port: u32::from(source_udp.local_addr().unwrap().port()),
                preserve_aspect: false,
            },
            events: source_events,
            recipients: Some(Arc::new(HashSet::from(["source-auth".into()]))),
            excluded_source: None,
        }));
        let source_server = tokio::spawn(run_authorized_listener(
            source_tcp,
            source_updates,
            Arc::new(Mutex::new(RoomRoster::default())),
            Ipv4Addr::LOCALHOST,
            "source-auth".into(),
        ));
        let jobs = Arc::new(super::super::monitoring::ObserveRequests::default());
        let (class_state, mut class_updates) = watch::channel(None);
        let class_tcp = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let class_port = class_tcp.local_addr().unwrap().port();
        let class_server = tokio::spawn(run_authorized_listener(
            class_tcp,
            class_updates.clone(),
            Arc::new(Mutex::new(RoomRoster::default())),
            Ipv4Addr::LOCALHOST,
            "receiver-auth".into(),
        ));
        let worker = worker(
            class_state,
            Arc::clone(&jobs),
            tokio::runtime::Handle::current(),
        );
        requested(&jobs).await;
        {
            let mut entries = jobs.entries.lock().unwrap();
            let job = entries.get_mut("source").unwrap();
            job.port = source_port;
            job.token = "source-auth".into();
        }
        tokio::time::timeout(Duration::from_secs(2), class_updates.changed())
            .await
            .unwrap()
            .unwrap();
        let mut bytes = [0; 2048];
        let (_, relay_upstream) =
            tokio::time::timeout(Duration::from_secs(2), source_udp.recv_from(&mut bytes))
                .await
                .unwrap()
                .unwrap();
        let receiver = tokio::net::UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let port = receiver.local_addr().unwrap().port();
        let (control, id, initial) = control_transport::register_student_for_presentation(
            SocketAddrV4::new(Ipv4Addr::LOCALHOST, class_port),
            StudentRegistration {
                hostname: "receiver".into(),
                instance_id: "receiver-auth".into(),
                selected_local_ip: Ipv4Addr::LOCALHOST,
                media_udp_port: port,
                feedback_udp_port: port,
                pointer_overlay: false,
            },
        )
        .await
        .unwrap();
        let presentation = initial.unwrap();
        assert_eq!(presentation.stream_id, 100);
        assert!(!class_updates.borrow().as_ref().unwrap().includes("source"));
        let relay_class = SocketAddrV4::new(
            Ipv4Addr::LOCALHOST,
            presentation.teacher_media_udp_port as u16,
        );
        receiver
            .send_to(&super::super::media_route_probe(id), relay_class)
            .await
            .unwrap();
        let mut sequence = 0;
        let received = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let packet = RtpPacket {
                    marker: true,
                    payload_type: 96,
                    sequence_number: sequence,
                    timestamp: u32::from(sequence) * 3000,
                    ssrc: protocol::presentation_ssrc(17),
                    payload: vec![0x65, 7, 8, 9],
                    timing: None,
                };
                source_udp
                    .send_to(&packet.to_bytes(), relay_upstream)
                    .await
                    .unwrap();
                if let Ok(Ok((n, _))) =
                    tokio::time::timeout(Duration::from_millis(25), receiver.recv_from(&mut bytes))
                        .await
                {
                    break bytes[..n].to_vec();
                }
                sequence += 1;
            }
        })
        .await
        .unwrap();
        let packet = RtpPacket::parse(&received).unwrap();
        assert_eq!(packet.ssrc, protocol::presentation_ssrc(100));
        assert_eq!(packet.payload, [0x65, 7, 8, 9]);
        control
            .nack(protocol::Nack {
                stream_id: 100,
                sequence_numbers: vec![u32::from(packet.sequence_number)],
            })
            .unwrap();
        let (n, _) = tokio::time::timeout(Duration::from_secs(1), receiver.recv_from(&mut bytes))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&bytes[..n], received);
        worker.finish().unwrap();
        assert!(jobs.entries.lock().unwrap().is_empty());
        assert!(class_updates.borrow().is_none());
        tokio::time::timeout(Duration::from_secs(2), async {
            while control.is_presenting(100) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        drop(control);
        source_state.send_replace(None);
        source_server.abort();
        class_server.abort();
    }
    #[test]
    fn relay_changes_only_session_ssrc_without_repacketizing() {
        let mut packet = vec![
            0x80, 0xe0, 0xff, 0xff, 0, 1, 2, 3, 0, 0, 0, 1, 0x65, 7, 8, 9,
        ];
        let original = packet.clone();
        rewrite_ssrc(&mut packet, 100).unwrap();
        assert_eq!(&packet[..8], &original[..8]);
        assert_eq!(&packet[12..], &original[12..]);
        assert_eq!(
            RtpPacket::parse(&packet).unwrap().ssrc,
            protocol::presentation_ssrc(100)
        );
        assert!(rewrite_ssrc(&mut [0; 3], 100).is_err());
    }
}
