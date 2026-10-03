#[cfg(any(windows, test))]
mod fit;

#[cfg(windows)]
#[allow(unsafe_code)]
mod windows_impl;

#[cfg(windows)]
#[allow(unsafe_code)]
mod gpu;

#[cfg(windows)]
#[allow(unsafe_code)]
mod gpu_timing;

#[cfg(windows)]
pub use windows_impl::NativeRenderer;

#[cfg(not(windows))]
mod unsupported {
    use media_core::{DecodedFrameRef, MediaError, Renderer};

    #[derive(Debug)]
    pub struct NativeRenderer;

    impl NativeRenderer {
        /// # Errors
        /// Always returns an unavailable-backend error outside Windows.
        pub fn new(_width: u32, _height: u32, _title: &str) -> Result<Self, MediaError> {
            Err(MediaError::BackendUnavailable(
                "the native Windows renderer is available only on Windows".to_owned(),
            ))
        }

        /// # Errors
        /// Always returns an unavailable-backend error outside Windows.
        pub fn new_student_presentation(
            _width: u32,
            _height: u32,
            _title: &str,
        ) -> Result<Self, MediaError> {
            Err(MediaError::BackendUnavailable(
                "the native Windows renderer is available only on Windows".to_owned(),
            ))
        }

        #[must_use]
        pub fn is_open(&mut self) -> bool {
            false
        }
    }

    impl Renderer for NativeRenderer {
        fn render(&mut self, _frame: DecodedFrameRef<'_>) -> Result<(), MediaError> {
            Err(MediaError::BackendUnavailable(
                "the native Windows renderer is available only on Windows".to_owned(),
            ))
        }
    }
}

#[cfg(not(windows))]
pub use unsupported::NativeRenderer;
