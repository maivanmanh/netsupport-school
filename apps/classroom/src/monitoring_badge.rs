//! Persistent, explicit Teacher notice. Network loss never hides this indicator.
#![allow(unsafe_code)] // Owned native UI handles; all calls stay on StudentDesktop UI thread.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
#[cfg(windows)]
use std::path::PathBuf;
use std::{
    path::Path,
    time::{Duration, Instant},
};

#[cfg(windows)]
pub const TEXT: &str = "ĐANG GIÁM SÁT";
pub fn supported() -> bool {
    cfg!(any(windows, target_os = "macos"))
}
pub fn pulse(seconds: f64) -> f64 {
    0.65 + 0.35 * (seconds * std::f64::consts::PI).cos()
}
#[derive(Default, Serialize, Deserialize)]
pub struct Preference {
    pub enabled: bool,
}
#[derive(Default)]
pub struct Request {
    pub enabled: bool,
    pub revision: Option<String>,
    pub doc: Option<classroom_store::Doc>,
}
impl Request {
    pub fn load(&mut self, profile: &Path) -> Result<()> {
        let doc = crate::startup::profile_doc(profile, "badge")?;
        if let Some(saved) = doc.read::<Preference>()? {
            self.enabled = saved.enabled;
        }
        self.doc = Some(doc);
        Ok(())
    }
    pub fn persist(&self) -> Result<()> {
        self.doc
            .as_ref()
            .context("Badge preference storage unavailable")?
            .write(&Preference {
                enabled: self.enabled,
            })?;
        Ok(())
    }
}

#[derive(Default)]
pub struct Badge {
    native: Option<Native>,
    started: Option<Instant>,
    last_draw: Option<Instant>,
    retry: Option<Instant>,
    observed: Option<bool>,
}
impl Badge {
    pub fn pump(&mut self, inbox: &crate::classroom_tools::Inbox, presenting: bool) {
        let (enabled, revision) = {
            let r = inbox.badge.lock().unwrap();
            (r.enabled, r.revision.clone())
        };
        let changed = self.observed != Some(enabled) || revision.is_some();
        if !enabled {
            self.native = None;
            self.started = None;
            self.observed = Some(false);
            inbox.badge_broadcast_suppression_applied.store(
                source_broadcast(&inbox.badge_broadcast_suppressed),
                std::sync::atomic::Ordering::Release,
            );
            if changed {
                self.finish(inbox, revision, false, Ok(()));
            }
            return;
        }
        let source_suppressed = source_broadcast(&inbox.badge_broadcast_suppressed);
        if presenting || source_suppressed {
            // Suppression is temporary. Keep/persist the Teacher's desired ON
            // state and acknowledge it, but destroy the native window so it is
            // neither overlaid on a received presentation nor captured from
            // this Student when acting as the class source.
            self.native = None;
            self.started = None;
            self.last_draw = None;
            self.observed = Some(true);
            inbox
                .badge_broadcast_suppression_applied
                .store(source_suppressed, std::sync::atomic::Ordering::Release);
            if revision.is_some() {
                self.finish(inbox, revision, true, Ok(()));
            }
            return;
        }
        if self
            .retry
            .is_some_and(|at| at.elapsed() < Duration::from_secs(5))
        {
            return;
        }
        if self.native.is_none() {
            match Native::new() {
                Ok(native) => {
                    self.native = Some(native);
                    self.started = Some(Instant::now());
                    self.retry = None;
                }
                Err(e) => {
                    self.retry = Some(Instant::now());
                    self.finish(inbox, revision, false, Err(e));
                    return;
                }
            }
        }
        inbox
            .badge_broadcast_suppression_applied
            .store(false, std::sync::atomic::Ordering::Release);
        if !changed
            && self
                .last_draw
                .is_some_and(|at| at.elapsed() < Duration::from_millis(33))
        {
            return;
        }
        self.last_draw = Some(Instant::now());
        let opacity = pulse(self.started.unwrap().elapsed().as_secs_f64());
        let result = self.native.as_mut().unwrap().draw(opacity);
        if result.is_err() {
            self.native = None;
            self.retry = Some(Instant::now());
        }
        if changed || result.is_err() {
            self.finish(inbox, revision, result.is_ok(), result);
        }
    }
    fn finish(
        &mut self,
        inbox: &crate::classroom_tools::Inbox,
        revision: Option<String>,
        visible: bool,
        result: Result<()>,
    ) {
        self.observed = Some(visible);
        let mut request = inbox.badge.lock().unwrap();
        if request.revision != revision {
            return;
        }
        let persisted = request.persist();
        let result = result.and(persisted);
        let succeeded = result.is_ok();
        if !succeeded {
            self.retry = Some(Instant::now());
        } else {
            self.retry = None;
        }
        if let Some(revision) = revision {
            let mut ack = protocol::ToolPacket {
                version: 1,
                action: "monitoring_badge_ack".into(),
                id: revision,
                ..Default::default()
            };
            ack.status = if result.is_ok() { "complete" } else { "error" }.into();
            ack.done = u64::from(visible);
            if let Err(e) = result {
                ack.text = format!("{e:#}").chars().take(300).collect();
            }
            inbox.badge_ack(ack);
            if succeeded {
                request.revision = None;
            }
        } else if let Err(e) = result {
            tracing::warn!(error=%e, "Monitoring badge restore failed");
        }
    }
}

fn source_broadcast(flag: &std::sync::atomic::AtomicBool) -> bool {
    flag.load(std::sync::atomic::Ordering::Acquire)
}

#[cfg(target_os = "macos")]
struct Native(*mut std::ffi::c_void);
#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn mfb_monitoring_create() -> *mut std::ffi::c_void;
    fn mfb_monitoring_draw(ptr: *mut std::ffi::c_void, opacity: f64) -> bool;
    fn mfb_monitoring_destroy(ptr: *mut std::ffi::c_void);
}
#[cfg(target_os = "macos")]
impl Native {
    fn new() -> Result<Self> {
        // SAFETY: invoked only by StudentDesktop's main UI thread.
        let ptr = unsafe { mfb_monitoring_create() };
        anyhow::ensure!(!ptr.is_null(), "Could not create monitoring NSPanel");
        Ok(Self(ptr))
    }
    fn draw(&mut self, opacity: f64) -> Result<()> {
        anyhow::ensure!(
            unsafe { mfb_monitoring_draw(self.0, opacity) },
            "Monitoring NSPanel display failed"
        );
        Ok(())
    }
}
#[cfg(target_os = "macos")]
impl Drop for Native {
    fn drop(&mut self) {
        unsafe { mfb_monitoring_destroy(self.0) };
    }
}

#[cfg(windows)]
struct DpiScope(windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT);
#[cfg(windows)]
impl DpiScope {
    fn new() -> Result<Self> {
        use windows::Win32::UI::HiDpi::{
            SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        };
        // Scope the UI call instead of changing DPI behavior of capture/other windows.
        let previous =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        anyhow::ensure!(!previous.0.is_null(), "Monitoring badge DPI context failed");
        Ok(Self(previous))
    }
}
#[cfg(windows)]
impl Drop for DpiScope {
    fn drop(&mut self) {
        unsafe {
            windows::Win32::UI::HiDpi::SetThreadDpiAwarenessContext(self.0);
        }
    }
}
#[cfg(windows)]
struct Native {
    window: minifb::Window,
    font: fontdue::Font,
    density: f32,
    width: usize,
    height: usize,
    base: Vec<u32>,
    pixels: Vec<u32>,
}
#[cfg(windows)]
impl Native {
    fn new() -> Result<Self> {
        let _dpi = DpiScope::new()?;
        let root = std::env::var_os("WINDIR").context("Windows font directory unavailable")?;
        let bytes = std::fs::read(PathBuf::from(root).join("Fonts/seguisb.ttf"))?;
        let font = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default())
            .map_err(anyhow::Error::msg)?;
        let mut window = minifb::Window::new(
            "Classroom monitoring notice",
            180,
            32,
            minifb::WindowOptions {
                visible: false,
                none: true,
                borderless: true,
                title: false,
                resize: false,
                ..Default::default()
            },
        )?;
        window.set_target_fps(0);
        let style = window.extended_style();
        let required = (windows::Win32::UI::WindowsAndMessaging::WS_EX_LAYERED.0
            | windows::Win32::UI::WindowsAndMessaging::WS_EX_TRANSPARENT.0
            | windows::Win32::UI::WindowsAndMessaging::WS_EX_NOACTIVATE.0
            | windows::Win32::UI::WindowsAndMessaging::WS_EX_TOOLWINDOW.0)
            as isize;
        anyhow::ensure!(
            style & required == required,
            "Monitoring window missing passive extended styles: {style:#x}"
        );
        Ok(Self {
            window,
            font,
            density: 0.0,
            width: 0,
            height: 0,
            base: vec![],
            pixels: vec![],
        })
    }
    fn draw(&mut self, opacity: f64) -> Result<()> {
        let _dpi = DpiScope::new()?;
        use windows::Win32::UI::HiDpi::GetDpiForWindow;
        // SAFETY: minifb owns this HWND and all calls occur on its UI thread.
        let dpi = unsafe {
            GetDpiForWindow(windows::Win32::Foundation::HWND(
                self.window.get_window_handle() as _,
            ))
        };
        let density = (dpi.max(96) as f32 / 96.0).clamp(1.0, 4.0);
        if self.density != density {
            self.density = density;
            let px = 12.0 * density;
            let text_width: f32 = TEXT
                .chars()
                .map(|c| self.font.metrics(c, px).advance_width)
                .sum();
            self.width = (text_width + 40.0 * density).ceil() as usize;
            self.height = (32.0 * density).ceil() as usize;
            self.window.resize_notification(self.width, self.height)?;
            self.base = background(self.width, self.height, density as f64);
            let mut x = 28.0 * density;
            let lines = self
                .font
                .horizontal_line_metrics(px)
                .context("Font has no line metrics")?;
            let baseline =
                (self.height as f32 - (lines.ascent - lines.descent)) / 2.0 + lines.ascent;
            for ch in TEXT.chars() {
                let (m, mask) = self.font.rasterize(ch, px);
                let left = x.round() as i32 + m.xmin;
                let top = baseline.round() as i32 - m.height as i32 - m.ymin;
                for y in 0..m.height {
                    for xx in 0..m.width {
                        let dx = left + xx as i32;
                        let dy = top + y as i32;
                        if dx >= 0
                            && dy >= 0
                            && (dx as usize) < self.width
                            && (dy as usize) < self.height
                        {
                            let i = dy as usize * self.width + dx as usize;
                            self.base[i] =
                                white_over(self.base[i], mask[y * m.width + xx] as f64 / 255.0);
                        }
                    }
                }
                x += m.advance_width;
            }
            self.pixels.resize(self.base.len(), 0);
        }
        self.pixels.copy_from_slice(&self.base);
        let d = self.density as f64;
        for y in 0..self.height {
            for x in 0..self.width {
                let distance = ((x as f64 + 0.5 - 16.0 * d).powi(2)
                    + (y as f64 + 0.5 - self.height as f64 / 2.0).powi(2))
                .sqrt();
                let coverage = (4.0 * d + 0.5 - distance).clamp(0.0, 1.0) * opacity;
                let i = y * self.width + x;
                self.pixels[i] = white_over(self.pixels[i], coverage);
            }
        }
        let [x, y, w, _] = self.window.primary_overlay_geometry();
        self.window.set_outer_origin(
            x + w - self.width as isize - (16.0 * d).round() as isize,
            y + (16.0 * d).round() as isize,
        );
        self.window
            .update_alpha_overlay_opacity(&self.pixels, self.width, self.height, 255)?;
        self.window.show_inactive_topmost();
        self.window.update();
        let style = self.window.extended_style();
        let required = (windows::Win32::UI::WindowsAndMessaging::WS_EX_LAYERED.0
            | windows::Win32::UI::WindowsAndMessaging::WS_EX_TRANSPARENT.0
            | windows::Win32::UI::WindowsAndMessaging::WS_EX_NOACTIVATE.0
            | windows::Win32::UI::WindowsAndMessaging::WS_EX_TOOLWINDOW.0)
            as isize;
        anyhow::ensure!(
            style & required == required,
            "Monitoring window lost passive extended styles: {style:#x}"
        );
        Ok(())
    }
}
#[cfg(any(windows, test))]
fn white_over(pixel: u32, opacity: f64) -> u32 {
    let a = (opacity.clamp(0.0, 1.0) * 255.0).round() as u32;
    let blend = |shift: u32| a + (((pixel >> shift) & 255u32) * (255 - a) + 127) / 255;
    (blend(24) << 24) | (blend(16) << 16) | (blend(8) << 8) | blend(0)
}
#[cfg(any(windows, test))]
fn background(width: usize, height: usize, density: f64) -> Vec<u32> {
    let radius = 6.0 * density;
    (0..width * height)
        .map(|i| {
            let x = (i % width) as f64 + 0.5;
            let y = (i / width) as f64 + 0.5;
            let dx = (x - width as f64 / 2.0).abs() - (width as f64 / 2.0 - radius);
            let dy = (y - height as f64 / 2.0).abs() - (height as f64 / 2.0 - radius);
            let distance = dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) - radius;
            let a = ((0.5 - distance).clamp(0.0, 1.0) * 0.75 * 255.0).round() as u32;
            (a << 24) | ((201 * a / 255) << 16) | ((31 * a / 255) << 8) | (40 * a / 255)
        })
        .collect()
}
#[cfg(not(any(windows, target_os = "macos")))]
struct Native;
#[cfg(not(any(windows, target_os = "macos")))]
impl Native {
    fn new() -> Result<Self> {
        anyhow::bail!("Monitoring badge unsupported")
    }
    fn draw(&mut self, _: f64) -> Result<()> {
        anyhow::bail!("Monitoring badge unsupported")
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pulse_is_two_seconds_and_bounded() {
        assert!((pulse(0.0) - 1.0).abs() < 1e-9);
        assert!((pulse(1.0) - 0.3).abs() < 1e-9);
        for n in 0..1000 {
            let t = n as f64 / 73.0;
            assert!((pulse(t) - pulse(t + 2.0)).abs() < 1e-12);
            assert!((0.3..=1.0).contains(&pulse(t)));
        }
    }
    #[test]
    fn alpha_background_does_not_dim_foreground() {
        let p = background(180, 32, 1.0);
        assert_eq!(p[0], 0);
        assert_eq!(p[16 * 180 + 90] >> 24, 191);
        assert_eq!(white_over(p[16 * 180 + 90], 1.0), 0xffffffff);
        assert_ne!(white_over(p[16 * 180 + 90], 0.3), p[16 * 180 + 90]);
    }
}

pub fn atomic_replace(from: &Path, to: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::{
            core::PCWSTR,
            Win32::Storage::FileSystem::{
                MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
            },
        };
        let a: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
        let b: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            MoveFileExW(
                PCWSTR(a.as_ptr()),
                PCWSTR(b.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )?;
        }
    }
    #[cfg(not(windows))]
    std::fs::rename(from, to)?;
    Ok(())
}
