#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod macos_impl;

#[cfg(target_os = "macos")]
pub use macos_impl::{control_display_bounds, control_display_id, ScreenCaptureKitCapture};

#[cfg(not(target_os = "macos"))]
mod unsupported {
    use std::time::Instant;

    use media_core::{CaptureConfig, CaptureSource, CaptureStatistics, MediaError, VideoFrame};

    #[derive(Debug, Default)]
    pub struct ScreenCaptureKitCapture;

    impl ScreenCaptureKitCapture {
        #[must_use]
        pub fn new() -> Self {
            Self
        }
    }

    impl CaptureSource for ScreenCaptureKitCapture {
        fn start(&mut self, _config: CaptureConfig) -> Result<(), MediaError> {
            Err(unsupported())
        }

        fn output_dimensions(&self) -> Option<(u32, u32)> {
            None
        }

        fn next_frame(&mut self, _deadline: Instant) -> Result<Option<VideoFrame>, MediaError> {
            Err(unsupported())
        }

        fn statistics(&self) -> CaptureStatistics {
            CaptureStatistics::default()
        }
    }

    fn unsupported() -> MediaError {
        MediaError::BackendUnavailable("ScreenCaptureKit is available only on macOS".to_owned())
    }
}

#[cfg(not(target_os = "macos"))]
pub use unsupported::ScreenCaptureKitCapture;
