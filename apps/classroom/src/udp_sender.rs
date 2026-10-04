use std::{
    io,
    net::{SocketAddrV4, UdpSocket},
    thread,
    time::{Duration, Instant},
};

/// One AU has one retry budget shared by all endpoints, so forty blocked
/// Students cannot multiply the Teacher's stall. No retry/frame queue exists.
pub struct SendBudget {
    deadline: Instant,
    pub would_block: u64,
    pub recovered: u64,
    pub retry_wait: Duration,
    remaining_retry_budget: Duration,
}

impl SendBudget {
    pub fn new(now: Instant, duration: Duration) -> Self {
        Self {
            deadline: now + duration,
            would_block: 0,
            recovered: 0,
            retry_wait: Duration::ZERO,
            remaining_retry_budget: duration.min(Duration::from_millis(2)),
        }
    }

    pub fn send(
        &mut self,
        socket: &UdpSocket,
        packet: &[u8],
        endpoint: SocketAddrV4,
    ) -> io::Result<usize> {
        self.send_with(|| socket.send_to(packet, endpoint))
    }

    /// Same retry budget for a socket already `connect()`ed to its Student.
    pub fn send_connected(&mut self, socket: &UdpSocket, packet: &[u8]) -> io::Result<usize> {
        self.send_with(|| socket.send(packet))
    }

    fn send_with(&mut self, mut send: impl FnMut() -> io::Result<usize>) -> io::Result<usize> {
        let first = match send() {
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => error,
            result => return result,
        };
        self.would_block += 1;
        let started = Instant::now();
        let retry_deadline = (started + self.remaining_retry_budget).min(self.deadline);
        let result = loop {
            if Instant::now() >= retry_deadline {
                break Err(first);
            }
            // Yield rather than sleeping: Windows timer granularity must not
            // turn a sub-ms socket retry into a full missed video tick.
            thread::yield_now();
            match send() {
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                result => break result,
            }
        };
        let waited = started.elapsed();
        self.retry_wait += waited;
        self.remaining_retry_budget = self.remaining_retry_budget.saturating_sub(waited);
        if result.is_ok() {
            self.recovered += 1;
        }
        result
    }
}

/// Spreads one packetized AU in bounded batches. Its entire pacing span is
/// capped, including for a large IDR or forty recipients; never queues an AU.
pub struct PacketPacer {
    started: Instant,
    span: Duration,
    packets: usize,
    batch_packets: usize,
    pub wait: Duration,
}

impl PacketPacer {
    #[allow(clippy::cast_precision_loss)] // AU bytes and packet counts are small bounded values.
    pub fn new(
        packet_bytes: usize,
        packets: usize,
        recipients: usize,
        mbps: u32,
        max_span: Duration,
    ) -> Self {
        let bytes = packet_bytes.saturating_mul(recipients);
        let span = if mbps == 0 {
            Duration::ZERO
        } else {
            Duration::from_secs_f64(bytes as f64 * 8.0 / (f64::from(mbps) * 1_000_000.0))
                .min(max_span)
        };
        Self {
            started: Instant::now(),
            span,
            packets,
            // Around 16 KiB per burst across all recipients, at least one RTP packet.
            batch_packets: (16_384 / recipients.max(1) / 1_200).max(1),
            wait: Duration::ZERO,
        }
    }

    #[allow(clippy::cast_precision_loss)]
    pub fn before_packet(&mut self, index: usize) {
        if self.span.is_zero() || self.packets == 0 || index % self.batch_packets != 0 {
            return;
        }
        let target = self.started + self.span.mul_f64(index as f64 / self.packets as f64);
        let started = Instant::now();
        while Instant::now() < target {
            thread::yield_now();
        }
        self.wait += started.elapsed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pacing_caps_large_idr_and_can_be_disabled() {
        let cap = Duration::from_millis(8);
        let paced = PacketPacer::new(2_000_000, 2_000, 40, 400, cap);
        assert_eq!(paced.span, cap);
        assert_eq!(paced.batch_packets, 1);
        let single = PacketPacer::new(20_000, 20, 1, 400, cap);
        assert_eq!(single.span, Duration::from_micros(400));
        let mut disabled = PacketPacer::new(2_000_000, 2_000, 40, 0, cap);
        disabled.before_packet(1_999);
        assert_eq!(disabled.wait, Duration::ZERO);
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
