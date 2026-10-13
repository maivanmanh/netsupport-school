use media_core::{DecodedFrameRef, MediaError, PointerOverlay};

/// Renderer-owned latest surface, not a video queue. The BGRA storage permits
/// safe shared pointer blending; packed RGB is minifb's platform-neutral format.
#[derive(Debug, Default)]
pub(crate) struct Surface {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u32>,
    bgra: Vec<u8>,
    pointer: Option<PointerOverlay>,
    pointer_dirty: bool,
    under: Option<([usize; 4], Vec<u8>)>,
}

impl Surface {
    pub fn set_pointer(&mut self, pointer: Option<PointerOverlay>) {
        if self.pointer != pointer {
            self.pointer = pointer;
            self.pointer_dirty = true;
        }
    }

    pub fn replace(&mut self, frame: DecodedFrameRef<'_>) -> Result<(), MediaError> {
        let bgra = frame.bgra()?;
        let width = usize::try_from(frame.width).map_err(operation)?;
        let height = usize::try_from(frame.height).map_err(operation)?;
        let count = width
            .checked_mul(height)
            .and_then(|count| count.checked_mul(4))
            .ok_or_else(|| MediaError::Operation("Linux render dimensions overflow".into()))?;
        self.bgra.clear();
        self.bgra.extend_from_slice(&bgra[..count]);
        self.width = width;
        self.height = height;
        self.pixels.resize(count / 4, 0);
        self.under = None;
        self.draw_pointer();
        self.pack([0, 0, width, height]);
        self.pointer_dirty = false;
        Ok(())
    }

    pub fn redraw_pointer(&mut self) -> bool {
        if self.bgra.is_empty() || !self.pointer_dirty {
            return false;
        }
        if let Some((rect @ [x, y, width, height], saved)) = self.under.take() {
            for row in 0..height {
                let start = ((y + row) * self.width + x) * 4;
                self.bgra[start..start + width * 4]
                    .copy_from_slice(&saved[row * width * 4..(row + 1) * width * 4]);
            }
            self.pack(rect);
        }
        self.draw_pointer();
        if let Some((rect, _)) = &self.under {
            self.pack(*rect);
        }
        self.pointer_dirty = false;
        true
    }

    fn draw_pointer(&mut self) {
        let Some(pointer) = &self.pointer else {
            return;
        };
        let (Ok(width), Ok(height)) = (u32::try_from(self.width), u32::try_from(self.height))
        else {
            return;
        };
        let [left, top, w, h] = pointer.rect(width, height);
        let x0 = left.clamp(0, i64::from(width));
        let y0 = top.clamp(0, i64::from(height));
        let x1 = left.saturating_add(w).clamp(0, i64::from(width));
        let y1 = top.saturating_add(h).clamp(0, i64::from(height));
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let rect = [x0, y0, x1 - x0, y1 - y0].map(|n| usize::try_from(n).unwrap_or(0));
        let [x, y, w, h] = rect;
        let mut saved = Vec::with_capacity(w * h * 4);
        for row in 0..h {
            let start = ((y + row) * self.width + x) * 4;
            saved.extend_from_slice(&self.bgra[start..start + w * 4]);
        }
        pointer.blend_into_bgra(&mut self.bgra, width, height);
        self.under = Some((rect, saved));
    }

    fn pack(&mut self, [x, y, width, height]: [usize; 4]) {
        for row in y..y + height {
            let start = row * self.width + x;
            for (pixel, bgra) in self.pixels[start..start + width]
                .iter_mut()
                .zip(self.bgra[start * 4..(start + width) * 4].chunks_exact(4))
            {
                *pixel =
                    (u32::from(bgra[2]) << 16) | (u32::from(bgra[1]) << 8) | u32::from(bgra[0]);
            }
        }
    }
}

fn operation(error: impl std::fmt::Display) -> MediaError {
    MediaError::Operation(format!("Linux surface conversion failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use media_core::{DecodedPixelFormat, PointerImage};
    use std::sync::Arc;

    fn bgra(bytes: &[u8], width: u32, height: u32) -> DecodedFrameRef<'_> {
        DecodedFrameRef {
            format: DecodedPixelFormat::Bgra,
            bytes,
            width,
            height,
        }
    }

    #[test]
    fn packs_bgra_channels_without_alpha_and_honors_visible_dimensions() {
        let mut surface = Surface::default();
        surface
            .replace(bgra(&[3, 2, 1, 255, 6, 5, 4, 0, 99], 2, 1))
            .unwrap();
        assert_eq!(surface.pixels, [0x0001_0203, 0x0004_0506]);
        surface.replace(bgra(&[9, 8, 7, 255], 1, 1)).unwrap();
        assert_eq!((surface.width, surface.height), (1, 1));
        assert_eq!(surface.pixels, [0x0007_0809]);
        assert!(surface.replace(bgra(&[0; 3], 1, 1)).is_err());
        assert_eq!(surface.pixels, [0x0007_0809]);
    }

    #[test]
    fn pointer_move_hide_and_new_frame_restore_original_pixels() {
        let mut surface = Surface::default();
        assert!(!surface.redraw_pointer());
        surface.replace(bgra(&[10; 16], 2, 2)).unwrap();
        assert!(!surface.redraw_pointer());
        let pointer = PointerOverlay {
            x: 0.0,
            y: 0.0,
            width_fraction: 0.5,
            image: Arc::new(PointerImage {
                serial: 1,
                width: 1,
                height: 1,
                hotspot_x: 0,
                hotspot_y: 0,
                bgra: vec![0, 0, 255, 255],
            }),
        };
        surface.set_pointer(Some(pointer.clone()));
        assert!(surface.redraw_pointer());
        assert_eq!(surface.pixels[0], 0x00ff_0000);
        assert!(!surface.redraw_pointer());
        surface.set_pointer(Some(pointer.clone()));
        assert!(!surface.redraw_pointer());
        surface.set_pointer(Some(PointerOverlay {
            x: 0.5,
            y: 0.5,
            ..pointer
        }));
        surface.redraw_pointer();
        assert_eq!(
            surface.pixels,
            [0x000a_0a0a, 0x000a_0a0a, 0x000a_0a0a, 0x00ff_0000]
        );
        surface.replace(bgra(&[20; 16], 2, 2)).unwrap();
        assert!(!surface.redraw_pointer());
        surface.set_pointer(None);
        surface.redraw_pointer();
        assert_eq!(surface.pixels, [0x0014_1414; 4]);
        assert!(!surface.redraw_pointer());
    }

    #[test]
    fn converts_padded_nv12_and_clips_pointer_outside_surface() {
        let mut surface = Surface::default();
        let mut bytes = vec![16; 24];
        bytes[16..18].copy_from_slice(&[128, 128]);
        surface
            .replace(DecodedFrameRef {
                format: DecodedPixelFormat::Nv12 {
                    stride: 4,
                    storage_height: 4,
                },
                bytes: &bytes,
                width: 2,
                height: 2,
            })
            .unwrap();
        assert_eq!(surface.pixels, [0; 4]);
        surface.set_pointer(Some(PointerOverlay {
            x: -0.5,
            y: -0.5,
            width_fraction: 0.5,
            image: Arc::new(PointerImage::default_arrow(1)),
        }));
        surface.redraw_pointer();
        surface.set_pointer(None);
        surface.redraw_pointer();
        assert_eq!(surface.pixels, [0; 4]);
    }
}
