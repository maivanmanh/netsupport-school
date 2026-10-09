//! One-shot Student capture, isolated from media/control by the file-job worker.
use anyhow::{Context, Result};
use media_core::{CaptureConfig, CaptureSource};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub(super) fn capture(path: &Path, cancel: &AtomicBool) -> Result<()> {
    let mut source = crate::PlatformCapture::new();
    source.start_thumbnail(CaptureConfig {
        display_index: 0,
        width: 1920,
        height: 1080,
        max_fps: 1,
    })?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let frame = loop {
        anyhow::ensure!(!cancel.load(Ordering::Acquire), "Screenshot cancelled");
        anyhow::ensure!(
            Instant::now() < deadline,
            "Student screenshot capture timed out"
        );
        if let Some(frame) =
            source.next_frame((Instant::now() + Duration::from_millis(250)).min(deadline))?
        {
            break frame;
        }
    };
    drop(source);
    anyhow::ensure!(
        frame.width > 0 && frame.height > 0 && frame.width <= 1920 && frame.height <= 1080,
        "Invalid screenshot dimensions"
    );
    let row = usize::try_from(frame.width)? * 4;
    let pitch = usize::try_from(frame.row_pitch)?;
    let height = usize::try_from(frame.height)?;
    anyhow::ensure!(
        pitch >= row && frame.bytes.len() >= pitch * height,
        "Incomplete screenshot pixels"
    );
    let mut rgba = Vec::with_capacity(row * height);
    for y in 0..height {
        for pixel in frame.bytes[y * pitch..y * pitch + row].chunks_exact(4) {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
        }
    }
    let file = std::fs::File::create(path).context("Cannot stage Student screenshot")?;
    let mut encoder = png::Encoder::new(file, frame.width, frame.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&rgba)?;
    Ok(())
}
