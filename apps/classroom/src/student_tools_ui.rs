//! Rust-only, UI-thread-owned Student notification/reply window.
use super::{classroom_tools::Inbox, dashboard_view::Canvas};
use anyhow::Result;
use minifb::{InputCallback, Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};
use protocol::ToolPacket;
use std::sync::{Arc, Mutex};
struct Characters(Arc<Mutex<Vec<char>>>);
impl InputCallback for Characters {
    fn add_char(&mut self, c: u32) {
        if let Some(c) = char::from_u32(c).filter(|c| !c.is_control()) {
            let mut q = self.0.lock().unwrap();
            if q.len() < 1000 {
                q.push(c);
            }
        }
    }
}
struct Popup {
    window: Window,
    canvas: Canvas,
    message: ToolPacket,
    characters: Arc<Mutex<Vec<char>>>,
    reply: String,
    previous_down: bool,
    last_draw: std::time::Instant,
    message_scroll: usize,
}
pub struct StudentToolsUi {
    inbox: Arc<Inbox>,
    popup: Option<Popup>,
}
impl StudentToolsUi {
    pub fn new(inbox: Arc<Inbox>) -> Self {
        Self { inbox, popup: None }
    }
    pub fn pump(&mut self) {
        if self.popup.is_none() {
            if let Some(p) = self.inbox.take() {
                match Popup::new(p.clone()) {
                    Ok(w) => self.popup = Some(w),
                    Err(e) => {
                        self.inbox
                            .event(&p.id, "error", &format!("Cannot show message: {e}"), 0);
                    }
                }
            }
        }
        let Some(p) = self.popup.as_mut() else { return };
        match p.pump() {
            Ok(Some((status, text))) => {
                self.inbox.event(&p.message.id, status, &text, 0);
                self.popup = None;
            }
            Ok(None) => {}
            Err(e) => {
                self.inbox.event(
                    &p.message.id,
                    "error",
                    &format!("Message window failed: {e}"),
                    0,
                );
                self.popup = None;
            }
        }
    }
}
impl Drop for StudentToolsUi {
    fn drop(&mut self) {
        if let Some(p) = self.popup.take() {
            self.inbox.event(&p.message.id, "dismissed", "", 0);
        }
    }
}
impl Popup {
    fn new(message: ToolPacket) -> Result<Self> {
        let mut canvas = Canvas::new(640, 470);
        let mut window = Window::new(
            "Classroom · Message from teacher",
            640,
            470,
            WindowOptions {
                visible: false,
                topmost: true,
                resize: false,
                ..Default::default()
            },
        )?;
        window.set_target_fps(0);
        let chars = Arc::new(Mutex::new(Vec::new()));
        window.set_input_callback(Box::new(Characters(Arc::clone(&chars))));
        canvas.set_density(window.backing_scale());
        let mut p = Self {
            window,
            canvas,
            message,
            characters: chars,
            reply: String::new(),
            previous_down: false,
            last_draw: std::time::Instant::now(),
            message_scroll: 0,
        };
        p.draw();
        p.window.update_with_buffer(
            &p.canvas.pixels,
            p.canvas.pixel_width(),
            p.canvas.pixel_height(),
        )?;
        p.window.set_visible(true);
        Ok(p)
    }
    fn pump(&mut self) -> Result<Option<(&'static str, String)>> {
        if !self.window.is_open() || self.window.is_key_pressed(Key::Escape, KeyRepeat::No) {
            return Ok(Some(("dismissed", String::new())));
        }
        if self.message.allow_reply {
            for c in self.characters.lock().unwrap().drain(..) {
                if self.reply.chars().count() < 500 {
                    self.reply.push(c);
                }
            }
            if self.window.is_key_pressed(Key::Backspace, KeyRepeat::Yes) {
                self.reply.pop();
            }
            if self.window.is_key_pressed(Key::Enter, KeyRepeat::No)
                && !self.reply.trim().is_empty()
            {
                return Ok(Some(("replied", self.reply.trim().to_owned())));
            }
        }
        if let Some((_, y)) = self.window.get_scroll_wheel() {
            if y > 0.0 {
                self.message_scroll = self.message_scroll.saturating_sub(3);
            } else if y < 0.0 {
                self.message_scroll = self.message_scroll.saturating_add(3);
            }
        }
        let lines = wrap(&self.message.text, 74);
        let max_scroll = lines.len().saturating_sub(10);
        if self.window.is_key_pressed(Key::PageDown, KeyRepeat::No) {
            self.message_scroll = (self.message_scroll + 8).min(max_scroll);
        }
        if self.window.is_key_pressed(Key::PageUp, KeyRepeat::No) {
            self.message_scroll = self.message_scroll.saturating_sub(8);
        }
        self.message_scroll = self.message_scroll.min(max_scroll);
        let down = self.window.get_mouse_down(MouseButton::Left);
        let click = self.window.left_click().or_else(|| {
            (!down && self.previous_down)
                .then(|| self.window.get_mouse_pos(MouseMode::Discard))
                .flatten()
        });
        self.previous_down = down;
        if let Some((x, y)) = click {
            if (414.0..452.0).contains(&y) {
                if (480.0..612.0).contains(&x) {
                    return Ok(Some(("dismissed", String::new())));
                }
                if self.message.allow_reply
                    && (328.0..464.0).contains(&x)
                    && !self.reply.trim().is_empty()
                {
                    return Ok(Some(("replied", self.reply.trim().to_owned())));
                }
            }
        }
        if self.last_draw.elapsed() >= std::time::Duration::from_millis(33) {
            self.canvas.set_density(self.window.backing_scale());
            self.draw();
            self.window.update_with_buffer(
                &self.canvas.pixels,
                self.canvas.pixel_width(),
                self.canvas.pixel_height(),
            )?;
            self.last_draw = std::time::Instant::now();
        } else {
            self.window.update();
        }
        Ok(None)
    }
    fn draw(&mut self) {
        self.canvas.pixels.fill(0x00f4_f7fa);
        self.canvas.fill_rect([0, 0, 640, 66], 0x0017_3747);
        self.canvas
            .text(24, 22, "Message from your teacher", 0x00ff_ffff, 2);
        let lines = wrap(&self.message.text, 74);
        for (i, line) in lines.iter().skip(self.message_scroll).take(10).enumerate() {
            self.canvas.text(24, 84 + i * 21, line, 0x0022_3342, 1);
        }
        if lines.len() > 10 {
            self.canvas.text(
                500,
                294,
                &format!(
                    "{}/{}",
                    self.message_scroll + 1,
                    lines.len().saturating_sub(9)
                ),
                0x0061_7180,
                1,
            );
        }
        if self.message.allow_reply {
            self.canvas
                .text(24, 313, "Reply once · Enter to send", 0x0061_7180, 1);
            self.canvas
                .button([24, 339, 588, 56], 0x00ff_ffff, false, true);
            let visible: String = self
                .reply
                .chars()
                .rev()
                .take(72)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            self.canvas
                .text(36, 358, &format!("{visible}|"), 0x0022_3342, 1);
            self.canvas
                .button([328, 414, 136, 38], 0x000d_7785, false, false);
            self.canvas.text(367, 425, "Reply", 0x00ff_ffff, 1);
        }
        self.canvas
            .button([480, 414, 132, 38], 0x00e5_ebf0, false, false);
        self.canvas.text(524, 425, "Close", 0x0022_3342, 1);
    }
}
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut line = String::new();
        for c in paragraph.chars() {
            line.push(c);
            if line.chars().count() >= width {
                lines.push(std::mem::take(&mut line));
            }
        }
        lines.push(line);
    }
    lines
}
