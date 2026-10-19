//! Clipboard **monitoring** — distinct from `clipboard_guard`'s blocking-only
//! mode, which never reads what was copied. This instead records each copy's
//! text while the Teacher has activity monitoring switched on for the class,
//! for exam-integrity review (e.g. a student pasting an answer they copied
//! from elsewhere). It starts and stops with activity monitoring, exactly
//! like the rest of [`super::collector`], and records only plain text — never
//! images, files, or other clipboard formats.
//!
//! This is deliberately *not* a keystroke logger: it sees only what the
//! Student explicitly copies, not everything typed everywhere.
#![cfg_attr(windows, allow(unsafe_code))]
use super::{now_ms, ClipboardEntry};

/// Longer copies are kept only up to this many characters (plenty for code,
/// bounds memory for an accidental whole-document copy).
pub const MAX_CHARS: usize = 20_000;
/// Entries older than this are not worth keeping even within today.
pub const MAX_ENTRIES: usize = 500;

#[cfg(windows)]
mod imp {
    use super::{ClipboardEntry, MAX_CHARS};
    use anyhow::{Context, Result};
    use std::{mem::ManuallyDrop, sync::mpsc, thread, time::Duration};
    use windows::core::PCWSTR;
    use windows::Win32::{
        Foundation::{HGLOBAL, HWND, LPARAM, LRESULT, WPARAM},
        System::{
            DataExchange::{
                AddClipboardFormatListener, CloseClipboard, GetClipboardData, OpenClipboard,
            },
            LibraryLoader::GetModuleHandleW,
            Memory::{GlobalLock, GlobalSize, GlobalUnlock},
        },
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DispatchMessageW, GetForegroundWindow, GetMessageW,
            GetWindowThreadProcessId, PostMessageW, PostQuitMessage, RegisterClassW,
            TranslateMessage, CW_USEDEFAULT, HWND_MESSAGE, MSG, WM_CLIPBOARDUPDATE, WM_CLOSE,
            WNDCLASSW, WS_OVERLAPPED,
        },
    };

    const CF_UNICODETEXT: u32 = 13;

    /// The foreground window's process name at the moment of the copy, best
    /// effort (helps tell "copied from the exam" apart from "copied from
    /// somewhere else").
    fn source_app() -> String {
        // SAFETY: plain, self-contained queries; `pid` is a valid out-param.
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return String::new();
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 {
                return String::new();
            }
            super::super::process_name_for_pid(pid)
        }
    }

    fn read_clipboard_text(hwnd: HWND) -> Option<String> {
        // SAFETY: a short-lived, self-contained clipboard open/read/close; a
        // transient failure (another app briefly holding the clipboard) just
        // means this one copy is missed.
        unsafe {
            OpenClipboard(Some(hwnd)).ok()?;
            let result = (|| {
                let handle = GetClipboardData(CF_UNICODETEXT).ok()?;
                let memory = HGLOBAL(handle.0);
                let size = GlobalSize(memory);
                if size == 0 || size > (MAX_CHARS + 1) * 8 {
                    return None;
                }
                let ptr = GlobalLock(memory);
                if ptr.is_null() {
                    return None;
                }
                // SAFETY: `ptr` is valid for `size` bytes while locked, and a
                // CF_UNICODETEXT payload is UTF-16 with a NUL terminator.
                let text = {
                    let units = std::slice::from_raw_parts(ptr.cast::<u16>(), size as usize / 2);
                    let end = units.iter().position(|&c| c == 0).unwrap_or(units.len());
                    String::from_utf16_lossy(&units[..end])
                };
                let _ = GlobalUnlock(memory);
                Some(text)
            })();
            let _ = CloseClipboard();
            result
        }
    }

    pub struct Watcher {
        hwnd: isize,
        worker: Option<thread::JoinHandle<()>>,
    }
    impl Watcher {
        pub fn start(on_copy: impl Fn(ClipboardEntry) + Send + 'static) -> Result<Self> {
            let (ready, started) = mpsc::channel::<Result<isize, String>>();
            let worker = thread::spawn(move || {
                let outcome = (|| -> Result<isize> {
                    let class_name: Vec<u16> =
                        "ClassroomClipboardMonitor\0".encode_utf16().collect();
                    // SAFETY: `class_name` outlives the registration/creation calls.
                    let hwnd = unsafe {
                        let module = GetModuleHandleW(None).context("GetModuleHandleW failed")?;
                        let instance = windows::Win32::Foundation::HINSTANCE(module.0);
                        let class = WNDCLASSW {
                            lpfnWndProc: Some(wndproc),
                            hInstance: instance,
                            lpszClassName: PCWSTR(class_name.as_ptr()),
                            ..Default::default()
                        };
                        anyhow::ensure!(RegisterClassW(&class) != 0, "RegisterClassW failed");
                        let hwnd = CreateWindowExW(
                            Default::default(),
                            PCWSTR(class_name.as_ptr()),
                            PCWSTR::null(),
                            WS_OVERLAPPED,
                            CW_USEDEFAULT,
                            CW_USEDEFAULT,
                            CW_USEDEFAULT,
                            CW_USEDEFAULT,
                            Some(HWND_MESSAGE),
                            None,
                            Some(instance),
                            None,
                        )?;
                        AddClipboardFormatListener(hwnd)
                            .context("AddClipboardFormatListener failed")?;
                        hwnd
                    };
                    Ok(hwnd.0 as isize)
                })();
                let hwnd = match outcome {
                    Ok(hwnd) => {
                        let _ = ready.send(Ok(hwnd));
                        hwnd
                    }
                    Err(error) => {
                        let _ = ready.send(Err(format!("{error:#}")));
                        return;
                    }
                };
                let mut message = MSG::default();
                // SAFETY: this loop runs only on the thread that created `hwnd`,
                // as window message delivery requires.
                unsafe {
                    while GetMessageW(&mut message, None, 0, 0).as_bool() {
                        if message.message == WM_CLIPBOARDUPDATE {
                            if let Some(text) =
                                read_clipboard_text(HWND(hwnd as *mut core::ffi::c_void))
                            {
                                let text = text.trim();
                                if !text.is_empty() {
                                    let truncated = text.chars().count() > MAX_CHARS;
                                    let text: String = text.chars().take(MAX_CHARS).collect();
                                    on_copy(ClipboardEntry {
                                        text,
                                        truncated,
                                        app: source_app(),
                                        unix_ms: super::super::now_ms(),
                                        duplicates: 0,
                                    });
                                }
                            }
                        }
                        let _ = TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
            });
            let hwnd = started
                .recv_timeout(Duration::from_secs(3))
                .context("Clipboard monitor did not start")?
                .map_err(|error| anyhow::anyhow!(error))?;
            Ok(Self {
                hwnd,
                worker: Some(worker),
            })
        }
    }
    impl Drop for Watcher {
        fn drop(&mut self) {
            // SAFETY: `self.hwnd` is this watcher's own message-only window;
            // WM_CLOSE there ends the loop above.
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(self.hwnd as *mut core::ffi::c_void)),
                    WM_CLOSE,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }
    unsafe extern "system" fn wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if msg == WM_CLOSE {
            // SAFETY: ends this thread's message loop cleanly; `hwnd` is this window.
            unsafe {
                PostQuitMessage(0);
            }
            return LRESULT(0);
        }
        // SAFETY: forwards the unchanged message arguments.
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }
    // A window handle is just an opaque, process-wide value here; only the
    // owning thread ever dereferences it (inside the message loop).
    unsafe impl Send for Watcher {}
    #[allow(dead_code)]
    fn unused(_: ManuallyDrop<()>) {}
}

#[cfg(windows)]
pub use imp::Watcher;

#[cfg(not(windows))]
pub struct Watcher;
#[cfg(not(windows))]
impl Watcher {
    pub fn start(_on_copy: impl Fn(ClipboardEntry) + Send + 'static) -> anyhow::Result<Self> {
        anyhow::bail!("Clipboard monitoring is only available on Windows")
    }
}

#[allow(dead_code)]
fn unused(_: fn() -> i64) {
    let _ = now_ms;
}
