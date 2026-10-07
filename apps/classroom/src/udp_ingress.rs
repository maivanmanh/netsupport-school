use media_core::diagnostics::StageMetrics;
use media_transport::RtpPacket;
use std::{
    collections::VecDeque,
    net::UdpSocket,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};

pub struct ReceivedPacket {
    pub packet: RtpPacket,
    pub arrived: Instant,
}
#[derive(Default)]
pub struct IngressState {
    queue: VecDeque<ReceivedPacket>,
    pub packets: u64,
    pub bytes: u64,
    pub dropped: u64,
    pub malformed: u64,
    pub peer_rejected: u64,
    pub high_water: usize,
    pub socket_errors: u64,
    last_arrived: Option<Instant>,
    pub diagnostics: StageMetrics,
    /// O1 pointer channel: latest state/image, never queued with video.
    pub pointer: media_transport::pointer::PointerReceiver,
    /// Test only (`CLASSROOM_TEST_DROP_PERCENT`): video packets discarded on
    /// arrival to emulate network loss without OS firewall changes.
    pub test_dropped: u64,
    pub test_parameter_dropped: u64,
    pub test_repair_dropped: u64,
    pub test_repair_accepted: u64,
}

#[derive(Debug, PartialEq)]
enum RepairInjectionEvent {
    OriginalDropped,
    FirstRepairDropped,
    Accepted {
        original_wait: Duration,
        retry_wait: Duration,
    },
}

/// Disabled diagnostic: two drops of one selected packet, never a video queue.
#[derive(Default)]
struct RepairInjection {
    enabled: bool,
    selected: Option<(u32, u32, u16)>,
    copies: u8,
    original_at: Option<Instant>,
    repair_at: Option<Instant>,
}
impl RepairInjection {
    fn observe(&mut self, packet: &RtpPacket, now: Instant) -> Option<RepairInjectionEvent> {
        if !self.enabled || self.copies == 3 {
            return None;
        }
        let identity = (packet.ssrc, packet.timestamp, packet.sequence_number);
        if self.selected.is_none() {
            let p = &packet.payload;
            // H.264 FU-A IDR middle: not S, not E, not the AU marker. Losing
            // it creates an observable sequence gap after the retained prefix.
            if packet.payload_type != media_core::VideoCodec::H264.rtp_payload_type()
                || packet.marker
                || p.len() < 3
                || p[0] & 0x1f != 28
                || p[1] & 0xc0 != 0
                || p[1] & 0x1f != 5
            {
                return None;
            }
            self.selected = Some(identity);
            self.original_at = Some(now);
            self.copies = 1;
            return Some(RepairInjectionEvent::OriginalDropped);
        }
        if self.selected != Some(identity) {
            return None;
        }
        self.copies += 1;
        if self.copies == 2 {
            self.repair_at = Some(now);
            Some(RepairInjectionEvent::FirstRepairDropped)
        } else {
            Some(RepairInjectionEvent::Accepted {
                original_wait: now.saturating_duration_since(self.original_at?),
                retry_wait: now.saturating_duration_since(self.repair_at?),
            })
        }
    }
}

/// Test-only random loss, 0 (default) disables. Bounded to 50 %.
fn test_drop_percent() -> u32 {
    std::env::var("CLASSROOM_TEST_DROP_PERCENT")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0)
        .min(50)
}
const CAPACITY: usize = 2048;
const MAX_AGE: Duration = Duration::from_millis(80);
impl IngressState {
    fn push(&mut self, packet: ReceivedPacket) {
        self.expire(packet.arrived);
        if self.queue.len() == CAPACITY {
            self.queue.pop_front();
            self.dropped += 1;
        }
        if let Some(last) = self.last_arrived {
            self.diagnostics.record(
                "udp_wire_arrival_gap",
                packet.arrived.saturating_duration_since(last),
            );
        }
        self.last_arrived = Some(packet.arrived);
        self.queue.push_back(packet);
        self.high_water = self.high_water.max(self.queue.len());
    }
    fn expire(&mut self, now: Instant) {
        while self
            .queue
            .front()
            .is_some_and(|item| now.saturating_duration_since(item.arrived) > MAX_AGE)
        {
            self.queue.pop_front();
            self.dropped += 1;
        }
    }
    pub fn pop(&mut self, now: Instant) -> Option<ReceivedPacket> {
        self.expire(now);
        let packet = self.queue.pop_front()?;
        self.diagnostics.record(
            "udp_ingress_wait",
            now.saturating_duration_since(packet.arrived),
        );
        Some(packet)
    }
    pub fn depth(&self) -> usize {
        self.queue.len()
    }
    pub fn oldest_age(&self) -> Duration {
        self.queue
            .front()
            .map_or(Duration::ZERO, |item| item.arrived.elapsed())
    }
}

pub fn receive(
    socket: &UdpSocket,
    teacher: std::net::SocketAddrV4,
    shared: &Mutex<IngressState>,
    stop: &AtomicBool,
) {
    let mut buffer = [0; 1500];
    let drop_percent = test_drop_percent();
    if drop_percent > 0 {
        tracing::warn!(
            drop_percent,
            "TEST ONLY: emulating random video packet loss"
        );
    }
    let parameter_kind = match std::env::var("CLASSROOM_TEST_DROP_FIRST_PARAMETER_SET").as_deref() {
        Ok("sps") => Some(7),
        Ok("pps") => Some(8),
        _ => None,
    };
    let mut parameter_dropped = false;
    let mut repair_injection = RepairInjection {
        enabled: std::env::var("CLASSROOM_TEST_DROP_FIRST_IDR_AND_REPAIR")
            .is_ok_and(|value| value == "1"),
        ..RepairInjection::default()
    };
    // xorshift; deterministic per process is fine for an emulator.
    let mut seed = std::env::var("CLASSROOM_TEST_DROP_SEED")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|&seed| seed != 0)
        .unwrap_or(0x9e37_79b9_u32 ^ std::process::id());
    while !stop.load(Ordering::Acquire) {
        match socket.recv_from(&mut buffer) {
            Ok((bytes, peer)) => {
                let arrived = Instant::now();
                let mut state = shared.lock().unwrap();
                if peer != std::net::SocketAddr::V4(teacher) {
                    state.peer_rejected += 1;
                    continue;
                }
                state.packets += 1;
                state.bytes += bytes as u64;
                match RtpPacket::parse(&buffer[..bytes]) {
                    Ok(packet)
                        if packet.payload_type
                            == media_transport::pointer::POINTER_PAYLOAD_TYPE =>
                    {
                        state.pointer.push(&packet);
                    }
                    Ok(packet) => {
                        if let Some(event) = repair_injection.observe(&packet, arrived) {
                            let (action, original_wait, retry_wait) = match event {
                                RepairInjectionEvent::OriginalDropped => {
                                    ("drop_original", Duration::ZERO, Duration::ZERO)
                                }
                                RepairInjectionEvent::FirstRepairDropped => {
                                    ("drop_first_repair", Duration::ZERO, Duration::ZERO)
                                }
                                RepairInjectionEvent::Accepted {
                                    original_wait,
                                    retry_wait,
                                } => ("accept_second_repair", original_wait, retry_wait),
                            };
                            tracing::info!(
                                action,
                                copy = repair_injection.copies,
                                sequence = packet.sequence_number,
                                rtp_timestamp = packet.timestamp,
                                ssrc = packet.ssrc,
                                original_drop_to_accept_ms = original_wait.as_secs_f64() * 1000.0,
                                repair_drop_to_accept_ms = retry_wait.as_secs_f64() * 1000.0,
                                "TEST ONLY: controlled IDR packet copy"
                            );
                            if repair_injection.copies < 3 {
                                state.test_dropped += 1;
                                state.test_repair_dropped += 1;
                                continue;
                            }
                            state.test_repair_accepted += 1;
                        }
                        if !parameter_dropped
                            && parameter_kind
                                .is_some_and(|kind| first_parameter_packet(&packet, kind))
                        {
                            parameter_dropped = true;
                            state.test_dropped += 1;
                            state.test_parameter_dropped += 1;
                            tracing::info!(
                                sequence = packet.sequence_number,
                                "TEST ONLY: dropped first parameter-set RTP packet"
                            );
                            continue;
                        }
                        if drop_percent > 0 {
                            seed ^= seed << 13;
                            seed ^= seed >> 17;
                            seed ^= seed << 5;
                            if seed % 100 < drop_percent {
                                state.test_dropped += 1;
                                continue;
                            }
                        }
                        state.push(ReceivedPacket { packet, arrived });
                    }
                    Err(_) => state.malformed += 1,
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(error) => {
                shared.lock().unwrap().socket_errors += 1;
                tracing::warn!(%error, "UDP ingest socket error");
                break;
            }
        }
    }
}

/// Test hook matches only H.264 SPS/PPS start packets, never continuation or pointer.
fn first_parameter_packet(packet: &RtpPacket, kind: u8) -> bool {
    packet.payload_type == media_core::VideoCodec::H264.rtp_payload_type()
        && (packet.payload.first().is_some_and(|b| b & 0x1f == kind)
            || (packet.payload.len() >= 3
                && packet.payload[0] & 0x1f == 28
                && packet.payload[1] & 0x80 != 0
                && packet.payload[1] & 0x1f == kind))
}

#[cfg(test)]
mod tests {
    fn middle_idr() -> RtpPacket {
        RtpPacket {
            timing: None,
            marker: false,
            payload_type: 96,
            sequence_number: 9,
            timestamp: 90,
            ssrc: 1,
            payload: vec![0x7c, 5, 42],
        }
    }

    #[test]
    fn controlled_repair_drops_exactly_two_copies_and_never_rearms() {
        let t = Instant::now();
        let p = middle_idr();
        let mut hook = RepairInjection {
            enabled: true,
            ..RepairInjection::default()
        };
        assert_eq!(
            hook.observe(&p, t),
            Some(RepairInjectionEvent::OriginalDropped)
        );
        let mut other = p.clone();
        other.timestamp += 1;
        assert_eq!(hook.observe(&other, t), None);
        other = p.clone();
        other.ssrc += 1;
        assert_eq!(hook.observe(&other, t), None);
        other = p.clone();
        other.sequence_number += 1;
        assert_eq!(hook.observe(&other, t), None);
        assert_eq!(
            hook.observe(&p, t + Duration::from_millis(5)),
            Some(RepairInjectionEvent::FirstRepairDropped)
        );
        assert_eq!(
            hook.observe(&p, t + Duration::from_millis(30)),
            Some(RepairInjectionEvent::Accepted {
                original_wait: Duration::from_millis(30),
                retry_wait: Duration::from_millis(25)
            })
        );
        assert_eq!(hook.observe(&p, t + Duration::from_millis(40)), None);
        assert_eq!(hook.observe(&other, t + Duration::from_millis(40)), None);
        assert_eq!(hook.copies, 3);
    }

    #[test]
    fn controlled_repair_is_disabled_by_default_and_selects_only_middle_idr() {
        let t = Instant::now();
        let p = middle_idr();
        assert_eq!(RepairInjection::default().observe(&p, t), None);
        let mut hook = RepairInjection {
            enabled: true,
            ..RepairInjection::default()
        };
        for flags in [0x85, 0x45, 1] {
            let mut other = p.clone();
            other.payload[1] = flags;
            assert_eq!(hook.observe(&other, t), None);
        }
        let mut other = p.clone();
        other.marker = true;
        assert_eq!(hook.observe(&other, t), None);
        other = p.clone();
        other.payload_type = media_transport::pointer::POINTER_PAYLOAD_TYPE;
        assert_eq!(hook.observe(&other, t), None);
        other = p.clone();
        other.payload = vec![0x65, 42];
        assert_eq!(hook.observe(&other, t), None);
        assert_eq!(hook.selected, None);
        assert_eq!(
            hook.observe(&p, t),
            Some(RepairInjectionEvent::OriginalDropped)
        );
    }

    #[test]
    fn parameter_injection_matches_only_first_nal_or_fu_start() {
        let mut p = RtpPacket {
            timing: None,
            marker: false,
            payload_type: 96,
            sequence_number: 0,
            timestamp: 0,
            ssrc: 1,
            payload: vec![0x67, 1],
        };
        assert!(first_parameter_packet(&p, 7));
        assert!(!first_parameter_packet(&p, 8));
        p.payload = vec![0x7c, 0x87, 1];
        assert!(first_parameter_packet(&p, 7));
        p.payload[1] = 7;
        assert!(!first_parameter_packet(&p, 7));
        p.payload_type = media_transport::pointer::POINTER_PAYLOAD_TYPE;
        p.payload = vec![0x67];
        assert!(!first_parameter_packet(&p, 7));
    }

    #[test]
    fn receives_and_timestamps_while_consumer_is_not_running() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver
            .set_read_timeout(Some(Duration::from_millis(5)))
            .unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        let std::net::SocketAddr::V4(teacher) = sender.local_addr().unwrap() else {
            unreachable!()
        };
        let shared = Mutex::new(IngressState::default());
        let stop = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| receive(&receiver, teacher, &shared, &stop));
            for seq in 0..20 {
                let packet = RtpPacket {
                    timing: None,
                    marker: true,
                    payload_type: 96,
                    sequence_number: seq,
                    timestamp: 1,
                    ssrc: 1,
                    payload: vec![0x65],
                };
                sender
                    .send_to(&packet.to_bytes(), receiver.local_addr().unwrap())
                    .unwrap();
            }
            let deadline = Instant::now() + Duration::from_secs(1);
            while shared.lock().unwrap().packets < 20 && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(1));
            }
            stop.store(true, Ordering::Release);
            worker.join().unwrap();
        });
        let mut state = shared.lock().unwrap();
        assert_eq!(state.packets, 20);
        assert_eq!(state.depth(), 20);
        assert_eq!(state.pop(Instant::now()).unwrap().packet.sequence_number, 0);
        assert_eq!(
            std::mem::take(&mut state.diagnostics)
                .summaries()
                .iter()
                .find(|s| s.stage == "udp_wire_arrival_gap")
                .unwrap()
                .samples,
            19
        );
    }

    use super::*;
    #[test]
    fn bounds_packet_backlog_by_capacity_and_age() {
        let start = Instant::now();
        let mut state = IngressState::default();
        for seq in 0..=CAPACITY {
            let packet = RtpPacket {
                timing: None,
                marker: false,
                payload_type: 96,
                sequence_number: u16::try_from(seq).unwrap(),
                timestamp: 0,
                ssrc: 1,
                payload: vec![1],
            };
            state.push(ReceivedPacket {
                packet,
                arrived: start,
            });
        }
        assert_eq!(state.depth(), CAPACITY);
        assert_eq!(state.dropped, 1);
        assert_eq!(state.pop(start).unwrap().packet.sequence_number, 1);
        assert!(state.pop(start + Duration::from_millis(81)).is_none());
        assert_eq!(state.depth(), 0);
    }
}
