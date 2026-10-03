use media_core::MediaError;

/// Select the layout of bytes actually copied, not the decoder surface's default pitch.
pub(crate) fn copied_nv12_stride(
    byte_count: usize,
    width: u32,
    height: u32,
    reported_stride: Option<u32>,
    packed_2d: bool,
) -> Result<u32, MediaError> {
    validate_even_dimensions(width, height)?;
    let layout_size = |stride: u32| {
        u64::from(stride)
            .checked_mul(u64::from(height))
            .and_then(|luma| luma.checked_add(luma / 2))
            .and_then(|bytes| usize::try_from(bytes).ok())
    };
    if layout_size(width) == Some(byte_count) {
        return Ok(width);
    }
    if packed_2d {
        return Err(MediaError::Operation(
            "2D NV12 packed copy does not match storage dimensions".to_owned(),
        ));
    }
    if let Some(stride) = reported_stride.filter(|stride| *stride >= width && *stride % 2 == 0) {
        if layout_size(stride) == Some(byte_count) {
            return Ok(stride);
        }
    }
    let inferred = byte_count
        .checked_mul(2)
        .and_then(|bytes| bytes.checked_div(usize::try_from(height).ok()?.checked_mul(3)?))
        .and_then(|stride| u32::try_from(stride).ok());
    if let Some(stride) = inferred.filter(|stride| *stride >= width && *stride % 2 == 0) {
        if layout_size(stride) == Some(byte_count) {
            return Ok(stride);
        }
    }
    Err(MediaError::Operation(
        "NV12 buffer length does not describe a valid row layout".to_owned(),
    ))
}

pub(crate) fn bgra_to_nv12(bgra: &[u8], width: u32, height: u32) -> Result<Vec<u8>, MediaError> {
    validate_even_dimensions(width, height)?;
    let width = usize::try_from(width).map_err(|_| size_error())?;
    let height = usize::try_from(height).map_err(|_| size_error())?;
    let pixels = width.checked_mul(height).ok_or_else(size_error)?;
    let bgra_len = pixels.checked_mul(4).ok_or_else(size_error)?;
    if bgra.len() < bgra_len {
        return Err(MediaError::Operation(format!(
            "BGRA frame is too short: expected {bgra_len} bytes, got {}",
            bgra.len()
        )));
    }
    let output_len = pixels.checked_add(pixels / 2).ok_or_else(size_error)?;
    let mut nv12 = vec![0_u8; output_len];

    // Keep the scalar implementation as the portable fallback. On x86,
    // accelerate luma and chroma when the CPU has byte-shuffle and packing
    // instructions.
    #[cfg(target_arch = "x86_64")]
    if std::arch::is_x86_feature_detected!("ssse3") && std::arch::is_x86_feature_detected!("sse4.1")
    {
        // SAFETY: runtime feature detection establishes both target features;
        // dimensions and source/output lengths were validated above.
        unsafe { bgra_to_nv12_sse41(bgra, width, height, &mut nv12) };
        return Ok(nv12);
    }

    bgra_to_nv12_scalar_validated(&bgra[..bgra_len], width, height, &mut nv12);
    Ok(nv12)
}

fn bgra_to_nv12_scalar_validated(bgra: &[u8], width: usize, height: usize, nv12: &mut [u8]) {
    // Input/output sizes are validated by the caller. Slice chunks avoid
    // repeated full-frame reads and per-pixel row/index arithmetic.
    let pixels = width * height;
    let (luma, chroma) = nv12.split_at_mut(pixels);
    for ((source, target), uv_row) in bgra
        .chunks_exact(width * 8)
        .zip(luma.chunks_exact_mut(width * 2))
        .zip(chroma.chunks_exact_mut(width))
    {
        let (top, bottom) = source.split_at(width * 4);
        let (y_top, y_bottom) = target.split_at_mut(width);
        for ((((top, bottom), y_top), y_bottom), uv) in top
            .chunks_exact(8)
            .zip(bottom.chunks_exact(8))
            .zip(y_top.chunks_exact_mut(2))
            .zip(y_bottom.chunks_exact_mut(2))
            .zip(uv_row.chunks_exact_mut(2))
        {
            let colors = [
                (i32::from(top[2]), i32::from(top[1]), i32::from(top[0])),
                (i32::from(top[6]), i32::from(top[5]), i32::from(top[4])),
                (
                    i32::from(bottom[2]),
                    i32::from(bottom[1]),
                    i32::from(bottom[0]),
                ),
                (
                    i32::from(bottom[6]),
                    i32::from(bottom[5]),
                    i32::from(bottom[4]),
                ),
            ];
            let y_value = |(r, g, b)| clamp_u8(((47 * r + 157 * g + 16 * b + 128) >> 8) + 16);
            y_top[0] = y_value(colors[0]);
            y_top[1] = y_value(colors[1]);
            y_bottom[0] = y_value(colors[2]);
            y_bottom[1] = y_value(colors[3]);
            let (r, g, b) = colors.iter().fold((0, 0, 0), |sum, color| {
                (sum.0 + color.0, sum.1 + color.1, sum.2 + color.2)
            });
            let (r, g, b) = (r / 4, g / 4, b / 4);
            uv[0] = clamp_u8(((-26 * r - 86 * g + 112 * b + 128) >> 8) + 128);
            uv[1] = clamp_u8(((112 * r - 102 * g - 10 * b + 128) >> 8) + 128);
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "ssse3,sse4.1")]
#[allow(clippy::too_many_lines)]
unsafe fn bgra_to_nv12_sse41(bgra: &[u8], width: usize, height: usize, nv12: &mut [u8]) {
    use std::arch::x86_64::{
        __m128i, _mm_add_epi32, _mm_cvtepu8_epi32, _mm_loadu_si128, _mm_mullo_epi32,
        _mm_packus_epi16, _mm_packus_epi32, _mm_set1_epi32, _mm_setr_epi8, _mm_shuffle_epi8,
        _mm_srai_epi32, _mm_srli_epi32, _mm_storeu_si32, _mm_unpacklo_epi8,
    };

    #[inline]
    unsafe fn luma4(
        source: *const u8,
        shuffle: __m128i,
        weight: __m128i,
        rounding: __m128i,
        offset: __m128i,
    ) -> __m128i {
        // SAFETY: caller supplies four complete BGRA pixels and the constants
        // are valid SIMD values.
        unsafe {
            let pixel = _mm_loadu_si128(source.cast());
            let red = _mm_cvtepu8_epi32(_mm_shuffle_epi8(pixel, shuffle));
            let green = _mm_cvtepu8_epi32(_mm_shuffle_epi8(
                pixel,
                _mm_setr_epi8(1, 5, 9, 13, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1),
            ));
            let blue = _mm_cvtepu8_epi32(_mm_shuffle_epi8(
                pixel,
                _mm_setr_epi8(0, 4, 8, 12, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1),
            ));
            let sum = _mm_add_epi32(
                _mm_add_epi32(
                    _mm_mullo_epi32(red, weight),
                    _mm_mullo_epi32(green, _mm_set1_epi32(157)),
                ),
                _mm_add_epi32(_mm_mullo_epi32(blue, _mm_set1_epi32(16)), rounding),
            );
            let value = _mm_add_epi32(_mm_srli_epi32(sum, 8), offset);
            _mm_packus_epi16(
                _mm_packus_epi32(value, _mm_set1_epi32(0)),
                _mm_set1_epi32(0),
            )
        }
    }

    #[inline]
    unsafe fn chroma2(top: *const u8, bottom: *const u8, destination: *mut u8) {
        use std::arch::x86_64::{_mm_add_epi32, _mm_cvtepu8_epi32, _mm_loadu_si128};
        // Two four-pixel loads provide two adjacent 2x2 chroma blocks per row.
        // The masks gather even/odd columns for B, G and R into the low lanes.
        let (top_pixels, bottom_pixels) =
            unsafe { (_mm_loadu_si128(top.cast()), _mm_loadu_si128(bottom.cast())) };
        let channel_sum = |pixels, even, odd| unsafe {
            _mm_add_epi32(
                _mm_cvtepu8_epi32(_mm_shuffle_epi8(pixels, even)),
                _mm_cvtepu8_epi32(_mm_shuffle_epi8(pixels, odd)),
            )
        };
        let blue_even = _mm_setr_epi8(0, 8, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1);
        let blue_odd = _mm_setr_epi8(
            4, 12, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
        );
        let green_even =
            _mm_setr_epi8(1, 9, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1);
        let green_odd = _mm_setr_epi8(
            5, 13, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
        );
        let red_even = _mm_setr_epi8(
            2, 10, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
        );
        let red_odd = _mm_setr_epi8(
            6, 14, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1,
        );
        // Shift the four-pixel totals right by two exactly matches integer
        // averaging in the scalar path (no rounding at this stage).
        let (blue, green, red) = unsafe {
            (
                _mm_srli_epi32(
                    _mm_add_epi32(
                        channel_sum(top_pixels, blue_even, blue_odd),
                        channel_sum(bottom_pixels, blue_even, blue_odd),
                    ),
                    2,
                ),
                _mm_srli_epi32(
                    _mm_add_epi32(
                        channel_sum(top_pixels, green_even, green_odd),
                        channel_sum(bottom_pixels, green_even, green_odd),
                    ),
                    2,
                ),
                _mm_srli_epi32(
                    _mm_add_epi32(
                        channel_sum(top_pixels, red_even, red_odd),
                        channel_sum(bottom_pixels, red_even, red_odd),
                    ),
                    2,
                ),
            )
        };
        let (u, v) = unsafe {
            let u = _mm_add_epi32(
                _mm_srai_epi32(
                    _mm_add_epi32(
                        _mm_add_epi32(
                            _mm_mullo_epi32(blue, _mm_set1_epi32(112)),
                            _mm_mullo_epi32(green, _mm_set1_epi32(-86)),
                        ),
                        _mm_add_epi32(
                            _mm_mullo_epi32(red, _mm_set1_epi32(-26)),
                            _mm_set1_epi32(128),
                        ),
                    ),
                    8,
                ),
                _mm_set1_epi32(128),
            );
            let v = _mm_add_epi32(
                _mm_srai_epi32(
                    _mm_add_epi32(
                        _mm_add_epi32(
                            _mm_mullo_epi32(red, _mm_set1_epi32(112)),
                            _mm_mullo_epi32(green, _mm_set1_epi32(-102)),
                        ),
                        _mm_add_epi32(
                            _mm_mullo_epi32(blue, _mm_set1_epi32(-10)),
                            _mm_set1_epi32(128),
                        ),
                    ),
                    8,
                ),
                _mm_set1_epi32(128),
            );
            (u, v)
        };
        let uv = unsafe {
            _mm_unpacklo_epi8(
                _mm_packus_epi16(_mm_packus_epi32(u, _mm_set1_epi32(0)), _mm_set1_epi32(0)),
                _mm_packus_epi16(_mm_packus_epi32(v, _mm_set1_epi32(0)), _mm_set1_epi32(0)),
            )
        };
        // SAFETY: destination has four writable bytes for the two chroma blocks.
        unsafe { _mm_storeu_si32(destination.cast(), uv) };
    }

    let (luma, chroma) = nv12.split_at_mut(width * height);
    // SAFETY: this function is only entered after SSSE3/SSE4.1 runtime checks.
    let (red_shuffle, weight, rounding, offset) = (
        _mm_setr_epi8(2, 6, 10, 14, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1, -1),
        _mm_set1_epi32(47),
        _mm_set1_epi32(128),
        _mm_set1_epi32(16),
    );

    for row in (0..height).step_by(2) {
        let top = row * width * 4;
        let bottom = top + width * 4;
        let top_y = row * width;
        let bottom_y = top_y + width;
        let mut column = 0;
        while column + 4 <= width {
            let top_values = unsafe {
                luma4(
                    bgra.as_ptr().add(top + column * 4),
                    red_shuffle,
                    weight,
                    rounding,
                    offset,
                )
            };
            let bottom_values = unsafe {
                luma4(
                    bgra.as_ptr().add(bottom + column * 4),
                    red_shuffle,
                    weight,
                    rounding,
                    offset,
                )
            };
            // SAFETY: each vector has four packed luma bytes and the validated
            // row has at least four bytes remaining.
            unsafe {
                _mm_storeu_si32(luma.as_mut_ptr().add(top_y + column).cast(), top_values);
                _mm_storeu_si32(
                    luma.as_mut_ptr().add(bottom_y + column).cast(),
                    bottom_values,
                );
            }
            column += 4;
        }
        while column < width {
            for (source_row, target_row) in [(top, top_y), (bottom, bottom_y)] {
                let source = source_row + column * 4;
                let b = i32::from(bgra[source]);
                let g = i32::from(bgra[source + 1]);
                let r = i32::from(bgra[source + 2]);
                luma[target_row + column] = clamp_u8(((47 * r + 157 * g + 16 * b + 128) >> 8) + 16);
            }
            column += 1;
        }
        let chroma_row = (row / 2) * width;
        let mut column = 0;
        while column + 4 <= width {
            // SAFETY: four adjacent pixels fit in each validated source row;
            // two UV pairs fit in the validated chroma row.
            unsafe {
                chroma2(
                    bgra.as_ptr().add(top + column * 4),
                    bgra.as_ptr().add(bottom + column * 4),
                    chroma.as_mut_ptr().add(chroma_row + column),
                );
            }
            column += 4;
        }
        while column < width {
            let mut r = 0_i32;
            let mut g = 0_i32;
            let mut b = 0_i32;
            for (source_row, x) in [
                (top, column),
                (top, column + 1),
                (bottom, column),
                (bottom, column + 1),
            ] {
                let source = source_row + x * 4;
                b += i32::from(bgra[source]);
                g += i32::from(bgra[source + 1]);
                r += i32::from(bgra[source + 2]);
            }
            r /= 4;
            g /= 4;
            b /= 4;
            let destination = chroma_row + column;
            chroma[destination] = clamp_u8(((-26 * r - 86 * g + 112 * b + 128) >> 8) + 128);
            chroma[destination + 1] = clamp_u8(((112 * r - 102 * g - 10 * b + 128) >> 8) + 128);
            column += 2;
        }
    }
}

pub(crate) fn nv12_to_bgra(nv12: &[u8], width: u32, height: u32) -> Result<Vec<u8>, MediaError> {
    nv12_to_bgra_strided(nv12, width, height, width)
}

pub(crate) fn nv12_to_bgra_strided(
    nv12: &[u8],
    width: u32,
    height: u32,
    stride: u32,
) -> Result<Vec<u8>, MediaError> {
    nv12_to_bgra_layout(nv12, width, height, stride, height)
}

pub(crate) fn nv12_to_bgra_layout(
    nv12: &[u8],
    width: u32,
    height: u32,
    stride: u32,
    storage_height: u32,
) -> Result<Vec<u8>, MediaError> {
    validate_even_dimensions(width, height)?;
    if storage_height < height || storage_height % 2 != 0 {
        return Err(MediaError::Operation(
            "invalid NV12 storage height".to_owned(),
        ));
    }
    let width = usize::try_from(width).map_err(|_| size_error())?;
    let height = usize::try_from(height).map_err(|_| size_error())?;
    let stride = usize::try_from(stride).map_err(|_| size_error())?;
    if stride < width || stride % 2 != 0 {
        return Err(MediaError::Operation("invalid NV12 row stride".to_owned()));
    }
    let pixels = width.checked_mul(height).ok_or_else(size_error)?;
    let luma_size = stride
        .checked_mul(usize::try_from(storage_height).map_err(|_| size_error())?)
        .ok_or_else(size_error)?;
    let expected = luma_size
        .checked_add(luma_size / 2)
        .ok_or_else(size_error)?;
    if nv12.len() < expected {
        return Err(MediaError::Operation(format!(
            "NV12 frame is too short: expected {expected} bytes, got {}",
            nv12.len()
        )));
    }
    let mut bgra = vec![0_u8; pixels.checked_mul(4).ok_or_else(size_error)?];
    for y in 0..height {
        for x in 0..width {
            let chroma = luma_size + (y / 2) * stride + (x & !1);
            let c = (i32::from(nv12[y * stride + x]) - 16).max(0);
            let d = i32::from(nv12[chroma]) - 128;
            let e = i32::from(nv12[chroma + 1]) - 128;
            let destination = (y * width + x) * 4;
            bgra[destination] = clamp_u8((298 * c + 541 * d + 128) >> 8);
            bgra[destination + 1] = clamp_u8((298 * c - 55 * d - 136 * e + 128) >> 8);
            bgra[destination + 2] = clamp_u8((298 * c + 459 * e + 128) >> 8);
            bgra[destination + 3] = u8::MAX;
        }
    }
    Ok(bgra)
}

fn validate_even_dimensions(width: u32, height: u32) -> Result<(), MediaError> {
    if width == 0 || height == 0 || width % 2 != 0 || height % 2 != 0 {
        return Err(MediaError::Operation(
            "NV12 conversion requires non-zero even dimensions".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(windows)]
pub(super) fn bgra_to_nv12_backend() -> &'static str {
    #[cfg(target_arch = "x86_64")]
    if std::arch::is_x86_feature_detected!("ssse3") && std::arch::is_x86_feature_detected!("sse4.1")
    {
        return "x86_64-ssse3-sse4.1-yuv";
    }
    "scalar"
}

fn clamp_u8(value: i32) -> u8 {
    u8::try_from(value.clamp(0, 255)).unwrap_or_default()
}

fn size_error() -> MediaError {
    MediaError::Operation("video frame size overflowed".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference_conversion(bgra: &[u8], width: usize, height: usize) -> Vec<u8> {
        let pixels = width * height;
        let mut nv12 = vec![0; pixels + pixels / 2];
        for y in 0..height {
            for x in 0..width {
                let source = (y * width + x) * 4;
                let b = i32::from(bgra[source]);
                let g = i32::from(bgra[source + 1]);
                let r = i32::from(bgra[source + 2]);
                nv12[y * width + x] = clamp_u8(((47 * r + 157 * g + 16 * b + 128) >> 8) + 16);
            }
        }

        let chroma_start = pixels;
        for y in (0..height).step_by(2) {
            for x in (0..width).step_by(2) {
                let mut r = 0_i32;
                let mut g = 0_i32;
                let mut b = 0_i32;
                for row in 0..2 {
                    for column in 0..2 {
                        let source = ((y + row) * width + x + column) * 4;
                        b += i32::from(bgra[source]);
                        g += i32::from(bgra[source + 1]);
                        r += i32::from(bgra[source + 2]);
                    }
                }
                r /= 4;
                g /= 4;
                b /= 4;
                let destination = chroma_start + (y / 2) * width + x;
                nv12[destination] = clamp_u8(((-26 * r - 86 * g + 112 * b + 128) >> 8) + 128);
                nv12[destination + 1] = clamp_u8(((112 * r - 102 * g - 10 * b + 128) >> 8) + 128);
            }
        }
        nv12
    }

    #[test]
    #[ignore = "timing probe; run release on the target Windows machine"]
    fn conversion_timing_probe() {
        use std::{hint::black_box, time::Instant};
        let (width, height) = (1920, 1080);
        let mut state = 0x1234_5678_u32;
        let bgra: Vec<_> = (0..width * height * 4)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                state.to_le_bytes()[3]
            })
            .collect();
        let expected = reference_conversion(&bgra, width, height);
        let mut scalar_output = vec![0; width * height * 3 / 2];
        bgra_to_nv12_scalar_validated(&bgra, width, height, &mut scalar_output);
        let simd_output = bgra_to_nv12(&bgra, 1920, 1080).unwrap();
        assert_eq!(scalar_output, expected);
        assert_eq!(simd_output, expected);
        let mut scalar = Vec::new();
        let mut simd = Vec::new();
        for iteration in 0..10 {
            // Alternate measurement order to reduce warm-up/clock bias.
            for mode in [iteration % 2, 1 - iteration % 2] {
                let start = Instant::now();
                for _ in 0..20 {
                    let bytes = if mode == 0 {
                        let mut output = vec![0; width * height * 3 / 2];
                        bgra_to_nv12_scalar_validated(black_box(&bgra), width, height, &mut output);
                        output
                    } else {
                        bgra_to_nv12(black_box(&bgra), 1920, 1080).unwrap()
                    };
                    black_box(bytes);
                }
                let mean_ms = start.elapsed().as_secs_f64() * 1000.0 / 20.0;
                if mode == 0 {
                    scalar.push(mean_ms);
                } else {
                    simd.push(mean_ms);
                }
            }
        }
        scalar.sort_by(f64::total_cmp);
        simd.sort_by(f64::total_cmp);
        println!("conversion_probe width=1920 height=1080 frames_per_path=200 scalar_median_ms={} simd_median_ms={} speedup={:.3} scalar_batches_ms={scalar:?} simd_batches_ms={simd:?} byte_identical=true",scalar[5],simd[5],scalar[5]/simd[5]);
        // Timing is reported rather than asserted: VM scheduling is variable.
    }

    #[test]
    fn fused_conversion_preserves_scalar_pixels_and_rounding() {
        // Independent original two-pass reference protects chroma averaging,
        // row/block placement and every byte of the clean-image baseline.
        for (width, height) in [(2, 2), (14, 6), (64, 32)] {
            let mut state = 0x8bad_f00d_u32;
            let bgra: Vec<_> = (0..width * height * 4)
                .map(|_| {
                    state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    state.to_le_bytes()[3]
                })
                .collect();
            let nv12 = reference_conversion(&bgra, width, height);
            assert_eq!(
                bgra_to_nv12(
                    &bgra,
                    u32::try_from(width).unwrap(),
                    u32::try_from(height).unwrap()
                )
                .unwrap(),
                nv12
            );
            #[cfg(target_arch = "x86_64")]
            if std::arch::is_x86_feature_detected!("ssse3")
                && std::arch::is_x86_feature_detected!("sse4.1")
            {
                let mut simd = vec![0; width * height * 3 / 2];
                // SAFETY: the test checks CPU features and uses validated even
                // dimensions plus exactly-sized input/output allocations.
                unsafe {
                    bgra_to_nv12_sse41(&bgra, width, height, &mut simd);
                }
                assert_eq!(simd, nv12, "SIMD mismatch at {width}x{height}");
            }
        }
    }

    #[test]
    fn copied_layout_does_not_apply_surface_padding_to_packed_bytes() {
        assert_eq!(copied_nv12_stride(12, 4, 2, Some(8), true).unwrap(), 4);
        assert_eq!(copied_nv12_stride(12, 4, 2, Some(8), false).unwrap(), 4);
        assert_eq!(copied_nv12_stride(24, 4, 2, Some(8), false).unwrap(), 8);
        assert_eq!(copied_nv12_stride(24, 4, 2, None, false).unwrap(), 8);
        assert!(copied_nv12_stride(24, 4, 2, Some(8), true).is_err());
        assert!(copied_nv12_stride(13, 4, 2, None, false).is_err());
    }

    #[test]
    fn black_and_white_convert_to_expected_luma_range() {
        let bgra = [
            0, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255,
        ];
        let nv12 = bgra_to_nv12(&bgra, 2, 2).unwrap();
        assert_eq!(&nv12[..4], &[16, 235, 16, 235]);
        assert_eq!(&nv12[4..], &[128, 128]);
    }

    #[test]
    fn conversion_rejects_odd_dimensions() {
        assert!(bgra_to_nv12(&[0; 12], 3, 1).is_err());
        assert!(nv12_to_bgra(&[0; 12], 3, 2).is_err());
    }

    #[test]
    fn nv12_round_trip_keeps_neutral_pixels_neutral() {
        let bgra = [
            90, 90, 90, 255, 180, 180, 180, 255, 90, 90, 90, 255, 180, 180, 180, 255,
        ];
        let nv12 = bgra_to_nv12(&bgra, 2, 2).unwrap();
        let round_trip = nv12_to_bgra(&nv12, 2, 2).unwrap();
        for pixel in round_trip.chunks_exact(4) {
            assert!(pixel[0].abs_diff(pixel[1]) <= 1);
            assert!(pixel[1].abs_diff(pixel[2]) <= 1);
            assert_eq!(pixel[3], 255);
        }
    }

    #[test]
    fn decoder_skips_nv12_row_padding() {
        let padded = [16, 235, 99, 99, 235, 16, 99, 99, 128, 128, 99, 99];
        let result = nv12_to_bgra_strided(&padded, 2, 2, 4).unwrap();
        assert_eq!(&result[..4], &[0, 0, 0, 255]);
        assert_eq!(&result[4..8], &[255, 255, 255, 255]);
        assert_eq!(&result[8..12], &[255, 255, 255, 255]);
        assert_eq!(&result[12..16], &[0, 0, 0, 255]);
    }

    #[test]
    fn decoder_skips_padded_luma_rows_before_chroma() {
        let padded = [
            16, 235, 99, 99, 235, 16, 99, 99, 0, 0, 0, 0, 0, 0, 0, 0, 128, 128, 99, 99, 0, 0, 0, 0,
        ];
        let result = nv12_to_bgra_layout(&padded, 2, 2, 4, 4).unwrap();
        assert_eq!(
            result,
            [0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 255]
        );
    }
}
