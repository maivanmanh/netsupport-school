//! Rust-only Student window for incoming files: compact, bottom-right,
//! borderless, collapsible. It shows each file's name and progress and cannot
//! be closed while a transfer is active. A file that has arrived is a row the
//! Student can click anywhere (it highlights and the cursor becomes a pointing
//! hand) to reveal it in Finder/Explorer. Closing hides the window but keeps the
//! list; "Clear" in the footer removes the files that are done.
use super::{
    classroom_tools::{Inbox, Transfer, TransferState},
    dashboard_view::Canvas,
};
use anyhow::Result;
use minifb::{CursorStyle, MouseButton, MouseMode, Window, WindowOptions};
use std::{path::Path, sync::Arc, time::Instant};

const TITLE: &str = "Classroom · Files from teacher";
const WIDTH: usize = 360;
const HEADER: usize = 48;
const ROW: usize = 64;
/// Footer: "+ N earlier" on the left, "Clear" on the right.
const FOOT: usize = 34;
const MAX_ROWS: usize = 3;
const MARGIN: isize = 16;
const PAD: usize = 16;
/// Left edge of the text column (after the file icon).
const TEXT_X: usize = PAD + 46;

const BACKGROUND: u32 = 0x00ff_ffff;
const ACCENT: u32 = 0x000d_7785;
const INK: u32 = 0x001f_2937;
const MUTED: u32 = 0x006b_7280;
const LINE: u32 = 0x00e5_e7eb;
const HOVER_ROW: u32 = 0x00ea_f4f6;
const GOOD: u32 = 0x0015_803d;
const BAD: u32 = 0x00b4_2309;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Hit {
    Collapse,
    Close,
    /// Forget the files that are done (keeps anything still arriving).
    Clear,
    /// A finished file's row (anywhere on it): reveal the file.
    Row(usize),
}

/// Pure geometry so sizing is testable without a window.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Layout {
    rows: usize,
    collapsed: bool,
}
impl Layout {
    fn new(count: usize, collapsed: bool) -> Self {
        Self {
            rows: count.clamp(1, MAX_ROWS),
            collapsed,
        }
    }
    fn height(self) -> usize {
        if self.collapsed {
            HEADER
        } else {
            HEADER + self.rows * ROW + FOOT
        }
    }
    #[allow(clippy::unused_self)] // keeps all hit areas on Layout
    fn collapse_button(self) -> [usize; 4] {
        [WIDTH - 74, 10, 28, 28]
    }
    #[allow(clippy::unused_self)]
    fn close_button(self) -> [usize; 4] {
        [WIDTH - 42, 10, 28, 28]
    }
    /// The footer's "Clear" link.
    fn clear_button(self) -> [usize; 4] {
        [WIDTH - PAD - 96, HEADER + self.rows * ROW + 4, 96, 26]
    }
    /// The whole row, edge to edge.
    #[allow(clippy::unused_self)]
    fn row(self, row: usize) -> [usize; 4] {
        [0, HEADER + row * ROW, WIDTH, ROW]
    }
}

/// A past instant so the next pump draws immediately.
fn stale() -> Instant {
    let now = Instant::now();
    now.checked_sub(std::time::Duration::from_secs(1))
        .unwrap_or(now)
}
pub struct FileWindow {
    window: Window,
    canvas: Canvas,
    layout: Layout,
    previous_down: bool,
    hover: Option<Hit>,
    last_draw: Instant,
    /// Shown at least once since (re)opening; hidden afterwards = OS close.
    shown: bool,
}
fn open(layout: Layout) -> Result<Window> {
    let mut window = Window::new(
        TITLE,
        WIDTH,
        layout.height(),
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
    // The OS close button hides; pump() decides whether that may close.
    window.set_close_to_hide(true);
    Ok(window)
}
/// Newest first, as displayed.
fn rows(transfers: &[Transfer]) -> Vec<&Transfer> {
    transfers.iter().rev().take(MAX_ROWS).collect()
}
impl FileWindow {
    fn new(count: usize) -> Result<Self> {
        let layout = Layout::new(count, false);
        let mut window = open(layout)?;
        let mut canvas = Canvas::new(WIDTH, layout.height());
        canvas.set_density(window.backing_scale());
        let [ax, ay, aw, ah] = window.work_area();
        let [_, _, w, h] = window.outer_frame();
        window.set_outer_origin(ax + (aw - w - MARGIN).max(0), ay + (ah - h - MARGIN).max(0));
        Ok(Self {
            window,
            canvas,
            layout,
            previous_down: false,
            hover: None,
            last_draw: stale(),
            shown: false,
        })
    }
    /// Reopen at a new size keeping the bottom-right corner where the
    /// Student left it (minifb cannot resize a fixed window).
    fn relayout(&mut self, layout: Layout) -> Result<()> {
        let [x, y, w, h] = self.window.outer_frame();
        let mut window = open(layout)?;
        let [_, _, nw, nh] = window.outer_frame();
        window.set_outer_origin(x + w - nw, y + h - nh);
        self.canvas.resize(WIDTH, layout.height());
        self.window = window;
        self.layout = layout;
        self.previous_down = false;
        self.hover = None;
        self.last_draw = stale();
        self.shown = false;
        Ok(())
    }
    /// Returns false once the window should be dropped.
    #[allow(clippy::cast_precision_loss)]
    fn pump(&mut self, inbox: &Inbox) -> Result<bool> {
        let transfers = inbox.transfers();
        let active = transfers.iter().any(|t| t.state.active());
        if !self.window.is_open() || (self.shown && !self.window.is_visible()) {
            if active {
                // Closing is not allowed mid-transfer: keep it, collapsed.
                self.relayout(Layout::new(transfers.len(), true))?;
            } else {
                return Ok(false);
            }
        }
        if transfers.is_empty() {
            return Ok(false);
        }
        let wanted = Layout::new(transfers.len(), self.layout.collapsed);
        if wanted != self.layout {
            self.relayout(wanted)?;
        }
        let shown = rows(&transfers);
        let layout = self.layout;
        let any_done = transfers.iter().any(|t| !t.state.active());
        let hit = |pos: Option<(f32, f32)>| -> Option<Hit> {
            let (x, y) = pos?;
            let inside = |[bx, by, bw, bh]: [usize; 4]| {
                (bx as f32..(bx + bw) as f32).contains(&x)
                    && (by as f32..(by + bh) as f32).contains(&y)
            };
            if inside(layout.collapse_button()) {
                return Some(Hit::Collapse);
            }
            if layout.collapsed {
                return None;
            }
            if inside(layout.close_button()) {
                return Some(Hit::Close);
            }
            if any_done && inside(layout.clear_button()) {
                return Some(Hit::Clear);
            }
            (0..shown.len())
                .find(|&i| finished_path(&shown[i].state).is_some() && inside(layout.row(i)))
                .map(Hit::Row)
        };
        self.hover = hit(self.window.get_mouse_pos(MouseMode::Discard));
        // Everything the Student can click shows the pointing hand.
        let clickable = match self.hover {
            Some(Hit::Collapse | Hit::Row(_) | Hit::Clear) => true,
            Some(Hit::Close) => !active,
            None => false,
        };
        self.window.set_cursor_style(if clickable {
            CursorStyle::OpenHand // "pointing hand" in the vendored minifb
        } else {
            CursorStyle::Arrow
        });
        let down = self.window.get_mouse_down(MouseButton::Left);
        let click = self.window.left_click().or_else(|| {
            (!down && self.previous_down)
                .then(|| self.window.get_mouse_pos(MouseMode::Discard))
                .flatten()
        });
        self.previous_down = down;
        match hit(click) {
            Some(Hit::Collapse) => {
                self.relayout(Layout::new(transfers.len(), !self.layout.collapsed))?;
                return Ok(true);
            }
            Some(Hit::Close) if !active => return Ok(false),
            Some(Hit::Clear) => {
                inbox.forget_finished();
                // Nothing left to show once the done files are gone.
                if inbox.transfers().is_empty() {
                    return Ok(false);
                }
                return Ok(true);
            }
            Some(Hit::Row(i)) => {
                if let Some(path) = finished_path(&shown[i].state) {
                    reveal(path);
                }
            }
            _ => {}
        }
        if self.last_draw.elapsed() >= std::time::Duration::from_millis(100) {
            self.canvas.set_density(self.window.backing_scale());
            self.draw(&transfers);
            self.window.update_with_buffer(
                &self.canvas.pixels,
                self.canvas.pixel_width(),
                self.canvas.pixel_height(),
            )?;
            if !self.shown {
                self.window.show_inactive_topmost();
                self.shown = true;
            }
            self.last_draw = Instant::now();
        } else {
            self.window.update();
        }
        Ok(true)
    }
    #[allow(clippy::too_many_lines)] // one immediate-mode paint pass
    fn draw(&mut self, transfers: &[Transfer]) {
        let layout = self.layout;
        let hover = self.hover;
        let active_any = transfers.iter().any(|t| t.state.active());
        let any_done = transfers.iter().any(|t| !t.state.active());
        let c = &mut self.canvas;
        c.pixels.fill(BACKGROUND);
        c.fill_rect([0, 0, WIDTH, 3], ACCENT);
        let title = if layout.collapsed {
            summary(transfers)
        } else {
            "Files from your teacher".to_owned()
        };
        let title = ellipsize(c, &title, WIDTH - PAD - 90, 15);
        c.text_bold(PAD, 15, &title, INK, 15);
        // Minimise / restore and close: quiet icon buttons that light up on hover.
        let collapse = layout.collapse_button();
        if hover == Some(Hit::Collapse) {
            c.round_rect(collapse, 6, 0x00ee_f2f6);
        }
        let (cx, cy) = (collapse[0] as f32 + 14.0, collapse[1] as f32 + 14.0);
        let ink = if hover == Some(Hit::Collapse) { INK } else { MUTED };
        c.line((cx - 5.0, cy), (cx + 5.0, cy), 1.6, ink);
        if layout.collapsed {
            c.line((cx, cy - 5.0), (cx, cy + 5.0), 1.6, ink);
        }
        let close = layout.close_button();
        let close_on = hover == Some(Hit::Close) && !active_any;
        if close_on {
            c.round_rect(close, 6, 0x00fd_e8e6);
        }
        let ink = if active_any {
            0x00c5_ccd3
        } else if close_on {
            BAD
        } else {
            MUTED
        };
        let (cx, cy) = (close[0] as f32 + 14.0, close[1] as f32 + 14.0);
        c.line((cx - 4.5, cy - 4.5), (cx + 4.5, cy + 4.5), 1.6, ink);
        c.line((cx - 4.5, cy + 4.5), (cx + 4.5, cy - 4.5), 1.6, ink);
        if layout.collapsed {
            c.notification_border(WIDTH, layout.height());
            return;
        }
        c.fill_rect([0, HEADER - 1, WIDTH, 1], LINE);
        for (i, t) in rows(transfers).into_iter().enumerate() {
            let y = HEADER + i * ROW;
            let finished = finished_path(&t.state).is_some();
            if hover == Some(Hit::Row(i)) {
                c.fill_rect([0, y, WIDTH, ROW], HOVER_ROW);
            } else if i > 0 {
                c.fill_rect([PAD, y, WIDTH - 2 * PAD, 1], LINE);
            }
            draw_file_icon(c, PAD, y + 14, &t.state);
            let side = if t.state.active() { 54 } else { 8 };
            let name = ellipsize(c, &t.name, WIDTH - TEXT_X - PAD - side, 15);
            c.text_px(TEXT_X, y + 11, &name, INK, 15);
            let (status, color) = match &t.state {
                TransferState::Waiting => ("Waiting…".to_owned(), MUTED),
                TransferState::Receiving => (format!("{} of {}", size(t.done), size(t.size)), MUTED),
                TransferState::Saved(_) | TransferState::AlreadyPresent(_)
                    if hover == Some(Hit::Row(i)) =>
                {
                    ("Click to show in folder".to_owned(), ACCENT)
                }
                TransferState::Saved(_) => ("Saved to Desktop".to_owned(), GOOD),
                TransferState::AlreadyPresent(_) => ("Already on Desktop".to_owned(), GOOD),
                TransferState::Failed(e) => (format!("Failed: {e}"), BAD),
                TransferState::Cancelled => ("Cancelled by teacher".to_owned(), BAD),
            };
            if t.state.active() {
                let pct = t
                    .done
                    .saturating_mul(100)
                    .checked_div(t.size)
                    .map_or(100, |p| usize::try_from(p).unwrap_or(100))
                    .min(100);
                let label = if t.state == TransferState::Waiting {
                    "…".to_owned()
                } else {
                    format!("{pct}%")
                };
                let lw = c.text_width_px(&label, 13);
                c.text_px(WIDTH - PAD - lw, y + 12, &label, MUTED, 13);
                c.text_px(TEXT_X, y + 31, &status, color, 13);
                let bar = [TEXT_X, y + 51, WIDTH - TEXT_X - PAD, 5];
                c.round_rect(bar, 2, LINE);
                if pct > 0 {
                    c.round_rect([bar[0], bar[1], (bar[2] * pct / 100).max(5), 5], 2, ACCENT);
                }
            } else {
                let status = ellipsize(c, &status, WIDTH - TEXT_X - PAD, 13);
                c.text_px(TEXT_X, y + 33, &status, color, 13);
            }
        }
        let footer = HEADER + layout.rows * ROW;
        c.fill_rect([PAD, footer, WIDTH - 2 * PAD, 1], LINE);
        if transfers.len() > MAX_ROWS {
            let extra = format!("+ {} earlier", transfers.len() - MAX_ROWS);
            c.text_px(PAD, footer + 11, &extra, MUTED, 13);
        }
        if any_done {
            let area = layout.clear_button();
            let hot = hover == Some(Hit::Clear);
            if hot {
                c.round_rect(area, 6, 0x00ee_f2f6);
            }
            let color = if hot { ACCENT } else { MUTED };
            let label = "Clear";
            let w = c.text_width_px(label, 13);
            // A small bin glyph before the label.
            let gx = area[0] + (area[2] - w - 22) / 2;
            let gy = area[1] + 6;
            c.round_rect([gx + 2, gy + 4, 9, 10], 1, color);
            c.fill_rect([gx, gy + 2, 13, 2], color);
            c.fill_rect([gx + 4, gy, 5, 2], color);
            c.text_px(gx + 20, area[1] + 5, label, color, 13);
        }
        c.notification_border(WIDTH, layout.height());
    }
}

/// A 36 px badge: the file for transfers in flight, a check once it arrived,
/// a cross when it did not.
fn draw_file_icon(c: &mut Canvas, x: usize, y: usize, state: &TransferState) {
    let (cx, cy) = (x as f32 + 18.0, y as f32 + 18.0);
    match state {
        TransferState::Saved(_) | TransferState::AlreadyPresent(_) => {
            c.disc(cx, cy, 18.0, 0x00dc_fce7);
            c.line((cx - 6.0, cy + 0.5), (cx - 1.5, cy + 5.0), 2.2, GOOD);
            c.line((cx - 1.5, cy + 5.0), (cx + 6.5, cy - 4.5), 2.2, GOOD);
        }
        TransferState::Failed(_) | TransferState::Cancelled => {
            c.disc(cx, cy, 18.0, 0x00fd_e8e6);
            c.line((cx - 5.0, cy - 5.0), (cx + 5.0, cy + 5.0), 2.2, BAD);
            c.line((cx - 5.0, cy + 5.0), (cx + 5.0, cy - 5.0), 2.2, BAD);
        }
        _ => {
            c.disc(cx, cy, 18.0, 0x00dd_eff1);
            c.round_rect([x + 12, y + 9, 13, 18], 2, ACCENT);
            c.line((cx - 3.0, cy - 1.5), (cx + 3.0, cy - 1.5), 1.2, 0x00ff_ffff);
            c.line((cx - 3.0, cy + 2.0), (cx + 3.0, cy + 2.0), 1.2, 0x00ff_ffff);
        }
    }
}

/// Owns the optional window: opens it when a file is offered, drops it when
/// the Student closes it after every transfer has finished.
#[derive(Default)]
pub struct FileProgress {
    window: Option<FileWindow>,
    /// Ids already shown, so a reopened window means something new arrived.
    seen: std::collections::HashSet<String>,
}
impl FileProgress {
    pub fn pump(&mut self, inbox: &Arc<Inbox>) {
        let transfers = inbox.transfers();
        if self.window.is_none() {
            // Reopen only for a new offer, not for leftovers already closed.
            let fresh = transfers.iter().any(|t| !self.seen.contains(&t.id) || t.state.active());
            if !transfers.is_empty() && fresh {
                match FileWindow::new(transfers.len()) {
                    Ok(window) => self.window = Some(window),
                    Err(error) => tracing::warn!(%error, "file progress window unavailable"),
                }
            }
        }
        if let Some(window) = self.window.as_mut() {
            match window.pump(inbox) {
                Ok(true) => {}
                Ok(false) => self.window = None,
                Err(error) => {
                    tracing::warn!(%error, "file progress window failed");
                    self.window = None;
                }
            }
        }
        self.seen = inbox.transfers().into_iter().map(|t| t.id).collect();
    }
}

fn finished_path(state: &TransferState) -> Option<&Path> {
    match state {
        TransferState::Saved(p) | TransferState::AlreadyPresent(p) => Some(p),
        _ => None,
    }
}

fn summary(transfers: &[Transfer]) -> String {
    let active: Vec<_> = transfers.iter().filter(|t| t.state.active()).collect();
    if active.is_empty() {
        return match transfers.len() {
            1 => format!("Received {}", transfers[0].name),
            n => format!("{n} files received"),
        };
    }
    let (done, total) = active
        .iter()
        .fold((0_u64, 0_u64), |(d, s), t| (d + t.done, s + t.size));
    let pct = (done * 100).checked_div(total).unwrap_or(0);
    match active.len() {
        1 => format!("{pct}% · {}", active[0].name),
        n => format!("Receiving {n} files · {pct}%"),
    }
}

fn size(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let b = bytes as f64;
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.0} KB", b / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", b / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", b / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Truncates with "…" to fit `max` logical pixels at pixel size `px`.
fn ellipsize(canvas: &Canvas, text: &str, max: usize, px: u16) -> String {
    if canvas.text_width_px(text, px) <= max {
        return text.to_owned();
    }
    let mut out: String = text.to_owned();
    while !out.is_empty() && canvas.text_width_px(&format!("{out}…"), px) > max {
        out.pop();
    }
    format!("{out}…")
}

/// Opens the containing folder with the file selected; never opens the file.
fn reveal(path: &Path) {
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("/usr/bin/open")
        .arg("-R")
        .arg(path)
        .spawn();
    #[cfg(windows)]
    let result = {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{}\"", path.display()))
            .spawn()
    };
    #[cfg(not(any(windows, target_os = "macos")))]
    let result = std::process::Command::new("xdg-open")
        .arg(path.parent().unwrap_or(path))
        .spawn();
    if let Err(error) = result {
        tracing::warn!(%error, "could not reveal received file");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn transfer(name: &str, state: TransferState, done: u64) -> Transfer {
        Transfer {
            id: name.into(),
            name: name.into(),
            size: 100,
            done,
            state,
        }
    }
    #[test]
    fn layout_is_compact_and_bounded() {
        assert_eq!(Layout::new(1, true).height(), HEADER);
        assert_eq!(Layout::new(1, false).height(), HEADER + ROW + FOOT);
        assert_eq!(Layout::new(9, false).rows, MAX_ROWS);
        assert!(Layout::new(9, false).height() < 320);
        // A row is clickable edge to edge, and the header buttons stay inside the window.
        let layout = Layout::new(2, false);
        let [x, y, w, h] = layout.clear_button();
        assert!(x + w <= WIDTH && y >= HEADER + 2 * ROW && y + h <= layout.height());
        assert_eq!(layout.row(1), [0, HEADER + ROW, WIDTH, ROW]);
        for [x, y, w, h] in [layout.collapse_button(), layout.close_button()] {
            assert!(x + w <= WIDTH && y + h <= HEADER);
        }
    }
    #[test]
    fn summary_reports_progress_then_completion() {
        let mut list = vec![transfer("a.pdf", TransferState::Receiving, 50)];
        assert_eq!(summary(&list), "50% · a.pdf");
        list.push(transfer("b.pdf", TransferState::Waiting, 0));
        assert_eq!(summary(&list), "Receiving 2 files · 25%");
        list[0].state = TransferState::Saved("/x/a.pdf".into());
        list[1].state = TransferState::AlreadyPresent("/x/b.pdf".into());
        assert_eq!(summary(&list), "2 files received");
        assert!(rows(&list)[0].name == "b.pdf", "newest first");
    }
    #[test]
    fn long_names_are_ellipsized() {
        let c = Canvas::new(1, 1);
        let name = "a very long lecture handout filename that never ends.pdf";
        let short = ellipsize(&c, name, 120, 15);
        assert!(short.ends_with('…'));
        assert!(c.text_width_px(&short, 15) <= 120);
        assert_eq!(ellipsize(&c, "x.pdf", 120, 15), "x.pdf");
    }
}
