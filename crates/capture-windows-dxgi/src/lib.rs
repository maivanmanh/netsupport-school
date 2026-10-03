#[cfg_attr(not(any(windows, test)), allow(dead_code))]
mod cursor;
#[cfg(any(windows, test))]
mod scale;

#[cfg(windows)]
#[allow(unsafe_code)]
mod windows_impl;

#[cfg(windows)]
pub use windows_impl::{control_display_bounds, control_display_monitor, DxgiCapture};

#[cfg(not(windows))]
mod unsupported {
    use std::time::Instant;

    use media_core::{CaptureConfig, CaptureSource, CaptureStatistics, MediaError, VideoFrame};

    #[derive(Debug, Default)]
    pub struct DxgiCapture;

    impl DxgiCapture {
        #[must_use]
        pub fn new() -> Self {
            Self
        }
    }

    impl CaptureSource for DxgiCapture {
        fn start(&mut self, _config: CaptureConfig) -> Result<(), MediaError> {
            Err(MediaError::BackendUnavailable(
                "DXGI Desktop Duplication is available only on Windows".to_owned(),
            ))
        }

        fn output_dimensions(&self) -> Option<(u32, u32)> {
            None
        }

        fn next_frame(&mut self, _deadline: Instant) -> Result<Option<VideoFrame>, MediaError> {
            Err(MediaError::BackendUnavailable(
                "DXGI Desktop Duplication is available only on Windows".to_owned(),
            ))
        }

        fn statistics(&self) -> CaptureStatistics {
            CaptureStatistics::default()
        }
    }
}

#[cfg(not(windows))]
pub use unsupported::DxgiCapture;
