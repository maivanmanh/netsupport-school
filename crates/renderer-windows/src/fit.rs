/// Shared GPU/GDI display geometry. Stretch uses the whole client rectangle;
/// aspect preservation fits the complete source without cropping.
pub(crate) fn display_rect(
    width: u32,
    height: u32,
    client_width: i32,
    client_height: i32,
    preserve_aspect: bool,
) -> (i32, i32) {
    if width == 0 || height == 0 || client_width <= 0 || client_height <= 0 {
        return (0, 0);
    }
    if preserve_aspect {
        fit_rect(width, height, client_width, client_height)
    } else {
        (client_width, client_height)
    }
}

/// Fit a nonzero source in a positive client rectangle without cropping.
pub(crate) fn fit_rect(
    width: u32,
    height: u32,
    client_width: i32,
    client_height: i32,
) -> (i32, i32) {
    let cw = u64::try_from(client_width).unwrap_or_default();
    let ch = u64::try_from(client_height).unwrap_or_default();
    if width == 0 || height == 0 || cw == 0 || ch == 0 {
        return (0, 0);
    }
    if cw * u64::from(height) <= ch * u64::from(width) {
        (
            client_width,
            i32::try_from(cw * u64::from(height) / u64::from(width))
                .unwrap_or(client_height)
                .max(1),
        )
    } else {
        (
            i32::try_from(ch * u64::from(width) / u64::from(height))
                .unwrap_or(client_width)
                .max(1),
            client_height,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn presentation_fills_four_by_three_but_can_restore_letterboxing() {
        assert_eq!(display_rect(1920, 1080, 1024, 768, false), (1024, 768));
        assert_eq!(display_rect(1920, 1080, 1024, 768, true), (1024, 576));
        assert_eq!(display_rect(1920, 1080, 1080, 1920, false), (1080, 1920));
        assert_eq!(display_rect(1920, 1080, 0, 768, false), (0, 0));
        assert_eq!(display_rect(0, 1080, 1024, 768, false), (0, 0));
    }
    #[test]
    fn letterboxes_different_student_window_shapes() {
        assert_eq!(fit_rect(1920, 1080, 1024, 768), (1024, 576));
        assert_eq!(fit_rect(1280, 800, 1920, 1080), (1728, 1080));
        assert_eq!(fit_rect(1920, 804, 960, 540), (960, 402));
        assert_eq!(fit_rect(1920, 1080, 0, 0), (0, 0));
        assert_eq!(fit_rect(1920, 1080, 1, 1), (1, 1));
    }
}
