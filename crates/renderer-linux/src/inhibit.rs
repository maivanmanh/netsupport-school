//! Keeps the desktop from blanking/locking/suspending while a Teacher
//! presentation is on screen. Best effort: failure never blocks presentation.

use zbus::blocking::Connection;

const APP: &str = "Classroom Student";
const REASON: &str = "Teacher presentation";
// org.gnome.SessionManager flags: 4 = suspend, 8 = idle.
const GNOME_SUSPEND_IDLE: u32 = 4 | 8;

impl std::fmt::Debug for IdleInhibitor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("IdleInhibitor").finish_non_exhaustive()
    }
}

enum Kind {
    Gnome,
    FreeDesktop(&'static str),
}

/// Holds an inhibit cookie for as long as it lives. The bus also drops the
/// inhibition when the process (connection) dies, so a crash cannot leave it set.
pub(crate) struct IdleInhibitor {
    connection: Connection,
    kind: Kind,
    cookie: u32,
}

impl IdleInhibitor {
    pub fn acquire() -> Option<Self> {
        let connection = Connection::session().ok()?;
        if let Ok(reply) = connection.call_method(
            Some("org.gnome.SessionManager"),
            "/org/gnome/SessionManager",
            Some("org.gnome.SessionManager"),
            "Inhibit",
            &(APP, 0_u32, REASON, GNOME_SUSPEND_IDLE),
        ) {
            if let Ok(cookie) = reply.body().deserialize::<u32>() {
                return Some(Self { connection, kind: Kind::Gnome, cookie });
            }
        }
        for path in ["/org/freedesktop/ScreenSaver", "/ScreenSaver"] {
            if let Ok(reply) = connection.call_method(
                Some("org.freedesktop.ScreenSaver"),
                path,
                Some("org.freedesktop.ScreenSaver"),
                "Inhibit",
                &(APP, REASON),
            ) {
                if let Ok(cookie) = reply.body().deserialize::<u32>() {
                    return Some(Self { connection, kind: Kind::FreeDesktop(path), cookie });
                }
            }
        }
        None
    }
}

impl Drop for IdleInhibitor {
    fn drop(&mut self) {
        let _ = match self.kind {
            Kind::Gnome => self.connection.call_method(
                Some("org.gnome.SessionManager"),
                "/org/gnome/SessionManager",
                Some("org.gnome.SessionManager"),
                "Uninhibit",
                &(self.cookie,),
            ),
            Kind::FreeDesktop(path) => self.connection.call_method(
                Some("org.freedesktop.ScreenSaver"),
                path,
                Some("org.freedesktop.ScreenSaver"),
                "UnInhibit",
                &(self.cookie,),
            ),
        };
    }
}
