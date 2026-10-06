//! Teacher RTP fan-out off the encode thread (docs/17 §9).
//!
//! The encode thread packetizes once and hands each AU, shared behind an
//! `Arc`, to a few worker threads. Students are sharded by client id. Each
//! worker has a one-AU bounded queue: when a worker is still busy with the
//! previous AU, the encode thread waits (the capture slot keeps replacing old
//! frames upstream), so the stage stays bounded and nothing is dropped from a
//! reference chain.
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
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use media_core::diagnostics::StageMetrics;
use socket2::{Domain, Protocol, Socket, Type};
use tracing::{info, warn};

use crate::udp_sender::{PacketPacer, SendBudget};

#[derive(Clone, Copy, Debug)]
pub struct FanoutConfig {
    pub workers: usize,
    pub connected: bool,
    pub pacing_mbps: u32,
    pub max_pacing_span: Duration,
    pub send_buffer_bytes: usize,
}

/// One packetized AU and the Students that should receive it.
pub struct FanoutJob {
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
    pub pacing_wait_us: AtomicU64,
    /// Time the encode thread spent waiting for a busy worker.
    pub handoff_wait_us: AtomicU64,
    pub connected_sockets: AtomicU64,
    pub connect_failures: AtomicU64,
}

pub struct Fanout {
    workers: Vec<(SyncSender<Arc<FanoutJob>>, JoinHandle<()>)>,
    pub stats: Arc<FanoutStats>,
    stages: Arc<Mutex<StageMetrics>>,
}

impl Fanout {
    pub fn start(socket: &UdpSocket, config: FanoutConfig) -> io::Result<Self> {
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
        let mut workers = Vec::with_capacity(count);
        for index in 0..count {
            let (sender, receiver) = mpsc::sync_channel(1);
            let worker = Worker {
                index: index as u64,
                count: count as u64,
                local,
                shared: socket.try_clone()?,
                config,
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
            local = %local,
            "teacher RTP fan-out workers ready"
        );
        Ok(Self {
            workers,
            stats,
            stages,
        })
    }

    /// Hands one AU to every worker; waits only while a worker still holds a
    /// queued AU (bounded back-pressure, never an unbounded queue).
    pub fn send(&self, job: FanoutJob) {
        let job = Arc::new(job);
        let started = Instant::now();
        for (sender, _) in &self.workers {
            // A worker only disappears by panicking; the next session restarts it.
            let _ = sender.send(Arc::clone(&job));
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
    stats: Arc<FanoutStats>,
    stages: Arc<Mutex<StageMetrics>>,
    /// client id -> (endpoint, connected socket when available)
    sockets: HashMap<u64, (SocketAddrV4, Option<UdpSocket>)>,
    warned_at: HashMap<u64, Instant>,
}

impl Worker {
    fn run(mut self, jobs: &Receiver<Arc<FanoutJob>>) {
        while let Ok(job) = jobs.recv() {
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
        let mut pacer = PacketPacer::new(
            job.packets.iter().map(Vec::len).sum(),
            job.packets.len(),
            mine.len(),
            // Each worker paces its share of the aggregate budget.
            self.config.pacing_mbps / u32::try_from(self.count).unwrap_or(1),
            self.config.max_pacing_span,
        );
        let mut budget = SendBudget::new(Instant::now(), Duration::from_millis(10));
        let mut failed = HashSet::new();
        let (mut sent, mut bytes, mut errors, mut abandoned) = (0_u64, 0_u64, 0_u64, 0_u64);
        for (index, packet) in job.packets.iter().enumerate() {
            pacer.before_packet(index);
            for (id, endpoint) in &mine {
                if failed.contains(id) {
                    abandoned += 1;
                    continue;
                }
                let result = match self.sockets.get(id).and_then(|(_, s)| s.as_ref()) {
                    Some(socket) => budget.send_connected(socket, packet),
                    None => budget.send(&self.shared, packet, *endpoint),
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
            .pacing_wait_us
            .fetch_add(micros(pacer.wait), Ordering::Relaxed);
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
            max_pacing_span: Duration::from_millis(8),
            send_buffer_bytes: 256 * 1024,
        }
    }

    #[test]
    fn every_student_gets_every_packet_from_the_shared_source_address() {
        for connected in [false, true] {
            let shared = shared_socket();
            let source = shared.local_addr().unwrap();
            let students: Vec<_> = (0..5).map(|_| receiver()).collect();
            let fanout = Fanout::start(&shared, config(2, connected)).unwrap();
            fanout.send(FanoutJob {
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
            packets: vec![vec![0; 64]; 4],
            targets: vec![(7, address)],
        });
        let mut buffer = [0_u8; 128];
        for _ in 0..4 {
            student.recv_from(&mut buffer).unwrap();
        }
        // The next job without the Student closes its connected socket.
        fanout.send(FanoutJob {
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
