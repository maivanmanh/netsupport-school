#[cfg_attr(not(any(windows, test)), allow(dead_code))]
#[cfg_attr(target_arch = "x86_64", allow(unsafe_code))]
mod color;
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
mod h264;

pub use media_core::{CodecBackendKind, CodecDescriptor, H264Config};

#[cfg(windows)]
#[allow(unsafe_code)]
mod windows_impl;

#[cfg(windows)]
pub use windows_impl::{MediaFoundationRuntime, MfH264Decoder, MfH264Encoder};

#[cfg(not(windows))]
mod unsupported {
    use media_core::{
        DecodedFrame, EncodedAccessUnit, EncodedAccessUnitRef, MediaError, VideoDecoder,
        VideoEncoder, VideoFrameRef,
    };

    use crate::{CodecBackendKind, CodecDescriptor, H264Config};

    #[derive(Debug, Default)]
    pub struct MediaFoundationRuntime;

    impl MediaFoundationRuntime {
        /// # Errors
        /// Always returns an unavailable-backend error outside Windows.
        pub fn new() -> Result<Self, MediaError> {
            Err(unsupported())
        }
    }

    #[derive(Debug)]
    pub struct MfH264Encoder;

    impl MfH264Encoder {
        /// # Errors
        /// Always returns an unavailable-backend error outside Windows.
        pub fn new(
            _runtime: &MediaFoundationRuntime,
            _config: H264Config,
        ) -> Result<Self, MediaError> {
            Err(unsupported())
        }

        #[must_use]
        pub fn descriptor(&self) -> CodecDescriptor {
            CodecDescriptor {
                name: "unavailable".to_owned(),
                kind: CodecBackendKind::Software,
                asynchronous: false,
            }
        }
    }

    impl VideoEncoder for MfH264Encoder {
        fn encode(
            &mut self,
            _frame: VideoFrameRef<'_>,
        ) -> Result<Option<EncodedAccessUnit>, MediaError> {
            Err(unsupported())
        }

        fn set_bitrate(&mut self, _bits_per_second: u32) -> Result<(), MediaError> {
            Err(unsupported())
        }

        fn request_keyframe(&mut self) -> Result<(), MediaError> {
            Err(unsupported())
        }
    }

    #[derive(Debug)]
    pub struct MfH264Decoder;

    impl MfH264Decoder {
        /// # Errors
        /// Always returns an unavailable-backend error outside Windows.
        pub fn new(
            _runtime: &MediaFoundationRuntime,
            _config: H264Config,
        ) -> Result<Self, MediaError> {
            Err(unsupported())
        }

        #[must_use]
        pub fn descriptor(&self) -> CodecDescriptor {
            CodecDescriptor {
                name: "unavailable".to_owned(),
                kind: CodecBackendKind::Software,
                asynchronous: false,
            }
        }
    }

    impl VideoDecoder for MfH264Decoder {
        fn decode(
            &mut self,
            _access_unit: EncodedAccessUnitRef<'_>,
        ) -> Result<Vec<DecodedFrame>, MediaError> {
            Err(unsupported())
        }
    }

    fn unsupported() -> MediaError {
        MediaError::BackendUnavailable(
            "Windows Media Foundation is available only on Windows".to_owned(),
        )
    }
}

#[cfg(not(windows))]
pub use unsupported::{MediaFoundationRuntime, MfH264Decoder, MfH264Encoder};
