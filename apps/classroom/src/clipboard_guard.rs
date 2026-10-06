//! Class-wide clipboard control: while active, anything copied on the Student
//! is immediately discarded (the clipboard is cleared, not read or logged).
//! This blocks sharing answers by copy/paste during a locked-down exam
//! without ever inspecting what was copied.
//!
//! Windows only, no administrator rights required: a message-only window
//! registers as a clipboard format listener and empties the clipboard on
//! every `WM_CLIPBOARDUPDATE` while [`ClipboardGuard`] is alive.
#![cfg_attr(windows, allow(unsafe_code))]

#[cfg(windows)]
mod imp {
    use anyhow::{Context, Result};
    use std::{sync::mpsc, thread, time::Duration};
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::{HWND, LPARAM, LRESULT, WPARAM},
            System::{
                DataExchange::{AddClipboardFormatListener, CloseClipboard, EmptyClipboard, OpenClipboard},
                LibraryLoader::GetModuleHandleW,
            },
            UI::WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PostMessageW,
                PostQuitMessage, RegisterClassW, TranslateMessage, CW_USEDEFAULT, HWND_MESSAGE, MSG, WM_CLIPBOARDUPDATE,
                WM_CLOSE, WNDCLASSW, WS_OVERLAPPED,
            },
        },
    };

    unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_CLIPBOARDUPDATE => {
                // SAFETY: standard, self-contained clipboard sequence; a transient
                // failure (another app briefly holding the clipboard) is ignored
                // and simply tried again on the next copy.
                unsafe {
                    if OpenClipboard(Some(hwnd)).is_ok() {
                        let _ = EmptyClipboard();
                        let _ = CloseClipboard();
                    }
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                // SAFETY: ends this thread's message loop cleanly; `hwnd` is this
                // window.
                unsafe {
                    PostQuitMessage(0);
                }
                LRESULT(0)
            }
            // SAFETY: forwards the unchanged message arguments.
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    pub struct ClipboardGuard {
        hwnd: isize,
        worker: Option<thread::JoinHandle<()>>,
    }
    impl ClipboardGuard {
        pub fn start() -> Result<Self> {
            let (ready, started) = mpsc::channel::<Result<isize, String>>();
            let worker = thread::spawn(move || {
                let outcome = (|| -> Result<isize> {
                    let class_name: Vec<u16> = "ClassroomClipboardGuard\0".encode_utf16().collect();
                    // SAFETY: `module`/`class_name` outlive the registration and
                    // window creation calls below.
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
                        AddClipboardFormatListener(hwnd).context("AddClipboardFormatListener failed")?;
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
                // SAFETY: `message` is a valid out-parameter owned by this frame;
                // this loop runs only on the thread that created `hwnd`, as
                // required for window message delivery.
                unsafe {
                    while GetMessageW(&mut message, None, 0, 0).as_bool() {
                        let _ = TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
                let _ = hwnd; // kept alive only for clarity; the window is gone by now
            });
            let hwnd = started
                .recv_timeout(Duration::from_secs(3))
                .context("Clipboard guard did not start")?
                .map_err(|error| anyhow::anyhow!(error))?;
            Ok(Self { hwnd, worker: Some(worker) })
        }
    }
    impl Drop for ClipboardGuard {
        fn drop(&mut self) {
            // SAFETY: `self.hwnd` is this guard's own message-only window;
            // WM_CLOSE there ends the loop above without touching the clipboard.
            unsafe {
                let _ = PostMessageW(Some(HWND(self.hwnd as *mut core::ffi::c_void)), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }
    // A window handle is just an opaque, process-wide value here; only the
    // owning thread ever dereferences it (inside the message loop).
    unsafe impl Send for ClipboardGuard {}
}

#[cfg(windows)]
pub use imp::ClipboardGuard;

#[cfg(not(windows))]
pub struct ClipboardGuard;
#[cfg(not(windows))]
impl ClipboardGuard {
    pub fn start() -> anyhow::Result<Self> {
        anyhow::bail!("Clipboard control is only available on Windows")
    }
}
