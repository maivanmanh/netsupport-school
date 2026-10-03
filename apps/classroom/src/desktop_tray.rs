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
