#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod macos_impl;

#[cfg(target_os = "macos")]
pub use macos_impl::NativeRenderer;

#[cfg(not(target_os = "macos"))]
mod unsupported {
    use media_core::{DecodedFrameRef, MediaError, Renderer};

    #[derive(Debug)]
    pub struct NativeRenderer;

    impl NativeRenderer {
        /// # Errors
        /// Always returns an unavailable-backend error outside macOS.
        pub fn new(_width: u32, _height: u32, _title: &str) -> Result<Self, MediaError> {
            Err(unsupported())
        }

        /// # Errors
        /// Always returns an unavailable-backend error outside macOS.
        pub fn new_student_presentation(
            _width: u32,
            _height: u32,
            _title: &str,
        ) -> Result<Self, MediaError> {
            Err(unsupported())
        }

        #[must_use]
        pub const fn is_open(&self) -> bool {
            false
        }
    }

    impl Renderer for NativeRenderer {
        fn render(&mut self, _frame: DecodedFrameRef<'_>) -> Result<(), MediaError> {
            Err(unsupported())
        }
    }

    fn unsupported() -> MediaError {
        MediaError::BackendUnavailable("macOS renderer is available only on macOS".to_owned())
    }
}

#[cfg(not(target_os = "macos"))]
pub use unsupported::NativeRenderer;
