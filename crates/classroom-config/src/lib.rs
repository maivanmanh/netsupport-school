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
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct ToolsConfig {
    pub max_file_mb: u32,
}
impl Default for ToolsConfig {
    fn default() -> Self {
        Self { max_file_mb: 300 }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct DiagnosticsConfig {
    /// Collect bounded, sanitized runtime logs after a presentation stops.
    pub auto_collect: bool,
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
}
impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_ms: 2000,
            width: 480,
            height: 270,
            jpeg_quality: 65,
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
    pub control_port: u16,
    pub media_port: u16,
    pub feedback_port: u16,
    pub retransmission_cache_ms: u32,
    pub send_pacing_mbps: u32,
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
const fn default_fanout_threads() -> u32 {
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

    #[allow(clippy::too_many_lines)]
    fn validate(&self) -> Result<(), ConfigError> {
        if !(1..=4096).contains(&self.tools.max_file_mb) {
            return Err(ConfigError::Validation(
                "tools.max_file_mb must be 1..4096".into(),
            ));
        }
        let m = &self.monitoring;
        if !(1000..=10000).contains(&m.interval_ms)
            || !(2..=480).contains(&m.width)
            || !(2..=270).contains(&m.height)
            || !(30..=85).contains(&m.jpeg_quality)
            || (m.enabled
                && (self.network.control_port == u16::MAX
                    || [self.network.media_port, self.network.feedback_port]
                        .contains(&(self.network.control_port + 1))))
        {
            return Err(ConfigError::Validation(
                "invalid bounded monitoring settings or port collision".into(),
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
}
