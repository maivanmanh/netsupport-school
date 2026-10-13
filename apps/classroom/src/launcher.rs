//! Small native launch screen for selecting a role and the Teacher address.
use std::{
    net::Ipv4Addr,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result};
use minifb::{InputCallback, Key, KeyRepeat, MouseButton, MouseMode, Scale, Window, WindowOptions};

use app_core::Role;

const WIDTH: usize = 600;
const HEIGHT: usize = 430;
#[derive(Clone, Copy, Debug)]
pub struct LaunchSelection {
    pub role: Role,
    pub teacher_ip: Ipv4Addr,
}

#[derive(Default)]
struct CharacterBuffer(Arc<Mutex<Vec<char>>>);

impl InputCallback for CharacterBuffer {
    fn add_char(&mut self, character: u32) {
        if let Some(character) = char::from_u32(character) {
            if !character.is_control() {
                if let Ok(mut buffer) = self.0.lock() {
                    buffer.push(character);
                }
            }
        }
    }
}

pub fn show(initial_role: &str, initial_ip: Ipv4Addr) -> Result<Option<LaunchSelection>> {
    show_impl(initial_role, initial_ip, false, None)
}
pub fn show_settings(selection: LaunchSelection) -> Result<Option<LaunchSelection>> {
    show_impl(
        &selection.role.to_string(),
        selection.teacher_ip,
        true,
        None,
    )
}
pub fn show_settings_diagnostic(
    selection: LaunchSelection,
    path: Option<&std::path::Path>,
    accept: bool,
) -> Result<Option<LaunchSelection>> {
    show_impl(
        &selection.role.to_string(),
        selection.teacher_ip,
        true,
        Some((path, accept)),
    )
}
#[allow(clippy::too_many_lines)]
/// Keyboard focus targets in Tab order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Focus {
    Teacher,
    Student,
    Address,
    Submit,
}
const FOCUS_ORDER: [Focus; 4] = [
    Focus::Teacher,
    Focus::Student,
    Focus::Address,
    Focus::Submit,
];
fn next_focus(current: Focus, back: bool) -> Focus {
    let at = FOCUS_ORDER.iter().position(|f| *f == current).unwrap_or(0);
    let n = FOCUS_ORDER.len();
    FOCUS_ORDER[if back { (at + n - 1) % n } else { (at + 1) % n }]
}
#[allow(clippy::too_many_lines)] // one native event loop; keyboard and pointer share state
fn show_impl(
    initial_role: &str,
    initial_ip: Ipv4Addr,
    settings: bool,
    diagnostic: Option<(Option<&std::path::Path>, bool)>,
) -> Result<Option<LaunchSelection>> {
    let mut role = initial_role.parse::<Role>().unwrap_or(Role::Student);
    let mut ip_text = if settings {
        initial_ip
    } else {
        load_teacher_ip(initial_ip)
    }
    .to_string();
    let input_selected = false;
    let mut editor = super::text_editor::Editor::default();
    editor.end(&ip_text);
    // Enter starts/saves immediately; Tab moves between roles, address, button.
    let mut focus = Focus::Submit;
    let mut error = String::new();
    let callback_buffer = Arc::new(Mutex::new(Vec::new()));
    let mut canvas = super::dashboard_view::Canvas::new(WIDTH, HEIGHT);
    draw_screen(
        &mut canvas,
        role,
        &ip_text,
        false,
        false,
        "",
        None,
        false,
        settings,
        None,
    );
    let mut window = Window::new(
        if settings {
            "Classroom | Settings"
        } else {
            "Classroom | Start a session"
        },
        WIDTH,
        HEIGHT,
        WindowOptions {
            visible: false,
            resize: false,
            scale: Scale::X1,
            ..WindowOptions::default()
        },
    )
    .context("cannot create Classroom launch window")?;
    window.set_target_fps(30);
    window.set_input_callback(Box::new(CharacterBuffer(Arc::clone(&callback_buffer))));
    canvas.set_density(window.backing_scale());
    draw_screen(
        &mut canvas,
        role,
        &ip_text,
        false,
        false,
        "",
        None,
        false,
        settings,
        None,
    );
    window.update_with_buffer(&canvas.pixels, canvas.pixel_width(), canvas.pixel_height())?;
    window.set_visible(true);
    if let Some((Some(path), _)) = diagnostic {
        super::teacher_dashboard::snapshot(&canvas, &path.join("settings.ppm"))?;
    }
    let diagnostic_start = std::time::Instant::now();
    let mut previous_mouse_down = false;

    while window.is_open() {
        if diagnostic.is_some()
            && diagnostic_start.elapsed() >= std::time::Duration::from_millis(250)
        {
            if diagnostic.is_some_and(|(_, accept)| accept) {
                return Ok(Some(LaunchSelection {
                    role,
                    teacher_ip: ip_text.parse::<Ipv4Addr>()?,
                }));
            }
            tracing::info!("native Settings diagnostic cancelled without changing profile");
            return Ok(None);
        }
        let mouse_down = window.get_mouse_down(MouseButton::Left);
        if mouse_down && !previous_mouse_down {
            if let Some(p) = window.get_mouse_pos(MouseMode::Discard) {
                if (64.0..536.0).contains(&p.0) && (285.0..335.0).contains(&p.1) {
                    focus = Focus::Address;
                    editor.click(
                        &canvas,
                        &ip_text,
                        [82, 299, 432, 26],
                        2,
                        p,
                        window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift),
                    );
                    editor.start_drag();
                }
            }
        }
        let click = window.left_click();
        let clicked = click.is_some() || (!mouse_down && previous_mouse_down);
        previous_mouse_down = mouse_down;
        if clicked {
            if let Some((mouse_x, mouse_y)) =
                click.or_else(|| window.get_mouse_pos(MouseMode::Discard))
            {
                let (x, y) = pointer_pixel(mouse_x, mouse_y);
                if (64..=286).contains(&x) && (142..=200).contains(&y) {
                    role = Role::Teacher;
                    focus = Focus::Teacher;
                } else if (314..=536).contains(&x) && (142..=200).contains(&y) {
                    role = Role::Student;
                    focus = Focus::Student;
                } else if (64..=536).contains(&x) && (285..=335).contains(&y) {
                    focus = Focus::Address;
                    editor.click(
                        &canvas,
                        &ip_text,
                        [82, 299, 432, 26],
                        2,
                        (mouse_x, mouse_y),
                        window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift),
                    );
                    editor.start_drag();
                    error.clear();
                }
                if (225..=375).contains(&x) && (356..=407).contains(&y) {
                    match ip_text.parse::<Ipv4Addr>() {
                        Ok(teacher_ip) => {
                            save_teacher_ip(teacher_ip)?;
                            return Ok(Some(LaunchSelection { role, teacher_ip }));
                        }
                        Err(_) => "Enter a valid IPv4 address".clone_into(&mut error),
                    }
                }
            }
        }

        let keys = window.get_keys_pressed(KeyRepeat::Yes);
        if keys.contains(&Key::Escape) {
            return Ok(None);
        }
        if keys.contains(&Key::Tab) {
            let back = window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift);
            focus = next_focus(focus, back);
            if focus == Focus::Address {
                editor.select_all(&ip_text);
                error.clear();
            }
        }
        let activate = keys.contains(&Key::Enter)
            || keys.contains(&Key::NumPadEnter)
            || (focus != Focus::Address && keys.contains(&Key::Space));
        match focus {
            Focus::Teacher | Focus::Student => {
                if keys.contains(&Key::Left) || keys.contains(&Key::Right) {
                    focus = if focus == Focus::Teacher {
                        Focus::Student
                    } else {
                        Focus::Teacher
                    };
                    role = if focus == Focus::Teacher {
                        Role::Teacher
                    } else {
                        Role::Student
                    };
                } else if activate {
                    role = if focus == Focus::Teacher {
                        Role::Teacher
                    } else {
                        Role::Student
                    };
                }
            }
            Focus::Submit if activate => match ip_text.parse::<Ipv4Addr>() {
                Ok(teacher_ip) => {
                    save_teacher_ip(teacher_ip)?;
                    return Ok(Some(LaunchSelection { role, teacher_ip }));
                }
                Err(_) => "Enter a valid IPv4 address".clone_into(&mut error),
            },
            _ => {}
        }
        let input_focused = focus == Focus::Address;
        if input_focused {
            if keys.contains(&Key::Enter) || keys.contains(&Key::NumPadEnter) {
                match ip_text.parse::<Ipv4Addr>() {
                    Ok(teacher_ip) => {
                        save_teacher_ip(teacher_ip)?;
                        return Ok(Some(LaunchSelection { role, teacher_ip }));
                    }
                    Err(_) => "Enter a valid IPv4 address".clone_into(&mut error),
                }
            }
            let typed: Vec<_> = callback_buffer.lock().unwrap().drain(..).collect();
            editor.input(
                &mut ip_text,
                &mut window,
                &keys,
                &typed,
                15,
                super::text_editor::address,
            );
            editor.drag(&canvas, &ip_text, [82, 299, 432, 26], 2, &window);
            if !typed.is_empty() || keys.contains(&Key::Backspace) || keys.contains(&Key::Delete) {
                error.clear();
            }
        } else if let Ok(mut input) = callback_buffer.lock() {
            input.clear();
        }

        canvas.set_density(window.backing_scale());
        draw_screen(
            &mut canvas,
            role,
            &ip_text,
            input_focused,
            input_selected,
            &error,
            window.get_mouse_pos(MouseMode::Discard),
            mouse_down,
            settings,
            Some(focus),
        );
        if input_focused {
            editor.draw(&mut canvas, &ip_text, [82, 299, 432, 26], 2, true);
        }
        window
            .update_with_buffer(&canvas.pixels, canvas.pixel_width(), canvas.pixel_height())
            .context("cannot update Classroom launch window")?;
    }
    Ok(None)
}

pub fn load_teacher_ip(default: Ipv4Addr) -> Ipv4Addr {
    settings_path()
        .and_then(|path| super::startup::profile_doc(&path, "teacher_ip").ok())
        .and_then(|doc| doc.read::<String>().ok().flatten())
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(default)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn pointer_pixel(x: f32, y: f32) -> (usize, usize) {
    // MouseMode::Discard guarantees non-negative coordinates within the window.
    (x as usize, y as usize)
}

fn save_teacher_ip(address: Ipv4Addr) -> Result<()> {
    let path = settings_path().context("cannot determine Classroom settings directory")?;
    super::startup::profile_doc(&path, "teacher_ip")?
        .write(&address.to_string())
        .with_context(|| format!("cannot save Teacher IP in {}", path.display()))
}

pub(crate) fn settings_path() -> Option<PathBuf> {
    #[cfg(windows)]
    let root = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("LOCALAPPDATA"))
        .map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let root = std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
    });
    #[cfg(not(any(windows, target_os = "macos")))]
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    root.map(|root| root.join("Classroom").join("teacher-ip.txt"))
}

#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
fn draw_screen(
    canvas: &mut super::dashboard_view::Canvas,
    role: Role,
    ip: &str,
    focused: bool,
    selected: bool,
    error: &str,
    pointer: Option<(f32, f32)>,
    down: bool,
    settings: bool,
    focus: Option<Focus>,
) {
    canvas.pixels.fill(0x00f1_f4f8);
    canvas.fill_rect([0, 0, WIDTH, 104], 0x001c_4d85);
    canvas.text(64, 32, "Classroom", 0x00ff_ffff, 2);
    canvas.text(
        64,
        68,
        if settings {
            "Connection settings"
        } else {
            "Choose a role to get started"
        },
        0x00e1_ecfb,
        1,
    );
    let hover = |x: usize, y: usize, w: usize, h: usize| {
        pointer.is_some_and(|(px, py)| {
            let (px, py) = pointer_pixel(px, py);
            px >= x && py >= y && px < x + w && py < y + h
        })
    };
    for (x, label, chosen) in [
        (64, "Teacher", role == Role::Teacher),
        (314, "Student", role == Role::Student),
    ] {
        let hovered = hover(x, 142, 222, 58);
        canvas.button(
            [x, 142, 222, 58],
            if chosen { 0x00da_e9fd } else { 0x00ef_f2f6 },
            hovered,
            hovered && down,
        );
        canvas.text(x + 18, 161, if chosen { "●" } else { "○" }, 0x001c_4d85, 2);
        canvas.text(x + 51, 161, label, 0x0024_3348, 2);
        let target = if x == 64 {
            Focus::Teacher
        } else {
            Focus::Student
        };
        if focus == Some(target) {
            canvas.focus_ring([x, 142, 222, 58]);
        }
    }
    canvas.text(
        64,
        225,
        if settings {
            "Changes apply after Classroom restarts."
        } else {
            "Student runs quietly until the Teacher starts sharing."
        },
        0x0064_748b,
        1,
    );
    canvas.text(64, 260, "Teacher IPv4 address", 0x0033_4155, 1);
    canvas.button([64, 285, 472, 50], 0x00ff_ffff, false, true);
    if focused {
        canvas.fill_rect([64, 285, 472, 2], 0x0025_63eb);
        canvas.fill_rect([64, 333, 472, 2], 0x0025_63eb);
    }
    if selected {
        canvas.fill_rect([80, 296, ip.len() * 12 + 4, 28], 0x00dc_eafb);
    }
    if !focused {
        canvas.text(82, 299, ip, 0x0024_3348, 2);
    }
    if !error.is_empty() {
        canvas.text(64, 338, error, 0x00b4_2309, 1);
    }
    let hovered = hover(225, 356, 150, 51);
    canvas.button([225, 356, 150, 51], 0x0025_63eb, hovered, hovered && down);
    if focus == Some(Focus::Submit) {
        canvas.focus_ring([225, 356, 150, 51]);
    }
    canvas.text(
        if settings { 241 } else { 275 },
        372,
        if settings { "Save & restart" } else { "Start" },
        0x00ff_ffff,
        1,
    );
}

pub(crate) fn glyph(character: char) -> [u8; 7] {
    match character {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [14, 4, 4, 4, 4, 4, 14],
        'J' => [7, 2, 2, 2, 18, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 25, 21, 19, 17, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        '[' => [14, 8, 8, 8, 8, 8, 14],
        ']' => [14, 2, 2, 2, 2, 2, 14],
        '>' => [16, 8, 4, 2, 4, 8, 16],
        '+' => [0, 4, 4, 31, 4, 4, 0],
        '#' => [10, 31, 10, 10, 31, 10, 0],
        '=' => [0, 31, 0, 31, 0, 0, 0],
        '|' => [4, 4, 4, 4, 4, 4, 4],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 31],
        '/' => [1, 2, 2, 4, 8, 8, 16],
        ':' => [0, 12, 12, 0, 12, 12, 0],
        _ => [0; 7],
    }
}
