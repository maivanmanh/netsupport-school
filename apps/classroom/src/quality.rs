use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use classroom_config::VideoConfig;
use media_core::{MediaError, VideoEncoder, VideoFrame};

const SAMPLE_STEP: usize = 4;

#[derive(Debug, Default)]
struct ActivitySampler {
    previous: Vec<u32>,
    scratch: Vec<u32>,
}

impl ActivitySampler {
    #[allow(clippy::cast_precision_loss)] // The sampled frame has far fewer than 2^24 pixels.
    fn changed_ratio(&mut self, frame: &VideoFrame) -> f32 {
        let width = frame.width as usize;
        let height = frame.height as usize;
        let pitch = frame.row_pitch as usize;
        self.scratch.clear();
        self.scratch
            .reserve(width.div_ceil(SAMPLE_STEP) * height.div_ceil(SAMPLE_STEP));
        for y in (0..height).step_by(SAMPLE_STEP) {
            for x in (0..width).step_by(SAMPLE_STEP) {
                let offset = y * pitch + x * 4;
                let Some(pixel) = frame.bytes.get(offset..offset + 4) else {
                    continue;
                };
                self.scratch
                    .push(u32::from_ne_bytes([pixel[0], pixel[1], pixel[2], pixel[3]]));
            }
        }
        let ratio = if self.previous.len() == self.scratch.len() && !self.scratch.is_empty() {
            let changed = self
                .scratch
                .iter()
                .zip(&self.previous)
                .filter(|(a, b)| a != b)
                .count();
            changed as f32 / self.scratch.len() as f32
        } else {
            1.0
        };
        std::mem::swap(&mut self.previous, &mut self.scratch);
        ratio
    }
}

#[derive(Clone, Copy, Debug)]
pub struct QualityUpdate {
    pub encode_pressure: bool,
    pub cpu_pressure: bool,
    pub pressure_ceiling_fps: u32,
    pub bitrate_reason: Option<&'static str>,
    pub target_fps: u32,
    pub bitrate_change_bps: Option<u32>,
    pub receiver_loss_percent: Option<f64>,
    pub receiver_reporting_students: usize,
    pub receiver_high_loss_students: usize,
    pub receiver_slow_students: usize,
    pub receiver_render_slow_students: usize,
    pub receiver_feedback_eligible: bool,
    pub receiver_recovery_eligible: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ReceiverMediaReport {
    pub id: u64,
    pub received_packets: u64,
    pub missing_packets: u64,
    pub decoded_frames: u64,
    pub rendered_frames: u64,
    pub rendering_enabled: bool,
    pub render_latency_p95_us: u64,
    pub received_at: Instant,
}

#[derive(Debug, Default)]
struct ReceiverFeedback {
    last_counters: Option<(u64, u64)>,
    interval_received: u64,
    interval_missing: u64,
    last_decode_counter: Option<(u64, Instant)>,
    interval_decoded: u64,
    interval_media_duration: Duration,
    last_render_counter: Option<(u64, Instant)>,
    interval_rendered: u64,
    interval_render_duration: Duration,
    rendering_enabled: bool,
    interval_render_latency_p95_us: u64,
}

#[derive(Clone, Copy, Debug)]
struct ReceiverInterval {
    loss_percent: Option<f64>,
    reporting_students: usize,
    high_loss_students: usize,
    slow_students: usize,
    render_slow_students: usize,
    degraded_students: usize,
    healthy_students: usize,
    feedback_eligible: bool,
    recovery_eligible: bool,
}

/// Shared Teacher-side policy. It samples pixels only to detect activity;
/// video itself remains in the existing single latest-frame slot.
pub struct QualityController {
    config: VideoConfig,
    sampler: ActivitySampler,
    last_large_activity: Instant,
    last_small_activity: Instant,
    activity_ratio: f32,
    target_fps: u32,
    pressure_ceiling_fps: u32,
    pressure_streak: u8,
    healthy_streak: u8,
    bitrate_bps: u32,
    last_bitrate_change: Instant,
    bitrate_supported: bool,
    sender_pressure: bool,
    sender_ceiling_bps: u32,
    degraded_streak: u8,
    low_loss_streak: u8,
    receivers: HashMap<u64, ReceiverFeedback>,
}

impl QualityController {
    pub fn new(config: &VideoConfig, now: Instant) -> Self {
        Self {
            config: config.clone(),
            sampler: ActivitySampler::default(),
            last_large_activity: now,
            last_small_activity: now,
            activity_ratio: 0.0,
            target_fps: config.max_fps,
            pressure_ceiling_fps: config.max_fps,
            pressure_streak: 0,
            healthy_streak: 0,
            bitrate_bps: config.target_bitrate_kbps.saturating_mul(1_000),
            last_bitrate_change: now,
            bitrate_supported: true,
            sender_pressure: false,
            sender_ceiling_bps: u32::MAX,
            degraded_streak: 0,
            low_loss_streak: 0,
            receivers: HashMap::new(),
        }
    }

    /// Returns true when fresh activity raises the encode cadence.
    pub fn observe_frame(&mut self, frame: &VideoFrame, now: Instant) -> bool {
        if !self.config.adaptive {
            return false;
        }
        self.activity_ratio = self.sampler.changed_ratio(frame);
        let change_percent = self.activity_ratio * 100.0;
        if change_percent >= self.config.adaptive_large_change_percent {
            self.last_large_activity = now;
        } else if change_percent >= self.config.adaptive_small_change_percent {
            self.last_small_activity = now;
        }
        let old_fps = self.target_fps;
        self.refresh_fps(now);
        self.target_fps > old_fps
    }

    pub fn target_fps(&mut self, now: Instant) -> u32 {
        self.refresh_fps(now);
        self.target_fps
    }

    fn refresh_fps(&mut self, now: Instant) {
        if !self.config.adaptive {
            self.target_fps = self.config.max_fps;
            return;
        }
        let large_active = now.saturating_duration_since(self.last_large_activity)
            < Duration::from_millis(u64::from(self.config.adaptive_idle_after_ms));
        let small_active = now.saturating_duration_since(self.last_small_activity)
            < Duration::from_millis(u64::from(self.config.adaptive_small_hold_ms));
        self.target_fps = if large_active {
            self.pressure_ceiling_fps
        } else if small_active {
            self.config
                .adaptive_typing_fps
                .min(self.pressure_ceiling_fps)
        } else {
            self.config.adaptive_idle_fps.min(self.pressure_ceiling_fps)
        };
    }

    pub fn activity_ratio(&self) -> f32 {
        self.activity_ratio
    }

    pub fn bitrate_bps(&self) -> u32 {
        self.bitrate_bps
    }

    pub fn observe_sender(&mut self, overloaded: bool, ceiling_bps: u32) {
        self.sender_ceiling_bps =
            ceiling_bps.max(self.config.min_bitrate_kbps.saturating_mul(1_000));
        self.sender_pressure = overloaded || self.bitrate_bps > self.sender_ceiling_bps;
    }

    pub fn refinement_allowed(&self) -> bool {
        !self.sender_pressure && self.pressure_streak == 0 && self.degraded_streak == 0
    }

    pub fn register_receiver(&mut self, id: u64) {
        self.receivers.insert(id, ReceiverFeedback::default());
    }

    pub fn observe_receiver_report(&mut self, id: u64, received: u64, missing: u64) {
        let Some(feedback) = self.receivers.get_mut(&id) else {
            return;
        };
        if let Some((previous_received, previous_missing)) = feedback.last_counters {
            if received >= previous_received && missing >= previous_missing {
                feedback.interval_received = feedback
                    .interval_received
                    .saturating_add(received - previous_received);
                feedback.interval_missing = feedback
                    .interval_missing
                    .saturating_add(missing - previous_missing);
            } else {
                // A restarted receiver needs a fresh baseline, not a negative delta.
                feedback.interval_received = 0;
                feedback.interval_missing = 0;
            }
        }
        feedback.last_counters = Some((received, missing));
    }

    pub fn observe_receiver_media_report(&mut self, report: ReceiverMediaReport) {
        self.observe_receiver_report(report.id, report.received_packets, report.missing_packets);
        let Some(feedback) = self.receivers.get_mut(&report.id) else {
            return;
        };
        if let Some((previous_decoded, previous_at)) = feedback.last_decode_counter {
            if report.decoded_frames >= previous_decoded {
                feedback.interval_decoded = feedback
                    .interval_decoded
                    .saturating_add(report.decoded_frames - previous_decoded);
                feedback.interval_media_duration +=
                    report.received_at.saturating_duration_since(previous_at);
            } else {
                feedback.interval_decoded = 0;
                feedback.interval_media_duration = Duration::ZERO;
            }
        }
        feedback.last_decode_counter = Some((report.decoded_frames, report.received_at));
        if let Some((previous_rendered, previous_at)) = feedback.last_render_counter {
            if report.rendered_frames >= previous_rendered {
                feedback.interval_rendered = feedback
                    .interval_rendered
                    .saturating_add(report.rendered_frames - previous_rendered);
                feedback.interval_render_duration +=
                    report.received_at.saturating_duration_since(previous_at);
            } else {
                feedback.interval_rendered = 0;
                feedback.interval_render_duration = Duration::ZERO;
            }
        }
        feedback.last_render_counter = Some((report.rendered_frames, report.received_at));
        feedback.rendering_enabled = report.rendering_enabled;
        feedback.interval_render_latency_p95_us = feedback
            .interval_render_latency_p95_us
            .max(report.render_latency_p95_us);
    }

    pub fn remove_receiver(&mut self, id: u64) {
        self.receivers.remove(&id);
    }

    #[allow(clippy::cast_precision_loss)] // One-second packet counters are far below 2^52.
    fn take_receiver_interval(&mut self, expected_fps: u32) -> ReceiverInterval {
        let mut interval_received = 0_u64;
        let mut interval_missing = 0_u64;
        let mut receiver_reporting_students = 0_usize;
        let mut receiver_high_loss_students = 0_usize;
        let mut receiver_slow_students = 0_usize;
        let mut receiver_render_slow_students = 0_usize;
        let mut receiver_degraded_students = 0_usize;
        let mut receiver_healthy_students = 0_usize;
        let threshold = f64::from(self.config.adaptive_receiver_loss_percent);
        let minimum_receiver_fps = f64::from(expected_fps)
            * f64::from(self.config.adaptive_receiver_min_fps_percent)
            / 100.0;
        for (id, receiver) in &mut self.receivers {
            if receiver.interval_received > 0 {
                receiver_reporting_students += 1;
                interval_received = interval_received.saturating_add(receiver.interval_received);
                interval_missing = interval_missing.saturating_add(receiver.interval_missing);
                let loss =
                    100.0 * receiver.interval_missing as f64 / receiver.interval_received as f64;
                let decode_fps = (!receiver.interval_media_duration.is_zero()).then(|| {
                    receiver.interval_decoded as f64
                        / receiver.interval_media_duration.as_secs_f64()
                });
                let render_fps = (!receiver.interval_render_duration.is_zero()).then(|| {
                    receiver.interval_rendered as f64
                        / receiver.interval_render_duration.as_secs_f64()
                });
                tracing::info!(target: "telemetry", client_id = *id,
                    decode_fps = ?decode_fps, render_fps = ?render_fps,
                    received_packets = receiver.interval_received,
                    missing_packets = receiver.interval_missing,
                    decode_report_window_ms = receiver.interval_media_duration.as_millis(),
                    render_report_window_ms = receiver.interval_render_duration.as_millis(),
                    render_latency_p95_us = receiver.interval_render_latency_p95_us,
                    "teacher receiver cadence interval");
                let high_loss = loss >= threshold;
                let slow_decode = decode_fps.is_some_and(|fps| fps < minimum_receiver_fps);
                let slow_render = receiver.rendering_enabled
                    && receiver.interval_render_latency_p95_us
                        <= u64::from(self.config.adaptive_receiver_render_latency_max_ms) * 1_000
                    && render_fps.is_some_and(|fps| fps < minimum_receiver_fps);
                // Render cadence stays diagnostic. Lower bitrate cannot fix GPU/GDI
                // presentation scheduling when decode keeps up with the source.
                let slow = slow_decode;
                if high_loss {
                    receiver_high_loss_students += 1;
                }
                if slow {
                    receiver_slow_students += 1;
                }
                if slow_render {
                    receiver_render_slow_students += 1;
                }
                if high_loss || slow {
                    receiver_degraded_students += 1;
                } else if loss <= threshold / 4.0 {
                    receiver_healthy_students += 1;
                }
            }
            receiver.interval_received = 0;
            receiver.interval_missing = 0;
            receiver.interval_decoded = 0;
            receiver.interval_media_duration = Duration::ZERO;
            receiver.interval_rendered = 0;
            receiver.interval_render_duration = Duration::ZERO;
            receiver.interval_render_latency_p95_us = 0;
        }
        let receiver_loss_percent = (interval_received > 0)
            .then(|| 100.0 * interval_missing as f64 / interval_received as f64);
        let active_receivers = self.receivers.len();
        let receiver_feedback_eligible = active_receivers > 0
            && receiver_reporting_students
                >= reporter_quorum(
                    active_receivers,
                    self.config.adaptive_feedback_min_reporting_percent,
                );
        let receiver_recovery_eligible = receiver_feedback_eligible
            && receiver_reporting_students
                >= reporter_quorum(
                    active_receivers,
                    self.config.adaptive_recovery_min_reporting_percent,
                );
        ReceiverInterval {
            loss_percent: receiver_loss_percent,
            reporting_students: receiver_reporting_students,
            high_loss_students: receiver_high_loss_students,
            slow_students: receiver_slow_students,
            render_slow_students: receiver_render_slow_students,
            degraded_students: receiver_degraded_students,
            healthy_students: receiver_healthy_students,
            feedback_eligible: receiver_feedback_eligible,
            recovery_eligible: receiver_recovery_eligible,
        }
    }

    pub fn evaluate_interval(
        &mut self,
        encode_p95: Duration,
        cpu_capacity_percent: f32,
        now: Instant,
    ) -> QualityUpdate {
        self.refresh_fps(now);
        let interval = self.take_receiver_interval(self.target_fps);
        if !self.config.adaptive {
            return QualityUpdate {
                encode_pressure: false,
                cpu_pressure: false,
                pressure_ceiling_fps: self.config.max_fps,
                bitrate_reason: None,
                target_fps: self.config.max_fps,
                bitrate_change_bps: None,
                receiver_loss_percent: interval.loss_percent,
                receiver_reporting_students: interval.reporting_students,
                receiver_high_loss_students: interval.high_loss_students,
                receiver_slow_students: interval.slow_students,
                receiver_render_slow_students: interval.render_slow_students,
                receiver_feedback_eligible: interval.feedback_eligible,
                receiver_recovery_eligible: interval.recovery_eligible,
            };
        }

        // CPU input is host-capacity percent, not sysinfo's summed core usage.
        // A saturated serial encoder is still detected independently by latency.
        let encode_pressure =
            encode_p95 >= Duration::from_millis(u64::from(self.config.adaptive_pressure_encode_ms));
        let cpu_pressure = cpu_capacity_percent >= self.config.adaptive_pressure_cpu_percent;
        let pressure = encode_pressure || cpu_pressure || self.sender_pressure;
        let minimum_bitrate = self.config.min_bitrate_kbps.saturating_mul(1_000);
        if pressure {
            self.pressure_streak = self.pressure_streak.saturating_add(1);
            self.healthy_streak = 0;
            if self.pressure_streak >= 3
                && (!self.bitrate_supported || self.bitrate_bps <= minimum_bitrate)
            {
                self.pressure_ceiling_fps = next_lower_fps(self.pressure_ceiling_fps);
                self.pressure_streak = 0;
            }
        } else {
            self.healthy_streak = self.healthy_streak.saturating_add(1);
            self.pressure_streak = 0;
            if self.healthy_streak >= 5 {
                self.pressure_ceiling_fps =
                    next_higher_fps(self.pressure_ceiling_fps, self.config.max_fps);
                self.healthy_streak = 0;
            }
        }

        self.update_receiver_streaks(interval);
        if pressure {
            // Healthy receivers must not raise bitrate during local pressure.
            self.low_loss_streak = 0;
        }
        let (bitrate_change_bps, mut bitrate_reason) = self.propose_bitrate(
            now,
            interval.feedback_eligible,
            pressure && self.pressure_streak >= 3,
        );
        if self.sender_pressure && bitrate_reason == Some("local_pressure") {
            bitrate_reason = Some("sender_pressure");
        }
        self.refresh_fps(now);
        QualityUpdate {
            encode_pressure,
            cpu_pressure,
            pressure_ceiling_fps: self.pressure_ceiling_fps,
            bitrate_reason,
            target_fps: self.target_fps,
            bitrate_change_bps,
            receiver_loss_percent: interval.loss_percent,
            receiver_reporting_students: interval.reporting_students,
            receiver_high_loss_students: interval.high_loss_students,
            receiver_slow_students: interval.slow_students,
            receiver_render_slow_students: interval.render_slow_students,
            receiver_feedback_eligible: interval.feedback_eligible,
            receiver_recovery_eligible: interval.recovery_eligible,
        }
    }

    fn update_receiver_streaks(&mut self, interval: ReceiverInterval) {
        if !interval.feedback_eligible {
            // Report gaps are not evidence of sustained loss or sustained health.
            self.degraded_streak = 0;
            self.low_loss_streak = 0;
            return;
        }
        let required_high_loss_students = if self.receivers.len() == 1 {
            1
        } else {
            reporter_quorum(
                self.receivers.len(),
                self.config.adaptive_high_loss_quorum_percent,
            )
            .max(2)
        };
        if interval.degraded_students >= required_high_loss_students {
            self.degraded_streak = self.degraded_streak.saturating_add(1);
            self.low_loss_streak = 0;
        } else if interval.recovery_eligible
            && interval.healthy_students == interval.reporting_students
        {
            self.low_loss_streak = self.low_loss_streak.saturating_add(1);
            self.degraded_streak = 0;
        } else {
            self.degraded_streak = 0;
            self.low_loss_streak = 0;
        }
    }

    fn propose_bitrate(
        &mut self,
        now: Instant,
        feedback_eligible: bool,
        encode_pressure: bool,
    ) -> (Option<u32>, Option<&'static str>) {
        let cooldown = Duration::from_millis(u64::from(self.config.adaptive_bitrate_cooldown_ms));
        if !self.bitrate_supported
            || (!feedback_eligible && !encode_pressure)
            || now.saturating_duration_since(self.last_bitrate_change) < cooldown
        {
            return (None, None);
        }
        let reason;
        let candidate = if encode_pressure || self.degraded_streak >= 3 {
            reason = if encode_pressure {
                "local_pressure"
            } else {
                "receiver_degradation"
            };
            self.degraded_streak = 0;
            (self
                .bitrate_bps
                .saturating_mul(100 - self.config.adaptive_bitrate_step_down_percent)
                / 100)
                .min(self.sender_ceiling_bps)
                .max(self.config.min_bitrate_kbps.saturating_mul(1_000))
        } else if self.low_loss_streak >= 10 {
            reason = "receiver_recovery";
            self.low_loss_streak = 0;
            (self
                .bitrate_bps
                .saturating_mul(100 + self.config.adaptive_bitrate_step_up_percent)
                / 100)
                .min(self.config.target_bitrate_kbps.saturating_mul(1_000))
                .min(self.sender_ceiling_bps)
        } else {
            return (None, None);
        };
        if candidate == self.bitrate_bps {
            (None, None)
        } else {
            (Some(candidate), Some(reason))
        }
    }

    pub fn bitrate_applied(&mut self, bitrate_bps: u32, now: Instant) {
        self.bitrate_bps = bitrate_bps;
        self.last_bitrate_change = now;
        self.pressure_streak = 0;
    }

    /// Applies a policy proposal to the one live encoder. A backend rejection
    /// leaves the tracked bitrate unchanged and stops further proposals.
    pub fn apply_bitrate<E: VideoEncoder>(
        &mut self,
        encoder: &mut E,
        proposed_bitrate_bps: Option<u32>,
        now: Instant,
    ) -> Result<Option<u32>, MediaError> {
        let Some(bitrate_bps) = proposed_bitrate_bps else {
            return Ok(None);
        };
        if let Err(error) = encoder.set_bitrate(bitrate_bps) {
            self.disable_bitrate_adaptation();
            return Err(error);
        }
        self.bitrate_applied(bitrate_bps, now);
        Ok(Some(bitrate_bps))
    }

    pub fn disable_bitrate_adaptation(&mut self) {
        self.bitrate_supported = false;
    }
}

fn next_lower_fps(current: u32) -> u32 {
    // 48/30 only matter for enhanced 60 FPS one-to-one sessions; a 30 FPS
    // broadcast still steps 30 → 24 → 20 → 15.
    [48, 30, 24, 20, 15]
        .into_iter()
        .find(|&candidate| candidate < current)
        .unwrap_or(current)
}

fn reporter_quorum(active_receivers: usize, percent: u32) -> usize {
    active_receivers
        .saturating_mul(percent as usize)
        .div_ceil(100)
}

fn next_higher_fps(current: u32, max_fps: u32) -> u32 {
    [20, 24, 30, 48, max_fps]
        .into_iter()
        .find(|&candidate| candidate > current && candidate <= max_fps)
        .unwrap_or(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use classroom_config::Config;
    use media_core::{EncodedAccessUnit, VideoFrameRef};

    #[derive(Default)]
    struct RecordingEncoder {
        bitrates: Vec<u32>,
        reject: bool,
    }

    impl VideoEncoder for RecordingEncoder {
        fn encode(
            &mut self,
            _frame: VideoFrameRef<'_>,
        ) -> Result<Option<EncodedAccessUnit>, MediaError> {
            Ok(None)
        }

        fn set_bitrate(&mut self, bits_per_second: u32) -> Result<(), MediaError> {
            if self.reject {
                return Err(MediaError::Operation("bitrate rejected".to_owned()));
            }
            self.bitrates.push(bits_per_second);
            Ok(())
        }

        fn request_keyframe(&mut self) -> Result<(), MediaError> {
            Ok(())
        }
    }

    fn config() -> VideoConfig {
        Config::load(None).unwrap().0.video
    }

    #[allow(clippy::too_many_arguments)]
    fn observe_media(
        policy: &mut QualityController,
        received_packets: u64,
        missing_packets: u64,
        decoded_frames: u64,
        rendered_frames: u64,
        rendering_enabled: bool,
        render_latency_p95_us: u64,
        received_at: Instant,
    ) {
        policy.observe_receiver_media_report(ReceiverMediaReport {
            id: 1,
            received_packets,
            missing_packets,
            decoded_frames,
            rendered_frames,
            rendering_enabled,
            render_latency_p95_us,
            received_at,
        });
    }

    fn frame(value: u8) -> VideoFrame {
        VideoFrame {
            bytes: vec![value; 16 * 16 * 4],
            captured_at: Instant::now(),
            width: 16,
            height: 16,
            row_pitch: 64,
            dirty_region_ratio: 1.0,
            accumulated_frames: 1,
            pointer_visible: false,
            pointer_composited: false,
            protected_content_masked_out: false,
            capture_latency: Duration::ZERO,
        }
    }

    #[test]
    fn activity_sampler_reuses_both_sample_buffers() {
        let mut sampler = ActivitySampler::default();
        assert!(sampler.changed_ratio(&frame(1)) > 0.999);
        assert!(sampler.changed_ratio(&frame(1)) < 0.001);
        let capacities = (sampler.previous.capacity(), sampler.scratch.capacity());
        assert!(sampler.changed_ratio(&frame(2)) > 0.999);
        assert!(sampler.changed_ratio(&frame(2)) < 0.001);
        assert_eq!(
            (sampler.previous.capacity(), sampler.scratch.capacity()),
            capacities
        );
    }

    #[test]
    fn static_screen_idles_and_first_change_wakes_immediately_even_with_full_dirty_metadata() {
        let start = Instant::now();
        let mut policy = QualityController::new(&config(), start);
        assert!(!policy.observe_frame(&frame(1), start));
        assert_eq!(policy.target_fps(start + Duration::from_millis(1_300)), 5);
        assert!(policy.observe_frame(&frame(2), start + Duration::from_millis(1_301)));
        assert_eq!(policy.target_fps(start + Duration::from_millis(1_301)), 30);
    }

    #[test]
    fn many_core_teacher_does_not_downshift_on_one_core_of_work() {
        let start = Instant::now();
        let mut policy = QualityController::new(&config(), start);
        for second in 1..=20 {
            let now = start + Duration::from_secs(second);
            policy.observe_frame(&frame(u8::try_from(second).unwrap()), now);
            let update = policy.evaluate_interval(
                Duration::from_millis(18),
                crate::process_metrics::cpu_capacity_percent(155.0, 28),
                now,
            );
            assert_eq!(update.target_fps, 30);
            assert!(!update.cpu_pressure);
            assert!(!update.encode_pressure);
            assert!(update.bitrate_change_bps.is_none());
        }
    }

    #[test]
    fn saturated_small_host_and_slow_encoder_report_independent_pressure() {
        for (encode_ms, core_cpu, cpus, expected_cpu, expected_encode) in [
            (18, 90.0, 1, true, false),
            (18, 180.0, 2, true, false),
            (29, 88.0, 28, false, true),
        ] {
            let start = Instant::now();
            let mut policy = QualityController::new(&config(), start);
            for second in 5..=7 {
                let now = start + Duration::from_secs(second);
                let update = policy.evaluate_interval(
                    Duration::from_millis(encode_ms),
                    crate::process_metrics::cpu_capacity_percent(core_cpu, cpus),
                    now,
                );
                assert_eq!(update.cpu_pressure, expected_cpu);
                assert_eq!(update.encode_pressure, expected_encode);
                if second == 7 {
                    assert_eq!(update.bitrate_change_bps, Some(4_800_000));
                    assert_eq!(update.bitrate_reason, Some("local_pressure"));
                }
            }
        }
        let mut policy = QualityController::new(&config(), Instant::now());
        let start = Instant::now();
        policy.observe_sender(true, 3_000_000);
        assert!(!policy.refinement_allowed());
        for second in 1..=3 {
            let update = policy.evaluate_interval(
                Duration::from_millis(10),
                3.0,
                start + Duration::from_secs(10 + second),
            );
            assert!(!update.cpu_pressure && !update.encode_pressure);
            if second == 3 {
                assert_eq!(update.bitrate_reason, Some("sender_pressure"));
                assert_eq!(update.bitrate_change_bps, Some(3_000_000));
            }
        }
    }

    #[test]
    fn sustained_encode_pressure_reduces_bitrate_before_fps() {
        let start = Instant::now();
        let mut policy = QualityController::new(&config(), start);
        for second in 5..=7 {
            policy.observe_frame(
                &frame(u8::try_from(second).unwrap()),
                start + Duration::from_secs(second),
            );
            let update = policy.evaluate_interval(
                Duration::from_millis(29),
                20.0,
                start + Duration::from_secs(second),
            );
            assert_eq!(update.target_fps, 30);
            assert_eq!(
                update.bitrate_change_bps,
                (second == 7).then_some(4_800_000)
            );
        }
    }

    #[test]
    fn sustained_encode_pressure_reduces_fps_at_minimum_bitrate() {
        let start = Instant::now();
        let mut policy = QualityController::new(&config(), start);
        policy.bitrate_applied(1_500_000, start);
        for second in 1..=3 {
            policy.observe_frame(
                &frame(u8::try_from(second).unwrap()),
                start + Duration::from_secs(second),
            );
            let update = policy.evaluate_interval(
                Duration::from_millis(29),
                20.0,
                start + Duration::from_secs(second),
            );
            assert!(update.bitrate_change_bps.is_none());
        }
        assert_eq!(policy.target_fps(start + Duration::from_secs(3)), 24);
    }

    #[test]
    fn isolated_text_like_change_wakes_typing_tier_without_forcing_full_motion_rate() {
        let start = Instant::now();
        let mut policy = QualityController::new(&config(), start);
        let mut still = frame(1);
        still.width = 100;
        still.height = 100;
        still.row_pitch = 400;
        still.bytes = vec![1; 100 * 100 * 4];
        policy.observe_frame(&still, start);
        assert_eq!(policy.target_fps(start + Duration::from_millis(1_300)), 5);
        still.bytes[0] = 2;
        assert!(policy.observe_frame(&still, start + Duration::from_millis(1_301)));
        assert_eq!(policy.target_fps(start + Duration::from_millis(1_301)), 24);
        assert_eq!(policy.target_fps(start + Duration::from_millis(2_400)), 24);
        assert_eq!(policy.target_fps(start + Duration::from_millis(2_502)), 5);
    }

    #[test]
    fn repeated_typing_changes_keep_typing_cadence_between_sparse_samples() {
        let start = Instant::now();
        let mut policy = QualityController::new(&config(), start);
        let mut still = frame(1);
        still.width = 100;
        still.height = 100;
        still.row_pitch = 400;
        still.bytes = vec![1; 100 * 100 * 4];
        policy.observe_frame(&still, start);
        assert_eq!(policy.target_fps(start + Duration::from_millis(1_300)), 5);

        for (index, millis) in [1_301_u64, 2_200, 3_100].into_iter().enumerate() {
            // One sampled pixel changes, matching a small text/cursor update.
            still.bytes[0] = u8::try_from(index + 2).unwrap();
            policy.observe_frame(&still, start + Duration::from_millis(millis));
            assert_eq!(
                policy.target_fps(start + Duration::from_millis(millis + 900)),
                24,
                "typing tier should survive a normal pause after sample {index}"
            );
        }
        assert_eq!(policy.target_fps(start + Duration::from_millis(4_301)), 5);
    }

    #[test]
    fn disabled_adaptation_keeps_nominal_fps() {
        let start = Instant::now();
        let mut config = config();
        config.adaptive = false;
        let mut policy = QualityController::new(&config, start);
        policy.observe_frame(&frame(1), start);
        assert_eq!(policy.target_fps(start + Duration::from_secs(5)), 30);
    }

    #[test]
    fn sustained_receiver_loss_proposes_bounded_bitrate_change() {
        let start = Instant::now();
        let mut policy = QualityController::new(&config(), start);
        policy.register_receiver(1);
        policy.observe_receiver_report(1, 0, 0);
        for second in 1..=3 {
            policy.observe_receiver_report(1, second * 100, second * 5);
            let update = policy.evaluate_interval(
                Duration::from_millis(10),
                20.0,
                start + Duration::from_secs(10 + second),
            );
            if second < 3 {
                assert!(update.bitrate_change_bps.is_none());
            } else {
                assert_eq!(update.bitrate_change_bps, Some(4_800_000));
            }
        }
        policy.bitrate_applied(4_800_000, start + Duration::from_secs(13));
        assert_eq!(policy.bitrate_bps(), 4_800_000);
    }

    #[test]
    fn sustained_slow_render_with_fast_renderer_preserves_bitrate() {
        let start = Instant::now();
        let mut policy = QualityController::new(&config(), start);
        policy.register_receiver(1);
        observe_media(
            &mut policy,
            0,
            0,
            0,
            0,
            true,
            1_000,
            start + Duration::from_secs(5),
        );
        for second in 6..=8 {
            policy.observe_frame(
                &frame(u8::try_from(second).unwrap()),
                start + Duration::from_secs(second),
            );
            let elapsed = second - 5;
            observe_media(
                &mut policy,
                elapsed * 100,
                0,
                elapsed * 30,
                elapsed * 15,
                true,
                1_000,
                start + Duration::from_secs(second),
            );
            let update = policy.evaluate_interval(
                Duration::from_millis(10),
                20.0,
                start + Duration::from_secs(second),
            );
            assert_eq!(update.target_fps, 30);
            assert_eq!(update.receiver_slow_students, 0);
            assert_eq!(update.receiver_render_slow_students, 1);
            assert!(update.bitrate_change_bps.is_none());
        }
    }

    #[test]
    fn slow_render_with_slow_renderer_does_not_reduce_bitrate() {
        let start = Instant::now();
        let mut policy = QualityController::new(&config(), start);
        policy.register_receiver(1);
        observe_media(
            &mut policy,
            0,
            0,
            0,
            0,
            true,
            20_000,
            start + Duration::from_secs(5),
        );
        for second in 6..=10 {
            let elapsed = second - 5;
            observe_media(
                &mut policy,
                elapsed * 100,
                0,
                elapsed * 30,
                elapsed * 15,
                true,
                20_000,
                start + Duration::from_secs(second),
            );
            let update = policy.evaluate_interval(
                Duration::from_millis(10),
                20.0,
                start + Duration::from_secs(second),
            );
            assert_eq!(update.receiver_render_slow_students, 0);
            assert!(update.bitrate_change_bps.is_none());
        }
    }

    #[test]
    fn sustained_slow_decode_receiver_reduces_bitrate_while_preserving_fps() {
        let start = Instant::now();
        let mut policy = QualityController::new(&config(), start);
        policy.register_receiver(1);
        observe_media(
            &mut policy,
            0,
            0,
            0,
            0,
            false,
            0,
            start + Duration::from_secs(5),
        );
        for second in 6..=8 {
            policy.observe_frame(
                &frame(u8::try_from(second).unwrap()),
                start + Duration::from_secs(second),
            );
            let elapsed = second - 5;
            observe_media(
                &mut policy,
                elapsed * 100,
                0,
                elapsed * 15,
                elapsed * 15,
                false,
                0,
                start + Duration::from_secs(second),
            );
            let update = policy.evaluate_interval(
                Duration::from_millis(10),
                20.0,
                start + Duration::from_secs(second),
            );
            assert_eq!(update.target_fps, 30);
            assert_eq!(update.receiver_slow_students, 1);
            assert_eq!(
                update.bitrate_change_bps,
                (second == 8).then_some(4_800_000)
            );
        }
    }

    #[test]
    fn headless_receiver_is_not_slow_when_decode_cadence_is_healthy() {
        let start = Instant::now();
        let mut policy = QualityController::new(&config(), start);
        policy.register_receiver(1);
        observe_media(
            &mut policy,
            0,
            0,
            0,
            0,
            false,
            0,
            start + Duration::from_secs(5),
        );
        policy.observe_frame(&frame(1), start + Duration::from_secs(6));
        observe_media(
            &mut policy,
            100,
            0,
            30,
            0,
            false,
            0,
            start + Duration::from_secs(6),
        );
        let update = policy.evaluate_interval(
            Duration::from_millis(10),
            20.0,
            start + Duration::from_secs(6),
        );
        assert_eq!(update.receiver_slow_students, 0);
    }

    #[test]
    fn fps_recovery_never_exceeds_a_lower_configured_cap() {
        assert_eq!(next_lower_fps(18), 15);
        assert_eq!(next_higher_fps(15, 18), 18);
        assert_eq!(next_higher_fps(18, 18), 18);
    }

    #[test]
    fn missing_receiver_report_breaks_a_loss_streak() {
        let start = Instant::now();
        let mut policy = QualityController::new(&config(), start);
        policy.register_receiver(1);
        policy.observe_receiver_report(1, 0, 0);
        for second in 1..=2 {
            policy.observe_receiver_report(1, second * 100, second * 5);
            assert!(policy
                .evaluate_interval(
                    Duration::from_millis(10),
                    20.0,
                    start + Duration::from_secs(10 + second),
                )
                .bitrate_change_bps
                .is_none());
        }
        policy.evaluate_interval(
            Duration::from_millis(10),
            20.0,
            start + Duration::from_secs(13),
        );
        policy.observe_receiver_report(1, 300, 15);
        assert!(policy
            .evaluate_interval(
                Duration::from_millis(10),
                20.0,
                start + Duration::from_secs(14),
            )
            .bitrate_change_bps
            .is_none());
    }

    #[test]
    fn receiver_feedback_applies_one_bitrate_change_to_the_live_encoder() {
        let start = Instant::now();
        let mut policy = QualityController::new(&config(), start);
        let mut encoder = RecordingEncoder::default();
        policy.register_receiver(1);
        policy.observe_receiver_report(1, 0, 0);
        for second in 1..=3 {
            policy.observe_receiver_report(1, second * 100, second * 5);
            let now = start + Duration::from_secs(10 + second);
            let update = policy.evaluate_interval(Duration::from_millis(10), 20.0, now);
            let applied = policy
                .apply_bitrate(&mut encoder, update.bitrate_change_bps, now)
                .unwrap();
            assert_eq!(applied, (second == 3).then_some(4_800_000));
            assert_eq!(
                update.bitrate_reason,
                (second == 3).then_some("receiver_degradation")
            );
        }
        assert_eq!(encoder.bitrates, [4_800_000]);
        assert_eq!(policy.bitrate_bps(), 4_800_000);
    }

    #[test]
    fn healthy_receiver_reports_raise_bitrate_after_cooldown_without_overshoot() {
        let start = Instant::now();
        let mut policy = QualityController::new(&config(), start);
        let mut encoder = RecordingEncoder::default();
        policy.register_receiver(1);
        policy.observe_receiver_report(1, 0, 0);
        for second in 1..=3 {
            policy.observe_receiver_report(1, second * 100, second * 5);
            let now = start + Duration::from_secs(10 + second);
            let update = policy.evaluate_interval(Duration::from_millis(10), 20.0, now);
            policy
                .apply_bitrate(&mut encoder, update.bitrate_change_bps, now)
                .unwrap();
        }
        assert_eq!(policy.bitrate_bps(), 4_800_000);
        for second in 4..=13 {
            policy.observe_receiver_report(1, second * 100, 15);
            let now = start + Duration::from_secs(10 + second);
            let update = policy.evaluate_interval(Duration::from_millis(10), 20.0, now);
            let applied = policy
                .apply_bitrate(&mut encoder, update.bitrate_change_bps, now)
                .unwrap();
            assert_eq!(applied, (second == 13).then_some(5_280_000));
            assert_eq!(
                update.bitrate_reason,
                (second == 13).then_some("receiver_recovery")
            );
        }
        assert_eq!(encoder.bitrates, [4_800_000, 5_280_000]);
        assert_eq!(policy.bitrate_bps(), 5_280_000);
    }

    #[test]
    fn encoder_rejection_preserves_bitrate_and_disables_further_proposals() {
        let start = Instant::now();
        let mut policy = QualityController::new(&config(), start);
        let mut encoder = RecordingEncoder {
            reject: true,
            ..RecordingEncoder::default()
        };
        policy.register_receiver(1);
        policy.observe_receiver_report(1, 0, 0);
        for second in 1..=3 {
            policy.observe_receiver_report(1, second * 100, second * 5);
            let now = start + Duration::from_secs(10 + second);
            let update = policy.evaluate_interval(Duration::from_millis(10), 20.0, now);
            let result = policy.apply_bitrate(&mut encoder, update.bitrate_change_bps, now);
            if second == 3 {
                assert!(result.is_err());
            } else {
                assert!(result.unwrap().is_none());
            }
        }
        assert_eq!(policy.bitrate_bps(), 6_000_000);
        assert!(encoder.bitrates.is_empty());
        for second in 4..=6 {
            policy.observe_receiver_report(1, second * 100, second * 5);
            assert!(policy
                .evaluate_interval(
                    Duration::from_millis(10),
                    20.0,
                    start + Duration::from_secs(10 + second),
                )
                .bitrate_change_bps
                .is_none());
        }
    }

    #[test]
    fn five_receivers_need_more_than_one_noisy_reporter() {
        let start = Instant::now();
        let mut policy = QualityController::new(&config(), start);
        for id in 1..=5 {
            policy.register_receiver(id);
            policy.observe_receiver_report(id, 0, 0);
        }
        for second in 1..=3 {
            for id in 1..=5 {
                policy.observe_receiver_report(id, second * 100, u64::from(id == 1) * second * 5);
            }
            let update = policy.evaluate_interval(
                Duration::from_millis(10),
                20.0,
                start + Duration::from_secs(10 + second),
            );
            assert_eq!(update.receiver_reporting_students, 5);
            assert_eq!(update.receiver_high_loss_students, 1);
            assert!(update.receiver_feedback_eligible);
            assert!(update.bitrate_change_bps.is_none());
        }
        for second in 4..=6 {
            for id in 1..=5 {
                let missing = if id <= 2 { second * 5 } else { 0 };
                policy.observe_receiver_report(id, second * 100, missing);
            }
            let update = policy.evaluate_interval(
                Duration::from_millis(10),
                20.0,
                start + Duration::from_secs(10 + second),
            );
            assert_eq!(update.receiver_high_loss_students, 2);
            assert_eq!(
                update.bitrate_change_bps,
                (second == 6).then_some(4_800_000)
            );
        }
    }

    #[test]
    fn forty_receivers_ignore_a_single_noisy_or_stale_reporter() {
        let start = Instant::now();
        let mut policy = QualityController::new(&config(), start);
        for id in 1..=40 {
            policy.register_receiver(id);
            policy.observe_receiver_report(id, 0, 0);
        }
        for second in 1..=3 {
            for id in 1..=40 {
                policy.observe_receiver_report(id, second * 100, u64::from(id == 1) * second * 5);
            }
            let update = policy.evaluate_interval(
                Duration::from_millis(10),
                20.0,
                start + Duration::from_secs(10 + second),
            );
            assert_eq!(update.receiver_reporting_students, 40);
            assert_eq!(update.receiver_high_loss_students, 1);
            assert!(update.bitrate_change_bps.is_none());
        }
        for second in 4..=6 {
            policy.observe_receiver_report(1, second * 100, second * 5);
            let update = policy.evaluate_interval(
                Duration::from_millis(10),
                20.0,
                start + Duration::from_secs(10 + second),
            );
            assert_eq!(update.receiver_reporting_students, 1);
            assert!(!update.receiver_feedback_eligible);
            assert!(update.bitrate_change_bps.is_none());
        }
        for second in 7..=9 {
            for id in 1..=40 {
                let missing = if id <= 10 { second * 5 } else { 0 };
                policy.observe_receiver_report(id, second * 100, missing);
            }
            let update = policy.evaluate_interval(
                Duration::from_millis(10),
                20.0,
                start + Duration::from_secs(10 + second),
            );
            assert_eq!(update.receiver_high_loss_students, 10);
            assert_eq!(
                update.bitrate_change_bps,
                (second == 9).then_some(4_800_000)
            );
        }
    }

    #[test]
    fn forty_receivers_need_broad_healthy_coverage_to_recover_bitrate() {
        let start = Instant::now();
        let mut policy = QualityController::new(&config(), start);
        for id in 1..=40 {
            policy.register_receiver(id);
            policy.observe_receiver_report(id, 0, 0);
        }
        policy.bitrate_applied(4_800_000, start);
        for second in 1..=10 {
            for id in 1..=20 {
                policy.observe_receiver_report(id, second * 100, 0);
            }
            let update = policy.evaluate_interval(
                Duration::from_millis(10),
                20.0,
                start + Duration::from_secs(second),
            );
            assert!(update.receiver_feedback_eligible);
            assert!(!update.receiver_recovery_eligible);
            assert!(update.bitrate_change_bps.is_none());
        }
        for second in 11..=20 {
            for id in 1..=30 {
                policy.observe_receiver_report(id, second * 100, 0);
            }
            let update = policy.evaluate_interval(
                Duration::from_millis(10),
                20.0,
                start + Duration::from_secs(second),
            );
            assert!(update.receiver_recovery_eligible);
            assert_eq!(
                update.bitrate_change_bps,
                (second == 20).then_some(5_280_000)
            );
        }
    }

    #[test]
    fn disconnect_removes_feedback_and_reconnect_starts_a_fresh_baseline() {
        let start = Instant::now();
        let mut policy = QualityController::new(&config(), start);
        policy.register_receiver(1);
        policy.observe_receiver_report(1, 0, 0);
        policy.observe_receiver_report(1, 100, 5);
        policy.remove_receiver(1);
        policy.observe_receiver_report(1, 200, 100);
        let empty = policy.evaluate_interval(Duration::ZERO, 0.0, start + Duration::from_secs(11));
        assert_eq!(empty.receiver_reporting_students, 0);
        assert!(empty.receiver_loss_percent.is_none());
        policy.register_receiver(1);
        policy.observe_receiver_report(1, 0, 0);
        policy.observe_receiver_report(1, 100, 0);
        let fresh = policy.evaluate_interval(Duration::ZERO, 0.0, start + Duration::from_secs(12));
        assert_eq!(fresh.receiver_reporting_students, 1);
        assert_eq!(fresh.receiver_high_loss_students, 0);
        assert!(fresh.receiver_feedback_eligible);
    }
}
