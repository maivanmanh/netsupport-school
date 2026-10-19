//! Native Ubuntu Student presentation through X11 / `XWayland`.
//!
//! The UI thread owns the window; this backend retains only the last presented
//! frame. Native Wayland and operating-system input restrictions are out of scope.

#[cfg(any(target_os = "linux", test))]
mod surface;

#[cfg(target_os = "linux")]
mod inhibit;
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
mod linux_impl;

#[cfg(target_os = "linux")]
pub use linux_impl::{screen_size, FullscreenRaiser, NativeRenderer};

#[cfg(not(target_os = "linux"))]
mod unsupported {
    use media_core::{DecodedFrameRef, MediaError, Renderer};

    #[derive(Debug)]
    pub struct NativeRenderer;

    impl NativeRenderer {
        /// # Errors
        /// Always returns an unavailable-backend error outside Linux.
        pub fn new(_width: u32, _height: u32, _title: &str) -> Result<Self, MediaError> {
            Err(unavailable())
        }

        /// # Errors
        /// Always returns an unavailable-backend error outside Linux.
        pub fn new_student_presentation(
            width: u32,
            height: u32,
            title: &str,
        ) -> Result<Self, MediaError> {
            Self::new(width, height, title)
        }

        #[must_use]
        pub const fn is_open(&mut self) -> bool {
            false
        }
    }

    impl Renderer for NativeRenderer {
        fn render(&mut self, _frame: DecodedFrameRef<'_>) -> Result<(), MediaError> {
            Err(unavailable())
        }
    }

    fn unavailable() -> MediaError {
        MediaError::BackendUnavailable("Linux renderer requires Linux with X11/XWayland".into())
    }
}

#[cfg(not(target_os = "linux"))]
pub use unsupported::NativeRenderer;
