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

/// Elementary video codec carried over RTP. Classroom broadcast stays H.264;
/// richer codecs are negotiated per session only when both ends support them.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub enum VideoCodec {
    #[default]
    H264,
    H265,
}
impl VideoCodec {
    /// Preference order when both peers support several codecs.
    pub const PREFERENCE: [Self; 2] = [Self::H265, Self::H264];
    /// Dynamic RTP payload type used on the wire.
    #[must_use]
    pub const fn rtp_payload_type(self) -> u8 {
        match self {
            Self::H264 => 96,
            Self::H265 => 98,
        }
    }
    #[must_use]
    pub const fn from_rtp_payload_type(value: u32) -> Option<Self> {
        match value {
            96 => Some(Self::H264),
            98 => Some(Self::H265),
            _ => None,
        }
    }
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::H264 => "h264",
            Self::H265 => "h265",
        }
    }
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "h264" | "avc" => Some(Self::H264),
            "h265" | "hevc" => Some(Self::H265),
            _ => None,
        }
    }
    /// NAL header length in bytes.
    #[must_use]
    pub const fn nal_header_len(self) -> usize {
        match self {
            Self::H264 => 1,
            Self::H265 => 2,
        }
    }
    /// NAL unit type of a unit starting with its header.
    #[must_use]
    pub fn nal_type(self, unit: &[u8]) -> Option<u8> {
        let first = *unit.first()?;
        Some(match self {
            Self::H264 => first & 0x1f,
            Self::H265 => (first >> 1) & 0x3f,
        })
    }
    /// A unit that can begin decoding (IDR / IRAP).
    #[must_use]
    pub fn is_random_access(self, unit: &[u8]) -> bool {
        match (self, self.nal_type(unit)) {
            (Self::H264, Some(5)) => true,
            (Self::H265, Some(kind)) => (16..=21).contains(&kind),
            _ => false,
        }
    }
    /// Elementary-stream NAL types (excludes RTP aggregation/fragment types).
    #[must_use]
    pub fn is_elementary(self, unit: &[u8]) -> bool {
        match (self, self.nal_type(unit)) {
            (Self::H264, Some(kind)) => (1..=23).contains(&kind),
            (Self::H265, Some(kind)) => kind <= 47 && unit.len() >= 2,
            _ => false,
        }
    }
}
impl std::fmt::Display for VideoCodec {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::H264 => "H.264",
            Self::H265 => "H.265",
        })
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
    /// Pointer-only updates not turned into frames because receivers draw
    /// the pointer themselves (separate pointer channel).
    pub pointer_only_updates_skipped: u64,
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

/// Teacher pointer image, straight-alpha BGRA, at most 128×128.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PointerImage {
    /// Changes whenever the image changes; receivers cache by serial.
    pub serial: u32,
    pub width: u16,
    pub height: u16,
    pub hotspot_x: u16,
    pub hotspot_y: u16,
    pub bgra: Vec<u8>,
}
impl PointerImage {
    pub const MAX_SIDE: u16 = 128;
    /// True when dimensions and storage agree and are within bounds.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.width > 0
            && self.height > 0
            && self.width <= Self::MAX_SIDE
            && self.height <= Self::MAX_SIDE
            && self.hotspot_x < self.width
            && self.hotspot_y < self.height
            && self.bgra.len() == usize::from(self.width) * usize::from(self.height) * 4
    }
    /// A standard 12×19 arrow (black with white outline) used when the
    /// platform cannot report the real pointer image.
    #[must_use]
    pub fn default_arrow(serial: u32) -> Self {
        const ROWS: [&str; 19] = [
            "X           ",
            "XX          ",
            "X.X         ",
            "X..X        ",
            "X...X       ",
            "X....X      ",
            "X.....X     ",
            "X......X    ",
            "X.......X   ",
            "X........X  ",
            "X.........X ",
            "X......XXXXX",
            "X...X..X    ",
            "X..XX..X    ",
            "X.X  X..X   ",
            "XX   X..X   ",
            "X     X..X  ",
            "      X..X  ",
            "       XX   ",
        ];
        let mut bgra = Vec::with_capacity(12 * 19 * 4);
        for row in ROWS {
            for c in row.bytes() {
                bgra.extend_from_slice(match c {
                    b'X' => &[255, 255, 255, 255],
                    b'.' => &[0, 0, 0, 255],
                    _ => &[0, 0, 0, 0],
                });
            }
        }
        Self {
            serial,
            width: 12,
            height: 19,
            hotspot_x: 0,
            hotspot_y: 0,
            bgra,
        }
    }
    /// Nearest-neighbour resample to `width`×`height` (straight-alpha BGRA).
    #[must_use]
    pub fn resized(&self, width: u32, height: u32) -> Vec<u8> {
        let (sw, sh) = (u32::from(self.width), u32::from(self.height));
        let mut out = vec![0; (width * height * 4) as usize];
        for y in 0..height {
            let sy = (y * sh / height.max(1)).min(sh.saturating_sub(1));
            for x in 0..width {
                let sx = (x * sw / width.max(1)).min(sw.saturating_sub(1));
                let s = ((sy * sw + sx) * 4) as usize;
                let d = ((y * width + x) * 4) as usize;
                out[d..d + 4].copy_from_slice(&self.bgra[s..s + 4]);
            }
        }
        out
    }
}

/// One pointer observation on the Teacher, in capture-relative units.
#[derive(Clone, Debug, PartialEq)]
pub struct PointerSample {
    pub visible: bool,
    /// Hotspot position as a fraction of the captured area (0..=1).
    pub x: f32,
    pub y: f32,
    /// Pointer image width as a fraction of the captured area width.
    pub width_fraction: f32,
    /// Current image; the same `Arc` while unchanged.
    pub image: Option<std::sync::Arc<PointerImage>>,
}

/// Samples the Teacher pointer independently of frame capture cadence.
pub trait PointerSource: Send {
    fn sample(&mut self) -> Option<PointerSample>;
}

/// What a renderer draws over the latest frame (receiver side).
#[derive(Clone, Debug, PartialEq)]
pub struct PointerOverlay {
    pub x: f32,
    pub y: f32,
    pub width_fraction: f32,
    pub image: std::sync::Arc<PointerImage>,
}
impl PointerOverlay {
    /// Pixel rectangle `[left, top, width, height]` (may extend off-frame) for
    /// a frame drawn `frame_width`×`frame_height` pixels.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss
    )]
    pub fn rect(&self, frame_width: u32, frame_height: u32) -> [i64; 4] {
        let image = &self.image;
        let width = (self.width_fraction * frame_width as f32)
            .round()
            .clamp(4.0, 256.0) as i64;
        let height = (width as f32 * f32::from(image.height) / f32::from(image.width).max(1.0))
            .round()
            .max(1.0) as i64;
        let hx = i64::from(image.hotspot_x) * width / i64::from(image.width.max(1));
        let hy = i64::from(image.hotspot_y) * height / i64::from(image.height.max(1));
        let x = (self.x.clamp(0.0, 1.0) * frame_width as f32).round() as i64;
        let y = (self.y.clamp(0.0, 1.0) * frame_height as f32).round() as i64;
        [x - hx, y - hy, width, height]
    }
    /// Alpha-blends the pointer into a BGRA frame in place.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn blend_into_bgra(&self, frame: &mut [u8], frame_width: u32, frame_height: u32) {
        let [left, top, width, height] = self.rect(frame_width, frame_height);
        let pixels = self.image.resized(width as u32, height as u32);
        for row in 0..height {
            let y = top + row;
            if y < 0 || y >= i64::from(frame_height) {
                continue;
            }
            for column in 0..width {
                let x = left + column;
                if x < 0 || x >= i64::from(frame_width) {
                    continue;
                }
                let s = ((row * width + column) * 4) as usize;
                let alpha = u32::from(pixels[s + 3]);
                if alpha == 0 {
                    continue;
                }
                let d = ((y * i64::from(frame_width) + x) * 4) as usize;
                if d + 4 > frame.len() {
                    continue;
                }
                for channel in 0..3 {
                    let fg = u32::from(pixels[s + channel]);
                    let bg = u32::from(frame[d + channel]);
                    frame[d + channel] = ((fg * alpha + bg * (255 - alpha) + 127) / 255) as u8;
                }
                frame[d + 3] = 255;
            }
        }
    }
}

pub trait CaptureSource: Send {
    /// Optional independent pointer sampler. When present, the publisher may
    /// stop compositing the pointer into video (`set_cursor_policy`) and send
    /// it on a separate channel to receivers that draw it themselves.
    fn pointer_source(&self) -> Option<Box<dyn PointerSource>> {
        None
    }
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
    /// O2: cap quantization while refining static content (`true`) or
    /// restore normal rate control (`false`). Returns whether the backend
    /// supports it; unsupported backends only get the bitrate raise.
    ///
    /// # Errors
    /// Returns an error when the backend fails to apply a supported setting.
    fn set_refinement(&mut self, _active: bool) -> Result<bool, MediaError> {
        Ok(false)
    }
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
    /// Pointer to draw over every subsequent presentation (`None` hides it).
    fn set_pointer(&mut self, _pointer: Option<PointerOverlay>) {}
    /// Re-presents the latest frame (e.g. after pointer-only motion).
    /// Returns false when the backend cannot redraw without a new frame.
    ///
    /// # Errors
    /// Returns an error when presentation fails.
    fn redraw(&mut self) -> Result<bool, MediaError> {
        Ok(false)
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
    fn pointer_overlay_scales_with_frame_and_blends_alpha() {
        let image = std::sync::Arc::new(PointerImage::default_arrow(1));
        assert!(image.is_valid());
        let overlay = PointerOverlay {
            x: 0.5,
            y: 0.5,
            width_fraction: 12.0 / 1920.0,
            image,
        };
        // Same fraction on a half-size frame halves the pointer.
        assert_eq!(overlay.rect(1920, 1080), [960, 540, 12, 19]);
        assert_eq!(overlay.rect(960, 540)[2], 6);
        let mut frame = vec![10_u8; 1920 * 1080 * 4];
        overlay.blend_into_bgra(&mut frame, 1920, 1080);
        let at = |x: usize, y: usize| &frame[(y * 1920 + x) * 4..(y * 1920 + x) * 4 + 4];
        assert_eq!(
            at(960, 540),
            &[255, 255, 255, 255],
            "white outline at hotspot"
        );
        assert_eq!(at(961, 542), &[0, 0, 0, 255], "black fill");
        assert_eq!(
            at(970, 541),
            &[10, 10, 10, 10],
            "transparent pixels untouched"
        );
    }
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
