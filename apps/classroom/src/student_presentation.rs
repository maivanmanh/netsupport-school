use crate::presentation_pacer::PresentationPacer;
use media_core::DecodedFrame;
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

const SAMPLE_LIMIT: usize = 4096;

/// Shared between decoder and window owner. Never hold this lock while calling
/// a codec or renderer: only one waiting decoded frame crosses the boundary.
pub struct StudentPresentation {
    pacer: PresentationPacer,
    ready_at: Option<Instant>,
    rendered_frames: u64,
    latencies: VecDeque<Duration>,
    interval_latencies: VecDeque<Duration>,
    interval_waits: VecDeque<Duration>,
}

pub struct RenderSnapshot {
    pub rendered_frames: u64,
    pub replaced_frames: u64,
    pub pending_frames: u8,
    pub gap_p95: Duration,
    pub total_gap_p95: Duration,
    pub latency_p95: Duration,
    pub total_latency_p50: Duration,
    pub total_latency_p95: Duration,
    pub wait_p95: Duration,
}

impl StudentPresentation {
    pub fn new(max_fps: u32, now: Instant) -> Self {
        Self {
            pacer: PresentationPacer::new(max_fps, now),
            ready_at: None,
            rendered_frames: 0,
            latencies: VecDeque::new(),
            interval_latencies: VecDeque::new(),
            interval_waits: VecDeque::new(),
        }
    }

    pub fn submit(&mut self, frame: DecodedFrame, now: Instant) {
        self.pacer.submit(frame);
        self.ready_at = Some(now);
    }

    pub fn clear(&mut self) {
        self.pacer.clear();
        self.ready_at = None;
    }

    pub fn take_due(&mut self, now: Instant) -> Option<(DecodedFrame, Instant)> {
        let frame = self.pacer.take_due(now)?;
        Some((frame, self.ready_at.take().unwrap_or(now)))
    }

    pub fn note_presented(&mut self, started: Instant, ready: Instant, finished: Instant) {
        self.rendered_frames = self.rendered_frames.saturating_add(1);
        self.pacer.note_presented(finished);
        let latency = finished.saturating_duration_since(started);
        push_sample(&mut self.latencies, latency);
        push_sample(&mut self.interval_latencies, latency);
        push_sample(
            &mut self.interval_waits,
            started.saturating_duration_since(ready),
        );
    }

    pub fn snapshot(&mut self) -> RenderSnapshot {
        let snapshot = RenderSnapshot {
            rendered_frames: self.rendered_frames,
            replaced_frames: self.pacer.replaced_frames(),
            pending_frames: u8::from(self.ready_at.is_some()),
            gap_p95: self.pacer.take_interval_render_gap_p95(),
            total_gap_p95: self.pacer.render_gap_p95(),
            latency_p95: percentile(&self.interval_latencies, 95),
            total_latency_p50: percentile(&self.latencies, 50),
            total_latency_p95: percentile(&self.latencies, 95),
            wait_p95: percentile(&self.interval_waits, 95),
        };
        self.interval_latencies.clear();
        self.interval_waits.clear();
        snapshot
    }
}

fn push_sample(samples: &mut VecDeque<Duration>, value: Duration) {
    if samples.len() == SAMPLE_LIMIT {
        samples.pop_front();
    }
    samples.push_back(value);
}

fn percentile(samples: &VecDeque<Duration>, percent: usize) -> Duration {
    let mut sorted: Vec<_> = samples.iter().copied().collect();
    sorted.sort_unstable();
    if sorted.is_empty() {
        Duration::ZERO
    } else {
        sorted[(sorted.len() - 1) * percent / 100]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
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
    fn decoder_can_replace_frames_while_renderer_holds_in_flight_frame() {
        let start = Instant::now();
        let output = Mutex::new(StudentPresentation::new(30, start));
        output.lock().unwrap().submit(frame(1), start);
        let (in_flight, ready) = output.lock().unwrap().take_due(start).unwrap();
        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let mut slot = output.lock().unwrap();
                    slot.submit(frame(2), start + Duration::from_millis(10));
                    slot.submit(frame(3), start + Duration::from_millis(20));
                })
                .join()
                .unwrap();
        });
        assert_eq!(in_flight.bytes, [1]);
        let mut slot = output.lock().unwrap();
        slot.note_presented(start, ready, start + Duration::from_millis(50));
        let snapshot = slot.snapshot();
        assert_eq!(snapshot.pending_frames, 1);
        assert_eq!(snapshot.replaced_frames, 1);
        assert_eq!(snapshot.latency_p95, Duration::from_millis(50));
        let (newest, ready) = slot.take_due(start + Duration::from_millis(51)).unwrap();
        assert_eq!(newest.bytes, [3]);
        slot.note_presented(
            start + Duration::from_millis(51),
            ready,
            start + Duration::from_millis(52),
        );
        assert_eq!(slot.snapshot().wait_p95, Duration::from_millis(31));
        slot.submit(frame(4), start);
        slot.clear();
        assert_eq!(slot.snapshot().pending_frames, 0);
        assert!(slot.take_due(start + Duration::from_secs(1)).is_none());
    }
}
