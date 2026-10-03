//! Bounded input/output association, including synchronous codecs that buffer.
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};
#[derive(Clone, Copy, Debug)]
pub struct SourceTiming {
    pub captured_at: Instant,
    pub capture_unix_us: u64,
    pub submitted_at: Instant,
}
#[derive(Default)]
pub struct SourceTimeline {
    pending: VecDeque<(Duration, SourceTiming)>,
    last_capture: Option<(Instant, u64)>,
    pub evicted_inputs: u64,
    pub unmapped_outputs: u64,
}
impl SourceTimeline {
    pub fn record(
        &mut self,
        timestamp: Duration,
        captured_at: Instant,
        submitted_at: Instant,
        unix_us: u64,
    ) {
        let capture_unix_us = match self.last_capture {
            Some((previous, wall)) if previous == captured_at => wall,
            _ => {
                let wall = unix_us.saturating_sub(
                    u64::try_from(
                        submitted_at
                            .saturating_duration_since(captured_at)
                            .as_micros(),
                    )
                    .unwrap_or(u64::MAX),
                );
                self.last_capture = Some((captured_at, wall));
                wall
            }
        };
        if self.pending.len() == 128 {
            self.pending.pop_front();
            self.evicted_inputs += 1;
        }
        self.pending.push_back((
            timestamp,
            SourceTiming {
                captured_at,
                capture_unix_us,
                submitted_at,
            },
        ));
    }
    pub fn take(&mut self, timestamp: Duration) -> Option<SourceTiming> {
        let Some(index) = self.pending.iter().position(|(time, _)| *time == timestamp) else {
            self.unmapped_outputs += 1;
            return None;
        };
        let result = self.pending[index].1;
        self.pending.drain(..=index);
        Some(result)
    }
    #[must_use]
    pub fn depth(&self) -> usize {
        self.pending.len()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delayed_output_maps_to_its_input_and_repeat_wall_time_is_stable() {
        let now = Instant::now();
        let captured = now.checked_sub(Duration::from_millis(5)).unwrap();
        let mut timeline = SourceTimeline::default();
        timeline.record(Duration::ZERO, captured, now, 10000);
        timeline.record(
            Duration::from_millis(33),
            captured,
            now + Duration::from_millis(33),
            43007,
        );
        timeline.record(
            Duration::from_millis(66),
            now,
            now + Duration::from_millis(66),
            76000,
        );
        let first = timeline.take(Duration::ZERO).unwrap();
        let repeated = timeline.take(Duration::from_millis(33)).unwrap();
        assert_eq!(first.capture_unix_us, repeated.capture_unix_us);
        assert_eq!(first.capture_unix_us, 5000);
        assert_eq!(first.captured_at, captured);
        assert_eq!(timeline.depth(), 1);
        assert!(timeline.take(Duration::from_millis(12)).is_none());
    }
    #[test]
    fn evicted_output_is_unmapped_instead_of_getting_latest_source() {
        let now = Instant::now();
        let mut timeline = SourceTimeline::default();
        for us in 0..129 {
            timeline.record(Duration::from_micros(us), now, now, 1000);
        }
        assert_eq!(timeline.depth(), 128);
        assert_eq!(timeline.evicted_inputs, 1);
        assert!(timeline.take(Duration::ZERO).is_none());
        assert_eq!(timeline.unmapped_outputs, 1);
    }
}
