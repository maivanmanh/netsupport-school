//! Session-owned OS input adapter. Cooperative by default, lease-controlled
//! exclusive Windows input, never logs key contents or pointer coordinates.
#![allow(unsafe_code)]
use anyhow::{Context, Result};
#[cfg(not(any(windows, target_os = "macos")))]
use enigo::Coordinate;
#[cfg(not(target_os = "macos"))]
use enigo::Keyboard;
use enigo::{Axis, Button, Direction, Enigo, Key, Mouse, Settings};
use protocol::ObserveMessage;
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};
use tracing::{info, warn};
#[derive(Default)]
pub struct Controller {
    device: Option<Enigo>,
    #[cfg(target_os = "macos")]
    keyboard: mac_keyboard::Keyboard,
    enabled: bool,
    exclusive: bool,
    lease: Option<Instant>,
    keys: HashSet<String>,
    buttons: HashSet<String>,
    bounds: Option<[f64; 4]>,
    display: Option<ControlDisplay>,
}
impl Controller {
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn apply(&mut self, m: &ObserveMessage) -> Result<()> {
        if m.action == "button_at" {
            self.expire();
            if !self.enabled {
                return Ok(());
            }
            // Position and click travel together: motion coalescing cannot move
            // the click to an older position, or past a newer queued motion.
            let began = Instant::now();
            self.apply(&ObserveMessage {
                action: "move".into(),
                ..m.clone()
            })?;
            self.apply(&ObserveMessage {
                action: "button".into(),
                ..m.clone()
            })?;
            if m.down {
                info!(
                    apply_us = began.elapsed().as_micros(),
                    "Observe positioned click applied"
                );
            }
            return Ok(());
        }
        if m.action == "observe" {
            self.release();
            return Ok(());
        }
        if m.action == "control" || m.action == "exclusive" {
            return self.enable(m.action == "exclusive");
        }
        if !self.enabled {
            return Ok(());
        }
        // Only explicit keepalive renews lease; queued old input cannot hold a lock.
        if m.action == "keepalive" {
            self.lease = Some(Instant::now());
            return Ok(());
        }
        self.expire();
        if !self.enabled {
            return Ok(());
        }
        // Resolve live geometry before each pointer placement. Use the selected
        // display's stable OS identity, not repeated expensive capture discovery.
        // This also covers positioned clicks immediately after a mode/DPI change.
        if m.action == "move" {
            self.refresh_bounds()?;
        }
        let d = self.device.as_mut().unwrap();
        let direction = if m.down {
            Direction::Press
        } else {
            Direction::Release
        };
        match m.action.as_str() {
            "move" => {
                anyhow::ensure!(
                    m.x.is_finite()
                        && m.y.is_finite()
                        && (0.0..=1.0).contains(&m.x)
                        && (0.0..=1.0).contains(&m.y),
                    "Invalid remote position"
                );
                let (x, y) =
                    desktop_position(self.bounds.context("input geometry missing")?, m.x, m.y)?;
                #[cfg(windows)]
                move_windows(x, y)?;
                #[cfg(target_os = "macos")]
                move_macos(x, y, !self.buttons.is_empty())?;
                #[cfg(not(any(windows, target_os = "macos")))]
                d.move_mouse(x, y, Coordinate::Abs)?;
            }
            "button" => {
                let button =
                    button(&m.key).ok_or_else(|| anyhow::anyhow!("Invalid remote button"))?;
                d.button(button, direction)?;
                if m.down {
                    self.buttons.insert(m.key.clone());
                } else {
                    self.buttons.remove(&m.key);
                }
            }
            "key" => {
                let key = key(&m.key).ok_or_else(|| anyhow::anyhow!("Unsupported remote key"))?;
                anyhow::ensure!(
                    !m.down || self.keys.contains(&m.key) || self.keys.len() < 128,
                    "Too many held remote keys"
                );
                #[cfg(target_os = "macos")]
                self.keyboard.apply(d, &m.key, key, direction)?;
                #[cfg(not(target_os = "macos"))]
                d.key(key, direction)?;
                if m.down {
                    self.keys.insert(m.key.clone());
                } else {
                    self.keys.remove(&m.key);
                }
            }
            "scroll" => {
                anyhow::ensure!(m.y.is_finite() && m.y.abs() <= 20.0, "Invalid scroll delta");
                #[allow(clippy::cast_possible_truncation)]
                d.scroll(m.y.round() as i32, Axis::Vertical)?;
            }
            _ => anyhow::bail!("Unknown remote input action"),
        }
        Ok(())
    }
    fn refresh_bounds(&mut self) -> Result<()> {
        let bounds = self
            .display
            .as_ref()
            .context("input display missing")?
            .bounds()?;
        if self.bounds != Some(bounds) {
            info!(bounds = ?bounds, "Observe input geometry updated");
            self.bounds = Some(bounds);
        }
        Ok(())
    }
    fn enable(&mut self, exclusive: bool) -> Result<()> {
        self.release();
        let device = Enigo::new(&Settings::default())
            .map_err(|e| anyhow::anyhow!("Remote input permission/init: {e}"))?;
        // Reverse publisher currently captures display 0. Resolve with the exact
        // capture backend selection, not Enigo's unrelated main-display pixels.
        let display = ControlDisplay::new()?;
        self.bounds = Some(display.bounds()?);
        self.display = Some(display);
        let coordinate_space = if cfg!(target_os = "macos") {
            "desktop_points"
        } else {
            "physical_desktop_pixels"
        };
        info!(display_index = 0, coordinate_space, bounds = ?self.bounds,
            "Observe input geometry ready");
        self.device = Some(device);
        if exclusive {
            #[cfg(windows)]
            {
                // SAFETY: this session's input thread owns the block and all
                // SendInput calls; release runs on this same thread on Drop/expiry.
                unsafe {
                    windows::Win32::UI::Input::KeyboardAndMouse::BlockInput(true)?;
                }
                self.exclusive = true;
            }
            #[cfg(not(windows))]
            {
                self.device = None;
                anyhow::bail!("Exclusive input is supported only on Windows Students");
            }
        }
        self.enabled = true;
        self.lease = Some(Instant::now());
        info!(exclusive = self.exclusive, "Observe input mode enabled");
        Ok(())
    }
    pub fn expire(&mut self) -> bool {
        if self.enabled
            && self
                .lease
                .is_none_or(|t| t.elapsed() > Duration::from_secs(3))
        {
            warn!("Observe input lease expired; releasing Student input");
            self.release();
            return true;
        }
        false
    }
    pub fn release(&mut self) {
        if let Some(d) = self.device.as_mut() {
            #[cfg(target_os = "macos")]
            {
                self.keyboard.release(d);
                self.keys.clear();
            }
            #[cfg(not(target_os = "macos"))]
            for k in self.keys.drain() {
                if let Some(k) = key(&k) {
                    let _ = d.key(k, Direction::Release);
                }
            }
            for b in self.buttons.drain() {
                if let Some(b) = button(&b) {
                    let _ = d.button(b, Direction::Release);
                }
            }
        }
        #[cfg(windows)]
        if self.exclusive {
            // SAFETY: balanced unlock on the thread that acquired the block.
            let _ = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::BlockInput(false) };
        }
        if self.enabled {
            info!("Observe remote input released");
        }
        self.exclusive = false;
        self.enabled = false;
        self.device = None;
        self.lease = None;
        self.bounds = None;
        self.display = None;
    }
}

/// HIToolbox keyboard-layout lookup asserts the main dispatch queue on current
/// macOS. Only lookup runs there; CGEvent injection stays with the input owner.
/// One outstanding lookup per session, a short deadline and no late injection
/// prevent a stalled UI/shutdown from building an input queue or hanging join.
#[cfg(target_os = "macos")]
mod mac_keyboard {
    use super::*;
    use enigo::Keyboard as _;
    use std::{
        collections::HashMap,
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc, Arc,
        },
    };

    #[derive(Default)]
    pub struct Keyboard {
        held: HashMap<String, u16>,
        pending: Arc<AtomicBool>,
    }
    impl Keyboard {
        fn resolve(&self, key: Key) -> Result<u16> {
            // Named keys use constant native keycodes, without HIToolbox.
            if !matches!(key, Key::Unicode(_)) || objc2::MainThreadMarker::new().is_some() {
                return u16::try_from(key).map_err(|()| anyhow::anyhow!("Unsupported macOS key"));
            }
            anyhow::ensure!(
                !self.pending.swap(true, Ordering::AcqRel),
                "macOS keyboard layout lookup still pending"
            );
            let pending = Arc::clone(&self.pending);
            let (send, receive) = mpsc::sync_channel(1);
            let budget = Duration::from_millis(250);
            let deadline = Instant::now() + budget;
            dispatch2::DispatchQueue::main().exec_async(move || {
                if Instant::now() < deadline {
                    let _ = send.try_send(u16::try_from(key));
                }
                pending.store(false, Ordering::Release);
            });
            receive
                .recv_timeout(budget)
                .context("macOS keyboard layout lookup timed out")?
                .map_err(|()| anyhow::anyhow!("Unsupported macOS key"))
        }
        pub fn apply(
            &mut self,
            device: &mut Enigo,
            name: &str,
            key: Key,
            direction: Direction,
        ) -> Result<()> {
            // Retain the keycode chosen at key-down, even if the user changes
            // layouts before key-up. Release/Drop never calls HIToolbox.
            let code = match self.held.get(name) {
                Some(code) => *code,
                None if direction == Direction::Release => return Ok(()),
                None => self.resolve(key)?,
            };
            device.raw(code, direction)?;
            if direction == Direction::Press {
                self.held.insert(name.to_owned(), code);
            } else if direction == Direction::Release {
                self.held.remove(name);
            }
            Ok(())
        }
        pub fn release(&mut self, device: &mut Enigo) {
            for (_, code) in self.held.drain() {
                let _ = device.raw(code, Direction::Release);
            }
        }
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn named_keys_do_not_need_a_main_queue_lookup() {
            let keyboard = Keyboard::default();
            assert_eq!(keyboard.resolve(Key::Return).unwrap(), 36);
            assert!(!keyboard.pending.load(Ordering::Acquire));
        }
        #[test]
        fn only_one_layout_lookup_can_be_pending() {
            let keyboard = Keyboard::default();
            keyboard.pending.store(true, Ordering::Release);
            let began = Instant::now();
            assert!(keyboard.resolve(Key::Unicode('a')).is_err());
            assert!(began.elapsed() < Duration::from_millis(100));
        }
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        self.release();
    }
}
struct ControlDisplay {
    #[cfg(target_os = "macos")]
    id: u32,
    #[cfg(windows)]
    monitor: windows::Win32::Graphics::Gdi::HMONITOR,
}
impl ControlDisplay {
    fn new() -> Result<Self> {
        #[cfg(target_os = "macos")]
        return Ok(Self {
            id: capture_macos_sck::control_display_id(0)?,
        });
        #[cfg(windows)]
        return Ok(Self {
            monitor: capture_windows_dxgi::control_display_monitor(0)?,
        });
        #[cfg(not(any(windows, target_os = "macos")))]
        anyhow::bail!("Remote input is unavailable on this platform");
    }
    fn bounds(&self) -> Result<[f64; 4]> {
        #[cfg(target_os = "macos")]
        {
            let frame = core_graphics::display::CGDisplay::new(self.id).bounds();
            let bounds = [
                frame.origin.x,
                frame.origin.y,
                frame.size.width,
                frame.size.height,
            ];
            validate_bounds(bounds)?;
            Ok(bounds)
        }
        #[cfg(windows)]
        {
            use windows::Win32::{
                Graphics::Gdi::{GetMonitorInfoW, MONITORINFO},
                UI::HiDpi::{
                    SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
                },
            };
            let mut info = MONITORINFO {
                cbSize: u32::try_from(std::mem::size_of::<MONITORINFO>())?,
                ..Default::default()
            };
            // SAFETY: borrowed OS monitor handle, correctly sized output struct.
            // Always restore this thread's DPI context before returning errors.
            let ok = unsafe {
                let previous =
                    SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
                anyhow::ensure!(
                    !previous.0.is_null(),
                    "Cannot query physical input geometry"
                );
                let ok = GetMonitorInfoW(self.monitor, &raw mut info).as_bool();
                SetThreadDpiAwarenessContext(previous);
                ok
            };
            anyhow::ensure!(ok, "Control display unavailable");
            let r = info.rcMonitor;
            let bounds = [
                f64::from(r.left),
                f64::from(r.top),
                f64::from(r.right - r.left),
                f64::from(r.bottom - r.top),
            ];
            validate_bounds(bounds)?;
            Ok(bounds)
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        anyhow::bail!("Remote input is unavailable on this platform");
    }
}
fn validate_bounds(bounds: [f64; 4]) -> Result<()> {
    anyhow::ensure!(
        bounds.iter().all(|v| v.is_finite()) && bounds[2] >= 1.0 && bounds[3] >= 1.0,
        "Invalid control display geometry"
    );
    Ok(())
}

#[allow(clippy::cast_possible_truncation)]
fn desktop_position(bounds: [f64; 4], x: f64, y: f64) -> Result<(i32, i32)> {
    let [left, top, width, height] = bounds;
    validate_bounds(bounds)?;
    anyhow::ensure!(
        x.is_finite() && y.is_finite() && (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y),
        "Invalid remote position"
    );
    Ok((
        (left + x * (width - 1.0)).round() as i32,
        (top + y * (height - 1.0)).round() as i32,
    ))
}

#[cfg(windows)]
fn move_windows(x: i32, y: i32) -> Result<()> {
    use windows::Win32::UI::{
        HiDpi::{SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2},
        Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_MOVE,
            MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT,
        },
        WindowsAndMessaging::{
            GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
            SM_YVIRTUALSCREEN,
        },
    };
    // SAFETY: temporarily query physical virtual-desktop metrics on this input
    // owner's thread, restore its previous DPI context before any return.
    let (left, top, width, height) = unsafe {
        let previous = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        anyhow::ensure!(
            !previous.0.is_null(),
            "Cannot query physical input geometry"
        );
        let rect = (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        );
        SetThreadDpiAwarenessContext(previous);
        rect
    };
    anyhow::ensure!(width > 1 && height > 1, "Invalid virtual desktop geometry");
    let normalized = |value: i32, origin: i32, extent: i32| -> i32 {
        i32::try_from((i64::from(value - origin) * 65535 / i64::from(extent - 1)).clamp(0, 65535))
            .unwrap_or_default()
    };
    let packet = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: normalized(x, left, width),
                dy: normalized(y, top, height),
                dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                ..Default::default()
            },
        },
    };
    // SAFETY: one correctly tagged INPUT, on the owner thread (also valid during
    // its BlockInput lease); SendInput handles physical normalized coordinates.
    let sent = unsafe { SendInput(&[packet], i32::try_from(std::mem::size_of::<INPUT>())?) };
    anyhow::ensure!(sent == 1, "Remote pointer injection was rejected");
    Ok(())
}

#[cfg(target_os = "macos")]
fn move_macos(x: i32, y: i32, dragged: bool) -> Result<()> {
    use core_graphics::{
        event::{CGEvent, CGEventTapLocation, CGEventType, CGMouseButton},
        event_source::{CGEventSource, CGEventSourceStateID},
        geometry::CGPoint,
    };
    let source = CGEventSource::new(CGEventSourceStateID::Private)
        .map_err(|()| anyhow::anyhow!("Cannot create macOS input event source"))?;
    let kind = if dragged {
        CGEventType::LeftMouseDragged
    } else {
        CGEventType::MouseMoved
    };
    let event = CGEvent::new_mouse_event(
        source,
        kind,
        CGPoint::new(f64::from(x), f64::from(y)),
        CGMouseButton::Left,
    )
    .map_err(|()| anyhow::anyhow!("Cannot create macOS pointer event"))?;
    event.post(CGEventTapLocation::HID);
    Ok(())
}

fn button(s: &str) -> Option<Button> {
    match s {
        "left" => Some(Button::Left),
        "right" => Some(Button::Right),
        "middle" => Some(Button::Middle),
        _ => None,
    }
}
fn key(s: &str) -> Option<Key> {
    Some(match s {
        "Enter" => Key::Return,
        "Backspace" => Key::Backspace,
        "Tab" => Key::Tab,
        "Escape" => Key::Escape,
        "Shift" => Key::Shift,
        "Control" => Key::Control,
        "Alt" => Key::Alt,
        "Meta" => Key::Meta,
        "ArrowLeft" => Key::LeftArrow,
        "ArrowRight" => Key::RightArrow,
        "ArrowUp" => Key::UpArrow,
        "ArrowDown" => Key::DownArrow,
        "Delete" => Key::Delete,
        "Home" => Key::Home,
        "End" => Key::End,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        _ => {
            let mut c = s.chars();
            let c1 = c.next()?;
            if c.next().is_some() {
                return None;
            }
            Key::Unicode(c1)
        }
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalized_pointer_uses_logical_retina_bounds_and_display_origin() {
        // A 2x backing surface does not double macOS input coordinates.
        assert_eq!(
            desktop_position([0.0, 0.0, 1536.0, 960.0], 0.5, 0.5).unwrap(),
            (768, 480)
        );
        assert_eq!(
            desktop_position([-1920.0, 120.0, 1920.0, 1080.0], 0.0, 0.0).unwrap(),
            (-1920, 120)
        );
        assert_eq!(
            desktop_position([-1920.0, 120.0, 1920.0, 1080.0], 1.0, 1.0).unwrap(),
            (-1, 1199)
        );
        assert!(desktop_position([0.0, 0.0, 1920.0, 1080.0], f64::NAN, 0.5).is_err());
        assert!(desktop_position([0.0, 0.0, 0.0, 1080.0], 0.5, 0.5).is_err());
    }
    #[test]
    fn observe_never_injects() {
        let mut c = Controller::default();
        c.apply(&ObserveMessage {
            action: "key".into(),
            key: "a".into(),
            down: true,
            ..Default::default()
        })
        .unwrap();
        assert!(c.device.is_none());
        assert!(!c.enabled);
    }
    #[test]
    fn key_validation_is_bounded() {
        assert!(key("ArrowLeft").is_some());
        assert!(key("ab").is_none());
        assert!(button("bad").is_none());
    }
    #[test]
    fn expired_input_lease_fails_open_without_device() {
        let mut input = Controller::default();
        input.enabled = true;
        input.lease = Instant::now().checked_sub(Duration::from_secs(4));
        assert!(input.expire());
        assert!(!input.enabled);
        assert!(!input.exclusive);
        assert!(input.lease.is_none());
    }
}
