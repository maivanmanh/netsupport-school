//! Portable software H.264 decode backend for Ubuntu Student.
//!
//! `OpenH264` is compiled into the executable: no codec download, external process,
//! or hardware decoder is required at runtime. Calls are synchronous and must
//! stay on the existing receiver worker, outside the async control runtime.

use std::time::Instant;

use media_core::{
    diagnostics::StageMetrics, CodecBackendKind, CodecDescriptor, DecodedFrame, DecodedPixelFormat,
    EncodedAccessUnitRef, H264Config, MediaError, VideoCodec, VideoDecoder,
};
use openh264::{
    decoder::{Decoder, DecoderConfig, Flush},
    formats::YUVSource,
    OpenH264API,
};

/// Bounds complete access units independently of network fragmentation.
const MAX_ACCESS_UNIT_BYTES: usize = 4 * 1024 * 1024;
const MAX_WIDTH: usize = 1920;
const MAX_HEIGHT: usize = 1080;

/// One decoder per receiving stream; no application frame queue is held here.
pub struct LinuxH264Decoder {
    decoder: Decoder,
    diagnostics: StageMetrics,
}

impl LinuxH264Decoder {
    /// Starts the H.264 software decoder. Actual dimensions come from the SPS.
    ///
    /// # Errors
    /// Returns an error for unsupported dimensions or codec initialization failure.
    pub fn new(config: H264Config) -> Result<Self, MediaError> {
        Self::with_codec(config, VideoCodec::H264)
    }

    /// # Errors
    /// Rejects H.265, invalid initial dimensions, or decoder initialization failure.
    pub fn with_codec(config: H264Config, codec: VideoCodec) -> Result<Self, MediaError> {
        if codec != VideoCodec::H264 {
            return Err(MediaError::BackendUnavailable(
                "Ubuntu Student software decoder supports H.264 only".to_owned(),
            ));
        }
        validate_dimensions(config.width as usize, config.height as usize)?;
        Ok(Self {
            decoder: create_decoder()?,
            diagnostics: StageMetrics::default(),
        })
    }

    #[must_use]
    pub fn descriptor(&self) -> CodecDescriptor {
        CodecDescriptor {
            name: "OpenH264 software H.264 (Ubuntu)".to_owned(),
            kind: CodecBackendKind::Software,
            asynchronous: false,
        }
    }

    /// Drops damaged reference state. The receiver must request SPS/PPS + IDR.
    ///
    /// # Errors
    /// Returns an error if recreating the decoder fails.
    pub fn reset(&mut self) -> Result<(), MediaError> {
        self.decoder = create_decoder()?;
        Ok(())
    }
}

fn create_decoder() -> Result<Decoder, MediaError> {
    // The wire profile has no B-frames. Flush after every complete access unit
    // to return its picture immediately instead of waiting for another packet.
    // Keep OpenH264's default thread count; the binding warns that overriding
    // num_threads can crash. Error concealment is disabled so corrupt reference
    // pictures go through the existing receiver keyframe-recovery mechanism.
    Decoder::with_api_config(
        OpenH264API::from_source(),
        DecoderConfig::new().flush_after_decode(Flush::Flush),
    )
    .map_err(|error| MediaError::BackendUnavailable(format!("OpenH264 initialization: {error}")))
}

fn validate_dimensions(width: usize, height: usize) -> Result<(), MediaError> {
    if width == 0 || height == 0 || width > MAX_WIDTH || height > MAX_HEIGHT {
        return Err(MediaError::Operation(format!(
            "Ubuntu decoded dimensions {width}x{height} exceed the 1920x1080 media bounds"
        )));
    }
    Ok(())
}

impl VideoDecoder for LinuxH264Decoder {
    fn take_diagnostics(&mut self) -> StageMetrics {
        std::mem::take(&mut self.diagnostics)
    }

    fn decode(
        &mut self,
        access_unit: EncodedAccessUnitRef<'_>,
    ) -> Result<Vec<DecodedFrame>, MediaError> {
        if access_unit.bytes.is_empty() || access_unit.bytes.len() > MAX_ACCESS_UNIT_BYTES {
            return Err(MediaError::Operation(
                "OpenH264 access unit is empty or exceeds 4 MiB".to_owned(),
            ));
        }
        // RTP depacketization supplies Annex B; do not silently feed AVCC or raw
        // payloads into a decoder expecting start codes.
        if !access_unit.bytes.starts_with(&[0, 0, 1])
            && !access_unit.bytes.starts_with(&[0, 0, 0, 1])
        {
            return Err(MediaError::Operation(
                "OpenH264 requires Annex B access units".to_owned(),
            ));
        }
        let started = Instant::now();
        let result = self.decoder.decode(access_unit.bytes);
        self.diagnostics
            .record("openh264_decode", started.elapsed());
        let Some(picture) =
            result.map_err(|error| MediaError::Operation(format!("OpenH264 decode: {error}")))?
        else {
            return Ok(Vec::new());
        };
        let started = Instant::now();
        let (width, height) = picture.dimensions();
        // Check before allocating/converting the application frame. OpenH264
        // itself owns SPS parsing and its codec reference-picture allocation.
        validate_dimensions(width, height)?;
        let mut bytes = vec![0; width * height * 4];
        picture.write_rgba8(&mut bytes);
        for pixel in bytes.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        self.diagnostics
            .record("openh264_copy_convert_bgra", started.elapsed());
        Ok(vec![DecodedFrame {
            media_timestamp: Some(access_unit.timestamp),
            timing: None,
            format: DecodedPixelFormat::Bgra,
            bytes,
            // Validated bounds make these conversions exact.
            width: u32::try_from(width).unwrap_or_default(),
            height: u32::try_from(height).unwrap_or_default(),
        }])
    }
}
