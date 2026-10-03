use std::{
    borrow::Cow,
    net::SocketAddr,
    time::{Duration, Instant},
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("media backend is unavailable: {0}")]
    BackendUnavailable(String),
    #[error("media operation failed: {0}")]
    Operation(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodecBackendKind {
    Hardware,
    Software,
    Unknown,
}

impl std::fmt::Display for CodecBackendKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hardware => formatter.write_str("hardware"),
            Self::Software => formatter.write_str("software"),
            Self::Unknown => formatter.write_str("unknown"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodecDescriptor {
    pub name: String,
    pub kind: CodecBackendKind,
    pub asynchronous: bool,
}

impl CodecDescriptor {
    #[must_use]
    pub fn unavailable() -> Self {
        Self {
            name: "unavailable".to_owned(),
            kind: CodecBackendKind::Software,
            asynchronous: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct H264Config {
    pub width: u32,
    pub height: u32,
    pub frames_per_second: u32,
    pub bitrate: u32,
    pub keyframe_interval_frames: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureConfig {
    pub display_index: u32,
    pub width: u32,
    pub height: u32,
    pub max_fps: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct VideoFrameRef<'a> {
    pub bytes: &'a [u8],
    pub captured_at: Instant,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug)]
pub struct VideoFrame {
    pub bytes: Vec<u8>,
    pub captured_at: Instant,
    pub width: u32,
    pub height: u32,
    pub row_pitch: u32,
    pub dirty_region_ratio: f32,
    pub accumulated_frames: u32,
    pub pointer_visible: bool,
    pub pointer_composited: bool,
    pub protected_content_masked_out: bool,
    pub capture_latency: Duration,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CaptureStatistics {
    pub frames_acquired: u64,
    /// Frames overwritten in a bounded capture slot before the media loop consumed them.
    pub latest_frame_replacements: u64,
    pub no_change_timeouts: u64,
    pub access_loss_recoveries: u64,
    pub accumulated_frames: u64,
    pub pointer_updates: u64,
    pub pointer_frames_composited: u64,
    pub protected_content_frames: u64,
}

#[derive(Clone, Debug)]
pub struct EncodedAccessUnit {
    pub bytes: Vec<u8>,
    pub timestamp: Duration,
    pub is_keyframe: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct EncodedAccessUnitRef<'a> {
    pub bytes: &'a [u8],
    pub timestamp: Duration,
    pub is_keyframe: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DecodedPixelFormat {
    #[default]
    Bgra,
    Nv12 {
        stride: u32,
        storage_height: u32,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameTiming {
    pub capture_unix_us: u64,
    pub send_unix_us: u64,
}

#[must_use]
pub fn unix_us() -> u64 {
    u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros(),
    )
    .unwrap_or(u64::MAX)
}

#[derive(Clone, Debug)]
pub struct DecodedFrame {
    pub media_timestamp: Option<Duration>,
    pub timing: Option<FrameTiming>,
    pub format: DecodedPixelFormat,
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct DecodedFrameRef<'a> {
    pub format: DecodedPixelFormat,
    pub bytes: &'a [u8],
    pub width: u32,
    pub height: u32,
}

impl<'a> DecodedFrameRef<'a> {
    /// Validates storage before any renderer/FFI reads pixels.
    /// # Errors
    /// Returns an error for invalid dimensions, padding, pitch or buffer length.
    pub fn validate(self) -> Result<(), MediaError> {
        let size = match self.format {
            DecodedPixelFormat::Bgra => u64::from(self.width)
                .checked_mul(u64::from(self.height))
                .and_then(|size| size.checked_mul(4))
                .unwrap_or(u64::MAX),
            DecodedPixelFormat::Nv12 {
                stride,
                storage_height,
            } => {
                if self.width % 2 != 0
                    || self.height % 2 != 0
                    || stride < self.width
                    || stride % 2 != 0
                    || storage_height < self.height
                    || storage_height % 2 != 0
                {
                    return Err(MediaError::Operation(
                        "invalid NV12 render layout".to_owned(),
                    ));
                }
                u64::from(stride)
                    .checked_mul(u64::from(storage_height))
                    .and_then(|size| size.checked_mul(3))
                    .map_or(u64::MAX, |size| size / 2)
            }
        };
        if self.width == 0
            || self.height == 0
            || usize::try_from(size).map_or(true, |size| self.bytes.len() < size)
        {
            return Err(MediaError::Operation(
                "invalid decoded frame storage".to_owned(),
            ));
        }
        Ok(())
    }

    /// CPU fallback, using the existing BT.709 limited-range integer coefficients.
    /// # Errors
    /// Returns an error if storage is malformed.
    pub fn bgra(self) -> Result<Cow<'a, [u8]>, MediaError> {
        self.validate()?;
        let DecodedPixelFormat::Nv12 {
            stride,
            storage_height,
        } = self.format
        else {
            return Ok(Cow::Borrowed(self.bytes));
        };
        let (width, height, stride, storage_height) = (
            self.width as usize,
            self.height as usize,
            stride as usize,
            storage_height as usize,
        );
        let mut output = vec![0; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                let uv = stride * storage_height + (y / 2) * stride + (x & !1);
                let c = (i32::from(self.bytes[y * stride + x]) - 16).max(0);
                let d = i32::from(self.bytes[uv]) - 128;
                let e = i32::from(self.bytes[uv + 1]) - 128;
                let offset = (y * width + x) * 4;
                for (channel, value) in [
                    298 * c + 541 * d,
                    298 * c - 55 * d - 136 * e,
                    298 * c + 459 * e,
                ]
                .into_iter()
                .enumerate()
                {
                    output[offset + channel] =
                        u8::try_from(((value + 128) >> 8).clamp(0, 255)).unwrap_or_default();
                }
                output[offset + 3] = 255;
            }
        }
        Ok(Cow::Owned(output))
    }
}

pub mod diagnostics;
pub mod source_timing;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubscriberEndpoint {
    pub id: u64,
    pub address: SocketAddr,
}

pub trait CaptureSource: Send {
    /// Optional session-owned cursor policy. True includes the source pointer;
    /// false lets an interactive viewer use its immediate local pointer.
    /// Ordinary classroom broadcast leaves this unset and includes the cursor.
    fn set_cursor_policy(&mut self, _visible: std::sync::Arc<std::sync::atomic::AtomicBool>) {}
    /// Starts capture for the requested display and format.
    ///
    /// # Errors
    /// Returns an error when the backend cannot initialize the capture session.
    fn start(&mut self, config: CaptureConfig) -> Result<(), MediaError>;
    /// Start aspect-preserving thumbnail capture without changing the broadcast path.
    /// # Errors
    /// Returns initialization or invalid-dimension errors.
    fn start_thumbnail(&mut self, config: CaptureConfig) -> Result<(), MediaError> {
        self.start(config)
    }
    /// Dimensions of frames produced by the active capture session.
    #[must_use]
    fn output_dimensions(&self) -> Option<(u32, u32)>;
    /// Returns the newest frame available before the deadline.
    ///
    /// # Errors
    /// Returns an error when the capture backend cannot continue safely.
    fn next_frame(&mut self, deadline: Instant) -> Result<Option<VideoFrame>, MediaError>;
    #[must_use]
    fn statistics(&self) -> CaptureStatistics;
}

pub trait VideoEncoder {
    /// Returns source metadata associated with an actual encoded output timestamp.
    fn take_source_timing(&mut self, _timestamp: Duration) -> Option<source_timing::SourceTiming> {
        None
    }
    fn take_diagnostics(&mut self) -> diagnostics::StageMetrics {
        diagnostics::StageMetrics::default()
    }
    /// Encodes one frame, optionally producing a complete access unit.
    ///
    /// # Errors
    /// Returns an error when conversion or encoding fails.
    fn encode(&mut self, frame: VideoFrameRef<'_>)
        -> Result<Option<EncodedAccessUnit>, MediaError>;
    /// Changes the target encoder bitrate.
    ///
    /// # Errors
    /// Returns an error when the backend cannot apply the requested bitrate.
    fn set_bitrate(&mut self, bits_per_second: u32) -> Result<(), MediaError>;
    /// Requests that the next suitable access unit be an IDR/keyframe.
    ///
    /// # Errors
    /// Returns an error when the backend rejects the request.
    fn request_keyframe(&mut self) -> Result<(), MediaError>;
}

pub trait VideoDecoder {
    fn take_diagnostics(&mut self) -> diagnostics::StageMetrics {
        diagnostics::StageMetrics::default()
    }
    /// Decodes one encoded access unit.
    ///
    /// # Errors
    /// Returns an error when the access unit is invalid or decoding fails.
    fn decode(
        &mut self,
        access_unit: EncodedAccessUnitRef<'_>,
    ) -> Result<Vec<DecodedFrame>, MediaError>;
}

pub trait MediaDistributor: Send {
    /// Adds or replaces a subscriber endpoint.
    ///
    /// # Errors
    /// Returns an error when the endpoint cannot be registered.
    fn add_subscriber(&mut self, endpoint: SubscriberEndpoint) -> Result<(), MediaError>;
    fn remove_subscriber(&mut self, id: u64);
    /// Fans one already-packetized media packet out to current subscribers.
    ///
    /// # Errors
    /// Returns an error when distribution cannot proceed without violating boundedness.
    fn distribute(&mut self, packet: &[u8]) -> Result<(), MediaError>;
}

pub trait Renderer {
    fn take_diagnostics(&mut self) -> diagnostics::StageMetrics {
        diagnostics::StageMetrics::default()
    }
    /// Presents one decoded frame.
    ///
    /// # Errors
    /// Returns an error when the frame cannot be presented.
    fn render(&mut self, frame: DecodedFrameRef<'_>) -> Result<(), MediaError>;
}

#[cfg(test)]
mod pixel_tests {
    use super::*;
    #[test]
    fn padded_nv12_uses_storage_height_for_chroma_plane() {
        // Visible 2x2, allocation stride 4 and height 4; padding differs from UV.
        let mut bytes = vec![0; 24];
        bytes[0..2].copy_from_slice(&[16, 235]);
        bytes[4..6].copy_from_slice(&[81, 145]);
        bytes[16..18].copy_from_slice(&[128, 128]);
        let frame = DecodedFrameRef {
            format: DecodedPixelFormat::Nv12 {
                stride: 4,
                storage_height: 4,
            },
            bytes: &bytes,
            width: 2,
            height: 2,
        };
        let bgra = frame.bgra().unwrap();
        assert_eq!(&bgra[0..8], &[0, 0, 0, 255, 255, 255, 255, 255]);
        assert_eq!(&bgra[8..16], &[76, 76, 76, 255, 150, 150, 150, 255]);
        let bad = DecodedFrameRef {
            bytes: &bytes[..17],
            ..frame
        };
        assert!(bad.validate().is_err());
    }
    #[test]
    fn pixel_validation_rejects_invalid_and_overflowing_layouts() {
        let frame = DecodedFrameRef {
            format: DecodedPixelFormat::Bgra,
            bytes: &[],
            width: u32::MAX,
            height: u32::MAX,
        };
        assert!(frame.validate().is_err());
        let frame = DecodedFrameRef {
            format: DecodedPixelFormat::Nv12 {
                stride: 4,
                storage_height: 4,
            },
            bytes: &[0; 24],
            width: 3,
            height: 2,
        };
        assert!(frame.validate().is_err());
    }
}
