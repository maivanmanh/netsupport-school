//! macOS Teacher pointer sampler: position from CoreGraphics, image from
//! `NSCursor.currentSystemCursor` (deprecated by Apple and may return nil in a
//! future macOS — then a standard arrow is used so the pointer stays visible).
use media_core::{PointerImage, PointerSample, PointerSource};
use objc2_app_kit::{NSBitmapFormat, NSBitmapImageRep, NSCursor};
use std::{
    ffi::c_void,
    hash::{Hash, Hasher},
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

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
    fn CGEventCreate(source: *const c_void) -> *mut c_void;
    fn CGEventGetLocation(event: *mut c_void) -> CGPoint;
    fn CGDisplayBounds(display: u32) -> CGRect;
    fn CGCursorIsVisible() -> u8;
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(value: *const c_void);
}

const SHAPE_POLL: Duration = Duration::from_millis(120);

pub struct MacPointerSource {
    display: Arc<AtomicU32>,
    image: Option<Arc<PointerImage>>,
    /// Image size in points, for the width fraction.
    image_points: f64,
    shape_hash: u64,
    serial: u32,
    last_shape_poll: Option<Instant>,
}
impl MacPointerSource {
    pub fn new(display: Arc<AtomicU32>) -> Self {
        Self {
            display,
            image: None,
            image_points: 12.0,
            shape_hash: 0,
            serial: 0,
            last_shape_poll: None,
        }
    }
    fn refresh_shape(&mut self) {
        if self
            .last_shape_poll
            .is_some_and(|at| at.elapsed() < SHAPE_POLL)
        {
            return;
        }
        self.last_shape_poll = Some(Instant::now());
        let current = objc2::rc::autoreleasepool(|_| current_cursor());
        match current {
            Some((hash, image_points, build)) => {
                if hash != self.shape_hash || self.image.is_none() {
                    self.serial = self.serial.wrapping_add(1);
                    if let Some(image) = build(self.serial) {
                        self.shape_hash = hash;
                        self.image_points = image_points;
                        self.image = Some(Arc::new(image));
                    }
                }
            }
            None if self.image.is_none() => {
                self.serial = self.serial.wrapping_add(1);
                self.image = Some(Arc::new(PointerImage::default_arrow(self.serial)));
                self.image_points = 12.0;
            }
            None => {}
        }
    }
}

type Builder = Box<dyn FnOnce(u32) -> Option<PointerImage>>;

/// (pixel hash, width in points, image builder) for the current system cursor.
#[allow(
    deprecated,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::many_single_char_names,
    clippy::too_many_lines
)]
fn current_cursor() -> Option<(u64, f64, Builder)> {
    let cursor = NSCursor::currentSystemCursor()?;
    let image = cursor.image();
    let points = image.size();
    let hotspot = cursor.hotSpot();
    let data = image.TIFFRepresentation()?;
    let rep = NSBitmapImageRep::imageRepWithData(&data)?;
    let (width, height) = (rep.pixelsWide(), rep.pixelsHigh());
    let (samples, bits, row) = (rep.samplesPerPixel(), rep.bitsPerPixel(), rep.bytesPerRow());
    if width <= 0 || height <= 0 || samples != 4 || bits != 32 || row < width * 4 {
        return None;
    }
    let format = rep.bitmapFormat();
    let pointer = rep.bitmapData();
    if pointer.is_null() || points.width <= 0.0 {
        return None;
    }
    let (width, height, row) = (width as usize, height as usize, row as usize);
    // SAFETY: NSBitmapImageRep owns `height * bytesPerRow` bytes at bitmapData
    // for its lifetime; we copy them out before `rep` drops.
    let raw = unsafe { std::slice::from_raw_parts(pointer, row * height) }.to_vec();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    raw.hash(&mut hasher);
    (hotspot.x.to_bits(), hotspot.y.to_bits()).hash(&mut hasher);
    let hash = hasher.finish();
    let alpha_first = format.contains(NSBitmapFormat::AlphaFirst);
    let premultiplied = !format.contains(NSBitmapFormat::AlphaNonpremultiplied);
    let scale = width as f64 / points.width;
    let hot = (
        (hotspot.x * scale).max(0.0) as u32,
        (hotspot.y * scale).max(0.0) as u32,
    );
    let build: Builder = Box::new(move |serial| {
        let mut bgra = vec![0_u8; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                let s = y * row + x * 4;
                let (r, g, b, a) = if alpha_first {
                    (raw[s + 1], raw[s + 2], raw[s + 3], raw[s])
                } else {
                    (raw[s], raw[s + 1], raw[s + 2], raw[s + 3])
                };
                let unpremultiply = |c: u8| {
                    if premultiplied && a > 0 && a < 255 {
                        ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8
                    } else {
                        c
                    }
                };
                let d = (y * width + x) * 4;
                bgra[d..d + 4].copy_from_slice(&[
                    unpremultiply(b),
                    unpremultiply(g),
                    unpremultiply(r),
                    a,
                ]);
            }
        }
        let max = usize::from(PointerImage::MAX_SIDE);
        let factor = width.max(height).div_ceil(max).max(1);
        let image = PointerImage {
            serial,
            width: u16::try_from(width).ok()?,
            height: u16::try_from(height).ok()?,
            hotspot_x: u16::try_from((hot.0 as usize).min(width - 1)).ok()?,
            hotspot_y: u16::try_from((hot.1 as usize).min(height - 1)).ok()?,
            bgra,
        };
        if factor == 1 {
            return Some(image);
        }
        let (w, h) = ((width / factor).max(1), (height / factor).max(1));
        Some(PointerImage {
            serial,
            width: u16::try_from(w).ok()?,
            height: u16::try_from(h).ok()?,
            hotspot_x: u16::try_from((hot.0 as usize / factor).min(w - 1)).ok()?,
            hotspot_y: u16::try_from((hot.1 as usize / factor).min(h - 1)).ok()?,
            bgra: image.resized(w as u32, h as u32),
        })
    });
    Some((hash, points.width, build))
}

impl PointerSource for MacPointerSource {
    #[allow(clippy::cast_possible_truncation)]
    fn sample(&mut self) -> Option<PointerSample> {
        self.refresh_shape();
        // SAFETY: CoreGraphics returns an owned event (released below) and
        // value-type geometry; null is handled.
        let (location, bounds, visible) = unsafe {
            let event = CGEventCreate(std::ptr::null());
            if event.is_null() {
                return None;
            }
            let location = CGEventGetLocation(event);
            CFRelease(event);
            (
                location,
                CGDisplayBounds(self.display.load(Ordering::Acquire)),
                CGCursorIsVisible() != 0,
            )
        };
        let (w, h) = (bounds.size.width, bounds.size.height);
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        let x = (location.x - bounds.origin.x) / w;
        let y = (location.y - bounds.origin.y) / h;
        let inside = (0.0..1.0).contains(&x) && (0.0..1.0).contains(&y);
        Some(PointerSample {
            visible: visible && inside && self.image.is_some(),
            x: x as f32,
            y: y as f32,
            width_fraction: (self.image_points / w) as f32,
            image: self.image.clone(),
        })
    }
}
