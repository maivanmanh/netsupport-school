use std::{
    io,
    net::{SocketAddrV4, UdpSocket},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    thread,
    time::{Duration, Instant},
};

/// One AU has one retry budget shared by all endpoints, so forty blocked
/// Students cannot multiply the Teacher's stall. No retry/frame queue exists.
pub struct SendBudget {
    deadline: Instant,
    pub would_block: u64,
    /// Transient kernel send-queue exhaustion, distinct from `WouldBlock`.
    pub resource_backpressure: u64,
    pub recovered: u64,
    pub retry_wait: Duration,
    pub pacing_wait: Duration,
    pub deadline_exhausted: u64,
    pub retry_budget_exhausted: u64,
    remaining_retry_budget: Duration,
}

impl SendBudget {
    pub fn new(now: Instant, duration: Duration) -> Self {
        Self {
            deadline: now + duration,
            would_block: 0,
            resource_backpressure: 0,
            recovered: 0,
            retry_wait: Duration::ZERO,
            pacing_wait: Duration::ZERO,
            deadline_exhausted: 0,
            retry_budget_exhausted: 0,
            remaining_retry_budget: duration.min(Duration::from_millis(2)),
        }
    }

    /// Deliberate pacing is not socket retry work. Keep the 2ms active retry
    /// allowance and the send-work deadline; do not let pacing consume both.
    pub fn exclude_pacing(&mut self, wait: Duration) {
        self.deadline += wait;
    }

    pub fn send_to_paced(
        &mut self,
        socket: &UdpSocket,
        packet: &[u8],
        endpoint: SocketAddrV4,
        pacer: &AggregatePacer,
        deadline: Instant,
    ) -> io::Result<usize> {
        self.send_with_pacing(
            || socket.send_to(packet, endpoint),
            Some((pacer, deadline, packet.len())),
        )
    }

    pub fn send_connected_paced(
        &mut self,
        socket: &UdpSocket,
        packet: &[u8],
        pacer: &AggregatePacer,
        deadline: Instant,
    ) -> io::Result<usize> {
        self.send_with_pacing(
            || socket.send(packet),
            Some((pacer, deadline, packet.len())),
        )
    }

    fn attempt(
        &mut self,
        send: &mut impl FnMut() -> io::Result<usize>,
        pacing: Option<(&AggregatePacer, Instant, usize)>,
        paced: &mut Duration,
    ) -> io::Result<usize> {
        if let Some((pacer, deadline, bytes)) = pacing {
            let wait = pacer.wait(bytes, deadline)?;
            self.exclude_pacing(wait);
            self.pacing_wait += wait;
            *paced += wait;
        }
        send()
    }

    #[cfg(test)]
    fn send_with(&mut self, send: impl FnMut() -> io::Result<usize>) -> io::Result<usize> {
        self.send_with_pacing(send, None)
    }

    fn send_with_pacing(
        &mut self,
        mut send: impl FnMut() -> io::Result<usize>,
        pacing: Option<(&AggregatePacer, Instant, usize)>,
    ) -> io::Result<usize> {
        let mut paced = Duration::ZERO;
        let first = match self.attempt(&mut send, pacing, &mut paced) {
            Err(error) if is_transient_backpressure(&error) => error,
            result => return result,
        };
        if first.kind() == io::ErrorKind::WouldBlock {
            self.would_block += 1;
        } else {
            self.resource_backpressure += 1;
        }
        let started = Instant::now();
        if self.remaining_retry_budget.is_zero() {
            self.retry_budget_exhausted += 1;
        } else if started >= self.deadline {
            self.deadline_exhausted += 1;
        }
        let mut retry_paced = Duration::ZERO;
        let result = loop {
            if started.elapsed().saturating_sub(retry_paced) >= self.remaining_retry_budget
                || Instant::now() >= self.deadline
            {
                break Err(first);
            }
            // Yield rather than sleeping: Windows timer granularity must not
            // turn a sub-ms socket retry into a full missed video tick.
            thread::yield_now();
            match self.attempt(&mut send, pacing, &mut retry_paced) {
                Err(error) if is_transient_backpressure(&error) => {}
                result => break result,
            }
        };
        let waited = started.elapsed().saturating_sub(retry_paced);
        self.retry_wait += waited;
        self.remaining_retry_budget = self.remaining_retry_budget.saturating_sub(waited);
        if result.is_ok() {
            self.recovered += 1;
        }
        result
    }
}

fn is_transient_backpressure(error: &io::Error) -> bool {
    if error.kind() == io::ErrorKind::WouldBlock {
        return true;
    }
    match error.raw_os_error() {
        #[cfg(target_os = "macos")]
        Some(55) => true, // ENOBUFS
        #[cfg(target_os = "freebsd")]
        Some(55) => true, // ENOBUFS
        #[cfg(target_os = "linux")]
        Some(105) => true, // ENOBUFS
        #[cfg(target_os = "windows")]
        Some(10055) => true, // WSAENOBUFS
        _ => false,
    }
}

/// One session-wide wire-rate budget for video, repair and pointer. Reservations
/// are at most one datagram per sending thread; no packet queue is created.
/// UDP/IP/Ethernet framing, preamble and inter-frame gap are budgeted conservatively.
pub struct AggregatePacer {
    mbps: u32,
    clock: Mutex<RateClock>,
    pub wait_us: AtomicU64,
    pub denied: AtomicU64,
}

struct RateClock {
    next_free: Instant,
    burst: Duration,
}

impl RateClock {
    fn reserve(&mut self, now: Instant, cost: Duration, deadline: Instant) -> Option<Instant> {
        let target = self
            .next_free
            .checked_sub(self.burst)
            .unwrap_or(now)
            .max(now);
        if target >= deadline {
            return None;
        }
        self.next_free = self.next_free.max(now) + cost;
        Some(target)
    }
}

impl AggregatePacer {
    pub fn new(mbps: u32) -> Self {
        Self {
            mbps,
            clock: Mutex::new(RateClock {
                next_free: Instant::now(),
                burst: wire_duration(16_384, mbps),
            }),
            wait_us: AtomicU64::new(0),
            denied: AtomicU64::new(0),
        }
    }

    /// Eighty percent of the wire budget is available to the encoded stream;
    /// leave room for packet headers, recovery and pointer state.
    pub fn bitrate_ceiling(&self, recipients: usize) -> u32 {
        if self.mbps == 0 || recipients == 0 {
            return u32::MAX;
        }
        u32::try_from(u64::from(self.mbps) * 800_000 / recipients as u64).unwrap_or(u32::MAX)
    }

    pub fn fits(
        &self,
        payload_bytes: usize,
        packets: usize,
        recipients: usize,
        budget: Duration,
    ) -> bool {
        self.mbps == 0
            || wire_duration(
                payload_bytes
                    .saturating_add(packets.saturating_mul(80))
                    .saturating_mul(recipients),
                self.mbps,
            ) <= budget
    }

    pub fn wait(&self, payload_bytes: usize, deadline: Instant) -> io::Result<Duration> {
        let started = Instant::now();
        let target = if self.mbps == 0 {
            (started < deadline).then_some(started)
        } else {
            self.clock.lock().unwrap().reserve(
                started,
                wire_duration(payload_bytes.saturating_add(80), self.mbps),
                deadline,
            )
        };
        let Some(target) = target else {
            self.denied.fetch_add(1, Ordering::Relaxed);
            return Err(io::ErrorKind::TimedOut.into());
        };
        while Instant::now() < target {
            let remaining = target.saturating_duration_since(Instant::now());
            if remaining > Duration::from_millis(2) {
                thread::sleep(Duration::from_millis(1));
            } else {
                thread::yield_now();
            }
        }
        let waited = started.elapsed();
        self.wait_us.fetch_add(
            u64::try_from(waited.as_micros()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
        if Instant::now() >= deadline {
            self.denied.fetch_add(1, Ordering::Relaxed);
            return Err(io::ErrorKind::TimedOut.into());
        }
        Ok(waited)
    }
}

fn wire_duration(bytes: usize, mbps: u32) -> Duration {
    if mbps == 0 {
        return Duration::ZERO;
    }
    Duration::from_nanos(
        (bytes as u64)
            .saturating_mul(8_000)
            .div_ceil(u64::from(mbps)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pacing_large_idr_respects_wire_rate_and_can_be_disabled() {
        let now = Instant::now();
        let pacer = AggregatePacer::new(400);
        let mut clock = pacer.clock.lock().unwrap();
        let mut last = now;
        for _ in 0..10_500 {
            last = clock
                .reserve(now, wire_duration(1280, 400), now + Duration::from_secs(1))
                .unwrap();
        }
        assert!(last.duration_since(now) > Duration::from_millis(250));
        let saved = clock.next_free;
        assert!(clock
            .reserve(
                now,
                wire_duration(1280, 400),
                now + Duration::from_millis(8)
            )
            .is_none());
        assert_eq!(
            clock.next_free, saved,
            "rejected work must not create rate debt"
        );
        drop(clock);
        assert!(AggregatePacer::new(0)
            .wait(2_000_000, now + Duration::from_secs(1))
            .is_ok());
        assert_eq!(pacer.bitrate_ceiling(40), 8_000_000);
        assert!(pacer.fits(361_003, 310, 35, Duration::from_millis(300)));
        assert!(!pacer.fits(361_003, 310, 35, Duration::from_millis(120)));
    }

    #[test]
    fn fanout_bursts_share_one_aggregate_allowance() {
        let now = Instant::now();
        let pacer = AggregatePacer::new(400);
        let mut clock = pacer.clock.lock().unwrap();
        for _worker in 0..4 {
            for _ in 0..4 {
                clock
                    .reserve(now, wire_duration(1280, 400), now + Duration::from_secs(1))
                    .unwrap();
            }
        }
        assert!(clock.next_free.duration_since(now) > clock.burst);
        assert!(
            clock
                .reserve(now, wire_duration(1280, 400), now + Duration::from_secs(1))
                .unwrap()
                > now
        );
    }

    #[test]
    fn intentional_pacing_does_not_expire_unused_retry_budget() {
        let now = Instant::now();
        let mut budget = SendBudget::new(now - Duration::from_secs(1), Duration::from_millis(10));
        budget.exclude_pacing(Duration::from_secs(1));
        let mut attempts = 0;
        let result = budget.send_with(|| {
            attempts += 1;
            if attempts == 1 {
                Err(io::ErrorKind::WouldBlock.into())
            } else {
                Ok(1200)
            }
        });
        assert!(result.is_ok());
        assert_eq!(budget.recovered, 1);
        assert_eq!(budget.deadline_exhausted, 0);
        assert!(budget.remaining_retry_budget <= Duration::from_millis(2));
    }

    #[test]
    fn expired_send_work_is_not_retried_without_pacing() {
        let mut budget = SendBudget::new(
            Instant::now() - Duration::from_secs(1),
            Duration::from_millis(10),
        );
        assert!(budget
            .send_with(|| Err(io::ErrorKind::WouldBlock.into()))
            .is_err());
        assert_eq!(budget.deadline_exhausted, 1);
        assert_eq!(budget.recovered, 0);
        assert_eq!(
            budget.remaining_retry_budget,
            Duration::from_millis(2) - budget.retry_wait
        );
    }

    #[test]
    fn recovers_transient_backpressure_without_dropping_packet() {
        let mut budget = SendBudget::new(Instant::now(), Duration::from_secs(1));
        let mut attempts = 0;
        let result = budget.send_with(|| {
            attempts += 1;
            if attempts == 1 {
                Err(io::ErrorKind::WouldBlock.into())
            } else {
                Ok(1200)
            }
        });
        assert_eq!(result.unwrap(), 1200);
        assert_eq!((budget.would_block, budget.recovered), (1, 1));
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn recovers_macos_enobufs_without_abandoning_access_unit() {
        let mut budget = SendBudget::new(Instant::now(), Duration::from_secs(1));
        let mut attempts = 0;
        let result = budget.send_with(|| {
            attempts += 1;
            if attempts == 1 {
                Err(io::Error::from_raw_os_error(55))
            } else {
                Ok(1200)
            }
        });
        assert_eq!(result.unwrap(), 1200);
        assert_eq!(budget.would_block, 0);
        assert_eq!(budget.resource_backpressure, 1);
        assert_eq!(budget.recovered, 1);
    }
    #[test]
    fn exhausted_budget_is_shared_and_permanent_errors_are_not_retried() {
        let mut budget = SendBudget::new(Instant::now(), Duration::ZERO);
        let mut attempts = 0;
        for _ in 0..40 {
            assert!(budget
                .send_with(|| {
                    attempts += 1;
                    Err(io::ErrorKind::WouldBlock.into())
                })
                .is_err());
        }
        assert_eq!(attempts, 40);
        assert_eq!(budget.recovered, 0);
        let mut budget = SendBudget::new(Instant::now(), Duration::from_secs(1));
        assert_eq!(
            budget
                .send_with(|| Err(io::ErrorKind::AddrNotAvailable.into()))
                .unwrap_err()
                .kind(),
            io::ErrorKind::AddrNotAvailable
        );
        assert_eq!(budget.would_block, 0);
    }
}
