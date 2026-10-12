//! One-shot Student capture, isolated from media/control by the file-job worker.
use anyhow::{Context, Result};
use media_core::{CaptureConfig, CaptureSource};
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

pub(super) fn capture(path: &Path, cancel: &AtomicBool) -> Result<()> {
    #[cfg(windows)]
    {
        // DXGI permits only one duplication per output per process. Monitoring
        // may own that duplication; a short-lived child avoids disrupting it.
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};
        let error_path = path.with_extension("capture-error.txt");
        let error_file = std::fs::File::create(&error_path)?;
        let mut child = Command::new(std::env::current_exe()?)
            .arg("--capture-screenshot-internal")
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(error_file)
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .spawn()
            .context("Cannot start isolated Student screenshot capture")?;
        let deadline = Instant::now() + Duration::from_secs(12);
        let result = loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    break if status.success() {
                        Ok(())
                    } else {
                        let detail = std::fs::read_to_string(&error_path).unwrap_or_default();
                        Err(anyhow::anyhow!(
                            "Student screenshot capture failed: {}",
                            detail.trim()
                        ))
                    };
                }
                Ok(None) => {}
                Err(error) => break Err(error.into()),
            }
            if cancel.load(Ordering::Acquire) {
                break Err(anyhow::anyhow!("Screenshot cancelled"));
            }
            if Instant::now() >= deadline {
                break Err(anyhow::anyhow!("Student screenshot capture timed out"));
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        // Reap on every exit, including cancellation and process-status errors.
        if result.is_err() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_file(error_path);
        result
    }
    #[cfg(not(windows))]
    capture_inline(path, cancel)
}

#[cfg(windows)]
pub(crate) fn early_operation() -> Option<Result<()>> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--capture-screenshot-internal")) {
        return None;
    }
    Some((|| {
        let path = args.next().context("Missing screenshot output path")?;
        anyhow::ensure!(args.next().is_none(), "Unexpected screenshot arguments");
        capture_inline(Path::new(&path), &AtomicBool::new(false))
    })())
}

fn capture_inline(path: &Path, cancel: &AtomicBool) -> Result<()> {
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
