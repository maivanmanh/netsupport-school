//! Rust-only, UI-thread-owned Student notification/reply window.
//!
//! The window is sized to the message. OK closes it silently (the Teacher only
//! records the job as dismissed; no notification is raised). Reply expands the
//! window with a compact text box; Send delivers the reply to the Teacher.
use super::{classroom_tools::Inbox, dashboard_view::Canvas};
use anyhow::Result;
use minifb::{
    CursorStyle, InputCallback, Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions,
};
use protocol::ToolPacket;
use std::sync::{Arc, Mutex};

const TITLE: &str = "Classroom · Message from teacher";
const PAD: usize = 20;
const HEADER: usize = 56;
/// Message text: 15 px on 23 px lines.
const MSG_PX: u16 = 15;
const LINE: usize = 23;
/// The reply editor draws 14 px text on fixed 21 px lines.
const EDIT_LINE: usize = 21;
const MAX_LINES: usize = 10;
const MIN_WIDTH: usize = 380;
const MAX_WIDTH: usize = 560;
const BUTTON_H: usize = 36;
const BUTTON_W: usize = 92;
const GAP: usize = 10;
const REPLY_LINES: usize = 3;
const REPLY_BOX_H: usize = REPLY_LINES * EDIT_LINE + 16;
const MAX_REPLY: usize = 500;

const BACKGROUND: u32 = 0x00ff_ffff;
const ACCENT: u32 = 0x000d_7785;
const ACCENT_HOVER: u32 = 0x000a_6874;
const INK: u32 = 0x001f_2937;
const MUTED: u32 = 0x006b_7280;
const FIELD_BORDER: u32 = 0x00cb_d5e1;
const LINE_COLOR: u32 = 0x00e5_e7eb;

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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Button {
    Ok,
    Reply,
    Send,
    Cancel,
    /// The × in the header: dismisses like OK.
    Close,
}

/// Keyboard focus target: the reply box or a button.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Focus {
    Input,
    Button(Button),
}
/// Tab order follows the visual left-to-right order; the second element is the
/// default focus (OK when collapsed, the reply box when replying).
fn focus_order(replying: bool, allow_reply: bool) -> (Vec<Focus>, usize) {
    if replying {
        (
            vec![
                Focus::Input,
                Focus::Button(Button::Cancel),
                Focus::Button(Button::Send),
            ],
            0,
        )
    } else if allow_reply {
        (
            vec![Focus::Button(Button::Reply), Focus::Button(Button::Ok)],
            1,
        )
    } else {
        (vec![Focus::Button(Button::Ok)], 0)
    }
}
/// Pure geometry so sizing can be unit-tested without a window.
#[derive(Debug, PartialEq, Eq)]
struct Layout {
    width: usize,
    height: usize,
    visible_lines: usize,
    reply_top: usize,
    buttons_top: usize,
}
impl Layout {
    fn new(width: usize, lines: usize, replying: bool) -> Self {
        let visible_lines = lines.clamp(1, MAX_LINES);
        let message_bottom = HEADER + 18 + visible_lines * LINE;
        let reply_top = message_bottom + 14;
        let buttons_top = if replying {
            reply_top + 24 + REPLY_BOX_H + 16
        } else {
            message_bottom + 20
        };
        Self {
            width,
            height: buttons_top + BUTTON_H + 18,
            visible_lines,
            reply_top,
            buttons_top,
        }
    }
    fn box_top(&self) -> usize {
        self.reply_top + 24
    }
    /// Where the reply editor draws its text (inside the box).
    fn editor_rect(&self) -> [usize; 4] {
        [
            PAD + 12,
            self.box_top() + 8,
            self.width - 2 * PAD - 24,
            REPLY_LINES * EDIT_LINE,
        ]
    }
    fn close_button(&self) -> [usize; 4] {
        [self.width - 44, 14, 28, 28]
    }
    fn buttons(&self, replying: bool, allow_reply: bool) -> Vec<(Button, [usize; 4])> {
        let right = self.width - PAD;
        let first = [right - BUTTON_W, self.buttons_top, BUTTON_W, BUTTON_H];
        let second = [
            right - 2 * BUTTON_W - GAP,
            self.buttons_top,
            BUTTON_W,
            BUTTON_H,
        ];
        if replying {
            vec![(Button::Send, first), (Button::Cancel, second)]
        } else if allow_reply {
            vec![(Button::Ok, first), (Button::Reply, second)]
        } else {
            vec![(Button::Ok, first)]
        }
    }
}

struct Popup {
    window: Window,
    canvas: Canvas,
    message: ToolPacket,
    characters: Arc<Mutex<Vec<char>>>,
    reply: String,
    editor: super::text_editor::Editor,
    replying: bool,
    lines: Vec<String>,
    layout: Layout,
    previous_down: bool,
    hover: Option<Button>,
    focus: Focus,
    last_draw: std::time::Instant,
    message_scroll: usize,
    copy_pending: Option<std::time::Instant>,
    copy_notice_until: Option<std::time::Instant>,
    copy_failed: bool,
}
pub struct StudentToolsUi {
    inbox: Arc<Inbox>,
    popup: Option<Popup>,
    message_epoch: u64,
    files: super::student_file_ui::FileProgress,
    lock: Option<super::student_lock::Lock>,
    lock_retry: std::time::Instant,
}
impl StudentToolsUi {
    pub fn new(inbox: Arc<Inbox>) -> Self {
        Self {
            message_epoch: inbox.message_epoch(),
            inbox,
            popup: None,
            files: super::student_file_ui::FileProgress::default(),
            lock: None,
            lock_retry: std::time::Instant::now(),
        }
    }
    /// A lock the Teacher can no longer lift (silent for this long) ends itself.
    const LOCK_FAILSAFE_MS: u64 = 60_000;
    fn pump_lock(&mut self) {
        let (epoch, message) = self.inbox.lock_state();
        if message.is_some() && self.inbox.silent_ms() > Self::LOCK_FAILSAFE_MS {
            tracing::warn!("Teacher silent too long; lifting screen lock");
            self.inbox.set_lock(None);
            self.lock = None;
            return;
        }
        let Some(message) = message else {
            self.lock = None;
            return;
        };
        if let Some(lock) = self.lock.as_mut() {
            if lock.epoch() != epoch {
                lock.update(epoch, &message);
            }
            if lock.pump().is_err() {
                self.lock = None;
            }
        } else if self.lock_retry.elapsed() >= std::time::Duration::from_secs(1) {
            self.lock_retry = std::time::Instant::now();
            match super::student_lock::Lock::start(epoch, &message) {
                Ok(lock) => self.lock = Some(lock),
                Err(error) => tracing::warn!(%error, "Could not show lock screen"),
            }
        }
    }
    pub fn pump(&mut self) {
        self.pump_lock();
        self.files.pump(&self.inbox);
        let epoch = self.inbox.message_epoch();
        if epoch != self.message_epoch {
            self.popup = None; // Old Teacher session: do not send a dismissed event.
            self.message_epoch = epoch;
        }
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
                p.window.set_visible(false);
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
fn open_window(width: usize, height: usize) -> Result<Window> {
    let mut window = Window::new(
        TITLE,
        width,
        height,
        WindowOptions {
            visible: false,
            topmost: true,
            resize: false,
            borderless: true,
            title: false,
            ..Default::default()
        },
    )?;
    window.set_target_fps(0);
    let (x, y) = centered(window.work_area(), window.outer_frame());
    window.set_outer_origin(x, y);
    Ok(window)
}
impl Popup {
    fn new(message: ToolPacket) -> Result<Self> {
        // Measure with a scratch canvas so the window opens at its final size.
        let measure = Canvas::new(1, 1);
        let (width, lines) = fit_message(&measure, &message.text);
        let layout = Layout::new(width, lines.len(), false);
        let mut window = open_window(layout.width, layout.height)?;
        let chars = Arc::new(Mutex::new(Vec::new()));
        window.set_input_callback(Box::new(Characters(Arc::clone(&chars))));
        let mut canvas = measure;
        canvas.resize(layout.width, layout.height);
        canvas.set_density(window.backing_scale());
        let (order, default) = focus_order(false, message.allow_reply);
        let focus = order[default];
        let mut p = Self {
            window,
            canvas,
            message,
            characters: chars,
            reply: String::new(),
            editor: super::text_editor::Editor::default(),
            replying: false,
            lines,
            layout,
            previous_down: false,
            hover: None,
            focus,
            last_draw: std::time::Instant::now(),
            message_scroll: 0,
            copy_pending: None,
            copy_notice_until: None,
            copy_failed: false,
        };
        p.present()?;
        p.window.set_visible(true);
        Ok(p)
    }
    fn present(&mut self) -> Result<()> {
        self.canvas.set_density(self.window.backing_scale());
        self.draw();
        self.window.update_with_buffer(
            &self.canvas.pixels,
            self.canvas.pixel_width(),
            self.canvas.pixel_height(),
        )?;
        self.last_draw = std::time::Instant::now();
        Ok(())
    }
    /// Reply changes the existing native surface, never creates a second dialog.
    fn set_replying(&mut self, replying: bool) -> Result<()> {
        self.replying = replying;
        self.characters.lock().unwrap().clear();
        let layout = Layout::new(self.layout.width, self.lines.len(), replying);
        let area = self.window.work_area();
        self.window
            .resize_notification(layout.width, layout.height)?;
        let (x, y) = centered(area, self.window.outer_frame());
        self.window.set_outer_origin(x, y);
        self.canvas.resize(layout.width, layout.height);
        self.layout = layout;
        self.previous_down = false;
        self.hover = None;
        let (order, default) = focus_order(replying, self.message.allow_reply);
        self.focus = order[default];
        self.present()?;
        self.window.set_visible(true);
        Ok(())
    }
    #[allow(clippy::cast_precision_loss)] // window coordinates are small
    fn pump(&mut self) -> Result<Option<(&'static str, String)>> {
        if !self.window.is_open() {
            return Ok(Some(("dismissed", String::new())));
        }
        if self.window.is_key_pressed(Key::Escape, KeyRepeat::No) {
            if self.replying {
                self.set_replying(false)?;
                return Ok(None);
            }
            return Ok(Some(("dismissed", String::new())));
        }
        let typed: Vec<char> = self.characters.lock().unwrap().drain(..).collect();
        if self.window.is_key_pressed(Key::Tab, KeyRepeat::Yes) {
            let (order, _) = focus_order(self.replying, self.message.allow_reply);
            let at = order.iter().position(|f| *f == self.focus).unwrap_or(0);
            let back =
                self.window.is_key_down(Key::LeftShift) || self.window.is_key_down(Key::RightShift);
            let next = if back {
                (at + order.len() - 1) % order.len()
            } else {
                (at + 1) % order.len()
            };
            self.focus = order[next];
        }
        if self.focus == Focus::Input {
            let keys = self.window.get_keys_pressed(KeyRepeat::Yes);
            self.editor.input(
                &mut self.reply,
                &mut self.window,
                &keys,
                &typed,
                MAX_REPLY,
                super::text_editor::plain,
            );
            self.editor.vertical(
                &self.canvas,
                &self.reply,
                self.layout.editor_rect(),
                1,
                &self.window,
                &keys,
            );
        }
        let enter = self.window.is_key_pressed(Key::Enter, KeyRepeat::No)
            || self.window.is_key_pressed(Key::NumPadEnter, KeyRepeat::No);
        let space = self.window.is_key_pressed(Key::Space, KeyRepeat::No);
        match self.focus {
            Focus::Input if enter => {
                if let Some(result) = self.activate(Button::Send)? {
                    return Ok(Some(result));
                }
            }
            Focus::Button(button) if enter || space => {
                return self.activate(button);
            }
            _ => {}
        }
        let max_scroll = self.lines.len().saturating_sub(self.layout.visible_lines);
        if let Some((_, y)) = self.window.get_scroll_wheel() {
            if y > 0.0 {
                self.message_scroll = self.message_scroll.saturating_sub(2);
            } else if y < 0.0 {
                self.message_scroll += 2;
            }
        }
        if self.window.is_key_pressed(Key::PageDown, KeyRepeat::No) {
            self.message_scroll += self.layout.visible_lines;
        }
        if self.window.is_key_pressed(Key::PageUp, KeyRepeat::No) {
            self.message_scroll = self
                .message_scroll
                .saturating_sub(self.layout.visible_lines);
        }
        self.message_scroll = self.message_scroll.min(max_scroll);

        let mut buttons = self.layout.buttons(self.replying, self.message.allow_reply);
        buttons.push((Button::Close, self.layout.close_button()));
        let at = |pos: Option<(f32, f32)>| {
            let (x, y) = pos?;
            buttons
                .iter()
                .find(|(_, [bx, by, bw, bh])| {
                    (*bx as f32..(bx + bw) as f32).contains(&x)
                        && (*by as f32..(by + bh) as f32).contains(&y)
                })
                .map(|(b, _)| *b)
        };
        self.hover = at(self.window.get_mouse_pos(MouseMode::Discard));
        let pointer = self.window.get_mouse_pos(MouseMode::Discard);
        let over_input = self.replying && pointer.is_some_and(|(x, y)| self.in_reply_box(x, y));
        let can_send = !self.reply.trim().is_empty();
        self.window.set_cursor_style(match self.hover {
            Some(Button::Send) if !can_send => CursorStyle::Arrow,
            Some(_) => CursorStyle::OpenHand, // the pointing hand, see vendor/minifb
            None if over_input => CursorStyle::Ibeam,
            None => CursorStyle::Arrow,
        });
        let down = self.window.get_mouse_down(MouseButton::Left);
        if down && !self.previous_down && self.replying {
            if let Some(p) = self.window.get_mouse_pos(MouseMode::Discard) {
                if self.in_reply_box(p.0, p.1) {
                    self.focus = Focus::Input;
                    self.editor.click(
                        &self.canvas,
                        &self.reply,
                        self.layout.editor_rect(),
                        1,
                        p,
                        self.window.is_key_down(Key::LeftShift)
                            || self.window.is_key_down(Key::RightShift),
                    );
                    self.editor.start_drag();
                }
            }
        }
        let click = self.window.left_click().or_else(|| {
            (!down && self.previous_down)
                .then(|| self.window.get_mouse_pos(MouseMode::Discard))
                .flatten()
        });
        self.previous_down = down;
        if let Some(button) = at(click) {
            return self.activate(button);
        }
        if self.replying && click.is_some_and(|(x, y)| self.in_reply_box(x, y)) {
            self.focus = Focus::Input;
            self.editor.click(
                &self.canvas,
                &self.reply,
                self.layout.editor_rect(),
                1,
                click.unwrap(),
                self.window.is_key_down(Key::LeftShift) || self.window.is_key_down(Key::RightShift),
            );
            self.editor.start_drag();
        } else if click.is_some_and(|(x, y)| copy_hit(&self.layout, self.replying, x, y)) {
            self.copy_pending =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(400));
        }
        if self.replying && self.focus == Focus::Input {
            self.editor.drag(
                &self.canvas,
                &self.reply,
                self.layout.editor_rect(),
                1,
                &self.window,
            );
        }
        if let Some(deadline) = self.copy_pending {
            match self.window.copy_text(&self.message.text) {
                Ok(()) => {
                    self.copy_pending = None;
                    self.copy_failed = false;
                    self.copy_notice_until =
                        Some(std::time::Instant::now() + std::time::Duration::from_secs(2));
                }
                Err(error) if std::time::Instant::now() >= deadline => {
                    tracing::warn!(%error, "Student message clipboard copy failed");
                    self.copy_pending = None;
                    self.copy_failed = true;
                    self.copy_notice_until =
                        Some(std::time::Instant::now() + std::time::Duration::from_secs(2));
                }
                Err(_) => {} // Retry on next UI pump; never sleep/block the UI.
            }
        }
        if self.last_draw.elapsed() >= std::time::Duration::from_millis(33) {
            self.present()?;
        } else {
            self.window.update();
        }
        Ok(None)
    }
    /// Performs a button action (mouse, Enter or Space).
    fn activate(&mut self, button: Button) -> Result<Option<(&'static str, String)>> {
        match button {
            Button::Ok | Button::Close => return Ok(Some(("dismissed", String::new()))),
            Button::Reply => self.set_replying(true)?,
            Button::Cancel => self.set_replying(false)?,
            Button::Send if !self.reply.trim().is_empty() => {
                return Ok(Some(("replied", self.reply.trim().to_owned())));
            }
            Button::Send => self.focus = Focus::Input,
        }
        Ok(None)
    }
    #[allow(clippy::cast_precision_loss)]
    fn in_reply_box(&self, x: f32, y: f32) -> bool {
        let top = self.layout.box_top();
        (PAD as f32..(self.layout.width - PAD) as f32).contains(&x)
            && (top as f32..(top + REPLY_BOX_H) as f32).contains(&y)
    }
    #[allow(clippy::too_many_lines)] // one immediate-mode paint pass
    fn draw(&mut self) {
        let Layout {
            width,
            height,
            visible_lines,
            reply_top,
            ..
        } = self.layout;
        let hover = self.hover;
        let c = &mut self.canvas;
        c.pixels.fill(BACKGROUND);
        c.fill_rect([0, 0, width, 3], ACCENT);
        // Header: a chat badge, the title and the close button.
        c.disc(PAD as f32 + 16.0, 30.0, 16.0, 0x00dd_eff1);
        c.round_rect([PAD + 8, 23, 17, 12], 4, ACCENT);
        c.disc(PAD as f32 + 12.0, 36.5, 2.4, ACCENT);
        c.text_bold(PAD + 44, 20, "Message from your teacher", INK, 15);
        let close = self.layout.close_button();
        let close_hot = hover == Some(Button::Close);
        if close_hot {
            c.round_rect(close, 6, 0x00ee_f2f6);
        }
        let ink = if close_hot { INK } else { MUTED };
        let (cx, cy) = (close[0] as f32 + 14.0, close[1] as f32 + 14.0);
        c.line((cx - 4.5, cy - 4.5), (cx + 4.5, cy + 4.5), 1.6, ink);
        c.line((cx - 4.5, cy + 4.5), (cx + 4.5, cy - 4.5), 1.6, ink);
        c.fill_rect([0, HEADER - 1, width, 1], LINE_COLOR);
        // The message.
        let top = HEADER + 18;
        for (i, line) in self
            .lines
            .iter()
            .skip(self.message_scroll)
            .take(visible_lines)
            .enumerate()
        {
            c.text_px(PAD, top + i * LINE, line, INK, MSG_PX);
        }
        if self.lines.len() > visible_lines {
            let label = format!(
                "{}–{} / {}",
                self.message_scroll + 1,
                (self.message_scroll + visible_lines).min(self.lines.len()),
                self.lines.len()
            );
            let w = c.text_width_px(&label, 12);
            c.text_px(width - PAD - w, HEADER + 3, &label, 0x009c_a3af, 12);
        }
        if self.replying {
            c.text_bold(PAD, reply_top, "Your reply", INK, 13);
            let counter = format!("{} / {MAX_REPLY}", self.reply.chars().count());
            let cw = c.text_width_px(&counter, 12);
            c.text_px(width - PAD - cw, reply_top + 1, &counter, 0x009c_a3af, 12);
            let box_top = self.layout.box_top();
            let box_w = width - 2 * PAD;
            let input_focused = self.focus == Focus::Input;
            if input_focused {
                c.round_rect([PAD, box_top, box_w, REPLY_BOX_H], 8, ACCENT);
                c.round_rect(
                    [PAD + 2, box_top + 2, box_w - 4, REPLY_BOX_H - 4],
                    6,
                    BACKGROUND,
                );
            } else {
                c.round_rect_border(
                    [PAD, box_top, box_w, REPLY_BOX_H],
                    8,
                    FIELD_BORDER,
                    BACKGROUND,
                );
            }
            self.editor
                .draw(c, &self.reply, self.layout.editor_rect(), 1, input_focused);
        } else {
            let notice = if self
                .copy_notice_until
                .is_some_and(|t| std::time::Instant::now() < t)
            {
                if self.copy_failed {
                    "Click again to copy"
                } else {
                    "Copied to clipboard"
                }
            } else {
                "Click the message to copy it"
            };
            c.text_px(PAD, self.layout.buttons_top + 11, notice, MUTED, 12);
        }
        let can_send = !self.reply.trim().is_empty();
        for (button, area) in self.layout.buttons(self.replying, self.message.allow_reply) {
            let primary = matches!(button, Button::Ok | Button::Send);
            let disabled = button == Button::Send && !can_send;
            let hot = hover == Some(button) && !disabled;
            if self.focus == Focus::Button(button) {
                let [x, y, w, h] = area;
                c.round_rect([x - 3, y - 3, w + 6, h + 6], 11, 0x0025_63eb);
                c.round_rect([x - 1, y - 1, w + 2, h + 2], 9, BACKGROUND);
            }
            let (fill, border, color) = match (primary, disabled) {
                (true, true) => (0x00b8_c4cc, 0x00b8_c4cc, 0x00ff_ffff),
                (true, false) => {
                    let fill = if hot { ACCENT_HOVER } else { ACCENT };
                    (fill, fill, 0x00ff_ffff)
                }
                (false, _) => (
                    if hot { 0x00f1_f5f9 } else { BACKGROUND },
                    FIELD_BORDER,
                    INK,
                ),
            };
            c.round_rect_border(area, 8, border, fill);
            let label = match button {
                Button::Ok => "OK",
                Button::Reply => "Reply",
                Button::Send => "Send",
                Button::Cancel => "Cancel",
                Button::Close => "",
            };
            let w = c.text_width_px(label, 14);
            c.text_bold(
                area[0] + area[2].saturating_sub(w) / 2,
                area[1] + 9,
                label,
                color,
                14,
            );
        }
        c.notification_border(width, height);
    }
}
fn centered(area: [isize; 4], frame: [isize; 4]) -> (isize, isize) {
    (
        area[0] + (area[2] - frame[2]).max(0) / 2,
        area[1] + (area[3] - frame[3]).max(0) / 2,
    )
}
/// Copy from body/background only; buttons and reply editor keep their actions.
#[allow(clippy::cast_precision_loss)]
fn copy_hit(layout: &Layout, replying: bool, x: f32, y: f32) -> bool {
    let bottom = if replying {
        layout.reply_top
    } else {
        layout.buttons_top
    };
    // The header (title, close button) is not part of the copyable message.
    x >= 0.0 && x < layout.width as f32 && y >= HEADER as f32 && y < bottom as f32
}
/// Picks the narrowest comfortable width (within bounds) and wraps to it.
fn fit_message(canvas: &Canvas, text: &str) -> (usize, Vec<String>) {
    let natural = text
        .lines()
        .map(|line| canvas.text_width_px(line, MSG_PX))
        .max()
        .unwrap_or(0);
    let width = (natural + 2 * PAD + 4).clamp(MIN_WIDTH, MAX_WIDTH);
    (width, wrap_px(canvas, text, width - 2 * PAD, MSG_PX))
}
/// Word wrap by rendered width at the default text size.
pub(super) fn wrap(canvas: &Canvas, text: &str, max_width: usize) -> Vec<String> {
    wrap_px(canvas, text, max_width, 14)
}
/// Word wrap by rendered width; words longer than a line are split by character.
fn wrap_px(canvas: &Canvas, text: &str, max_width: usize, px: u16) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut line = String::new();
        for word in paragraph.split_inclusive(' ') {
            let candidate = format!("{line}{word}");
            if canvas.text_width_px(candidate.trim_end(), px) <= max_width {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(line.trim_end().to_owned());
                line.clear();
            }
            for c in word.chars() {
                line.push(c);
                if line.chars().count() > 1 && canvas.text_width_px(line.trim_end(), px) > max_width
                {
                    line.pop();
                    lines.push(std::mem::take(&mut line));
                    line.push(c);
                }
            }
        }
        lines.push(line.trim_end().to_owned());
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn message_center_and_copy_preserve_action_areas() {
        assert_eq!(centered([0, 0, 1920, 1040], [0, 0, 320, 130]), (800, 455));
        assert_eq!(
            centered([-1920, 40, 1920, 1000], [0, 0, 520, 300]),
            (-1220, 390)
        );
        assert_eq!(centered([10, 20, 200, 100], [0, 0, 320, 150]), (10, 20));
        let collapsed = Layout::new(MIN_WIDTH, 3, false);
        assert!(copy_hit(&collapsed, false, 40.0, (HEADER + 18) as f32));
        // The title bar (and its close button) is not copyable text.
        assert!(!copy_hit(&collapsed, false, 40.0, 20.0));
        let [cx, cy, ..] = collapsed.close_button();
        assert!(!copy_hit(
            &collapsed,
            false,
            (cx + 4) as f32,
            (cy + 4) as f32
        ));
        assert!(!copy_hit(&collapsed, false, -1.0, 40.0));
        for (_, [x, y, ..]) in collapsed.buttons(false, true) {
            assert!(!copy_hit(&collapsed, false, (x + 2) as f32, (y + 2) as f32));
        }
        let reply = Layout::new(MIN_WIDTH, 3, true);
        assert!(!copy_hit(
            &reply,
            true,
            PAD as f32 + 10.0,
            (reply.reply_top + 25) as f32
        ));
    }
    #[test]
    fn short_message_opens_compact_without_reply_box() {
        let canvas = Canvas::new(1, 1);
        let (width, lines) = fit_message(&canvas, "Please open slide 4");
        assert_eq!(width, MIN_WIDTH);
        assert_eq!(lines.len(), 1);
        let collapsed = Layout::new(width, lines.len(), false);
        let expanded = Layout::new(width, lines.len(), true);
        assert!(collapsed.height < 190, "{collapsed:?}");
        assert!(expanded.height > collapsed.height + REPLY_BOX_H);
        let labels: Vec<_> = collapsed
            .buttons(false, true)
            .into_iter()
            .map(|(b, _)| b)
            .collect();
        assert_eq!(labels, [Button::Ok, Button::Reply]);
        assert_eq!(
            expanded
                .buttons(true, true)
                .into_iter()
                .map(|(b, _)| b)
                .collect::<Vec<_>>(),
            [Button::Send, Button::Cancel]
        );
        assert_eq!(collapsed.buttons(false, false).len(), 1);
    }
    #[test]
    fn tab_order_defaults_to_ok_then_reply_box() {
        let (order, default) = focus_order(false, true);
        assert_eq!(order[default], Focus::Button(Button::Ok));
        assert_eq!(order.len(), 2);
        let (order, default) = focus_order(true, true);
        assert_eq!(order[default], Focus::Input);
        assert_eq!(
            order,
            [
                Focus::Input,
                Focus::Button(Button::Cancel),
                Focus::Button(Button::Send)
            ]
        );
        assert_eq!(focus_order(false, false).0, [Focus::Button(Button::Ok)]);
    }
    #[test]
    fn long_message_is_wrapped_bounded_and_scrollable() {
        let canvas = Canvas::new(1, 1);
        let text = "word ".repeat(600);
        let (width, lines) = fit_message(&canvas, &text);
        assert_eq!(width, MAX_WIDTH);
        assert!(lines.len() > MAX_LINES);
        assert!(lines
            .iter()
            .all(|l| canvas.text_width_px(l, MSG_PX) <= MAX_WIDTH - 2 * PAD));
        assert_eq!(
            Layout::new(width, lines.len(), false).visible_lines,
            MAX_LINES
        );
    }
    #[test]
    fn unbroken_text_is_split_by_character() {
        let canvas = Canvas::new(1, 1);
        let lines = wrap(&canvas, &"x".repeat(400), 120);
        assert!(lines.len() > 1);
        assert_eq!(lines.concat().len(), 400);
    }
}
