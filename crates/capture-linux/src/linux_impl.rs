use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use media_core::{
    CaptureConfig, CaptureSource, CaptureStatistics, MediaError, VideoFrame,
};
use pipewire as pw;
use pw::{
    spa::{
        param::{
            format::{FormatProperties, MediaSubtype, MediaType},
            format_utils,
            video::{VideoFormat, VideoInfoRaw},
            ParamType,
        },
        pod::{self, serialize::PodSerializer, Pod},
        utils::{Direction, SpaTypes},
    },
    stream::{Stream, StreamFlags},
};

use crate::{
    mutter::{Cursor, Session},
    scale,
};

#[derive(Default)]
struct Slot {
    frame: Option<VideoFrame>,
    dimensions: Option<(u32, u32)>,
    error: Option<String>,
}

#[derive(Default)]
struct Shared {
    slot: Mutex<Slot>,
    ready: Condvar,
    acquired: AtomicU64,
    replaced: AtomicU64,
    closed: AtomicBool,
}

/// Capture of one GNOME monitor through Mutter ScreenCast + PipeWire.
pub struct LinuxCapture {
    shared: Arc<Shared>,
    quit: Option<pw::channel::Sender<()>>,
    thread: Option<JoinHandle<()>>,
    dimensions: Option<(u32, u32)>,
}

impl LinuxCapture {
    #[must_use]
    pub fn new() -> Self {
        Self { shared: Arc::new(Shared::default()), quit: None, thread: None, dimensions: None }
    }

    fn shutdown(&mut self) {
        if let Some(quit) = self.quit.take() {
            let _ = quit.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Default for LinuxCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for LinuxCapture {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn operation(message: impl std::fmt::Display) -> MediaError {
    MediaError::Operation(format!("Linux capture: {message}"))
}

struct UserData {
    shared: Arc<Shared>,
    format: VideoInfoRaw,
    max_w: u32,
    max_h: u32,
    min_interval: Duration,
    last_frame: Option<Instant>,
    scratch: Vec<u8>,
}

fn run(
    shared: &Arc<Shared>,
    config: CaptureConfig,
    ready: &mpsc::Sender<Result<(), String>>,
    quit_rx: pw::channel::Receiver<()>,
) -> Result<(), String> {
    // Monitor selection by index needs DisplayConfig; the primary monitor is used.
    if config.display_index != 0 {
        return Err("only the primary display can be captured on Ubuntu for now".into());
    }
    let mut session = Session::open("", Cursor::Embedded, None)?;
    pw::init();
    let mainloop = pw::main_loop::MainLoop::new(None).map_err(|e| e.to_string())?;
    let context = pw::context::Context::new(&mainloop).map_err(|e| e.to_string())?;
    let core = context.connect(None).map_err(|e| e.to_string())?;
    let stream = Stream::new(
        &core,
        "classroom-student-capture",
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )
    .map_err(|e| e.to_string())?;
    let data = UserData {
        shared: Arc::clone(shared),
        format: VideoInfoRaw::default(),
        max_w: config.width,
        max_h: config.height,
        min_interval: Duration::from_secs_f64(1.0 / f64::from(config.max_fps.max(1))),
        last_frame: None,
        scratch: Vec::new(),
    };
    let _listener = stream
        .add_local_listener_with_user_data(data)
        .state_changed({
            let ready = ready.clone();
            move |_, _, _old, new| {
                if let pw::stream::StreamState::Error(error) = new {
                    let _ = ready.send(Err(format!("PipeWire stream error: {error}")));
                }
            }
        })
        .param_changed(|_, user, id, param| {
            let Some(param) = param else { return };
            if id != ParamType::Format.as_raw() {
                return;
            }
            let Ok((media_type, subtype)) = format_utils::parse_format(param) else { return };
            if media_type != MediaType::Video || subtype != MediaSubtype::Raw {
                return;
            }
            let _ = user.format.parse(param);
            let size = user.format.size();
            let (w, h) = scale::fit(size.width, size.height, user.max_w, user.max_h);
            let mut slot = user.shared.slot.lock().unwrap();
            slot.dimensions = Some((w, h));
        })
        .process(|stream, user| {
            let Some(mut buffer) = stream.dequeue_buffer() else { return };
            let started = Instant::now();
            // Honour the requested cadence: drop frames that arrive too soon.
            if user.last_frame.is_some_and(|at| started.duration_since(at) < user.min_interval) {
                return;
            }
            let size = user.format.size();
            let (sw, sh) = (size.width as usize, size.height as usize);
            let datas = buffer.datas_mut();
            let Some(data) = datas.first_mut() else { return };
            let stride = usize::try_from(data.chunk().stride()).unwrap_or(sw * 4);
            let Some(bytes) = data.data() else { return };
            if sw == 0 || sh == 0 || stride < sw * 4 || bytes.len() < stride * (sh - 1) + sw * 4 {
                return;
            }
            let format = user.format.format();
            let swap = format == VideoFormat::RGBx || format == VideoFormat::RGBA;
            let (dw, dh) = scale::fit(size.width, size.height, user.max_w, user.max_h);
            let (dw, dh) = (dw as usize, dh as usize);
            let mut out = vec![0_u8; dw * dh * 4];
            if (dw, dh) == (sw, sh) {
                for row in 0..sh {
                    out[row * dw * 4..(row + 1) * dw * 4]
                        .copy_from_slice(&bytes[row * stride..row * stride + sw * 4]);
                }
                for px in out.chunks_exact_mut(4) {
                    if swap {
                        px.swap(0, 2);
                    }
                    px[3] = 255;
                }
            } else {
                scale::downscale_bgra(bytes, sw, sh, stride, &mut out, dw, dh);
                if swap {
                    for px in out.chunks_exact_mut(4) {
                        px.swap(0, 2);
                    }
                }
            }
            user.scratch.clear();
            user.last_frame = Some(started);
            let frame = VideoFrame {
                bytes: out,
                captured_at: started,
                width: dw as u32,
                height: dh as u32,
                row_pitch: (dw * 4) as u32,
                dirty_region_ratio: 1.0,
                accumulated_frames: 1,
                pointer_visible: true,
                pointer_composited: true,
                protected_content_masked_out: false,
                capture_latency: started.elapsed(),
            };
            let mut slot = user.shared.slot.lock().unwrap();
            if slot.frame.replace(frame).is_some() {
                user.shared.replaced.fetch_add(1, Ordering::Relaxed);
            }
            slot.dimensions = Some((dw as u32, dh as u32));
            user.shared.acquired.fetch_add(1, Ordering::Relaxed);
            user.shared.ready.notify_all();
        })
        .register()
        .map_err(|e| e.to_string())?;

    // Offer packed 32-bit formats and any size/rate; no modifiers, so Mutter
    // supplies CPU-mappable shared memory rather than DMA-BUF.
    let object = pod::object!(
        SpaTypes::ObjectParamFormat,
        ParamType::EnumFormat,
        pod::property!(FormatProperties::MediaType, Id, MediaType::Video),
        pod::property!(FormatProperties::MediaSubtype, Id, MediaSubtype::Raw),
        pod::property!(
            FormatProperties::VideoFormat,
            Choice,
            Enum,
            Id,
            VideoFormat::BGRx,
            VideoFormat::BGRx,
            VideoFormat::BGRA,
            VideoFormat::RGBx,
            VideoFormat::RGBA
        ),
        pod::property!(
            FormatProperties::VideoSize,
            Choice,
            Range,
            Rectangle,
            pw::spa::utils::Rectangle { width: 1280, height: 720 },
            pw::spa::utils::Rectangle { width: 1, height: 1 },
            pw::spa::utils::Rectangle { width: 8192, height: 8192 }
        ),
        pod::property!(
            FormatProperties::VideoFramerate,
            Choice,
            Range,
            Fraction,
            pw::spa::utils::Fraction { num: 30, denom: 1 },
            pw::spa::utils::Fraction { num: 0, denom: 1 },
            pw::spa::utils::Fraction { num: 120, denom: 1 }
        ),
    );
    let values: Vec<u8> = PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pod::Value::Object(object),
    )
    .map_err(|e| e.to_string())?
    .0
    .into_inner();
    let mut params = [Pod::from_bytes(&values).ok_or("invalid format pod")?];
    stream
        .connect(
            Direction::Input,
            Some(session.node_id),
            StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS,
            &mut params,
        )
        .map_err(|e| e.to_string())?;
    let _ = ready.send(Ok(()));
    let loop_handle = mainloop.clone();
    let _quit = quit_rx.attach(mainloop.loop_(), move |()| loop_handle.quit());
    mainloop.run();
    session.stop();
    Ok(())
}

impl CaptureSource for LinuxCapture {
    fn start(&mut self, config: CaptureConfig) -> Result<(), MediaError> {
        if config.width == 0 || config.height == 0 || config.max_fps == 0 {
            return Err(operation("dimensions and frame rate must be non-zero"));
        }
        self.shutdown();
        self.shared = Arc::new(Shared::default());
        let (quit_tx, quit_rx) = pw::channel::channel::<()>();
        let (ready_tx, ready_rx) = mpsc::channel();
        let shared = Arc::clone(&self.shared);
        self.thread = Some(
            thread::Builder::new()
                .name("linux-capture".into())
                .spawn(move || {
                    if let Err(error) = run(&shared, config, &ready_tx, quit_rx) {
                        let _ = ready_tx.send(Err(error.clone()));
                        shared.slot.lock().unwrap().error = Some(error);
                    }
                    shared.closed.store(true, Ordering::Release);
                    shared.ready.notify_all();
                })
                .map_err(operation)?,
        );
        self.quit = Some(quit_tx);
        match ready_rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                self.shutdown();
                return Err(operation(error));
            }
            Err(_) => {
                self.shutdown();
                return Err(operation("timed out starting the screen stream"));
            }
        }
        // Dimensions are known once the stream negotiates its first frame.
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut slot = self.shared.slot.lock().unwrap();
        while slot.dimensions.is_none() && Instant::now() < deadline {
            slot = self
                .shared
                .ready
                .wait_timeout(slot, Duration::from_millis(100))
                .unwrap()
                .0;
        }
        self.dimensions = slot.dimensions;
        Ok(())
    }

    fn output_dimensions(&self) -> Option<(u32, u32)> {
        self.dimensions
            .or_else(|| self.shared.slot.lock().unwrap().dimensions)
    }

    fn next_frame(&mut self, deadline: Instant) -> Result<Option<VideoFrame>, MediaError> {
        let mut slot = self.shared.slot.lock().unwrap();
        loop {
            if let Some(frame) = slot.frame.take() {
                self.dimensions = Some((frame.width, frame.height));
                return Ok(Some(frame));
            }
            if let Some(error) = &slot.error {
                return Err(operation(error));
            }
            if self.shared.closed.load(Ordering::Acquire) {
                return Err(operation("screen stream closed"));
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(None);
            }
            slot = self.shared.ready.wait_timeout(slot, deadline - now).unwrap().0;
        }
    }

    fn statistics(&self) -> CaptureStatistics {
        CaptureStatistics {
            frames_acquired: self.shared.acquired.load(Ordering::Relaxed),
            latest_frame_replacements: self.shared.replaced.load(Ordering::Relaxed),
            ..CaptureStatistics::default()
        }
    }
}
