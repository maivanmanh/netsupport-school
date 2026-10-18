//! DXGI + existing hardware/software MF encoder in an isolated owned helper.
use super::{
    atomic_json, chunk_name, epoch,
    mux::Mux,
    student::{load_json, Metadata},
    MAX_CHUNKS,
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

/// Extra free space beyond the stop limit that a Student needs to begin
/// recording (so it does not start only to stop within minutes).
const START_MARGIN_BYTES: u64 = 512 * 1024 * 1024;

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
    // Do not start what the disk cannot sustain: it would stop within minutes.
    let options = meta.options.clone().sanitized();
    let (bound_width, bound_height) = options.bounds();
    let chunk_seconds = options.chunk_seconds;
    let keep_free = options.student_stop_bytes;
    let warn_free = options.student_warn_bytes;
    anyhow::ensure!(
        fs2::available_space(dir)? > keep_free + START_MARGIN_BYTES,
        "Ổ đĩa máy học viên còn quá ít chỗ trống; không bắt đầu ghi"
    );
    let mut capture = crate::PlatformCapture::new();
    capture.start(CaptureConfig {
        display_index: 0,
        width: bound_width,
        height: bound_height,
        max_fps: options.fps,
    })?;
    let dimensions = capture
        .output_dimensions()
        .context("Capture dimensions unavailable")?;
    anyhow::ensure!(
        dimensions.0 <= bound_width && dimensions.1 <= bound_height,
        "Recording exceeds the chosen size limit"
    );
    let runtime = MediaFoundationRuntime::new()?;
    let config = H264Config {
        width: dimensions.0,
        height: dimensions.1,
        frames_per_second: options.fps,
        bitrate: options.bitrate_kbps * 1000,
        keyframe_interval_frames: options.fps * 2,
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
    let mut stop_reason = String::new();
    let outcome = (|| -> Result<()> {
        loop {
            let requested = fs::read_to_string(dir.join("stop"));
            let stop = requested.is_ok() || epoch() >= meta.expires;
            if stop && stop_reason.is_empty() {
                stop_reason = match requested {
                    Ok(reason) => reason.trim().to_owned(),
                    Err(_) => "time_limit".to_owned(),
                };
            }
            let heartbeat = fs::read_to_string(dir.join("heartbeat.json"))
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            anyhow::ensure!(
                epoch().saturating_sub(heartbeat) < 15,
                "Student Agent heartbeat expired"
            );
            if stop || segment.elapsed() >= Duration::from_secs(chunk_seconds) {
                if let Some(writer) = mux.take() {
                    writer.finish(pts(segment
                        .elapsed()
                        .min(Duration::from_secs(chunk_seconds))))?;
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
                let free = fs2::available_space(dir)?;
                if free <= keep_free {
                    stop_reason = "student_disk_low".to_owned();
                }
                anyhow::ensure!(
                    free > keep_free,
                    "Ổ đĩa máy học viên gần đầy; đã dừng ghi (video đã ghi vẫn được giữ và tải về)"
                );
                let backlog: u64 = fs::read_dir(dir)?
                    .flatten()
                    .filter_map(|e| e.metadata().ok())
                    .map(|m| m.len())
                    .sum();
                if backlog >= 8 * 1024 * 1024 * 1024 {
                    stop_reason = "spool_full".to_owned();
                }
                anyhow::ensure!(
                    backlog < 8 * 1024 * 1024 * 1024,
                    "Video chưa tải về đã đạt 8 GiB trên máy học viên (Teacher không nhận được); đã dừng ghi"
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
                    "spool_bytes":backlog,"encoder":backend,"chunk_seconds":chunk_seconds,
                    "free_bytes":free,"disk_warning":free < warn_free,"deadline":meta.expires}),
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
            next = now + Duration::from_nanos(1_000_000_000 / u64::from(options.fps));
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
                        options.max_samples(),
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
    if !stop_reason.is_empty() {
        status["stop_reason"] = json!(stop_reason);
    }
    if let Err(e) = &outcome {
        status["error"] = json!(format!("{e:#}"));
    }
    atomic_json(&dir.join("status.json"), &status)?;
    outcome
}
