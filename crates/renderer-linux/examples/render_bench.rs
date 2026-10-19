//! Measures NativeRenderer::render() cost for a synthetic 1920x1080 stream.
//! Usage (inside a graphical session): `cargo run --release -p renderer-linux
//! --example render_bench -- [frames] [windowed|full] [pace_ms]`. Not a product code path.
#[cfg(target_os = "linux")]
fn main() {
    use media_core::{DecodedFrameRef, DecodedPixelFormat, Renderer};
    use std::time::Instant;

    let mut args = std::env::args().skip(1);
    let frames: usize = args.next().and_then(|v| v.parse().ok()).unwrap_or(300);
    let windowed = args.next().as_deref() == Some("windowed");
    let pace_ms: u64 = args.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let (width, height) = (1920u32, 1080u32);
    let mut renderer = if windowed {
        renderer_linux::NativeRenderer::new(width, height, "render bench")
    } else {
        renderer_linux::NativeRenderer::new_student_presentation(width, height, "render bench")
    }
    .expect("renderer");
    let mut bytes = vec![0u8; (width * height * 4) as usize];
    let mut times = Vec::with_capacity(frames);
    for frame in 0..frames {
        for (index, pixel) in bytes.chunks_exact_mut(4).enumerate() {
            let x = (index % width as usize) as u32;
            pixel.copy_from_slice(&[(x as usize + frame * 7) as u8, (frame * 3) as u8, (index >> 11) as u8, 255]);
        }
        let started = Instant::now();
        renderer
            .render(DecodedFrameRef { format: DecodedPixelFormat::Bgra, bytes: &bytes, width, height })
            .expect("render");
        times.push(started.elapsed().as_secs_f64() * 1000.0);
        let _ = renderer.is_open();
        if pace_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(pace_ms));
        }
    }
    times.sort_by(f64::total_cmp);
    let pick = |q: f64| times[((times.len() - 1) as f64 * q) as usize];
    println!(
        "render ms: p50 {:.2} p95 {:.2} max {:.2} (frames {frames}, windowed {windowed})",
        pick(0.5), pick(0.95), times[times.len() - 1]
    );
}
#[cfg(not(target_os = "linux"))]
fn main() {}
