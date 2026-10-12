use media_core::MediaError;

/// Largest even H.264/NV12 size inside both the source and configured limits.
pub(crate) fn fit_even(source: (u32, u32), limit: (u32, u32)) -> Result<(u32, u32), MediaError> {
    let (sw, sh) = source;
    let (mw, mh) = (limit.0.min(1920), limit.1.min(1080));
    if sw < 2 || sh < 2 || mw < 2 || mh < 2 {
        return Err(MediaError::Operation(
            "source display and video limits must each be at least 2x2".to_owned(),
        ));
    }
    let (bound_w, bound_h) = (sw.min(mw) & !1, sh.min(mh) & !1);
    let width_limited = u64::from(bound_w) * u64::from(sh) <= u64::from(bound_h) * u64::from(sw);
    let (width, height) = if width_limited {
        (
            bound_w,
            nearest_even(u64::from(bound_w) * u64::from(sh), sw, bound_h),
        )
    } else {
        (
            nearest_even(u64::from(bound_h) * u64::from(sw), sh, bound_w),
            bound_h,
        )
    };
    Ok((width.max(2), height.max(2)))
}

fn nearest_even(numerator: u64, denominator: u32, maximum: u32) -> u32 {
    let rounded = ((numerator + u64::from(denominator)) / (2 * u64::from(denominator))) * 2;
    u32::try_from(rounded).unwrap_or(maximum).clamp(2, maximum)
}

/// Bilinear downsampling after cursor composition; input and output are packed BGRA.
// Coordinates and byte offsets are bounded by the validated source/output buffer sizes.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(crate) fn resize_bgra(
    source: &[u8],
    sw: u32,
    sh: u32,
    dw: u32,
    dh: u32,
) -> Result<Vec<u8>, MediaError> {
    let byte_len = |width, height| {
        u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|pixels| pixels.checked_mul(4))
            .and_then(|bytes| usize::try_from(bytes).ok())
            .ok_or_else(|| MediaError::Operation("image size overflowed".to_owned()))
    };
    let source_len = byte_len(sw, sh)?;
    let output_len = byte_len(dw, dh)?;
    if source.len() < source_len || dw == 0 || dh == 0 || sw == 0 || sh == 0 {
        return Err(MediaError::Operation(
            "invalid BGRA image for resize".to_owned(),
        ));
    }
    let coordinate = |index: u32, source: u32, destination: u32| {
        let position =
            ((f64::from(index) + 0.5) * f64::from(source) / f64::from(destination) - 0.5).max(0.0);
        let first = (position as u32).min(source - 1);
        let next = (first + 1).min(source - 1);
        let weight = ((position - f64::from(first)) * 256.0).round() as u32;
        (first as usize, next as usize, weight.min(256))
    };
    let mut output = vec![0; output_len];
    if u64::from(sw) == u64::from(dw) * 2 && u64::from(sh) == u64::from(dh) * 2 {
        // Exact 2:1 bilinear sampling is the rounded mean of four pixels. Sum
        // interleaved channels in padded 16-bit lanes to avoid cross-channel carry.
        // This preserves the existing pixels while avoiding scalar multiplies at 4K.
        for (y, row) in output.chunks_exact_mut(dw as usize * 4).enumerate() {
            let source_offset = y * 2 * sw as usize * 4;
            let row0 = &source[source_offset..source_offset + sw as usize * 4];
            let row1 = &source[source_offset + sw as usize * 4..source_offset + sw as usize * 8];
            for (x, pixel) in row.chunks_exact_mut(4).enumerate() {
                let offset = x * 8;
                let read = |row: &[u8], start| {
                    u32::from_le_bytes(
                        row[start..start + 4]
                            .try_into()
                            .expect("validated pixel size"),
                    )
                };
                let values = [
                    read(row0, offset),
                    read(row0, offset + 4),
                    read(row1, offset),
                    read(row1, offset + 4),
                ];
                let lanes = |shift: u32| {
                    let sum: u32 = values
                        .iter()
                        .map(|value| (*value >> shift) & 0x00ff_00ff_u32)
                        .sum();
                    ((sum + 0x0002_0002) >> 2) & 0x00ff_00ff
                };
                pixel.copy_from_slice(&(lanes(0) | (lanes(8) << 8)).to_le_bytes());
            }
        }
        return Ok(output);
    }
    // Reuse horizontal coordinates for every row. Eight-bit fractional weights avoid
    // floating-point interpolation and rounding four times for every output pixel.
    let columns: Vec<_> = (0..dw).map(|x| coordinate(x, sw, dw)).collect();
    for (y, row) in output.chunks_exact_mut(dw as usize * 4).enumerate() {
        let (y0, y1, wy) = coordinate(y as u32, sh, dh);
        let row0 = &source[y0 * sw as usize * 4..(y0 + 1) * sw as usize * 4];
        let row1 = &source[y1 * sw as usize * 4..(y1 + 1) * sw as usize * 4];
        for (pixel, &(x0, x1, wx)) in row.chunks_exact_mut(4).zip(&columns) {
            for channel in 0..4 {
                let a = u32::from(row0[x0 * 4 + channel]) * (256 - wx)
                    + u32::from(row0[x1 * 4 + channel]) * wx;
                let b = u32::from(row1[x0 * 4 + channel]) * (256 - wx)
                    + u32::from(row1[x1 * 4 + channel]) * wx;
                pixel[channel] = ((a * (256 - wy) + b * wy + 32768) >> 16) as u8;
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_aspect_without_upscaling_or_exceeding_limit() {
        assert_eq!(fit_even((1920, 1080), (1920, 1080)).unwrap(), (1920, 1080));
        assert_eq!(fit_even((1280, 800), (1920, 1080)).unwrap(), (1280, 800));
        assert_eq!(fit_even((2560, 1600), (1920, 1080)).unwrap(), (1728, 1080));
        assert_eq!(fit_even((3440, 1440), (1920, 1080)).unwrap(), (1920, 804));
        assert_eq!(fit_even((1365, 767), (1920, 1080)).unwrap(), (1364, 766));
        assert_eq!(fit_even((3840, 2160), (3840, 2160)).unwrap(), (1920, 1080));
        assert_eq!(fit_even((1366, 768), (1920, 1080)).unwrap(), (1366, 768));
        assert_eq!(fit_even((1080, 1920), (1920, 1080)).unwrap(), (608, 1080));
        assert_eq!(fit_even((2560, 1440), (1280, 720)).unwrap(), (1280, 720));
        assert!(fit_even((0, 1080), (1920, 1080)).is_err());
    }

    #[test]
    fn recording_720p_preserves_portrait_ultrawide_and_small_displays() {
        assert_eq!(fit_even((1920, 1080), (1280, 720)).unwrap(), (1280, 720));
        assert_eq!(fit_even((1600, 1200), (1280, 720)).unwrap(), (960, 720));
        assert_eq!(fit_even((1080, 1920), (1280, 720)).unwrap(), (406, 720));
        assert_eq!(fit_even((2560, 1080), (1280, 720)).unwrap(), (1280, 540));
        assert_eq!(fit_even((800, 600), (1280, 720)).unwrap(), (800, 600));
    }

    #[test]
    fn resize_preserves_constant_bgra_and_size() {
        let input = [12, 34, 56, 255].repeat(16);
        assert_eq!(
            resize_bgra(&input, 4, 4, 2, 2).unwrap(),
            [12, 34, 56, 255].repeat(4)
        );
    }

    #[test]
    fn downsampling_averages_pixels_without_cropping() {
        let input: Vec<u8> = (0_u8..16)
            .flat_map(|value| [value * 10, 0, 0, 255])
            .collect();
        let output = resize_bgra(&input, 4, 4, 2, 2).unwrap();
        assert_eq!(
            output,
            [25, 0, 0, 255, 45, 0, 0, 255, 105, 0, 0, 255, 125, 0, 0, 255]
        );
        assert!(resize_bgra(&input[..8], 4, 4, 2, 2).is_err());
        assert!(resize_bgra(&[], u32::MAX, u32::MAX, 2, 2).is_err());
    }

    #[test]
    fn exact_half_scale_preserves_channel_rounding_and_row_boundaries() {
        let block = [
            255, 254, 253, 252, 0, 1, 2, 3, 100, 101, 102, 103, 200, 201, 202, 203,
        ];
        assert_eq!(
            resize_bgra(&block, 2, 2, 1, 1).unwrap(),
            [139, 139, 140, 140]
        );
        let rows = [255_u8; 4 * 4 * 2];
        assert_eq!(resize_bgra(&rows, 4, 2, 2, 1).unwrap(), [255_u8; 8]);
        let dark_image = [0_u8; 4 * 4 * 4];
        assert_eq!(resize_bgra(&dark_image, 4, 4, 2, 2).unwrap(), [0_u8; 16]);
    }
}
