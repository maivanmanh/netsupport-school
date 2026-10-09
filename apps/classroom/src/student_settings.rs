//! Native, role-fixed Student operational settings. Media remains Rust-only.
use anyhow::{Context, Result};
use classroom_config::Config;
use minifb::{InputCallback, Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};
use std::{
    net::Ipv4Addr,
    path::Path,
    sync::{Arc, Mutex},
};

const WIDTH: usize = 820;
const HEIGHT: usize = 600;
const PAGES: [&str; 3] = ["Connection", "Presentation", "Diagnostics"];

fn settings_path(profile: &Path) -> std::path::PathBuf {
    profile.with_file_name("student-settings.toml")
}
pub fn apply_saved(config: &mut Config, profile: &Path) -> Result<()> {
    let path = settings_path(profile);
    if path.exists() {
        config
            .apply_saved(&std::fs::read_to_string(&path)?)
            .with_context(|| format!("invalid Student settings: {}", path.display()))?;
    }
    Ok(())
}
fn overlay(config: &Config) -> String {
    format!("[network]\nteacher_ip = \"{}\"\nteacher_ip_mode = \"explicit\"\ncontrol_port = {}\nmedia_port = {}\nfeedback_port = {}\n\n[video]\njitter_buffer_ms = {}\nnack_repair_ms = {}\n\n[app]\nlog_level = {:?}\n",
        config.network.teacher_ip, config.network.control_port, config.network.media_port,
        config.network.feedback_port, config.video.jitter_buffer_ms, config.video.nack_repair_ms,
        config.app.log_level)
}
pub fn save(config: &Config, profile: &Path) -> Result<()> {
    if let Some(parent) = profile.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(settings_path(profile), overlay(config))?;
    Ok(())
}

struct Input(Arc<Mutex<Vec<char>>>);
impl InputCallback for Input {
    fn add_char(&mut self, value: u32) {
        if let Some(c) = char::from_u32(value).filter(|c| c.is_ascii_digit() || *c == '.') {
            if let Ok(mut chars) = self.0.lock() {
                chars.push(c);
            }
        }
    }
}
struct Field {
    label: &'static str,
    value: String,
    page: usize,
}
fn fields(config: &Config, teacher: Ipv4Addr) -> Vec<Field> {
    vec![
        Field {
            label: "Teacher IPv4 address",
            value: teacher.to_string(),
            page: 0,
        },
        Field {
            label: "Teacher control port",
            value: config.network.control_port.to_string(),
            page: 0,
        },
        Field {
            label: "Local UDP media port",
            value: config.network.media_port.to_string(),
            page: 0,
        },
        Field {
            label: "Local feedback port",
            value: config.network.feedback_port.to_string(),
            page: 0,
        },
        Field {
            label: "Jitter hold (ms, 1-100)",
            value: config.video.jitter_buffer_ms.to_string(),
            page: 1,
        },
        Field {
            label: "Packet repair (ms, 1-100)",
            value: config.video.nack_repair_ms.to_string(),
            page: 1,
        },
    ]
}
fn candidate(config: &Config, fields: &[Field], level: &str) -> Result<Config> {
    let mut updated = config.clone();
    updated.network.teacher_ip = fields[0]
        .value
        .parse()
        .context("Enter a valid Teacher IPv4 address")?;
    updated.network.control_port = fields[1]
        .value
        .parse()
        .context("Enter a valid control port")?;
    updated.network.media_port = fields[2]
        .value
        .parse()
        .context("Enter a valid media port")?;
    updated.network.feedback_port = fields[3]
        .value
        .parse()
        .context("Enter a valid feedback port")?;
    anyhow::ensure!(
        [
            updated.network.control_port,
            updated.network.media_port,
            updated.network.feedback_port
        ]
        .iter()
        .all(|port| *port >= 1024),
        "Use ports from 1024 to 65535"
    );
    updated.video.jitter_buffer_ms = fields[4]
        .value
        .parse()
        .context("Enter a valid jitter hold")?;
    updated.video.nack_repair_ms = fields[5]
        .value
        .parse()
        .context("Enter a valid repair budget")?;
    anyhow::ensure!(
        (1..=100).contains(&updated.video.jitter_buffer_ms),
        "Jitter hold must be 1-100 ms"
    );
    level.clone_into(&mut updated.app.log_level);
    // Shared validation checks port collisions, repair bounds and existing invariants.
    updated.apply_saved(&overlay(&updated.clone()))?;
    Ok(updated)
}

fn open_location(config: &Config, privacy: Option<&str>) -> Result<()> {
    #[cfg(target_os = "macos")]
    if let Some(kind) = privacy {
        std::process::Command::new("open")
            .arg(format!(
                "x-apple.systempreferences:com.apple.preference.security?Privacy_{kind}"
            ))
            .spawn()?;
        return Ok(());
    }
    let _ = privacy;
    let folder = super::diagnostics::log_folder(config)?;
    std::fs::create_dir_all(&folder)?;
    std::process::Command::new(if cfg!(target_os = "macos") {
        "open"
    } else {
        "explorer.exe"
    })
    .arg(folder)
    .spawn()?;
    Ok(())
}
#[allow(clippy::too_many_lines)] // native event loop with shared keyboard/pointer actions
pub fn show(
    config: &Config,
    teacher: Ipv4Addr,
    desktop: &mut super::desktop_tray::StudentDesktop,
) -> Result<Option<Config>> {
    let mut items = fields(config, teacher);
    let mut page = 0;
    let mut focused = None;
    let mut editors: Vec<_> = items
        .iter()
        .map(|f| {
            let mut editor = super::text_editor::Editor::default();
            editor.end(&f.value);
            editor
        })
        .collect();
    let mut level = config.app.log_level.clone();
    let mut error = String::new();
    let chars = Arc::new(Mutex::new(Vec::new()));
    let mut canvas = super::dashboard_view::Canvas::new(WIDTH, HEIGHT);
    let mut window = Window::new(
        "Classroom Student | Settings",
        WIDTH,
        HEIGHT,
        WindowOptions {
            visible: false,
            resize: false,
            ..Default::default()
        },
    )?;
    window.set_input_callback(Box::new(Input(Arc::clone(&chars))));
    window.set_target_fps(30);
    window.set_visible(true);
    let mut previous_down = false;
    while window.is_open() {
        // A modal settings window must still honor Exit and Restart from the tray.
        desktop.pump("Student settings", false);
        desktop.settings_requested = false;
        if desktop.exit_requested {
            return Ok(None);
        }
        let keys = window.get_keys_pressed(KeyRepeat::Yes);
        if keys.contains(&Key::Escape) {
            return Ok(None);
        }
        let down = window.get_mouse_down(MouseButton::Left);
        if down && !previous_down {
            if let Some((x, y)) = window.get_mouse_pos(MouseMode::Discard) {
                for (row, (i, field)) in items
                    .iter()
                    .enumerate()
                    .filter(|(_, f)| f.page == page)
                    .enumerate()
                {
                    let top = 165 + row * 76;
                    if (212.0..790.0).contains(&x) && (top as f32..(top + 40) as f32).contains(&y) {
                        focused = Some(i);
                        editors[i].click(
                            &canvas,
                            &field.value,
                            [224, top + 11, 554, 26],
                            2,
                            (x, y),
                            window.is_key_down(Key::LeftShift)
                                || window.is_key_down(Key::RightShift),
                        );
                        editors[i].start_drag();
                    }
                }
            }
        }
        let clicked = window.left_click().or_else(|| {
            (!down && previous_down)
                .then(|| window.get_mouse_pos(MouseMode::Discard))
                .flatten()
        });
        previous_down = down;
        let mut submit = keys.contains(&Key::Enter) || keys.contains(&Key::NumPadEnter);
        if let Some((x, y)) = clicked {
            if x < 184.0 && (110.0..260.0).contains(&y) {
                page = if y < 160.0 {
                    0
                } else if y < 210.0 {
                    1
                } else {
                    2
                };
                focused = None;
            } else if (212.0..790.0).contains(&x) {
                for (row, (i, _)) in items
                    .iter()
                    .enumerate()
                    .filter(|(_, f)| f.page == page)
                    .enumerate()
                {
                    let top = 165.0 + f32::from(u16::try_from(row).unwrap_or(0)) * 76.0;
                    if (top..top + 40.0).contains(&y) {
                        focused = Some(i);
                        editors[i].click(
                            &canvas,
                            &items[i].value,
                            [224, top as usize + 11, 554, 26],
                            2,
                            (x, y),
                            window.is_key_down(Key::LeftShift)
                                || window.is_key_down(Key::RightShift),
                        );
                        editors[i].start_drag();
                    }
                }
                if page == 1 && (350.0..393.0).contains(&y) {
                    let wifi = x >= 496.0;
                    items[4].value = if wifi { "50" } else { "20" }.into();
                    items[5].value = if wifi { "100" } else { "25" }.into();
                }
                if page == 2 {
                    if (165.0..207.0).contains(&y) {
                        level = match level.as_str() {
                            "info" => "debug",
                            "debug" => "warn",
                            _ => "info",
                        }
                        .into();
                    } else if (237.0..280.0).contains(&y) {
                        if let Err(e) = open_location(config, None) {
                            error = e.to_string();
                        }
                    } else if (309.0..352.0).contains(&y) && cfg!(target_os = "macos") {
                        if let Err(e) = open_location(config, Some("ScreenCapture")) {
                            error = e.to_string();
                        }
                    }
                }
            }
            if (625.0..790.0).contains(&x) && (535.0..578.0).contains(&y) {
                submit = true;
            }
            if (515.0..610.0).contains(&x) && (535.0..578.0).contains(&y) {
                return Ok(None);
            }
        }
        if keys.contains(&Key::Tab) {
            let indices: Vec<usize> = items
                .iter()
                .enumerate()
                .filter(|(_, f)| f.page == page)
                .map(|(i, _)| i)
                .collect();
            if !indices.is_empty() {
                let next = focused
                    .and_then(|i| indices.iter().position(|n| *n == i))
                    .map_or(0, |i| {
                        if window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift)
                        {
                            (i + indices.len() - 1) % indices.len()
                        } else {
                            (i + 1) % indices.len()
                        }
                    });
                focused = Some(indices[next]);
                editors[indices[next]].select_all(&items[indices[next]].value);
            }
        }
        if let Some(i) = focused {
            let typed: Vec<_> = chars.lock().unwrap().drain(..).collect();
            editors[i].input(
                &mut items[i].value,
                &mut window,
                &keys,
                &typed,
                15,
                if i == 0 {
                    super::text_editor::address
                } else {
                    super::text_editor::digits
                },
            );
            let row = items.iter().take(i).filter(|f| f.page == page).count();
            editors[i].drag(
                &canvas,
                &items[i].value,
                [224, 176 + row * 76, 554, 26],
                2,
                &window,
            );
        } else if let Ok(mut buffer) = chars.lock() {
            buffer.clear();
        }
        if submit {
            match candidate(config, &items, &level) {
                Ok(updated) => return Ok(Some(updated)),
                Err(e) => error = e.to_string(),
            }
        }
        canvas.set_density(window.backing_scale());
        canvas.pixels.fill(0x00f3_f5f9);
        canvas.fill_rect([0, 0, WIDTH, 88], 0x001c_4d85);
        canvas.text(24, 26, "Student settings", 0x00ff_ffff, 2);
        canvas.text(24, 61, "Classroom", 0x00de_eafa, 1);
        canvas.fill_rect([0, 88, 184, HEIGHT - 88], 0x00e8_edf4);
        for (i, label) in PAGES.iter().enumerate() {
            if i == page {
                canvas.fill_rect([12, 110 + i * 50, 160, 43], 0x00d1_e2f8);
            }
            canvas.text(24, 126 + i * 50, label, 0x0024_3348, 1);
        }
        canvas.text(212, 110, PAGES[page], 0x0024_3348, 2);
        for (row, (i, field)) in items
            .iter()
            .enumerate()
            .filter(|(_, f)| f.page == page)
            .enumerate()
        {
            canvas.text(212, 143 + row * 76, field.label, 0x0033_4155, 1);
            let rect = [212, 165 + row * 76, 578, 40];
            canvas.button(rect, 0x00ff_ffff, false, false);
            if focused == Some(i) {
                canvas.focus_ring(rect);
            }
            editors[i].draw(
                &mut canvas,
                &field.value,
                [224, rect[1] + 11, 554, 26],
                2,
                focused == Some(i),
            );
        }
        match page {
            0 => {
                canvas.text(
                    212,
                    475,
                    "Uses the network route to Teacher automatically.",
                    0x0064_748b,
                    1,
                );
                canvas.text(
                    212,
                    491,
                    "Explicit launch flags override saved settings.",
                    0x0064_748b,
                    1,
                );
            }
            1 => {
                for (x, label) in [(212, "Wired LAN: 20 / 25 ms"), (496, "Wi-Fi: 50 / 100 ms")] {
                    canvas.button([x, 350, 278, 43], 0x00da_e9fd, false, false);
                    canvas.text(x + 12, 365, label, 0x001c_4d85, 1);
                }
                canvas.text(
                    212,
                    419,
                    "Teacher presentation fills the primary display.",
                    0x0064_748b,
                    1,
                );
                canvas.text(
                    212,
                    439,
                    "Local cursor hidden; Teacher pointer stays visible.",
                    0x0064_748b,
                    1,
                );
                canvas.text(
                    212,
                    465,
                    "Wi-Fi preset tolerates more reordering; it adds delay.",
                    0x0064_748b,
                    1,
                );
                canvas.text(
                    212,
                    485,
                    "It cannot eliminate packet loss or CPU overload.",
                    0x0064_748b,
                    1,
                );
            }
            _ => {
                canvas.text(212, 143, "Log detail (click to change)", 0x0033_4155, 1);
                canvas.button([212, 165, 578, 42], 0x00ff_ffff, false, false);
                canvas.text(224, 180, &level, 0x0024_3348, 1);
                canvas.button([212, 237, 578, 43], 0x00da_e9fd, false, false);
                canvas.text(224, 252, "Open logs folder", 0x001c_4d85, 1);
                if cfg!(target_os = "macos") {
                    canvas.button([212, 309, 578, 43], 0x00da_e9fd, false, false);
                    canvas.text(224, 324, "Screen Recording permissions", 0x001c_4d85, 1);
                    canvas.text(212, 386, "Observe needs Screen Recording.", 0x0064_748b, 1);
                    canvas.text(
                        212,
                        406,
                        "Control also needs Accessibility permission.",
                        0x0064_748b,
                        1,
                    );
                }
                canvas.text(
                    212,
                    449,
                    "Exit Classroom stops all Student workers.",
                    0x0064_748b,
                    1,
                );
                canvas.text(
                    212,
                    469,
                    "Opening the app starts them again.",
                    0x0064_748b,
                    1,
                );
            }
        }
        if !error.is_empty() {
            canvas.text(
                212,
                512,
                &error.chars().take(92).collect::<String>(),
                0x00b4_2309,
                1,
            );
        }
        canvas.button([515, 535, 95, 43], 0x00e0_e7ef, false, false);
        canvas.text(536, 550, "Cancel", 0x0024_3348, 1);
        canvas.button([625, 535, 165, 43], 0x0025_63eb, false, false);
        canvas.text(640, 550, "Save & restart", 0x00ff_ffff, 1);
        canvas.text(
            212,
            584,
            "Apply after restart. Esc cancels. Tab moves between fields.",
            0x0064_748b,
            1,
        );
        window.update_with_buffer(&canvas.pixels, canvas.pixel_width(), canvas.pixel_height())?;
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_reject_collision_and_persist_without_changing_media_policy() {
        let mut config = Config::load(None).unwrap().0;
        let mut edited = fields(&config, Ipv4Addr::new(192, 168, 50, 94));
        edited[2].value = edited[1].value.clone();
        assert!(candidate(&config, &edited, "info").is_err());
        edited[2].value = "48710".into();
        edited[4].value = "50".into();
        edited[5].value = "100".into();
        let updated = candidate(&config, &edited, "debug").unwrap();
        config.apply_saved(&overlay(&updated)).unwrap();
        assert_eq!(config.network.teacher_ip, Ipv4Addr::new(192, 168, 50, 94));
        assert_eq!(config.video.jitter_buffer_ms, 50);
        assert_eq!(config.video.nack_repair_ms, 100);
        assert_eq!(config.video.width, 1920);
        assert_eq!(config.video.target_bitrate_kbps, 6000);
    }
}
