use std::{
    mem::ManuallyDrop,
    ptr, thread,
    time::{Duration, Instant},
};

use media_core::{
    DecodedFrame, EncodedAccessUnit, EncodedAccessUnitRef, MediaError, VideoDecoder, VideoEncoder,
    VideoFrameRef,
};
use tracing::{info, warn};
use windows::{
    core::{Error as WindowsError, Interface, PWSTR},
    Win32::{
        Foundation::VARIANT_BOOL,
        Media::MediaFoundation::{
            eAVEncCommonRateControlMode_CBR, eAVEncH264VProfile_Main,
            CODECAPI_AVDecNumWorkerThreads, CODECAPI_AVEncCommonLowLatency,
            CODECAPI_AVEncCommonMeanBitRate, CODECAPI_AVEncCommonRateControlMode,
            CODECAPI_AVEncCommonRealTime, CODECAPI_AVEncMPVDefaultBPictureCount,
            CODECAPI_AVEncVideoForceKeyFrame, CODECAPI_AVEncVideoMaxKeyframeDistance,
            CODECAPI_AVLowLatencyMode, ICodecAPI, IMF2DBuffer, IMFActivate, IMFMediaBuffer,
            IMFMediaEventGenerator, IMFMediaType, IMFSample, IMFShutdown, IMFTransform,
            METransformHaveOutput, METransformNeedInput, MFCreateAlignedMemoryBuffer,
            MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample, MFMediaType_Video,
            MFSampleExtension_CleanPoint, MFShutdown, MFStartup, MFTEnumEx,
            MFT_FRIENDLY_NAME_Attribute, MFVideoArea, MFVideoFormat_H264, MFVideoFormat_NV12,
            MFVideoInterlace_Progressive, MFSTARTUP_FULL, MFT_CATEGORY_VIDEO_DECODER,
            MFT_CATEGORY_VIDEO_ENCODER, MFT_ENUM_FLAG, MFT_ENUM_FLAG_ASYNCMFT,
            MFT_ENUM_FLAG_HARDWARE, MFT_ENUM_FLAG_LOCALMFT, MFT_ENUM_FLAG_SORTANDFILTER,
            MFT_ENUM_FLAG_SYNCMFT, MFT_MESSAGE_NOTIFY_BEGIN_STREAMING,
            MFT_MESSAGE_NOTIFY_START_OF_STREAM, MFT_OUTPUT_DATA_BUFFER,
            MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES, MFT_OUTPUT_STREAM_PROVIDES_SAMPLES,
            MFT_REGISTER_TYPE_INFO, MF_EVENT_FLAG_NO_WAIT, MF_E_NO_EVENTS_AVAILABLE,
            MF_E_NO_MORE_TYPES, MF_E_TRANSFORM_NEED_MORE_INPUT, MF_E_TRANSFORM_STREAM_CHANGE,
            MF_MT_ALL_SAMPLES_INDEPENDENT, MF_MT_AVG_BITRATE, MF_MT_DEFAULT_STRIDE,
            MF_MT_FIXED_SIZE_SAMPLES, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE,
            MF_MT_MAJOR_TYPE, MF_MT_MINIMUM_DISPLAY_APERTURE, MF_MT_MPEG2_PROFILE,
            MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SAMPLE_SIZE, MF_MT_SUBTYPE, MF_TRANSFORM_ASYNC,
            MF_TRANSFORM_ASYNC_UNLOCK, MF_VERSION,
        },
        System::{
            Com::{CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED},
            Variant::{VARENUM, VARIANT, VARIANT_0_0, VARIANT_0_0_0, VT_BOOL, VT_UI4},
        },
    },
};

use crate::{
    color::{bgra_to_nv12, copied_nv12_stride, nv12_to_bgra, nv12_to_bgra_layout},
    h264::ParameterSetCache,
    CodecBackendKind, CodecDescriptor, H264Config,
};

pub struct MediaFoundationRuntime {
    com_initialized: bool,
}

impl MediaFoundationRuntime {
    /// Starts COM and Media Foundation on the current media worker thread.
    ///
    /// # Errors
    /// Returns an error when COM or Media Foundation initialization fails.
    pub fn new() -> Result<Self, MediaError> {
        // SAFETY: this guard is created and dropped on the dedicated media worker thread.
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(|error| mf_error("initialize COM", error))?;
            if let Err(error) = MFStartup(MF_VERSION, MFSTARTUP_FULL) {
                CoUninitialize();
                return Err(mf_error("start Media Foundation", error));
            }
        }
        Ok(Self {
            com_initialized: true,
        })
    }
}

impl Drop for MediaFoundationRuntime {
    fn drop(&mut self) {
        // SAFETY: this balances successful initialization on the same worker thread.
        unsafe {
            if let Err(error) = MFShutdown() {
                warn!(%error, "Media Foundation shutdown reported an error");
            }
            if self.com_initialized {
                CoUninitialize();
            }
        }
    }
}

pub struct MfH264Encoder {
    runner: TransformRunner,
    codec_api: ICodecAPI,
    descriptor: CodecDescriptor,
    config: H264Config,
    frame_index: u64,
    source_timeline: media_core::source_timing::SourceTimeline,
    output_has_timestamp: bool,
    keyframe_pending: Option<(i64, Instant)>,
    parameter_sets: ParameterSetCache,
    diagnostics: media_core::diagnostics::StageMetrics,
}

impl MfH264Encoder {
    /// Selects and configures a hardware H.264 encoder, with software fallback.
    ///
    /// # Errors
    /// Returns an error when no enumerated encoder satisfies the low-latency contract.
    pub fn new(_runtime: &MediaFoundationRuntime, config: H264Config) -> Result<Self, MediaError> {
        validate_config(config)?;
        let input = MFT_REGISTER_TYPE_INFO {
            guidMajorType: MFMediaType_Video,
            guidSubtype: MFVideoFormat_NV12,
        };
        let output = MFT_REGISTER_TYPE_INFO {
            guidMajorType: MFMediaType_Video,
            guidSubtype: MFVideoFormat_H264,
        };

        let hardware_flags =
            MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_ASYNCMFT | MFT_ENUM_FLAG_SORTANDFILTER;
        let software_flags =
            MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_LOCALMFT | MFT_ENUM_FLAG_SORTANDFILTER;
        let attempts = [
            (CodecBackendKind::Hardware, hardware_flags),
            (CodecBackendKind::Software, software_flags),
        ];
        let mut failures = Vec::new();
        for (kind, flags) in attempts {
            for activation in enumerate_transforms(
                MFT_CATEGORY_VIDEO_ENCODER,
                flags,
                Some(&input),
                Some(&output),
            )? {
                let name = activation_name(&activation)
                    .unwrap_or_else(|_| "unnamed H.264 encoder".to_owned());
                match activate_encoder(&activation, config, name.clone(), kind) {
                    Ok(encoder) => {
                        info!(encoder = %name, backend = %kind, asynchronous = encoder.descriptor.asynchronous, color_conversion = super::color::bgra_to_nv12_backend(), "selected Media Foundation H.264 encoder");
                        return Ok(encoder);
                    }
                    Err(error) => {
                        failures.push(format!("{name}: {error}"));
                        warn!(encoder = %name, backend = %kind, %error, "H.264 encoder candidate rejected");
                    }
                }
            }
        }
        Err(MediaError::BackendUnavailable(format!(
            "no usable Media Foundation H.264 encoder; {}",
            failures.join("; ")
        )))
    }

    #[must_use]
    pub fn descriptor(&self) -> CodecDescriptor {
        self.descriptor.clone()
    }
}

impl VideoEncoder for MfH264Encoder {
    fn take_source_timing(
        &mut self,
        timestamp: Duration,
    ) -> Option<media_core::source_timing::SourceTiming> {
        if !self.output_has_timestamp {
            self.source_timeline.unmapped_outputs += 1;
            return None;
        }
        self.source_timeline.take(timestamp)
    }
    fn take_diagnostics(&mut self) -> media_core::diagnostics::StageMetrics {
        tracing::info!(target: "telemetry",
            pending_inputs = self.source_timeline.depth(),
            evicted_inputs = self.source_timeline.evicted_inputs,
            unmapped_outputs = self.source_timeline.unmapped_outputs,
            keyframe_pending = self.keyframe_pending.is_some(),
            "teacher MF source association");
        std::mem::take(&mut self.diagnostics)
    }
    fn encode(
        &mut self,
        frame: VideoFrameRef<'_>,
    ) -> Result<Option<EncodedAccessUnit>, MediaError> {
        if frame.width != self.config.width || frame.height != self.config.height {
            return Err(MediaError::Operation(format!(
                "encoder expected {}x{}, got {}x{}",
                self.config.width, self.config.height, frame.width, frame.height
            )));
        }
        let stage = Instant::now();
        let nv12 = bgra_to_nv12(frame.bytes, frame.width, frame.height)?;
        self.diagnostics
            .record("encode_color_conversion_cpu", stage.elapsed());
        let stage = Instant::now();
        let timestamp_hns = timestamp_hns(self.frame_index, self.config.frames_per_second);
        self.frame_index += 1;
        self.source_timeline.record(
            hns_duration(timestamp_hns),
            frame.captured_at,
            Instant::now(),
            media_core::unix_us(),
        );
        let sample = sample_from_bytes(
            &nv12,
            timestamp_hns,
            frame_duration_hns(self.config.frames_per_second),
        )?;
        self.diagnostics
            .record("encode_input_copy", stage.elapsed());
        let stage = Instant::now();
        self.runner.submit(&sample)?;
        self.diagnostics.record("encode_mft_input", stage.elapsed());
        let stage = Instant::now();
        let Some(output) = self.runner.take_output(Duration::from_secs(2))? else {
            return Ok(None);
        };
        self.diagnostics
            .record("encode_mft_output", stage.elapsed());
        let stage = Instant::now();
        let (bytes, _, _clean_point) = sample_bytes(&output)?;
        // A missing sample timestamp cannot be associated with the current input.
        // Keep an RTP timestamp for transport, but omit source freshness metadata.
        // SAFETY: output is a live COM sample owned by this thread.
        let actual_time = unsafe { output.GetSampleTime() }
            .ok()
            .filter(|time| *time >= 0);
        self.output_has_timestamp = actual_time.is_some();
        let output_time = actual_time.unwrap_or(timestamp_hns);
        let (bytes, nal_keyframe) = self.parameter_sets.normalize(&bytes);
        self.diagnostics
            .record("encode_output_copy_normalize", stage.elapsed());
        if nal_keyframe {
            if let Some((minimum_time, requested)) = self.keyframe_pending {
                if self.output_has_timestamp && output_time >= minimum_time {
                    self.diagnostics
                        .record("encode_keyframe_response", requested.elapsed());
                    self.keyframe_pending = None;
                }
            }
        }
        Ok(Some(EncodedAccessUnit {
            bytes,
            timestamp: hns_duration(output_time.max(0)),
            is_keyframe: nal_keyframe,
        }))
    }

    fn set_bitrate(&mut self, bits_per_second: u32) -> Result<(), MediaError> {
        set_codec_u32(
            &self.codec_api,
            &CODECAPI_AVEncCommonMeanBitRate,
            bits_per_second,
        )?;
        self.config.bitrate = bits_per_second;
        Ok(())
    }

    fn set_refinement(&mut self, active: bool) -> Result<bool, MediaError> {
        // Optional MFT capability; absent support keeps bitrate-only refinement.
        Ok(set_codec_u32(
            &self.codec_api,
            &windows::Win32::Media::MediaFoundation::CODECAPI_AVEncVideoMaxQP,
            if active { 22 } else { 51 },
        )
        .is_ok())
    }

    fn request_keyframe(&mut self) -> Result<(), MediaError> {
        set_codec_u32(&self.codec_api, &CODECAPI_AVEncVideoForceKeyFrame, 1)?;
        self.keyframe_pending.get_or_insert((
            timestamp_hns(self.frame_index, self.config.frames_per_second),
            Instant::now(),
        ));
        Ok(())
    }
}

pub struct MfH264Decoder {
    runner: TransformRunner,
    descriptor: CodecDescriptor,
    config: H264Config,
    logged_output_layout: bool,
    diagnostics: media_core::diagnostics::StageMetrics,
    cpu_conversion: bool,
    input_timeline: media_core::source_timing::SourceTimeline,
}

impl MfH264Decoder {
    /// Selects and configures the Media Foundation H.264 decoder.
    ///
    /// # Errors
    /// Returns an error when no compatible decoder can be initialized.
    pub fn new(_runtime: &MediaFoundationRuntime, config: H264Config) -> Result<Self, MediaError> {
        validate_config(config)?;
        let input = MFT_REGISTER_TYPE_INFO {
            guidMajorType: MFMediaType_Video,
            guidSubtype: MFVideoFormat_H264,
        };
        let output = MFT_REGISTER_TYPE_INFO {
            guidMajorType: MFMediaType_Video,
            guidSubtype: MFVideoFormat_NV12,
        };
        let flags = MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_LOCALMFT | MFT_ENUM_FLAG_SORTANDFILTER;
        let mut failures = Vec::new();
        for activation in enumerate_transforms(
            MFT_CATEGORY_VIDEO_DECODER,
            flags,
            Some(&input),
            Some(&output),
        )? {
            let name =
                activation_name(&activation).unwrap_or_else(|_| "unnamed H.264 decoder".to_owned());
            match activate_decoder(&activation, config, name.clone()) {
                Ok(decoder) => {
                    info!(decoder = %name, "selected Media Foundation H.264 decoder");
                    return Ok(decoder);
                }
                Err(error) => failures.push(format!("{name}: {error}")),
            }
        }
        Err(MediaError::BackendUnavailable(format!(
            "no usable Media Foundation H.264 decoder; {}",
            failures.join("; ")
        )))
    }

    #[must_use]
    pub fn descriptor(&self) -> CodecDescriptor {
        self.descriptor.clone()
    }
}

fn decoded_nv12_frame(
    nv12: Vec<u8>,
    config: H264Config,
    stride: u32,
    storage_height: u32,
    timestamp: Option<i64>,
    cpu_conversion: bool,
) -> Result<DecodedFrame, MediaError> {
    let frame = if cpu_conversion {
        DecodedFrame {
            format: media_core::DecodedPixelFormat::Bgra,
            bytes: if stride == config.width && storage_height == config.height {
                nv12_to_bgra(&nv12, config.width, config.height)?
            } else {
                nv12_to_bgra_layout(&nv12, config.width, config.height, stride, storage_height)?
            },
            media_timestamp: timestamp.map(hns_duration),
            timing: None,
            width: config.width,
            height: config.height,
        }
    } else {
        let frame = DecodedFrame {
            format: media_core::DecodedPixelFormat::Nv12 {
                stride,
                storage_height,
            },
            bytes: nv12,
            media_timestamp: timestamp.map(hns_duration),
            timing: None,
            width: config.width,
            height: config.height,
        };
        media_core::DecodedFrameRef {
            format: frame.format,
            bytes: &frame.bytes,
            width: frame.width,
            height: frame.height,
        }
        .validate()?;
        frame
    };

    Ok(frame)
}

impl VideoDecoder for MfH264Decoder {
    fn take_diagnostics(&mut self) -> media_core::diagnostics::StageMetrics {
        tracing::info!(target: "telemetry",
            pending_inputs = self.input_timeline.depth(),
            evicted_inputs = self.input_timeline.evicted_inputs,
            unmapped_outputs = self.input_timeline.unmapped_outputs,
            "student MF input output association");
        std::mem::take(&mut self.diagnostics)
    }
    fn decode(
        &mut self,
        access_unit: EncodedAccessUnitRef<'_>,
    ) -> Result<Vec<DecodedFrame>, MediaError> {
        let stage_started = Instant::now();
        let timestamp = duration_hns(access_unit.timestamp);
        let submitted_at = Instant::now();
        let sample = sample_from_bytes(
            access_unit.bytes,
            timestamp,
            frame_duration_hns(self.config.frames_per_second),
        )?;
        self.diagnostics
            .record("decode_input_copy", stage_started.elapsed());
        let stage_started = Instant::now();
        self.runner.submit(&sample)?;
        self.input_timeline
            .record(hns_duration(timestamp), submitted_at, submitted_at, 0);
        self.diagnostics
            .record("decode_mft_input", stage_started.elapsed());
        let mut frames = Vec::new();
        for _ in 0..4 {
            let stage_started = Instant::now();
            let Some(output) = self.runner.take_output(Duration::from_secs(2))? else {
                self.diagnostics
                    .record("decode_mft_output", stage_started.elapsed());
                break;
            };
            self.diagnostics
                .record("decode_mft_output", stage_started.elapsed());
            let stage_started = Instant::now();
            let (nv12, packed_2d) = decoder_sample_bytes(&output)?;
            self.diagnostics
                .record("decode_output_copy", stage_started.elapsed());
            let stage_started = Instant::now();
            let output_type = unsafe { self.runner.transform.GetOutputCurrentType(0) }
                .map_err(|error| mf_error("query current decoder output type", error))?;
            let subtype = unsafe { output_type.GetGUID(ptr::from_ref(&MF_MT_SUBTYPE)) }
                .map_err(|error| mf_error("read current decoder output subtype", error))?;
            if subtype != MFVideoFormat_NV12 {
                return Err(MediaError::Operation(
                    "decoder output is not NV12".to_owned(),
                ));
            }
            let reported_stride =
                unsafe { output_type.GetUINT32(ptr::from_ref(&MF_MT_DEFAULT_STRIDE)) }
                    .ok()
                    .and_then(|stride| i32::try_from(stride).ok())
                    .filter(|stride| *stride > 0)
                    .and_then(|stride| u32::try_from(stride).ok());
            let (storage_width, storage_height) =
                decoder_storage_dimensions(&output_type, self.config)?;
            let stride = copied_nv12_stride(
                nv12.len(),
                storage_width,
                storage_height,
                reported_stride,
                packed_2d,
            )?;
            if !self.logged_output_layout {
                info!(sample_bytes = nv12.len(), reported_stride = ?reported_stride,
                    selected_stride = stride, packed_2d, storage_width, storage_height,
                    width = self.config.width, height = self.config.height,
                    "Media Foundation decoder output layout");
                self.logged_output_layout = true;
            }
            self.diagnostics
                .record("decode_layout", stage_started.elapsed());
            let stage_started = Instant::now();
            let output_timestamp = unsafe { output.GetSampleTime() }.ok();
            if let Some(timing) =
                output_timestamp.and_then(|time| self.input_timeline.take(hns_duration(time)))
            {
                self.diagnostics
                    .record("decode_input_to_output", timing.submitted_at.elapsed());
            }
            let frame = decoded_nv12_frame(
                nv12,
                self.config,
                stride,
                storage_height,
                output_timestamp,
                self.cpu_conversion,
            )?;
            self.diagnostics
                .record("decode_color_conversion_cpu", stage_started.elapsed());
            frames.push(frame);
        }
        Ok(frames)
    }
}

fn activate_encoder(
    activation: &IMFActivate,
    config: H264Config,
    name: String,
    kind: CodecBackendKind,
) -> Result<MfH264Encoder, MediaError> {
    // SAFETY: the activation object was returned by MFTEnumEx for this media type.
    let transform: IMFTransform = unsafe { activation.ActivateObject() }
        .map_err(|error| mf_error("activate H.264 encoder", error))?;
    let asynchronous = unlock_if_asynchronous(&transform)?;
    let codec_api: ICodecAPI = transform
        .cast()
        .map_err(|error| mf_error("query encoder ICodecAPI", error))?;
    configure_encoder_codec(&codec_api, config);
    let output_type = video_type(MFVideoFormat_H264, config, Some(config.bitrate))?;
    let input_type = video_type(MFVideoFormat_NV12, config, None)?;
    // SAFETY: media types are fully initialized and stream zero is the single video stream.
    unsafe {
        transform
            .SetOutputType(0, &output_type, 0)
            .map_err(|error| mf_error("set H.264 encoder output type", error))?;
        transform
            .SetInputType(0, &input_type, 0)
            .map_err(|error| mf_error("set H.264 encoder input type", error))?;
    }
    let runner =
        TransformRunner::start(transform, asynchronous, encoded_buffer_size(config)?, None)?;
    Ok(MfH264Encoder {
        runner,
        codec_api,
        descriptor: CodecDescriptor {
            name,
            kind,
            asynchronous,
        },
        config,
        frame_index: 0,
        source_timeline: media_core::source_timing::SourceTimeline::default(),
        output_has_timestamp: true,
        keyframe_pending: None,
        parameter_sets: ParameterSetCache::default(),
        diagnostics: media_core::diagnostics::StageMetrics::default(),
    })
}

fn activate_decoder(
    activation: &IMFActivate,
    config: H264Config,
    name: String,
) -> Result<MfH264Decoder, MediaError> {
    // SAFETY: the activation object was returned by MFTEnumEx for H.264 input.
    let transform: IMFTransform = unsafe { activation.ActivateObject() }
        .map_err(|error| mf_error("activate H.264 decoder", error))?;
    let asynchronous = unlock_if_asynchronous(&transform)?;
    if let Ok(attributes) = unsafe { transform.GetAttributes() } {
        // SAFETY: setting a documented UINT32 transform attribute before streaming.
        let _ = unsafe { attributes.SetUINT32(ptr::from_ref(&CODECAPI_AVLowLatencyMode), 1) };
    }
    if let Ok(codec_api) = transform.cast::<ICodecAPI>() {
        let _ = set_codec_u32(&codec_api, &CODECAPI_AVLowLatencyMode, 1);
    }
    let input_type = video_type(MFVideoFormat_H264, config, None)?;
    let output_type = video_type(MFVideoFormat_NV12, config, None)?;
    // SAFETY: media types are fully initialized and stream zero is the single video stream.
    unsafe {
        transform
            .SetInputType(0, &input_type, 0)
            .map_err(|error| mf_error("set H.264 decoder input type", error))?;
        transform
            .SetOutputType(0, &output_type, 0)
            .map_err(|error| mf_error("set H.264 decoder output type", error))?;
    }
    // Negotiate first, then reassert the decoder attribute and record its actual
    // state. Microsoft H.264 uses UINT32/VT_UI4 here (unlike the encoder).
    // SAFETY: these are documented transform attributes set before streaming.
    if let Ok(attributes) = unsafe { transform.GetAttributes() } {
        let latency_result =
            unsafe { attributes.SetUINT32(ptr::from_ref(&CODECAPI_AVLowLatencyMode), 1) };
        if let Err(error) = &latency_result {
            warn!(%error, "decoder low-latency attribute rejected");
        }
        let requested_threads = std::env::var("CLASSROOM_MF_DECODER_THREADS")
            .ok()
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|value| (1..=16).contains(value));
        if let Some(threads) = requested_threads {
            if let Err(error) = unsafe {
                attributes.SetUINT32(ptr::from_ref(&CODECAPI_AVDecNumWorkerThreads), threads)
            } {
                warn!(%error, threads, "decoder test worker-thread override rejected");
            }
        }
        let low_latency =
            unsafe { attributes.GetUINT32(ptr::from_ref(&CODECAPI_AVLowLatencyMode)) }.ok();
        let worker_threads =
            unsafe { attributes.GetUINT32(ptr::from_ref(&CODECAPI_AVDecNumWorkerThreads)) }.ok();
        info!(
            ?low_latency,
            ?worker_threads,
            ?requested_threads,
            "Media Foundation decoder runtime configuration"
        );
    }
    let runner = TransformRunner::start(transform, asynchronous, nv12_size(config)?, Some(config))?;
    configure_decoder_codec_latency(&runner.transform);
    Ok(MfH264Decoder {
        runner,
        descriptor: CodecDescriptor {
            name,
            kind: CodecBackendKind::Software,
            asynchronous,
        },
        config,
        logged_output_layout: false,
        diagnostics: media_core::diagnostics::StageMetrics::default(),
        cpu_conversion: std::env::var_os("CLASSROOM_CPU_CONVERSION").is_some(),
        input_timeline: media_core::source_timing::SourceTimeline::default(),
    })
}

struct TransformRunner {
    transform: IMFTransform,
    events: Option<IMFMediaEventGenerator>,
    pending_inputs: u32,
    pending_outputs: u32,
    fallback_output_size: u32,
    decoder_config: Option<H264Config>,
}

impl TransformRunner {
    fn start(
        transform: IMFTransform,
        asynchronous: bool,
        fallback_output_size: u32,
        decoder_config: Option<H264Config>,
    ) -> Result<Self, MediaError> {
        let events = if asynchronous {
            Some(
                transform
                    .cast()
                    .map_err(|error| mf_error("query asynchronous MFT event generator", error))?,
            )
        } else {
            None
        };
        // SAFETY: messages are sent after type negotiation and before sample processing.
        unsafe {
            transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)
                .map_err(|error| mf_error("begin MFT streaming", error))?;
            transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)
                .map_err(|error| mf_error("start MFT stream", error))?;
        }
        Ok(Self {
            transform,
            events,
            pending_inputs: 0,
            pending_outputs: 0,
            fallback_output_size,
            decoder_config,
        })
    }

    fn submit(&mut self, sample: &IMFSample) -> Result<(), MediaError> {
        if self.events.is_some() {
            self.wait_for_input(Duration::from_secs(2))?;
            self.pending_inputs = self.pending_inputs.saturating_sub(1);
        }
        // SAFETY: the sample remains owned by this call and stream zero is configured.
        unsafe { self.transform.ProcessInput(0, sample, 0) }
            .map_err(|error| mf_error("submit MFT input sample", error))
    }

    fn take_output(&mut self, timeout: Duration) -> Result<Option<IMFSample>, MediaError> {
        if self.events.is_some() {
            self.wait_for_output(timeout)?;
            self.pending_outputs = self.pending_outputs.saturating_sub(1);
        }
        for _ in 0..3 {
            match self.process_output_once() {
                Ok(OutputAttempt::StreamChange) => {
                    self.renegotiate_decoder_output()?;
                }
                Ok(OutputAttempt::Sample(sample)) => return Ok(sample),
                Err(error) => return Err(error),
            }
        }
        Err(MediaError::Operation(
            "decoder output format changed repeatedly".to_owned(),
        ))
    }

    fn process_output_once(&self) -> Result<OutputAttempt, MediaError> {
        let stream_info = unsafe { self.transform.GetOutputStreamInfo(0) }
            .map_err(|error| mf_error("query MFT output stream", error))?;
        let provides_samples = stream_info.dwFlags
            & u32::try_from(MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0).unwrap_or_default()
            != 0;
        let can_provide_samples = stream_info.dwFlags
            & u32::try_from(MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES.0).unwrap_or_default()
            != 0;
        let supplied_sample = if provides_samples || can_provide_samples {
            None
        } else {
            Some(empty_sample(
                stream_info.cbSize.max(self.fallback_output_size),
                stream_info.cbAlignment,
            )?)
        };
        let mut output = MFT_OUTPUT_DATA_BUFFER {
            dwStreamID: 0,
            pSample: ManuallyDrop::new(supplied_sample),
            dwStatus: 0,
            pEvents: ManuallyDrop::new(None),
        };
        let mut status = 0_u32;
        // SAFETY: output owns valid optional COM objects for the duration of ProcessOutput.
        let result = unsafe {
            self.transform.ProcessOutput(
                0,
                std::slice::from_mut(&mut output),
                ptr::addr_of_mut!(status),
            )
        };
        // SAFETY: the ManuallyDrop fields are taken exactly once after ProcessOutput returns.
        let sample = unsafe { ManuallyDrop::take(&mut output.pSample) };
        // SAFETY: same ownership rule as pSample; dropping releases an optional event collection.
        drop(unsafe { ManuallyDrop::take(&mut output.pEvents) });
        match result {
            Ok(()) => Ok(OutputAttempt::Sample(sample)),
            Err(error) if error.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => {
                Ok(OutputAttempt::Sample(None))
            }
            Err(error) if error.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                Ok(OutputAttempt::StreamChange)
            }
            Err(error) => Err(mf_error("read MFT output sample", error)),
        }
    }

    fn renegotiate_decoder_output(&self) -> Result<(), MediaError> {
        let config = self.decoder_config.ok_or_else(|| {
            MediaError::Operation(
                "encoder unexpectedly requested an output format change".to_owned(),
            )
        })?;
        for index in 0..32 {
            let media_type = match unsafe { self.transform.GetOutputAvailableType(0, index) } {
                Ok(value) => value,
                Err(error) if error.code() == MF_E_NO_MORE_TYPES => break,
                Err(error) => return Err(mf_error("enumerate decoder output formats", error)),
            };
            let subtype = unsafe { media_type.GetGUID(ptr::from_ref(&MF_MT_SUBTYPE)) }
                .map_err(|error| mf_error("read decoder output subtype", error))?;
            if subtype != MFVideoFormat_NV12 {
                continue;
            }
            decoder_storage_dimensions(&media_type, config)?;
            unsafe { self.transform.SetOutputType(0, &media_type, 0) }
                .map_err(|error| mf_error("renegotiate NV12 decoder output", error))?;
            configure_decoder_codec_latency(&self.transform);
            let stride = unsafe { media_type.GetUINT32(ptr::from_ref(&MF_MT_DEFAULT_STRIDE)) }
                .unwrap_or_default();
            info!(
                width = config.width,
                height = config.height,
                stride,
                "renegotiated NV12 decoder output"
            );
            return Ok(());
        }
        Err(MediaError::Operation(
            "decoder offered no NV12 output format".to_owned(),
        ))
    }

    fn wait_for_input(&mut self, timeout: Duration) -> Result<(), MediaError> {
        let deadline = Instant::now() + timeout;
        while self.pending_inputs == 0 {
            self.pump_event(deadline)?;
        }
        Ok(())
    }

    fn wait_for_output(&mut self, timeout: Duration) -> Result<(), MediaError> {
        let deadline = Instant::now() + timeout;
        while self.pending_outputs == 0 {
            self.pump_event(deadline)?;
        }
        Ok(())
    }

    fn pump_event(&mut self, deadline: Instant) -> Result<(), MediaError> {
        let events = self.events.as_ref().ok_or_else(|| {
            MediaError::Operation("synchronous transform has no event generator".to_owned())
        })?;
        loop {
            // SAFETY: non-blocking event polling retains the returned COM event until inspection.
            match unsafe { events.GetEvent(MF_EVENT_FLAG_NO_WAIT) } {
                Ok(event) => {
                    let status = unsafe { event.GetStatus() }
                        .map_err(|error| mf_error("read asynchronous MFT event status", error))?;
                    status
                        .ok()
                        .map_err(|error| mf_error("asynchronous MFT event failed", error))?;
                    let event_type = unsafe { event.GetType() }
                        .map_err(|error| mf_error("read asynchronous MFT event type", error))?;
                    if event_type == u32::try_from(METransformNeedInput.0).unwrap_or_default() {
                        self.pending_inputs += 1;
                    } else if event_type
                        == u32::try_from(METransformHaveOutput.0).unwrap_or_default()
                    {
                        self.pending_outputs += 1;
                    }
                    return Ok(());
                }
                Err(error) if error.code() == MF_E_NO_EVENTS_AVAILABLE => {
                    if Instant::now() >= deadline {
                        return Err(MediaError::Operation(
                            "asynchronous Media Foundation transform timed out".to_owned(),
                        ));
                    }
                    thread::sleep(Duration::from_millis(1));
                }
                Err(error) => return Err(mf_error("poll asynchronous MFT event", error)),
            }
        }
    }
}

enum OutputAttempt {
    Sample(Option<IMFSample>),
    StreamChange,
}

impl Drop for TransformRunner {
    fn drop(&mut self) {
        if let Ok(shutdown) = self.transform.cast::<IMFShutdown>() {
            // SAFETY: shutdown is idempotent for an initialized asynchronous MFT.
            let _ = unsafe { shutdown.Shutdown() };
        }
    }
}

fn unlock_if_asynchronous(transform: &IMFTransform) -> Result<bool, MediaError> {
    // SAFETY: transform attributes are queried and changed before type negotiation/streaming.
    let attributes = unsafe { transform.GetAttributes() }
        .map_err(|error| mf_error("query MFT attributes", error))?;
    let asynchronous = unsafe { attributes.GetUINT32(ptr::from_ref(&MF_TRANSFORM_ASYNC)) }
        .unwrap_or_default()
        != 0;
    if asynchronous {
        unsafe {
            attributes
                .SetUINT32(ptr::from_ref(&MF_TRANSFORM_ASYNC_UNLOCK), 1)
                .map_err(|error| mf_error("unlock asynchronous MFT", error))?;
        }
    }
    Ok(asynchronous)
}

fn configure_encoder_codec(codec: &ICodecAPI, config: H264Config) {
    // H.264 uses VT_BOOL for AVLowLatencyMode, unlike its decoder's VT_UI4.
    // https://learn.microsoft.com/en-us/windows/win32/medfound/codecapi-avlowlatencymode
    match set_codec_bool(codec, &CODECAPI_AVLowLatencyMode, true) {
        Ok(()) => info!(
            property = "CODECAPI_AVLowLatencyMode",
            enabled = true,
            "encoder low-latency mode accepted"
        ),
        Err(error) => {
            warn!(%error, "H.264 low-latency mode rejected; trying generic property");
            try_codec_bool(
                codec,
                &CODECAPI_AVEncCommonLowLatency,
                true,
                "generic low-latency",
            );
        }
    }
    try_codec_bool(codec, &CODECAPI_AVEncCommonRealTime, true, "real-time");
    try_codec_u32(
        codec,
        &CODECAPI_AVEncMPVDefaultBPictureCount,
        0,
        "B-picture count",
    );
    try_codec_u32(
        codec,
        &CODECAPI_AVEncCommonRateControlMode,
        u32::try_from(eAVEncCommonRateControlMode_CBR.0).unwrap_or_default(),
        "rate-control mode",
    );
    try_codec_u32(
        codec,
        &CODECAPI_AVEncCommonMeanBitRate,
        config.bitrate,
        "mean bitrate",
    );
    try_codec_u32(
        codec,
        &CODECAPI_AVEncVideoMaxKeyframeDistance,
        config.keyframe_interval_frames,
        "keyframe distance",
    );
}

fn configure_decoder_codec_latency(transform: &IMFTransform) {
    if let Ok(codec) = transform.cast::<ICodecAPI>() {
        // SAFETY: the GUID identifies a documented decoder property; returned
        // variants own their storage. Reapply after output-format negotiation.
        let before = unsafe { codec.GetValue(ptr::from_ref(&CODECAPI_AVLowLatencyMode)) }
            .ok()
            .as_ref()
            .and_then(variant_scalar);
        let result = set_codec_u32(&codec, &CODECAPI_AVLowLatencyMode, 1);
        if let Err(error) = &result {
            warn!(%error, "decoder ICodecAPI low-latency setting rejected");
        }
        let after = unsafe { codec.GetValue(ptr::from_ref(&CODECAPI_AVLowLatencyMode)) }
            .ok()
            .as_ref()
            .and_then(variant_scalar);
        info!(
            ?before,
            ?after,
            accepted = result.is_ok(),
            "decoder ICodecAPI low-latency state"
        );
    }
}

fn variant_scalar(value: &VARIANT) -> Option<(u16, i64)> {
    // SAFETY: read only the union member identified by the variant's type tag.
    unsafe {
        let inner = &value.Anonymous.Anonymous;
        match inner.vt {
            VT_UI4 => Some((VT_UI4.0, i64::from(inner.Anonymous.ulVal))),
            VT_BOOL => Some((VT_BOOL.0, i64::from(inner.Anonymous.boolVal.0))),
            _ => None,
        }
    }
}

fn try_codec_u32(codec: &ICodecAPI, key: &windows::core::GUID, value: u32, property: &str) {
    if let Err(error) = set_codec_u32(codec, key, value) {
        warn!(%error, property, "Media Foundation codec property was not accepted; continuing with MFT defaults");
    }
}

fn try_codec_bool(codec: &ICodecAPI, key: &windows::core::GUID, value: bool, property: &str) {
    if let Err(error) = set_codec_bool(codec, key, value) {
        warn!(%error, property, "Media Foundation codec property was not accepted; continuing with MFT defaults");
    }
}

fn set_codec_u32(
    codec: &ICodecAPI,
    key: &windows::core::GUID,
    value: u32,
) -> Result<(), MediaError> {
    let variant = variant_u32(value);
    // SAFETY: the variant is initialized as VT_UI4 and lives for the call duration.
    unsafe { codec.SetValue(ptr::from_ref(key), ptr::from_ref(&variant)) }
        .map_err(|error| mf_error("set codec UINT32 property", error))
}

fn set_codec_bool(
    codec: &ICodecAPI,
    key: &windows::core::GUID,
    value: bool,
) -> Result<(), MediaError> {
    let variant = variant_bool(value);
    // SAFETY: the variant is initialized as VT_BOOL and lives for the call duration.
    unsafe { codec.SetValue(ptr::from_ref(key), ptr::from_ref(&variant)) }
        .map_err(|error| mf_error("set codec BOOL property", error))
}

fn variant_u32(value: u32) -> VARIANT {
    VARIANT {
        Anonymous: windows::Win32::System::Variant::VARIANT_0 {
            Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                vt: VARENUM(VT_UI4.0),
                Anonymous: VARIANT_0_0_0 { ulVal: value },
                ..Default::default()
            }),
        },
    }
}

fn variant_bool(value: bool) -> VARIANT {
    VARIANT {
        Anonymous: windows::Win32::System::Variant::VARIANT_0 {
            Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                vt: VARENUM(VT_BOOL.0),
                Anonymous: VARIANT_0_0_0 {
                    boolVal: VARIANT_BOOL(if value { -1 } else { 0 }),
                },
                ..Default::default()
            }),
        },
    }
}

fn video_type(
    subtype: windows::core::GUID,
    config: H264Config,
    bitrate: Option<u32>,
) -> Result<windows::Win32::Media::MediaFoundation::IMFMediaType, MediaError> {
    // SAFETY: a new empty media type is populated with documented video attributes.
    unsafe {
        let media_type = MFCreateMediaType()
            .map_err(|error| mf_error("create Media Foundation media type", error))?;
        media_type
            .SetGUID(
                ptr::from_ref(&MF_MT_MAJOR_TYPE),
                ptr::from_ref(&MFMediaType_Video),
            )
            .map_err(|error| mf_error("set media major type", error))?;
        media_type
            .SetGUID(ptr::from_ref(&MF_MT_SUBTYPE), ptr::from_ref(&subtype))
            .map_err(|error| mf_error("set media subtype", error))?;
        media_type
            .SetUINT64(
                ptr::from_ref(&MF_MT_FRAME_SIZE),
                pack_ratio(config.width, config.height),
            )
            .map_err(|error| mf_error("set media frame size", error))?;
        media_type
            .SetUINT64(
                ptr::from_ref(&MF_MT_FRAME_RATE),
                pack_ratio(config.frames_per_second, 1),
            )
            .map_err(|error| mf_error("set media frame rate", error))?;
        media_type
            .SetUINT64(ptr::from_ref(&MF_MT_PIXEL_ASPECT_RATIO), pack_ratio(1, 1))
            .map_err(|error| mf_error("set media pixel aspect ratio", error))?;
        media_type
            .SetUINT32(
                ptr::from_ref(&MF_MT_INTERLACE_MODE),
                u32::try_from(MFVideoInterlace_Progressive.0).unwrap_or_default(),
            )
            .map_err(|error| mf_error("set progressive video mode", error))?;
        if subtype == MFVideoFormat_H264 {
            media_type
                .SetUINT32(
                    ptr::from_ref(&MF_MT_MPEG2_PROFILE),
                    u32::try_from(eAVEncH264VProfile_Main.0).unwrap_or_default(),
                )
                .map_err(|error| mf_error("set H.264 profile", error))?;
        }
        if let Some(bitrate) = bitrate {
            media_type
                .SetUINT32(ptr::from_ref(&MF_MT_AVG_BITRATE), bitrate)
                .map_err(|error| mf_error("set media average bitrate", error))?;
        }
        let pixel_count = config
            .width
            .checked_mul(config.height)
            .ok_or_else(|| MediaError::Operation("media sample size overflowed".to_owned()))?;
        if subtype == MFVideoFormat_NV12 {
            let sample_size = pixel_count
                .checked_add(pixel_count / 2)
                .ok_or_else(|| MediaError::Operation("NV12 sample size overflowed".to_owned()))?;
            media_type
                .SetUINT32(ptr::from_ref(&MF_MT_SAMPLE_SIZE), sample_size)
                .map_err(|error| mf_error("set media sample size", error))?;
        }
        media_type
            .SetUINT32(ptr::from_ref(&MF_MT_FIXED_SIZE_SAMPLES), 1)
            .map_err(|error| mf_error("set fixed sample attribute", error))?;
        media_type
            .SetUINT32(ptr::from_ref(&MF_MT_ALL_SAMPLES_INDEPENDENT), 1)
            .map_err(|error| mf_error("set independent sample attribute", error))?;
        Ok(media_type)
    }
}

fn enumerate_transforms(
    category: windows::core::GUID,
    flags: MFT_ENUM_FLAG,
    input: Option<&MFT_REGISTER_TYPE_INFO>,
    output: Option<&MFT_REGISTER_TYPE_INFO>,
) -> Result<Vec<IMFActivate>, MediaError> {
    let mut raw: *mut Option<IMFActivate> = ptr::null_mut();
    let mut count = 0_u32;
    // SAFETY: MFTEnumEx initializes a CoTaskMem array of count optional activation objects.
    unsafe {
        MFTEnumEx(
            category,
            flags,
            input.map(ptr::from_ref),
            output.map(ptr::from_ref),
            ptr::addr_of_mut!(raw),
            ptr::addr_of_mut!(count),
        )
        .map_err(|error| mf_error("enumerate Media Foundation transforms", error))?;
    }
    let mut activations = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
    for index in 0..count {
        // SAFETY: index is within the array size returned by MFTEnumEx. Reading transfers the
        // COM reference out of the plain C allocation before that allocation is freed.
        if let Some(activation) = unsafe { ptr::read(raw.add(usize::try_from(index).unwrap_or(0))) }
        {
            activations.push(activation);
        }
    }
    // SAFETY: raw is either null or the exact allocation returned by MFTEnumEx.
    unsafe { CoTaskMemFree((!raw.is_null()).then_some(raw.cast())) };
    Ok(activations)
}

fn activation_name(activation: &IMFActivate) -> Result<String, MediaError> {
    let mut value = PWSTR::null();
    let mut length = 0_u32;
    // SAFETY: Media Foundation allocates the returned string with CoTaskMem.
    unsafe {
        activation
            .GetAllocatedString(
                ptr::from_ref(&MFT_FRIENDLY_NAME_Attribute),
                ptr::addr_of_mut!(value),
                ptr::addr_of_mut!(length),
            )
            .map_err(|error| mf_error("read MFT friendly name", error))?;
    }
    let name = if value.is_null() {
        "unnamed Media Foundation transform".to_owned()
    } else {
        // SAFETY: the allocation contains length UTF-16 code units as reported by the API.
        String::from_utf16_lossy(unsafe {
            std::slice::from_raw_parts(value.0, usize::try_from(length).unwrap_or(0))
        })
    };
    // SAFETY: value is null or the exact allocation returned by GetAllocatedString.
    unsafe { CoTaskMemFree((!value.is_null()).then_some(value.0.cast())) };
    Ok(name)
}

fn sample_from_bytes(
    bytes: &[u8],
    timestamp_hns: i64,
    duration_hns: i64,
) -> Result<IMFSample, MediaError> {
    let length = u32::try_from(bytes.len())
        .map_err(|_| MediaError::Operation("media sample exceeds 4 GiB".to_owned()))?;
    // SAFETY: Media Foundation creates owning COM buffers and sample objects.
    unsafe {
        let buffer = MFCreateMemoryBuffer(length)
            .map_err(|error| mf_error("create Media Foundation input buffer", error))?;
        write_buffer(&buffer, bytes)?;
        let sample = MFCreateSample()
            .map_err(|error| mf_error("create Media Foundation input sample", error))?;
        sample
            .AddBuffer(&buffer)
            .map_err(|error| mf_error("attach Media Foundation input buffer", error))?;
        sample
            .SetSampleTime(timestamp_hns)
            .map_err(|error| mf_error("set media sample timestamp", error))?;
        sample
            .SetSampleDuration(duration_hns)
            .map_err(|error| mf_error("set media sample duration", error))?;
        Ok(sample)
    }
}

fn empty_sample(length: u32, alignment: u32) -> Result<IMFSample, MediaError> {
    // SAFETY: Media Foundation creates owning COM buffers and sample objects.
    unsafe {
        let buffer = if alignment > 0 {
            MFCreateAlignedMemoryBuffer(length, alignment - 1)
        } else {
            MFCreateMemoryBuffer(length)
        }
        .map_err(|error| mf_error("create Media Foundation output buffer", error))?;
        let sample = MFCreateSample()
            .map_err(|error| mf_error("create Media Foundation output sample", error))?;
        sample
            .AddBuffer(&buffer)
            .map_err(|error| mf_error("attach Media Foundation output buffer", error))?;
        Ok(sample)
    }
}

fn write_buffer(buffer: &IMFMediaBuffer, bytes: &[u8]) -> Result<(), MediaError> {
    let mut destination = ptr::null_mut();
    // SAFETY: Lock returns a writable buffer of at least the length used to create it.
    unsafe {
        buffer
            .Lock(ptr::addr_of_mut!(destination), None, None)
            .map_err(|error| mf_error("lock Media Foundation buffer", error))?;
        ptr::copy_nonoverlapping(bytes.as_ptr(), destination, bytes.len());
        let unlock = buffer.Unlock();
        unlock.map_err(|error| mf_error("unlock Media Foundation buffer", error))?;
        buffer
            .SetCurrentLength(u32::try_from(bytes.len()).unwrap_or(u32::MAX))
            .map_err(|error| mf_error("set Media Foundation buffer length", error))?;
    }
    Ok(())
}

fn decoder_sample_bytes(sample: &IMFSample) -> Result<(Vec<u8>, bool), MediaError> {
    // SAFETY: COM interfaces own the sample buffer. ContiguousCopyTo writes only into
    // the allocated destination, and strips the surface pitch padding by contract.
    // ConvertToContiguousBuffer alone joins sample buffers; it does not normalize pitch.
    let buffer = unsafe { sample.ConvertToContiguousBuffer() }
        .map_err(|error| mf_error("get decoder sample buffer", error))?;
    if let Ok(surface) = buffer.cast::<IMF2DBuffer>() {
        let length = unsafe { surface.GetContiguousLength() }
            .map_err(|error| mf_error("get packed decoder buffer length", error))?;
        let mut bytes = vec![
            0;
            usize::try_from(length).map_err(|_| MediaError::Operation(
                "decoder buffer length overflowed".to_owned()
            ))?
        ];
        unsafe { surface.ContiguousCopyTo(&mut bytes) }
            .map_err(|error| mf_error("copy packed decoder surface", error))?;
        Ok((bytes, true))
    } else {
        // CPU MFTs may expose only IMFMediaBuffer; its exact byte length and output
        // media type determine the layout, checked by copied_nv12_stride.
        let (bytes, _, _) = sample_bytes(sample)?;
        Ok((bytes, false))
    }
}

fn sample_bytes(sample: &IMFSample) -> Result<(Vec<u8>, i64, bool), MediaError> {
    // SAFETY: the contiguous COM buffer remains alive and locked while its bytes are copied.
    unsafe {
        let buffer = sample
            .ConvertToContiguousBuffer()
            .map_err(|error| mf_error("make Media Foundation sample contiguous", error))?;
        let length = buffer
            .GetCurrentLength()
            .map_err(|error| mf_error("read Media Foundation sample length", error))?;
        let mut source = ptr::null_mut();
        buffer
            .Lock(ptr::addr_of_mut!(source), None, None)
            .map_err(|error| mf_error("lock Media Foundation output buffer", error))?;
        let bytes =
            std::slice::from_raw_parts(source, usize::try_from(length).unwrap_or(0)).to_vec();
        buffer
            .Unlock()
            .map_err(|error| mf_error("unlock Media Foundation output buffer", error))?;
        let timestamp = sample.GetSampleTime().unwrap_or_default();
        let clean_point = sample
            .GetUINT32(ptr::from_ref(&MFSampleExtension_CleanPoint))
            .unwrap_or_default()
            != 0;
        Ok((bytes, timestamp, clean_point))
    }
}

fn pack_ratio(numerator: u32, denominator: u32) -> u64 {
    (u64::from(numerator) << 32) | u64::from(denominator)
}

fn decoder_storage_dimensions(
    media_type: &IMFMediaType,
    config: H264Config,
) -> Result<(u32, u32), MediaError> {
    // SAFETY: attributes are read from a live COM media type. The aperture buffer is an
    // initialized repr(C) Windows value and stays alive for the duration of GetBlob.
    let size = unsafe { media_type.GetUINT64(ptr::from_ref(&MF_MT_FRAME_SIZE)) }
        .map_err(|error| mf_error("read decoder output dimensions", error))?;
    let width = u32::try_from(size >> 32).unwrap_or_default();
    let height = u32::try_from(size & 0xffff_ffff).unwrap_or_default();
    if (width, height) == (config.width, config.height) {
        return Ok((width, height));
    }
    let mut area = MFVideoArea::default();
    let mut written = 0;
    let bytes = unsafe {
        std::slice::from_raw_parts_mut(
            ptr::addr_of_mut!(area).cast::<u8>(),
            std::mem::size_of::<MFVideoArea>(),
        )
    };
    let has_aperture = unsafe {
        media_type.GetBlob(
            ptr::from_ref(&MF_MT_MINIMUM_DISPLAY_APERTURE),
            bytes,
            Some(ptr::addr_of_mut!(written)),
        )
    }
    .is_ok()
        && usize::try_from(written).ok() == Some(std::mem::size_of::<MFVideoArea>());
    if has_aperture
        && area.OffsetX.value == 0
        && area.OffsetX.fract == 0
        && area.OffsetY.value == 0
        && area.OffsetY.fract == 0
        && u32::try_from(area.Area.cx).ok() == Some(config.width)
        && u32::try_from(area.Area.cy).ok() == Some(config.height)
        && width >= config.width
        && height >= config.height
        && width % 2 == 0
        && height % 2 == 0
    {
        return Ok((width, height));
    }
    Err(MediaError::Operation(format!(
        "decoder output {width}x{height} has no supported display aperture for {}x{}",
        config.width, config.height,
    )))
}

fn validate_config(config: H264Config) -> Result<(), MediaError> {
    if config.width == 0
        || config.height == 0
        || config.width % 2 != 0
        || config.height % 2 != 0
        || config.frames_per_second == 0
        || config.bitrate == 0
    {
        return Err(MediaError::Operation(
            "H.264 configuration requires non-zero even dimensions, FPS, and bitrate".to_owned(),
        ));
    }
    Ok(())
}

fn nv12_size(config: H264Config) -> Result<u32, MediaError> {
    config
        .width
        .checked_mul(config.height)
        .and_then(|pixels| pixels.checked_add(pixels / 2))
        .ok_or_else(|| MediaError::Operation("NV12 frame size overflowed".to_owned()))
}

fn encoded_buffer_size(config: H264Config) -> Result<u32, MediaError> {
    nv12_size(config)
}

fn frame_duration_hns(frames_per_second: u32) -> i64 {
    10_000_000_i64 / i64::from(frames_per_second)
}

fn timestamp_hns(frame_index: u64, frames_per_second: u32) -> i64 {
    let value = u128::from(frame_index) * 10_000_000 / u128::from(frames_per_second);
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn duration_hns(duration: Duration) -> i64 {
    i64::try_from(duration.as_nanos() / 100).unwrap_or(i64::MAX)
}

fn hns_duration(value: i64) -> Duration {
    Duration::from_nanos(u64::try_from(value).unwrap_or_default().saturating_mul(100))
}

#[allow(clippy::needless_pass_by_value)]
fn mf_error(context: &str, error: WindowsError) -> MediaError {
    MediaError::Operation(format!("{context}: {error}"))
}
