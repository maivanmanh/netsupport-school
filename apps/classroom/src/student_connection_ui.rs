//! Optional, nonmodal connection diagnostic, owned by the existing Student UI pump.
use super::dashboard_view::Canvas;
use minifb::{Window, WindowOptions};
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct ConnectionStatus {
    window: Option<Window>,
    canvas: Option<Canvas>,
    last: String,
    connected_since: Option<Instant>,
    dismissed: bool,
    last_draw: Option<Instant>,
}
impl ConnectionStatus {
    #[allow(clippy::too_many_lines)] // One bounded native popup paint/pump pass.
    pub fn pump(&mut self, enabled: bool, status: &str, presenting: bool) {
        if !enabled || presenting {
            self.window = None;
            self.canvas = None;
            self.last.clear();
            return;
        }
        let connected = status.starts_with("Connected to Teacher");
        if status != self.last {
            status.clone_into(&mut self.last);
            self.connected_since = connected.then(Instant::now);
            self.dismissed = false;
        }
        if self
            .connected_since
            .is_some_and(|t| t.elapsed() >= Duration::from_secs(3))
        {
            if self.window.is_some() {
                tracing::info!("Student connected popup hidden after 3 seconds");
            }
            self.window = None;
            self.canvas = None;
            return;
        }
        if self.dismissed {
            return;
        }
        if self.window.is_none() {
            match Window::new(
                "Classroom · Connection",
                400,
                118,
                WindowOptions {
                    visible: false,
                    resize: false,
                    ..Default::default()
                },
            ) {
                Ok(mut window) => {
                    window.set_target_fps(0);
                    self.canvas = Some(Canvas::new(400, 118));
                    self.window = Some(window);
                    self.last_draw = None;
                    tracing::info!(connected, status, "Student connection popup shown");
                }
                Err(error) => {
                    tracing::warn!(%error,"Student connection popup unavailable");
                    self.dismissed = true;
                    return;
                }
            }
        }
        let window = self.window.as_mut().unwrap();
        if !window.is_open() {
            self.window = None;
            self.canvas = None;
            self.dismissed = true;
            return;
        }
        if self
            .last_draw
            .is_none_or(|t| t.elapsed() >= Duration::from_millis(100))
        {
            let canvas = self.canvas.as_mut().unwrap();
            canvas.set_density(window.backing_scale());
            canvas.pixels.fill(0x00f7_f9fb);
            canvas.fill_rect([0, 0, 400, 36], 0x0017_3747);
            canvas.text(16, 9, "Classroom Student", 0x00ff_ffff, 1);
            canvas.fill_rect(
                [16, 60, 10, 10],
                if connected { 0x0021_a366 } else { 0x00e5_9a23 },
            );
            canvas.text(
                36,
                52,
                if connected {
                    "Connected to server"
                } else {
                    "Connecting to server…"
                },
                0x001c_3345,
                1,
            );
            canvas.text(
                36,
                80,
                status
                    .trim_start_matches("Connected to Teacher ")
                    .trim_start_matches("Connecting to Teacher "),
                0x0065_7583,
                1,
            );
            if let Err(error) = window.update_with_buffer(
                &canvas.pixels,
                canvas.pixel_width(),
                canvas.pixel_height(),
            ) {
                tracing::warn!(%error,"Connection popup draw failed");
                self.dismissed = true;
            }
            window.show_inactive();
            self.last_draw = Some(Instant::now());
        } else {
            window.update();
        }
    }
}
