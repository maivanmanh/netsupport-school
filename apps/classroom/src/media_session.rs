//! Role-independent publisher orchestration; all topologies use the same media hot path.
use super::{
    bind_teacher_media_socket, run_teacher_media_session, AtomicBool, CaptureConfig, CaptureSource,
    Config, Context, Ipv4Addr, Ordering, PlatformCapture, Result, RTP_VIDEO_CLOCK_HZ,
};
use control_transport::room::RoomPresentation;
use std::sync::Arc;
use tokio::sync::watch;
#[allow(clippy::too_many_arguments)]
pub(super) fn publish(
    config: &Config,
    bind_ip: Ipv4Addr,
    display_index: u32,
    stream_id: u64,
    cancel: &AtomicBool,
    state: &watch::Sender<Option<RoomPresentation>>,
    recipients: Option<&Arc<std::collections::HashSet<String>>>,
) -> Result<()> {
    publish_with_cursor_policy(
        config,
        bind_ip,
        display_index,
        stream_id,
        cancel,
        state,
        recipients,
        None,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn publish_with_cursor_policy(
    config: &Config,
    bind_ip: Ipv4Addr,
    display_index: u32,
    stream_id: u64,
    cancel: &AtomicBool,
    state: &watch::Sender<Option<RoomPresentation>>,
    recipients: Option<&Arc<std::collections::HashSet<String>>>,
    cursor: Option<Arc<AtomicBool>>,
    pointer_source: Option<std::sync::mpsc::SyncSender<Box<dyn media_core::PointerSource>>>,
) -> Result<()> {
    let mut capture = PlatformCapture::new();
    if let Some(cursor) = cursor {
        capture.set_cursor_policy(cursor);
    }
    capture.start(CaptureConfig {
        display_index,
        width: config.video.width,
        height: config.video.height,
        max_fps: config.video.max_fps,
    })?;
    if let (Some(sender), Some(source)) = (pointer_source, capture.pointer_source()) {
        let _ = sender.try_send(source);
    }
    let dimensions = capture
        .output_dimensions()
        .context("capture dimensions missing")?;
    let socket = bind_teacher_media_socket(bind_ip, config.network.feedback_port)?;
    let (events, receiver) = std::sync::mpsc::sync_channel(256);
    if cancel.load(Ordering::Acquire) {
        return Ok(());
    }
    let media_port = socket.local_addr()?.port();
    state.send_if_modified(|current| {
        if cancel.load(Ordering::Acquire) {
            return false;
        }
        *current = Some(RoomPresentation {
            config: protocol::StartPresentation {
                stream_id,
                width: dimensions.0,
                height: dimensions.1,
                max_fps: config.video.max_fps,
                bitrate_bps: config.video.target_bitrate_kbps.saturating_mul(1000),
                rtp_payload_type: u32::from(
                    super::stream_profile::session_codec(&config.video).rtp_payload_type(),
                ),
                rtp_clock_hz: RTP_VIDEO_CLOCK_HZ,
                teacher_media_udp_port: u32::from(media_port),
                preserve_aspect: config.video.presentation_preserve_aspect,
            },
            events,
            recipients: recipients.cloned(),
            excluded_source: None,
        });
        true
    });
    let result = run_teacher_media_session(
        config, capture, &socket, dimensions, None, receiver, 0, stream_id, cancel,
    );
    if let Err(error) = &result {
        tracing::error!(stream_id, %error, "teacher media publisher failed");
    }
    state.send_replace(None);
    result.map(|_| ())
}
