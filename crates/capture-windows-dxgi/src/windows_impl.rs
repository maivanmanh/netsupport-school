use std::{
    cmp::Ordering,
    mem::size_of,
    time::{Duration, Instant},
};

use media_core::{CaptureConfig, CaptureSource, CaptureStatistics, MediaError, VideoFrame};
use thiserror::Error;
use tracing::{info, warn};
use windows::{
    core::{Error as WindowsError, Interface},
    Win32::{
        Foundation::{E_ACCESSDENIED, HMODULE, RECT},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_UNKNOWN,
            Direct3D11::{
                D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
                D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE,
                D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
            },
            Dxgi::{
                CreateDXGIFactory1, IDXGIAdapter, IDXGIAdapter1, IDXGIFactory1, IDXGIOutput1,
                IDXGIOutputDuplication, IDXGIResource, DXGI_ERROR_ACCESS_LOST,
                DXGI_ERROR_NOT_FOUND, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO,
                DXGI_OUTDUPL_MOVE_RECT, DXGI_OUTDUPL_POINTER_SHAPE_INFO,
                DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR,
                DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR,
                DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME, DXGI_OUTPUT_DESC,
            },
        },
    },
};

use crate::cursor::{compose_cursor, PointerShape, PointerShapeKind};
use crate::scale::{fit_even, resize_bgra};

#[derive(Default)]
pub struct DxgiCapture {
    cursor_policy: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    config: Option<CaptureConfig>,
    session: Option<DxgiSession>,
    statistics: CaptureStatistics,
    output_dimensions: Option<(u32, u32)>,
    recovery_started: Option<Instant>,
    recovery_retry_at: Option<Instant>,
    recovery_attempts: u32,
}

impl DxgiCapture {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn initialize_session(config: CaptureConfig) -> Result<DxgiSession, MediaError> {
        DxgiSession::new(config.display_index).map_err(CaptureAttemptError::into_media_error)
    }

    fn recover_access_loss(&mut self) -> Result<(), MediaError> {
        let config = self
            .config
            .ok_or_else(|| MediaError::Operation("capture was not started".to_owned()))?;
        let now = Instant::now();
        if self.recovery_retry_at.is_some_and(|retry| now < retry) {
            return Ok(());
        }
        let session = match DxgiSession::new(config.display_index) {
            Ok(session) => session,
            Err(error) if error.retryable_during_recovery() => {
                self.recovery_attempts = self.recovery_attempts.saturating_add(1);
                // At most one attempt per 100 ms, rising to one per second.
                // No sleeps or queues: control and share-duration checks keep running.
                let delay_ms =
                    (100_u64 << self.recovery_attempts.saturating_sub(1).min(4)).min(1000);
                self.recovery_retry_at = Some(now + Duration::from_millis(delay_ms));
                if self.recovery_attempts == 1 || self.recovery_attempts % 10 == 0 {
                    warn!(%error, attempts=self.recovery_attempts, retry_ms=delay_ms,
                        "DXGI desktop temporarily unavailable; capture recovery will retry");
                }
                return Ok(());
            }
            Err(error) => return Err(error.into_media_error()),
        };
        let dimensions = fit_even(
            (session.width, session.height),
            (config.width, config.height),
        )?;
        if Some(dimensions) != self.output_dimensions {
            return Err(MediaError::Operation(format!(
                "display resolution changed; restart presentation to negotiate {dimensions:?}"
            )));
        }
        self.session = Some(session);
        self.statistics.access_loss_recoveries += 1;
        warn!(
            recoveries = self.statistics.access_loss_recoveries,
            attempts = self.recovery_attempts,
            recovery_ms = self
                .recovery_started
                .map(|start| start.elapsed().as_millis()),
            "DXGI duplication recovered after access loss"
        );
        self.recovery_started = None;
        self.recovery_retry_at = None;
        self.recovery_attempts = 0;
        Ok(())
    }
}

impl CaptureSource for DxgiCapture {
    fn set_cursor_policy(&mut self, visible: std::sync::Arc<std::sync::atomic::AtomicBool>) {
        self.cursor_policy = Some(visible);
    }
    fn start(&mut self, config: CaptureConfig) -> Result<(), MediaError> {
        let session = Self::initialize_session(config)?;
        let dimensions = fit_even(
            (session.width, session.height),
            (config.width, config.height),
        )?;
        info!(
            display_index = config.display_index,
            output = %session.output_name,
            adapter = %session.adapter_name,
            source_width = session.width,
            source_height = session.height,
            width = dimensions.0,
            height = dimensions.1,
            "DXGI Desktop Duplication initialized"
        );
        self.config = Some(config);
        self.output_dimensions = Some(dimensions);
        self.session = Some(session);
        self.statistics = CaptureStatistics::default();
        self.recovery_started = None;
        self.recovery_retry_at = None;
        self.recovery_attempts = 0;
        Ok(())
    }

    fn output_dimensions(&self) -> Option<(u32, u32)> {
        self.output_dimensions
    }

    fn next_frame(&mut self, deadline: Instant) -> Result<Option<VideoFrame>, MediaError> {
        if self.session.is_none() && self.recovery_started.is_some() {
            self.recover_access_loss()?;
            return Ok(None);
        }
        let include_cursor = self
            .cursor_policy
            .as_ref()
            .is_none_or(|v| v.load(std::sync::atomic::Ordering::Acquire));
        let result = self
            .session
            .as_mut()
            .ok_or_else(|| MediaError::Operation("capture was not started".to_owned()))?
            .capture(deadline, include_cursor);
        match result {
            Ok(acquired) => {
                let mut frame = acquired.frame;
                let (width, height) = self.output_dimensions.ok_or_else(|| {
                    MediaError::Operation("capture output dimensions unavailable".to_owned())
                })?;
                if (frame.width, frame.height) != (width, height) {
                    frame.bytes =
                        resize_bgra(&frame.bytes, frame.width, frame.height, width, height)?;
                    frame.width = width;
                    frame.height = height;
                    frame.row_pitch = width * 4;
                    frame.capture_latency = frame
                        .capture_latency
                        .saturating_add(frame.captured_at.elapsed());
                }
                self.statistics.frames_acquired += 1;
                self.statistics.accumulated_frames += u64::from(frame.accumulated_frames);
                if acquired.pointer_updated {
                    self.statistics.pointer_updates += 1;
                }
                if frame.pointer_composited {
                    self.statistics.pointer_frames_composited += 1;
                }
                if frame.protected_content_masked_out {
                    self.statistics.protected_content_frames += 1;
                }
                Ok(Some(frame))
            }
            Err(CaptureAttemptError::Timeout) => {
                self.statistics.no_change_timeouts += 1;
                Ok(None)
            }
            Err(CaptureAttemptError::AccessLost) => {
                self.session = None;
                self.recovery_started = Some(Instant::now());
                self.recovery_retry_at = None;
                self.recovery_attempts = 0;
                self.recover_access_loss()?;
                Ok(None)
            }
            Err(error) => Err(error.into_media_error()),
        }
    }

    fn statistics(&self) -> CaptureStatistics {
        self.statistics
    }
}

#[derive(Debug, Error)]
enum CaptureAttemptError {
    #[error("no desktop update arrived before the capture deadline")]
    Timeout,
    #[error("DXGI duplication access was lost")]
    AccessLost,
    #[error("{context}: {source}")]
    Windows {
        context: &'static str,
        source: WindowsError,
    },
    #[error("{0}")]
    InvalidData(String),
}

impl CaptureAttemptError {
    fn retryable_during_recovery(&self) -> bool {
        matches!(self, Self::AccessLost)
            || matches!(self, Self::Windows { context: "duplicate selected desktop output", source }
                if source.code() == E_ACCESSDENIED)
    }
    fn from_windows(context: &'static str, source: WindowsError) -> Self {
        if source.code() == DXGI_ERROR_WAIT_TIMEOUT {
            Self::Timeout
        } else if source.code() == DXGI_ERROR_ACCESS_LOST {
            Self::AccessLost
        } else {
            Self::Windows { context, source }
        }
    }

    fn into_media_error(self) -> MediaError {
        match self {
            Self::Timeout => MediaError::Operation("capture deadline elapsed".to_owned()),
            Self::AccessLost => {
                MediaError::Operation("DXGI duplication access was lost".to_owned())
            }
            Self::Windows { context, source } => {
                MediaError::Operation(format!("{context}: {source}"))
            }
            Self::InvalidData(message) => MediaError::Operation(message),
        }
    }
}

struct DxgiSession {
    _device: ID3D11Device,
    context: ID3D11DeviceContext,
    duplication: IDXGIOutputDuplication,
    staging: ID3D11Texture2D,
    pointer: PointerState,
    width: u32,
    height: u32,
    output_left: i32,
    output_top: i32,
    output_name: String,
    adapter_name: String,
}

struct AcquiredFrame {
    frame: VideoFrame,
    pointer_updated: bool,
}

impl DxgiSession {
    fn new(display_index: u32) -> Result<Self, CaptureAttemptError> {
        // SAFETY: COM interfaces are created through Windows factory functions and retained by
        // owning windows-rs wrappers. Every returned pointer is checked before use.
        unsafe {
            let factory: IDXGIFactory1 = CreateDXGIFactory1()
                .map_err(|error| CaptureAttemptError::from_windows("create DXGI factory", error))?;
            let mut outputs = enumerate_outputs(&factory)?;
            outputs.sort_by(compare_outputs);
            let output = outputs
                .into_iter()
                .nth(usize::try_from(display_index).unwrap_or(usize::MAX))
                .ok_or_else(|| {
                    CaptureAttemptError::InvalidData(format!(
                        "display index {display_index} does not identify an attached desktop output"
                    ))
                })?;

            let adapter_base: IDXGIAdapter = output.adapter.cast().map_err(|error| {
                CaptureAttemptError::from_windows("cast selected DXGI adapter", error)
            })?;
            let mut device = None;
            let mut context = None;
            D3D11CreateDevice(
                &adapter_base,
                D3D_DRIVER_TYPE_UNKNOWN,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(std::ptr::addr_of_mut!(device)),
                None,
                Some(std::ptr::addr_of_mut!(context)),
            )
            .map_err(|error| CaptureAttemptError::from_windows("create D3D11 device", error))?;
            let device = device.ok_or_else(|| {
                CaptureAttemptError::InvalidData("D3D11 returned no device".to_owned())
            })?;
            let context = context.ok_or_else(|| {
                CaptureAttemptError::InvalidData("D3D11 returned no immediate context".to_owned())
            })?;
            let duplication = output.output.DuplicateOutput(&device).map_err(|error| {
                CaptureAttemptError::from_windows("duplicate selected desktop output", error)
            })?;
            let duplication_desc = duplication.GetDesc();
            let width = duplication_desc.ModeDesc.Width;
            let height = duplication_desc.ModeDesc.Height;
            if width == 0 || height == 0 {
                return Err(CaptureAttemptError::InvalidData(
                    "DXGI reported a zero-sized desktop output".to_owned(),
                ));
            }
            let texture_desc = D3D11_TEXTURE2D_DESC {
                Width: width,
                Height: height,
                MipLevels: 1,
                ArraySize: 1,
                Format: duplication_desc.ModeDesc.Format,
                SampleDesc: windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_STAGING,
                BindFlags: 0,
                CPUAccessFlags: u32::try_from(D3D11_CPU_ACCESS_READ.0).unwrap_or_default(),
                MiscFlags: 0,
            };
            let mut staging = None;
            device
                .CreateTexture2D(
                    std::ptr::addr_of!(texture_desc),
                    None,
                    Some(std::ptr::addr_of_mut!(staging)),
                )
                .map_err(|error| {
                    CaptureAttemptError::from_windows("create CPU-readable staging texture", error)
                })?;
            let staging = staging.ok_or_else(|| {
                CaptureAttemptError::InvalidData("D3D11 returned no staging texture".to_owned())
            })?;

            Ok(Self {
                _device: device,
                context,
                duplication,
                staging,
                pointer: PointerState::default(),
                width,
                height,
                output_left: output.desc.DesktopCoordinates.left,
                output_top: output.desc.DesktopCoordinates.top,
                output_name: utf16_name(&output.desc.DeviceName),
                adapter_name: output.adapter_name,
            })
        }
    }

    fn capture(
        &mut self,
        deadline: Instant,
        include_cursor: bool,
    ) -> Result<AcquiredFrame, CaptureAttemptError> {
        let started = Instant::now();
        let timeout_ms = u32::try_from(
            deadline
                .saturating_duration_since(started)
                .as_millis()
                .min(u128::from(u32::MAX)),
        )
        .unwrap_or(u32::MAX);
        let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut desktop_resource: Option<IDXGIResource> = None;

        // SAFETY: output pointers refer to initialized stack variables and the duplication object
        // is valid for the duration of the call.
        unsafe {
            self.duplication
                .AcquireNextFrame(
                    timeout_ms,
                    std::ptr::addr_of_mut!(frame_info),
                    std::ptr::addr_of_mut!(desktop_resource),
                )
                .map_err(|error| {
                    CaptureAttemptError::from_windows("acquire next desktop frame", error)
                })?;
        }

        let result =
            self.process_acquired_frame(frame_info, desktop_resource, started, include_cursor);
        // SAFETY: AcquireNextFrame succeeded, so exactly one matching ReleaseFrame is required.
        let release_result = unsafe { self.duplication.ReleaseFrame() }.map_err(|error| {
            CaptureAttemptError::from_windows("release acquired desktop frame", error)
        });
        match (result, release_result) {
            (Err(error), _) | (Ok(_), Err(error)) => Err(error),
            (Ok(acquired), Ok(())) => Ok(acquired),
        }
    }

    fn process_acquired_frame(
        &mut self,
        frame_info: DXGI_OUTDUPL_FRAME_INFO,
        desktop_resource: Option<IDXGIResource>,
        started: Instant,
        include_cursor: bool,
    ) -> Result<AcquiredFrame, CaptureAttemptError> {
        let desktop_resource = desktop_resource.ok_or_else(|| {
            CaptureAttemptError::InvalidData(
                "DXGI acquired a frame without a desktop resource".to_owned(),
            )
        })?;
        self.update_pointer(&frame_info)?;
        let dirty_region_ratio = self.dirty_region_ratio(&frame_info)?;
        let texture: ID3D11Texture2D = desktop_resource.cast().map_err(|error| {
            CaptureAttemptError::from_windows("cast desktop resource to D3D11 texture", error)
        })?;

        // SAFETY: both resources belong to the same D3D11 device and have matching descriptions.
        unsafe {
            self.context.CopyResource(&self.staging, &texture);
        }
        let row_pitch = self.width.checked_mul(4).ok_or_else(|| {
            CaptureAttemptError::InvalidData("desktop row pitch overflowed".to_owned())
        })?;
        let byte_count = usize::try_from(row_pitch)
            .ok()
            .and_then(|pitch| pitch.checked_mul(usize::try_from(self.height).ok()?))
            .ok_or_else(|| {
                CaptureAttemptError::InvalidData("desktop frame size overflowed".to_owned())
            })?;
        let mut bytes = vec![0_u8; byte_count];
        self.copy_staging_to_cpu(&mut bytes, row_pitch)?;
        let pointer_composited = include_cursor
            && self
                .pointer
                .composite(&mut bytes, self.width, self.height, row_pitch);

        Ok(AcquiredFrame {
            pointer_updated: frame_info.LastMouseUpdateTime != 0,
            frame: VideoFrame {
                bytes,
                captured_at: Instant::now(),
                width: self.width,
                height: self.height,
                row_pitch,
                dirty_region_ratio,
                accumulated_frames: frame_info.AccumulatedFrames,
                pointer_visible: self.pointer.visible,
                pointer_composited,
                protected_content_masked_out: frame_info.ProtectedContentMaskedOut.as_bool(),
                capture_latency: started.elapsed(),
            },
        })
    }

    fn copy_staging_to_cpu(
        &self,
        destination: &mut [u8],
        packed_pitch: u32,
    ) -> Result<(), CaptureAttemptError> {
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        // SAFETY: the staging texture was created for CPU read access. The mapped pointer remains
        // valid until the matching Unmap below.
        unsafe {
            self.context
                .Map(
                    &self.staging,
                    0,
                    D3D11_MAP_READ,
                    0,
                    Some(std::ptr::addr_of_mut!(mapped)),
                )
                .map_err(|error| CaptureAttemptError::from_windows("map staging texture", error))?;
        }
        let copy_result = if mapped.RowPitch < packed_pitch || mapped.pData.is_null() {
            Err(CaptureAttemptError::InvalidData(
                "D3D11 returned an invalid mapped staging texture".to_owned(),
            ))
        } else {
            let packed_pitch = usize::try_from(packed_pitch).unwrap_or(0);
            let mapped_pitch = usize::try_from(mapped.RowPitch).unwrap_or(0);
            for row in 0..usize::try_from(self.height).unwrap_or(0) {
                // SAFETY: D3D11 guarantees at least RowPitch bytes for each mapped row and the
                // destination was allocated as height * packed_pitch bytes.
                let source = unsafe {
                    std::slice::from_raw_parts(
                        mapped.pData.cast::<u8>().add(row * mapped_pitch),
                        packed_pitch,
                    )
                };
                let offset = row * packed_pitch;
                destination[offset..offset + packed_pitch].copy_from_slice(source);
            }
            Ok(())
        };
        // SAFETY: this exactly matches the successful Map call above.
        unsafe {
            self.context.Unmap(&self.staging, 0);
        }
        copy_result
    }

    fn update_pointer(
        &mut self,
        frame_info: &DXGI_OUTDUPL_FRAME_INFO,
    ) -> Result<(), CaptureAttemptError> {
        if frame_info.LastMouseUpdateTime == 0 {
            return Ok(());
        }
        self.pointer.visible = frame_info.PointerPosition.Visible.as_bool();
        self.pointer.x = frame_info
            .PointerPosition
            .Position
            .x
            .saturating_sub(self.output_left);
        self.pointer.y = frame_info
            .PointerPosition
            .Position
            .y
            .saturating_sub(self.output_top);
        if frame_info.PointerShapeBufferSize == 0 {
            return Ok(());
        }

        let capacity = usize::try_from(frame_info.PointerShapeBufferSize).map_err(|_| {
            CaptureAttemptError::InvalidData("pointer shape buffer is too large".to_owned())
        })?;
        self.pointer.bytes.resize(capacity, 0);
        let mut required = 0_u32;
        let mut shape_info = DXGI_OUTDUPL_POINTER_SHAPE_INFO::default();
        // SAFETY: the buffer has PointerShapeBufferSize writable bytes as requested by DXGI.
        unsafe {
            self.duplication
                .GetFramePointerShape(
                    frame_info.PointerShapeBufferSize,
                    self.pointer.bytes.as_mut_ptr().cast(),
                    std::ptr::addr_of_mut!(required),
                    std::ptr::addr_of_mut!(shape_info),
                )
                .map_err(|error| {
                    CaptureAttemptError::from_windows("read desktop pointer shape", error)
                })?;
        }
        self.pointer.bytes.truncate(
            usize::try_from(required)
                .unwrap_or(self.pointer.bytes.len())
                .min(self.pointer.bytes.len()),
        );
        self.pointer.shape_info = Some(shape_info);
        Ok(())
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    fn dirty_region_ratio(
        &self,
        frame_info: &DXGI_OUTDUPL_FRAME_INFO,
    ) -> Result<f32, CaptureAttemptError> {
        if frame_info.TotalMetadataBufferSize == 0 {
            return Ok(if frame_info.LastPresentTime == 0 {
                0.0
            } else {
                1.0
            });
        }
        let total_size = usize::try_from(frame_info.TotalMetadataBufferSize).unwrap_or(0);
        let dirty_capacity = total_size / size_of::<RECT>() + 1;
        let move_capacity = total_size / size_of::<DXGI_OUTDUPL_MOVE_RECT>() + 1;
        let mut dirty_rects = vec![RECT::default(); dirty_capacity];
        let mut move_rects = vec![DXGI_OUTDUPL_MOVE_RECT::default(); move_capacity];
        let mut dirty_bytes = 0_u32;
        let mut move_bytes = 0_u32;

        // SAFETY: both vectors provide writable buffers at least TotalMetadataBufferSize bytes
        // large after rounding up to their element sizes.
        unsafe {
            self.duplication
                .GetFrameDirtyRects(
                    u32::try_from(dirty_rects.len() * size_of::<RECT>()).unwrap_or(u32::MAX),
                    dirty_rects.as_mut_ptr(),
                    std::ptr::addr_of_mut!(dirty_bytes),
                )
                .map_err(|error| {
                    CaptureAttemptError::from_windows("read dirty rectangles", error)
                })?;
            self.duplication
                .GetFrameMoveRects(
                    u32::try_from(move_rects.len() * size_of::<DXGI_OUTDUPL_MOVE_RECT>())
                        .unwrap_or(u32::MAX),
                    move_rects.as_mut_ptr(),
                    std::ptr::addr_of_mut!(move_bytes),
                )
                .map_err(|error| {
                    CaptureAttemptError::from_windows("read move rectangles", error)
                })?;
        }
        dirty_rects.truncate(usize::try_from(dirty_bytes).unwrap_or(0) / size_of::<RECT>());
        move_rects.truncate(
            usize::try_from(move_bytes).unwrap_or(0) / size_of::<DXGI_OUTDUPL_MOVE_RECT>(),
        );
        let dirty_area: u64 = dirty_rects
            .iter()
            .map(|rect| clipped_rect_area(rect, self.width, self.height))
            .sum();
        let moved_area: u64 = move_rects
            .iter()
            .map(|movement| clipped_rect_area(&movement.DestinationRect, self.width, self.height))
            .sum();
        let total_pixels = u64::from(self.width) * u64::from(self.height);
        if total_pixels == 0 {
            return Ok(0.0);
        }
        Ok(((dirty_area + moved_area).min(total_pixels) as f64 / total_pixels as f64) as f32)
    }
}

#[derive(Default)]
struct PointerState {
    visible: bool,
    x: i32,
    y: i32,
    bytes: Vec<u8>,
    shape_info: Option<DXGI_OUTDUPL_POINTER_SHAPE_INFO>,
}

impl PointerState {
    fn composite(&self, frame: &mut [u8], width: u32, height: u32, row_pitch: u32) -> bool {
        if !self.visible {
            return false;
        }
        let Some(info) = self.shape_info else {
            return false;
        };
        let kind = if info.Type
            == u32::try_from(DXGI_OUTDUPL_POINTER_SHAPE_TYPE_COLOR.0).unwrap_or_default()
        {
            PointerShapeKind::Color
        } else if info.Type
            == u32::try_from(DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MASKED_COLOR.0).unwrap_or_default()
        {
            PointerShapeKind::MaskedColor
        } else if info.Type
            == u32::try_from(DXGI_OUTDUPL_POINTER_SHAPE_TYPE_MONOCHROME.0).unwrap_or_default()
        {
            PointerShapeKind::Monochrome
        } else {
            return false;
        };
        compose_cursor(
            frame,
            width,
            height,
            row_pitch,
            self.x,
            self.y,
            PointerShape {
                kind,
                width: info.Width,
                height: info.Height,
                pitch: info.Pitch,
                bytes: &self.bytes,
            },
        )
    }
}

struct OutputCandidate {
    adapter: IDXGIAdapter1,
    output: IDXGIOutput1,
    desc: DXGI_OUTPUT_DESC,
    adapter_name: String,
}

fn enumerate_outputs(factory: &IDXGIFactory1) -> Result<Vec<OutputCandidate>, CaptureAttemptError> {
    let mut outputs = Vec::new();
    let mut adapter_index = 0_u32;
    loop {
        // SAFETY: indices are advanced monotonically until DXGI reports not found.
        let adapter = match unsafe { factory.EnumAdapters1(adapter_index) } {
            Ok(adapter) => adapter,
            Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
            Err(error) => {
                return Err(CaptureAttemptError::from_windows(
                    "enumerate DXGI adapters",
                    error,
                ));
            }
        };
        // SAFETY: adapter is a valid COM object returned by the factory.
        let adapter_desc = unsafe { adapter.GetDesc1() }.map_err(|error| {
            CaptureAttemptError::from_windows("read adapter description", error)
        })?;
        let adapter_name = utf16_name(&adapter_desc.Description);
        let mut output_index = 0_u32;
        loop {
            // SAFETY: indices are advanced monotonically until DXGI reports not found.
            let output = match unsafe { adapter.EnumOutputs(output_index) } {
                Ok(output) => output,
                Err(error) if error.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(error) => {
                    return Err(CaptureAttemptError::from_windows(
                        "enumerate DXGI outputs",
                        error,
                    ));
                }
            };
            // SAFETY: output is a valid COM object returned by the adapter.
            let desc = unsafe { output.GetDesc() }.map_err(|error| {
                CaptureAttemptError::from_windows("read output description", error)
            })?;
            if desc.AttachedToDesktop.as_bool() {
                let output = output.cast::<IDXGIOutput1>().map_err(|error| {
                    CaptureAttemptError::from_windows("cast DXGI output for duplication", error)
                })?;
                outputs.push(OutputCandidate {
                    adapter: adapter.clone(),
                    output,
                    desc,
                    adapter_name: adapter_name.clone(),
                });
            }
            output_index += 1;
        }
        adapter_index += 1;
    }
    if outputs.is_empty() {
        return Err(CaptureAttemptError::InvalidData(
            "no attached desktop output supports DXGI enumeration".to_owned(),
        ));
    }
    Ok(outputs)
}

fn compare_outputs(left: &OutputCandidate, right: &OutputCandidate) -> Ordering {
    let left_primary =
        left.desc.DesktopCoordinates.left == 0 && left.desc.DesktopCoordinates.top == 0;
    let right_primary =
        right.desc.DesktopCoordinates.left == 0 && right.desc.DesktopCoordinates.top == 0;
    right_primary.cmp(&left_primary).then_with(|| {
        left.desc
            .DesktopCoordinates
            .left
            .cmp(&right.desc.DesktopCoordinates.left)
    })
}

fn clipped_rect_area(rect: &RECT, width: u32, height: u32) -> u64 {
    let left = rect
        .left
        .max(0)
        .min(i32::try_from(width).unwrap_or(i32::MAX));
    let right = rect
        .right
        .max(0)
        .min(i32::try_from(width).unwrap_or(i32::MAX));
    let top = rect
        .top
        .max(0)
        .min(i32::try_from(height).unwrap_or(i32::MAX));
    let bottom = rect
        .bottom
        .max(0)
        .min(i32::try_from(height).unwrap_or(i32::MAX));
    let rect_width = u64::try_from((right - left).max(0)).unwrap_or(0);
    let rect_height = u64::try_from((bottom - top).max(0)).unwrap_or(0);
    rect_width * rect_height
}

fn utf16_name(buffer: &[u16]) -> String {
    let length = buffer
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..length])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_area_is_clipped_to_frame() {
        let rect = RECT {
            left: -20,
            top: 10,
            right: 200,
            bottom: 100,
        };
        assert_eq!(clipped_rect_area(&rect, 100, 80), 7_000);
    }

    #[test]
    fn utf16_names_stop_at_null() {
        assert_eq!(
            utf16_name(&[u16::from(b'A'), u16::from(b'B'), 0, u16::from(b'C')]),
            "AB"
        );
    }
}

/// Physical desktop rectangle selected by the same DXGI order as capture.
///
/// # Errors
/// Returns an error if DXGI enumeration fails or the requested display is unavailable.
pub fn control_display_bounds(display_index: u32) -> Result<[f64; 4], MediaError> {
    // SAFETY: factory owns enumerated output wrappers; descriptors are copied values.
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }
        .map_err(|e| MediaError::Operation(format!("control DXGI factory: {e}")))?;
    let mut outputs = enumerate_outputs(&factory).map_err(CaptureAttemptError::into_media_error)?;
    outputs.sort_by(compare_outputs);
    let output = outputs
        .into_iter()
        .nth(display_index as usize)
        .ok_or_else(|| MediaError::Operation("control display unavailable".into()))?;
    let r = output.desc.DesktopCoordinates;
    Ok([
        f64::from(r.left),
        f64::from(r.top),
        f64::from(r.right - r.left),
        f64::from(r.bottom - r.top),
    ])
}

/// Stable monitor handle selected in the same DXGI order as capture.
///
/// # Errors
/// Returns an error when DXGI cannot enumerate the requested display.
pub fn control_display_monitor(
    display_index: u32,
) -> Result<windows::Win32::Graphics::Gdi::HMONITOR, MediaError> {
    // SAFETY: owned DXGI factory/output wrappers; copied monitor handle is
    // borrowed from the OS and must not be closed by the caller.
    let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }
        .map_err(|e| MediaError::Operation(format!("control DXGI factory: {e}")))?;
    let mut outputs = enumerate_outputs(&factory).map_err(CaptureAttemptError::into_media_error)?;
    outputs.sort_by(compare_outputs);
    outputs
        .into_iter()
        .nth(display_index as usize)
        .map(|output| output.desc.Monitor)
        .ok_or_else(|| MediaError::Operation("control display unavailable".into()))
}
