use std::{
    fs,
    net::Ipv4Addr,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

const EMBEDDED_DEFAULTS: &str = include_str!("../../../config/default.toml");

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub app: AppConfig,
    pub network: NetworkConfig,
    pub video: VideoConfig,
    pub phase1: Phase1Config,
    #[serde(default)]
    pub monitoring: MonitoringConfig,
    #[serde(default)]
    pub observe: ObserveConfig,
    #[serde(default)]
    pub diagnostics: DiagnosticsConfig,
    #[serde(default)]
    pub tools: ToolsConfig,
    #[serde(default)]
    pub performance: PerformanceConfig,
    #[serde(default)]
    pub recording: RecordingConfig,
    #[serde(default)]
    pub class: ClassConfig,
    #[serde(default)]
    pub activity: ActivityConfig,
    #[serde(default)]
    pub transfer: TransferConfig,
}

/// Student screen recording quality, storage and safety limits. Quality values
/// travel to each Student with the Start command, so nothing is configured
/// per Student computer. The session length (`tools.recording_max_minutes`)
/// predates this section and stays where it was.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct RecordingConfig {
    /// Output height in pixels (width follows the screen's shape): 360, 480, 720 or 1080.
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    /// Length of each MP4 part.
    pub chunk_minutes: u32,
    /// Teacher folder for recordings; empty = the Desktop.
    pub folder: String,
    /// Free space (MiB) on the Teacher below which every recording stops / warns.
    pub teacher_stop_mb: u32,
    pub teacher_warn_mb: u32,
    /// The same for a Student's own disk.
    pub student_stop_mb: u32,
    pub student_warn_mb: u32,
    /// A Student that cannot reach the Teacher this long stops recording; 0 = never.
    pub gone_minutes: u32,
}
impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            height: 720,
            fps: 30,
            bitrate_kbps: 3000,
            chunk_minutes: 10,
            folder: String::new(),
            teacher_stop_mb: 1024,
            teacher_warn_mb: 5120,
            student_stop_mb: 512,
            student_warn_mb: 2048,
            gone_minutes: 30,
        }
    }
}

/// Class-management behaviour.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct ClassConfig {
    /// Warning given on a Student before a requested restart/shutdown happens.
    pub power_grace_seconds: u32,
}
impl Default for ClassConfig {
    fn default() -> Self {
        Self {
            power_grace_seconds: 15,
        }
    }
}

/// Activity monitoring ("Theo dõi hoạt động").
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct ActivityConfig {
    /// How often a Student looks at its windows and browser tabs.
    pub scan_seconds: u32,
    /// Applications shown on each roster tile.
    pub live_apps: u32,
    pub collect_clipboard: bool,
    pub collect_downloads: bool,
    /// Minimum gap between two "connection lost" alerts for the same Student.
    pub alert_cooldown_seconds: u32,
    /// While AI websites are not blocked, a Student on one of them gets a
    /// screenshot taken (at a well-chosen moment) as evidence.
    pub evidence: bool,
    /// Screenshots kept per site per Student per day.
    pub evidence_per_site: u32,
    /// How long the site must be in use before the first picture, when its page
    /// does not visibly change.
    pub evidence_first_seconds: u32,
    /// Least time between two pictures of the same site.
    pub evidence_gap_seconds: u32,
}
impl Default for ActivityConfig {
    fn default() -> Self {
        Self {
            scan_seconds: 6,
            live_apps: 3,
            collect_clipboard: true,
            collect_downloads: true,
            alert_cooldown_seconds: 30,
            evidence: true,
            evidence_per_site: 3,
            evidence_first_seconds: 60,
            evidence_gap_seconds: 180,
        }
    }
}

/// File sending.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct TransferConfig {
    /// Speed of each file sent to a Student, in KiB per second.
    pub rate_kib_s: u32,
}
impl Default for TransferConfig {
    fn default() -> Self {
        Self { rate_kib_s: 1000 }
    }
}

/// Class-wide trade of picture quality for network and Student-PC load (see
/// `docs/29_LOW_BANDWIDTH_MODE.md`). The video/monitoring/observe values the
/// Teacher chose are never rewritten; [`Config::with_profile`] derives what is
/// actually used.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default)]
pub struct PerformanceConfig {
    /// `normal` (nothing changes), `economy` or `minimal` (very weak).
    pub mode: String,
    /// While `mode` is `normal`, behave as `economy` on a wireless adapter.
    pub auto_wifi: bool,
    /// The most the Teacher sends to the whole class per second, in Mbps; 0 = no extra limit.
    pub bandwidth_cap_mbps: u32,
}
impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            mode: "normal".into(),
            auto_wifi: false,
            bandwidth_cap_mbps: 0,
        }
    }
}

/// Limits a mode puts on a setting. Applied with `min`, so a Teacher who chose
/// less keeps it.
struct Profile {
    max_width: u32,
    max_height: u32,
    max_fps: u32,
    typing_fps: u32,
    idle_fps: u32,
    target_kbps: u32,
    max_kbps: u32,
    min_kbps: u32,
    refine_percent: u32,
    observe_fps: u32,
    thumbnail_interval_ms: u32,
    thumbnail_width: u32,
    thumbnail_height: u32,
    thumbnail_quality: u8,
}
const ECONOMY: Profile = Profile {
    max_width: 1280,
    max_height: 720,
    max_fps: 20,
    typing_fps: 15,
    idle_fps: 4,
    target_kbps: 3000,
    max_kbps: 4000,
    min_kbps: 800,
    refine_percent: 150,
    observe_fps: 30,
    thumbnail_interval_ms: 4000,
    thumbnail_width: 360,
    thumbnail_height: 203,
    thumbnail_quality: 60,
};
const MINIMAL: Profile = Profile {
    max_width: 960,
    max_height: 540,
    max_fps: 12,
    typing_fps: 10,
    idle_fps: 3,
    target_kbps: 1200,
    max_kbps: 1800,
    min_kbps: 400,
    refine_percent: 100,
    observe_fps: 20,
    thumbnail_interval_ms: 6000,
    thumbnail_width: 240,
    thumbnail_height: 135,
    thumbnail_quality: 50,
};
/// What a bandwidth cap falls back to when each Student's share is under 900 kbps.
const TINY: Profile = Profile {
    max_width: 640,
    max_height: 360,
    max_fps: 10,
    typing_fps: 8,
    idle_fps: 2,
    target_kbps: 500,
    max_kbps: 700,
    min_kbps: 200,
    refine_percent: 100,
    observe_fps: 15,
    thumbnail_interval_ms: 8000,
    thumbnail_width: 240,
    thumbnail_height: 135,
    thumbnail_quality: 45,
};

/// Fits the picture size and frame rates inside `profile`, keeping the aspect.
fn limit_picture(v: &mut VideoConfig, profile: &Profile) {
    if v.width > profile.max_width || v.height > profile.max_height {
        let scale = (f64::from(profile.max_width) / f64::from(v.width))
            .min(f64::from(profile.max_height) / f64::from(v.height));
        // Even numbers, which the encoder needs.
        let even = |n: f64| ((n.round() as u32).max(2)) & !1;
        v.width = even(f64::from(v.width) * scale);
        v.height = even(f64::from(v.height) * scale);
    }
    v.max_fps = v.max_fps.min(profile.max_fps);
    v.adaptive_typing_fps = v.adaptive_typing_fps.min(profile.typing_fps).min(v.max_fps);
    v.adaptive_idle_fps = v.adaptive_idle_fps.min(profile.idle_fps).min(v.max_fps);
}

/// Lowest per-Student bitrate a bandwidth cap may impose.
const CAP_FLOOR_KBPS: u64 = 300;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct ToolsConfig {
    pub max_file_mb: u32,
    pub connection_popup: bool,
    /// A screen recording stops by itself after this many minutes (it is cut
    /// into 10-minute MP4 parts as it goes), so a forgotten one cannot run on.
    pub recording_max_minutes: u32,
}
impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            max_file_mb: 300,
            connection_popup: false,
            recording_max_minutes: 60,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct DiagnosticsConfig {
    /// Collect bounded, sanitized runtime logs after a presentation stops.
    pub auto_collect: bool,
    /// Passive rendered-FPS badge during class broadcast (not Observe receiver).
    pub student_fps_badge: bool,
}

impl Default for DiagnosticsConfig {
    fn default() -> Self {
        Self {
            auto_collect: false,
            student_fps_badge: true,
        }
    }
}

/// One-to-one Observe/Control tuning. When `enhanced`, the Teacher offers what
/// its hardware decodes and the Student picks the best profile its hardware
/// encodes; otherwise (or when unsupported) the class-broadcast profile is used.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct ObserveConfig {
    pub enhanced: bool,
    /// `auto` (best mutually hardware-supported), `h265` or `h264`.
    pub codec: String,
    /// Frame-rate ceiling for an enhanced session (hardware encode required).
    pub max_fps: u32,
    /// Target bitrate for an enhanced session; 0 derives it from `video`.
    pub bitrate_kbps: u32,
}
impl Default for ObserveConfig {
    fn default() -> Self {
        Self {
            enhanced: true,
            codec: "auto".into(),
            max_fps: 60,
            bitrate_kbps: 0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct MonitoringConfig {
    pub enabled: bool,
    pub interval_ms: u32,
    pub width: u32,
    pub height: u32,
    pub jpeg_quality: u8,
    /// Thumbnails are requested at `max(jpeg_quality, quality_floor)`; the floor
    /// keeps small text legible and only an economy mode lowers it.
    pub quality_floor: u8,
    /// Ask a Student that can for twice-sized thumbnails for sharper HiDPI tiles.
    pub hidpi: bool,
}
impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_ms: 2000,
            width: 480,
            height: 270,
            jpeg_quality: 65,
            quality_floor: 85,
            hidpi: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AppConfig {
    pub role: String,
    pub log_level: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NetworkConfig {
    pub teacher_ip: Ipv4Addr,
    pub teacher_ip_mode: String,
    /// A Student whose Teacher does not answer looks for it on its own subnet
    /// (and accepts a Teacher "knock"). The configured address is still tried
    /// first; turn this off to use only that address.
    #[serde(default = "default_true")]
    pub teacher_discovery: bool,
    pub control_port: u16,
    pub media_port: u16,
    pub feedback_port: u16,
    pub retransmission_cache_ms: u32,
    pub send_pacing_mbps: u32,
    #[serde(default = "default_send_au_deadline_ms")]
    pub send_au_deadline_ms: u32,
    #[serde(default = "default_send_keyframe_deadline_ms")]
    pub send_keyframe_deadline_ms: u32,
    pub bind_ip: String,
    pub interface: String,
    /// Teacher RTP fan-out worker threads (Students sharded by client id),
    /// running off the encode thread with a one-AU bounded queue each.
    #[serde(default = "default_fanout_threads")]
    pub fanout_threads: u32,
    /// Per-Student connected UDP sockets sharing the Teacher media port:
    /// avoids a route lookup per `sendto` (4-7x faster on macOS).
    #[serde(default = "default_true")]
    pub connected_fanout: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[allow(clippy::struct_excessive_bools)] // independent feature toggles
pub struct VideoConfig {
    pub width: u32,
    pub height: u32,
    pub max_fps: u32,
    pub target_bitrate_kbps: u32,
    pub min_bitrate_kbps: u32,
    pub max_bitrate_kbps: u32,
    pub keyframe_interval_ms: u32,
    pub jitter_buffer_ms: u32,
    pub nack_repair_ms: u32,
    pub adaptive: bool,
    pub adaptive_idle_fps: u32,
    pub adaptive_idle_after_ms: u32,
    pub adaptive_typing_fps: u32,
    pub adaptive_small_change_percent: f32,
    pub adaptive_large_change_percent: f32,
    pub adaptive_small_hold_ms: u32,
    pub adaptive_pressure_encode_ms: u32,
    /// Teacher process share of total logical-CPU capacity, not summed core usage.
    pub adaptive_pressure_cpu_percent: f32,
    pub adaptive_receiver_loss_percent: f32,
    pub adaptive_receiver_min_fps_percent: u32,
    pub adaptive_receiver_render_latency_max_ms: u32,
    pub adaptive_feedback_min_reporting_percent: u32,
    pub adaptive_high_loss_quorum_percent: u32,
    pub adaptive_recovery_min_reporting_percent: u32,
    pub adaptive_bitrate_step_down_percent: u32,
    pub adaptive_bitrate_step_up_percent: u32,
    pub adaptive_bitrate_cooldown_ms: u32,
    pub preserve_resolution_first: bool,
    /// Windows Student presentation geometry; missing in old profiles means stretch.
    #[serde(default)]
    pub presentation_preserve_aspect: bool,
    /// Elementary codec for this media session; empty/`h264` for classroom
    /// broadcast. Observe sessions set it after negotiation.
    #[serde(default)]
    pub codec: String,
    /// O1: send the pointer on its own channel and let receivers that support
    /// it draw it (the encode stops compositing only if all receivers can).
    #[serde(default = "default_true")]
    pub pointer_channel: bool,
    /// O2: bitrate for one refinement frame after activity stops, as a percent
    /// of the current target (0 disables).
    #[serde(default = "default_refine_percent")]
    pub refine_bitrate_percent: u32,
    /// O3: keyframe requests (losses, joins) within this window share one IDR.
    #[serde(default = "default_keyframe_coalesce_ms")]
    pub keyframe_request_coalesce_ms: u32,
    /// Encode a fresh capture as soon as the fps cap allows instead of on a
    /// free-running clock (removes up to one frame interval of wait).
    #[serde(default = "default_true")]
    pub capture_aligned_encode: bool,
}
const fn default_true() -> bool {
    true
}
const fn default_send_au_deadline_ms() -> u32 {
    120
}
fn default_send_keyframe_deadline_ms() -> u32 {
    300
}

fn default_fanout_threads() -> u32 {
    4
}
const fn default_refine_percent() -> u32 {
    250
}
const fn default_keyframe_coalesce_ms() -> u32 {
    400
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Phase1Config {
    pub audio: bool,
    pub multicast: bool,
    pub security_hardening: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Overrides {
    pub role: Option<String>,
    pub log_level: Option<String>,
    pub teacher_ip: Option<Ipv4Addr>,
    pub bind_ip: Option<String>,
    pub interface: Option<String>,
    pub control_port: Option<u16>,
    pub media_port: Option<u16>,
    pub feedback_port: Option<u16>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("unable to read configuration file {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid configuration: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("invalid configuration: {0}")]
    Validation(String),
}

impl Config {
    /// Loads embedded defaults and overlays an explicit or adjacent configuration file.
    ///
    /// # Errors
    ///
    /// Returns an error when the overlay cannot be read, TOML cannot be decoded, or the
    /// resulting configuration violates Phase 1 invariants.
    pub fn load(path: Option<&Path>) -> Result<(Self, Option<PathBuf>), ConfigError> {
        let mut merged = toml::from_str::<toml::Value>(EMBEDDED_DEFAULTS)?;
        let source = match path {
            Some(explicit) => Some(explicit.to_path_buf()),
            None => adjacent_config_path().filter(|candidate| candidate.is_file()),
        };

        if let Some(source_path) = &source {
            let contents = fs::read_to_string(source_path).map_err(|source| ConfigError::Read {
                path: source_path.clone(),
                source,
            })?;
            let overlay = toml::from_str::<toml::Value>(&contents)?;
            merge_toml(&mut merged, overlay);
        }

        let config = merged.try_into::<Self>().map_err(ConfigError::Parse)?;
        config.validate()?;
        Ok((config, source))
    }

    /// Applies command-line overrides and validates the resulting configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when an override makes the configuration invalid.
    pub fn apply_overrides(&mut self, overrides: Overrides) -> Result<(), ConfigError> {
        if let Some(role) = overrides.role {
            self.app.role = role;
        }
        if let Some(level) = overrides.log_level {
            self.app.log_level = level;
        }
        if let Some(ip) = overrides.teacher_ip {
            self.network.teacher_ip = ip;
            "explicit".clone_into(&mut self.network.teacher_ip_mode);
        }
        if let Some(ip) = overrides.bind_ip {
            self.network.bind_ip = ip;
        }
        if let Some(interface) = overrides.interface {
            self.network.interface = interface;
        }
        if let Some(port) = overrides.control_port {
            self.network.control_port = port;
        }
        if let Some(port) = overrides.media_port {
            self.network.media_port = port;
        }
        if let Some(port) = overrides.feedback_port {
            self.network.feedback_port = port;
        }
        self.validate()
    }

    /// Applies saved role settings after external defaults and before CLI overrides.
    /// The candidate is validated before changing the live configuration.
    ///
    /// # Errors
    /// Returns invalid TOML or configuration errors without changing `self`.
    pub fn apply_saved(&mut self, contents: &str) -> Result<(), ConfigError> {
        let mut merged = toml::Value::try_from(&*self)
            .map_err(|error| ConfigError::Validation(error.to_string()))?;
        merge_toml(&mut merged, toml::from_str(contents)?);
        let candidate: Self = merged.try_into()?;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    /// The mode in force and why: `("normal" | "economy" | "minimal", "" | "manual" | "wifi")`.
    #[must_use]
    pub fn active_mode(&self, wireless: bool) -> (&'static str, &'static str) {
        match self.performance.mode.as_str() {
            "minimal" => ("minimal", "manual"),
            "economy" => ("economy", "manual"),
            _ if self.performance.auto_wifi && wireless => ("economy", "wifi"),
            _ => ("normal", ""),
        }
    }

    /// This configuration with the performance mode applied. Values only go
    /// down; `normal` returns it unchanged. `wireless` is whether the Teacher's
    /// adapter is Wi-Fi (for `auto_wifi`).
    #[must_use]
    pub fn with_profile(&self, wireless: bool) -> Self {
        let profile = match self.active_mode(wireless).0 {
            "economy" => &ECONOMY,
            "minimal" => &MINIMAL,
            _ => return self.clone(),
        };
        let mut out = self.clone();
        let v = &mut out.video;
        limit_picture(v, profile);
        v.target_bitrate_kbps = v.target_bitrate_kbps.min(profile.target_kbps);
        v.max_bitrate_kbps = v.max_bitrate_kbps.min(profile.max_kbps).max(v.target_bitrate_kbps);
        v.min_bitrate_kbps = v.min_bitrate_kbps.min(profile.min_kbps).min(v.target_bitrate_kbps);
        v.refine_bitrate_percent = v.refine_bitrate_percent.min(profile.refine_percent);
        // One-to-one sessions use the class profile: a weak Student is the one
        // that would otherwise have to encode the 60 fps hardware profile.
        let o = &mut out.observe;
        o.enhanced = false;
        o.max_fps = o.max_fps.min(profile.observe_fps);
        let m = &mut out.monitoring;
        m.interval_ms = m.interval_ms.max(profile.thumbnail_interval_ms).min(10_000);
        m.width = m.width.min(profile.thumbnail_width);
        m.height = m.height.min(profile.thumbnail_height);
        m.jpeg_quality = m.jpeg_quality.min(profile.thumbnail_quality);
        m.quality_floor = m.quality_floor.min(profile.thumbnail_quality);
        m.hidpi = false;
        out
    }

    /// This configuration for one presentation to `recipients` Students: the
    /// class-wide bandwidth cap becomes a per-Student bitrate (75 % of the cap,
    /// shared out, never below 300 kbps; the rest is headroom for keyframes and
    /// repair) and also bounds the wire budget. A small share also shrinks the
    /// picture and the frame rate, because a large keyframe that does not fit the
    /// wire budget in time is dropped and nothing would arrive at all.
    #[must_use]
    pub fn for_stream(&self, recipients: usize) -> Self {
        let cap = self.performance.bandwidth_cap_mbps;
        if cap == 0 {
            return self.clone();
        }
        let mut out = self.clone();
        let each = (u64::from(cap) * 1000 * 75 / 100 / (recipients.max(1) as u64))
            .max(CAP_FLOOR_KBPS);
        let each = u32::try_from(each).unwrap_or(u32::MAX);
        let v = &mut out.video;
        match each {
            4000.. => {}
            2000..=3999 => limit_picture(v, &ECONOMY),
            900..=1999 => limit_picture(v, &MINIMAL),
            _ => limit_picture(v, &TINY),
        }
        v.target_bitrate_kbps = v.target_bitrate_kbps.min(each);
        v.max_bitrate_kbps = v.max_bitrate_kbps.min(each).max(v.target_bitrate_kbps);
        v.min_bitrate_kbps = v.min_bitrate_kbps.min(v.target_bitrate_kbps);
        // The wire budget is a safety limit, not a bitrate: the pacer lets the
        // video use 80 % of it, so it must sit well above what the Students are
        // allotted or keyframes and repair find no room and nothing is sent. Take
        // the cap, or the floor the Students were given if that is more.
        let allotted_mbps = (u64::from(each) * recipients.max(1) as u64).div_ceil(1000);
        let budget = u32::try_from(u64::from(cap).max(allotted_mbps) * 12 / 5).unwrap_or(u32::MAX);
        let n = &mut out.network;
        n.send_pacing_mbps = if n.send_pacing_mbps == 0 {
            budget
        } else {
            n.send_pacing_mbps.min(budget)
        };
        out
    }

    #[allow(clippy::too_many_lines)]
    fn validate(&self) -> Result<(), ConfigError> {
        if !(1..=4096).contains(&self.tools.max_file_mb) {
            return Err(ConfigError::Validation(
                "tools.max_file_mb must be 1..4096".into(),
            ));
        }
        if !(5..=480).contains(&self.tools.recording_max_minutes) {
            return Err(ConfigError::Validation(
                "tools.recording_max_minutes must be 5..480".into(),
            ));
        }
        let r = &self.recording;
        if ![360, 480, 720, 1080].contains(&r.height)
            || !(5..=30).contains(&r.fps)
            || !(500..=12000).contains(&r.bitrate_kbps)
            || !(1..=30).contains(&r.chunk_minutes)
        {
            return Err(ConfigError::Validation(
                "recording needs height 360/480/720/1080, fps 5..30, bitrate 500..12000 kbps, parts 1..30 min".into(),
            ));
        }
        // A part must stay under the 1 GiB transfer limit with room to spare.
        if u64::from(r.chunk_minutes) * u64::from(r.bitrate_kbps) > 100_000 {
            return Err(ConfigError::Validation(
                "recording.chunk_minutes x bitrate_kbps must not exceed 100000 (parts would be too large)".into(),
            ));
        }
        if !(256..=1_048_576).contains(&r.teacher_stop_mb)
            || !(256..=1_048_576).contains(&r.student_stop_mb)
            || r.teacher_warn_mb < r.teacher_stop_mb
            || r.student_warn_mb < r.student_stop_mb
            || r.teacher_warn_mb > 2_097_152
            || r.student_warn_mb > 2_097_152
        {
            return Err(ConfigError::Validation(
                "recording disk limits: stop >= 256 MiB and warning >= stop".into(),
            ));
        }
        if r.gone_minutes != 0 && !(5..=240).contains(&r.gone_minutes) {
            return Err(ConfigError::Validation(
                "recording.gone_minutes must be 0 (never) or 5..240".into(),
            ));
        }
        if r.folder.len() > 400 || r.folder.contains('\0') {
            return Err(ConfigError::Validation("recording.folder is invalid".into()));
        }
        let class = &self.class;
        if class.power_grace_seconds > 300 {
            return Err(ConfigError::Validation(
                "class.power_grace_seconds must be 0..300".into(),
            ));
        }
        let a = &self.activity;
        if !(3..=120).contains(&a.scan_seconds)
            || !(1..=5).contains(&a.live_apps)
            || !(10..=3600).contains(&a.alert_cooldown_seconds)
            || !(1..=10).contains(&a.evidence_per_site)
            || !(20..=600).contains(&a.evidence_first_seconds)
            || !(60..=1800).contains(&a.evidence_gap_seconds)
        {
            return Err(ConfigError::Validation(
                "activity needs scan_seconds 3..120, live_apps 1..5, alert_cooldown_seconds 10..3600, evidence_per_site 1..10, evidence_first_seconds 20..600, evidence_gap_seconds 60..1800".into(),
            ));
        }
        if !(128..=51_200).contains(&self.transfer.rate_kib_s) {
            return Err(ConfigError::Validation(
                "transfer.rate_kib_s must be 128..51200".into(),
            ));
        }
        let m = &self.monitoring;
        if !(1000..=10000).contains(&m.interval_ms)
            || !(2..=480).contains(&m.width)
            || !(2..=270).contains(&m.height)
            || !(30..=85).contains(&m.jpeg_quality)
            || !(30..=85).contains(&m.quality_floor)
            || (m.enabled
                && (self.network.control_port == u16::MAX
                    || [self.network.media_port, self.network.feedback_port]
                        .contains(&(self.network.control_port + 1))))
        {
            return Err(ConfigError::Validation(
                "invalid bounded monitoring settings or port collision".into(),
            ));
        }
        if !matches!(
            self.performance.mode.as_str(),
            "normal" | "economy" | "minimal"
        ) || !(self.performance.bandwidth_cap_mbps == 0
            || (2..=10_000).contains(&self.performance.bandwidth_cap_mbps))
        {
            return Err(ConfigError::Validation(
                "performance.mode must be normal, economy or minimal and bandwidth_cap_mbps 0 or 2..=10000"
                    .into(),
            ));
        }
        if !matches!(self.app.role.as_str(), "ask" | "teacher" | "student") {
            return Err(ConfigError::Validation(
                "app.role must be ask, teacher, or student".to_owned(),
            ));
        }
        if !matches!(
            self.network.teacher_ip_mode.as_str(),
            "infer_room_then_fallback" | "explicit"
        ) {
            return Err(ConfigError::Validation(
                "network.teacher_ip_mode must be infer_room_then_fallback or explicit".to_owned(),
            ));
        }
        if self.network.interface.trim().is_empty() {
            return Err(ConfigError::Validation(
                "network.interface must not be empty".to_owned(),
            ));
        }
        let ports = [
            self.network.control_port,
            self.network.media_port,
            self.network.feedback_port,
        ];
        if self.network.control_port > 65533
            || [self.network.media_port, self.network.feedback_port]
                .contains(&self.network.control_port.saturating_add(2))
        {
            return Err(ConfigError::Validation(
                "control port must leave room for tools (+2), without a UDP port collision".into(),
            ));
        }
        if ports.contains(&0)
            || ports[0] == ports[1]
            || ports[0] == ports[2]
            || ports[1] == ports[2]
        {
            return Err(ConfigError::Validation(
                "network ports must be non-zero and distinct".to_owned(),
            ));
        }
        if self.network.retransmission_cache_ms == 0 || self.network.retransmission_cache_ms > 1_000
        {
            return Err(ConfigError::Validation(
                "network.retransmission_cache_ms must be 1..=1000".to_owned(),
            ));
        }
        if self.network.send_pacing_mbps > 10_000 {
            return Err(ConfigError::Validation(
                "network.send_pacing_mbps must be 0..=10000 (0 disables pacing)".to_owned(),
            ));
        }
        if !(10..=500).contains(&self.network.send_au_deadline_ms)
            || !(10..=500).contains(&self.network.send_keyframe_deadline_ms)
            || self.network.send_keyframe_deadline_ms < self.network.send_au_deadline_ms
        {
            return Err(ConfigError::Validation(
                "network send deadlines must be 10..=500 ms; keyframe >= normal AU".to_owned(),
            ));
        }
        if !(1..=16).contains(&self.network.fanout_threads) {
            return Err(ConfigError::Validation(
                "network.fanout_threads must be 1..=16".to_owned(),
            ));
        }
        if !(1..=100).contains(&self.video.nack_repair_ms) {
            return Err(ConfigError::Validation(
                "video.nack_repair_ms must be 1..=100".to_owned(),
            ));
        }
        if self.video.width == 0
            || self.video.height == 0
            || self.video.max_fps == 0
            || self.video.max_fps > 30
        {
            return Err(ConfigError::Validation(
                "video dimensions must be non-zero and max_fps must be 1..=30".to_owned(),
            ));
        }
        if !(self.video.min_bitrate_kbps <= self.video.target_bitrate_kbps
            && self.video.target_bitrate_kbps <= self.video.max_bitrate_kbps)
        {
            return Err(ConfigError::Validation(
                "video bitrate must satisfy min <= target <= max".to_owned(),
            ));
        }
        if self.video.adaptive_idle_fps == 0
            || self.video.adaptive_idle_fps > self.video.max_fps
            || !(100..=30_000).contains(&self.video.adaptive_idle_after_ms)
            || self.video.adaptive_typing_fps == 0
            || self.video.adaptive_typing_fps > 30
            || !self.video.adaptive_small_change_percent.is_finite()
            || !self.video.adaptive_large_change_percent.is_finite()
            || self.video.adaptive_small_change_percent <= 0.0
            || self.video.adaptive_large_change_percent <= self.video.adaptive_small_change_percent
            || self.video.adaptive_large_change_percent > 100.0
            || !(100..=30_000).contains(&self.video.adaptive_small_hold_ms)
            || self.video.adaptive_pressure_encode_ms == 0
            || self.video.adaptive_pressure_encode_ms > 1_000
            || !self.video.adaptive_pressure_cpu_percent.is_finite()
            || !(1.0..=100.0).contains(&self.video.adaptive_pressure_cpu_percent)
            || !self.video.adaptive_receiver_loss_percent.is_finite()
            || !(0.1..=50.0).contains(&self.video.adaptive_receiver_loss_percent)
            || !(1..=100).contains(&self.video.adaptive_receiver_min_fps_percent)
            || !(1..=1_000).contains(&self.video.adaptive_receiver_render_latency_max_ms)
            || !(1..=100).contains(&self.video.adaptive_feedback_min_reporting_percent)
            || !(1..=100).contains(&self.video.adaptive_high_loss_quorum_percent)
            || !(1..=100).contains(&self.video.adaptive_recovery_min_reporting_percent)
            || self.video.adaptive_recovery_min_reporting_percent
                < self.video.adaptive_feedback_min_reporting_percent
            || !(1..=50).contains(&self.video.adaptive_bitrate_step_down_percent)
            || !(1..=50).contains(&self.video.adaptive_bitrate_step_up_percent)
            || !(1_000..=120_000).contains(&self.video.adaptive_bitrate_cooldown_ms)
        {
            return Err(ConfigError::Validation(
                "invalid adaptive video thresholds".to_owned(),
            ));
        }
        if self.phase1.audio || self.phase1.multicast || self.phase1.security_hardening {
            return Err(ConfigError::Validation(
                "Phase 1 audio, multicast, and security hardening must remain disabled".to_owned(),
            ));
        }
        Ok(())
    }
}

fn adjacent_config_path() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(|directory| directory.join("config.toml"))
}

fn merge_toml(base: &mut toml::Value, overlay: toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base_table), toml::Value::Table(overlay_table)) => {
            for (key, value) in overlay_table {
                match base_table.get_mut(&key) {
                    Some(base_value) => merge_toml(base_value, value),
                    None => {
                        base_table.insert(key, value);
                    }
                }
            }
        }
        (base_value, overlay_value) => *base_value = overlay_value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_geometry_defaults_for_old_profiles_and_persists_both_modes() {
        let legacy = EMBEDDED_DEFAULTS.replace("presentation_preserve_aspect = false", "");
        let mut config: Config = toml::from_str(&legacy).unwrap();
        assert!(!config.video.presentation_preserve_aspect);
        for preserve in [false, true] {
            config.video.presentation_preserve_aspect = preserve;
            let stored = toml::to_string(&config).unwrap();
            let loaded: Config = toml::from_str(&stored).unwrap();
            assert_eq!(loaded.video.presentation_preserve_aspect, preserve);
            loaded.validate().unwrap();
        }
    }

    #[test]
    fn new_tunables_default_to_todays_behaviour_and_are_bounded() {
        let mut c: Config = toml::from_str(EMBEDDED_DEFAULTS).unwrap();
        c.validate().unwrap();
        assert_eq!(c.recording, RecordingConfig::default());
        assert_eq!((c.recording.height, c.recording.fps), (720, 30));
        assert_eq!(c.recording.chunk_minutes, 10);
        assert_eq!(c.class.power_grace_seconds, 15);
        assert_eq!((c.activity.scan_seconds, c.activity.live_apps), (6, 3));
        assert!(c.activity.evidence);
        assert_eq!(
            (c.activity.evidence_per_site, c.activity.evidence_first_seconds, c.activity.evidence_gap_seconds),
            (3, 60, 180)
        );
        assert_eq!(c.transfer.rate_kib_s, 1000);
        // A profile saved before these sections existed still loads unchanged.
        let mut old = c.clone();
        old.apply_saved("[tools]\nmax_file_mb = 50\n").unwrap();
        assert_eq!(old.recording, RecordingConfig::default());

        type Mutation = fn(&mut Config);
        let bad: [Mutation; 15] = [
            |c| c.recording.height = 700,
            |c| c.recording.fps = 31,
            |c| c.recording.bitrate_kbps = 100,
            |c| c.recording.chunk_minutes = 0,
            // 30 min at 12 Mbps would be ~2.7 GB: over the part size limit.
            |c| {
                c.recording.chunk_minutes = 30;
                c.recording.bitrate_kbps = 12000;
            },
            |c| c.recording.teacher_stop_mb = 10,
            |c| c.recording.student_warn_mb = 100,
            |c| c.recording.gone_minutes = 3,
            |c| c.class.power_grace_seconds = 301,
            |c| c.activity.scan_seconds = 1,
            |c| c.activity.live_apps = 6,
            |c| c.activity.evidence_per_site = 0,
            |c| c.activity.evidence_first_seconds = 5,
            |c| c.activity.evidence_gap_seconds = 10,
            |c| c.transfer.rate_kib_s = 1,
        ];
        for (i, mutate) in bad.iter().enumerate() {
            let mut c2 = c.clone();
            mutate(&mut c2);
            assert!(c2.validate().is_err(), "case {i} should be refused");
        }
        c.recording.gone_minutes = 0; // "never" is allowed
        c.validate().unwrap();
    }

    #[test]
    fn recording_length_defaults_to_an_hour_and_is_bounded() {
        let mut config: Config = toml::from_str(EMBEDDED_DEFAULTS).unwrap();
        assert_eq!(config.tools.recording_max_minutes, 60);
        for valid in [5, 60, 480] {
            config.tools.recording_max_minutes = valid;
            config.validate().unwrap();
        }
        for invalid in [0, 4, 481] {
            config.tools.recording_max_minutes = invalid;
            assert!(config.validate().is_err(), "{invalid}");
        }
        // A saved profile from before this setting keeps working.
        let mut old: Config = toml::from_str(EMBEDDED_DEFAULTS).unwrap();
        old.apply_saved("[tools]\nmax_file_mb = 100\n").unwrap();
        assert_eq!(old.tools.recording_max_minutes, 60);
    }

    #[test]
    fn file_transfer_limit_defaults_and_bounds() {
        let mut config: Config = toml::from_str(EMBEDDED_DEFAULTS).unwrap();
        assert_eq!(config.tools.max_file_mb, 300);
        for valid in [1, 300, 4096] {
            config.tools.max_file_mb = valid;
            config.validate().unwrap();
        }
        for invalid in [0, 4097] {
            config.tools.max_file_mb = invalid;
            assert!(config.validate().is_err());
        }
    }

    #[test]
    fn embedded_defaults_are_valid() {
        let config: Config = toml::from_str(EMBEDDED_DEFAULTS).unwrap();
        config.validate().unwrap();
        assert_eq!(config.network.control_port, 47_800);
        assert_eq!(config.network.teacher_ip, Ipv4Addr::new(192, 168, 100, 115));
    }

    #[test]
    fn nested_overlay_preserves_other_defaults() {
        let mut base = toml::from_str::<toml::Value>(EMBEDDED_DEFAULTS).unwrap();
        merge_toml(
            &mut base,
            toml::from_str("[network]\ncontrol_port = 49000").unwrap(),
        );
        let config: Config = base.try_into().unwrap();
        assert_eq!(config.network.control_port, 49_000);
        assert_eq!(config.network.media_port, 47_810);
    }

    #[test]
    fn rejects_phase_one_scope_expansion() {
        let mut config: Config = toml::from_str(EMBEDDED_DEFAULTS).unwrap();
        config.phase1.audio = true;
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_invalid_receiver_feedback_quorums() {
        let mut config: Config = toml::from_str(EMBEDDED_DEFAULTS).unwrap();
        config.video.adaptive_feedback_min_reporting_percent = 0;
        assert!(config.validate().is_err());
        config.video.adaptive_feedback_min_reporting_percent = 50;
        config.video.adaptive_high_loss_quorum_percent = 101;
        assert!(config.validate().is_err());
        config.video.adaptive_high_loss_quorum_percent = 25;
        config.video.adaptive_recovery_min_reporting_percent = 49;
        assert!(config.validate().is_err());
        config.video.adaptive_recovery_min_reporting_percent = 75;
        config.video.adaptive_receiver_render_latency_max_ms = 0;
        assert!(config.validate().is_err());
    }
    #[test]
    fn validates_independent_bounded_repair_budget() {
        let mut config: Config = toml::from_str(EMBEDDED_DEFAULTS).unwrap();
        assert_eq!(config.video.nack_repair_ms, 25);
        for valid in [1, 25, 100] {
            config.video.nack_repair_ms = valid;
            config.validate().unwrap();
        }
        for invalid in [0, 101] {
            config.video.nack_repair_ms = invalid;
            assert!(config.validate().is_err());
        }
    }

    fn base() -> Config {
        toml::from_str(EMBEDDED_DEFAULTS).unwrap()
    }
    fn with_mode(mode: &str) -> Config {
        let mut c = base();
        c.performance.mode = mode.into();
        c
    }

    #[test]
    fn normal_mode_and_defaults_change_nothing() {
        let c = base();
        assert_eq!(c.performance, PerformanceConfig::default());
        assert_eq!(c.monitoring.quality_floor, 85);
        assert!(c.monitoring.hidpi);
        assert_eq!(
            toml::to_string(&c.with_profile(false)).unwrap(),
            toml::to_string(&c).unwrap()
        );
        // Wi-Fi alone does nothing unless the owner turned auto_wifi on.
        assert_eq!(
            toml::to_string(&c.with_profile(true)).unwrap(),
            toml::to_string(&c).unwrap()
        );
        assert_eq!(c.active_mode(true), ("normal", ""));
    }

    #[test]
    fn a_mode_only_lowers_values_and_stays_valid() {
        for chosen in [(1920, 1080, 30, 6000), (1600, 900, 24, 4000), (1280, 720, 15, 1500), (640, 360, 10, 800)] {
            let mut original = base();
            original.video.width = chosen.0;
            original.video.height = chosen.1;
            original.video.max_fps = chosen.2;
            original.video.target_bitrate_kbps = chosen.3;
            original.video.min_bitrate_kbps = original.video.min_bitrate_kbps.min(chosen.3);
            original.video.max_bitrate_kbps = original.video.max_bitrate_kbps.max(chosen.3);
            original.video.adaptive_idle_fps = original.video.adaptive_idle_fps.min(chosen.2);
            original.validate().unwrap();
            for mode in ["economy", "minimal"] {
                original.performance.mode = mode.into();
                let e = original.with_profile(false);
                e.validate().unwrap_or_else(|error| panic!("{mode} {chosen:?}: {error}"));
                assert!(e.video.width <= original.video.width && e.video.height <= original.video.height);
                assert!(e.video.max_fps <= original.video.max_fps);
                assert!(e.video.target_bitrate_kbps <= original.video.target_bitrate_kbps);
                assert!(e.video.max_bitrate_kbps <= original.video.max_bitrate_kbps.max(e.video.target_bitrate_kbps));
                assert!(e.video.adaptive_idle_fps <= e.video.max_fps);
                assert!(e.observe.max_fps <= original.observe.max_fps && !e.observe.enhanced);
                assert!(e.monitoring.interval_ms >= original.monitoring.interval_ms);
                assert!(e.monitoring.jpeg_quality <= original.monitoring.jpeg_quality);
                assert!(e.monitoring.width <= original.monitoring.width);
            }
        }
    }

    #[test]
    fn economy_and_minimal_have_the_documented_numbers_and_keep_the_aspect() {
        let economy = with_mode("economy").with_profile(false);
        assert_eq!((economy.video.width, economy.video.height), (1280, 720));
        assert_eq!(economy.video.max_fps, 20);
        assert_eq!(economy.video.target_bitrate_kbps, 3000);
        assert_eq!(economy.monitoring.interval_ms, 4000);
        assert_eq!(economy.monitoring.jpeg_quality, 60);
        assert_eq!(economy.monitoring.quality_floor, 60);
        assert!(!economy.monitoring.hidpi);
        // The wire budget is not shrunk by a mode: a class of 40 at 3 Mbps needs 120 Mbps.
        assert_eq!(economy.network.send_pacing_mbps, base().network.send_pacing_mbps);
        let minimal = with_mode("minimal").with_profile(false);
        assert_eq!((minimal.video.width, minimal.video.height), (960, 540));
        assert_eq!(minimal.video.max_fps, 12);
        assert!(minimal.video.target_bitrate_kbps < economy.video.target_bitrate_kbps);
        assert!(minimal.monitoring.interval_ms > economy.monitoring.interval_ms);
        // A 4:3 desktop is fitted inside the limit, not stretched.
        let mut odd = with_mode("economy");
        odd.video.width = 1600;
        odd.video.height = 1200;
        let fitted = odd.with_profile(false);
        assert_eq!((fitted.video.width, fitted.video.height), (960, 720));
        assert!(fitted.video.width % 2 == 0 && fitted.video.height % 2 == 0);
    }

    #[test]
    fn auto_wifi_switches_to_economy_only_for_a_normal_wireless_teacher() {
        let mut c = base();
        c.performance.auto_wifi = true;
        assert_eq!(c.active_mode(true), ("economy", "wifi"));
        assert_eq!(c.active_mode(false), ("normal", ""));
        assert_eq!(c.with_profile(true).video.max_fps, 20);
        assert_eq!(c.with_profile(false).video.max_fps, 30);
        // A manual choice is not overridden by the adapter.
        c.performance.mode = "minimal".into();
        assert_eq!(c.active_mode(false), ("minimal", "manual"));
        assert_eq!(c.active_mode(true), ("minimal", "manual"));
    }

    #[test]
    fn bandwidth_cap_is_shared_out_per_student_with_a_floor() {
        let mut c = base();
        assert_eq!(c.for_stream(40).video.target_bitrate_kbps, 6000); // no cap: unchanged
        c.performance.bandwidth_cap_mbps = 30;
        // 30 Mbps x 0.75 / 40 = 562 kbps each: far too little for 1080p, so the
        // picture shrinks to 640x360 at 10 fps with it.
        let each = c.for_stream(40);
        assert_eq!(each.video.target_bitrate_kbps, 562);
        assert_eq!((each.video.width, each.video.height, each.video.max_fps), (640, 360, 10));
        assert!(each.video.max_bitrate_kbps >= each.video.target_bitrate_kbps);
        assert!(each.video.min_bitrate_kbps <= each.video.target_bitrate_kbps);
        // Budget: 2.4 x the cap, so the video (75 % of the cap) leaves room for keyframes.
        assert_eq!(each.network.send_pacing_mbps, 72);
        each.validate().unwrap();
        // Few Students: the chosen bitrate and picture stay (the cap is not a target).
        let few = c.for_stream(3);
        assert_eq!(few.video.target_bitrate_kbps, 6000);
        assert_eq!((few.video.width, few.video.max_fps), (1920, 30));
        // The picture steps down with the share each Student gets.
        for (mbps, students, size) in [(64, 8, (1920, 1080)), (32, 8, (1280, 720)), (16, 8, (960, 540)), (6, 8, (640, 360))] {
            c.performance.bandwidth_cap_mbps = mbps;
            let step = c.for_stream(students);
            assert_eq!((step.video.width, step.video.height), size, "{mbps} Mbps / {students}");
            step.validate().unwrap();
        }
        c.performance.bandwidth_cap_mbps = 30;
        // Many Students: never below the floor.
        c.performance.bandwidth_cap_mbps = 2;
        let crowded = c.for_stream(400);
        assert_eq!(crowded.video.target_bitrate_kbps, 300);
        // The floor wins over an unreachable cap: 400 x 300 kbps = 120 Mbps, so the
        // budget follows that (288), not the 5 Mbps the cap would allow.
        assert_eq!(crowded.network.send_pacing_mbps, 288);
        crowded.validate().unwrap();
        // Zero recipients behaves as one.
        assert!(c.for_stream(0).video.target_bitrate_kbps >= 300);
    }

    #[test]
    fn performance_settings_are_validated_and_survive_the_saved_overlay() {
        let mut c = base();
        c.apply_saved("[performance]\nmode = \"economy\"\nauto_wifi = true\nbandwidth_cap_mbps = 50\n").unwrap();
        assert_eq!(c.performance.mode, "economy");
        assert!(c.performance.auto_wifi);
        assert_eq!(c.performance.bandwidth_cap_mbps, 50);
        // The video values the Teacher chose are untouched by the mode.
        assert_eq!(c.video.width, 1920);
        for bad in ["[performance]\nmode = \"turbo\"\n", "[performance]\nbandwidth_cap_mbps = 1\n"] {
            let mut other = base();
            assert!(other.apply_saved(bad).is_err(), "{bad}");
        }
    }
}
