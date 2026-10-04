//! Native tray/menu bar ownership and commands; created and polled on UI thread.
use anyhow::Result;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Show,
    Settings,
    Restart,
    Exit,
}

#[cfg(any(windows, target_os = "macos"))]
mod native {
    use super::{Command, Result};
    use tray_icon::{
        menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
        Icon, TrayIcon, TrayIconBuilder,
    };
    pub struct DesktopTray {
        icon: TrayIcon,
        show: MenuItem,
        settings: MenuItem,
        exit: MenuItem,
        restart: MenuItem,
        status: MenuItem,
        last_status: String,
    }
    impl DesktopTray {
        pub fn new(teacher: bool) -> Result<Self> {
            let menu = Menu::new();
            let status = MenuItem::new("Classroom: starting", false, None);
            let show = MenuItem::new(
                if teacher {
                    "Open Teacher console"
                } else {
                    "Connection settings"
                },
                true,
                None,
            );
            let settings = MenuItem::new("Settings...", true, None);
            let exit = MenuItem::new("Exit Classroom", true, None);
            let restart = MenuItem::new("Restart Classroom", true, None);
            menu.append_items(&[&status, &PredefinedMenuItem::separator()])?;
            if teacher {
                menu.append(&show)?;
            }
            menu.append_items(&[&settings, &PredefinedMenuItem::separator()])?;
            menu.append(&restart)?;
            menu.append(&exit)?;
            // Small code-generated monitor symbol, no image/network dependency.
            let mut rgba = vec![0_u8; 24 * 24 * 4];
            for y in 3..21 {
                for x in 3..21 {
                    if y <= 16
                        || (y >= 19 && (8..16).contains(&x))
                        || ((10..14).contains(&x) && y >= 16)
                    {
                        let i = (y * 24 + x) * 4;
                        let color = if (5..19).contains(&x) && (5..15).contains(&y) {
                            [235, 243, 255, 255]
                        } else {
                            [30, 78, 136, 255]
                        };
                        rgba[i..i + 4].copy_from_slice(&color);
                    }
                }
            }
            let icon = TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_tooltip("Classroom")
                .with_icon(Icon::from_rgba(rgba, 24, 24)?)
                .build()?;
            tracing::info!(teacher, "native Classroom tray ready");
            Ok(Self {
                icon,
                show,
                settings,
                exit,
                restart,
                status,
                last_status: String::new(),
            })
        }
        pub fn poll(&self) -> Option<Command> {
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                if event.id == *self.show.id() {
                    return Some(Command::Show);
                }
                if event.id == *self.settings.id() {
                    return Some(Command::Settings);
                }
                if event.id == *self.exit.id() {
                    return Some(Command::Exit);
                }
                if event.id == *self.restart.id() {
                    return Some(Command::Restart);
                }
            }
            None
        }
        pub fn update(&mut self, status: &str, settings_enabled: bool) {
            self.settings.set_enabled(settings_enabled);
            if self.last_status != status {
                self.status.set_text(status);
                let _ = self.icon.set_tooltip(Some(status));
                status.clone_into(&mut self.last_status);
            }
        }
    }
}
#[cfg(any(windows, target_os = "macos"))]
pub use native::DesktopTray;

#[cfg(windows)]
#[allow(unsafe_code)] // Small Win32 boundary: cancel only this worker's windows.
mod windows_teacher {
    use super::{Command, DesktopTray, Result};
    use anyhow::Context;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc, Mutex,
    };
    use std::thread;
    use std::time::Duration;
    use windows::core::BOOL;
    use windows::Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        UI::WindowsAndMessaging::{
            EnumThreadWindows, GetWindowThreadProcessId, PostMessageW, WM_CANCELMODE,
        },
    };

    /// All native tray objects stay on one dedicated message-pump thread.
    /// Status is a single latest-value slot; commands have a small fixed bound.
    pub struct WindowsTeacherTray {
        status: Arc<Mutex<(String, bool)>>,
        last: (String, bool),
        commands: Receiver<Command>,
        stop: Arc<AtomicBool>,
        thread_id: u32,
        worker: Option<thread::JoinHandle<()>>,
    }
    impl WindowsTeacherTray {
        pub fn new() -> Result<Self> {
            let status = Arc::new(Mutex::new((String::new(), true)));
            let latest = Arc::clone(&status);
            let stop = Arc::new(AtomicBool::new(false));
            let canceled = Arc::clone(&stop);
            let (send, commands) = mpsc::sync_channel(8);
            let (ready, startup) = mpsc::sync_channel(1);
            let worker = thread::spawn(move || {
                let run = || -> Result<()> {
                    let mut host = minifb::Window::new(
                        "Classroom Teacher tray",
                        1,
                        1,
                        minifb::WindowOptions {
                            visible: false,
                            background: true,
                            borderless: true,
                            title: false,
                            ..Default::default()
                        },
                    )?;
                    host.set_target_fps(0);
                    host.update();
                    let mut tray = DesktopTray::new(true)?;
                    // SAFETY: minifb owns this live HWND on the current thread.
                    let thread_id =
                        unsafe { GetWindowThreadProcessId(HWND(host.get_window_handle()), None) };
                    let _ = ready.send(Ok(thread_id));
                    while !canceled.load(Ordering::Acquire) {
                        host.update();
                        if canceled.load(Ordering::Acquire) {
                            break;
                        }
                        let (text, enabled) = latest.lock().unwrap().clone();
                        tray.update(&text, enabled);
                        while let Some(command) = tray.poll() {
                            let _ = send.try_send(command);
                        }
                        thread::sleep(Duration::from_millis(10));
                    }
                    Ok(())
                };
                if let Err(error) = run() {
                    let _ = ready.try_send(Err(format!("{error:#}")));
                    tracing::warn!(%error, "Teacher tray worker stopped");
                }
            });
            let thread_id = startup
                .recv()
                .context("Teacher tray startup channel closed")?
                .map_err(anyhow::Error::msg)?;
            Ok(Self {
                status,
                last: (String::new(), true),
                commands,
                stop,
                thread_id,
                worker: Some(worker),
            })
        }
        pub fn update(&mut self, text: &str, enabled: bool) {
            if self.last.0 != text || self.last.1 != enabled {
                text.clone_into(&mut self.last.0);
                self.last.1 = enabled;
                *self.status.lock().unwrap() = self.last.clone();
            }
        }
        pub fn poll(&self) -> Option<Command> {
            self.commands.try_recv().ok()
        }
    }
    unsafe extern "system" fn cancel_menu(window: HWND, _: LPARAM) -> BOOL {
        // SAFETY: EnumThreadWindows supplies live windows belonging only to our
        // tray thread. WM_CANCELMODE releases its modal menu without user input.
        let _ = unsafe { PostMessageW(Some(window), WM_CANCELMODE, WPARAM(0), LPARAM(0)) };
        BOOL(1)
    }
    impl Drop for WindowsTeacherTray {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Release);
            // SAFETY: this worker remains alive until join; enumeration and the
            // callback do not retain HWNDs or touch another thread's windows.
            let _ = unsafe { EnumThreadWindows(self.thread_id, Some(cancel_menu), LPARAM(0)) };
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }
}
#[cfg(windows)]
pub use windows_teacher::WindowsTeacherTray;

pub struct StudentDesktop {
    pub tools: Option<super::student_tools_ui::StudentToolsUi>,
    host: minifb::Window,
    tray: Option<DesktopTray>,
    pub exit_requested: bool,
    pub settings_requested: bool,
    pub restart_requested: bool,
}
impl StudentDesktop {
    pub fn new() -> Result<Self> {
        let mut host = minifb::Window::new(
            "Classroom background agent",
            1,
            1,
            minifb::WindowOptions {
                visible: false,
                background: true,
                borderless: true,
                title: false,
                ..Default::default()
            },
        )?;
        host.set_target_fps(0);
        host.update(); // initialize the native loop before adding the status item
        let tray = match DesktopTray::new(false) {
            Ok(tray) => Some(tray),
            Err(error) => {
                tracing::warn!(%error,"Student tray unavailable; native background pump remains active");
                None
            }
        };
        Ok(Self {
            tools: None,
            host,
            tray,
            exit_requested: false,
            settings_requested: false,
            restart_requested: false,
        })
    }
    pub fn pump(&mut self, status: &str, presenting: bool) {
        if let Some(tools) = self.tools.as_mut() {
            tools.pump();
        }
        if !presenting {
            self.host.update();
        }
        if let Some(tray) = self.tray.as_mut() {
            tray.update(status, !presenting);
            if let Some(command) = tray.poll() {
                tracing::info!(?command, presenting, "Student tray command");
                match command {
                    Command::Exit => self.exit_requested = true,
                    Command::Restart => {
                        self.restart_requested = true;
                        self.exit_requested = true;
                    }
                    Command::Show | Command::Settings if !presenting => {
                        self.settings_requested = true;
                    }
                    _ => {}
                }
            }
        }
    }
}
