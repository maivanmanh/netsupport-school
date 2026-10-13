use std::{
    fmt, ptr,
    time::{Duration, Instant},
};

use media_core::{DecodedFrameRef, MediaError, PointerOverlay, Renderer};
use minifb::{ScaleMode, Window, WindowOptions};
use x11_dl::xlib;

use crate::surface::Surface;

#[derive(Debug)]
pub struct NativeRenderer {
    window: Window,
    surface: Surface,
    presentation: Option<Presentation>,
    last_event_pump: Instant,
    presented_window_size: (usize, usize),
    presentation_failed: bool,
}

impl NativeRenderer {
    /// Creates a normal resizable preview window.
    /// # Errors
    /// Returns invalid-size errors or X11/XWayland connection/window errors.
    pub fn new(width: u32, height: u32, title: &str) -> Result<Self, MediaError> {
        Self::create(width, height, title, false)
    }

    /// Creates a borderless fullscreen Student presentation. The window manager
    /// decides fullscreen/stacking policy; this is not an operating-system lock.
    /// # Errors
    /// Returns invalid-size errors or X11/XWayland connection/window errors.
    pub fn new_student_presentation(
        width: u32,
        height: u32,
        title: &str,
    ) -> Result<Self, MediaError> {
        Self::create(width, height, title, true)
    }

    fn create(width: u32, height: u32, title: &str, fullscreen: bool) -> Result<Self, MediaError> {
        if width == 0 || height == 0 || width > 8192 || height > 8192 {
            return Err(MediaError::Operation(
                "invalid Linux render dimensions".into(),
            ));
        }
        let connection = if fullscreen {
            Some(Connection::open()?)
        } else {
            None
        };
        let (window_width, window_height) = connection
            .as_ref()
            .map_or((width as usize, height as usize), Connection::screen_size);
        let mut window = Window::new(
            title,
            window_width,
            window_height,
            WindowOptions {
                borderless: fullscreen,
                title: !fullscreen,
                // Let EWMH fullscreen configure the actual monitor geometry. A fixed
                // X11 size hint would prevent this on multi-monitor desktops.
                resize: true,
                scale_mode: ScaleMode::AspectRatioStretch,
                ..WindowOptions::default()
            },
        )
        .map_err(operation)?;
        window.set_background_color(0, 0, 0);
        window.set_target_fps(0);
        let presentation = if let Some(connection) = connection {
            window.set_position(0, 0);
            window.set_cursor_visibility(false);
            let presentation = Presentation {
                connection,
                window: window.get_window_handle() as xlib::Window,
                last_request: Instant::now(),
            };
            presentation.request();
            Some(presentation)
        } else {
            None
        };
        Ok(Self {
            presented_window_size: window.get_size(),
            window,
            surface: Surface::default(),
            presentation,
            last_event_pump: Instant::now(),
            presentation_failed: false,
        })
    }

    #[must_use]
    pub fn is_open(&mut self) -> bool {
        if self.presentation_failed {
            return false;
        }
        // Static slides still need a bounded native event pump for close,
        // resizing and disconnect. No waiting/sleeping occurs in this pump.
        if self.last_event_pump.elapsed() >= Duration::from_millis(10) {
            self.window.update();
            self.last_event_pump = Instant::now();
        }
        let size = self.window.get_size();
        if size != self.presented_window_size
            && size.0 > 0
            && size.1 > 0
            && !self.surface.pixels.is_empty()
        {
            // EWMH fullscreen and display changes can resize a static slide
            // without another video/pointer event. Repaint once per geometry
            // change; never turn the idle event pump into a frame upload loop.
            if let Err(error) = self.window.update_with_buffer(
                &self.surface.pixels,
                self.surface.width,
                self.surface.height,
            ) {
                // is_open has no error channel. Close this presentation so its
                // owner tears down the native surface instead of keeping a
                // stale image; preserve the backend reason in process stderr.
                eprintln!("native Linux renderer resize presentation failed: {error}");
                self.presentation_failed = true;
                return false;
            }
            self.presented_window_size = size;
            self.last_event_pump = Instant::now();
        }
        if let Some(presentation) = &mut self.presentation {
            if presentation.last_request.elapsed() >= Duration::from_secs(1) {
                presentation.request();
                presentation.last_request = Instant::now();
            }
        }
        self.window.is_open()
    }
}

impl Renderer for NativeRenderer {
    fn set_pointer(&mut self, pointer: Option<PointerOverlay>) {
        self.surface.set_pointer(pointer);
    }

    fn redraw(&mut self) -> Result<bool, MediaError> {
        if !self.surface.redraw_pointer() {
            return Ok(false);
        }
        let size = self.window.get_size();
        self.window
            .update_with_buffer(
                &self.surface.pixels,
                self.surface.width,
                self.surface.height,
            )
            .map_err(operation)?;
        self.last_event_pump = Instant::now();
        self.presented_window_size = size;
        Ok(true)
    }

    fn render(&mut self, frame: DecodedFrameRef<'_>) -> Result<(), MediaError> {
        self.surface.replace(frame)?;
        let size = self.window.get_size();
        self.window
            .update_with_buffer(
                &self.surface.pixels,
                self.surface.width,
                self.surface.height,
            )
            .map_err(operation)?;
        self.last_event_pump = Instant::now();
        self.presented_window_size = size;
        Ok(())
    }
}

impl Drop for NativeRenderer {
    fn drop(&mut self) {
        if self.presentation.is_some() {
            self.window.set_cursor_visibility(true);
        }
    }
}

struct Connection {
    api: xlib::Xlib,
    display: *mut xlib::Display,
}

impl fmt::Debug for Connection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("X11Connection")
            .finish_non_exhaustive()
    }
}

impl Connection {
    fn open() -> Result<Self, MediaError> {
        let api = xlib::Xlib::open().map_err(operation)?;
        // SAFETY: initialize Xlib locking before this backend's first Xlib call.
        // minifb also initializes it when it owns the first display connection.
        if unsafe { (api.XInitThreads)() } == 0 {
            return Err(MediaError::BackendUnavailable(
                "Xlib thread initialization failed".into(),
            ));
        }
        // SAFETY: null selects DISPLAY; the RAII object exclusively owns and
        // closes this connection on the window thread. No pointer escapes.
        let display = unsafe { (api.XOpenDisplay)(ptr::null()) };
        if display.is_null() {
            return Err(MediaError::BackendUnavailable(
                "Ubuntu presentation requires X11 or XWayland (check DISPLAY)".into(),
            ));
        }
        Ok(Self { api, display })
    }

    fn screen_size(&self) -> (usize, usize) {
        // SAFETY: display remains live for this connection's lifetime.
        unsafe {
            let screen = (self.api.XDefaultScreen)(self.display);
            let width = (self.api.XDisplayWidth)(self.display, screen);
            let height = (self.api.XDisplayHeight)(self.display, screen);
            (
                usize::try_from(width).unwrap_or(1).max(1),
                usize::try_from(height).unwrap_or(1).max(1),
            )
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        // SAFETY: the connection is owned here and closed exactly once.
        unsafe {
            (self.api.XCloseDisplay)(self.display);
        }
    }
}

#[derive(Debug)]
struct Presentation {
    connection: Connection,
    window: xlib::Window,
    last_request: Instant,
}

impl Presentation {
    fn request(&self) {
        // EWMH mapped-window state change, per
        // https://specifications.freedesktop.org/wm/latest/ar01s05.html#idm140200472615568
        // SAFETY: this display is live, window is minifb's X11 XID (not a
        // pointer), atom strings are NUL terminated, and the event is initialized.
        unsafe {
            let api = &self.connection.api;
            let display = self.connection.display;
            let root = (api.XDefaultRootWindow)(display);
            let state = (api.XInternAtom)(display, c"_NET_WM_STATE".as_ptr(), 0);
            let fullscreen = (api.XInternAtom)(display, c"_NET_WM_STATE_FULLSCREEN".as_ptr(), 0);
            let above = (api.XInternAtom)(display, c"_NET_WM_STATE_ABOVE".as_ptr(), 0);
            let mut event: xlib::XEvent = std::mem::zeroed();
            let mut data = xlib::ClientMessageData::new();
            data.set_long(0, 1); // _NET_WM_STATE_ADD
            data.set_long(1, std::os::raw::c_long::try_from(fullscreen).unwrap_or(0));
            data.set_long(2, std::os::raw::c_long::try_from(above).unwrap_or(0));
            data.set_long(3, 1); // normal application source
            event.client_message = xlib::XClientMessageEvent {
                type_: xlib::ClientMessage,
                serial: 0,
                send_event: 1,
                display,
                window: self.window,
                message_type: state,
                format: 32,
                data,
            };
            (api.XSendEvent)(
                display,
                root,
                0,
                xlib::SubstructureRedirectMask | xlib::SubstructureNotifyMask,
                &raw mut event,
            );
            (api.XFlush)(display);
        }
    }
}

fn operation(error: impl fmt::Display) -> MediaError {
    MediaError::Operation(format!("native Linux renderer failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires X11; run under xvfb-run -a"]
    fn hidden_host_stays_unmapped_while_pumping() {
        let mut window = Window::new(
            "Classroom hidden-host test",
            1,
            1,
            WindowOptions {
                visible: false,
                ..WindowOptions::default()
            },
        )
        .unwrap();
        window.set_target_fps(0);
        window.update();
        window.set_visible(false);
        window.update();
        assert!(!window.is_visible());
        let connection = Connection::open().unwrap();
        // SAFETY: the display and minifb XID remain live through this query;
        // attributes is initialized storage for Xlib's out-parameter.
        unsafe {
            let mut attributes: xlib::XWindowAttributes = std::mem::zeroed();
            assert_ne!(
                (connection.api.XGetWindowAttributes)(
                    connection.display,
                    window.get_window_handle() as xlib::Window,
                    &raw mut attributes,
                ),
                0
            );
            assert_eq!(attributes.map_state, xlib::IsUnmapped);
        }
    }
}
