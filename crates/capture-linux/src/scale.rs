/// Aspect-preserving fit inside `max_w`×`max_h`; never upscales. Even dimensions.
pub fn fit(src_w: u32, src_h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    if src_w == 0 || src_h == 0 || max_w == 0 || max_h == 0 || (src_w <= max_w && src_h <= max_h) {
        return (src_w & !1, src_h & !1);
    }
    let (w, h) = if u64::from(max_w) * u64::from(src_h) <= u64::from(max_h) * u64::from(src_w) {
        (max_w, u32::try_from(u64::from(max_w) * u64::from(src_h) / u64::from(src_w)).unwrap_or(max_h))
    } else {
        (u32::try_from(u64::from(max_h) * u64::from(src_w) / u64::from(src_h)).unwrap_or(max_w), max_h)
    };
    ((w & !1).max(2), (h & !1).max(2))
}

/// Box-filter downscale of tightly addressed BGRA rows (`src_stride` bytes per row).
/// Output has alpha 255. `dst` must hold `dst_w * dst_h * 4` bytes.
pub fn downscale_bgra(
    src: &[u8],
    src_w: usize,
    src_h: usize,
    src_stride: usize,
    dst: &mut [u8],
    dst_w: usize,
    dst_h: usize,
) {
    for dy in 0..dst_h {
        let y0 = dy * src_h / dst_h;
        let y1 = (((dy + 1) * src_h) / dst_h).max(y0 + 1).min(src_h);
        for dx in 0..dst_w {
            let x0 = dx * src_w / dst_w;
            let x1 = (((dx + 1) * src_w) / dst_w).max(x0 + 1).min(src_w);
            let (mut b, mut g, mut r, mut n) = (0_u32, 0_u32, 0_u32, 0_u32);
            for y in y0..y1 {
                let row = y * src_stride;
                for x in x0..x1 {
                    let i = row + x * 4;
                    b += u32::from(src[i]);
                    g += u32::from(src[i + 1]);
                    r += u32::from(src[i + 2]);
                    n += 1;
                }
            }
            let o = (dy * dst_w + dx) * 4;
            dst[o] = (b / n) as u8;
            dst[o + 1] = (g / n) as u8;
            dst[o + 2] = (r / n) as u8;
            dst[o + 3] = 255;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fit_keeps_aspect_and_never_upscales() {
        assert_eq!(fit(1920, 1080, 480, 270), (480, 270));
        assert_eq!(fit(1512, 949, 480, 270), (430, 270));
        assert_eq!(fit(640, 360, 1920, 1080), (640, 360));
    }
    #[test]
    fn box_filter_averages_and_sets_alpha() {
        let src = [0_u8, 0, 0, 0, 100, 100, 100, 0, 0, 0, 0, 0, 100, 100, 100, 0];
        let mut dst = [0_u8; 4];
        downscale_bgra(&src, 2, 2, 8, &mut dst, 1, 1);
        assert_eq!(dst, [50, 50, 50, 255]);
    }
}
