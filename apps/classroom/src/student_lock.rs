//! Classroom lock screen: an always-on-top window over every monitor that hides
//! the Student desktop and, on Windows, swallows all keyboard and mouse input
//! until the Teacher unlocks it. The Student cannot dismiss it;
//! `StudentToolsUi` removes it when the Teacher unlocks, a different Teacher
//! session takes over, or the Teacher has been silent for too long.
use super::dashboard_view::Canvas;
use anyhow::Result;
use minifb::{Window, WindowOptions};
use std::time::{Duration, Instant};

const TITLE: &str = "Classroom · Screen locked";
const BACKGROUND: u32 = 0x000f_1b24;
const ACCENT: u32 = 0x007d_d3dc;
const TEXT: u32 = 0x00e6_eef2;
const MUTED: u32 = 0x009a_b0bb;
const REDRAW: Duration = Duration::from_millis(250);

/// `(x, y, width, height)` of the area to cover: every monitor on Windows.
#[cfg(windows)]
#[allow(unsafe_code)]
fn screen_rect() -> Option<(isize, isize, usize, usize)> {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };
    // SAFETY: GetSystemMetrics has no preconditions and only reads system state.
    let (x, y, w, h) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    };
    Some((
        isize::try_from(x).ok()?,
        isize::try_from(y).ok()?,
        usize::try_from(w).ok().filter(|w| *w > 0)?,
        usize::try_from(h).ok().filter(|h| *h > 0)?,
    ))
}
#[cfg(target_os = "macos")]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn screen_rect() -> Option<(isize, isize, usize, usize)> {
    let size = core_graphics::display::CGDisplay::main().bounds().size;
    Some((0, 0, size.width as usize, size.height as usize))
}
#[cfg(not(any(windows, target_os = "macos")))]
fn screen_rect() -> Option<(isize, isize, usize, usize)> {
    None
}

/// While locked, low-level hooks swallow every keyboard and mouse event system
/// wide (Windows key, Alt+Tab, Win+R, Ctrl+Shift+Esc, clicks…). Only Ctrl+Alt+Del,
/// which Windows handles in the kernel, still works. Windows silently skips a
/// hook whose thread does not service messages promptly, so the thread that
/// installs it must keep pumping them (see `Lock`).
#[cfg(windows)]
#[allow(unsafe_code)]
mod input {
    use std::sync::atomic::{AtomicBool, Ordering};
    use windows::Win32::{
        Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, WH_KEYBOARD_LL,
            WH_MOUSE_LL,
        },
    };

    /// The hook callbacks have no user-data slot, so the "locked" switch is the
    /// one piece of process-wide state; it is only ever set by `Guard`.
    static ACTIVE: AtomicBool = AtomicBool::new(false);

    unsafe extern "system" fn swallow(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
        if code >= 0 && ACTIVE.load(Ordering::Acquire) {
            return LRESULT(1);
        }
        // SAFETY: forwards the unchanged hook arguments.
        unsafe { CallNextHookEx(None, code, w, l) }
    }

    pub struct Guard(Vec<HHOOK>);
    impl Guard {
        /// Installs both hooks on the calling thread.
        pub fn install() -> Self {
            // SAFETY: `swallow` matches HOOKPROC and the module handle is this process.
            let hooks: Vec<HHOOK> = unsafe {
                let module = GetModuleHandleW(None).ok().map(|m| HINSTANCE(m.0));
                [WH_KEYBOARD_LL, WH_MOUSE_LL]
                    .into_iter()
                    .filter_map(|kind| SetWindowsHookExW(kind, Some(swallow), module, 0).ok())
                    .collect()
            };
            tracing::info!(hooks = hooks.len(), "Lock screen input hooks installed");
            ACTIVE.store(!hooks.is_empty(), Ordering::Release);
            Self(hooks)
        }
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            ACTIVE.store(false, Ordering::Release);
            for hook in self.0.drain(..) {
                // SAFETY: each hook came from SetWindowsHookExW and is released once.
                let _ = unsafe { UnhookWindowsHookEx(hook) };
            }
        }
    }

    /// Dispatches every pending message of this thread (this is also when
    /// Windows calls the hooks).
    pub fn pump_messages() {
        use windows::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
        };
        let mut message = MSG::default();
        // SAFETY: `message` is a valid out-parameter owned by this frame.
        unsafe {
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
}

/// Keeps the window above everything, including the taskbar.
#[cfg(windows)]
#[allow(unsafe_code)]
fn raise(window: &Window) {
    use windows::Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{
            FindWindowW, GetWindowThreadProcessId, IsWindowVisible, SetForegroundWindow,
            SetWindowPos, HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
        },
    };
    let handle = HWND(window.get_window_handle());
    // SAFETY: `handle` is this process's live window; both calls only restack it.
    unsafe {
        // The passive monitoring notice stays above the lock window. Raising
        // both to HWND_TOPMOST on different timers made their z-order alternate.
        let notice = FindWindowW(None, windows::core::w!("Classroom monitoring notice"))
            .ok()
            .filter(|notice| {
                let mut pid = 0;
                GetWindowThreadProcessId(*notice, Some(&mut pid));
                pid == std::process::id() && IsWindowVisible(*notice).as_bool()
            });
        let _ = SetWindowPos(
            handle,
            Some(notice.unwrap_or(HWND_TOPMOST)),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
        );
        let _ = SetForegroundWindow(handle);
    }
}
#[cfg(not(windows))]
fn raise(_window: &Window) {}

pub struct LockScreen {
    window: Window,
    canvas: Canvas,
    pub epoch: u64,
    message: String,
    last_draw: Instant,
    #[cfg(windows)]
    _input: input::Guard,
}
impl LockScreen {
    pub fn new(epoch: u64, message: &str) -> Result<Self> {
        let mut window = Window::new(
            TITLE,
            320,
            200,
            WindowOptions {
                visible: false,
                topmost: true,
                resize: false,
                borderless: true,
                title: false,
                ..Default::default()
            },
        )?;
        window.set_target_fps(0);
        let (x, y, w, h) = screen_rect().unwrap_or_else(|| {
            let area = window.work_area();
            (
                0,
                0,
                usize::try_from(area[2]).unwrap_or(1280),
                usize::try_from(area[3]).unwrap_or(720),
            )
        });
        window.resize_notification(w, h)?;
        window.set_outer_origin(x, y);
        window.set_cursor_visibility(false);
        let mut lock = Self {
            window,
            canvas: Canvas::new(w, h),
            epoch,
            message: message.to_owned(),
            last_draw: Instant::now() - REDRAW,
            #[cfg(windows)]
            _input: input::Guard::install(),
        };
        lock.present()?;
        lock.window.set_visible(true);
        raise(&lock.window);
        Ok(lock)
    }
    pub fn set_message(&mut self, epoch: u64, message: &str) {
        self.epoch = epoch;
        message.clone_into(&mut self.message);
        self.last_draw = Instant::now() - REDRAW;
    }
    /// Redraws a few times per second, which also pumps window/hook messages
    /// and re-asserts the window on top.
    pub fn pump(&mut self) -> Result<()> {
        anyhow::ensure!(self.window.is_open(), "Lock window was closed");
        if self.last_draw.elapsed() >= REDRAW {
            raise(&self.window);
            self.present()?;
        }
        Ok(())
    }
    fn present(&mut self) -> Result<()> {
        self.canvas.set_density(self.window.backing_scale());
        self.draw();
        self.window.update_with_buffer(
            &self.canvas.pixels,
            self.canvas.pixel_width(),
            self.canvas.pixel_height(),
        )?;
        self.last_draw = Instant::now();
        Ok(())
    }
    fn draw(&mut self) {
        let (w, h) = (self.canvas.width, self.canvas.height);
        self.canvas.fill_rect([0, 0, w, h], BACKGROUND);
        let cx = w / 2;
        let top = h / 3;
        // Padlock: shackle (three bars) above a solid body.
        self.canvas.fill_rect([cx - 22, top - 38, 8, 40], ACCENT);
        self.canvas.fill_rect([cx + 14, top - 38, 8, 40], ACCENT);
        self.canvas.fill_rect([cx - 22, top - 46, 44, 8], ACCENT);
        self.canvas.fill_rect([cx - 38, top, 76, 58], ACCENT);
        self.canvas
            .fill_rect([cx - 5, top + 16, 10, 22], BACKGROUND);
        let title = "This computer is locked";
        let tw = self.canvas.text_width(title, 2);
        self.canvas
            .text(cx.saturating_sub(tw / 2), top + 92, title, TEXT, 2);
        let mut y = top + 140;
        let lines = super::student_tools_ui::wrap(&self.canvas, &self.message, w.min(900) - 80);
        for line in lines.iter().take(8).filter(|l| !l.is_empty()) {
            let lw = self.canvas.text_width(line, 1);
            self.canvas
                .text(cx.saturating_sub(lw / 2), y, line, TEXT, 1);
            y += 26;
        }
        let note = "Your teacher will unlock it shortly";
        let nw = self.canvas.text_width(note, 1);
        self.canvas
            .text(cx.saturating_sub(nw / 2), y + 14, note, MUTED, 1);
    }
}

/// Owner of a lock screen. On Windows the window and its input hooks live on a
/// dedicated thread that services messages continuously; elsewhere (macOS
/// requires the main thread) the UI thread drives the window directly.
#[cfg(windows)]
pub struct Lock {
    epoch: u64,
    shared: std::sync::Arc<Shared>,
    thread: Option<std::thread::JoinHandle<()>>,
}
#[cfg(windows)]
struct Shared {
    message: std::sync::Mutex<(u64, String)>,
    stop: std::sync::atomic::AtomicBool,
    alive: std::sync::atomic::AtomicBool,
    heartbeat_ms: std::sync::atomic::AtomicU64,
}
#[cfg(windows)]
fn now_ms() -> u64 {
    media_core::unix_us() / 1000
}
#[cfg(windows)]
impl Lock {
    /// If the UI thread stops calling `pump` for this long, the lock lifts.
    const ORPHAN_MS: u64 = 10_000;
    pub fn start(epoch: u64, message: &str) -> Result<Self> {
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        let shared = std::sync::Arc::new(Shared {
            message: std::sync::Mutex::new((epoch, message.to_owned())),
            stop: AtomicBool::new(false),
            alive: AtomicBool::new(true),
            heartbeat_ms: AtomicU64::new(now_ms()),
        });
        let (ready, started) = std::sync::mpsc::channel::<std::result::Result<(), String>>();
        let worker = std::sync::Arc::clone(&shared);
        let thread = std::thread::spawn(move || {
            let (epoch, message) = worker.message.lock().unwrap().clone();
            let mut screen = match LockScreen::new(epoch, &message) {
                Ok(screen) => {
                    let _ = ready.send(Ok(()));
                    screen
                }
                Err(error) => {
                    let _ = ready.send(Err(format!("{error:#}")));
                    worker.alive.store(false, Ordering::Release);
                    return;
                }
            };
            while !worker.stop.load(Ordering::Acquire)
                && now_ms().saturating_sub(worker.heartbeat_ms.load(Ordering::Acquire))
                    < Self::ORPHAN_MS
            {
                input::pump_messages();
                let (epoch, message) = worker.message.lock().unwrap().clone();
                if epoch != screen.epoch {
                    screen.set_message(epoch, &message);
                }
                if screen.pump().is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(4));
            }
            worker.alive.store(false, Ordering::Release);
        });
        match started.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(Self {
                epoch,
                shared,
                thread: Some(thread),
            }),
            Ok(Err(error)) => Err(anyhow::anyhow!(error)),
            Err(_) => Err(anyhow::anyhow!("Lock window did not start")),
        }
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn update(&mut self, epoch: u64, message: &str) {
        self.epoch = epoch;
        *self.shared.message.lock().unwrap() = (epoch, message.to_owned());
    }
    /// Heartbeat from the UI thread; `Err` once the lock thread has ended.
    pub fn pump(&mut self) -> Result<()> {
        use std::sync::atomic::Ordering;
        self.shared.heartbeat_ms.store(now_ms(), Ordering::Release);
        anyhow::ensure!(
            self.shared.alive.load(Ordering::Acquire),
            "Lock window ended"
        );
        Ok(())
    }
}
#[cfg(windows)]
impl Drop for Lock {
    fn drop(&mut self) {
        self.shared
            .stop
            .store(true, std::sync::atomic::Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(not(windows))]
pub struct Lock(LockScreen);
#[cfg(not(windows))]
impl Lock {
    pub fn start(epoch: u64, message: &str) -> Result<Self> {
        LockScreen::new(epoch, message).map(Self)
    }
    pub fn epoch(&self) -> u64 {
        self.0.epoch
    }
    pub fn update(&mut self, epoch: u64, message: &str) {
        self.0.set_message(epoch, message);
    }
    pub fn pump(&mut self) -> Result<()> {
        self.0.pump()
    }
}
