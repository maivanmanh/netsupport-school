//! Passive class-broadcast diagnostics; one tiny paint per second, no video copies.
use super::dashboard_view::Canvas;
use minifb::{Window, WindowOptions};
use std::time::{Duration, Instant};

const SIZE: usize = 56;
const MARGIN: isize = 12;

pub struct FpsBadge {
    window: Option<Window>,
    canvas: Option<Canvas>,
    started: Instant,
    frames: u64,
    last_pump: Instant,
    failed: bool,
    enabled: bool,
}
impl FpsBadge {
    pub fn new(enabled: bool) -> Self {
        let now = Instant::now();
        Self {
            window: None,
            canvas: None,
            started: now,
            frames: 0,
            last_pump: now,
            failed: false,
            enabled,
        }
    }
    pub fn frame(&mut self) {
        self.frames = self.frames.saturating_add(1);
    }
    pub fn pump(&mut self) {
        if !self.enabled || self.failed || self.last_pump.elapsed() < Duration::from_millis(100) {
            return;
        }
        self.last_pump = Instant::now();
        if let Some(window) = self.window.as_mut() {
            window.update();
        }
        if self.started.elapsed() < Duration::from_secs(1) {
            return;
        }
        let fps = self.frames as f64 / self.started.elapsed().as_secs_f64();
        self.started = Instant::now();
        self.frames = 0;
        if self.window.is_none() {
            match Window::new(
                "Classroom · FPS",
                SIZE,
                SIZE,
                WindowOptions {
                    visible: false,
                    resize: false,
                    borderless: true,
                    title: false,
                    ..Default::default()
                },
            ) {
                Ok(mut window) => {
                    window.set_target_fps(0);
                    window.set_passive_overlay(200);
                    #[cfg(not(windows))]
                    window.set_round_overlay();
                    self.canvas = Some(Canvas::new(SIZE, SIZE));
                    self.window = Some(window);
                }
                Err(error) => {
                    tracing::warn!(%error,"Student FPS badge unavailable");
                    self.failed = true;
                    return;
                }
            }
        }
        let window = self.window.as_mut().unwrap();
        let canvas = self.canvas.as_mut().unwrap();
        canvas.set_density(window.backing_scale());
        canvas.pixels.fill(0x001b2635);
        let text = format!("{fps:.0} FPS");
        let text_width = canvas.text_width(&text, 1);
        canvas.text(
            SIZE.saturating_sub(text_width) / 2,
            SIZE / 2 - 8,
            &text,
            0x00eff7ff,
            1,
        );
        let [x, y, _, _] = window.work_area();
        window.set_outer_origin(x + MARGIN, y + MARGIN);
        #[cfg(windows)]
        let draw = {
            let width = canvas.pixel_width();
            let height = canvas.pixel_height();
            smooth_circle(&mut canvas.pixels, width, height);
            window.update_alpha_overlay(&canvas.pixels, width, height)
        };
        #[cfg(not(windows))]
        let draw =
            window.update_with_buffer(&canvas.pixels, canvas.pixel_width(), canvas.pixel_height());
        if let Err(error) = draw {
            tracing::warn!(%error,"Student FPS badge draw failed");
            self.failed = true;
            self.window = None;
            return;
        }
        window.show_inactive_topmost();
    }
}

/// Signed-distance coverage gives a one-pixel antialias transition. Premultiply
/// RGB for the desktop compositor so transparent edges never acquire dark halos.
#[cfg(any(windows, test))]
fn smooth_circle(pixels: &mut [u32], width: usize, height: usize) {
    let radius = width.min(height) as f64 / 2.0;
    for (i, pixel) in pixels.iter_mut().enumerate() {
        let x = (i % width) as f64 + 0.5 - width as f64 / 2.0;
        let y = (i / width) as f64 + 0.5 - height as f64 / 2.0;
        let alpha = ((radius + 0.5 - x.hypot(y)).clamp(0.0, 1.0) * 255.0).round() as u32;
        let channel = |shift: u32| (((*pixel >> shift) & 255u32) * alpha + 127) / 255;
        *pixel = (alpha << 24) | (channel(16) << 16) | (channel(8) << 8) | channel(0);
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn circle_has_transparent_corners_and_smooth_symmetric_edges() {
        let mut pixels = vec![0x00ffffff; 56 * 56];
        super::smooth_circle(&mut pixels, 56, 56);
        assert_eq!(pixels[0], 0);
        assert_eq!(pixels[28 * 56 + 28], 0xffffffff);
        assert!(pixels.iter().any(|p| (p >> 24) > 0 && (p >> 24) < 255));
        for y in 0..56 {
            for x in 0..56 {
                assert_eq!(pixels[y * 56 + x], pixels[y * 56 + 55 - x]);
                assert_eq!(pixels[y * 56 + x], pixels[(55 - y) * 56 + x]);
                assert_eq!(pixels[y * 56 + x] & 255, pixels[y * 56 + x] >> 24);
            }
        }
    }
}
