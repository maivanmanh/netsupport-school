//! Native console layout/hit-testing, independent of capture/network workers.
use control_transport::room::RoomStudent;
use std::collections::{HashMap, HashSet};

const HEADER: usize = 194;
const FOOTER: usize = 34;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Grid,
    List,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Share,
    ShareSelected,
    Settings,
    Stop,
    Grid,
    List,
    Previous,
    Next,
    Select(usize),
    Inspect,
    CloseInspect,
}
#[derive(Default)]
pub struct Selection {
    pub ids: HashSet<String>,
    pub inspect: bool,
}
#[derive(Clone, Copy)]
struct Rect {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}
impl Rect {
    fn contains(self, x: usize, y: usize) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
}
fn toolbar(w: usize) -> [(Rect, Action); 9] {
    let actions = [
        Action::Share,
        Action::Stop,
        Action::ShareSelected,
        Action::Inspect,
        Action::Grid,
        Action::List,
        Action::Previous,
        Action::Next,
        Action::Settings,
    ];
    let narrow = w < 1000;
    let slot = w.saturating_sub(32) / 6;
    let positions = [
        (16, 164),
        (190, 130),
        (330, 140),
        (480, 140),
        (w.saturating_sub(230), 95),
        (w.saturating_sub(126), 95),
    ];
    std::array::from_fn(|index| {
        let rect = if index == 8 {
            Rect {
                x: w.saturating_sub(116),
                y: 40,
                w: 100,
                h: 27,
            }
        } else if index >= 6 {
            Rect {
                x: w.saturating_sub(if index == 6 { 218 } else { 118 }),
                y: 0,
                w: 90,
                h: 30,
            }
        } else {
            let (x, width) = if narrow {
                (16 + slot * index, slot.saturating_sub(4))
            } else {
                positions[index]
            };
            Rect {
                x,
                y: 73,
                w: width,
                h: 62,
            }
        };
        (rect, actions[index].clone())
    })
}
#[must_use]
pub fn capacity(mode: Mode, w: usize, h: usize) -> usize {
    let height = h.saturating_sub(HEADER + FOOTER + 20);
    if mode == Mode::List {
        (height / 78).max(1)
    } else {
        (w.saturating_sub(32) / 280).clamp(1, 6) * (height / (grid_height(w) + 12)).max(1)
    }
}
fn grid_height(w: usize) -> usize {
    let cols = (w.saturating_sub(32) / 280).clamp(1, 6);
    (w.saturating_sub(32) / cols)
        .saturating_sub(12)
        .min(288)
        .saturating_sub(20)
        * 9
        / 16
        + 94
}
fn row_rect(mode: Mode, w: usize, index: usize) -> Rect {
    if mode == Mode::List {
        Rect {
            x: 16,
            y: HEADER + index * 78,
            w: w.saturating_sub(32),
            h: 70,
        }
    } else {
        let cols = (w.saturating_sub(32) / 280).clamp(1, 6);
        let cw = w.saturating_sub(32) / cols;
        Rect {
            x: 16 + (index % cols) * cw,
            y: HEADER + (index / cols) * (grid_height(w) + 12),
            w: cw.saturating_sub(12).min(288),
            h: grid_height(w),
        }
    }
}
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn hit(
    mode: Mode,
    w: usize,
    h: usize,
    x: f32,
    y: f32,
    visible: usize,
    inspecting: bool,
) -> Option<Action> {
    let (x, y) = (x as usize, y as usize);
    if inspecting {
        return Some(Action::CloseInspect);
    }
    for (mut rect, action) in toolbar(w) {
        if matches!(action, Action::Previous | Action::Next) {
            rect.y = h.saturating_sub(FOOTER);
        }
        if rect.contains(x, y) {
            return Some(action);
        }
    }
    (0..visible)
        .find(|&i| row_rect(mode, w, i).contains(x, y))
        .map(Action::Select)
}

pub struct Canvas {
    pub pixels: Vec<u32>,
    pub width: usize,
    pub height: usize,
    pub hover: Option<Action>,
    pub pressed: Option<Action>,
    density: usize,
    font: Option<fontdue::Font>,
    glyphs: HashMap<(char, u16), (fontdue::Metrics, Vec<u8>)>,
}
impl Canvas {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            pixels: vec![0; width * height],
            width,
            height,
            hover: None,
            pressed: None,
            density: 1,
            font: system_font(),
            glyphs: HashMap::new(),
        }
    }
    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
        self.pixels
            .resize(self.pixel_width() * self.pixel_height(), 0);
    }
    pub fn pixel_width(&self) -> usize {
        self.width * self.density
    }
    pub fn pixel_height(&self) -> usize {
        self.height * self.density
    }
    pub fn set_density(&mut self, density: usize) -> bool {
        let density = density.clamp(1, 2);
        if self.density == density {
            return false;
        }
        self.density = density;
        self.pixels
            .resize(self.pixel_width() * self.pixel_height(), 0);
        self.glyphs.clear();
        true
    }
    /// Thin custom outline for borderless native notifications (logical pixels).
    pub(crate) fn notification_border(&mut self, width: usize, height: usize) {
        let color = 0x00b8_c6d1;
        self.fill_rect([0, 0, width, 1], color);
        self.fill_rect([0, height.saturating_sub(1), width, 1], color);
        self.fill_rect([0, 0, 1, height], color);
        self.fill_rect([width.saturating_sub(1), 0, 1, height], color);
    }
    pub(crate) fn fill_rect(&mut self, area: [usize; 4], color: u32) {
        self.rect(
            Rect {
                x: area[0],
                y: area[1],
                w: area[2],
                h: area[3],
            },
            color,
        );
    }
    pub(crate) fn button(&mut self, area: [usize; 4], base: u32, hovered: bool, pressed: bool) {
        let [x, y, w, h] = area;
        let blend = |a: u32, b: u32, t: u32| -> u32 {
            [0, 8, 16].into_iter().fold(0, |out, shift| {
                out | (((((a >> shift) & 255) * (100 - t) + ((b >> shift) & 255) * t) / 100)
                    << shift)
            })
        };
        let base = if hovered {
            blend(base, 0x007c_b9ff, 15)
        } else {
            base
        };
        if !pressed {
            self.fill_rect([x + 1, y + 2, w, h], 0x00c1_c9d3);
        }
        let (top, bottom) = if pressed {
            (blend(base, 0, 16), base)
        } else {
            (blend(base, 0x00ff_ffff, 16), blend(base, 0, 7))
        };
        for row in 0..h {
            self.fill_rect(
                [x, y + row, w, 1],
                blend(
                    top,
                    bottom,
                    u32::try_from(row * 100 / h.max(1)).unwrap_or(100),
                ),
            );
        }
        let border = blend(base, 0, 25);
        self.fill_rect([x, y, w, 1], if pressed { border } else { 0x00ff_ffff });
        self.fill_rect([x, y, 1, h], if pressed { border } else { 0x00ff_ffff });
        self.fill_rect([x, y + h.saturating_sub(1), w, 1], border);
        self.fill_rect([x + w.saturating_sub(1), y, 1, h], border);
    }
    /// Keyboard-focus indicator drawn just outside `area`.
    pub(crate) fn focus_ring(&mut self, area: [usize; 4]) {
        const RING: u32 = 0x0025_63eb;
        let [x, y, w, h] = area;
        let (x, y, w, h) = (x.saturating_sub(3), y.saturating_sub(3), w + 6, h + 6);
        self.fill_rect([x, y, w, 2], RING);
        self.fill_rect([x, y + h - 2, w, 2], RING);
        self.fill_rect([x, y, 2, h], RING);
        self.fill_rect([x + w - 2, y, 2, h], RING);
    }
    fn rect(&mut self, r: Rect, color: u32) {
        let stride = self.pixel_width();
        let x = r.x.min(self.width) * self.density;
        let end = r.x.saturating_add(r.w).min(self.width) * self.density;
        for y in r.y.min(self.height) * self.density
            ..r.y.saturating_add(r.h).min(self.height) * self.density
        {
            self.pixels[y * stride + x..y * stride + end].fill(color);
        }
    }
    pub(crate) fn text(&mut self, x: usize, y: usize, value: &str, color: u32, scale: usize) {
        if self.font.is_some() {
            self.font_text(x, y, value, color, if scale == 1 { 14 } else { 20 });
            return;
        }
        let max = self.width.saturating_sub(x) / (6 * scale);
        for (i, c) in value.chars().take(max).enumerate() {
            for (row, bits) in super::launcher::glyph(c.to_ascii_uppercase())
                .iter()
                .enumerate()
            {
                for col in 0..5 {
                    if bits & (1 << (4 - col)) != 0 {
                        self.rect(
                            Rect {
                                x: x + (i * 6 + col) * scale,
                                y: y + row * scale,
                                w: scale,
                                h: scale,
                            },
                            color,
                        );
                    }
                }
            }
        }
    }
    /// Logical width of `value` as drawn by [`Canvas::text`] at `scale`.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub(crate) fn text_width(&self, value: &str, scale: usize) -> usize {
        let Some(font) = self.font.as_ref() else {
            return value.chars().count() * 6 * scale;
        };
        let size = if scale == 1 { 14.0 } else { 20.0 };
        let mut width = 0.0_f32;
        let mut previous = None;
        for c in value.chars() {
            if let Some(left) = previous {
                width += font.horizontal_kern(left, c, size).unwrap_or(0.0);
            }
            previous = Some(c);
            width += font.metrics(c, size).advance_width;
        }
        width.ceil().max(0.0) as usize
    }
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_possible_wrap
    )]
    fn font_text(&mut self, x: usize, y: usize, value: &str, color: u32, size: u16) {
        let size = size * u16::try_from(self.density).unwrap_or(1);
        let x = x * self.density;
        let y = y * self.density;
        let width = self.pixel_width();
        let height = self.pixel_height();
        let font = self.font.as_ref().unwrap();
        let mut cursor = x as f32;
        let mut previous = None;
        for c in value.chars() {
            if let Some(left) = previous {
                cursor += font
                    .horizontal_kern(left, c, f32::from(size))
                    .unwrap_or(0.0);
            }
            previous = Some(c);
            let (metrics, mask) = self
                .glyphs
                .entry((c, size))
                .or_insert_with(|| font.rasterize(c, f32::from(size)));
            if cursor + metrics.advance_width > width as f32 {
                break;
            }
            let left = cursor as i64 + i64::from(metrics.xmin);
            let top = y as i64 + i64::from(size) - metrics.height as i64 - i64::from(metrics.ymin);
            for row in 0..metrics.height {
                for col in 0..metrics.width {
                    let (px, py) = (left + col as i64, top + row as i64);
                    if px < 0 || py < 0 || px >= width as i64 || py >= height as i64 {
                        continue;
                    }
                    let alpha = u32::from(mask[row * metrics.width + col]);
                    let index = py as usize * width + px as usize;
                    let bg = self.pixels[index];
                    let mut blended = 0;
                    for shift in [0, 8, 16] {
                        blended |= ((((color >> shift) & 255) * alpha
                            + ((bg >> shift) & 255) * (255 - alpha)
                            + 127)
                            / 255)
                            << shift;
                    }
                    self.pixels[index] = blended;
                }
            }
            cursor += metrics.advance_width;
        }
    }
    fn image(&mut self, r: Rect, row: &RoomStudent) {
        self.draw_image(r, row, true);
    }
    fn image_fit(&mut self, r: Rect, row: &RoomStudent) {
        self.draw_image(r, row, false);
    }
    fn draw_image(&mut self, r: Rect, row: &RoomStudent, fill: bool) {
        if r.w == 0 || r.h == 0 {
            return;
        }
        self.rect(r, 0x0015_2030);
        let Some(thumb) = row.thumbnail.as_ref() else {
            self.text(
                r.x + 16,
                r.y + r.h / 2,
                "Waiting for screen",
                0x0094_a3b8,
                1,
            );
            return;
        };
        let r = Rect {
            x: r.x * self.density,
            y: r.y * self.density,
            w: r.w * self.density,
            h: r.h * self.density,
        };
        let (tw, th) = (thumb.width, thumb.height);
        // Center crop for thumbnails; inspection retains the complete desktop.
        let width_limited = r.w * th <= r.h * tw;
        let (dw, dh) = if width_limited == fill {
            ((r.h * tw / th).max(1), r.h)
        } else {
            (r.w, (r.w * th / tw).max(1))
        };
        let (vw, vh) = (dw.min(r.w), dh.min(r.h));
        let (x, y) = (r.x + (r.w - vw) / 2, r.y + (r.h - vh) / 2);
        let (crop_x, crop_y) = (dw.saturating_sub(r.w) / 2, dh.saturating_sub(r.h) / 2);
        let stride = self.pixel_width();
        let height = self.pixel_height();
        for dy in 0..vh {
            for dx in 0..vw {
                if x + dx < stride && y + dy < height {
                    self.pixels[(y + dy) * stride + x + dx] =
                        thumb.pixels[((dy + crop_y) * th / dh).min(th - 1) * tw
                            + ((dx + crop_x) * tw / dw).min(tw - 1)];
                }
            }
        }
    }
}
fn system_font() -> Option<fontdue::Font> {
    let mut paths = Vec::new();
    if let Some(windows) = std::env::var_os("WINDIR") {
        paths.push(std::path::PathBuf::from(windows).join("Fonts/segoeui.ttf"));
    }
    paths.push("/System/Library/Fonts/SFNS.ttf".into());
    paths.push("/System/Library/Fonts/Helvetica.ttc".into());
    paths.push("/System/Library/Fonts/Supplemental/Arial.ttf".into());
    paths.push("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf".into());
    paths.into_iter().find_map(|p| {
        let font =
            fontdue::Font::from_bytes(std::fs::read(&p).ok()?, fontdue::FontSettings::default())
                .ok()?;
        tracing::info!(path=%p.display(), name=?font.name(), "native UI font selected");
        Some(font)
    })
}

fn state(row: &RoomStudent) -> (&'static str, u32) {
    if !row.online {
        ("Offline", 0x0080_8999)
    } else if row.monitoring_state == "paused" {
        ("Watching Teacher", 0x0025_63eb)
    } else if row.monitoring_state == "unavailable" {
        ("Capture unavailable", 0x00b4_5309)
    } else if row
        .thumbnail
        .as_ref()
        .is_some_and(|t| t.received_at.elapsed().as_secs() > 7)
    {
        ("Screen stale", 0x00b4_5309)
    } else if row.thumbnail.is_some() {
        ("Connected", 0x0015_8050)
    } else {
        ("Connecting screen", 0x0064_748b)
    }
}
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn draw(
    canvas: &mut Canvas,
    rows: &[RoomStudent],
    mode: Mode,
    page: usize,
    selection: &Selection,
    sharing: bool,
    status: &str,
    ip: &str,
) {
    let (w, h) = (canvas.width, canvas.height);
    canvas.pixels.fill(0x00f1_f4f8);
    canvas.rect(
        Rect {
            x: 0,
            y: 0,
            w,
            h: 36,
        },
        0x001c_4d85,
    );
    canvas.text(18, 11, "Classroom", 0x00ff_ffff, 2);
    canvas.text(
        w.saturating_sub(310),
        14,
        &format!("Teacher  {ip}"),
        0x00dc_e8fa,
        1,
    );
    canvas.rect(
        Rect {
            x: 0,
            y: 36,
            w,
            h: 116,
        },
        0x00ff_ffff,
    );
    canvas.rect(
        Rect {
            x: 8,
            y: 39,
            w: 95,
            h: 25,
        },
        0x00e8_eff9,
    );
    canvas.text(25, 47, "Home", 0x001c_4d85, 1);
    canvas.text(130, 47, "View", 0x0033_4155, 1);
    canvas.text(212, 47, "Files", 0x0094_a3b8, 1);
    canvas.text(300, 47, "Devices", 0x0094_a3b8, 1);
    for (rect, action) in toolbar(w).into_iter().take(6) {
        let enabled = match action {
            Action::Share => !sharing,
            Action::ShareSelected => {
                !sharing
                    && rows
                        .iter()
                        .any(|r| r.online && selection.ids.contains(&r.instance_id))
            }
            Action::Stop => sharing,
            Action::Inspect => selection.ids.len() == 1,
            _ => true,
        };
        let chosen = matches!(
            (&action, mode),
            (Action::Grid, Mode::Grid) | (Action::List, Mode::List)
        );
        let primary = action == Action::Share && enabled;
        let hovered = enabled && canvas.hover.as_ref() == Some(&action);
        let pressed = hovered && canvas.pressed.as_ref() == Some(&action);
        canvas.button(
            [rect.x, rect.y, rect.w, rect.h],
            if primary {
                0x0025_63eb
            } else if chosen {
                0x00df_eafd
            } else if enabled {
                0x00ee_f1f5
            } else {
                0x00f4_f5f7
            },
            hovered,
            pressed,
        );
        let (icon, label) = match action {
            Action::Share => (">", "Share class"),
            Action::Stop => ("[]", "Stop sharing"),
            Action::ShareSelected => (">", "Share selected"),
            Action::Inspect => ("+", "Inspect"),
            Action::Grid => ("#", "Grid"),
            _ => ("=", "List"),
        };
        let color = if primary {
            0x00ff_ffff
        } else if enabled {
            0x002d_3e54
        } else {
            0x0094_a3b8
        };
        let offset = usize::from(pressed);
        canvas.text(rect.x + 14 + offset, rect.y + 10 + offset, icon, color, 2);
        let label = if w < 1000 {
            match action {
                Action::Share => "Class",
                Action::ShareSelected => "Selected",
                Action::Stop => "Stop",
                Action::Inspect => "View",
                _ => label,
            }
        } else {
            label
        };
        canvas.text(rect.x + 14 + offset, rect.y + 40 + offset, label, color, 1);
    }
    canvas.text(66, 142, "Presentation", 0x0064_748b, 1);
    canvas.text(495, 142, "Monitor", 0x0064_748b, 1);
    let settings = Rect {
        x: w.saturating_sub(116),
        y: 40,
        w: 100,
        h: 27,
    };
    let hover = !sharing && canvas.hover == Some(Action::Settings);
    canvas.button(
        [settings.x, settings.y, settings.w, settings.h],
        0x00e9_edf2,
        hover,
        hover && canvas.pressed == Some(Action::Settings),
    );
    canvas.text(
        settings.x + 14,
        settings.y + 5,
        "Settings...",
        if sharing { 0x0094_a3b8 } else { 0x0033_4155 },
        1,
    );
    let online = rows.iter().filter(|r| r.online).count();
    canvas.text(
        18,
        169,
        &format!(
            "Students: {online} online   {} offline   {} selected",
            rows.len() - online,
            selection.ids.len()
        ),
        0x0033_4155,
        1,
    );
    let cap = capacity(mode, w, h);
    let pages = rows.len().div_ceil(cap).max(1);
    if rows.is_empty() {
        canvas.text(40, HEADER + 80, "Waiting for students", 0x0033_4155, 2);
        canvas.text(
            40,
            HEADER + 120,
            "Student agents connect automatically to this Teacher.",
            0x0064_748b,
            1,
        );
    }
    for (i, row) in rows.iter().skip(page * cap).take(cap).enumerate() {
        let r = row_rect(mode, w, i);
        let selected = selection.ids.contains(&row.instance_id);
        canvas.rect(r, if selected { 0x0025_63eb } else { 0x00d8_e0eb });
        canvas.rect(
            Rect {
                x: r.x + 2,
                y: r.y + 2,
                w: r.w.saturating_sub(4),
                h: r.h.saturating_sub(4),
            },
            0x00ff_ffff,
        );
        let (label, color) = state(row);
        if mode == Mode::Grid {
            let name: String = row
                .hostname
                .chars()
                .take(r.w.saturating_sub(40) / 6)
                .collect();
            canvas.text(
                r.x + 12,
                r.y + 13,
                if selected { "[X]" } else { "[ ]" },
                0x0025_63eb,
                1,
            );
            canvas.text(r.x + 40, r.y + 13, &name, 0x0024_3348, 1);
            canvas.image(
                Rect {
                    x: r.x + 10,
                    y: r.y + 34,
                    w: r.w.saturating_sub(20),
                    h: r.w.saturating_sub(20) * 9 / 16,
                },
                row,
            );
            canvas.text(
                r.x + 12,
                r.y + 44 + r.w.saturating_sub(20) * 9 / 16,
                label,
                color,
                1,
            );
            canvas.text(
                r.x + 12,
                r.y + 66 + r.w.saturating_sub(20) * 9 / 16,
                &row.endpoint.ip().to_string(),
                0x0064_748b,
                1,
            );
        } else {
            canvas.text(
                r.x + 12,
                r.y + 31,
                if selected { "[X]" } else { "[ ]" },
                0x0025_63eb,
                1,
            );
            canvas.image(
                Rect {
                    x: r.x + 44,
                    y: r.y + 8,
                    w: 96,
                    h: 54,
                },
                row,
            );
            let name: String = row
                .hostname
                .chars()
                .take(w.saturating_sub(400) / 12)
                .collect();
            canvas.text(r.x + 160, r.y + 18, &name, 0x0024_3348, 2);
            canvas.text(
                r.x + 160,
                r.y + 46,
                &row.endpoint.ip().to_string(),
                0x0064_748b,
                1,
            );
            canvas.text(w.saturating_sub(205), r.y + 30, label, color, 1);
        }
    }
    canvas.rect(
        Rect {
            x: 0,
            y: h.saturating_sub(FOOTER),
            w,
            h: FOOTER,
        },
        0x00e5_eaf2,
    );
    canvas.text(
        16,
        h.saturating_sub(22),
        &status
            .chars()
            .take(w.saturating_sub(400) / 7)
            .collect::<String>(),
        0x0033_4155,
        1,
    );
    canvas.text(
        w.saturating_sub(380),
        h.saturating_sub(22),
        &format!("Page {} / {pages}", page + 1),
        0x0064_748b,
        1,
    );
    canvas.text(
        w.saturating_sub(210),
        h.saturating_sub(22),
        "Previous     Next",
        0x0025_63eb,
        1,
    );
    if selection.inspect {
        if let Some(row) = rows.iter().find(|r| selection.ids.contains(&r.instance_id)) {
            let r = Rect {
                x: 32,
                y: 56,
                w: w.saturating_sub(64),
                h: h.saturating_sub(112),
            };
            canvas.rect(r, 0x00ff_ffff);
            canvas.text(
                r.x + 20,
                r.y + 20,
                &format!("{}  |  SNAPSHOT", row.hostname),
                0x0024_3348,
                2,
            );
            canvas.image_fit(
                Rect {
                    x: r.x + 20,
                    y: r.y + 55,
                    w: r.w.saturating_sub(40),
                    h: r.h.saturating_sub(100),
                },
                row,
            );
            let (label, color) = state(row);
            canvas.text(
                r.x + 20,
                r.y + r.h - 25,
                &format!("{label}   ·   Refresh every 2s   ·   Click or Esc to close"),
                color,
                1,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[allow(clippy::many_single_char_names)]
    fn retina_font_is_rasterized_at_backing_resolution() {
        let mut canvas = Canvas::new(120, 50);
        canvas.set_density(2);
        assert_eq!((canvas.pixel_width(), canvas.pixel_height()), (240, 100));
        canvas.text(4, 4, "Retina", 0x00ff_ffff, 1);
        if canvas.font.is_some() {
            // A doubled bitmap has identical pixels in every 2x2 block. Real
            // high-resolution glyph rasterization contains distinct subpixels.
            assert!((0..99).step_by(2).any(|y| (0..239).step_by(2).any(|x| {
                let i = y * 240 + x;
                canvas.pixels[i] != canvas.pixels[i + 1]
                    || canvas.pixels[i] != canvas.pixels[i + 240]
            })));
        }
        canvas.resize(80, 40);
        assert_eq!(canvas.pixels.len(), 160 * 80);
        assert!(canvas.set_density(1));
        assert_eq!(canvas.pixels.len(), 80 * 40);
    }
    #[test]
    #[allow(clippy::many_single_char_names)]
    fn grid_and_list_thumbnails_fill_without_letterboxing() {
        use control_transport::room::RoomThumbnail;
        use std::{sync::Arc, time::Instant};
        let row = RoomStudent {
            client_id: 1,
            instance_id: "test".into(),
            hostname: "Student".into(),
            endpoint: "127.0.0.1:47810".parse().unwrap(),
            online: true,
            monitoring_state: "ready".into(),
            thumbnail: Some(Arc::new(RoomThumbnail {
                jpeg: Vec::new(),
                width: 4,
                height: 3,
                pixels: vec![0x00ab_cd12; 12],
                received_at: Instant::now(),
                sequence: 1,
            })),
        };
        for density in [1, 2] {
            let mut canvas = Canvas::new(1200, 760);
            canvas.set_density(density);
            for mode in [Mode::Grid, Mode::List] {
                draw(
                    &mut canvas,
                    std::slice::from_ref(&row),
                    mode,
                    0,
                    &Selection::default(),
                    false,
                    "Ready",
                    "127.0.0.1",
                );
                let r = row_rect(mode, 1200, 0);
                let (x, y, w, h) = if mode == Mode::Grid {
                    let width = r.w - 20;
                    (r.x + 10, r.y + 34, width, width * 9 / 16)
                } else {
                    (r.x + 44, r.y + 8, 96, 54)
                };
                assert!((w * 9).abs_diff(h * 16) < 16);
                for py in y * density..(y + h) * density {
                    for px in x * density..(x + w) * density {
                        assert_eq!(canvas.pixels[py * canvas.pixel_width() + px], 0x00ab_cd12);
                    }
                }
            }
        }
    }
    #[test]
    #[allow(clippy::many_single_char_names)]
    fn grid_list_capacity_and_hit_targets_follow_window_size() {
        assert_eq!(capacity(Mode::Grid, 1200, 760), 8);
        assert!(capacity(Mode::List, 1200, 760) > 0);
        assert_eq!(
            hit(Mode::Grid, 1200, 760, 40.0, 220.0, 2, false),
            Some(Action::Select(0))
        );
        assert_eq!(
            hit(Mode::Grid, 1200, 760, 120.0, 100.0, 0, false),
            Some(Action::Share)
        );
        assert_eq!(
            hit(Mode::Grid, 1200, 760, 210.0, 100.0, 0, false),
            Some(Action::Stop)
        );
        let mut canvas = Canvas::new(1200, 760);
        draw(
            &mut canvas,
            &[],
            Mode::Grid,
            0,
            &Selection::default(),
            false,
            "READY",
            "127.0.0.1",
        );
        assert_eq!(canvas.pixels.len(), 1200 * 760);
    }
}
