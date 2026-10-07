//! A bounded RTP cache and control dispatcher. Repair never waits for capture/MFT.
use control_transport::TeacherMediaEvent;
use media_transport::MAX_NACK_SEQUENCES;
use std::{
    collections::{HashMap, VecDeque},
    net::{SocketAddrV4, UdpSocket},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{Receiver, SyncSender, TrySendError},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
#[derive(Default)]
struct State {
    packets: VecDeque<(Instant, u16, Arc<Vec<u8>>)>,
    evicted: VecDeque<(Instant, u16, bool)>,
    endpoints: HashMap<u64, SocketAddrV4>,
}
#[derive(Default)]
pub struct Stats {
    pub requested: AtomicU64,
    pub misses: AtomicU64,
    pub expired_misses: AtomicU64,
    pub capacity_misses: AtomicU64,
    pub invalid_misses: AtomicU64,
    pub unknown_misses: AtomicU64,
    pub expired_packets: AtomicU64,
    pub capacity_evictions: AtomicU64,
    pub stale_repairs: AtomicU64,
    pub resent: AtomicU64,
    pub bytes: AtomicU64,
    pub errors: AtomicU64,
    pub would_block: AtomicU64,
    pub deadline_exhausted: AtomicU64,
    pub retry_budget_exhausted: AtomicU64,
    pub reports_dropped: AtomicU64,
    pub router_overflow: AtomicBool,
    pub keyframe_pending: AtomicBool,
}
pub struct RepairService {
    stream_id: u64,
    socket: UdpSocket,
    ttl: Duration,
    state: Mutex<State>,
    pub pacer: Arc<crate::udp_sender::AggregatePacer>,
    pub stats: Stats,
    timings: Mutex<media_core::diagnostics::StageMetrics>,
}
impl RepairService {
    #[cfg(test)]
    pub fn new(socket: UdpSocket, ttl: Duration) -> std::io::Result<Self> {
        Self::for_stream(1, socket, ttl)
    }
    #[cfg(test)]
    pub fn for_stream(stream_id: u64, socket: UdpSocket, ttl: Duration) -> std::io::Result<Self> {
        Self::for_stream_paced(
            stream_id,
            socket,
            ttl,
            Arc::new(crate::udp_sender::AggregatePacer::new(0)),
        )
    }
    pub fn for_stream_paced(
        stream_id: u64,
        socket: UdpSocket,
        ttl: Duration,
        pacer: Arc<crate::udp_sender::AggregatePacer>,
    ) -> std::io::Result<Self> {
        socket.set_nonblocking(true)?;
        Ok(Self {
            stream_id,
            socket,
            ttl,
            pacer,
            state: Mutex::new(State::default()),
            stats: Stats::default(),
            timings: Mutex::new(media_core::diagnostics::StageMetrics::default()),
        })
    }
    pub fn ready(&self, id: u64, endpoint: SocketAddrV4) {
        self.state.lock().unwrap().endpoints.insert(id, endpoint);
    }
    pub fn remove(&self, id: u64) {
        self.state.lock().unwrap().endpoints.remove(&id);
    }
    pub fn cache(&self, sequence: u16, packet: &[u8], now: Instant) {
        let mut state = self.state.lock().unwrap();
        self.prune(&mut state, now);
        if state.packets.len() == 4096 {
            if let Some((_, sequence, _)) = state.packets.pop_front() {
                Self::remember_eviction(&mut state, now, sequence, false);
                self.stats
                    .capacity_evictions
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
        state
            .packets
            .push_back((now, sequence, Arc::new(packet.to_vec())));
    }
    fn prune(&self, state: &mut State, now: Instant) {
        while state
            .packets
            .front()
            .is_some_and(|(at, _, _)| now.saturating_duration_since(*at) > self.ttl)
        {
            if let Some((_, sequence, _)) = state.packets.pop_front() {
                Self::remember_eviction(state, now, sequence, true);
                self.stats.expired_packets.fetch_add(1, Ordering::Relaxed);
            }
        }
        while state
            .evicted
            .front()
            .is_some_and(|(at, _, _)| now.saturating_duration_since(*at) > self.ttl * 2)
        {
            state.evicted.pop_front();
        }
    }
    fn remember_eviction(state: &mut State, now: Instant, sequence: u16, expired: bool) {
        if state.evicted.len() == 4096 {
            state.evicted.pop_front();
        }
        state.evicted.push_back((now, sequence, expired));
    }
    fn respond(&self, client: u64, message: &protocol::Nack) {
        if message.stream_id != self.stream_id {
            return;
        }
        let started = Instant::now();
        let (endpoint, packets) = {
            let mut state = self.state.lock().unwrap();
            self.prune(&mut state, started);
            let Some(&endpoint) = state.endpoints.get(&client) else {
                return;
            };
            let packets: Vec<_> = message
                .sequence_numbers
                .iter()
                .take(MAX_NACK_SEQUENCES)
                .filter_map(|sequence| {
                    self.stats.requested.fetch_add(1, Ordering::Relaxed);
                    let found = u16::try_from(*sequence)
                        .ok()
                        .and_then(|seq| {
                            state
                                .packets
                                .iter()
                                .rev()
                                .find(|(_, number, _)| *number == seq)
                        })
                        .map(|(at, _, bytes)| (Arc::clone(bytes), *at));
                    if found.is_none() {
                        self.stats.misses.fetch_add(1, Ordering::Relaxed);
                        let counter = match u16::try_from(*sequence) {
                            Err(_) => &self.stats.invalid_misses,
                            Ok(seq) => match state
                                .evicted
                                .iter()
                                .rev()
                                .find(|(_, number, _)| *number == seq)
                            {
                                Some((_, _, true)) => &self.stats.expired_misses,
                                Some((_, _, false)) => &self.stats.capacity_misses,
                                None => &self.stats.unknown_misses,
                            },
                        };
                        counter.fetch_add(1, Ordering::Relaxed);
                        if u16::try_from(*sequence).is_ok() {
                            self.stats.keyframe_pending.store(true, Ordering::Release);
                        }
                    }
                    found
                })
                .collect();
            (endpoint, packets)
        };
        // Do not hold the cache lock during socket I/O; retry budget is shared.
        let mut budget = crate::udp_sender::SendBudget::new(started, Duration::from_millis(5));
        for (packet, cached_at) in packets {
            self.timings
                .lock()
                .unwrap()
                .record("nack_packet_age", cached_at.elapsed());
            let deadline = (cached_at + self.ttl).min(started + Duration::from_millis(5));
            if Instant::now() >= deadline {
                self.stats.stale_repairs.fetch_add(1, Ordering::Relaxed);
                self.stats.keyframe_pending.store(true, Ordering::Release);
                continue;
            }
            if budget
                .send_to_paced(&self.socket, &packet, endpoint, &self.pacer, deadline)
                .is_ok()
            {
                self.stats.resent.fetch_add(1, Ordering::Relaxed);
                self.stats
                    .bytes
                    .fetch_add(packet.len() as u64, Ordering::Relaxed);
            } else {
                self.stats.errors.fetch_add(1, Ordering::Relaxed);
                self.stats.keyframe_pending.store(true, Ordering::Release);
            }
        }
        self.stats
            .would_block
            .fetch_add(budget.would_block, Ordering::Relaxed);
        self.stats
            .deadline_exhausted
            .fetch_add(budget.deadline_exhausted, Ordering::Relaxed);
        self.stats
            .retry_budget_exhausted
            .fetch_add(budget.retry_budget_exhausted, Ordering::Relaxed);
        self.timings
            .lock()
            .unwrap()
            .record("nack_service", started.elapsed());
    }
    // Ownership makes queue disconnection explicit when the dispatcher exits.
    #[allow(clippy::needless_pass_by_value)]
    pub fn route(
        &self,
        events: Receiver<TeacherMediaEvent>,
        forward: SyncSender<(Instant, TeacherMediaEvent)>,
        stop: &AtomicBool,
    ) {
        let mut pending = VecDeque::new();
        while !stop.load(Ordering::Acquire) {
            while let Some(item) = pending.pop_front() {
                match forward.try_send(item) {
                    Ok(()) => {}
                    Err(TrySendError::Full(item)) => {
                        pending.push_front(item);
                        break;
                    }
                    Err(TrySendError::Disconnected(_)) => return,
                }
            }
            let event = match events.recv_timeout(Duration::from_millis(1)) {
                Ok(event) => event,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => return,
            };
            match event {
                TeacherMediaEvent::Nack {
                    client_id, message, ..
                } => self.respond(client_id, &message),
                event => {
                    if let TeacherMediaEvent::Disconnected { client_id, .. } = &event {
                        self.remove(*client_id);
                    }
                    if matches!(&event, TeacherMediaEvent::KeyframeRequested { message, .. } if message.stream_id == self.stream_id)
                    {
                        self.stats.keyframe_pending.store(true, Ordering::Release);
                    }
                    let item = (Instant::now(), event);
                    if pending.is_empty() {
                        match forward.try_send(item) {
                            Ok(()) => continue,
                            Err(TrySendError::Disconnected(_)) => return,
                            Err(TrySendError::Full(item)) => pending.push_back(item),
                        }
                    } else {
                        pending.push_back(item);
                    }
                    if matches!(
                        pending.back().map(|(_, event)| event),
                        Some(TeacherMediaEvent::ReceiverReport { .. })
                    ) {
                        pending.pop_back();
                        self.stats.reports_dropped.fetch_add(1, Ordering::Relaxed);
                    }
                    if pending.len() > 256 {
                        self.stats.router_overflow.store(true, Ordering::Release);
                        return;
                    }
                }
            }
        }
    }
    pub fn diagnostics(&self) -> media_core::diagnostics::StageMetrics {
        std::mem::take(&mut *self.timings.lock().unwrap())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repair_runs_without_media_consumer_and_preserves_cached_bytes() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let std::net::SocketAddr::V4(endpoint) = receiver.local_addr().unwrap() else {
            unreachable!()
        };
        let service = RepairService::new(
            UdpSocket::bind("127.0.0.1:0").unwrap(),
            Duration::from_millis(300),
        )
        .unwrap();
        service.ready(1, endpoint);
        service.cache(7, &[1, 2, 3], Instant::now());
        let (send, events) = std::sync::mpsc::channel();
        let (forward, _idle) = std::sync::mpsc::sync_channel(1);
        let stop = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| service.route(events, forward, &stop));
            send.send(TeacherMediaEvent::ReceiverReport {
                client_id: 1,
                report: protocol::ReceiverReport::default(),
            })
            .unwrap();
            send.send(TeacherMediaEvent::KeyframeRequested {
                client_id: 1,
                message: protocol::RequestKeyframe { stream_id: 1 },
            })
            .unwrap();
            send.send(TeacherMediaEvent::Nack {
                client_id: 1,
                endpoint,
                message: protocol::Nack {
                    stream_id: 1,
                    sequence_numbers: vec![7, 8],
                },
            })
            .unwrap();
            let mut bytes = [0; 16];
            let count = receiver.recv(&mut bytes).unwrap();
            assert_eq!(&bytes[..count], &[1, 2, 3]);
            stop.store(true, Ordering::Release);
            worker.join().unwrap();
        });
        assert!(service.stats.keyframe_pending.load(Ordering::Acquire));
        assert_eq!(service.stats.resent.load(Ordering::Relaxed), 1);
        assert_eq!(service.stats.misses.load(Ordering::Relaxed), 1);
        service.remove(1);
        service.respond(
            1,
            &protocol::Nack {
                stream_id: 1,
                sequence_numbers: vec![7],
            },
        );
        assert_eq!(service.stats.resent.load(Ordering::Relaxed), 1);
    }
    #[test]
    fn expired_and_invalid_sequences_do_not_retransmit_a_different_packet() {
        let service = RepairService::new(
            UdpSocket::bind("127.0.0.1:0").unwrap(),
            Duration::from_millis(10),
        )
        .unwrap();
        service.ready(1, "127.0.0.1:12345".parse().unwrap());
        service.cache(
            65535,
            &[3],
            Instant::now()
                .checked_sub(Duration::from_millis(11))
                .unwrap(),
        );
        service.respond(
            1,
            &protocol::Nack {
                stream_id: 1,
                sequence_numbers: vec![65535, 65536],
            },
        );
        assert_eq!(service.stats.misses.load(Ordering::Relaxed), 2);
        assert_eq!(service.stats.expired_misses.load(Ordering::Relaxed), 1);
        assert_eq!(service.stats.invalid_misses.load(Ordering::Relaxed), 1);
        assert_eq!(service.stats.resent.load(Ordering::Relaxed), 0);
    }
}
