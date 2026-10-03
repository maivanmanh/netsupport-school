//! Rust-only Student window for incoming files: compact, bottom-right,
//! movable (native title bar), collapsible. It shows each file's name and
//! progress, cannot be closed while a transfer is active, and offers "Show"
//! (reveal the saved file in Finder/Explorer) once a file has arrived.
use super::{
    classroom_tools::{Inbox, Transfer, TransferState},
    dashboard_view::Canvas,
};
use anyhow::Result;
use minifb::{MouseButton, MouseMode, Window, WindowOptions};
use std::{path::Path, sync::Arc, time::Instant};

const TITLE: &str = "Classroom · Files from teacher";
const WIDTH: usize = 360;
const HEADER: usize = 36;
const ROW: usize = 56;
const FOOTER: usize = 44;
const MAX_ROWS: usize = 3;
const MARGIN: isize = 16;
const PAD: usize = 14;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Hit {
    Collapse,
    Close,
    Show(usize),
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
            HEADER + self.rows * ROW + FOOTER
        }
    }
    #[allow(clippy::unused_self)] // keeps all hit areas on Layout
    fn collapse_button(self) -> [usize; 4] {
        [WIDTH - 34, 6, 26, 24]
    }
    fn close_button(self) -> [usize; 4] {
        [WIDTH - PAD - 80, HEADER + self.rows * ROW + 8, 80, 28]
    }
    #[allow(clippy::unused_self)]
    fn show_button(self, row: usize) -> [usize; 4] {
        [WIDTH - PAD - 62, HEADER + row * ROW + 14, 62, 28]
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
        window.set_outer_origin(ax + aw - w - MARGIN, ay + ah - h - MARGIN);
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
                inbox.forget_finished();
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
            (0..shown.len())
                .find(|&i| {
                    finished_path(&shown[i].state).is_some() && inside(layout.show_button(i))
                })
                .map(Hit::Show)
        };
        self.hover = hit(self.window.get_mouse_pos(MouseMode::Discard));
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
            Some(Hit::Close) if !active => {
                inbox.forget_finished();
                return Ok(false);
            }
            Some(Hit::Show(i)) => {
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
                self.window.show_inactive();
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
        let active: Vec<_> = transfers.iter().filter(|t| t.state.active()).collect();
        let c = &mut self.canvas;
        c.pixels.fill(0x00f7_f9fb);
        c.fill_rect([0, 0, WIDTH, HEADER], 0x0017_3747);
        let title = if layout.collapsed {
            summary(transfers)
        } else {
            "Files from your teacher".to_owned()
        };
        let title = ellipsize(c, &title, WIDTH - PAD - 44);
        c.text(PAD, 10, &title, 0x00ff_ffff, 1);
        let collapse = layout.collapse_button();
        c.button(
            collapse,
            0x0029_4b5e,
            self.hover == Some(Hit::Collapse),
            false,
        );
        let glyph = if layout.collapsed { "+" } else { "–" };
        let gw = c.text_width(glyph, 1);
        c.text(collapse[0] + (26 - gw) / 2, 9, glyph, 0x00ff_ffff, 1);
        if layout.collapsed {
            return;
        }
        for (i, t) in rows(transfers).into_iter().enumerate() {
            let y = HEADER + i * ROW;
            if i > 0 {
                c.fill_rect([PAD, y, WIDTH - 2 * PAD, 1], 0x00dd_e3e8);
            }
            let name = ellipsize(c, &t.name, WIDTH - 2 * PAD - 76);
            c.text(PAD, y + 9, &name, 0x0022_3342, 1);
            let (status, color) = match &t.state {
                TransferState::Waiting => ("Waiting…".to_owned(), 0x0061_7180),
                TransferState::Receiving => {
                    (format!("{} / {}", size(t.done), size(t.size)), 0x0061_7180)
                }
                TransferState::Saved(_) => ("Saved to Desktop".to_owned(), 0x0015_803d),
                TransferState::AlreadyPresent(_) => ("Already on Desktop".to_owned(), 0x0015_803d),
                TransferState::Failed(e) => (format!("Failed: {e}"), 0x00b4_2309),
                TransferState::Cancelled => ("Cancelled by teacher".to_owned(), 0x00b4_2309),
            };
            if t.state.active() {
                // Percentage beside the name, full-width progress bar below.
                let bar = [PAD, y + 34, WIDTH - 2 * PAD, 6];
                c.fill_rect(bar, 0x00dd_e3e8);
                let pct = t
                    .done
                    .saturating_mul(100)
                    .checked_div(t.size)
                    .map_or(100, |p| usize::try_from(p).unwrap_or(100));
                c.fill_rect(
                    [bar[0], bar[1], bar[2] * pct.min(100) / 100, 6],
                    0x000d_7785,
                );
                let label = if t.state == TransferState::Waiting {
                    "…".to_owned()
                } else {
                    format!("{pct}%")
                };
                let lw = c.text_width(&label, 1);
                c.text(WIDTH - PAD - lw, y + 9, &label, 0x0022_3342, 1);
                let _ = (status, color);
            } else {
                let status = ellipsize(c, &status, WIDTH - 2 * PAD - 76);
                c.text(PAD, y + 30, &status, color, 1);
                if finished_path(&t.state).is_some() {
                    let area = layout.show_button(i);
                    c.button(area, 0x00e5_ebf0, self.hover == Some(Hit::Show(i)), false);
                    let w = c.text_width("Show", 1);
                    c.text(
                        area[0] + (area[2] - w) / 2,
                        area[1] + 6,
                        "Show",
                        0x0022_3342,
                        1,
                    );
                }
            }
        }
        let close = layout.close_button();
        let footer = if !active.is_empty() {
            "Receiving… close when done".to_owned()
        } else if transfers.len() > MAX_ROWS {
            format!("+ {} earlier", transfers.len() - MAX_ROWS)
        } else {
            String::new()
        };
        c.text(PAD, close[1] + 6, &footer, 0x0061_7180, 1);
        let enabled = active.is_empty();
        c.button(
            close,
            if enabled { 0x000d_7785 } else { 0x00b8_c4cc },
            enabled && self.hover == Some(Hit::Close),
            false,
        );
        let w = c.text_width("Close", 1);
        c.text(
            close[0] + (80 - w) / 2,
            close[1] + 6,
            "Close",
            0x00ff_ffff,
            1,
        );
    }
}

/// Owns the optional window: opens it when a file is offered, drops it when
/// the Student closes it after every transfer has finished.
#[derive(Default)]
pub struct FileProgress {
    window: Option<FileWindow>,
    seen: usize,
}
impl FileProgress {
    pub fn pump(&mut self, inbox: &Arc<Inbox>) {
        let transfers = inbox.transfers();
        if self.window.is_none() {
            // Reopen only for a new offer, not for leftovers already closed.
            let fresh = transfers.len() > self.seen || transfers.iter().any(|t| t.state.active());
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
        self.seen = inbox.transfers().len();
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

/// Truncates with "…" to fit `max` logical pixels.
fn ellipsize(canvas: &Canvas, text: &str, max: usize) -> String {
    if canvas.text_width(text, 1) <= max {
        return text.to_owned();
    }
    let mut out: String = text.to_owned();
    while !out.is_empty() && canvas.text_width(&format!("{out}…"), 1) > max {
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
        assert_eq!(Layout::new(1, false).height(), HEADER + ROW + FOOTER);
        assert_eq!(Layout::new(9, false).rows, MAX_ROWS);
        assert!(Layout::new(9, false).height() < 260);
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
        let short = ellipsize(&c, name, 120);
        assert!(short.ends_with('…'));
        assert!(c.text_width(&short, 1) <= 120);
        assert_eq!(ellipsize(&c, "x.pdf", 120), "x.pdf");
    }
}
