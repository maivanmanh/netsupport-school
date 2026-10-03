//! O3: keyframe-storm control. Every IDR source (new receiver route, NACK
//! repair escalation) asks this gate; requests inside the coalescing window
//! share one IDR and a request is deferred — never dropped — until the window
//! reopens. Any keyframe the encoder emits (periodic IDR, bitrate rebuild)
//! satisfies requests made before it.
use std::time::{Duration, Instant};

#[derive(Debug, Default, Clone, Copy)]
pub struct KeyframeStats {
    pub requests: u64,
    pub coalesced: u64,
    pub forced: u64,
    pub emitted: u64,
}

#[derive(Debug)]
pub struct KeyframeGate {
    base_window: Duration,
    /// Widens (×2, up to 2 s) while IDRs keep being forced back-to-back — a
    /// loss storm where more IDRs only add burst load — and relaxes after.
    window: Duration,
    pending_since: Option<Instant>,
    last_keyframe: Option<Instant>,
    last_forced: Option<Instant>,
    pub stats: KeyframeStats,
    /// Largest keyframe access unit seen in the current interval (bytes).
    pub interval_max_keyframe_bytes: usize,
}

impl KeyframeGate {
    pub fn new(window: Duration) -> Self {
        Self {
            base_window: window,
            window,
            pending_since: None,
            last_keyframe: None,
            last_forced: None,
            stats: KeyframeStats::default(),
            interval_max_keyframe_bytes: 0,
        }
    }
    pub fn request(&mut self, now: Instant) {
        self.stats.requests += 1;
        if self.pending_since.is_some() {
            self.stats.coalesced += 1;
        } else {
            self.pending_since = Some(now);
        }
    }
    /// True when the encoder should be asked for an IDR now.
    pub fn poll(&mut self, now: Instant) -> bool {
        if self.pending_since.is_none() {
            return false;
        }
        if self
            .last_keyframe
            .is_some_and(|at| now.saturating_duration_since(at) < self.window)
        {
            return false;
        }
        let storm = self
            .last_forced
            .is_some_and(|at| now.saturating_duration_since(at) < self.window * 2);
        self.window = if storm {
            (self.window * 2).min(Duration::from_secs(2))
        } else {
            self.base_window
        };
        self.pending_since = None;
        self.last_keyframe = Some(now);
        self.last_forced = Some(now);
        self.stats.forced += 1;
        true
    }
    /// Current coalescing window (telemetry).
    pub const fn window(&self) -> Duration {
        self.window
    }
    /// Records an emitted keyframe (forced or periodic).
    pub fn emitted(&mut self, bytes: usize, now: Instant) {
        self.stats.emitted += 1;
        self.interval_max_keyframe_bytes = self.interval_max_keyframe_bytes.max(bytes);
        if self.pending_since.is_some_and(|since| since <= now) {
            self.pending_since = None;
        }
        self.last_keyframe = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn join_burst_shares_one_idr_and_late_request_is_deferred_not_dropped() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut gate = KeyframeGate::new(Duration::from_millis(400));
        // 40 Students join within 100 ms.
        for i in 0..40 {
            gate.request(ms(i * 2));
        }
        assert!(gate.poll(ms(100)));
        assert_eq!(
            (gate.stats.requests, gate.stats.coalesced, gate.stats.forced),
            (40, 39, 1)
        );
        // A loss right after: deferred until the window reopens.
        gate.request(ms(150));
        assert!(!gate.poll(ms(200)));
        assert!(!gate.poll(ms(499)));
        assert!(gate.poll(ms(500)));
        assert_eq!(gate.stats.forced, 2);
    }
    #[test]
    fn storm_widens_window_and_quiet_restores_it() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut gate = KeyframeGate::new(Duration::from_millis(400));
        gate.request(ms(0));
        assert!(gate.poll(ms(0)));
        gate.request(ms(450));
        assert!(gate.poll(ms(450)), "within 2x window: storm");
        assert_eq!(gate.window(), Duration::from_millis(800));
        gate.request(ms(900));
        assert!(!gate.poll(ms(900)), "deferred by the widened window");
        assert!(gate.poll(ms(1250)));
        assert_eq!(gate.window(), Duration::from_millis(1600));
        gate.request(ms(9000));
        assert!(gate.poll(ms(9000)), "quiet period");
        assert_eq!(gate.window(), Duration::from_millis(400));
    }
    #[test]
    fn a_periodic_keyframe_satisfies_pending_requests() {
        let t0 = Instant::now();
        let mut gate = KeyframeGate::new(Duration::from_millis(400));
        gate.request(t0);
        gate.emitted(120_000, t0 + Duration::from_millis(10));
        assert!(!gate.poll(t0 + Duration::from_millis(900)));
        assert_eq!(gate.interval_max_keyframe_bytes, 120_000);
    }
}
