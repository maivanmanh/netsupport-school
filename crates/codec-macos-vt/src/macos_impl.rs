use std::{
    ffi::c_void,
    fmt::Write as _,
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    time::{Duration, Instant},
};

use apple_cf::{
    cf::{CFDictionary, CFNumber, CFString, CFType},
    cm::{CMBlockBuffer, CMFormatDescription, CMSampleBuffer, CMSampleTimingInfo, CMTime},
    cv::{CVPixelBuffer, CVPixelBufferLockFlags},
    iosurface::IOSurface,
    raw::{CFBooleanGetTypeID, CFBooleanGetValue},
};
use media_core::{
    CodecBackendKind, CodecDescriptor, DecodedFrame as CoreDecodedFrame, EncodedAccessUnit,
    EncodedAccessUnitRef, H264Config, MediaError, VideoCodec, VideoDecoder, VideoEncoder,
    VideoFrameRef,
};
use tracing::{info, warn};
use videotoolbox::{ffi, Codec, CompressionSession, DecompressionSession, ProfileLevel};

const BGRA: u32 = u32::from_be_bytes(*b"BGRA");
const NV12_VIDEO_RANGE: u32 = u32::from_be_bytes(*b"420v");
const NV12_FULL_RANGE: u32 = u32::from_be_bytes(*b"420f");

extern "C" {
    fn CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
        video_format_description: *mut c_void,
        parameter_set_index: usize,
        parameter_set_pointer_out: *mut *const u8,
        parameter_set_size_out: *mut usize,
        parameter_set_count_out: *mut usize,
        nal_unit_header_length_out: *mut i32,
    ) -> i32;
    fn CMVideoFormatDescriptionCreateFromH264ParameterSets(
        allocator: *const c_void,
        parameter_set_count: usize,
        parameter_set_pointers: *const *const u8,
        parameter_set_sizes: *const usize,
        nal_unit_header_length: i32,
        format_description_out: *mut *mut c_void,
    ) -> i32;
    fn CMVideoFormatDescriptionGetHEVCParameterSetAtIndex(
        video_format_description: *mut c_void,
        parameter_set_index: usize,
        parameter_set_pointer_out: *mut *const u8,
        parameter_set_size_out: *mut usize,
        parameter_set_count_out: *mut usize,
        nal_unit_header_length_out: *mut i32,
    ) -> i32;
    fn CMVideoFormatDescriptionCreateFromHEVCParameterSets(
        allocator: *const c_void,
        parameter_set_count: usize,
        parameter_set_pointers: *const *const u8,
        parameter_set_sizes: *const usize,
        nal_unit_header_length: i32,
        extensions: *const c_void,
        format_description_out: *mut *mut c_void,
    ) -> i32;
    fn CMSampleBufferCreateReady(
        allocator: *const c_void,
        data_buffer: *mut c_void,
        format_description: *mut c_void,
        num_samples: i64,
        num_sample_timing_entries: i64,
        sample_timing_array: *const CMSampleTimingInfo,
        num_sample_size_entries: i64,
        sample_size_array: *const usize,
        sample_buffer_out: *mut *mut c_void,
    ) -> i32;
    static kCFAllocatorDefault: *const c_void;
}

/// True when this Mac has a hardware encoder for `codec` (cached).
#[must_use]
pub fn hardware_encode_supported(codec: VideoCodec) -> bool {
    use std::sync::OnceLock;
    static H264: OnceLock<bool> = OnceLock::new();
    static H265: OnceLock<bool> = OnceLock::new();
    let cell = match codec {
        VideoCodec::H264 => &H264,
        VideoCodec::H265 => &H265,
    };
    *cell.get_or_init(|| {
        let wanted = vt_codec(codec).as_cm_codec_type();
        videotoolbox::encoder_list::available_video_encoder_details().is_ok_and(|list| {
            list.iter().any(|encoder| {
                encoder.base.codec_type == wanted && encoder.is_hardware_accelerated == Some(true)
            })
        })
    })
}
/// True when this Mac advertises hardware decode for `codec`.
#[must_use]
pub fn hardware_decode_supported(codec: VideoCodec) -> bool {
    DecompressionSession::is_hardware_decode_supported(vt_codec(codec))
}
const fn vt_codec(codec: VideoCodec) -> Codec {
    match codec {
        VideoCodec::H264 => Codec::H264,
        VideoCodec::H265 => Codec::HEVC,
    }
}

pub struct VideoToolboxH264Encoder {
    config: H264Config,
    codec: VideoCodec,
    session: CompressionSession,
    surface: IOSurface,
    frame_index: i64,
    rebuild_before_next_frame: bool,
    /// Parameter sets in stream order (H.264 SPS, PPS; H.265 VPS, SPS, PPS).
    parameters: Vec<Vec<u8>>,
    nal_length_bytes: usize,
    source_timeline: media_core::source_timing::SourceTimeline,
    last_bitrate_update_live: bool,
    diagnostics: media_core::diagnostics::StageMetrics,
}

impl std::fmt::Debug for VideoToolboxH264Encoder {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VideoToolboxH264Encoder")
            .field("config", &self.config)
            .field("frame_index", &self.frame_index)
            .finish_non_exhaustive()
    }
}

impl VideoToolboxH264Encoder {
    /// Creates a real-time H.264 encoder backed by `VideoToolbox`.
    ///
    /// # Errors
    /// Returns an error when the configuration is invalid or `VideoToolbox` cannot initialize.
    pub fn new(config: H264Config) -> Result<Self, MediaError> {
        Self::with_codec(config, VideoCodec::H264)
    }

    /// Creates a real-time encoder for `codec` (H.264 or H.265).
    ///
    /// # Errors
    /// Returns an error when the configuration is invalid or `VideoToolbox` cannot initialize.
    pub fn with_codec(config: H264Config, codec: VideoCodec) -> Result<Self, MediaError> {
        validate_config(config)?;
        let session = build_encoder(config, codec)?;
        let surface = IOSurface::create(config.width as usize, config.height as usize, BGRA, 4)
            .ok_or_else(|| MediaError::Operation("failed to allocate BGRA IOSurface".to_owned()))?;
        Ok(Self {
            config,
            codec,
            session,
            surface,
            frame_index: 0,
            rebuild_before_next_frame: false,
            parameters: Vec::new(),
            nal_length_bytes: 4,
            source_timeline: media_core::source_timing::SourceTimeline::default(),
            last_bitrate_update_live: false,
            diagnostics: media_core::diagnostics::StageMetrics::default(),
        })
    }

    #[must_use]
    pub fn descriptor(&self) -> CodecDescriptor {
        CodecDescriptor {
            name: format!("Apple VideoToolbox {} encoder", self.codec),
            kind: encoder_backend(&self.session),
            asynchronous: false,
        }
    }

    #[must_use]
    pub const fn codec(&self) -> VideoCodec {
        self.codec
    }

    fn rebuild(&mut self) -> Result<(), MediaError> {
        self.session = build_encoder(self.config, self.codec)?;
        self.rebuild_before_next_frame = false;
        self.parameters.clear();
        self.source_timeline = media_core::source_timing::SourceTimeline::default();
        Ok(())
    }

    fn copy_input_to_surface(
        &mut self,
        bytes: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(), MediaError> {
        let started = Instant::now();
        let mut guard = self.surface.lock_read_write().map_err(|status| {
            MediaError::Operation(format!("failed to lock encoder IOSurface: {status}"))
        })?;
        let surface_pitch = guard.bytes_per_row();
        let packed_pitch = width as usize * 4;
        let Some(target) = (unsafe { guard.as_slice_mut() }) else {
            return Err(MediaError::Operation(
                "encoder IOSurface did not expose writable BGRA bytes".to_owned(),
            ));
        };
        for row in 0..height as usize {
            let source_start = row * packed_pitch;
            let target_start = row * surface_pitch;
            target[target_start..target_start + packed_pitch]
                .copy_from_slice(&bytes[source_start..source_start + packed_pitch]);
        }
        drop(guard);
        self.diagnostics
            .record("vt_input_surface_copy", started.elapsed());
        Ok(())
    }

    fn normalize_output(
        &mut self,
        encoded: &videotoolbox::compression::EncodedFrame,
    ) -> Result<EncodedAccessUnit, MediaError> {
        let sample = encoded.cm_sample_buffer().ok_or_else(|| {
            MediaError::Operation("VideoToolbox output sample is unexpectedly missing".to_owned())
        })?;
        let format = sample.format_description().ok_or_else(|| {
            MediaError::Operation("VideoToolbox sample has no format description".to_owned())
        })?;
        let (parameters, nal_length_bytes) = parameter_sets(&format, self.codec)?;
        if self.parameters != parameters {
            // Parameter sets describe codec configuration, never screen pixels.
            let mut hex = String::new();
            for byte in parameters.iter().flatten().take(64) {
                let _ = write!(hex, "{byte:02x}");
            }
            info!(codec = %self.codec, parameter_sets = parameters.len(), parameters_hex = hex, "VideoToolbox sequence configuration");
        }
        self.parameters = parameters;
        self.nal_length_bytes = nal_length_bytes;
        let (mut bytes, is_keyframe) =
            avcc_to_annex_b(&encoded.data, self.nal_length_bytes, self.codec)?;
        if is_keyframe {
            // Cached parameter sets precede every keyframe for clean joins.
            let mut with_parameters = Vec::with_capacity(
                bytes.len() + self.parameters.iter().map(|p| p.len() + 4).sum::<usize>(),
            );
            for parameter in &self.parameters {
                append_annex_b(&mut with_parameters, Some(parameter));
            }
            with_parameters.append(&mut bytes);
            bytes = with_parameters;
        }
        let timestamp = decoded_timestamp(encoded.presentation_time)
            .ok_or_else(|| MediaError::Operation("invalid encoder output timestamp".to_owned()))?;
        Ok(EncodedAccessUnit {
            bytes,
            timestamp,
            is_keyframe,
        })
    }
}

impl VideoEncoder for VideoToolboxH264Encoder {
    fn take_diagnostics(&mut self) -> media_core::diagnostics::StageMetrics {
        std::mem::take(&mut self.diagnostics)
    }

    fn take_source_timing(
        &mut self,
        timestamp: Duration,
    ) -> Option<media_core::source_timing::SourceTiming> {
        self.source_timeline.take(timestamp)
    }

    fn encode(
        &mut self,
        frame: VideoFrameRef<'_>,
    ) -> Result<Option<EncodedAccessUnit>, MediaError> {
        let encode_started = Instant::now();
        if frame.width != self.config.width || frame.height != self.config.height {
            return Err(MediaError::Operation(format!(
                "encoder frame size {}x{} does not match configured {}x{}",
                frame.width, frame.height, self.config.width, self.config.height
            )));
        }
        let expected_len = usize::try_from(frame.width)
            .ok()
            .and_then(|width| {
                usize::try_from(frame.height)
                    .ok()
                    .map(|height| (width, height))
            })
            .and_then(|(width, height)| width.checked_mul(height))
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| MediaError::Operation("encoder frame size overflows".to_owned()))?;
        if frame.bytes.len() != expected_len {
            return Err(MediaError::Operation(format!(
                "encoder expected {expected_len} BGRA bytes, got {}",
                frame.bytes.len()
            )));
        }
        if self.rebuild_before_next_frame {
            self.rebuild()?;
        }
        self.copy_input_to_surface(frame.bytes, frame.width, frame.height)?;

        let input_time = (
            self.frame_index,
            i32::try_from(self.config.frames_per_second).unwrap_or(i32::MAX),
        );
        let input_timestamp = decoded_timestamp(input_time)
            .ok_or_else(|| MediaError::Operation("invalid encoder input timestamp".to_owned()))?;
        self.source_timeline.record(
            input_timestamp,
            frame.captured_at,
            Instant::now(),
            media_core::unix_us(),
        );
        let session_encode_started = Instant::now();
        let encoded = self
            .session
            .encode(&self.surface, input_time)
            .map_err(operation)?;
        let session_encode_elapsed = session_encode_started.elapsed();
        self.diagnostics
            .record("vt_session_encode_wait", session_encode_elapsed);
        self.frame_index = self.frame_index.saturating_add(1);
        if encoded.info_flags & ffi::kVTEncodeInfo_Asynchronous != 0 {
            self.diagnostics
                .record("vt_encode_async_callback", session_encode_elapsed);
        }
        let frame_dropped = encoded.info_flags & ffi::kVTEncodeInfo_FrameDropped != 0;
        if frame_dropped {
            self.diagnostics
                .record("vt_encode_dropped_flag", session_encode_elapsed);
        }
        if encoded.cm_sample_buffer().is_none() {
            // VideoToolbox communicates a rate-control/frame-drop decision via
            // `kVTEncodeInfo_FrameDropped` and a missing CMSampleBuffer. Keep
            // both signals: a missing sample without the flag is diagnostically
            // different from an explicit encoder drop.
            self.diagnostics
                .record("vt_encode_no_sample", session_encode_elapsed);
            if !frame_dropped {
                self.diagnostics.record(
                    "vt_encode_no_sample_without_drop_flag",
                    session_encode_elapsed,
                );
            }
            self.diagnostics
                .record("vt_encode_call", encode_started.elapsed());
            return Ok(None);
        }
        let output_normalize_started = Instant::now();
        let output = self.normalize_output(&encoded)?;
        self.diagnostics
            .record("vt_encode_sample_returned", session_encode_elapsed);
        self.diagnostics.record(
            "vt_encode_output_normalize",
            output_normalize_started.elapsed(),
        );
        self.diagnostics
            .record("vt_encode_call", encode_started.elapsed());
        Ok(Some(output))
    }

    fn set_bitrate(&mut self, bits_per_second: u32) -> Result<(), MediaError> {
        if bits_per_second == 0 {
            return Err(MediaError::Operation(
                "encoder bitrate must be non-zero".to_owned(),
            ));
        }
        if bits_per_second == self.config.bitrate {
            return Ok(());
        }
        let bitrate = CFNumber::from_i64(i64::from(bits_per_second));
        // VideoToolbox exposes AverageBitRate as a mutable compression-session
        // property. Update the live session first so routine adaptation does
        // not tear down the reference chain or stall the media loop rebuilding
        // the encoder. Keep the previous compatible rebuild as a fallback for
        // encoders that reject changing this property while running.
        // SAFETY: the SDK constant is a process-lifetime CFString and bitrate
        // is a live CFNumber retained for the duration of VTSessionSetProperty.
        match unsafe {
            self.session.set_property(
                ffi::kVTCompressionPropertyKey_AverageBitRate,
                bitrate.as_ptr().cast(),
            )
        } {
            Ok(()) => {
                self.config.bitrate = bits_per_second;
                self.last_bitrate_update_live = true;
                info!(
                    bitrate_bps = bits_per_second,
                    update = "live_property",
                    "VideoToolbox bitrate updated without rebuilding the encoder"
                );
                return Ok(());
            }
            Err(error) => {
                self.last_bitrate_update_live = false;
                warn!(%error, bitrate_bps = bits_per_second, "VideoToolbox rejected live bitrate change; rebuilding encoder session");
            }
        }
        // Build first: a rejected bitrate must not poison the live session and
        // turn the next encode into a fatal error for the whole presentation.
        let mut next_config = self.config;
        next_config.bitrate = bits_per_second;
        let next_session = build_encoder(next_config, self.codec)?;
        self.session = next_session;
        self.config = next_config;
        self.rebuild_before_next_frame = false;
        self.parameters.clear();
        self.source_timeline = media_core::source_timing::SourceTimeline::default();
        Ok(())
    }

    fn request_keyframe(&mut self) -> Result<(), MediaError> {
        self.rebuild_before_next_frame = true;
        Ok(())
    }

    fn set_refinement(&mut self, active: bool) -> Result<bool, MediaError> {
        // QP 51 is the H.264/H.265 maximum (no cap); 22 keeps text crisp.
        let qp = CFNumber::from_i64(if active { 22 } else { 51 });
        // SAFETY: SDK-owned key; the CFNumber outlives the call.
        match unsafe {
            self.session.set_property(
                ffi::kVTCompressionPropertyKey_MaxAllowedFrameQP,
                qp.as_ptr().cast(),
            )
        } {
            Ok(()) => Ok(true),
            Err(error) => {
                warn!(%error, active, "VideoToolbox MaxAllowedFrameQP unsupported; bitrate-only refinement");
                Ok(false)
            }
        }
    }
}

pub struct VideoToolboxH264Decoder {
    config: H264Config,
    codec: VideoCodec,
    session: Option<DecompressionSession>,
    receiver: Option<Receiver<videotoolbox::DecodedFrame>>,
    /// Latest parameter sets by role: H.264 [-, SPS, PPS]; H.265 [VPS, SPS, PPS].
    parameters: [Option<Vec<u8>>; 3],
    diagnostics: media_core::diagnostics::StageMetrics,
    output_pixel_format: Option<u32>,
}

impl std::fmt::Debug for VideoToolboxH264Decoder {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VideoToolboxH264Decoder")
            .field("config", &self.config)
            .field("initialized", &self.session.is_some())
            .finish_non_exhaustive()
    }
}

impl VideoToolboxH264Decoder {
    fn note_output_format(&mut self, pixel_format: u32) {
        if self.output_pixel_format != Some(pixel_format) {
            info!(
                pixel_format,
                native_bgra = pixel_format == BGRA,
                "VideoToolbox decoded output format"
            );
            self.output_pixel_format = Some(pixel_format);
        }
    }

    /// Creates a lazy H.264 decoder that initializes after receiving SPS/PPS.
    ///
    /// # Errors
    /// Returns an error when the configuration is invalid.
    pub fn new(config: H264Config) -> Result<Self, MediaError> {
        Self::with_codec(config, VideoCodec::H264)
    }

    /// Creates a lazy decoder for `codec` that initializes after parameter sets arrive.
    ///
    /// # Errors
    /// Returns an error when the configuration is invalid.
    pub fn with_codec(config: H264Config, codec: VideoCodec) -> Result<Self, MediaError> {
        validate_config(config)?;
        Ok(Self {
            config,
            codec,
            session: None,
            receiver: None,
            parameters: [None, None, None],
            diagnostics: media_core::diagnostics::StageMetrics::default(),
            output_pixel_format: None,
        })
    }

    #[must_use]
    pub fn descriptor(&self) -> CodecDescriptor {
        CodecDescriptor {
            name: format!("Apple VideoToolbox {} decoder", self.codec),
            kind: self
                .session
                .as_ref()
                .map_or(CodecBackendKind::Unknown, decoder_backend),
            asynchronous: true,
        }
    }

    fn ensure_session(&mut self) -> Result<(), MediaError> {
        if self.session.is_some() {
            return Ok(());
        }
        let format = self.format_description()?;
        let (sender, receiver) = mpsc::channel();
        // Ask VideoToolbox for the renderer's BGRA format, avoiding the scalar
        // per-pixel NV12 conversion. Keep the existing NV12 path as a fallback.
        // SAFETY: this exported CoreVideo key is a process-lifetime CFString;
        // from_raw_borrowed retains it for the dictionary's construction.
        let key = unsafe {
            CFString::from_raw_borrowed(
                apple_cf::raw::kCVPixelBufferPixelFormatTypeKey
                    .cast_mut()
                    .cast(),
            )
        }
        .ok_or_else(|| MediaError::Operation("CoreVideo pixel format key missing".to_owned()))?;
        let pixel_format = CFNumber::from_u64(u64::from(BGRA));
        let attributes = CFDictionary::from_pairs(&[(&key, &pixel_format)]);
        let native_bgra = std::env::var("CLASSROOM_VT_NATIVE_BGRA").as_deref() != Ok("0");
        let preferred_sender = sender.clone();
        let session = DecompressionSession::new_with_image_buffer_attributes(
            &format,
            native_bgra.then_some(&attributes),
            move |frame| {
                let _ = preferred_sender.send(frame);
            },
        )
        .or_else(|error| {
            warn!(%error, "VideoToolbox preferred output unavailable; retrying default format");
            DecompressionSession::new(&format, move |frame| {
                let _ = sender.send(frame);
            })
        })
        .map_err(operation)?;
        session.set_real_time(true).map_err(operation)?;
        info!(codec = %self.codec, decoder_backend = %decoder_backend(&session), native_bgra_requested = native_bgra, "selected VideoToolbox decoder");
        self.session = Some(session);
        self.receiver = Some(receiver);
        Ok(())
    }
}

impl VideoToolboxH264Decoder {
    fn format_description(&self) -> Result<CMFormatDescription, MediaError> {
        let needed = match self.codec {
            VideoCodec::H264 => &self.parameters[1..],
            VideoCodec::H265 => &self.parameters[..],
        };
        let sets = needed
            .iter()
            .map(|p| p.as_deref())
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                MediaError::Operation(format!(
                    "decoder is waiting for {} parameter sets",
                    self.codec
                ))
            })?;
        make_format_description(self.codec, &sets)
    }
    /// Parameter-set role for a NAL type, if it is one.
    const fn parameter_role(codec: VideoCodec, kind: u8) -> Option<usize> {
        match (codec, kind) {
            (VideoCodec::H264, 7) | (VideoCodec::H265, 33) => Some(1),
            (VideoCodec::H264, 8) | (VideoCodec::H265, 34) => Some(2),
            (VideoCodec::H265, 32) => Some(0),
            _ => None,
        }
    }
    const fn is_slice(codec: VideoCodec, kind: u8) -> bool {
        match codec {
            VideoCodec::H264 => matches!(kind, 1 | 5),
            VideoCodec::H265 => kind <= 31,
        }
    }
}

impl VideoDecoder for VideoToolboxH264Decoder {
    fn take_diagnostics(&mut self) -> media_core::diagnostics::StageMetrics {
        std::mem::take(&mut self.diagnostics)
    }

    fn decode(
        &mut self,
        access_unit: EncodedAccessUnitRef<'_>,
    ) -> Result<Vec<CoreDecodedFrame>, MediaError> {
        let units = annex_b_units(access_unit.bytes);
        let mut parameter_changed = false;
        let mut avcc = Vec::with_capacity(access_unit.bytes.len());
        for unit in units {
            let Some(kind) = self.codec.nal_type(unit) else {
                continue;
            };
            if let Some(role) = Self::parameter_role(self.codec, kind) {
                if self.parameters[role].as_deref() != Some(unit) {
                    self.parameters[role] = Some(unit.to_vec());
                    parameter_changed = true;
                }
            } else if Self::is_slice(self.codec, kind) {
                let length = u32::try_from(unit.len())
                    .map_err(|_| MediaError::Operation("NAL unit is too large".to_owned()))?;
                avcc.extend_from_slice(&length.to_be_bytes());
                avcc.extend_from_slice(unit);
            }
        }
        if parameter_changed {
            self.session = None;
            self.receiver = None;
        }
        if avcc.is_empty() {
            return Ok(Vec::new());
        }
        self.ensure_session()?;
        let format = self.format_description()?;
        let sample = make_sample_buffer(&avcc, &format, access_unit.timestamp)?;
        let session = self
            .session
            .as_ref()
            .expect("initialized by ensure_session");
        let submitted = Instant::now();
        session.decode(&sample).map_err(operation)?;
        self.diagnostics
            .record("vt_decode_submit", submitted.elapsed());
        let waiting = Instant::now();
        session.wait_for_async_frames().map_err(operation)?;
        self.diagnostics.record("vt_decode_wait", waiting.elapsed());

        let receiver = self
            .receiver
            .as_ref()
            .expect("initialized by ensure_session");
        let mut native_frames = Vec::new();
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(frame) => native_frames.push(frame),
            Err(RecvTimeoutError::Timeout) => return Ok(Vec::new()),
            Err(RecvTimeoutError::Disconnected) => {
                return Err(MediaError::Operation(
                    "VideoToolbox decoder callback channel disconnected".to_owned(),
                ));
            }
        }
        native_frames.extend(receiver.try_iter());
        let callback_error = native_frames
            .iter()
            .find(|decoded| decoded.status != 0)
            .map(|decoded| decoded.status);
        if let Some(status) = callback_error {
            // A failed VT decode can leave reference state unusable. Drop the
            // session so the caller's keyframe recovery recreates it cleanly.
            self.session = None;
            self.receiver = None;
            return Err(MediaError::Operation(format!(
                "VideoToolbox decoder callback failed with status {status}; session reset"
            )));
        }
        let mut output = Vec::new();
        for decoded in native_frames {
            if let Some(image) = decoded.image_buffer {
                self.note_output_format(image.pixel_format());
                let copied = Instant::now();
                let mut frame = copy_decoded_bgra(&image)?;
                self.diagnostics
                    .record("vt_decode_copy_convert", copied.elapsed());
                // Use the callback's actual output timestamp, never the most
                // recently submitted input: VideoToolbox may buffer frames.
                frame.media_timestamp = decoded_timestamp(decoded.presentation_time);
                output.push(frame);
            }
        }
        Ok(output)
    }
}

fn decoded_timestamp((value, timescale): (i64, i32)) -> Option<Duration> {
    let value = u128::try_from(value).ok()?;
    let timescale = u128::try_from(timescale).ok().filter(|scale| *scale != 0)?;
    let nanos = value.checked_mul(1_000_000_000)? / timescale;
    Some(Duration::from_nanos(u64::try_from(nanos).ok()?))
}

fn validate_config(config: H264Config) -> Result<(), MediaError> {
    if config.width == 0
        || config.height == 0
        || config.frames_per_second == 0
        || config.bitrate == 0
        || config.keyframe_interval_frames == 0
    {
        return Err(MediaError::Operation(
            "H.264 dimensions, frame rate, bitrate, and keyframe interval must be non-zero"
                .to_owned(),
        ));
    }
    Ok(())
}

fn build_encoder(config: H264Config, codec: VideoCodec) -> Result<CompressionSession, MediaError> {
    if std::env::var("CLASSROOM_VT_LOW_LATENCY").as_deref() != Ok("0") {
        match build_encoder_mode(config, codec, true, true) {
            Ok(session) => {
                info!(
                    low_latency_rate_control = true,
                    "VideoToolbox encoder mode selected"
                );
                return Ok(session);
            }
            Err(error) => {
                warn!(%error, "VideoToolbox low-latency encoder unavailable; using compatible fallback");
            }
        }
    }
    match build_encoder_mode(config, codec, false, true) {
        Ok(session) => {
            info!(
                low_latency_rate_control = false,
                hardware_encoder_allowed = true,
                "VideoToolbox encoder mode selected"
            );
            Ok(session)
        }
        Err(error) => {
            warn!(%error, "VideoToolbox hardware-enabled session unavailable; falling back to default encoder selection");
            let session = build_encoder_mode(config, codec, false, false)?;
            info!(
                low_latency_rate_control = false,
                hardware_encoder_allowed = false,
                "VideoToolbox encoder mode selected"
            );
            Ok(session)
        }
    }
}

fn build_encoder_mode(
    config: H264Config,
    codec: VideoCodec,
    low_latency: bool,
    hardware_allowed: bool,
) -> Result<CompressionSession, MediaError> {
    let builder = CompressionSession::builder(
        i32::try_from(config.width)
            .map_err(|_| MediaError::Operation("encoder width is too large".to_owned()))?,
        i32::try_from(config.height)
            .map_err(|_| MediaError::Operation("encoder height is too large".to_owned()))?,
        vt_codec(codec),
    )
    .with_real_time(true)
    .with_allow_frame_reordering(false)
    .with_average_bit_rate(i32::try_from(config.bitrate).unwrap_or(i32::MAX))
    .with_expected_frame_rate(f64::from(config.frames_per_second))
    .with_max_keyframe_interval(i32::try_from(config.keyframe_interval_frames).unwrap_or(i32::MAX));
    let builder = match (codec, std::env::var("CLASSROOM_VT_PROFILE").as_deref()) {
        (VideoCodec::H265, _) => builder.with_profile_level(ProfileLevel::HEVCMainAutoLevel),
        (VideoCodec::H264, Ok("baseline")) => {
            builder.with_profile_level(ProfileLevel::H264BaselineAutoLevel)
        }
        (VideoCodec::H264, Ok("main")) => {
            builder.with_profile_level(ProfileLevel::H264MainAutoLevel)
        }
        (VideoCodec::H264, _) => builder,
    };
    // Explicitly allow VideoToolbox to select hardware. Without this encoder
    // specification some macOS versions create a software session even when
    // Apple Silicon hardware encoding is available. Keep it enabled in both
    // low-latency and compatibility modes, then report the selected backend.
    // SAFETY: SDK-owned keys and CFBoolean are retained by the dictionary
    // passed to CompressionSessionCreate.
    let enabled = unsafe { CFType::from_raw_borrowed(ffi::kCFBooleanTrue.cast_mut().cast()) }
        .ok_or_else(|| MediaError::Operation("CoreFoundation true missing".to_owned()))?;
    let hardware_key = if hardware_allowed {
        Some(
            unsafe {
                CFString::from_raw_borrowed(
                    ffi::kVTVideoEncoderSpecification_EnableHardwareAcceleratedVideoEncoder
                        .cast_mut()
                        .cast(),
                )
            }
            .ok_or_else(|| MediaError::Operation("hardware encoder key missing".to_owned()))?,
        )
    } else {
        None
    };
    let low_latency_key = if low_latency {
        Some(
            unsafe {
                CFString::from_raw_borrowed(
                    ffi::kVTVideoEncoderSpecification_EnableLowLatencyRateControl
                        .cast_mut()
                        .cast(),
                )
            }
            .ok_or_else(|| MediaError::Operation("low-latency encoder key missing".to_owned()))?,
        )
    } else {
        None
    };
    let builder =
        match (hardware_key.as_ref(), low_latency_key.as_ref()) {
            (Some(hardware), Some(low_latency)) => {
                builder.with_encoder_specification(CFDictionary::from_pairs(&[
                    (hardware, &enabled),
                    (low_latency, &enabled),
                ]))
            }
            (Some(hardware), None) => builder
                .with_encoder_specification(CFDictionary::from_pairs(&[(hardware, &enabled)])),
            (None, Some(low_latency)) => builder
                .with_encoder_specification(CFDictionary::from_pairs(&[(low_latency, &enabled)])),
            (None, None) => builder,
        };
    let session = builder.build().map_err(operation)?;
    let speed = apply_latency_properties(&session);
    info!(codec = %codec, encoder_backend = %encoder_backend(&session), speed_priority = speed, "selected VideoToolbox encoder");
    Ok(session)
}

/// Latency over power/quality: encode each frame as fast as the hardware
/// can and emit it immediately (no frame delay). Best effort; disable for
/// A/B with `CLASSROOM_VT_SPEED_PRIORITY=0`. Returns whether it was attempted.
fn apply_latency_properties(session: &CompressionSession) -> bool {
    let speed = std::env::var("CLASSROOM_VT_SPEED_PRIORITY").as_deref() != Ok("0");
    if speed {
        let zero = CFNumber::from_i64(0);
        // SAFETY: SDK-owned keys; CFBoolean constants are process-lifetime and
        // the CFNumber outlives the call.
        let results = unsafe {
            [
                session
                    .set_property(
                        ffi::kVTCompressionPropertyKey_PrioritizeEncodingSpeedOverQuality,
                        ffi::kCFBooleanTrue.cast(),
                    )
                    .is_ok(),
                session
                    .set_property(
                        ffi::kVTCompressionPropertyKey_MaximizePowerEfficiency,
                        ffi::kCFBooleanFalse.cast(),
                    )
                    .is_ok(),
                session
                    .set_property(
                        ffi::kVTCompressionPropertyKey_MaxFrameDelayCount,
                        zero.as_ptr().cast(),
                    )
                    .is_ok(),
            ]
        };
        info!(
            prioritize_speed = results[0],
            power_efficiency_off = results[1],
            max_frame_delay_zero = results[2],
            "VideoToolbox latency properties"
        );
    }
    speed
}

fn encoder_backend(session: &CompressionSession) -> CodecBackendKind {
    // SAFETY: VideoToolbox exports this CFString key for compression sessions.
    hardware_kind(
        unsafe {
            session
                .copy_property(ffi::kVTCompressionPropertyKey_UsingHardwareAcceleratedVideoEncoder)
        }
        .ok()
        .flatten(),
    )
}

fn decoder_backend(session: &DecompressionSession) -> CodecBackendKind {
    // SAFETY: VideoToolbox exports this CFString key for decompression sessions.
    hardware_kind(
        unsafe {
            session.copy_property(
                ffi::kVTDecompressionPropertyKey_UsingHardwareAcceleratedVideoDecoder,
            )
        }
        .ok()
        .flatten(),
    )
}

fn hardware_kind(property: Option<CFType>) -> CodecBackendKind {
    let Some(value) = property else {
        return CodecBackendKind::Unknown;
    };
    // SAFETY: a type check precedes CFBooleanGetValue; CFType owns the value.
    if u64::try_from(value.type_id()).unwrap_or(u64::MAX) != unsafe { CFBooleanGetTypeID() } {
        return CodecBackendKind::Unknown;
    }
    if unsafe { CFBooleanGetValue(value.as_ptr().cast()) } != 0 {
        CodecBackendKind::Hardware
    } else {
        CodecBackendKind::Software
    }
}

/// Parameter sets in stream order plus the AVCC/HVCC NAL length-field width.
fn parameter_sets(
    format: &CMFormatDescription,
    codec: VideoCodec,
) -> Result<(Vec<Vec<u8>>, usize), MediaError> {
    let mut values = Vec::new();
    let mut nal_length_bytes = 4;
    let mut index = 0_usize;
    loop {
        let mut pointer = std::ptr::null();
        let mut size = 0_usize;
        let mut count = 0_usize;
        let mut header_length = 0_i32;
        // SAFETY: CoreMedia writes borrowed pointers into the format
        // description, which outlives this copy.
        let status = unsafe {
            match codec {
                VideoCodec::H264 => CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
                    format.as_ptr(),
                    index,
                    &raw mut pointer,
                    &raw mut size,
                    &raw mut count,
                    &raw mut header_length,
                ),
                VideoCodec::H265 => CMVideoFormatDescriptionGetHEVCParameterSetAtIndex(
                    format.as_ptr(),
                    index,
                    &raw mut pointer,
                    &raw mut size,
                    &raw mut count,
                    &raw mut header_length,
                ),
            }
        };
        if status != 0 || pointer.is_null() || size == 0 {
            return Err(MediaError::Operation(format!(
                "failed to read {codec} parameter set {index}: status {status}"
            )));
        }
        let parameter_set = unsafe { std::slice::from_raw_parts(pointer, size) }.to_vec();
        if index == 0 {
            if !(1..=4).contains(&header_length) {
                return Err(MediaError::Operation(format!(
                    "unsupported NAL length field size {header_length}"
                )));
            }
            nal_length_bytes =
                usize::try_from(header_length).expect("positive NAL length field size checked");
        }
        values.push(parameter_set);
        index += 1;
        if index >= count.min(8) {
            break;
        }
    }
    // Keep only the decoder-required parameter sets (drop e.g. SEI entries).
    let required: &[u8] = match codec {
        VideoCodec::H264 => &[7, 8],
        VideoCodec::H265 => &[32, 33, 34],
    };
    values.retain(|set| {
        codec
            .nal_type(set)
            .is_some_and(|kind| required.contains(&kind))
    });
    let refs: Vec<&[u8]> = values.iter().map(Vec::as_slice).collect();
    validate_parameter_sets(codec, &refs)?;
    Ok((values, nal_length_bytes))
}

fn validate_parameter_sets(codec: VideoCodec, sets: &[&[u8]]) -> Result<(), MediaError> {
    let expected: &[u8] = match codec {
        VideoCodec::H264 => &[7, 8],
        VideoCodec::H265 => &[32, 33, 34],
    };
    let mut seen = Vec::new();
    for set in sets {
        let kind = codec.nal_type(set).unwrap_or(u8::MAX);
        if !expected.contains(&kind) {
            return Err(MediaError::Operation(format!(
                "invalid {codec} parameter set: NAL type {kind}, {} bytes",
                set.len()
            )));
        }
        seen.push(kind);
    }
    if expected.iter().any(|kind| !seen.contains(kind)) {
        return Err(MediaError::Operation(format!(
            "incomplete {codec} parameter sets"
        )));
    }
    Ok(())
}

fn make_format_description(
    codec: VideoCodec,
    sets: &[&[u8]],
) -> Result<CMFormatDescription, MediaError> {
    validate_parameter_sets(codec, sets)?;
    let pointers: Vec<*const u8> = sets.iter().map(|set| set.as_ptr()).collect();
    let sizes: Vec<usize> = sets.iter().map(|set| set.len()).collect();
    let mut raw = std::ptr::null_mut();
    // SAFETY: pointers/sizes describe live slices for the duration of the call;
    // CoreMedia copies the parameter sets into the new description.
    let status = unsafe {
        match codec {
            VideoCodec::H264 => CMVideoFormatDescriptionCreateFromH264ParameterSets(
                kCFAllocatorDefault,
                pointers.len(),
                pointers.as_ptr(),
                sizes.as_ptr(),
                4,
                &raw mut raw,
            ),
            VideoCodec::H265 => CMVideoFormatDescriptionCreateFromHEVCParameterSets(
                kCFAllocatorDefault,
                pointers.len(),
                pointers.as_ptr(),
                sizes.as_ptr(),
                4,
                std::ptr::null(),
                &raw mut raw,
            ),
        }
    };
    if status != 0 || raw.is_null() {
        return Err(MediaError::Operation(format!(
            "failed to create {codec} format description: status {status}"
        )));
    }
    unsafe { CMFormatDescription::from_raw(raw) }.ok_or_else(|| {
        MediaError::Operation(format!("{codec} format description was unexpectedly null"))
    })
}

fn make_sample_buffer(
    avcc: &[u8],
    format: &CMFormatDescription,
    timestamp: Duration,
) -> Result<CMSampleBuffer, MediaError> {
    let block = CMBlockBuffer::create(avcc)
        .ok_or_else(|| MediaError::Operation("failed to allocate H.264 block buffer".to_owned()))?;
    let nanos = i64::try_from(timestamp.as_nanos()).unwrap_or(i64::MAX);
    let timing = CMSampleTimingInfo::with_times(
        CMTime::INVALID,
        CMTime::new(nanos, 1_000_000_000),
        CMTime::INVALID,
    );
    let size = avcc.len();
    let mut raw = std::ptr::null_mut();
    let status = unsafe {
        CMSampleBufferCreateReady(
            kCFAllocatorDefault,
            block.as_ptr(),
            format.as_ptr(),
            1,
            1,
            &raw const timing,
            1,
            &raw const size,
            &raw mut raw,
        )
    };
    if status != 0 || raw.is_null() {
        return Err(MediaError::Operation(format!(
            "failed to create H.264 sample buffer: status {status}"
        )));
    }
    unsafe { CMSampleBuffer::from_raw(raw) }.ok_or_else(|| {
        MediaError::Operation("H.264 sample buffer was unexpectedly null".to_owned())
    })
}

fn copy_decoded_bgra(image: &CVPixelBuffer) -> Result<CoreDecodedFrame, MediaError> {
    let guard = image
        .lock(CVPixelBufferLockFlags::READ_ONLY)
        .map_err(|status| {
            MediaError::Operation(format!("failed to lock decoded frame: {status}"))
        })?;
    let width = guard.width();
    let height = guard.height();
    let mut bytes = vec![0_u8; width.saturating_mul(height).saturating_mul(4)];
    match guard.pixel_format() {
        BGRA => {
            let source = unsafe { guard.as_slice() }.ok_or_else(|| {
                MediaError::Operation("decoded BGRA frame has no byte storage".to_owned())
            })?;
            for row in 0..height {
                let source_start = row * guard.bytes_per_row();
                let target_start = row * width * 4;
                bytes[target_start..target_start + width * 4]
                    .copy_from_slice(&source[source_start..source_start + width * 4]);
            }
        }
        NV12_VIDEO_RANGE | NV12_FULL_RANGE => {
            let y_plane = unsafe { guard.plane_data(0) }.ok_or_else(|| {
                MediaError::Operation("decoded NV12 frame has no Y plane".to_owned())
            })?;
            let uv_plane = unsafe { guard.plane_data(1) }.ok_or_else(|| {
                MediaError::Operation("decoded NV12 frame has no UV plane".to_owned())
            })?;
            nv12_to_bgra(
                y_plane,
                guard.bytes_per_row_of_plane(0),
                uv_plane,
                guard.bytes_per_row_of_plane(1),
                width,
                height,
                guard.pixel_format() == NV12_FULL_RANGE,
                &mut bytes,
            );
        }
        format => {
            return Err(MediaError::Operation(format!(
                "unsupported decoded pixel format 0x{format:08x}"
            )));
        }
    }
    Ok(CoreDecodedFrame {
        format: media_core::DecodedPixelFormat::Bgra,
        media_timestamp: None,
        timing: None,
        bytes,
        width: u32::try_from(width).unwrap_or(u32::MAX),
        height: u32::try_from(height).unwrap_or(u32::MAX),
    })
}

#[allow(clippy::too_many_arguments)]
fn nv12_to_bgra(
    y_plane: &[u8],
    y_pitch: usize,
    uv_plane: &[u8],
    uv_pitch: usize,
    width: usize,
    height: usize,
    full_range: bool,
    output: &mut [u8],
) {
    for y in 0..height {
        for x in 0..width {
            let luminance = i32::from(y_plane[y * y_pitch + x]);
            let uv_index = (y / 2) * uv_pitch + (x / 2) * 2;
            let cb = i32::from(uv_plane[uv_index]) - 128;
            let cr = i32::from(uv_plane[uv_index + 1]) - 128;
            let (red, green, blue) = if full_range {
                (
                    luminance + ((1_403 * cr) >> 10),
                    luminance - ((344 * cb + 714 * cr) >> 10),
                    luminance + ((1_773 * cb) >> 10),
                )
            } else {
                let scaled_y = (luminance - 16).max(0) * 1_192;
                (
                    (scaled_y + 1_634 * cr) >> 10,
                    (scaled_y - 400 * cb - 833 * cr) >> 10,
                    (scaled_y + 2_066 * cb) >> 10,
                )
            };
            let offset = (y * width + x) * 4;
            output[offset] = clamp_u8(blue);
            output[offset + 1] = clamp_u8(green);
            output[offset + 2] = clamp_u8(red);
            output[offset + 3] = u8::MAX;
        }
    }
}

fn clamp_u8(value: i32) -> u8 {
    u8::try_from(value.clamp(0, 255)).unwrap_or(0)
}

fn avcc_to_annex_b(
    bytes: &[u8],
    length_bytes: usize,
    codec: VideoCodec,
) -> Result<(Vec<u8>, bool), MediaError> {
    if !(1..=4).contains(&length_bytes) {
        return Err(MediaError::Operation(format!(
            "unsupported AVCC NAL length field size {length_bytes}"
        )));
    }
    if bytes.is_empty() {
        return Err(MediaError::Operation(
            "empty VideoToolbox H.264 sample".to_owned(),
        ));
    }
    let mut output = Vec::with_capacity(bytes.len());
    let mut keyframe = false;
    let mut offset = 0_usize;
    while offset < bytes.len() {
        if offset + length_bytes > bytes.len() {
            return Err(MediaError::Operation(
                "truncated AVCC NAL length".to_owned(),
            ));
        }
        let mut length = 0_usize;
        for byte in &bytes[offset..offset + length_bytes] {
            length = length
                .checked_mul(256)
                .and_then(|value| value.checked_add(usize::from(*byte)))
                .ok_or_else(|| MediaError::Operation("AVCC NAL length overflow".to_owned()))?;
        }
        offset += length_bytes;
        let Some(end) = offset.checked_add(length) else {
            return Err(MediaError::Operation("AVCC NAL length overflow".to_owned()));
        };
        if length == 0 || end > bytes.len() {
            return Err(MediaError::Operation("invalid AVCC NAL length".to_owned()));
        }
        let unit = &bytes[offset..end];
        if !codec.is_elementary(unit) {
            return Err(MediaError::Operation(format!(
                "invalid {codec} NAL unit type {:?} in VideoToolbox output",
                codec.nal_type(unit)
            )));
        }
        keyframe |= codec.is_random_access(unit);
        output.extend_from_slice(&[0, 0, 0, 1]);
        output.extend_from_slice(&bytes[offset..offset + length]);
        offset = end;
    }
    Ok((output, keyframe))
}

fn append_annex_b(output: &mut Vec<u8>, unit: Option<&[u8]>) {
    if let Some(unit) = unit {
        output.extend_from_slice(&[0, 0, 0, 1]);
        output.extend_from_slice(unit);
    }
}

fn annex_b_units(bytes: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut index = 0_usize;
    while index + 3 <= bytes.len() {
        let length = if index + 4 <= bytes.len() && bytes[index..index + 4] == [0, 0, 0, 1] {
            4
        } else if bytes[index..index + 3] == [0, 0, 1] {
            3
        } else {
            index += 1;
            continue;
        };
        starts.push((index, index + length));
        index += length;
    }
    starts
        .iter()
        .enumerate()
        .filter_map(|(position, (_, start))| {
            let end = starts
                .get(position + 1)
                .map_or(bytes.len(), |(code_start, _)| *code_start);
            (*start < end).then_some(&bytes[*start..end])
        })
        .collect()
}

fn operation(error: impl std::fmt::Display) -> MediaError {
    MediaError::Operation(format!("VideoToolbox operation failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn output_timestamp_preserves_rtp_precision_and_rejects_invalid_times() {
        assert_eq!(decoded_timestamp((0, 90_000)), Some(Duration::ZERO));
        let stamp = decoded_timestamp((12_345, 90_000)).unwrap();
        assert_eq!(
            stamp.as_nanos() / 100,
            12_345_u128 * 1_000_000_000 / 90_000 / 100
        );
        assert_eq!(
            decoded_timestamp((i64::try_from(stamp.as_nanos()).unwrap(), 1_000_000_000)),
            Some(stamp)
        );
        assert!(decoded_timestamp((-1, 90_000)).is_none());
        assert!(decoded_timestamp((1, 0)).is_none());
        assert!(decoded_timestamp((1, -1)).is_none());
        assert!(decoded_timestamp((i64::MAX, 1)).is_none());
    }

    #[test]
    fn converts_avcc_to_annex_b_and_detects_idr() {
        let input = [0, 0, 0, 2, 0x65, 9, 0, 0, 0, 2, 0x41, 7];
        let (output, keyframe) = avcc_to_annex_b(&input, 4, VideoCodec::H264).unwrap();
        assert!(keyframe);
        assert_eq!(output, [0, 0, 0, 1, 0x65, 9, 0, 0, 0, 1, 0x41, 7]);
    }

    #[test]
    fn parses_avcc_by_declared_length_not_start_code_heuristic() {
        // The AVCC length 0x00000102 resembles a three-byte Annex-B start code.
        let mut input = vec![0, 0, 1, 2];
        input.push(0x41);
        input.resize(4 + 258, 0x55);
        let (output, keyframe) = avcc_to_annex_b(&input, 4, VideoCodec::H264).unwrap();
        assert!(!keyframe);
        assert_eq!(&output[..5], &[0, 0, 0, 1, 0x41]);
        assert_eq!(output.len(), input.len());
    }

    #[test]
    fn converts_avcc_with_non_four_byte_lengths() {
        let input = [2, 0x65, 9];
        let (output, keyframe) = avcc_to_annex_b(&input, 1, VideoCodec::H264).unwrap();
        assert!(keyframe);
        assert_eq!(output, [0, 0, 0, 1, 0x65, 9]);
    }

    #[test]
    fn parses_three_and_four_byte_annex_b_start_codes() {
        let input = [0, 0, 0, 1, 0x67, 1, 0, 0, 1, 0x68, 2];
        let units = annex_b_units(&input);
        assert_eq!(units, [&[0x67, 1][..], &[0x68, 2][..]]);
    }

    #[test]
    #[ignore = "measurement: run with --ignored --nocapture on a Mac"]
    fn data_rate_limit_caps_full_motion_overshoot() {
        let config = H264Config {
            width: 1280,
            height: 720,
            frames_per_second: 30,
            bitrate: 2_000_000,
            keyframe_interval_frames: 120,
        };
        let mut encoder = VideoToolboxH264Encoder::new(config).unwrap();
        let mut seed = 1_u32;
        let mut pixels = vec![0_u8; (config.width * config.height * 4) as usize];
        let mut per_second = [0_usize; 2];
        for index in 0..60 {
            for value in &mut pixels {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                *value = seed as u8;
            }
            let frame = VideoFrameRef {
                bytes: &pixels,
                captured_at: Instant::now(),
                width: config.width,
                height: config.height,
            };
            if let Some(unit) = encoder.encode(frame).unwrap() {
                per_second[index / 30] += unit.bytes.len();
            }
        }
        let target = config.bitrate as usize / 8;
        println!(
            "target_bytes_per_s={target} first_s={} second_s={}",
            per_second[0], per_second[1]
        );
        // Recorded 2026-10-03 (M1 Pro): low-latency RC ≈ 2.57 MB then 98 KB;
        // DataRateLimits made no difference in either RC mode, so it is not used.
        assert!(
            per_second[1] < target * 3,
            "steady-state second within 3x of target"
        );
    }

    #[test]
    fn hevc_hardware_round_trip_when_available() {
        if !hardware_encode_supported(VideoCodec::H265) {
            println!("hevc_hardware_encoder=unavailable; skipped");
            return;
        }
        assert!(hardware_decode_supported(VideoCodec::H265));
        let config = H264Config {
            width: 640,
            height: 360,
            frames_per_second: 60,
            bitrate: 1_500_000,
            keyframe_interval_frames: 120,
        };
        let mut encoder = VideoToolboxH264Encoder::with_codec(config, VideoCodec::H265).unwrap();
        let mut decoder = VideoToolboxH264Decoder::with_codec(config, VideoCodec::H265).unwrap();
        println!("{:?}", encoder.descriptor());
        let mut pixels = vec![0x40_u8; (config.width * config.height * 4) as usize];
        let mut decoded = 0;
        for index in 0..10_u8 {
            pixels[usize::from(index) * 4096] = 0xff;
            let frame = VideoFrameRef {
                bytes: &pixels,
                captured_at: Instant::now(),
                width: config.width,
                height: config.height,
            };
            let Some(unit) = encoder.encode(frame).unwrap() else {
                continue;
            };
            if index == 0 {
                assert!(unit.is_keyframe);
                let kinds: Vec<_> = annex_b_units(&unit.bytes)
                    .iter()
                    .filter_map(|u| VideoCodec::H265.nal_type(u))
                    .collect();
                assert_eq!(&kinds[..3], &[32, 33, 34], "VPS/SPS/PPS precede IRAP");
            }
            decoded += decoder
                .decode(EncodedAccessUnitRef {
                    bytes: &unit.bytes,
                    timestamp: unit.timestamp,
                    is_keyframe: unit.is_keyframe,
                })
                .unwrap()
                .len();
        }
        assert!(decoded >= 8, "decoded {decoded} HEVC frames");
    }

    #[test]
    fn bitrate_change_keeps_the_existing_stream_decodable() {
        let config = H264Config {
            width: 640,
            height: 360,
            frames_per_second: 30,
            bitrate: 1_200_000,
            keyframe_interval_frames: 60,
        };
        let mut encoder = VideoToolboxH264Encoder::new(config).unwrap();
        let mut decoder = VideoToolboxH264Decoder::new(config).unwrap();
        let pixels = vec![0x80_u8; (config.width * config.height * 4) as usize];
        let frame = VideoFrameRef {
            bytes: &pixels,
            captured_at: Instant::now(),
            width: config.width,
            height: config.height,
        };
        let first = encoder.encode(frame).unwrap().expect("initial access unit");
        assert!(first.is_keyframe);
        assert!(!decoder
            .decode(EncodedAccessUnitRef {
                bytes: &first.bytes,
                timestamp: first.timestamp,
                is_keyframe: first.is_keyframe,
            })
            .unwrap()
            .is_empty());

        assert!(encoder.set_bitrate(0).is_err());
        assert_eq!(encoder.config.bitrate, config.bitrate);
        encoder.set_bitrate(600_000).unwrap();
        assert_eq!(encoder.config.bitrate, 600_000);
        let used_live_update = encoder.last_bitrate_update_live;
        println!(
            "bitrate_update_path={}",
            if used_live_update {
                "live"
            } else {
                "rebuild_fallback"
            }
        );
        let after_change = encoder
            .encode(frame)
            .unwrap()
            .expect("access unit after bitrate change");
        assert_eq!(after_change.is_keyframe, !used_live_update);
        assert!(!decoder
            .decode(EncodedAccessUnitRef {
                bytes: &after_change.bytes,
                timestamp: after_change.timestamp,
                is_keyframe: after_change.is_keyframe,
            })
            .unwrap()
            .is_empty());
    }
}
