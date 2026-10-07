//! Teacher RTP fan-out off the encode thread (docs/17 §9).
//!
//! The encode thread packetizes once and hands each AU, shared behind an
//! `Arc`, to a few worker threads. Students are sharded by client id. Each
//! worker has a bounded channel. Admission waits for the previous AU before
//! taking a fresh capture, so an encoded queue cannot grow behind an IDR.
//! Failed recipient chains are skipped until the next successfully sent IDR.
//!
//! With `connected` set, each Student gets its own UDP socket bound to the
//! shared Teacher media address (reuse-port) and `connect()`ed to the Student.
//! The source address the Student validates and the route-probe firewall flow
//! are unchanged; the kernel skips a route lookup per send.

use std::{
    collections::{HashMap, HashSet},
    io,
    net::{SocketAddrV4, UdpSocket},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use media_core::diagnostics::StageMetrics;
use socket2::{Domain, Protocol, Socket, Type};
use tracing::{info, warn};

use crate::udp_sender::{AggregatePacer, SendBudget};

#[derive(Clone, Copy, Debug)]
pub struct FanoutConfig {
    pub workers: usize,
    pub connected: bool,
    pub pacing_mbps: u32,
    pub max_au_age: Duration,
    pub max_keyframe_age: Duration,
    pub send_buffer_bytes: usize,
}

/// One packetized AU and the Students that should receive it.
pub struct FanoutJob {
    pub is_keyframe: bool,
    pub packets: Vec<Vec<u8>>,
    pub targets: Vec<(u64, SocketAddrV4)>,
}

#[derive(Default)]
pub struct FanoutStats {
    pub sent_packets: AtomicU64,
    pub sent_bytes: AtomicU64,
    pub send_errors: AtomicU64,
    pub abandoned_packets: AtomicU64,
    pub would_block: AtomicU64,
    pub resource_backpressure: AtomicU64,
    pub retry_recovered: AtomicU64,
    pub retry_wait_us: AtomicU64,
    pub deadline_exhausted: AtomicU64,
    pub retry_budget_exhausted: AtomicU64,
    pub pacing_wait_us: AtomicU64,
    pub au_deadline_drops: AtomicU64,
    pub dependency_skipped_packets: AtomicU64,
    pub recovery_requested: AtomicBool,
    /// Time the encode thread spent waiting for a busy worker.
    pub handoff_wait_us: AtomicU64,
    pub connected_sockets: AtomicU64,
    pub connect_failures: AtomicU64,
}

#[derive(Default)]
struct Completion {
    pending: Mutex<usize>,
    changed: Condvar,
}
impl Completion {
    fn finish(&self) {
        let mut pending = self.pending.lock().unwrap();
        *pending = pending.saturating_sub(1);
        self.changed.notify_all();
    }
}
struct CompletionGuard(Arc<Completion>);
impl Drop for CompletionGuard {
    fn drop(&mut self) {
        self.0.finish();
    }
}

pub struct Fanout {
    workers: Vec<(SyncSender<Arc<FanoutJob>>, JoinHandle<()>)>,
    completion: Arc<Completion>,
    pub stats: Arc<FanoutStats>,
    stages: Arc<Mutex<StageMetrics>>,
}

impl Fanout {
    #[cfg(test)]
    pub fn start(socket: &UdpSocket, config: FanoutConfig) -> io::Result<Self> {
        Self::start_paced(
            socket,
            config,
            &Arc::new(AggregatePacer::new(config.pacing_mbps)),
        )
    }

    pub fn start_paced(
        socket: &UdpSocket,
        config: FanoutConfig,
        pacer: &Arc<AggregatePacer>,
    ) -> io::Result<Self> {
        let local = match socket.local_addr()? {
            std::net::SocketAddr::V4(local) => local,
            std::net::SocketAddr::V6(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "IPv6 media socket",
                ))
            }
        };
        let count = config.workers.max(1);
        let stats = Arc::new(FanoutStats::default());
        let stages = Arc::new(Mutex::new(StageMetrics::default()));
        let completion = Arc::new(Completion::default());
        let mut workers = Vec::with_capacity(count);
        for index in 0..count {
            let (sender, receiver) = mpsc::sync_channel(1);
            let worker = Worker {
                index: index as u64,
                count: count as u64,
                local,
                shared: socket.try_clone()?,
                config,
                pacer: Arc::clone(pacer),
                completion: Arc::clone(&completion),
                needs_keyframe: HashSet::new(),
                stats: Arc::clone(&stats),
                stages: Arc::clone(&stages),
                sockets: HashMap::new(),
                warned_at: HashMap::new(),
            };
            let handle = thread::Builder::new()
                .name(format!("rtp-fanout-{index}"))
                .spawn(move || worker.run(&receiver))?;
            workers.push((sender, handle));
        }
        info!(
            workers = count,
            connected = config.connected,
            aggregate_pacing_mbps = config.pacing_mbps,
            max_au_age_ms = config.max_au_age.as_millis(),
            max_keyframe_age_ms = config.max_keyframe_age.as_millis(),
            local = %local,
            "teacher RTP fan-out workers ready"
        );
        Ok(Self {
            workers,
            completion,
            stats,
            stages,
        })
    }

    /// Hands one AU to every worker; waits only while a worker still holds a
    /// queued AU (bounded back-pressure, never an unbounded queue).
    pub fn wait_idle(&self, cancelled: &AtomicBool) -> bool {
        let started = Instant::now();
        let mut pending = self.completion.pending.lock().unwrap();
        while *pending != 0 {
            if cancelled.load(Ordering::Acquire) {
                return false;
            }
            pending = self
                .completion
                .changed
                .wait_timeout(pending, Duration::from_millis(5))
                .unwrap()
                .0;
        }
        self.stats
            .handoff_wait_us
            .fetch_add(micros(started.elapsed()), Ordering::Relaxed);
        true
    }

    pub fn send(&self, job: FanoutJob) {
        self.wait_idle(&AtomicBool::new(false));
        *self.completion.pending.lock().unwrap() = self.workers.len();
        let job = Arc::new(job);
        let started = Instant::now();
        for (sender, _) in &self.workers {
            // A worker only disappears by panicking; the next session restarts it.
            if sender.send(Arc::clone(&job)).is_err() {
                self.completion.finish();
            }
        }
        self.stats
            .handoff_wait_us
            .fetch_add(micros(started.elapsed()), Ordering::Relaxed);
    }

    pub fn take_stages(&self) -> StageMetrics {
        std::mem::take(&mut *self.stages.lock().unwrap())
    }
}

impl Drop for Fanout {
    fn drop(&mut self) {
        for (sender, handle) in self.workers.drain(..) {
            drop(sender);
            if handle.join().is_err() {
                warn!("teacher RTP fan-out worker panicked");
            }
        }
    }
}

struct Worker {
    index: u64,
    count: u64,
    local: SocketAddrV4,
    shared: UdpSocket,
    config: FanoutConfig,
    pacer: Arc<AggregatePacer>,
    completion: Arc<Completion>,
    needs_keyframe: HashSet<u64>,
    stats: Arc<FanoutStats>,
    stages: Arc<Mutex<StageMetrics>>,
    /// client id -> (endpoint, connected socket when available)
    sockets: HashMap<u64, (SocketAddrV4, Option<UdpSocket>)>,
    warned_at: HashMap<u64, Instant>,
}

impl Worker {
    fn run(mut self, jobs: &Receiver<Arc<FanoutJob>>) {
        while let Ok(job) = jobs.recv() {
            let _completion = CompletionGuard(Arc::clone(&self.completion));
            let started = Instant::now();
            self.send(&job);
            self.stages
                .lock()
                .unwrap()
                .record("udp_fanout_worker", started.elapsed());
        }
        let connected = self.sockets.values().filter(|(_, s)| s.is_some()).count();
        self.stats
            .connected_sockets
            .fetch_sub(connected as u64, Ordering::Relaxed);
    }

    fn refresh(&mut self, mine: &[(u64, SocketAddrV4)]) {
        let before = self.connected_count();
        self.needs_keyframe
            .retain(|id| mine.iter().any(|(m, _)| m == id));
        self.sockets
            .retain(|id, (endpoint, _)| mine.iter().any(|(m, e)| m == id && e == endpoint));
        for &(id, endpoint) in mine {
            if self.sockets.contains_key(&id) {
                continue;
            }
            let socket = if self.config.connected {
                match connected_socket(self.local, endpoint, self.config.send_buffer_bytes) {
                    Ok(socket) => Some(socket),
                    Err(error) => {
                        self.stats.connect_failures.fetch_add(1, Ordering::Relaxed);
                        warn!(client_id = id, %endpoint, %error, "connected fan-out socket unavailable; using shared socket");
                        None
                    }
                }
            } else {
                None
            };
            self.sockets.insert(id, (endpoint, socket));
        }
        let after = self.connected_count();
        if after >= before {
            self.stats
                .connected_sockets
                .fetch_add((after - before) as u64, Ordering::Relaxed);
        } else {
            self.stats
                .connected_sockets
                .fetch_sub((before - after) as u64, Ordering::Relaxed);
        }
    }

    fn connected_count(&self) -> usize {
        self.sockets.values().filter(|(_, s)| s.is_some()).count()
    }

    #[allow(clippy::too_many_lines)] // One bounded AU transaction updates its endpoint reference state and counters.
    fn send(&mut self, job: &FanoutJob) {
        let mine: Vec<(u64, SocketAddrV4)> = job
            .targets
            .iter()
            .copied()
            .filter(|(id, _)| id % self.count == self.index)
            .collect();
        self.refresh(&mine);
        if mine.is_empty() {
            return;
        }
        let started = Instant::now();
        let deadline = started
            + if job.is_keyframe {
                self.config.max_keyframe_age
            } else {
                self.config.max_au_age
            };
        let age_budget = deadline.duration_since(started);
        if !self.pacer.fits(
            job.packets.iter().map(Vec::len).sum(),
            job.packets.len(),
            job.targets.len(),
            age_budget,
        ) {
            self.stats
                .abandoned_packets
                .fetch_add((job.packets.len() * mine.len()) as u64, Ordering::Relaxed);
            self.stats
                .au_deadline_drops
                .fetch_add(mine.len() as u64, Ordering::Relaxed);
            self.stats.recovery_requested.store(true, Ordering::Release);
            self.needs_keyframe.extend(mine.iter().map(|(id, _)| *id));
            return;
        }
        let mut budget = SendBudget::new(started, Duration::from_millis(10));
        let mut failed = HashSet::new();
        let mut dependency_skipped = HashSet::new();
        if !job.is_keyframe {
            for (id, _) in &mine {
                if self.needs_keyframe.contains(id) {
                    failed.insert(*id);
                    dependency_skipped.insert(*id);
                    self.stats
                        .dependency_skipped_packets
                        .fetch_add(job.packets.len() as u64, Ordering::Relaxed);
                }
            }
        }
        let (mut sent, mut bytes, mut errors, mut abandoned) = (0_u64, 0_u64, 0_u64, 0_u64);
        for packet in &job.packets {
            for (id, endpoint) in &mine {
                if failed.contains(id) {
                    if !dependency_skipped.contains(id) {
                        abandoned += 1;
                    }
                    continue;
                }
                let result = match self.sockets.get(id).and_then(|(_, s)| s.as_ref()) {
                    Some(socket) => {
                        budget.send_connected_paced(socket, packet, &self.pacer, deadline)
                    }
                    None => {
                        budget.send_to_paced(&self.shared, packet, *endpoint, &self.pacer, deadline)
                    }
                };
                match result {
                    Ok(_) => {
                        sent += 1;
                        bytes += packet.len() as u64;
                    }
                    Err(error) => {
                        errors += 1;
                        abandoned += 1;
                        failed.insert(*id);
                        self.needs_keyframe.insert(*id);
                        self.stats.recovery_requested.store(true, Ordering::Release);
                        if error.kind() == io::ErrorKind::TimedOut {
                            self.stats.au_deadline_drops.fetch_add(1, Ordering::Relaxed);
                        }
                        let now = Instant::now();
                        let warn_now = self.warned_at.get(id).is_none_or(|previous| {
                            now.saturating_duration_since(*previous) >= Duration::from_secs(5)
                        });
                        if warn_now {
                            self.warned_at.insert(*id, now);
                            warn!(client_id = id, %endpoint, %error, "teacher could not send RTP to Student; presentation continues");
                        }
                    }
                }
            }
        }
        if job.is_keyframe {
            for (id, _) in &mine {
                if !failed.contains(id) {
                    self.needs_keyframe.remove(id);
                }
            }
        }
        let mut stages = self.stages.lock().unwrap();
        stages.record("udp_au_completion", started.elapsed());
        if job.is_keyframe {
            stages.record("udp_keyframe_completion", started.elapsed());
        }
        drop(stages);
        let stats = &self.stats;
        stats.sent_packets.fetch_add(sent, Ordering::Relaxed);
        stats.sent_bytes.fetch_add(bytes, Ordering::Relaxed);
        stats.send_errors.fetch_add(errors, Ordering::Relaxed);
        stats
            .abandoned_packets
            .fetch_add(abandoned, Ordering::Relaxed);
        stats
            .would_block
            .fetch_add(budget.would_block, Ordering::Relaxed);
        stats
            .resource_backpressure
            .fetch_add(budget.resource_backpressure, Ordering::Relaxed);
        stats
            .retry_recovered
            .fetch_add(budget.recovered, Ordering::Relaxed);
        stats
            .retry_wait_us
            .fetch_add(micros(budget.retry_wait), Ordering::Relaxed);
        stats
            .deadline_exhausted
            .fetch_add(budget.deadline_exhausted, Ordering::Relaxed);
        stats
            .retry_budget_exhausted
            .fetch_add(budget.retry_budget_exhausted, Ordering::Relaxed);
        stats
            .pacing_wait_us
            .fetch_add(micros(budget.pacing_wait), Ordering::Relaxed);
    }
}

fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

/// Sets the options that let the per-Student sockets share the Teacher media
/// address: `SO_REUSEPORT` on BSD/macOS, `SO_REUSEADDR` on Windows (measured:
/// without it on the first socket, Windows refuses the later binds).
pub fn allow_shared_port(socket: &Socket) -> io::Result<()> {
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    #[cfg(not(unix))]
    socket.set_reuse_address(true)?;
    Ok(())
}

fn connected_socket(
    local: SocketAddrV4,
    remote: SocketAddrV4,
    send_buffer_bytes: usize,
) -> io::Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    allow_shared_port(&socket)?;
    socket.set_send_buffer_size(send_buffer_bytes)?;
    socket.bind(&local.into())?;
    socket.connect(&remote.into())?;
    socket.set_nonblocking(true)?;
    Ok(socket.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn shared_socket() -> UdpSocket {
        shared_socket_on(Ipv4Addr::LOCALHOST)
    }

    fn shared_socket_on(ip: Ipv4Addr) -> UdpSocket {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).unwrap();
        allow_shared_port(&socket).unwrap();
        socket.bind(&SocketAddrV4::new(ip, 0).into()).unwrap();
        socket.set_nonblocking(true).unwrap();
        socket.into()
    }

    fn receiver() -> (UdpSocket, SocketAddrV4) {
        let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let std::net::SocketAddr::V4(address) = socket.local_addr().unwrap() else {
            unreachable!()
        };
        (socket, address)
    }

    fn config(workers: usize, connected: bool) -> FanoutConfig {
        FanoutConfig {
            workers,
            connected,
            pacing_mbps: 0,
            max_au_age: Duration::from_millis(120),
            max_keyframe_age: Duration::from_millis(300),
            send_buffer_bytes: 256 * 1024,
        }
    }

    #[test]
    fn paced_multiworker_fanout_preserves_packet_order_and_source() {
        let shared = shared_socket();
        let source = shared.local_addr().unwrap();
        let students: Vec<_> = (0..5).map(|_| receiver()).collect();
        let mut settings = config(4, true);
        settings.pacing_mbps = 400;
        let fanout = Fanout::start(&shared, settings).unwrap();
        fanout.send(FanoutJob {
            is_keyframe: true,
            packets: (0..20).map(|n| vec![n; 1200]).collect(),
            targets: students
                .iter()
                .enumerate()
                .map(|(i, (_, address))| (i as u64 + 1, *address))
                .collect(),
        });
        for (socket, _) in &students {
            let mut buffer = [0_u8; 1200];
            for n in 0..20 {
                let (bytes, peer) = socket.recv_from(&mut buffer).unwrap();
                assert_eq!(bytes, 1200);
                assert_eq!(peer, source);
                assert!(buffer.iter().all(|b| *b == n));
            }
        }
        let stats = Arc::clone(&fanout.stats);
        drop(fanout);
        assert_eq!(stats.sent_packets.load(Ordering::Relaxed), 100);
        assert_eq!(stats.send_errors.load(Ordering::Relaxed), 0);
        let mut settings = config(1, true);
        settings.pacing_mbps = 1;
        settings.max_au_age = Duration::from_millis(1);
        let fanout = Fanout::start(&shared, settings).unwrap();
        let targets = vec![(1, students[0].1)];
        fanout.send(FanoutJob {
            is_keyframe: false,
            packets: vec![vec![1; 1200]; 20],
            targets: targets.clone(),
        });
        fanout.wait_idle(&AtomicBool::new(false));
        assert_eq!(fanout.stats.au_deadline_drops.load(Ordering::Relaxed), 1);
        fanout.send(FanoutJob {
            is_keyframe: false,
            packets: vec![vec![2; 16]],
            targets: targets.clone(),
        });
        fanout.wait_idle(&AtomicBool::new(false));
        assert_eq!(
            fanout
                .stats
                .dependency_skipped_packets
                .load(Ordering::Relaxed),
            1
        );
        fanout.send(FanoutJob {
            is_keyframe: true,
            packets: vec![vec![3; 16]],
            targets,
        });
        let mut buffer = [0; 32];
        let (count, _) = students[0].0.recv_from(&mut buffer).unwrap();
        assert_eq!(&buffer[..count], &[3; 16]);
        drop(fanout);
    }

    #[test]
    fn every_student_gets_every_packet_from_the_shared_source_address() {
        for connected in [false, true] {
            let shared = shared_socket();
            let source = shared.local_addr().unwrap();
            let students: Vec<_> = (0..5).map(|_| receiver()).collect();
            let fanout = Fanout::start(&shared, config(2, connected)).unwrap();
            fanout.send(FanoutJob {
                is_keyframe: true,
                packets: vec![vec![1; 100], vec![2; 200], vec![3; 300]],
                targets: students
                    .iter()
                    .enumerate()
                    .map(|(i, (_, address))| (i as u64 + 1, *address))
                    .collect(),
            });
            for (socket, _) in &students {
                let mut buffer = [0_u8; 512];
                for expected in [100, 200, 300] {
                    let (bytes, peer) = socket.recv_from(&mut buffer).unwrap();
                    assert_eq!(bytes, expected);
                    // Students validate this exact source address.
                    assert_eq!(peer, source);
                }
            }
            let stats = Arc::clone(&fanout.stats);
            drop(fanout);
            assert_eq!(stats.sent_packets.load(Ordering::Relaxed), 15);
            // The gauge returns to zero when workers stop.
            assert_eq!(stats.connected_sockets.load(Ordering::Relaxed), 0);
            assert_eq!(stats.connect_failures.load(Ordering::Relaxed), 0);
        }
    }

    #[test]
    fn counters_and_connected_socket_gauge_follow_targets() {
        let shared = shared_socket();
        let (student, address) = receiver();
        let fanout = Fanout::start(&shared, config(1, true)).unwrap();
        fanout.send(FanoutJob {
            is_keyframe: true,
            packets: vec![vec![0; 64]; 4],
            targets: vec![(7, address)],
        });
        let mut buffer = [0_u8; 128];
        for _ in 0..4 {
            student.recv_from(&mut buffer).unwrap();
        }
        // The next job without the Student closes its connected socket.
        fanout.send(FanoutJob {
            is_keyframe: true,
            packets: vec![vec![0; 64]],
            targets: Vec::new(),
        });
        let stats = Arc::clone(&fanout.stats);
        drop(fanout);
        assert_eq!(stats.sent_packets.load(Ordering::Relaxed), 4);
        assert_eq!(stats.sent_bytes.load(Ordering::Relaxed), 256);
        assert_eq!(stats.send_errors.load(Ordering::Relaxed), 0);
        assert_eq!(stats.connect_failures.load(Ordering::Relaxed), 0);
        assert_eq!(stats.connected_sockets.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn new_student_probes_still_reach_the_shared_socket() {
        let shared = shared_socket();
        shared
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        shared.set_nonblocking(false).unwrap();
        let (_first, first_address) = receiver();
        let fanout = Fanout::start(&shared, config(1, true)).unwrap();
        fanout.send(FanoutJob {
            is_keyframe: true,
            packets: vec![vec![0; 16]],
            targets: vec![(1, first_address)],
        });
        // Let the worker create its connected socket before the probe.
        thread::sleep(Duration::from_millis(50));
        assert_eq!(fanout.stats.connected_sockets.load(Ordering::Relaxed), 1);
        let (newcomer, newcomer_address) = receiver();
        newcomer
            .send_to(b"probe", shared.local_addr().unwrap())
            .unwrap();
        let mut buffer = [0_u8; 16];
        let (bytes, peer) = shared.recv_from(&mut buffer).unwrap();
        assert_eq!(&buffer[..bytes], b"probe");
        assert_eq!(peer, std::net::SocketAddr::V4(newcomer_address));
    }

    /// Manual measurement: `FANOUT_BENCH_TARGET=<ip> <test-exe> fanout_bench
    /// --ignored --nocapture`. 41 Students x 85 packets (one 1080p IDR).
    #[test]
    #[ignore = "manual send-path measurement"]
    fn fanout_bench_connected_vs_unconnected() {
        let ip: Ipv4Addr = std::env::var("FANOUT_BENCH_TARGET")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(Ipv4Addr::LOCALHOST);
        let targets: Vec<_> = (0..41)
            .map(|i| (i as u64 + 1, SocketAddrV4::new(ip, 49_000 + 2 * i)))
            .collect();
        for connected in [false, true, false, true] {
            for workers in [1, 2, 4] {
                let shared = shared_socket_on(Ipv4Addr::UNSPECIFIED);
                let fanout = Fanout::start(&shared, config(workers, connected)).unwrap();
                let started = Instant::now();
                for _ in 0..10 {
                    fanout.send(FanoutJob {
                        is_keyframe: true,
                        packets: vec![vec![0; 1_200]; 85],
                        targets: targets.clone(),
                    });
                }
                let stats = Arc::clone(&fanout.stats);
                drop(fanout);
                let sends = stats.sent_packets.load(Ordering::Relaxed);
                println!(
                    "connected={connected} workers={workers}: {:.1} ms per IDR, {:.2} us/send, errors={} connect_failures={}",
                    started.elapsed().as_secs_f64() * 100.0,
                    started.elapsed().as_secs_f64() * 1e6 / sends.max(1) as f64,
                    stats.send_errors.load(Ordering::Relaxed),
                    stats.connect_failures.load(Ordering::Relaxed),
                );
            }
        }
    }
}
