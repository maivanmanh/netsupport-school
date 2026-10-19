//! Portable software H.264 decode backend for Ubuntu Student.
//!
//! `OpenH264` is compiled into the executable: no codec download, external process,
//! or hardware decoder is required at runtime. Calls are synchronous and must
//! stay on the existing receiver worker, outside the async control runtime.

use std::time::Instant;

use media_core::{
    diagnostics::StageMetrics, CodecBackendKind, CodecDescriptor, DecodedFrame, DecodedPixelFormat,
    EncodedAccessUnit, EncodedAccessUnitRef, H264Config, MediaError, VideoCodec, VideoDecoder,
    VideoEncoder, VideoFrameRef,
};
use openh264::{
    decoder::{Decoder, DecoderConfig, Flush},
    encoder::{
        BitRate, Encoder, EncoderConfig, FrameRate, FrameType, IntraFramePeriod, RateControlMode,
        UsageType, VuiConfig,
    },
    formats::{BgraSliceU8, YUVBuffer, YUVSource},
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

/// Software H.264 encoder for Ubuntu Student sharing/Observe. No B-frames
/// (`OpenH264` real-time profile), SPS/PPS prepended to every IDR, BT.601 limited
/// range tagged in the SPS VUI so receivers convert colours consistently.
pub struct LinuxH264Encoder {
    encoder: Encoder,
    config: H264Config,
    yuv: YUVBuffer,
    force_idr: bool,
    epoch: Instant,
    diagnostics: StageMetrics,
}

fn encoder_config(config: &H264Config) -> EncoderConfig {
    EncoderConfig::new()
        .bitrate(BitRate::from_bps(config.bitrate.max(100_000)))
        .max_frame_rate(FrameRate::from_hz(config.frames_per_second.max(1) as f32))
        .usage_type(UsageType::ScreenContentRealTime)
        .rate_control_mode(RateControlMode::Bitrate)
        .intra_frame_period(IntraFramePeriod::from_num_frames(config.keyframe_interval_frames))
        .skip_frames(false)
        .adaptive_quantization(false)
        .background_detection(false)
        .vui(VuiConfig::bt601())
}

fn create_encoder(config: &H264Config) -> Result<Encoder, MediaError> {
    Encoder::with_api_config(OpenH264API::from_source(), encoder_config(config)).map_err(|error| {
        MediaError::BackendUnavailable(format!("OpenH264 encoder initialization: {error}"))
    })
}

impl LinuxH264Encoder {
    /// # Errors
    /// Rejects odd/oversized dimensions or encoder initialization failure.
    pub fn new(config: H264Config) -> Result<Self, MediaError> {
        Self::with_codec(config, VideoCodec::H264)
    }

    /// # Errors
    /// Rejects H.265, odd/oversized dimensions, or encoder initialization failure.
    pub fn with_codec(config: H264Config, codec: VideoCodec) -> Result<Self, MediaError> {
        if codec != VideoCodec::H264 {
            return Err(MediaError::BackendUnavailable(
                "Ubuntu Student software encoder supports H.264 only".to_owned(),
            ));
        }
        let (width, height) = (config.width as usize, config.height as usize);
        validate_dimensions(width, height)?;
        if width % 2 != 0 || height % 2 != 0 {
            return Err(MediaError::Operation("H.264 dimensions must be even".to_owned()));
        }
        Ok(Self {
            encoder: create_encoder(&config)?,
            yuv: YUVBuffer::new(width, height),
            config,
            force_idr: true,
            epoch: Instant::now(),
            diagnostics: StageMetrics::default(),
        })
    }

    #[must_use]
    pub fn descriptor(&self) -> CodecDescriptor {
        CodecDescriptor {
            name: "OpenH264 software H.264 encoder (Ubuntu)".to_owned(),
            kind: CodecBackendKind::Software,
            asynchronous: false,
        }
    }
}

impl VideoEncoder for LinuxH264Encoder {
    fn take_diagnostics(&mut self) -> StageMetrics {
        std::mem::take(&mut self.diagnostics)
    }

    fn encode(
        &mut self,
        frame: VideoFrameRef<'_>,
    ) -> Result<Option<EncodedAccessUnit>, MediaError> {
        let (width, height) = (self.config.width as usize, self.config.height as usize);
        if frame.width as usize != width
            || frame.height as usize != height
            || frame.bytes.len() < width * height * 4
        {
            return Err(MediaError::Operation(
                "frame does not match the configured encoder size".to_owned(),
            ));
        }
        let started = Instant::now();
        self.yuv
            .read_bgra8(BgraSliceU8::new(&frame.bytes[..width * height * 4], (width, height)));
        self.diagnostics.record("openh264_bgra_to_yuv", started.elapsed());
        if std::mem::take(&mut self.force_idr) {
            self.encoder.force_intra_frame();
        }
        let started = Instant::now();
        let bitstream = self
            .encoder
            .encode(&self.yuv)
            .map_err(|error| MediaError::Operation(format!("OpenH264 encode: {error}")))?;
        self.diagnostics.record("openh264_encode", started.elapsed());
        let frame_type = bitstream.frame_type();
        if matches!(frame_type, FrameType::Skip | FrameType::Invalid) {
            return Ok(None);
        }
        Ok(Some(EncodedAccessUnit {
            bytes: bitstream.to_vec(),
            timestamp: frame.captured_at.saturating_duration_since(self.epoch),
            is_keyframe: matches!(frame_type, FrameType::IDR | FrameType::I),
        }))
    }

    fn set_bitrate(&mut self, bits_per_second: u32) -> Result<(), MediaError> {
        if bits_per_second == self.config.bitrate {
            return Ok(());
        }
        // Rate-control parameters of a running OpenH264 encoder are raw FFI
        // options; recreating is safe and bitrate steps are seconds apart.
        self.config.bitrate = bits_per_second;
        self.encoder = create_encoder(&self.config)?;
        self.force_idr = true;
        Ok(())
    }

    fn request_keyframe(&mut self) -> Result<(), MediaError> {
        self.force_idr = true;
        Ok(())
    }
}
