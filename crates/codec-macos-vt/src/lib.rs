#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod macos_impl;

#[cfg(target_os = "macos")]
pub use macos_impl::{
    hardware_decode_supported, hardware_encode_supported, VideoToolboxH264Decoder,
    VideoToolboxH264Encoder,
};

#[cfg(not(target_os = "macos"))]
mod unsupported {
    use media_core::{
        CodecDescriptor, DecodedFrame, EncodedAccessUnit, EncodedAccessUnitRef, H264Config,
        MediaError, VideoDecoder, VideoEncoder, VideoFrameRef,
    };

    #[derive(Debug)]
    pub struct VideoToolboxH264Encoder;

    impl VideoToolboxH264Encoder {
        /// # Errors
        /// Always returns an unavailable-backend error outside macOS.
        pub fn new(_config: H264Config) -> Result<Self, MediaError> {
            Err(unsupported())
        }

        #[must_use]
        pub fn descriptor(&self) -> CodecDescriptor {
            CodecDescriptor::unavailable()
        }
    }

    impl VideoEncoder for VideoToolboxH264Encoder {
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
    pub struct VideoToolboxH264Decoder;

    impl VideoToolboxH264Decoder {
        /// # Errors
        /// Always returns an unavailable-backend error outside macOS.
        pub fn new(_config: H264Config) -> Result<Self, MediaError> {
            Err(unsupported())
        }

        #[must_use]
        pub fn descriptor(&self) -> CodecDescriptor {
            CodecDescriptor::unavailable()
        }
    }

    impl VideoDecoder for VideoToolboxH264Decoder {
        fn decode(
            &mut self,
            _access_unit: EncodedAccessUnitRef<'_>,
        ) -> Result<Vec<DecodedFrame>, MediaError> {
            Err(unsupported())
        }
    }

    #[must_use]
    pub fn hardware_encode_supported(_codec: media_core::VideoCodec) -> bool {
        false
    }

    #[must_use]
    pub fn hardware_decode_supported(_codec: media_core::VideoCodec) -> bool {
        false
    }

    fn unsupported() -> MediaError {
        MediaError::BackendUnavailable("VideoToolbox is available only on macOS".to_owned())
    }
}

#[cfg(not(target_os = "macos"))]
pub use unsupported::{
    hardware_decode_supported, hardware_encode_supported, VideoToolboxH264Decoder,
    VideoToolboxH264Encoder,
};
