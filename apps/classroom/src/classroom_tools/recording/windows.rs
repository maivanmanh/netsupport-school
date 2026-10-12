//! DXGI + existing hardware/software MF encoder in an isolated owned helper.
use super::{
    atomic_json, chunk_name, epoch,
    mux::Mux,
    student::{load_json, Metadata},
    CHUNK_SECONDS, MAX_CHUNKS,
};
use anyhow::{Context, Result};
use codec_windows_mf::{MediaFoundationRuntime, MfH264Encoder};
use media_core::{CaptureConfig, CaptureSource, H264Config, VideoEncoder, VideoFrameRef};
use serde_json::json;
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub(crate) fn early_operation() -> Option<Result<()>> {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(std::ffi::OsStr::new("--record-screen-internal")) {
        return None;
    }
    Some((|| {
        let dir = PathBuf::from(args.next().context("Missing recording directory")?);
        anyhow::ensure!(args.next().is_none(), "Unexpected recording arguments");
        let result = record(&dir);
        if let Err(e) = &result {
            let mut status = load_json(&dir.join("status.json")).unwrap_or_else(|_| json!({}));
            status["phase"] = json!("error");
            status["error"] = json!(format!("{e:#}").chars().take(300).collect::<String>());
            let _ = atomic_json(&dir.join("status.json"), &status);
        }
        // The parent must not replace the detailed persisted error with a generic
        // nonzero-exit message. Fatal termination is detected separately.
        Ok(())
    })())
}
fn pts(time: Duration) -> u64 {
    u64::try_from(time.as_micros()).unwrap_or(u64::MAX) * 90 / 1000
}
fn record(dir: &Path) -> Result<()> {
    use fs2::FileExt;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("capture.lock"))?;
    lock.try_lock_exclusive()
        .context("Recording helper already running")?;
    let meta: Metadata = serde_json::from_slice(&fs::read(dir.join("meta.json"))?)?;
    let mut capture = crate::PlatformCapture::new();
    capture.start(CaptureConfig {
        display_index: 0,
        width: 1280,
        height: 720,
        max_fps: 30,
    })?;
    let dimensions = capture
        .output_dimensions()
        .context("Capture dimensions unavailable")?;
    anyhow::ensure!(
        dimensions.0 <= 1280 && dimensions.1 <= 720,
        "Recording exceeds 720p limits"
    );
    let runtime = MediaFoundationRuntime::new()?;
    let config = H264Config {
        width: dimensions.0,
        height: dimensions.1,
        frames_per_second: 30,
        bitrate: 3_000_000,
        keyframe_interval_frames: 60,
    };
    let mut encoder = MfH264Encoder::new(&runtime, config)?;
    let descriptor = encoder.descriptor();
    let backend = format!("{} ({})", descriptor.name, descriptor.kind);
    let started = Instant::now();
    let mut segment = Instant::now();
    let mut index = 1;
    let mut mux: Option<Mux> = None;
    let mut latest = None;
    let mut next = Instant::now();
    let mut last_status = Instant::now()
        .checked_sub(Duration::from_secs(2))
        .unwrap_or_else(Instant::now);
    let mut interval = Instant::now();
    let mut interval_frames = 0u32;
    let mut frames = 0u64;
    let mut fps = 0.0;
    let mut completed = 0u64;
    let outcome = (|| -> Result<()> {
        loop {
            let stop = dir.join("stop").exists() || epoch() >= meta.expires;
            let heartbeat = fs::read_to_string(dir.join("heartbeat.json"))
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            anyhow::ensure!(
                epoch().saturating_sub(heartbeat) < 15,
                "Student Agent heartbeat expired"
            );
            if stop || segment.elapsed() >= Duration::from_secs(CHUNK_SECONDS) {
                if let Some(writer) = mux.take() {
                    writer.finish(pts(segment
                        .elapsed()
                        .min(Duration::from_secs(CHUNK_SECONDS))))?;
                    fs::rename(
                        dir.join(format!("Part-{index:04}.partial")),
                        dir.join(chunk_name(index)?),
                    )?;
                    completed += 1;
                }
                if stop {
                    break;
                }
                index += 1;
                anyhow::ensure!(index <= MAX_CHUNKS, "Recording duration limit reached");
                // Fresh encoder guarantees independent SPS/PPS+IDR for every MP4.
                encoder = MfH264Encoder::new(&runtime, config)?;
                segment = Instant::now();
            }
            if last_status.elapsed() >= Duration::from_secs(1) {
                anyhow::ensure!(
                    fs2::available_space(dir)? > 512 * 1024 * 1024,
                    "Student disk is low; recording stopped"
                );
                let backlog: u64 = fs::read_dir(dir)?
                    .flatten()
                    .filter_map(|e| e.metadata().ok())
                    .map(|m| m.len())
                    .sum();
                anyhow::ensure!(
                    backlog < 8 * 1024 * 1024 * 1024,
                    "Recording upload backlog reached 8 GiB; recording stopped"
                );
                if interval.elapsed() >= Duration::from_secs(1) {
                    fps = f64::from(interval_frames) / interval.elapsed().as_secs_f64();
                    interval = Instant::now();
                    interval_frames = 0;
                }
                atomic_json(
                    &dir.join("status.json"),
                    &json!({"phase":"recording","elapsed_seconds":started.elapsed().as_secs(),
                    "width":dimensions.0,"height":dimensions.1,"fps":fps,"frames":frames,"chunk":index,"completed_chunks":completed,
                    "spool_bytes":backlog,"encoder":backend,"chunk_seconds":CHUNK_SECONDS}),
                )?;
                last_status = Instant::now();
            }
            if let Some(frame) =
                capture.next_frame(next.max(Instant::now()) + Duration::from_millis(1))?
            {
                latest = Some(frame);
            }
            if Instant::now() < next {
                std::thread::sleep(next.saturating_duration_since(Instant::now()));
            }
            let now = Instant::now();
            next = now + Duration::from_nanos(1_000_000_000 / 30);
            let Some(frame) = &latest else {
                anyhow::ensure!(
                    started.elapsed() < Duration::from_secs(10),
                    "No screen frame received"
                );
                continue;
            };
            anyhow::ensure!(
                frame.width == dimensions.0
                    && frame.height == dimensions.1
                    && frame.row_pitch == frame.width * 4,
                "Recording capture geometry changed"
            );
            // Repeated static images use this tick's time. Encoded outputs are
            // associated with their original submission, never output arrival.
            let output = encoder.encode(VideoFrameRef {
                bytes: &frame.bytes,
                captured_at: now,
                width: frame.width,
                height: frame.height,
            })?;
            if let Some(au) = output {
                let timing = encoder
                    .take_source_timing(au.timestamp)
                    .context("Encoder omitted recording timestamp; cannot preserve elapsed time")?;
                let timestamp = pts(timing.captured_at.saturating_duration_since(segment));
                if let Some(writer) = mux.as_mut() {
                    writer.push(&au.bytes, timestamp, au.is_keyframe)?;
                } else if au.is_keyframe {
                    mux = Some(Mux::start(
                        &dir.join(format!("Part-{index:04}.partial")),
                        dimensions,
                        &au.bytes,
                        timestamp,
                    )?);
                } else {
                    encoder.request_keyframe()?;
                }
                interval_frames += 1;
                frames += 1;
            }
        }
        Ok(())
    })();
    // Graceful errors (disk/capture/geometry) still finalize the current playable
    // part. Only a forced kill/power failure can leave its .partial unrecoverable.
    if let Some(writer) = mux.take() {
        writer.finish(pts(segment.elapsed()))?;
        fs::rename(
            dir.join(format!("Part-{index:04}.partial")),
            dir.join(chunk_name(index)?),
        )?;
        completed += 1;
    }
    let mut status = load_json(&dir.join("status.json")).unwrap_or_else(|_| json!({}));
    status["phase"] = json!(if outcome.is_ok() { "stopped" } else { "error" });
    status["elapsed_seconds"] = json!(started.elapsed().as_secs());
    status["completed_chunks"] = json!(completed);
    if let Err(e) = &outcome {
        status["error"] = json!(format!("{e:#}"));
    }
    atomic_json(&dir.join("status.json"), &status)?;
    outcome
}
