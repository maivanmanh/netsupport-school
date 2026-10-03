#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PointerShapeKind {
    Monochrome,
    Color,
    MaskedColor,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PointerShape<'a> {
    pub kind: PointerShapeKind,
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub bytes: &'a [u8],
}

pub(crate) fn compose_cursor(
    frame: &mut [u8],
    frame_width: u32,
    frame_height: u32,
    row_pitch: u32,
    position_x: i32,
    position_y: i32,
    shape: PointerShape<'_>,
) -> bool {
    let cursor_height = match shape.kind {
        PointerShapeKind::Monochrome => shape.height / 2,
        PointerShapeKind::Color | PointerShapeKind::MaskedColor => shape.height,
    };
    if shape.width == 0 || cursor_height == 0 || shape.pitch == 0 {
        return false;
    }

    let mut changed = false;
    for cursor_y in 0..cursor_height {
        let target_y = position_y + i32::try_from(cursor_y).unwrap_or(i32::MAX);
        if target_y < 0 || target_y >= i32::try_from(frame_height).unwrap_or(i32::MAX) {
            continue;
        }
        for cursor_x in 0..shape.width {
            let target_x = position_x + i32::try_from(cursor_x).unwrap_or(i32::MAX);
            if target_x < 0 || target_x >= i32::try_from(frame_width).unwrap_or(i32::MAX) {
                continue;
            }
            let frame_offset = usize::try_from(target_y).unwrap_or(0)
                * usize::try_from(row_pitch).unwrap_or(0)
                + usize::try_from(target_x).unwrap_or(0) * 4;
            if frame_offset + 4 > frame.len() {
                continue;
            }

            match shape.kind {
                PointerShapeKind::Color => {
                    blend_color(frame, frame_offset, shape, cursor_x, cursor_y);
                }
                PointerShapeKind::MaskedColor => {
                    apply_masked_color(frame, frame_offset, shape, cursor_x, cursor_y);
                }
                PointerShapeKind::Monochrome => {
                    apply_monochrome(frame, frame_offset, shape, cursor_x, cursor_y);
                }
            }
            changed = true;
        }
    }
    changed
}

fn shape_pixel_offset(shape: PointerShape<'_>, x: u32, y: u32) -> Option<usize> {
    let offset = usize::try_from(y).ok()? * usize::try_from(shape.pitch).ok()?
        + usize::try_from(x).ok()? * 4;
    (offset + 4 <= shape.bytes.len()).then_some(offset)
}

fn blend_color(frame: &mut [u8], offset: usize, shape: PointerShape<'_>, x: u32, y: u32) {
    let Some(source) = shape_pixel_offset(shape, x, y) else {
        return;
    };
    let alpha = u32::from(shape.bytes[source + 3]);
    let inverse = 255 - alpha;
    for channel in 0..3 {
        let foreground = u32::from(shape.bytes[source + channel]);
        let background = u32::from(frame[offset + channel]);
        frame[offset + channel] =
            u8::try_from((foreground * alpha + background * inverse + 127) / 255)
                .unwrap_or(u8::MAX);
    }
    frame[offset + 3] = 255;
}

fn apply_masked_color(frame: &mut [u8], offset: usize, shape: PointerShape<'_>, x: u32, y: u32) {
    let Some(source) = shape_pixel_offset(shape, x, y) else {
        return;
    };
    if shape.bytes[source + 3] == 0 {
        frame[offset..offset + 3].copy_from_slice(&shape.bytes[source..source + 3]);
    } else {
        for channel in 0..3 {
            frame[offset + channel] ^= shape.bytes[source + channel];
        }
    }
    frame[offset + 3] = 255;
}

fn apply_monochrome(frame: &mut [u8], offset: usize, shape: PointerShape<'_>, x: u32, y: u32) {
    let byte_x = usize::try_from(x / 8).unwrap_or(usize::MAX);
    let bit = 0x80_u8 >> (x % 8);
    let pitch = usize::try_from(shape.pitch).unwrap_or(0);
    let y = usize::try_from(y).unwrap_or(usize::MAX);
    let half_height = usize::try_from(shape.height / 2).unwrap_or(0);
    let and_offset = y.saturating_mul(pitch).saturating_add(byte_x);
    let xor_offset = y
        .saturating_add(half_height)
        .saturating_mul(pitch)
        .saturating_add(byte_x);
    if and_offset >= shape.bytes.len() || xor_offset >= shape.bytes.len() {
        return;
    }
    let and_is_one = shape.bytes[and_offset] & bit != 0;
    let xor_is_one = shape.bytes[xor_offset] & bit != 0;
    for channel in 0..3 {
        let after_and = if and_is_one {
            frame[offset + channel]
        } else {
            0
        };
        frame[offset + channel] = if xor_is_one {
            after_and ^ 0xff
        } else {
            after_and
        };
    }
    frame[offset + 3] = 255;
}

/// Converts a DXGI pointer shape to straight-alpha BGRA for receivers that
/// draw the pointer themselves. XOR-style pixels (inverting cursors) have no
/// alpha equivalent; they become opaque black/white so they stay visible.
/// Shapes larger than `PointerImage::MAX_SIDE` are nearest-neighbour reduced.
pub(crate) fn to_image(
    shape: PointerShape<'_>,
    serial: u32,
    hotspot: (u32, u32),
) -> Option<media_core::PointerImage> {
    let height = match shape.kind {
        PointerShapeKind::Monochrome => shape.height / 2,
        PointerShapeKind::Color | PointerShapeKind::MaskedColor => shape.height,
    };
    let width = shape.width;
    if width == 0 || height == 0 || shape.pitch == 0 {
        return None;
    }
    let mut bgra = vec![0_u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let d = ((y * width + x) * 4) as usize;
            let pixel: [u8; 4] = match shape.kind {
                PointerShapeKind::Color => {
                    let s = shape_pixel_offset(shape, x, y)?;
                    [
                        shape.bytes[s],
                        shape.bytes[s + 1],
                        shape.bytes[s + 2],
                        shape.bytes[s + 3],
                    ]
                }
                PointerShapeKind::MaskedColor => {
                    let s = shape_pixel_offset(shape, x, y)?;
                    let color = [shape.bytes[s], shape.bytes[s + 1], shape.bytes[s + 2]];
                    if shape.bytes[s + 3] == 0 {
                        [color[0], color[1], color[2], 255]
                    } else if color == [0, 0, 0] {
                        [0, 0, 0, 0] // XOR with zero: screen unchanged
                    } else {
                        [color[0], color[1], color[2], 255]
                    }
                }
                PointerShapeKind::Monochrome => {
                    let pitch = shape.pitch as usize;
                    let byte = (x / 8) as usize;
                    let bit = 0x80_u8 >> (x % 8);
                    let and_offset = y as usize * pitch + byte;
                    let xor_offset = (y + height) as usize * pitch + byte;
                    let and_one = shape.bytes.get(and_offset)? & bit != 0;
                    let xor_one = shape.bytes.get(xor_offset)? & bit != 0;
                    match (and_one, xor_one) {
                        (true, false) => [0, 0, 0, 0],
                        (false, true) => [255, 255, 255, 255],
                        // Black, or an inverted pixel drawn dark so it reads
                        // on typical light content.
                        (false, false) | (true, true) => [0, 0, 0, 255],
                    }
                }
            };
            bgra[d..d + 4].copy_from_slice(&pixel);
        }
    }
    let max = u32::from(media_core::PointerImage::MAX_SIDE);
    let scale = width.max(height).div_ceil(max).max(1);
    let image = media_core::PointerImage {
        serial,
        width: u16::try_from(width).ok()?,
        height: u16::try_from(height).ok()?,
        hotspot_x: u16::try_from(hotspot.0.min(width - 1)).ok()?,
        hotspot_y: u16::try_from(hotspot.1.min(height - 1)).ok()?,
        bgra,
    };
    if scale == 1 {
        return Some(image);
    }
    let (w, h) = ((width / scale).max(1), (height / scale).max(1));
    Some(media_core::PointerImage {
        serial,
        width: u16::try_from(w).ok()?,
        height: u16::try_from(h).ok()?,
        hotspot_x: u16::try_from((hotspot.0 / scale).min(w - 1)).ok()?,
        hotspot_y: u16::try_from((hotspot.1 / scale).min(h - 1)).ok()?,
        bgra: image.resized(w, h),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_cursor_alpha_blends_bgra() {
        let mut frame = vec![0_u8, 0, 0, 255];
        let shape = PointerShape {
            kind: PointerShapeKind::Color,
            width: 1,
            height: 1,
            pitch: 4,
            bytes: &[100, 50, 20, 128],
        };
        assert!(compose_cursor(&mut frame, 1, 1, 4, 0, 0, shape));
        assert_eq!(frame, [50, 25, 10, 255]);
    }

    #[test]
    fn masked_color_cursor_replaces_or_xors() {
        let mut replace = vec![10_u8, 20, 30, 255];
        let shape = PointerShape {
            kind: PointerShapeKind::MaskedColor,
            width: 1,
            height: 1,
            pitch: 4,
            bytes: &[1, 2, 3, 0],
        };
        compose_cursor(&mut replace, 1, 1, 4, 0, 0, shape);
        assert_eq!(replace, [1, 2, 3, 255]);

        let mut xor = vec![0x0f_u8, 0xf0, 0xaa, 255];
        let xor_shape = PointerShape {
            bytes: &[0xff, 0x0f, 0xaa, 255],
            ..shape
        };
        compose_cursor(&mut xor, 1, 1, 4, 0, 0, xor_shape);
        assert_eq!(xor, [0xf0, 0xff, 0x00, 255]);
    }

    #[test]
    fn monochrome_cursor_applies_and_then_xor_masks() {
        let mut frame = vec![10_u8, 20, 30, 255];
        let shape = PointerShape {
            kind: PointerShapeKind::Monochrome,
            width: 1,
            height: 2,
            pitch: 1,
            bytes: &[0x00, 0x80],
        };
        compose_cursor(&mut frame, 1, 1, 4, 0, 0, shape);
        assert_eq!(frame, [255, 255, 255, 255]);
    }

    #[test]
    fn cursor_is_clipped_at_frame_edges() {
        let mut frame = vec![0_u8; 2 * 4];
        let shape = PointerShape {
            kind: PointerShapeKind::Color,
            width: 2,
            height: 1,
            pitch: 8,
            bytes: &[1, 2, 3, 255, 4, 5, 6, 255],
        };
        compose_cursor(&mut frame, 2, 1, 8, -1, 0, shape);
        assert_eq!(&frame[0..4], &[4, 5, 6, 255]);
    }

    #[test]
    fn shapes_convert_to_straight_alpha_images() {
        let color = PointerShape {
            kind: PointerShapeKind::Color,
            width: 2,
            height: 1,
            pitch: 8,
            bytes: &[1, 2, 3, 200, 9, 9, 9, 0],
        };
        let image = to_image(color, 4, (1, 0)).unwrap();
        assert!(image.is_valid());
        assert_eq!(image.bgra, [1, 2, 3, 200, 9, 9, 9, 0]);
        assert_eq!((image.serial, image.hotspot_x), (4, 1));
        // Monochrome 8x1: AND row then XOR row.
        let mono = PointerShape {
            kind: PointerShapeKind::Monochrome,
            width: 8,
            height: 2,
            pitch: 1,
            bytes: &[0b1010_0000, 0b0110_0000],
        };
        let image = to_image(mono, 1, (0, 0)).unwrap();
        // x0: AND1 XOR0 transparent; x1: AND0 XOR1 white; x2: AND1 XOR1 dark; x3: black
        assert_eq!(&image.bgra[0..4], &[0, 0, 0, 0]);
        assert_eq!(&image.bgra[4..8], &[255, 255, 255, 255]);
        assert_eq!(&image.bgra[8..12], &[0, 0, 0, 255]);
        assert_eq!(&image.bgra[12..16], &[0, 0, 0, 255]);
    }
    #[test]
    fn oversized_shapes_are_reduced_within_bounds() {
        let bytes = vec![255_u8; 256 * 256 * 4];
        let big = PointerShape {
            kind: PointerShapeKind::Color,
            width: 256,
            height: 256,
            pitch: 1024,
            bytes: &bytes,
        };
        let image = to_image(big, 1, (200, 10)).unwrap();
        assert!(image.is_valid());
        assert_eq!(
            (image.width, image.hotspot_x, image.hotspot_y),
            (128, 100, 5)
        );
    }
}
