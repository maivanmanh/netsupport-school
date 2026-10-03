use std::{
    cell::Cell,
    ptr,
    time::{Duration, Instant},
};

use media_core::{DecodedFrameRef, MediaError, Renderer};
use minifb::{ScaleMode, Window, WindowOptions};

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct CGRect {
    origin: CGPoint,
    size: CGSize,
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGMainDisplayID() -> u32;
    fn CGDisplayBounds(display: u32) -> CGRect;
}

#[derive(Debug)]
pub struct NativeRenderer {
    window: Window,
    pixels: Vec<u32>,
    width: usize,
    height: usize,
    forced_presentation: bool,
    last_topmost_maintenance: Cell<Instant>,
}

impl NativeRenderer {
    /// Creates a resizable native preview window.
    ///
    /// # Errors
    /// Returns an error when the dimensions are invalid or macOS cannot create the window.
    pub fn new(width: u32, height: u32, title: &str) -> Result<Self, MediaError> {
        Self::create(width, height, title, None)
    }

    /// Creates the borderless, always-on-top Student presentation surface.
    ///
    /// # Errors
    /// Returns an error when dimensions are invalid or macOS cannot create the window.
    pub fn new_student_presentation(
        width: u32,
        height: u32,
        title: &str,
    ) -> Result<Self, MediaError> {
        Self::create(width, height, title, Some(main_display_dimensions()?))
    }

    fn create(
        width: u32,
        height: u32,
        title: &str,
        presentation_size: Option<(usize, usize)>,
    ) -> Result<Self, MediaError> {
        let width = checked_dimension(width, "width")?;
        let height = checked_dimension(height, "height")?;
        let (window_width, window_height) = presentation_size.unwrap_or((width, height));
        let forced_presentation = presentation_size.is_some();
        let mut window = Window::new(
            title,
            window_width,
            window_height,
            WindowOptions {
                borderless: forced_presentation,
                title: !forced_presentation,
                resize: !forced_presentation,
                scale_mode: ScaleMode::AspectRatioStretch,
                topmost: forced_presentation,
                ..WindowOptions::default()
            },
        )
        .map_err(operation)?;
        if forced_presentation {
            window.set_position(0, 0);
            window.topmost(true);
        }
        window.set_background_color(0, 0, 0);
        window.set_target_fps(0);
        Ok(Self {
            window,
            pixels: vec![0; width.saturating_mul(height)],
            width,
            height,
            forced_presentation,
            last_topmost_maintenance: Cell::new(Instant::now()),
        })
    }

    #[must_use]
    pub fn is_open(&self) -> bool {
        // This is polled about every millisecond. Match Windows' one-second
        // maintenance cadence to avoid redundant native window-level calls.
        if self.forced_presentation
            && self.last_topmost_maintenance.get().elapsed() >= Duration::from_secs(1)
        {
            self.window.topmost(true);
            self.last_topmost_maintenance.set(Instant::now());
        }
        self.window.is_open()
    }
}

fn checked_dimension(value: u32, name: &str) -> Result<usize, MediaError> {
    usize::try_from(value).map_err(|_| MediaError::Operation(format!("render {name} is too large")))
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn main_display_dimensions() -> Result<(usize, usize), MediaError> {
    // SAFETY: CoreGraphics returns the bounds value for the process's main display.
    let bounds = unsafe { CGDisplayBounds(CGMainDisplayID()) };
    if !bounds.size.width.is_finite()
        || !bounds.size.height.is_finite()
        || bounds.size.width < 1.0
        || bounds.size.height < 1.0
        || bounds.size.width > f64::from(u32::MAX)
        || bounds.size.height > f64::from(u32::MAX)
    {
        return Err(MediaError::Operation(
            "macOS main display reported invalid dimensions".to_owned(),
        ));
    }
    Ok((
        bounds.size.width.round() as usize,
        bounds.size.height.round() as usize,
    ))
}

impl Renderer for NativeRenderer {
    fn render(&mut self, frame: DecodedFrameRef<'_>) -> Result<(), MediaError> {
        let width = usize::try_from(frame.width)
            .map_err(|_| MediaError::Operation("decoded width is too large".to_owned()))?;
        let height = usize::try_from(frame.height)
            .map_err(|_| MediaError::Operation("decoded height is too large".to_owned()))?;
        let pixel_count = width
            .checked_mul(height)
            .ok_or_else(|| MediaError::Operation("decoded dimensions overflow".to_owned()))?;
        let byte_count = pixel_count
            .checked_mul(4)
            .ok_or_else(|| MediaError::Operation("decoded byte count overflows".to_owned()))?;
        if frame.bytes.len() != byte_count {
            return Err(MediaError::Operation(format!(
                "decoded BGRA length mismatch: got {}, expected {byte_count}",
                frame.bytes.len()
            )));
        }
        if self.width != width || self.height != height {
            self.width = width;
            self.height = height;
            self.pixels.resize(pixel_count, 0);
        }
        // The decoded pixels are tightly packed BGRA and minifb's macOS backend
        // uploads its buffer as a BGRA Metal texture. Preserve its persistent backing
        // allocation (minifb draws asynchronously) and copy the bytes directly instead
        // of unpacking/repacking every channel on the Student media thread.
        // SAFETY: frame length was checked against `pixel_count * 4`; `self.pixels` was
        // resized to `pixel_count`, so its allocation has at least `byte_count` bytes.
        // The source frame and renderer-owned destination are distinct allocations.
        unsafe {
            ptr::copy_nonoverlapping(
                frame.bytes.as_ptr(),
                self.pixels.as_mut_ptr().cast::<u8>(),
                byte_count,
            );
        }
        self.window
            .update_with_buffer(&self.pixels, self.width, self.height)
            .map_err(operation)
    }
}

fn operation(error: impl std::fmt::Display) -> MediaError {
    MediaError::Operation(format!("native macOS renderer failed: {error}"))
}
