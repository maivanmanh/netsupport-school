//! O2: static-content refinement ("sharpen after settle").
//!
//! While content moves, the adaptive policy keeps the normal bitrate. When it
//! settles (target fps falls to the idle cadence), a few repeat frames are
//! encoded at a raised bitrate so the encoder spends bits on the residual of
//! the already-sent picture — text gets crisp — and the bitrate is restored.
//! No extra keyframe and no extra encoder: same single encode, same stream.
use std::time::Instant;

/// Encoded frames at the raised bitrate per settle.
pub const REFINE_FRAMES: u32 = 3;
/// Encode cadence while refining (frames per second).
pub const REFINE_FPS: u32 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Raise to this bitrate (bps) and cap QP (`set_refinement(true)`).
    Begin(u32),
    /// Restore this bitrate (bps) and normal QP (`set_refinement(false)`).
    End(u32),
}

#[derive(Debug, Default, Clone, Copy)]
pub struct RefinementStats {
    pub refinements: u64,
    pub frames: u64,
    pub bytes: u64,
    pub cancelled: u64,
}

#[derive(Debug)]
enum State {
    Idle,
    Active,
    Refining { frames_left: u32, base_bps: u32 },
}

#[derive(Debug)]
pub struct Refinement {
    percent: u32,
    max_bps: u32,
    state: State,
    started: Option<Instant>,
    pub stats: RefinementStats,
    pub last_duration_ms: f64,
}

impl Refinement {
    pub fn new(percent: u32, max_bps: u32) -> Self {
        Self {
            percent,
            max_bps,
            state: State::Idle,
            started: None,
            stats: RefinementStats::default(),
            last_duration_ms: 0.0,
        }
    }

    pub const fn refining(&self) -> bool {
        matches!(self.state, State::Refining { .. })
    }

    /// Called once per media tick with the policy's idle/active view.
    pub fn tick(&mut self, idle: bool, base_bps: u32, now: Instant) -> Option<Action> {
        if self.percent <= 100 {
            return None;
        }
        match (&self.state, idle) {
            (State::Idle, false) => {
                self.state = State::Active;
                None
            }
            (State::Active, true) => {
                let raised = u32::try_from(u64::from(base_bps) * u64::from(self.percent) / 100)
                    .unwrap_or(u32::MAX)
                    .min(self.max_bps.max(base_bps));
                if raised <= base_bps {
                    self.state = State::Idle;
                    return None;
                }
                self.state = State::Refining {
                    frames_left: REFINE_FRAMES,
                    base_bps,
                };
                self.started = Some(now);
                self.stats.refinements += 1;
                Some(Action::Begin(raised))
            }
            (State::Refining { base_bps, .. }, false) => {
                // Motion resumed: back to the normal bitrate immediately.
                let restore = *base_bps;
                self.state = State::Active;
                self.stats.cancelled += 1;
                Some(Action::End(restore))
            }
            _ => None,
        }
    }

    /// Called after each encoded access unit.
    pub fn encoded(&mut self, bytes: usize, now: Instant) -> Option<Action> {
        let State::Refining {
            frames_left,
            base_bps,
        } = &mut self.state
        else {
            return None;
        };
        self.stats.frames += 1;
        self.stats.bytes += bytes as u64;
        *frames_left = frames_left.saturating_sub(1);
        if *frames_left > 0 {
            return None;
        }
        let restore = *base_bps;
        self.state = State::Idle;
        if let Some(started) = self.started.take() {
            self.last_duration_ms = now.saturating_duration_since(started).as_secs_f64() * 1000.0;
        }
        Some(Action::End(restore))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settle_raises_then_restores_after_refine_frames() {
        let now = Instant::now();
        let mut r = Refinement::new(250, 20_000_000);
        assert_eq!(r.tick(true, 6_000_000, now), None, "never active yet");
        assert_eq!(r.tick(false, 6_000_000, now), None);
        assert_eq!(
            r.tick(true, 6_000_000, now),
            Some(Action::Begin(15_000_000))
        );
        assert!(r.refining());
        assert_eq!(r.tick(true, 6_000_000, now), None);
        assert_eq!(r.encoded(40_000, now), None);
        assert_eq!(r.encoded(30_000, now), None);
        assert_eq!(r.encoded(20_000, now), Some(Action::End(6_000_000)));
        assert_eq!(
            (r.stats.refinements, r.stats.frames, r.stats.bytes),
            (1, 3, 90_000)
        );
        // Still idle: no second refinement until content moves again.
        assert_eq!(r.tick(true, 6_000_000, now), None);
    }
    #[test]
    fn motion_cancels_refinement_and_disabled_or_capped_does_nothing() {
        let now = Instant::now();
        let mut r = Refinement::new(250, 8_000_000);
        r.tick(false, 6_000_000, now);
        assert_eq!(r.tick(true, 6_000_000, now), Some(Action::Begin(8_000_000)));
        assert_eq!(r.tick(false, 6_000_000, now), Some(Action::End(6_000_000)));
        assert_eq!(r.stats.cancelled, 1);
        let mut off = Refinement::new(0, 8_000_000);
        off.tick(false, 6_000_000, now);
        assert_eq!(off.tick(true, 6_000_000, now), None);
    }
}
