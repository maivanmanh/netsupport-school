//! Capability-negotiated media profile for one-to-one sessions (Observe).
//!
//! The Teacher offers the codecs its hardware can decode plus a frame-rate
//! ceiling; the Student picks the first offered codec its hardware can encode
//! and raises the frame rate only when that encode path is hardware-backed.
//! Anything unsupported keeps the class-broadcast profile (H.264, `video`
//! settings), so older or weaker peers behave exactly as before. Classroom
//! broadcast never uses this module and stays H.264 for every Student.
use classroom_config::{Config, ObserveConfig};
use media_core::VideoCodec;

/// The codec a media session's config selects (empty means H.264).
pub fn session_codec(video: &classroom_config::VideoConfig) -> VideoCodec {
    VideoCodec::parse(&video.codec).unwrap_or_default()
}

/// Hardware encoder present for `codec` on this machine.
pub fn hardware_encode(codec: VideoCodec) -> bool {
    #[cfg(target_os = "macos")]
    {
        codec_macos_vt::hardware_encode_supported(codec)
    }
    #[cfg(not(target_os = "macos"))]
    {
        // Media Foundation codec probing is a future backend; until then
        // Windows keeps the proven H.264 broadcast profile for Observe.
        let _ = codec;
        false
    }
}

/// This build can decode `codec` (H.264 everywhere, others where wired).
pub fn local_decodable(codec: VideoCodec) -> bool {
    match codec {
        VideoCodec::H264 => true,
        #[cfg(target_os = "macos")]
        VideoCodec::H265 => codec_macos_vt::hardware_decode_supported(codec),
        #[cfg(not(target_os = "macos"))]
        VideoCodec::H265 => false,
    }
}

/// This build can encode `codec` (Windows/macOS only; Linux is receive-only).
pub fn local_encodable(codec: VideoCodec) -> bool {
    cfg!(any(windows, target_os = "macos")) && (codec == VideoCodec::H264 || hardware_encode(codec))
}

fn allowed(observe: &ObserveConfig, codec: VideoCodec) -> bool {
    observe.codec.eq_ignore_ascii_case("auto")
        || VideoCodec::parse(&observe.codec) == Some(codec)
        || codec == VideoCodec::H264
}

/// Teacher side: codecs to offer (preferred first) and the fps ceiling.
/// An empty offer asks for the default profile.
pub fn teacher_offer(
    observe: &ObserveConfig,
    decodable: impl Fn(VideoCodec) -> bool,
) -> (Vec<String>, u32) {
    if !observe.enhanced {
        return (Vec::new(), 0);
    }
    let codecs = VideoCodec::PREFERENCE
        .into_iter()
        .filter(|&codec| allowed(observe, codec) && decodable(codec))
        .map(|codec| codec.name().to_owned())
        .collect();
    (codecs, observe.max_fps.clamp(1, 60))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionProfile {
    pub codec: VideoCodec,
    pub max_fps: u32,
    pub bitrate_kbps: u32,
    /// True when anything differs from the broadcast profile.
    pub enhanced: bool,
}
impl SessionProfile {
    pub fn label(&self) -> String {
        format!("{}@{}", self.codec.name(), self.max_fps)
    }
    /// Writes the profile into a per-session config copy.
    pub fn apply(&self, settings: &mut Config) {
        self.codec.name().clone_into(&mut settings.video.codec);
        if !self.enhanced {
            return;
        }
        settings.video.max_fps = self.max_fps;
        settings.video.target_bitrate_kbps = self.bitrate_kbps;
        settings.video.max_bitrate_kbps = settings.video.max_bitrate_kbps.max(self.bitrate_kbps);
        // Interactive 1-1 view: keep motion fluid while typing/moving windows.
        settings.video.adaptive_typing_fps = self.max_fps;
    }
}

/// Student side: choose the session profile for a Teacher offer.
pub fn choose(
    offer: &[String],
    offer_fps: u32,
    config: &Config,
    encodable: impl Fn(VideoCodec) -> bool,
    hardware: impl Fn(VideoCodec) -> bool,
) -> SessionProfile {
    let baseline = SessionProfile {
        codec: VideoCodec::H264,
        max_fps: config.video.max_fps,
        bitrate_kbps: config.video.target_bitrate_kbps,
        enhanced: false,
    };
    if !config.observe.enhanced || offer.is_empty() || offer_fps == 0 {
        return baseline;
    }
    let Some(codec) = offer
        .iter()
        .filter_map(|name| VideoCodec::parse(name))
        .find(|&codec| allowed(&config.observe, codec) && encodable(codec))
    else {
        return baseline;
    };
    // Higher frame rates only with a hardware encoder for the chosen codec.
    if !hardware(codec) {
        return baseline;
    }
    let max_fps = offer_fps
        .min(config.observe.max_fps)
        .clamp(config.video.max_fps.min(60), 60);
    let bitrate_kbps = if config.observe.bitrate_kbps > 0 {
        config.observe.bitrate_kbps
    } else {
        // Scale with frame rate (sub-linearly: consecutive screen frames are
        // highly similar) and credit H.265's better compression.
        let base = u64::from(config.video.target_bitrate_kbps);
        let fps_scaled = base * (30 + u64::from(max_fps)) / 60;
        let efficiency = match codec {
            VideoCodec::H264 => 100,
            VideoCodec::H265 => 75,
        };
        u32::try_from(fps_scaled * efficiency / 100).unwrap_or(u32::MAX)
    };
    SessionProfile {
        codec,
        max_fps,
        bitrate_kbps,
        enhanced: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Config {
        Config::load(None).unwrap().0
    }
    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }
    #[test]
    fn mac_pair_negotiates_hevc_at_60() {
        let c = config();
        let (offer, fps) = teacher_offer(&c.observe, |_| true);
        assert_eq!(offer, names(&["h265", "h264"]));
        assert_eq!(fps, 60);
        let p = choose(&offer, fps, &c, |_| true, |_| true);
        assert_eq!(p.codec, VideoCodec::H265);
        assert_eq!(p.max_fps, 60);
        assert!(p.enhanced);
        assert_eq!(p.label(), "h265@60");
        // 6000 kbps * (30+60)/60 * 0.75
        assert_eq!(p.bitrate_kbps, 6750);
        let mut s = c.clone();
        p.apply(&mut s);
        assert_eq!(s.video.codec, "h265");
        assert_eq!(s.video.max_fps, 60);
        assert!(s.video.max_bitrate_kbps >= 6750);
    }
    #[test]
    fn student_without_hardware_keeps_broadcast_profile() {
        let c = config();
        let offer = names(&["h265", "h264"]);
        // Windows today: only H.264 encodable, no hardware probing.
        let p = choose(&offer, 60, &c, |codec| codec == VideoCodec::H264, |_| false);
        assert_eq!(
            p,
            SessionProfile {
                codec: VideoCodec::H264,
                max_fps: 30,
                bitrate_kbps: 6000,
                enhanced: false
            }
        );
    }
    #[test]
    fn hardware_h264_only_student_gets_fps_but_not_hevc() {
        let c = config();
        let p = choose(
            &names(&["h265", "h264"]),
            60,
            &c,
            |codec| codec == VideoCodec::H264,
            |codec| codec == VideoCodec::H264,
        );
        assert_eq!(
            (p.codec, p.max_fps, p.enhanced),
            (VideoCodec::H264, 60, true)
        );
        assert_eq!(p.bitrate_kbps, 9000);
    }
    #[test]
    fn teacher_without_hevc_decode_or_disabled_offers_less() {
        let mut c = config();
        let (offer, _) = teacher_offer(&c.observe, |codec| codec == VideoCodec::H264);
        assert_eq!(offer, names(&["h264"]));
        c.observe.codec = "h264".into();
        assert_eq!(teacher_offer(&c.observe, |_| true).0, names(&["h264"]));
        c.observe.enhanced = false;
        assert_eq!(teacher_offer(&c.observe, |_| true), (Vec::new(), 0));
        // Old Teacher (no offer) → default profile.
        assert!(!choose(&[], 0, &config(), |_| true, |_| true).enhanced);
    }
}
