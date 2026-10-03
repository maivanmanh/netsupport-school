use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use media_core::{CaptureConfig, CaptureSource, CaptureStatistics, MediaError, VideoFrame};
use screencapturekit::{
    cm::{CMSampleBufferExt, CMSampleBufferSCExt, SCFrameStatus},
    cv::CVPixelBufferLockFlags,
    prelude::{
        CMSampleBuffer, PixelFormat, SCContentFilter, SCShareableContent, SCStream,
        SCStreamConfiguration, SCStreamOutputTrait, SCStreamOutputType,
    },
};

#[derive(Default)]
struct SharedState {
    latest: Option<VideoFrame>,
    statistics: CaptureStatistics,
}

#[derive(Default)]
struct Shared {
    state: Mutex<SharedState>,
    ready: Condvar,
    cursor_visible: std::sync::atomic::AtomicBool,
}

struct FrameHandler {
    shared: Arc<Shared>,
}

impl SCStreamOutputTrait for FrameHandler {
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, output_type: SCStreamOutputType) {
        if output_type != SCStreamOutputType::Screen {
            return;
        }
        let captured_at = Instant::now();
        let frame_info = sample.frame_info();
        if frame_info
            .as_ref()
            .and_then(|info| info.frame_status)
            .is_some_and(|status| {
                !matches!(status, SCFrameStatus::Complete | SCFrameStatus::Started)
            })
        {
            return;
        }
        let Some(pixel_buffer) = sample.pixel_buffer() else {
            return;
        };
        let Ok(guard) = pixel_buffer.lock(CVPixelBufferLockFlags::READ_ONLY) else {
            return;
        };
        let Some(source) = (unsafe { guard.as_slice() }) else {
            return;
        };
        let width = guard.width();
        let height = guard.height();
        let source_pitch = guard.bytes_per_row();
        let Some(packed_pitch) = width.checked_mul(4) else {
            return;
        };
        let Some(packed_len) = packed_pitch.checked_mul(height) else {
            return;
        };
        let mut bytes = vec![0_u8; packed_len];
        for row in 0..height {
            let source_start = row * source_pitch;
            let target_start = row * packed_pitch;
            bytes[target_start..target_start + packed_pitch]
                .copy_from_slice(&source[source_start..source_start + packed_pitch]);
        }

        let dirty_region_ratio = frame_info
            .as_ref()
            .and_then(|info| info.dirty_rects.as_ref())
            .map_or(1.0, |rects| {
                let changed = rects
                    .iter()
                    .map(|rect| (rect.size.width.max(0.0) * rect.size.height.max(0.0)) as f32)
                    .sum::<f32>();
                let area = (width * height) as f32;
                if area > 0.0 {
                    (changed / area).clamp(0.0, 1.0)
                } else {
                    0.0
                }
            });
        let cursor_visible = self
            .shared
            .cursor_visible
            .load(std::sync::atomic::Ordering::Acquire);
        let frame = VideoFrame {
            bytes,
            captured_at,
            width: u32::try_from(width).unwrap_or(u32::MAX),
            height: u32::try_from(height).unwrap_or(u32::MAX),
            row_pitch: u32::try_from(packed_pitch).unwrap_or(u32::MAX),
            dirty_region_ratio,
            accumulated_frames: 1,
            pointer_visible: cursor_visible,
            pointer_composited: cursor_visible,
            protected_content_masked_out: false,
            capture_latency: captured_at.elapsed(),
        };

        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.statistics.frames_acquired += 1;
        state.statistics.accumulated_frames += 1;
        state.statistics.pointer_updates += 1;
        state.statistics.pointer_frames_composited += u64::from(cursor_visible);
        if state.latest.replace(frame).is_some() {
            state.statistics.latest_frame_replacements += 1;
        }
        drop(state);
        self.shared.ready.notify_one();
    }
}

pub struct ScreenCaptureKitCapture {
    shared: Arc<Shared>,
    stream: Option<SCStream>,
    dimensions: Option<(u32, u32)>,
    cursor_policy: Option<Arc<std::sync::atomic::AtomicBool>>,
    stream_config: Option<SCStreamConfiguration>,
    cursor_visible: bool,
    display_index: usize,
    max_dimensions: (u32, u32),
    resolution_changed: Arc<AtomicBool>,
    resolution_monitor_stop: Arc<AtomicBool>,
    resolution_monitor: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for ScreenCaptureKitCapture {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ScreenCaptureKitCapture")
            .field("started", &self.stream.is_some())
            .finish_non_exhaustive()
    }
}

impl Default for ScreenCaptureKitCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl ScreenCaptureKitCapture {
    #[must_use]
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Shared::default()),
            stream: None,
            dimensions: None,
            cursor_policy: None,
            stream_config: None,
            cursor_visible: true,
            display_index: 0,
            max_dimensions: (1920, 1080),
            resolution_changed: Arc::new(AtomicBool::new(false)),
            resolution_monitor_stop: Arc::new(AtomicBool::new(false)),
            resolution_monitor: None,
        }
    }

    fn stop_resolution_monitor(&mut self) {
        self.resolution_monitor_stop.store(true, Ordering::Release);
        if let Some(worker) = self.resolution_monitor.take() {
            worker.thread().unpark();
            let _ = worker.join();
        }
    }

    fn start_resolution_monitor(&mut self, expected: (u32, u32)) -> Result<(), MediaError> {
        self.resolution_changed.store(false, Ordering::Release);
        self.resolution_monitor_stop.store(false, Ordering::Release);
        let stop = Arc::clone(&self.resolution_monitor_stop);
        let changed = Arc::clone(&self.resolution_changed);
        let display_index = self.display_index;
        let max_dimensions = self.max_dimensions;
        self.resolution_monitor = Some(
            thread::Builder::new()
                .name("classroom-display-resolution".to_owned())
                .spawn(move || {
                    // ScreenCaptureKit's shareable-content query can block on
                    // WindowServer. Keep it off the media loop, where a one
                    // second poll used to stall capture and encoding.
                    while !stop.load(Ordering::Acquire) {
                        thread::park_timeout(Duration::from_secs(2));
                        if stop.load(Ordering::Acquire) {
                            break;
                        }
                        let started = Instant::now();
                        let Ok(content) = SCShareableContent::get() else {
                            continue;
                        };
                        let actual = content.displays().get(display_index).and_then(|display| {
                            fit_even_dimensions((display.width(), display.height()), max_dimensions)
                                .ok()
                        });
                        let probe_ms = started.elapsed().as_secs_f64() * 1_000.0;
                        if actual.is_some_and(|dimensions| dimensions != expected) {
                            changed.store(true, Ordering::Release);
                            tracing::info!(
                                resolution_probe_ms = probe_ms,
                                expected_width = expected.0,
                                expected_height = expected.1,
                                actual = ?actual,
                                "ScreenCaptureKit display resolution changed"
                            );
                            break;
                        }
                        if probe_ms >= 25.0 {
                            tracing::info!(
                                resolution_probe_ms = probe_ms,
                                "ScreenCaptureKit display resolution check completed"
                            );
                        }
                    }
                })
                .map_err(|error| {
                    MediaError::Operation(format!(
                        "could not start display resolution monitor: {error}"
                    ))
                })?,
        );
        Ok(())
    }
}

impl CaptureSource for ScreenCaptureKitCapture {
    fn set_cursor_policy(&mut self, visible: Arc<std::sync::atomic::AtomicBool>) {
        self.cursor_policy = Some(visible);
    }
    fn start(&mut self, config: CaptureConfig) -> Result<(), MediaError> {
        if config.width == 0 || config.height == 0 || config.max_fps == 0 {
            return Err(MediaError::Operation(
                "capture dimensions and frame rate must be non-zero".to_owned(),
            ));
        }
        self.stop_resolution_monitor();
        if let Some(stream) = self.stream.take() {
            let _ = stream.stop_capture();
        }
        let content = SCShareableContent::get().map_err(operation)?;
        let displays = content.displays();
        let display = displays.get(config.display_index as usize).ok_or_else(|| {
            MediaError::Operation(format!(
                "display index {} is unavailable; found {} display(s)",
                config.display_index,
                displays.len()
            ))
        })?;
        let filter = SCContentFilter::create()
            .with_display(display)
            .with_excluding_windows(&[])
            .build();
        self.cursor_visible = self
            .cursor_policy
            .as_ref()
            .is_none_or(|v| v.load(std::sync::atomic::Ordering::Acquire));
        self.shared
            .cursor_visible
            .store(self.cursor_visible, std::sync::atomic::Ordering::Release);
        // The configured size is a ceiling, not the capture size. ScreenCaptureKit
        // otherwise upscales every Student display (including small displays) to
        // 1920x1080, which makes Observe stats and text fidelity misleading.
        let dimensions = fit_even_dimensions(
            (display.width(), display.height()),
            (config.width, config.height),
        )?;
        let stream_config = SCStreamConfiguration::new()
            .with_width(dimensions.0)
            .with_height(dimensions.1)
            .with_pixel_format(PixelFormat::BGRA)
            .with_shows_cursor(self.cursor_visible)
            .with_queue_depth(3)
            .with_fps(config.max_fps);
        let mut stream = SCStream::new(&filter, &stream_config);
        stream.add_output_handler(
            FrameHandler {
                shared: Arc::clone(&self.shared),
            },
            SCStreamOutputType::Screen,
        );
        stream.start_capture().map_err(operation)?;
        self.stream_config = Some(stream_config);
        self.stream = Some(stream);
        self.dimensions = Some(dimensions);
        self.display_index = config.display_index as usize;
        self.max_dimensions = (config.width, config.height);
        self.start_resolution_monitor(dimensions)?;
        Ok(())
    }

    fn start_thumbnail(&mut self, mut config: CaptureConfig) -> Result<(), MediaError> {
        let content = SCShareableContent::get().map_err(operation)?;
        let displays = content.displays();
        let display = displays
            .get(config.display_index as usize)
            .ok_or_else(|| MediaError::Operation("thumbnail display unavailable".into()))?;
        let (sw, sh) = (display.width(), display.height());
        if sw == 0 || sh == 0 || config.width < 2 || config.height < 2 {
            return Err(MediaError::Operation("invalid thumbnail dimensions".into()));
        }
        let (bw, bh) = (sw.min(config.width), sh.min(config.height));
        if u64::from(bw) * u64::from(sh) <= u64::from(bh) * u64::from(sw) {
            config.width = bw & !1;
            config.height = u32::try_from(u64::from(config.width) * u64::from(sh) / u64::from(sw))
                .unwrap_or(bh)
                .max(2)
                & !1;
        } else {
            config.height = bh & !1;
            config.width = u32::try_from(u64::from(config.height) * u64::from(sw) / u64::from(sh))
                .unwrap_or(bw)
                .max(2)
                & !1;
        }
        self.start(config)
    }

    fn output_dimensions(&self) -> Option<(u32, u32)> {
        self.dimensions
    }

    fn next_frame(&mut self, deadline: Instant) -> Result<Option<VideoFrame>, MediaError> {
        if self.stream.is_none() {
            return Err(MediaError::Operation(
                "ScreenCaptureKit capture has not been started".to_owned(),
            ));
        }
        // Detect mode changes in the background: SCShareableContent::get can
        // block on WindowServer, so it must not run on the media loop.
        if self.resolution_changed.load(Ordering::Acquire) {
            return Err(MediaError::Operation(
                "display resolution changed; restart observation to negotiate the new dimensions"
                    .into(),
            ));
        }
        let visible = self
            .cursor_policy
            .as_ref()
            .is_none_or(|v| v.load(std::sync::atomic::Ordering::Acquire));
        if visible != self.cursor_visible {
            // Update on the capture worker, never on the input-injection thread.
            // Keep dimensions, format and pacing unchanged across mode switches.
            let config = self
                .stream_config
                .as_mut()
                .ok_or_else(|| MediaError::Operation("capture configuration missing".into()))?;
            config.set_shows_cursor(visible);
            self.stream
                .as_ref()
                .unwrap()
                .update_configuration(config)
                .map_err(operation)?;
            self.cursor_visible = visible;
            self.shared
                .cursor_visible
                .store(visible, std::sync::atomic::Ordering::Release);
            self.shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .latest = None;
        }
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            if let Some(frame) = state.latest.take() {
                return Ok(Some(frame));
            }
            let now = Instant::now();
            if now >= deadline {
                state.statistics.no_change_timeouts += 1;
                return Ok(None);
            }
            let timeout = deadline.saturating_duration_since(now);
            let (next_state, wait_result) = self
                .shared
                .ready
                .wait_timeout(state, timeout)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state = next_state;
            if wait_result.timed_out() && state.latest.is_none() {
                state.statistics.no_change_timeouts += 1;
                return Ok(None);
            }
        }
    }

    fn statistics(&self) -> CaptureStatistics {
        self.shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .statistics
    }
}

fn fit_even_dimensions(source: (u32, u32), limit: (u32, u32)) -> Result<(u32, u32), MediaError> {
    let (width, height) = source;
    let (limit_width, limit_height) = limit;
    if width < 2 || height < 2 || limit_width < 2 || limit_height < 2 {
        return Err(MediaError::Operation(
            "display resolution is smaller than 2x2".into(),
        ));
    }
    let width_limited =
        u64::from(limit_width) * u64::from(height) <= u64::from(limit_height) * u64::from(width);
    let (fitted_width, fitted_height) = if width_limited {
        (
            width.min(limit_width),
            u32::try_from(u64::from(width.min(limit_width)) * u64::from(height) / u64::from(width))
                .map_err(|_| MediaError::Operation("fitted display height overflow".into()))?,
        )
    } else {
        (
            u32::try_from(
                u64::from(height.min(limit_height)) * u64::from(width) / u64::from(height),
            )
            .map_err(|_| MediaError::Operation("fitted display width overflow".into()))?,
            height.min(limit_height),
        )
    };
    let dimensions = (fitted_width & !1, fitted_height & !1);
    if dimensions.0 < 2 || dimensions.1 < 2 {
        return Err(MediaError::Operation(
            "fitted display resolution is smaller than 2x2".into(),
        ));
    }
    Ok(dimensions)
}

impl Drop for ScreenCaptureKitCapture {
    fn drop(&mut self) {
        self.stop_resolution_monitor();
        if let Some(stream) = self.stream.take() {
            let _ = stream.stop_capture();
        }
    }
}

fn operation(error: impl std::fmt::Display) -> MediaError {
    MediaError::Operation(format!("ScreenCaptureKit operation failed: {error}"))
}

/// Desktop coordinates in macOS points, in the same display order as capture.
/// Retina backing pixels must never be used for `CGEvent` pointer positions.
///
/// # Errors
/// Returns an error if shareable displays cannot be queried or the index is unavailable.
pub fn control_display_bounds(display_index: u32) -> Result<[f64; 4], MediaError> {
    let content = SCShareableContent::get().map_err(operation)?;
    let displays = content.displays();
    let display = displays
        .get(display_index as usize)
        .ok_or_else(|| MediaError::Operation("control display unavailable".into()))?;
    let frame = display.frame();
    Ok([
        frame.origin.x,
        frame.origin.y,
        frame.size.width,
        frame.size.height,
    ])
}

/// Stable display identity in the same `ScreenCaptureKit` order as capture.
///
/// # Errors
/// Returns an error when the requested capture display is unavailable.
pub fn control_display_id(display_index: u32) -> Result<u32, MediaError> {
    let content = SCShareableContent::get().map_err(operation)?;
    content
        .displays()
        .get(display_index as usize)
        .map(screencapturekit::shareable_content::SCDisplay::display_id)
        .ok_or_else(|| MediaError::Operation("control display unavailable".into()))
}
