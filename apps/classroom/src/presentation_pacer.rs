use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use media_core::DecodedFrame;

/// Holds at most one decoded preview frame. Decode remains continuous so H.264
/// reference state is preserved; only obsolete frames waiting to be shown are
/// replaced when the receiver catches up after a burst.
pub struct PresentationPacer {
    minimum_gap: Duration,
    max_fps: usize,
    admitted: VecDeque<Instant>,
    pending: Option<DecodedFrame>,
    replaced_frames: u64,
    last_presented_at: Option<Instant>,
    render_gaps: VecDeque<Duration>,
    interval_render_gaps: Vec<Duration>,
}

impl PresentationPacer {
    pub fn new(max_fps: u32, _now: Instant) -> Self {
        Self {
            minimum_gap: Duration::from_secs_f64(0.9 / f64::from(max_fps.clamp(1, 60))),
            max_fps: max_fps.clamp(1, 60) as usize,
            admitted: VecDeque::new(),
            pending: None,
            replaced_frames: 0,
            last_presented_at: None,
            render_gaps: VecDeque::new(),
            interval_render_gaps: Vec::new(),
        }
    }

    pub fn submit(&mut self, frame: DecodedFrame) {
        if self.pending.replace(frame).is_some() {
            self.replaced_frames = self.replaced_frames.saturating_add(1);
        }
    }

    pub fn clear(&mut self) {
        self.pending = None;
    }

    pub fn take_due(&mut self, now: Instant) -> Option<DecodedFrame> {
        while self
            .admitted
            .front()
            .is_some_and(|at| now.saturating_duration_since(*at) >= Duration::from_secs(1))
        {
            self.admitted.pop_front();
        }
        if self.admitted.len() >= self.max_fps
            || self
                .admitted
                .back()
                .is_some_and(|at| now.saturating_duration_since(*at) < self.minimum_gap)
        {
            return None;
        }
        let frame = self.pending.take()?;
        // Source arrival drives presentation; no independent fixed phase holds
        // every decoded frame. A short spacing guard prevents burst playback and
        // a rolling one-second budget enforces the configured FPS ceiling.
        self.admitted.push_back(now);
        Some(frame)
    }

    pub fn note_presented(&mut self, now: Instant) {
        if let Some(previous) = self.last_presented_at {
            let gap = now.saturating_duration_since(previous);
            if self.render_gaps.len() == 4_096 {
                self.render_gaps.pop_front();
            }
            self.render_gaps.push_back(gap);
            if self.interval_render_gaps.len() < 4_096 {
                self.interval_render_gaps.push(gap);
            }
        }
        self.last_presented_at = Some(now);
    }

    pub const fn replaced_frames(&self) -> u64 {
        self.replaced_frames
    }

    pub fn render_gap_p95(&self) -> Duration {
        let mut gaps: Vec<_> = self.render_gaps.iter().copied().collect();
        gaps.sort_unstable();
        if gaps.is_empty() {
            return Duration::ZERO;
        }
        gaps[(gaps.len() - 1) * 95 / 100]
    }

    pub fn take_interval_render_gap_p95(&mut self) -> Duration {
        self.interval_render_gaps.sort_unstable();
        let result = if self.interval_render_gaps.is_empty() {
            Duration::ZERO
        } else {
            self.interval_render_gaps[(self.interval_render_gaps.len() - 1) * 95 / 100]
        };
        self.interval_render_gaps.clear();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(value: u8) -> DecodedFrame {
        DecodedFrame {
            format: media_core::DecodedPixelFormat::Bgra,
            media_timestamp: None,
            timing: None,
            bytes: vec![value],
            width: 1,
            height: 1,
        }
    }

    #[test]
    fn replaces_waiting_frame_and_never_catches_up_with_a_burst() {
        let start = Instant::now();
        let mut pacer = PresentationPacer::new(30, start);
        pacer.submit(frame(1));
        assert_eq!(pacer.take_due(start).unwrap().bytes, [1]);
        pacer.note_presented(start);

        pacer.submit(frame(2));
        pacer.submit(frame(3));
        assert!(pacer.take_due(start + Duration::from_millis(20)).is_none());
        assert_eq!(pacer.replaced_frames(), 1);
        assert_eq!(
            pacer
                .take_due(start + Duration::from_millis(100))
                .unwrap()
                .bytes,
            [3]
        );
        pacer.note_presented(start + Duration::from_millis(100));

        pacer.submit(frame(4));
        assert!(pacer.take_due(start + Duration::from_millis(101)).is_none());
        assert_eq!(
            pacer
                .take_due(start + Duration::from_millis(134))
                .unwrap()
                .bytes,
            [4]
        );
        assert_eq!(pacer.render_gap_p95(), Duration::from_millis(100));
        assert_eq!(
            pacer.take_interval_render_gap_p95(),
            Duration::from_millis(100)
        );
        assert_eq!(pacer.take_interval_render_gap_p95(), Duration::ZERO);
    }

    #[test]
    fn new_source_can_advance_presentation_phase_after_a_late_tick() {
        let start = Instant::now();
        let mut pacer = PresentationPacer::new(30, start);
        pacer.submit(frame(1));
        assert!(pacer.take_due(start).is_some());
        let one_tick = start + Duration::from_secs_f64(1.0 / 30.0);
        pacer.submit(frame(2));
        assert!(pacer
            .take_due(one_tick + Duration::from_millis(4))
            .is_some());
        pacer.submit(frame(3));
        assert!(pacer
            .take_due(one_tick + Duration::from_millis(32))
            .is_none());
        assert!(pacer
            .take_due(one_tick + Duration::from_millis(34))
            .is_some());
    }
    #[test]
    fn bursty_source_never_exceeds_sixty_in_a_rolling_second() {
        let start = Instant::now();
        let mut pacer = PresentationPacer::new(120, start);
        let mut rendered = VecDeque::new();
        for ms in 0..3000 {
            let now = start + Duration::from_millis(ms);
            pacer.submit(frame(1));
            if pacer.take_due(now).is_some() {
                while rendered
                    .front()
                    .is_some_and(|at| now.duration_since(*at) >= Duration::from_secs(1))
                {
                    rendered.pop_front();
                }
                rendered.push_back(now);
                // Hard ceiling for enhanced one-to-one sessions; broadcast
                // sessions are configured at 30.
                assert!(rendered.len() <= 60);
            }
        }
        assert!(pacer.replaced_frames() > 2800);
    }
}
