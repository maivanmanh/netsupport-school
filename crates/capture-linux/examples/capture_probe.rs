//! Captures the primary monitor for N seconds and prints cadence; writes the last
//! frame as a PPM. `cargo run --release -p capture-linux --example capture_probe -- [seconds] [w] [h] [fps] [out.ppm]`
#[cfg(target_os = "linux")]
fn main() {
    use media_core::{CaptureConfig, CaptureSource};
    use std::time::{Duration, Instant};
    let a: Vec<String> = std::env::args().collect();
    let num = |i: usize, d: u32| a.get(i).and_then(|v| v.parse().ok()).unwrap_or(d);
    let (seconds, width, height, fps) = (num(1, 5), num(2, 1920), num(3, 1080), num(4, 30));
    let mut capture = capture_linux::LinuxCapture::new();
    let started = Instant::now();
    capture.start(CaptureConfig { display_index: 0, width, height, max_fps: fps }).expect("start");
    println!("started in {:?}, dims {:?}", started.elapsed(), capture.output_dimensions());
    let end = Instant::now() + Duration::from_secs(seconds.into());
    let (mut frames, mut last) = (0_u32, None);
    while Instant::now() < end {
        if let Some(frame) = capture.next_frame(Instant::now() + Duration::from_millis(500)).expect("frame") {
            frames += 1;
            last = Some(frame);
        }
    }
    println!("{frames} frames in {seconds}s, stats {:?}", capture.statistics());
    if let (Some(frame), Some(path)) = (last, a.get(5)) {
        let mut ppm = format!("P6\n{} {}\n255\n", frame.width, frame.height).into_bytes();
        for px in frame.bytes.chunks_exact(4) {
            ppm.extend_from_slice(&[px[2], px[1], px[0]]);
        }
        std::fs::write(path, ppm).expect("write");
        println!("wrote {path} {}x{}", frame.width, frame.height);
    }
}
#[cfg(not(target_os = "linux"))]
fn main() {}
