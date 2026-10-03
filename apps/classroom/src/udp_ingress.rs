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
    // xorshift; deterministic per process is fine for an emulator.
    let mut seed = 0x9e37_79b9_u32 ^ std::process::id();
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

#[cfg(test)]
mod tests {
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
