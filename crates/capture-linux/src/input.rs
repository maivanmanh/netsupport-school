//! Pointer/keyboard injection for remote control through Mutter RemoteDesktop.
//!
//! `enigo`/xdotool only reach XWayland windows; Mutter's RemoteDesktop session
//! moves the real compositor pointer and types into native Wayland apps. It is
//! paired with a ScreenCast stream so absolute motion has a coordinate space.

use std::collections::HashMap;
use zbus::{
    blocking::{Connection, Proxy},
    zvariant::OwnedObjectPath,
};

use crate::mutter::{Cursor, Session};

const SERVICE: &str = "org.gnome.Mutter.RemoteDesktop";
const ROOT: &str = "/org/gnome/Mutter/RemoteDesktop";

pub struct RemoteInput {
    connection: Connection,
    desktop: OwnedObjectPath,
    // Keeps the paired ScreenCast stream alive; its path anchors absolute motion.
    cast: Session,
    width: f64,
    height: f64,
    stopped: bool,
}

fn fail(context: &str, error: impl std::fmt::Display) -> String {
    format!("Mutter RemoteDesktop {context}: {error}")
}

impl RemoteInput {
    /// `width`/`height`: logical size of the primary monitor in pixels.
    ///
    /// # Errors
    /// Returns a message when Mutter's D-Bus services are unavailable.
    pub fn new(width: u32, height: u32) -> Result<Self, String> {
        if width == 0 || height == 0 {
            return Err("invalid input geometry".into());
        }
        let connection = Connection::session().map_err(|e| fail("session bus", e))?;
        let root = Proxy::new(&connection, SERVICE, ROOT, "org.gnome.Mutter.RemoteDesktop")
            .map_err(|e| fail("proxy", e))?;
        let desktop: OwnedObjectPath =
            root.call("CreateSession", &()).map_err(|e| fail("CreateSession", e))?;
        let session = Proxy::new(
            &connection,
            SERVICE,
            desktop.clone(),
            "org.gnome.Mutter.RemoteDesktop.Session",
        )
        .map_err(|e| fail("session proxy", e))?;
        let id: String = session
            .get_property("SessionId")
            .map_err(|e| fail("SessionId", e))?;
        let cast = Session::open("", Cursor::Hidden, Some(&id))?;
        session.call::<_, _, ()>("Start", &()).map_err(|e| fail("Start", e))?;
        Ok(Self {
            connection,
            desktop,
            cast,
            width: f64::from(width),
            height: f64::from(height),
            stopped: false,
        })
    }

    fn proxy(&self) -> Result<Proxy<'_>, String> {
        Proxy::new(
            &self.connection,
            SERVICE,
            self.desktop.clone(),
            "org.gnome.Mutter.RemoteDesktop.Session",
        )
        .map_err(|e| fail("proxy", e))
    }

    /// Absolute pointer position from normalized `0.0..=1.0` coordinates.
    ///
    /// # Errors
    /// Returns a message on D-Bus failure.
    pub fn move_to(&self, x: f64, y: f64) -> Result<(), String> {
        let (px, py) = (x.clamp(0.0, 1.0) * (self.width - 1.0), y.clamp(0.0, 1.0) * (self.height - 1.0));
        self.proxy()?
            .call::<_, _, ()>("NotifyPointerMotionAbsolute", &(self.cast.stream.as_str(), px, py))
            .map_err(|e| fail("pointer motion", e))
    }

    /// `button`: Linux evdev code (0x110 left, 0x111 right, 0x112 middle).
    ///
    /// # Errors
    /// Returns a message on D-Bus failure.
    pub fn button(&self, button: i32, pressed: bool) -> Result<(), String> {
        self.proxy()?
            .call::<_, _, ()>("NotifyPointerButton", &(button, pressed))
            .map_err(|e| fail("pointer button", e))
    }

    /// Vertical scroll in discrete wheel steps (positive scrolls down).
    ///
    /// # Errors
    /// Returns a message on D-Bus failure.
    pub fn scroll(&self, steps: i32) -> Result<(), String> {
        self.proxy()?
            .call::<_, _, ()>("NotifyPointerAxisDiscrete", &(0_u32, steps))
            .map_err(|e| fail("scroll", e))
    }

    /// Presses/releases an X11 keysym.
    ///
    /// # Errors
    /// Returns a message on D-Bus failure.
    pub fn key(&self, keysym: u32, pressed: bool) -> Result<(), String> {
        self.proxy()?
            .call::<_, _, ()>("NotifyKeyboardKeysym", &(keysym, pressed))
            .map_err(|e| fail("key", e))
    }

    /// Types one character that is not in the active keymap through the desktop's
    /// Unicode entry (Ctrl+Shift+U, hex digits, Space). Best effort: GTK/IBus apps.
    ///
    /// # Errors
    /// Returns a message on D-Bus failure.
    pub fn unicode(&self, c: char) -> Result<(), String> {
        let tap = |sym: u32| -> Result<(), String> {
            self.key(sym, true)?;
            self.key(sym, false)?;
            std::thread::sleep(std::time::Duration::from_millis(25));
            Ok(())
        };
        self.key(0xffe3, true)?;
        self.key(0xffe1, true)?;
        tap(u32::from(b'u'))?;
        self.key(0xffe1, false)?;
        self.key(0xffe3, false)?;
        std::thread::sleep(std::time::Duration::from_millis(80));
        for digit in format!("{:x}", u32::from(c)).bytes() {
            tap(u32::from(digit))?;
        }
        std::thread::sleep(std::time::Duration::from_millis(40));
        tap(0x20)?;
        std::thread::sleep(std::time::Duration::from_millis(60));
        Ok(())
    }

    pub fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        if let Ok(proxy) = self.proxy() {
            let _ = proxy.call::<_, _, ()>("Stop", &());
        }
        self.cast.stop();
    }
}

impl Drop for RemoteInput {
    fn drop(&mut self) {
        self.stop();
    }
}

/// X11 keysym for a Classroom wire key name (`"Enter"`, `"ArrowLeft"`, `"a"`, ...).
#[must_use]
pub fn keysym_for(name: &str) -> Option<u32> {
    let named: HashMap<&str, u32> = HashMap::from([
        ("Enter", 0xff0d),
        ("Backspace", 0xff08),
        ("Tab", 0xff09),
        ("Escape", 0xff1b),
        ("Shift", 0xffe1),
        ("Control", 0xffe3),
        ("Alt", 0xffe9),
        ("Meta", 0xffeb),
        ("ArrowLeft", 0xff51),
        ("ArrowUp", 0xff52),
        ("ArrowRight", 0xff53),
        ("ArrowDown", 0xff54),
        ("Delete", 0xffff),
        ("Home", 0xff50),
        ("End", 0xff57),
        ("PageUp", 0xff55),
        ("PageDown", 0xff56),
    ]);
    if let Some(sym) = named.get(name) {
        return Some(*sym);
    }
    let mut chars = name.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    let cp = u32::from(c);
    Some(match cp {
        0x20..=0x7e | 0xa0..=0xff => cp,
        _ if cp >= 0x100 && !c.is_control() => 0x0100_0000 | cp,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::keysym_for;
    #[test]
    fn maps_names_ascii_and_unicode() {
        assert_eq!(keysym_for("Enter"), Some(0xff0d));
        assert_eq!(keysym_for("a"), Some(0x61));
        assert_eq!(keysym_for("é"), Some(0xe9));
        assert_eq!(keysym_for("ạ"), Some(0x0100_1ea1));
        assert_eq!(keysym_for("ab"), None);
        assert_eq!(keysym_for("\n"), None);
    }
}
