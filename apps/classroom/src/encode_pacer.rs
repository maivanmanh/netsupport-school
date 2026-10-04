use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

/// Bounded encode admission. Capture may advance the phase, but never borrow
/// more than the configured number of frames from a rolling one-second budget.
pub struct EncodePacer {
    max_fps: usize,
    admitted: VecDeque<Instant>,
}

impl EncodePacer {
    pub fn new(max_fps: u32) -> Self {
        Self {
            max_fps: max_fps.max(1) as usize,
            admitted: VecDeque::new(),
        }
    }

    fn expire(&mut self, now: Instant) {
        while self
            .admitted
            .front()
            .is_some_and(|at| now.saturating_duration_since(*at) >= Duration::from_secs(1))
        {
            self.admitted.pop_front();
        }
    }

    pub fn deadline(&mut self, now: Instant, requested: Instant, interval: Duration) -> Instant {
        self.expire(now);
        let mut deadline = requested;
        if let Some(last) = self.admitted.back() {
            // Small phase adjustments absorb capture/scheduler jitter without
            // permitting burst playback. The rolling budget enforces the cap.
            deadline = deadline.max(*last + interval.mul_f64(0.85));
        }
        if self.admitted.len() >= self.max_fps {
            deadline = deadline.max(self.admitted[0] + Duration::from_secs(1));
        }
        deadline
    }

    pub fn admitted(&mut self, now: Instant) {
        self.expire(now);
        debug_assert!(self.admitted.len() < self.max_fps);
        self.admitted.push_back(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_captures_and_stalls_never_exceed_thirty_in_a_rolling_second() {
        let start = Instant::now();
        let interval = Duration::from_secs_f64(1.0 / 30.0);
        let mut pacer = EncodePacer::new(30);
        let mut history = VecDeque::new();
        let mut total = 0;
        for ms in 0..4_000 {
            let now = start + Duration::from_millis(ms);
            if (700..900).contains(&ms) || pacer.deadline(now, now, interval) > now {
                continue;
            }
            while history
                .front()
                .is_some_and(|at| now.duration_since(*at) >= Duration::from_secs(1))
            {
                history.pop_front();
            }
            if let Some(previous) = history.back() {
                assert!(now.duration_since(*previous) >= interval.mul_f64(0.85));
            }
            history.push_back(now);
            assert!(history.len() <= 30);
            pacer.admitted(now);
            total += 1;
        }
        assert!(
            total >= 110,
            "scheduler jitter must not permanently lower cadence"
        );
        assert!(pacer.admitted.len() <= 30);
    }

    #[test]
    fn future_requested_tick_does_not_expire_budget_early() {
        let start = Instant::now();
        let interval = Duration::from_millis(20);
        let mut pacer = EncodePacer::new(1);
        pacer.admitted(start);
        pacer.deadline(start, start + Duration::from_secs(10), interval);
        assert_eq!(
            pacer.deadline(start, start, interval),
            start + Duration::from_secs(1)
        );
    }
}
