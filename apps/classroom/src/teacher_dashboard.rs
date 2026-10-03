//! Native Teacher console. Media and monitoring workers are independent of UI.
use super::dashboard_view::{self, Action, Canvas, Mode, Selection};
use super::{bail, info, thread, warn, AtomicBool, Config, Ipv4Addr, Mutex, Ordering, Result};
use control_transport::room::RoomRoster;
use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::watch;

struct Session {
    cancel: Arc<AtomicBool>,
    worker: thread::JoinHandle<Result<()>>,
}
impl Session {
    fn stop(&self) {
        self.cancel.store(true, Ordering::Release);
    }
}

#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
pub(super) fn run(
    config: &Config,
    bind_ip: Ipv4Addr,
    display_index: u32,
    test_cycles: Option<u32>,
    test_min_students: u32,
    test_min_thumbnails: u32,
    test_output: Option<&Path>,
    test_selective: bool,
    test_desktop: bool,
    test_settings_restart: bool,
    profile_path: &Path,
) -> Result<()> {
    let opened = Instant::now();
    let preferences = super::startup::UiPreferences::load(profile_path);
    let mut mode = if preferences.list {
        Mode::List
    } else {
        Mode::Grid
    };
    let mut selection = Selection::default();
    let mut status = String::from("Starting classroom connection...");
    let mut canvas = Canvas::new(preferences.width, preferences.height);
    dashboard_view::draw(
        &mut canvas,
        &[],
        mode,
        0,
        &selection,
        false,
        &status,
        &bind_ip.to_string(),
    );
    let mut window = Window::new(
        "Classroom | Teacher Console",
        preferences.width,
        preferences.height,
        WindowOptions {
            visible: false,
            resize: true,
            ..WindowOptions::default()
        },
    )?;
    canvas.set_density(window.backing_scale());
    dashboard_view::draw(
        &mut canvas,
        &[],
        mode,
        0,
        &selection,
        false,
        &status,
        &bind_ip.to_string(),
    );
    if let Some(path) = test_output.filter(|_| test_desktop) {
        std::fs::create_dir_all(path)?;
        snapshot(&canvas, &path.join("first-frame.ppm"))?;
        canvas.hover = Some(Action::Share);
        dashboard_view::draw(
            &mut canvas,
            &[],
            mode,
            0,
            &selection,
            false,
            &status,
            &bind_ip.to_string(),
        );
        snapshot(&canvas, &path.join("hover.ppm"))?;
        canvas.pressed = Some(Action::Share);
        dashboard_view::draw(
            &mut canvas,
            &[],
            mode,
            0,
            &selection,
            false,
            &status,
            &bind_ip.to_string(),
        );
        snapshot(&canvas, &path.join("pressed.ppm"))?;
        canvas.hover = None;
        canvas.pressed = None;
        dashboard_view::draw(
            &mut canvas,
            &[],
            mode,
            0,
            &selection,
            false,
            &status,
            &bind_ip.to_string(),
        );
    }
    window.set_target_fps(20);
    window.update_with_buffer(&canvas.pixels, canvas.pixel_width(), canvas.pixel_height())?;
    window.set_visible(true);
    info!(
        prepare_to_show_ms = opened.elapsed().as_secs_f64() * 1000.0,
        first_frame_prepared = true,
        backing_scale = window.backing_scale(),
        framebuffer_width = canvas.pixel_width(),
        framebuffer_height = canvas.pixel_height(),
        "teacher first GUI frame shown"
    );
    let mut tray = match super::desktop_tray::DesktopTray::new(true) {
        Ok(tray) => Some(tray),
        Err(error) => {
            warn!(%error,"Teacher tray unavailable; close will exit normally");
            None
        }
    };
    window.set_close_to_hide(tray.is_some());
    let mut restart_requested = false;
    let mut saved_preferences = preferences;
    let mut preference_due = Instant::now();
    let mut pressed_action = None;
    let mut desktop_step = 0;
    let mut desktop_due = Instant::now();
    let roster = Arc::new(Mutex::new(RoomRoster::default()));
    let (state, updates) = watch::channel(None);
    let server = tokio::spawn(control_transport::room::run(
        bind_ip,
        config.network.control_port,
        updates,
        Arc::clone(&roster),
    ));
    let monitor = if config.monitoring.enabled {
        match super::monitoring::teacher(
            std::net::SocketAddrV4::new(bind_ip, config.network.control_port + 1),
            config.monitoring.clone(),
            Arc::clone(&roster),
            Arc::new(super::monitoring::ObserveRequests::default()),
        ) {
            Ok(worker) => Some(worker),
            Err(error) => {
                warn!(%error,"teacher monitoring unavailable; presentation remains enabled");
                None
            }
        }
    } else {
        None
    };
    if let Some(path) = test_output {
        std::fs::create_dir_all(path)?;
    }
    let mut session: Option<Session> = None;
    let mut stream_ids = super::presentation_ids::PresentationIds::new()?;
    let mut stream_id = 0_u64;
    info!("Teacher presentation ID epoch initialized");
    let mut mouse_was_down = false;
    let mut page = 0_usize;
    status = String::from("Ready — Student agents connect automatically");
    let mut metrics = Instant::now();
    let mut test_steps = 0_u32;
    let mut test_due = Instant::now() + Duration::from_secs(5);
    let test_deadline = Instant::now() + Duration::from_secs(90);
    let mut ui_step = 0_u32;
    #[cfg(target_os = "macos")]
    let mut tap_test_step = 0_u32;
    let mut ui_due = Instant::now();
    let mut snapshots = std::collections::HashSet::new();
    let mut previous_visual = String::new();
    info!(
        ?test_cycles,
        ?mode,
        width = canvas.width,
        height = canvas.height,
        "teacher dashboard ready"
    );
    if test_desktop {
        super::launcher::show_settings_diagnostic(
            super::launcher::LaunchSelection {
                role: app_core::Role::Teacher,
                teacher_ip: bind_ip,
            },
            test_output,
            false,
        )?;
        window.set_visible(true);
    }
    if test_settings_restart {
        if let Some(value) = super::launcher::show_settings_diagnostic(
            super::launcher::LaunchSelection {
                role: app_core::Role::Student,
                teacher_ip: bind_ip,
            },
            test_output,
            true,
        )? {
            super::startup::save(profile_path, value)?;
            restart_requested = true;
        }
    }
    let result = (|| -> Result<()> {
        while window.is_open() {
            if restart_requested {
                break;
            }
            let mut diagnostic_command = None;
            if test_desktop && test_steps == 1 && session.is_some() {
                if desktop_step == 0
                    && test_due.saturating_duration_since(Instant::now()) < Duration::from_secs(4)
                {
                    anyhow::ensure!(tray.is_some(), "Desktop diagnostic requires native tray");
                    window.request_close();
                    desktop_step = 1;
                } else if desktop_step == 1 && !window.is_visible() {
                    info!(
                        presentation_active =
                            session.as_ref().is_some_and(|s| !s.worker.is_finished()),
                        "Teacher native close-to-tray verified"
                    );
                    desktop_step = 2;
                    desktop_due = Instant::now() + Duration::from_millis(800);
                } else if desktop_step == 2 && Instant::now() >= desktop_due {
                    diagnostic_command = Some(super::desktop_tray::Command::Show);
                    desktop_step = 3;
                }
            }
            if let Some(tray) = tray.as_mut() {
                tray.update(&format!("Teacher: {status}"), session.is_none());
                if let Some(command) = tray.poll().or(diagnostic_command) {
                    info!(?command, "Teacher tray command");
                    match command {
                        super::desktop_tray::Command::Show => {
                            window.set_visible(true);
                            previous_visual.clear();
                            if test_desktop && desktop_step == 3 {
                                info!(
                                    visible = window.is_visible(),
                                    "Teacher tray reopen verified"
                                );
                                desktop_step = 4;
                            }
                        }
                        super::desktop_tray::Command::Exit => break,
                        super::desktop_tray::Command::Settings if session.is_none() => {
                            if let Some(value) =
                                super::launcher::show_settings(super::launcher::LaunchSelection {
                                    role: app_core::Role::Teacher,
                                    teacher_ip: bind_ip,
                                })?
                            {
                                super::startup::save(profile_path, value)?;
                                restart_requested = true;
                                break;
                            }
                            window.set_visible(true);
                            previous_visual.clear();
                        }
                        super::desktop_tray::Command::Restart => {
                            restart_requested = true;
                            break;
                        }
                        super::desktop_tray::Command::Settings => {}
                    }
                }
            }
            if server.is_finished() {
                bail!("Teacher control listener stopped; check log.txt for port/bind errors");
            }
            if session.as_ref().is_some_and(|v| v.worker.is_finished()) {
                let finished = session.take().unwrap();
                state.send_replace(None);
                status = match finished.worker.join() {
                    Ok(Ok(())) => "Ready — Monitoring students".into(),
                    Ok(Err(error)) => {
                        warn!(%error,"teacher presentation failed");
                        "Sharing failed — See log".into()
                    }
                    Err(_) => "Sharing worker failed".into(),
                };
            }
            let mut rows = roster.lock().unwrap().snapshot();
            rows.sort_by(|a, b| {
                a.hostname
                    .cmp(&b.hostname)
                    .then(a.instance_id.cmp(&b.instance_id))
            });
            selection
                .ids
                .retain(|id| rows.iter().any(|r| r.instance_id == *id));
            if selection.ids.len() != 1 {
                selection.inspect = false;
            }
            let online = rows.iter().filter(|r| r.online).count();
            let thumbs = rows
                .iter()
                .filter(|r| r.online && r.thumbnail.is_some())
                .count();
            let (w, h) = window.get_size();
            let (w, h) = (w.max(1), h.max(1));
            if canvas.set_density(window.backing_scale()) {
                previous_visual.clear();
                info!(
                    backing_scale = window.backing_scale(),
                    "native GUI backing density changed"
                );
            }
            if canvas.width != w || canvas.height != h {
                canvas.resize(w, h);
            }
            let cap = dashboard_view::capacity(mode, w, h);
            page = page.min(rows.len().saturating_sub(1) / cap);
            #[cfg(target_os = "macos")]
            if test_desktop && tap_test_step < 2 {
                if tap_test_step == 0 {
                    mode = Mode::Grid;
                    #[allow(clippy::cast_precision_loss)]
                    let x = w.saturating_sub(80) as f32;
                    window.test_left_tap(x, 100.0);
                    window.update_with_buffer(
                        &canvas.pixels,
                        canvas.pixel_width(),
                        canvas.pixel_height(),
                    )?;
                    anyhow::ensure!(
                        !window.get_mouse_down(MouseButton::Left) && window.left_click().is_some(),
                        "Quick tap was lost inside one native event pump"
                    );
                    tap_test_step = 1;
                } else {
                    anyhow::ensure!(mode == Mode::List, "Quick tap did not activate List view");
                    info!(
                        backing_scale = window.backing_scale(),
                        "native zero-pressure quick tap activated List view"
                    );
                    tap_test_step = 2;
                }
            }
            let down = window.get_mouse_down(MouseButton::Left);
            let hover = window.get_mouse_pos(MouseMode::Discard).and_then(|(x, y)| {
                dashboard_view::hit(
                    mode,
                    w,
                    h,
                    x,
                    y,
                    rows.len().saturating_sub(page * cap).min(cap),
                    selection.inspect,
                )
            });
            canvas.hover.clone_from(&hover);
            if down && !mouse_was_down {
                pressed_action.clone_from(&hover);
            }
            canvas.pressed = if down { pressed_action.clone() } else { None };
            let test_action = test_cycles.is_some_and(|cycles| {
                test_steps < cycles * 2
                    && (test_output.is_none() || ui_step >= 4)
                    && (test_steps > 0
                        || (online >= test_min_students as usize
                            && thumbs >= test_min_thumbnails as usize))
                    && Instant::now() >= test_due
                    && (test_steps % 2 == 1 || session.is_none())
            });
            let mut action = if test_action {
                let start = test_steps % 2 == 0;
                test_steps += 1;
                test_due = Instant::now()
                    + Duration::from_secs(if start {
                        6
                    } else if test_output.is_some() {
                        4
                    } else {
                        2
                    });
                Some(if start {
                    if test_selective && test_steps > 1 {
                        Action::ShareSelected
                    } else {
                        Action::Share
                    }
                } else {
                    Action::Stop
                })
            } else if let Some((x, y)) = window.left_click() {
                let clicked = dashboard_view::hit(
                    mode,
                    w,
                    h,
                    x,
                    y,
                    rows.len().saturating_sub(page * cap).min(cap),
                    selection.inspect,
                );
                if let Some(pressed) = pressed_action.take() {
                    clicked.filter(|action| *action == pressed)
                } else {
                    clicked
                }
            } else if !down && mouse_was_down {
                pressed_action
                    .take()
                    .filter(|pressed| Some(pressed) == hover.as_ref())
            } else {
                None
            };
            if window.is_key_pressed(Key::Escape, KeyRepeat::No) {
                action = Some(Action::CloseInspect);
            }
            if test_output.is_some()
                && window.left_click().is_none()
                && !rows.is_empty()
                && thumbs >= test_min_thumbnails as usize
                && ui_step < 4
                && Instant::now() >= ui_due
            {
                action = match ui_step {
                    0 => Some(Action::Grid),
                    1 => Some(Action::List),
                    2 => Some(Action::Select(0)),
                    _ => Some(Action::Inspect),
                };
                ui_step += 1;
                ui_due = Instant::now() + Duration::from_millis(800);
                // Start/Stop remains on its own schedule after UI smoke actions.
            }
            if let Some(action) = action {
                match action {
                    Action::Share | Action::ShareSelected if session.is_none() => {
                        let recipients = if action == Action::ShareSelected {
                            let ids: std::collections::HashSet<String> = rows
                                .iter()
                                .filter(|r| r.online && selection.ids.contains(&r.instance_id))
                                .map(|r| r.instance_id.clone())
                                .collect();
                            if ids.is_empty() {
                                status = "Select an online Student first".into();
                                continue;
                            }
                            Some(Arc::new(ids))
                        } else {
                            None
                        };
                        let recipient_count = recipients.as_ref().map_or(online, |ids| ids.len());
                        let scope = if recipients.is_some() {
                            "selected"
                        } else {
                            "class"
                        };
                        selection.inspect = false;
                        stream_id = stream_ids.next()?;
                        let cancel = Arc::new(AtomicBool::new(false));
                        let stop = Arc::clone(&cancel);
                        let settings = config.clone();
                        let publisher = state.clone();
                        let worker = thread::spawn(move || {
                            present(
                                &settings,
                                bind_ip,
                                display_index,
                                stream_id,
                                &stop,
                                &publisher,
                                recipients.as_ref(),
                            )
                        });
                        session = Some(Session { cancel, worker });
                        status = format!("Sharing to {scope}: {recipient_count} Students");
                        info!(
                            stream_id,
                            scope, recipient_count, "teacher dashboard Start Sharing"
                        );
                    }
                    Action::Stop => {
                        if let Some(active) = &session {
                            info!(stream_id, "teacher dashboard Stop requested");
                            active.stop();
                            state.send_replace(None);
                            status = "Stopping presentation...".into();
                            info!(stream_id, "teacher dashboard Stop Sharing");
                        }
                    }
                    Action::Grid | Action::List => {
                        mode = if action == Action::Grid {
                            Mode::Grid
                        } else {
                            Mode::List
                        };
                        page = 0;
                        selection.inspect = false;
                        info!(?mode, "teacher dashboard view changed");
                    }
                    Action::Previous => page = page.saturating_sub(1),
                    Action::Next => {
                        if (page + 1) * cap < rows.len() {
                            page += 1;
                        }
                    }
                    Action::Select(index) => {
                        if let Some(row) = rows.get(page * cap + index) {
                            if !selection.ids.remove(&row.instance_id) {
                                selection.ids.insert(row.instance_id.clone());
                            }
                            info!(
                                selected = selection.ids.len(),
                                "teacher dashboard selection changed"
                            );
                        }
                    }
                    Action::Inspect => {
                        if selection.ids.len() == 1 {
                            selection.inspect = true;
                            info!("teacher dashboard snapshot inspection opened");
                        }
                    }
                    Action::CloseInspect => selection.inspect = false,
                    Action::Settings if session.is_none() => {
                        if let Some(value) =
                            super::launcher::show_settings(super::launcher::LaunchSelection {
                                role: app_core::Role::Teacher,
                                teacher_ip: bind_ip,
                            })?
                        {
                            super::startup::save(profile_path, value)?;
                            restart_requested = true;
                            break;
                        }
                        window.set_visible(true);
                        previous_visual.clear();
                    }
                    Action::Settings => {
                        status = "Stop sharing before changing connection settings".into();
                    }
                    Action::Share | Action::ShareSelected => {}
                }
            }
            mouse_was_down = down;
            if test_cycles.is_some_and(|cycles| {
                test_steps == cycles * 2 && session.is_none() && Instant::now() >= test_due
            }) {
                anyhow::ensure!(
                    !test_desktop || desktop_step == 4,
                    "Native close/show diagnostic incomplete"
                );
                break;
            }
            if test_cycles.is_some() && Instant::now() >= test_deadline {
                bail!("Teacher diagnostic deadline: connected={online}, thumbnails={thumbs}, steps={test_steps}");
            }
            let mut visual = format!(
                "{mode:?}:{page}:{w}:{h}:{}:{}:{status}:{:?}:{:?}:{:?}",
                session.is_some(),
                selection.inspect,
                selection.ids,
                canvas.hover,
                canvas.pressed
            );
            for row in &rows {
                use std::fmt::Write;
                let _ = write!(
                    visual,
                    "|{}:{}:{}:{}:{}",
                    row.client_id,
                    row.online,
                    row.monitoring_state,
                    row.thumbnail.as_ref().map_or(0, |t| t.sequence),
                    row.thumbnail
                        .as_ref()
                        .is_some_and(|t| t.received_at.elapsed().as_secs() > 7)
                );
            }
            let prefs = super::startup::UiPreferences {
                list: mode == Mode::List,
                width: w,
                height: h,
            };
            if prefs != saved_preferences && Instant::now() >= preference_due {
                if let Err(error) = prefs.save(profile_path) {
                    warn!(%error,"cannot save UI preferences");
                }
                saved_preferences = prefs;
                preference_due = Instant::now() + Duration::from_secs(1);
            }
            let update_started = Instant::now();
            if visual == previous_visual {
                window.update();
            } else {
                dashboard_view::draw(
                    &mut canvas,
                    &rows,
                    mode,
                    page,
                    &selection,
                    session.is_some(),
                    &status,
                    &bind_ip.to_string(),
                );
                window.update_with_buffer(
                    &canvas.pixels,
                    canvas.pixel_width(),
                    canvas.pixel_height(),
                )?;
                previous_visual = visual;
            }
            if let Some(path) = test_output {
                if thumbs >= test_min_thumbnails as usize {
                    let name = if selection.inspect {
                        "inspect"
                    } else if rows.iter().any(|r| r.monitoring_state == "paused") {
                        "sharing"
                    } else if mode == Mode::Grid {
                        "grid"
                    } else {
                        "list"
                    };
                    if snapshots.insert(name) {
                        snapshot(&canvas, &path.join(format!("{name}.ppm")))?;
                        info!(name, "teacher dashboard diagnostic snapshot saved");
                    }
                }
            }
            if metrics.elapsed() >= Duration::from_secs(1) {
                let fresh = rows
                    .iter()
                    .filter(|r| {
                        r.online
                            && r.monitoring_state == "ready"
                            && r.thumbnail
                                .as_ref()
                                .is_some_and(|t| t.received_at.elapsed().as_secs() < 7)
                    })
                    .count();
                info!(
                    test_steps,
                    online,
                    thumbnails = thumbs,
                    fresh_thumbnails = fresh,
                    selected = selection.ids.len(),
                    ?mode,
                    update_ms = update_started.elapsed().as_secs_f64() * 1000.0,
                    "teacher dashboard interval"
                );
                metrics = Instant::now();
            }
        }
        Ok(())
    })();
    if let Some(active) = session {
        active.stop();
        state.send_replace(None);
        let _ = active.worker.join();
    }
    let _ = super::startup::UiPreferences {
        list: mode == Mode::List,
        width: canvas.width,
        height: canvas.height,
    }
    .save(profile_path);
    server.abort();
    drop(monitor);
    if restart_requested && result.is_ok() {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !server.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        anyhow::ensure!(
            server.is_finished(),
            "Room did not stop before settings restart"
        );
        drop(tray);
        drop(window);
        super::startup::restart(profile_path)?;
    }
    result
}
pub(crate) fn snapshot(canvas: &Canvas, path: &Path) -> Result<()> {
    use std::io::Write;
    let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
    write!(
        file,
        "P6\n{} {}\n255\n",
        canvas.pixel_width(),
        canvas.pixel_height()
    )?;
    let bytes: Vec<u8> = canvas
        .pixels
        .iter()
        .flat_map(|p| {
            [
                u8::try_from((p >> 16) & 255).unwrap(),
                u8::try_from((p >> 8) & 255).unwrap(),
                u8::try_from(p & 255).unwrap(),
            ]
        })
        .collect();
    file.write_all(&bytes)?;
    Ok(())
}

pub(super) use super::media_session::publish as present;
