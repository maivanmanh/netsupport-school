use std::{
    mem::size_of,
    ptr,
    time::{Duration, Instant},
};

use media_core::{DecodedFrameRef, MediaError, Renderer};
use windows::{
    core::{w, Error as WindowsError},
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            GetDC, PatBlt, ReleaseDC, SetBrushOrgEx, SetStretchBltMode, StretchDIBits, BITMAPINFO,
            BITMAPINFOHEADER, BI_RGB, BLACKNESS, DIB_RGB_COLORS, HALFTONE, HDC, SRCCOPY,
        },
        Media::{timeBeginPeriod, timeEndPeriod},
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
            GetSystemMetrics, GetWindowLongW, GetWindowRect, IsIconic, IsWindowVisible,
            PeekMessageW, PostQuitMessage, RegisterClassW, SetWindowPos, ShowWindow,
            TranslateMessage, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, GWL_EXSTYLE, HWND_TOPMOST,
            MSG, PM_REMOVE, SM_CXSCREEN, SM_CYSCREEN, SWP_SHOWWINDOW, SW_RESTORE, SW_SHOW,
            WINDOW_EX_STYLE, WM_CLOSE, WM_DESTROY, WM_QUIT, WNDCLASSW, WS_EX_TOPMOST,
            WS_OVERLAPPEDWINDOW, WS_POPUP, WS_VISIBLE,
        },
    },
};

pub struct NativeRenderer {
    window: HWND,
    open: bool,
    forced_presentation: bool,
    last_enforced: Instant,
    timer_period_active: bool,
    gpu: Option<crate::gpu::GpuRenderer>,
    diagnostics: media_core::diagnostics::StageMetrics,
    /// O1 pointer; also blended on the CPU when the GDI fallback is active.
    pointer: Option<media_core::PointerOverlay>,
}

impl NativeRenderer {
    /// Creates the native M2 preview window.
    ///
    /// # Errors
    /// Returns an error when the Win32 window class or window cannot be created.
    pub fn new(_width: u32, _height: u32, _title: &str) -> Result<Self, MediaError> {
        Self::create(false)
    }

    /// Creates a borderless, always-on-top Student presentation window.
    ///
    /// # Errors
    /// Returns an error when Windows cannot create or size the presentation window.
    pub fn new_student_presentation(
        _width: u32,
        _height: u32,
        _title: &str,
    ) -> Result<Self, MediaError> {
        Self::create(true)
    }

    #[allow(clippy::too_many_lines)] // Keep native class/window ownership and fallback setup together.
    fn create(forced_presentation: bool) -> Result<Self, MediaError> {
        // SAFETY: the window class and window are created and used on this media worker thread.
        unsafe {
            let module = GetModuleHandleW(None)
                .map_err(|error| win_error("get renderer module handle", error))?;
            let instance = HINSTANCE(module.0);
            let class_name = if forced_presentation {
                w!("ClassroomStudentPresentation")
            } else {
                w!("ClassroomM2Preview")
            };
            let class = WNDCLASSW {
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(if forced_presentation {
                    presentation_window_proc
                } else {
                    window_proc
                }),
                hInstance: instance,
                lpszClassName: class_name,
                ..Default::default()
            };
            // A reconnect can create this class again in the same process. If registration
            // genuinely failed, CreateWindowExW below returns the actionable Win32 error.
            let _ = RegisterClassW(ptr::addr_of!(class));
            let (extended_style, style, x, y, window_width, window_height) = if forced_presentation
            {
                (
                    WS_EX_TOPMOST,
                    WS_POPUP | WS_VISIBLE,
                    0,
                    0,
                    GetSystemMetrics(SM_CXSCREEN),
                    GetSystemMetrics(SM_CYSCREEN),
                )
            } else {
                (
                    WINDOW_EX_STYLE::default(),
                    WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                    CW_USEDEFAULT,
                    CW_USEDEFAULT,
                    960,
                    540,
                )
            };
            if window_width <= 0 || window_height <= 0 {
                return Err(MediaError::Operation(
                    "Windows reported invalid presentation dimensions".to_owned(),
                ));
            }
            let window = CreateWindowExW(
                extended_style,
                class_name,
                if forced_presentation {
                    w!("Classroom — Teacher presentation")
                } else {
                    w!("Classroom — M2 local H.264 preview")
                },
                style,
                x,
                y,
                window_width,
                window_height,
                None,
                None,
                Some(instance),
                None,
            )
            .map_err(|error| win_error("create native preview window", error))?;
            if forced_presentation {
                SetWindowPos(
                    window,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    window_width,
                    window_height,
                    SWP_SHOWWINDOW,
                )
                .map_err(|error| win_error("enforce Student presentation window", error))?;
            }
            let _ = ShowWindow(window, SW_SHOW);
            let gpu = if std::env::var("CLASSROOM_GDI_RENDERER").as_deref() == Ok("1") {
                tracing::info!(
                    renderer_backend = "gdi",
                    "diagnostic GDI renderer override enabled"
                );
                None
            } else {
                match crate::gpu::GpuRenderer::new(
                    window,
                    u32::try_from(window_width).unwrap_or(960),
                    u32::try_from(window_height).unwrap_or(540),
                ) {
                    Ok(gpu) => Some(gpu),
                    Err(error) => {
                        tracing::warn!(%error, renderer_backend = "gdi", "D3D11 renderer unavailable; using filtered GDI fallback");
                        None
                    }
                }
            };
            Ok(Self {
                window,
                open: true,
                forced_presentation,
                last_enforced: Instant::now(),
                // Only while this presentation is alive; balance on this thread.
                timer_period_active: timeBeginPeriod(1) == 0,
                gpu,
                pointer: None,
                diagnostics: media_core::diagnostics::StageMetrics::default(),
            })
        }
    }

    #[must_use]
    pub fn is_open(&mut self) -> bool {
        self.pump_messages();
        if self.forced_presentation
            && self.open
            && self.last_enforced.elapsed() >= Duration::from_secs(1)
        {
            let _ = self.enforce_presentation_window();
            self.last_enforced = Instant::now();
        }
        self.open
    }

    fn enforce_presentation_window(&self) -> Result<(), MediaError> {
        // SAFETY: this renderer owns a valid top-level window on the calling thread.
        unsafe {
            let width = GetSystemMetrics(SM_CXSCREEN);
            let height = GetSystemMetrics(SM_CYSCREEN);
            if IsIconic(self.window).as_bool() {
                let _ = ShowWindow(self.window, SW_RESTORE);
            }
            let mut bounds = RECT::default();
            GetWindowRect(self.window, ptr::addr_of_mut!(bounds))
                .map_err(|error| win_error("read Student presentation bounds", error))?;
            if bounds.left == 0
                && bounds.top == 0
                && bounds.right == width
                && bounds.bottom == height
                && GetWindowLongW(self.window, GWL_EXSTYLE)
                    & i32::try_from(WS_EX_TOPMOST.0).unwrap_or(0)
                    != 0
                && IsWindowVisible(self.window).as_bool()
            {
                return Ok(());
            }
            SetWindowPos(
                self.window,
                Some(HWND_TOPMOST),
                0,
                0,
                width,
                height,
                SWP_SHOWWINDOW,
            )
            .map_err(|error| win_error("maintain Student presentation window", error))
        }
    }

    fn pump_messages(&mut self) {
        // SAFETY: the message loop runs on the thread that owns the renderer window.
        unsafe {
            let mut message = MSG::default();
            while PeekMessageW(ptr::addr_of_mut!(message), None, 0, 0, PM_REMOVE).as_bool() {
                if message.message == WM_QUIT {
                    self.open = false;
                    break;
                }
                let _ = TranslateMessage(ptr::addr_of!(message));
                DispatchMessageW(ptr::addr_of!(message));
            }
        }
    }
}

impl Renderer for NativeRenderer {
    fn set_pointer(&mut self, pointer: Option<media_core::PointerOverlay>) {
        if let Some(gpu) = self.gpu.as_mut() {
            gpu.set_pointer(pointer.clone());
        }
        self.pointer = pointer;
    }
    fn redraw(&mut self) -> Result<bool, MediaError> {
        self.pump_messages();
        if !self.open {
            return Ok(false);
        }
        let Some(gpu) = self.gpu.as_mut() else {
            return Ok(false); // GDI fallback draws the pointer on the next frame
        };
        let mut client = RECT::default();
        // SAFETY: valid window handle owned by this thread.
        unsafe { GetClientRect(self.window, ptr::addr_of_mut!(client)) }
            .map_err(|error| win_error("read preview client rectangle", error))?;
        let (w, h) = (
            u32::try_from((client.right - client.left).max(0)).unwrap_or(0),
            u32::try_from((client.bottom - client.top).max(0)).unwrap_or(0),
        );
        gpu.redraw(w, h)
            .map_err(|error| MediaError::Operation(format!("pointer redraw failed: {error}")))
    }
    fn take_diagnostics(&mut self) -> media_core::diagnostics::StageMetrics {
        if let Some(gpu) = &mut self.gpu {
            gpu.take_diagnostics()
        } else {
            std::mem::take(&mut self.diagnostics)
        }
    }
    #[allow(clippy::too_many_lines)] // Validation and the existing GDI fallback remain together.
    fn render(&mut self, frame: DecodedFrameRef<'_>) -> Result<(), MediaError> {
        self.pump_messages();
        if !self.open {
            return Ok(());
        }
        if frame.width == 0 || frame.height == 0 {
            return Err(MediaError::Operation(
                "render dimensions must be non-zero".to_owned(),
            ));
        }
        frame.validate()?;
        // SAFETY: frame bytes remain valid for StretchDIBits and the DC is released exactly once.
        unsafe {
            let mut client = RECT::default();
            GetClientRect(self.window, ptr::addr_of_mut!(client))
                .map_err(|error| win_error("read preview client rectangle", error))?;
            let client_width = (client.right - client.left).max(0);
            let client_height = (client.bottom - client.top).max(0);
            if client_width == 0 || client_height == 0 {
                return Ok(());
            }
            if let Some(gpu) = self.gpu.as_mut() {
                match gpu.render(
                    frame,
                    u32::try_from(client_width).unwrap_or(0),
                    u32::try_from(client_height).unwrap_or(0),
                ) {
                    Ok(()) => return Ok(()),
                    Err(error) => {
                        tracing::warn!(%error, renderer_backend = "gdi", "D3D11 presentation failed; switching to filtered GDI fallback");
                        self.gpu = None;
                    }
                }
            }
            let conversion_started = Instant::now();
            let mut bgra = frame.bgra()?;
            if let Some(pointer) = self.pointer.as_ref() {
                pointer.blend_into_bgra(bgra.to_mut(), frame.width, frame.height);
            }
            self.diagnostics
                .record("render_color_conversion_cpu", conversion_started.elapsed());
            let frame = DecodedFrameRef {
                format: media_core::DecodedPixelFormat::Bgra,
                bytes: &bgra,
                ..frame
            };
            let dc = GetDC(Some(self.window));
            if dc.0.is_null() {
                return Err(MediaError::Operation(
                    "failed to acquire preview device context".to_owned(),
                ));
            }
            let bitmap = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: u32::try_from(size_of::<BITMAPINFOHEADER>()).unwrap_or_default(),
                    biWidth: i32::try_from(frame.width).unwrap_or(i32::MAX),
                    biHeight: -i32::try_from(frame.height).unwrap_or(i32::MAX),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let client_width = (client.right - client.left).max(0);
            let client_height = (client.bottom - client.top).max(0);
            if client_width == 0 || client_height == 0 {
                let _ = ReleaseDC(Some(self.window), dc);
                return Ok(());
            }
            let (fit_width, fit_height) =
                crate::fit::fit_rect(frame.width, frame.height, client_width, client_height);
            let left = (client_width - fit_width) / 2;
            let top = (client_height - fit_height) / 2;
            // A common window DC resets to its defaults after release. Select filtered
            // color scaling for every draw; the default monochrome AND mode damages text.
            if let Err(error) = configure_color_scaling(dc) {
                let _ = ReleaseDC(Some(self.window), dc);
                return Err(error);
            }
            // Paint only the bars; clearing the video area first produces visible flicker.
            let _ = PatBlt(dc, 0, 0, client_width, top, BLACKNESS);
            let _ = PatBlt(
                dc,
                0,
                top + fit_height,
                client_width,
                client_height - top - fit_height,
                BLACKNESS,
            );
            let _ = PatBlt(dc, 0, top, left, fit_height, BLACKNESS);
            let _ = PatBlt(
                dc,
                left + fit_width,
                top,
                client_width - left - fit_width,
                fit_height,
                BLACKNESS,
            );
            let result = StretchDIBits(
                dc,
                left,
                top,
                fit_width,
                fit_height,
                0,
                0,
                i32::try_from(frame.width).unwrap_or(i32::MAX),
                i32::try_from(frame.height).unwrap_or(i32::MAX),
                Some(frame.bytes.as_ptr().cast()),
                ptr::addr_of!(bitmap),
                DIB_RGB_COLORS,
                SRCCOPY,
            );
            let _ = ReleaseDC(Some(self.window), dc);
            if result == 0 {
                return Err(MediaError::Operation(
                    "StretchDIBits failed to present the decoded frame".to_owned(),
                ));
            }
        }
        Ok(())
    }
}

/// The caller must supply a valid, exclusively used device context.
unsafe fn configure_color_scaling(dc: HDC) -> Result<(), MediaError> {
    // SAFETY: caller retains the DC. HALFTONE requires resetting the brush origin.
    if unsafe { SetStretchBltMode(dc, HALFTONE) } == 0
        || !unsafe { SetBrushOrgEx(dc, 0, 0, None) }.as_bool()
    {
        return Err(MediaError::Operation(
            "could not configure filtered color rendering".to_owned(),
        ));
    }
    Ok(())
}

impl Drop for NativeRenderer {
    fn drop(&mut self) {
        self.gpu = None;
        if self.timer_period_active {
            // SAFETY: balances this renderer's successful timeBeginPeriod call.
            let _ = unsafe { timeEndPeriod(1) };
        }
        if self.open {
            // SAFETY: this renderer owns the window and drops on the window thread.
            let _ = unsafe { DestroyWindow(self.window) };
        }
        // A reconnect reuses the window thread. Consume this window's WM_QUIT
        // so it cannot immediately close the next presentation surface.
        // SAFETY: only removes quit messages from this thread's own queue.
        unsafe {
            let mut message = MSG::default();
            while PeekMessageW(
                ptr::addr_of_mut!(message),
                None,
                WM_QUIT,
                WM_QUIT,
                PM_REMOVE,
            )
            .as_bool()
            {}
        }
    }
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_CLOSE => {
            // SAFETY: Windows supplied a valid window handle for this callback.
            let _ = unsafe { DestroyWindow(window) };
            LRESULT(0)
        }
        WM_DESTROY => {
            // SAFETY: posts WM_QUIT to this window thread's message queue.
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        // SAFETY: all unhandled messages are forwarded to the default procedure.
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

unsafe extern "system" fn presentation_window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        // The Student presentation ends with the Teacher/control session, not via the
        // ordinary window-close command.
        WM_CLOSE => LRESULT(0),
        WM_DESTROY => {
            // SAFETY: posts WM_QUIT to this window thread's message queue.
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        // SAFETY: all unhandled messages are forwarded to the default procedure.
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

#[allow(clippy::needless_pass_by_value)]
fn win_error(context: &str, error: WindowsError) -> MediaError {
    MediaError::Operation(format!("{context}: {error}"))
}
