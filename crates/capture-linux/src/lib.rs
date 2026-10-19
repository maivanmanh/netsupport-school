//! Ubuntu screen capture for Student thumbnails, screenshots and sharing.
//!
//! GNOME on Wayland exposes no direct framebuffer. Mutter's ScreenCast D-Bus
//! service hands the logged-in user a PipeWire stream of a monitor without a
//! portal prompt. The capture thread owns the D-Bus session and the PipeWire
//! loop; the media loop only reads a one-slot "latest frame wins" buffer.

#[cfg(target_os = "linux")]
mod mutter;
#[cfg(target_os = "linux")]
mod scale;
#[cfg(target_os = "linux")]
mod linux_impl;
#[cfg(target_os = "linux")]
mod input;

#[cfg(target_os = "linux")]
pub use input::{keysym_for, RemoteInput};
#[cfg(target_os = "linux")]
pub use linux_impl::LinuxCapture;

#[cfg(not(target_os = "linux"))]
mod unsupported {
    use media_core::{
        CaptureConfig, CaptureSource, CaptureStatistics, MediaError, VideoFrame,
    };
    use std::time::Instant;

    #[derive(Default)]
    pub struct LinuxCapture;
    impl LinuxCapture {
        #[must_use]
        pub fn new() -> Self {
            Self
        }
    }
    impl CaptureSource for LinuxCapture {
        fn start(&mut self, _: CaptureConfig) -> Result<(), MediaError> {
            Err(MediaError::BackendUnavailable("Linux capture is only available on Linux".into()))
        }
        fn output_dimensions(&self) -> Option<(u32, u32)> {
            None
        }
        fn next_frame(&mut self, _: Instant) -> Result<Option<VideoFrame>, MediaError> {
            Ok(None)
        }
        fn statistics(&self) -> CaptureStatistics {
            CaptureStatistics::default()
        }
    }
}
#[cfg(not(target_os = "linux"))]
pub use unsupported::LinuxCapture;
