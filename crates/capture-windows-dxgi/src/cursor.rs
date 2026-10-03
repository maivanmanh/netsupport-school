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
}
