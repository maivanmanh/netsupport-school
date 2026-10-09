//! Shared bounded native text editing. Positions are UTF-8 boundaries, never bytes
//! inside a character; history stays in memory and is never logged/persisted.
use super::dashboard_view::Canvas;
use minifb::{Key, MouseButton, MouseMode, Window};

#[derive(Default)]
pub struct Editor {
    caret: usize,
    anchor: usize,
    first_line: usize,
    dragging: bool,
    undo: Vec<(String, usize, usize)>,
    redo: Vec<(String, usize, usize)>,
}
impl Editor {
    fn normalize(&mut self, text: &str) {
        for p in [&mut self.caret, &mut self.anchor] {
            *p = (*p).min(text.len());
            while !text.is_char_boundary(*p) {
                *p -= 1;
            }
        }
    }
    pub fn end(&mut self, text: &str) {
        self.caret = text.len();
        self.anchor = self.caret;
    }
    pub fn select_all(&mut self, text: &str) {
        self.anchor = 0;
        self.caret = text.len();
    }
    fn range(&self) -> std::ops::Range<usize> {
        self.anchor.min(self.caret)..self.anchor.max(self.caret)
    }
    fn remember(&mut self, text: &str) {
        if self.undo.len() == 32 {
            self.undo.remove(0);
        }
        self.undo.push((text.into(), self.caret, self.anchor));
        self.redo.clear();
    }
    fn replace(&mut self, text: &mut String, insert: &str) {
        self.remember(text);
        let r = self.range();
        self.caret = r.start + insert.len();
        self.anchor = self.caret;
        text.replace_range(r, insert);
    }
    /// Native shortcuts and typed characters share one edit path. Clipboard
    /// failures preserve the text/selection; paste is filtered and length-bounded.
    pub fn input(
        &mut self,
        text: &mut String,
        window: &mut Window,
        keys: &[Key],
        typed: &[char],
        max: usize,
        accept: fn(char) -> bool,
    ) {
        self.normalize(text);
        let shift = window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift);
        let ctrl = window.is_key_down(Key::LeftCtrl) || window.is_key_down(Key::RightCtrl);
        let command = if cfg!(target_os = "macos") {
            window.is_key_down(Key::LeftSuper) || window.is_key_down(Key::RightSuper)
        } else {
            ctrl
        };
        let alt = window.is_key_down(Key::LeftAlt) || window.is_key_down(Key::RightAlt);
        if command {
            if keys.contains(&Key::A) {
                self.select_all(text);
            }
            if keys.contains(&Key::C) || keys.contains(&Key::X) {
                let r = self.range();
                if !r.is_empty() && window.copy_text(&text[r]).is_ok() && keys.contains(&Key::X) {
                    self.replace(text, "");
                }
            }
            if keys.contains(&Key::V) {
                if let Ok(value) = window.paste_text() {
                    let available = max
                        .saturating_sub(text.chars().count() - text[self.range()].chars().count());
                    let value: String = value
                        .chars()
                        .filter(|c| accept(*c))
                        .take(available)
                        .collect();
                    if !value.is_empty() {
                        self.replace(text, &value);
                    }
                }
            }
            if keys.contains(&Key::Z) || keys.contains(&Key::Y) {
                let redo = keys.contains(&Key::Y) || shift;
                let (from, to) = if redo {
                    (&mut self.redo, &mut self.undo)
                } else {
                    (&mut self.undo, &mut self.redo)
                };
                if let Some((old, caret, anchor)) = from.pop() {
                    to.push((std::mem::replace(text, old), self.caret, self.anchor));
                    self.caret = caret;
                    self.anchor = anchor;
                }
            }
        }
        for (key, right) in [(Key::Left, false), (Key::Right, true)] {
            if !keys.contains(&key) {
                continue;
            }
            let r = self.range();
            let next = if !shift && !r.is_empty() {
                if right {
                    r.end
                } else {
                    r.start
                }
            } else if cfg!(target_os = "macos") && command {
                if right {
                    text.len()
                } else {
                    0
                }
            } else if ctrl || alt {
                let mut p = self.caret;
                if right {
                    while p < text.len() && !text[p..].starts_with(char::is_whitespace) {
                        p = next(text, p);
                    }
                    while p < text.len() && text[p..].starts_with(char::is_whitespace) {
                        p = next(text, p);
                    }
                } else {
                    while p > 0 && text[previous(text, p)..p].chars().all(char::is_whitespace) {
                        p = previous(text, p);
                    }
                    while p > 0 && !text[previous(text, p)..p].chars().all(char::is_whitespace) {
                        p = previous(text, p);
                    }
                }
                p
            } else if right {
                next(text, self.caret)
            } else {
                previous(text, self.caret)
            };
            self.caret = next;
            if !shift {
                self.anchor = next;
            }
        }
        for (key, position) in [(Key::Home, 0), (Key::End, text.len())] {
            if keys.contains(&key) {
                self.caret = position;
                if !shift {
                    self.anchor = position;
                }
            }
        }
        if keys.contains(&Key::Backspace) || keys.contains(&Key::Delete) {
            if self.range().is_empty() {
                let back = keys.contains(&Key::Backspace);
                self.anchor = if cfg!(target_os = "macos") && command && back {
                    0
                } else if ctrl || alt {
                    let mut p = self.caret;
                    if back {
                        while p > 0 && text[previous(text, p)..p].chars().all(char::is_whitespace) {
                            p = previous(text, p);
                        }
                        while p > 0 && !text[previous(text, p)..p].chars().all(char::is_whitespace)
                        {
                            p = previous(text, p);
                        }
                    } else {
                        while p < text.len() && !text[p..].starts_with(char::is_whitespace) {
                            p = next(text, p);
                        }
                        while p < text.len() && text[p..].starts_with(char::is_whitespace) {
                            p = next(text, p);
                        }
                    }
                    p
                } else if back {
                    previous(text, self.caret)
                } else {
                    next(text, self.caret)
                };
            }
            if !self.range().is_empty() {
                self.replace(text, "");
            }
        }
        if !command && !ctrl {
            let available =
                max.saturating_sub(text.chars().count() - text[self.range()].chars().count());
            let value: String = typed
                .iter()
                .copied()
                .filter(|c| accept(*c))
                .take(available)
                .collect();
            if !value.is_empty() {
                self.replace(text, &value);
            }
        }
    }
    /// The same measured layout is used for paint, hit testing and drag selection.
    fn lines(canvas: &Canvas, text: &str, width: usize, scale: usize) -> Vec<(usize, usize)> {
        let mut rows = Vec::new();
        let mut start = 0;
        for (i, c) in text.char_indices() {
            if i > start && canvas.text_width(&text[start..i + c.len_utf8()], scale) > width {
                rows.push((start, i));
                start = i;
            }
        }
        rows.push((start, text.len()));
        rows
    }
    pub fn click(
        &mut self,
        canvas: &Canvas,
        text: &str,
        rect: [usize; 4],
        scale: usize,
        point: (f32, f32),
        extend: bool,
    ) {
        if self.dragging && !extend {
            return;
        }
        self.normalize(text);
        let rows = Self::lines(canvas, text, rect[2], scale);
        let height = if scale == 1 { 21 } else { 26 };
        let row = self.first_line + ((point.1 - rect[1] as f32).max(0.0) as usize / height);
        let (start, end) = rows[row.min(rows.len() - 1)];
        let x = (point.0 - rect[0] as f32).max(0.0) as usize;
        let mut best = start;
        let mut distance = x;
        for p in text[start..end]
            .char_indices()
            .map(|(i, c)| start + i + c.len_utf8())
        {
            let d = canvas.text_width(&text[start..p], scale).abs_diff(x);
            if d <= distance {
                distance = d;
                best = p;
            }
        }
        self.caret = best;
        if !extend {
            self.anchor = best;
        }
    }
    pub fn drag(
        &mut self,
        canvas: &Canvas,
        text: &str,
        rect: [usize; 4],
        scale: usize,
        window: &Window,
    ) {
        let down = window.get_mouse_down(MouseButton::Left);
        if down {
            if let Some(p) = window.get_mouse_pos(MouseMode::Clamp) {
                if self.dragging {
                    self.click(canvas, text, rect, scale, p, true);
                }
            }
        } else {
            self.dragging = false;
        }
    }
    pub fn start_drag(&mut self) {
        self.dragging = true;
    }
    pub fn vertical(
        &mut self,
        canvas: &Canvas,
        text: &str,
        rect: [usize; 4],
        scale: usize,
        window: &Window,
        keys: &[Key],
    ) {
        if !keys.contains(&Key::Up) && !keys.contains(&Key::Down) {
            return;
        }
        self.normalize(text);
        let rows = Self::lines(canvas, text, rect[2], scale);
        let at = rows
            .iter()
            .position(|(_, end)| self.caret <= *end)
            .unwrap_or(rows.len() - 1);
        let x = canvas.text_width(&text[rows[at].0..self.caret], scale);
        let target = if keys.contains(&Key::Up) {
            at.saturating_sub(1)
        } else {
            (at + 1).min(rows.len() - 1)
        };
        let first = self.first_line;
        self.first_line = 0;
        let shift = window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift);
        self.click(
            canvas,
            text,
            rect,
            scale,
            (
                (rect[0] + x) as f32,
                (rect[1] + target * if scale == 1 { 21 } else { 26 }) as f32,
            ),
            shift,
        );
        self.first_line = first;
    }
    pub fn draw(
        &mut self,
        canvas: &mut Canvas,
        text: &str,
        rect: [usize; 4],
        scale: usize,
        focused: bool,
    ) {
        self.normalize(text);
        let rows = Self::lines(canvas, text, rect[2], scale);
        let height = if scale == 1 { 21 } else { 26 };
        let visible = (rect[3] / height).max(1);
        let at = rows
            .iter()
            .position(|(_, end)| self.caret <= *end)
            .unwrap_or(rows.len() - 1);
        if at < self.first_line {
            self.first_line = at;
        }
        if at >= self.first_line + visible {
            self.first_line = at + 1 - visible;
        }
        self.first_line = self.first_line.min(rows.len().saturating_sub(visible));
        let selected = self.range();
        for (line, (start, end)) in rows.iter().enumerate().skip(self.first_line).take(visible) {
            let y = rect[1] + (line - self.first_line) * height;
            if focused {
                let a = selected.start.max(*start);
                let b = selected.end.min(*end);
                if a < b {
                    let x = canvas.text_width(&text[*start..a], scale);
                    let w = canvas.text_width(&text[*start..b], scale) - x;
                    canvas.fill_rect([rect[0] + x, y, w, height], 0x00da_e9fd);
                }
            }
            canvas.text(rect[0], y, &text[*start..*end], 0x0024_3348, scale);
            if focused && line == at {
                let x = canvas.text_width(&text[*start..self.caret.min(*end)], scale);
                canvas.fill_rect(
                    [
                        rect[0] + x.min(rect[2].saturating_sub(2)),
                        y,
                        2,
                        height.saturating_sub(2),
                    ],
                    0x0025_63eb,
                );
            }
        }
    }
}
fn previous(text: &str, p: usize) -> usize {
    text[..p].char_indices().last().map_or(0, |(i, _)| i)
}
fn next(text: &str, p: usize) -> usize {
    text[p..].chars().next().map_or(p, |c| p + c.len_utf8())
}
pub fn address(c: char) -> bool {
    c.is_ascii_digit() || c == '.'
}
pub fn digits(c: char) -> bool {
    c.is_ascii_digit()
}
pub fn plain(c: char) -> bool {
    !c.is_control()
}
