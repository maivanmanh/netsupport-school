//! Persistent classroom control, independent of each bounded media session.
use super::{
    check_version, media_endpoint_ip, read_envelope, write_envelope, ControlError,
    RegisteredStudent, TeacherMediaEvent,
};
use protocol::{envelope::Payload, Envelope, StartPresentation};
use std::{
    collections::HashSet,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::{
        mpsc::{SyncSender, TrySendError},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::watch,
    task::{JoinHandle, JoinSet},
};
use tracing::{info, warn};

#[derive(Clone, Debug)]
pub struct RoomPresentation {
    pub config: StartPresentation,
    pub events: SyncSender<TeacherMediaEvent>,
    /// None means the whole room; Some is a fixed set of stable Student IDs.
    pub recipients: Option<Arc<HashSet<String>>>,
    /// A Student presentation must not fullscreen its own source desktop.
    pub excluded_source: Option<String>,
}
impl RoomPresentation {
    pub fn includes(&self, instance_id: &str) -> bool {
        self.excluded_source.as_deref() != Some(instance_id)
            && self
                .recipients
                .as_ref()
                .is_none_or(|ids| ids.contains(instance_id))
    }
}
#[derive(Clone, Debug)]
pub struct RoomStudent {
    pub client_id: u64,
    pub instance_id: String,
    pub hostname: String,
    pub endpoint: SocketAddrV4,
    pub online: bool,
    pub thumbnail: Option<Arc<RoomThumbnail>>,
    pub monitoring_state: String,
    pub media: Option<MediaHealth>,
}

/// One latest report per bounded roster entry. Counters are session scoped.
#[derive(Clone, Debug)]
pub struct MediaHealth {
    pub report: protocol::ReceiverReport,
    pub received_at: std::time::Instant,
    pub decoded_at: Option<std::time::Instant>,
    pub rendered_at: Option<std::time::Instant>,
    pub decode_fps: f64,
    pub render_fps: f64,
}
impl MediaHealth {
    fn update(previous: Option<&Self>, report: protocol::ReceiverReport) -> Self {
        let now = std::time::Instant::now();
        let previous = previous.filter(|p| p.report.stream_id == report.stream_id);
        let seconds = previous.map_or(1.0, |p| {
            now.duration_since(p.received_at).as_secs_f64().max(0.001)
        });
        let decoded = previous.map_or(0, |p| p.report.decoded_frames);
        let rendered = previous.map_or(0, |p| p.report.rendered_frames);
        Self {
            decoded_at: if report.decoded_frames > decoded {
                Some(now)
            } else {
                previous.and_then(|p| p.decoded_at)
            },
            rendered_at: if report.rendered_frames > rendered {
                Some(now)
            } else {
                previous.and_then(|p| p.rendered_at)
            },
            // The first cumulative report has no measured sampling interval.
            decode_fps: previous.map_or(0.0, |_| {
                report.decoded_frames.saturating_sub(decoded) as f64 / seconds
            }),
            render_fps: previous.map_or(0.0, |_| {
                report.rendered_frames.saturating_sub(rendered) as f64 / seconds
            }),
            received_at: now,
            report,
        }
    }
    #[must_use]
    pub fn state(&self) -> &'static str {
        if self.received_at.elapsed() > Duration::from_secs(5) {
            return "report_stale";
        }
        if self.report.health_version == 0 {
            return "unknown";
        }
        if self.report.recovering {
            return "recovering";
        }
        if self.decoded_at.is_none() {
            return "waiting_for_image";
        }
        if self
            .decoded_at
            .is_some_and(|at| at.elapsed() > Duration::from_secs(3))
        {
            return "decode_stalled";
        }
        if !self.report.rendering_enabled {
            return "receiving_headless";
        }
        if self
            .rendered_at
            .is_none_or(|at| at.elapsed() > Duration::from_secs(3))
        {
            return "render_stalled";
        }
        "receiving"
    }
}

#[cfg(test)]
mod media_health_tests {
    use super::*;
    fn report(stream: u64, frames: u64) -> protocol::ReceiverReport {
        protocol::ReceiverReport {
            stream_id: stream,
            decoded_frames: frames,
            rendered_frames: frames,
            rendering_enabled: true,
            health_version: 1,
            ..Default::default()
        }
    }
    #[test]
    fn decoded_and_rendered_progress_are_required() {
        let mut health = MediaHealth::update(None, report(1, 0));
        assert_eq!(health.state(), "waiting_for_image");
        health = MediaHealth::update(Some(&health), report(1, 3));
        assert_eq!(health.state(), "receiving");
        health.decoded_at = Some(std::time::Instant::now() - Duration::from_secs(4));
        assert_eq!(health.state(), "decode_stalled");
        health.decoded_at = Some(std::time::Instant::now());
        health.rendered_at = None;
        assert_eq!(health.state(), "render_stalled");
        health.report.rendering_enabled = false;
        assert_eq!(health.state(), "receiving_headless");
    }
    #[test]
    fn old_sessions_and_old_reports_never_look_healthy() {
        let old = MediaHealth::update(None, report(1, 50));
        assert_eq!(old.decode_fps, 0.0);
        assert_eq!(old.render_fps, 0.0);
        let mut next = MediaHealth::update(Some(&old), report(2, 0));
        assert!(next.decoded_at.is_none());
        next.report.recovering = true;
        assert_eq!(next.state(), "recovering");
        next.received_at = std::time::Instant::now() - Duration::from_secs(6);
        assert_eq!(next.state(), "report_stale");
        next.received_at = std::time::Instant::now();
        next.report.health_version = 0;
        assert_eq!(next.state(), "unknown");
    }
    #[test]
    fn student_source_is_excluded_but_unknown_late_join_is_included() {
        let (events, _) = std::sync::mpsc::sync_channel(1);
        let session = RoomPresentation {
            config: Default::default(),
            events,
            recipients: None,
            excluded_source: Some("source".into()),
        };
        assert!(!session.includes("source"));
        assert!(session.includes("late-join"));
    }
}
#[derive(Debug)]
pub struct RoomThumbnail {
    /// Original validated, bounded JPEG reused by the native management UI.
    pub jpeg: Vec<u8>,
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u32>,
    pub received_at: std::time::Instant,
    pub sequence: u64,
}

#[derive(Default, Debug)]
pub struct RoomRoster {
    students: Vec<RoomStudent>,
    inputs: std::collections::VecDeque<protocol::ObserveMessage>,
    input_status: Option<protocol::ObserveMessage>,
    cursor_status: Option<protocol::ObserveMessage>,
    cursor_changed: Arc<tokio::sync::Notify>,
}
impl RoomRoster {
    pub fn update_media(&mut self, client_id: u64, report: protocol::ReceiverReport) {
        if let Some(row) = self
            .students
            .iter_mut()
            .find(|r| r.online && r.client_id == client_id)
        {
            row.media = Some(MediaHealth::update(row.media.as_ref(), report));
        }
    }
    #[must_use]
    pub fn snapshot(&self) -> Vec<RoomStudent> {
        self.students.clone()
    }
    #[must_use]
    pub fn monitoring_client(&self, instance: &str, peer: Ipv4Addr) -> Option<u64> {
        self.students
            .iter()
            .find(|row| row.online && row.instance_id == instance && *row.endpoint.ip() == peer)
            .map(|row| row.client_id)
    }
    pub fn update_thumbnail(
        &mut self,
        client_id: u64,
        state: &str,
        thumbnail: Option<RoomThumbnail>,
    ) -> bool {
        let Some(row) = self
            .students
            .iter_mut()
            .find(|row| row.client_id == client_id && row.online)
        else {
            return false;
        };
        row.monitoring_state = state.chars().take(32).collect();
        if let Some(thumbnail) = thumbnail {
            row.thumbnail = Some(Arc::new(thumbnail));
        }
        true
    }
    pub fn cursor_status(&mut self, status: protocol::ObserveMessage) {
        self.cursor_status = Some(status);
        self.cursor_changed.notify_one();
    }
    pub fn input_status(&mut self, status: protocol::ObserveMessage) {
        self.input_status = Some(status);
    }
    pub fn take_input(&mut self) -> Option<protocol::ObserveMessage> {
        self.inputs.pop_front()
    }
    fn register(&mut self, student: RoomStudent) -> Result<(), ControlError> {
        if let Some(row) = self
            .students
            .iter_mut()
            .find(|row| row.instance_id == student.instance_id)
        {
            let thumbnail = row.thumbnail.clone();
            *row = student;
            row.thumbnail = thumbnail;
        } else {
            if self.students.len() == 256 {
                let Some(index) = self.students.iter().position(|row| !row.online) else {
                    return Err(ControlError::ControlClosed);
                };
                self.students.remove(index);
            }
            self.students.push(student);
        }
        Ok(())
    }
    fn disconnect(&mut self, client_id: u64) {
        if let Some(row) = self
            .students
            .iter_mut()
            .find(|row| row.client_id == client_id)
        {
            row.online = false;
        }
    }
}
struct PeerGuard {
    roster: Arc<Mutex<RoomRoster>>,
    client_id: u64,
}
impl Drop for PeerGuard {
    fn drop(&mut self) {
        self.roster.lock().unwrap().disconnect(self.client_id);
        info!(client_id = self.client_id, "teacher room Student offline");
    }
}
struct MediaGuard {
    active: Option<RoomPresentation>,
    client_id: u64,
    endpoint: SocketAddrV4,
}
impl Drop for MediaGuard {
    fn drop(&mut self) {
        if let Some(session) = &self.active {
            let _ = session.events.try_send(TeacherMediaEvent::Disconnected {
                client_id: self.client_id,
                endpoint: self.endpoint,
            });
        }
    }
}
struct ReaderGuard(JoinHandle<()>);
impl Drop for ReaderGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Run a persistent listener; aborting the future cancels its owned peers.
///
/// # Errors
/// Returns bind/accept errors. A failed peer does not stop the listener.
pub async fn run(
    bind_ip: Ipv4Addr,
    port: u16,
    presentation: watch::Receiver<Option<RoomPresentation>>,
    roster: Arc<Mutex<RoomRoster>>,
) -> Result<(), ControlError> {
    let listener = TcpListener::bind(SocketAddrV4::new(bind_ip, port)).await?;
    run_listener(listener, presentation, roster).await
}

/// Serve an already-bound publisher listener.
/// # Errors
/// Returns accept/socket errors; individual peer failures are isolated.
pub async fn run_listener(
    listener: TcpListener,
    presentation: watch::Receiver<Option<RoomPresentation>>,
    roster: Arc<Mutex<RoomRoster>>,
) -> Result<(), ControlError> {
    run_listener_inner(listener, presentation, roster, None).await
}

/// A reverse publisher accepts only its current Teacher and one session token.
/// # Errors
/// Returns listener/socket errors.
pub async fn run_authorized_listener(
    listener: TcpListener,
    presentation: watch::Receiver<Option<RoomPresentation>>,
    roster: Arc<Mutex<RoomRoster>>,
    peer: Ipv4Addr,
    token: String,
) -> Result<(), ControlError> {
    run_listener_inner(listener, presentation, roster, Some((peer, token))).await
}

async fn run_listener_inner(
    listener: TcpListener,
    presentation: watch::Receiver<Option<RoomPresentation>>,
    roster: Arc<Mutex<RoomRoster>>,
    authorized: Option<(Ipv4Addr, String)>,
) -> Result<(), ControlError> {
    info!(address=%listener.local_addr()?, "teacher room control ready");
    let permits = Arc::new(tokio::sync::Semaphore::new(128));
    let mut peers = JoinSet::new();
    let mut next_id = 1_u64;
    loop {
        tokio::select! {
            result=listener.accept() => {
                let (stream,peer)=result?;
                let Ok(permit)=Arc::clone(&permits).try_acquire_owned() else { continue };
                stream.set_nodelay(true)?;
                let id=next_id; next_id=next_id.wrapping_add(1);
                if authorized.as_ref().is_some_and(|(ip,_)| peer.ip()!=std::net::IpAddr::V4(*ip)) {continue;}
                let auth=authorized.clone();
                let state=presentation.clone(); let roster=Arc::clone(&roster);
                peers.spawn(async move {
                    let _permit=permit;
                    if let Err(error)=handle_authorized(stream,peer,id,state,roster,auth).await {
                        info!(%peer,client_id=id,%error,"teacher room peer ended");
                    }
                });
            }
            result=peers.join_next(), if !peers.is_empty() => {
                if let Some(Err(error))=result { warn!(%error,"teacher room peer task failed"); }
            }
        }
    }
}

async fn send(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    envelope: &Envelope,
) -> Result<(), ControlError> {
    tokio::time::timeout(Duration::from_secs(2), write_envelope(writer, envelope))
        .await
        .map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::TimedOut, "room control write deadline")
        })?
}
fn dispatch(state: &RoomPresentation, event: TeacherMediaEvent) -> Result<(), ControlError> {
    match state.events.try_send(event) {
        Ok(()) => Ok(()),
        Err(TrySendError::Full(TeacherMediaEvent::ReceiverReport { .. })) => {
            warn!("teacher room dropped excess receiver report");
            Ok(())
        }
        Err(_) => Err(ControlError::ControlClosed),
    }
}
#[allow(clippy::too_many_lines)]
#[cfg(test)]
async fn handle(
    stream: TcpStream,
    peer: SocketAddr,
    id: u64,
    state: watch::Receiver<Option<RoomPresentation>>,
    roster: Arc<Mutex<RoomRoster>>,
) -> Result<(), ControlError> {
    handle_authorized(stream, peer, id, state, roster, None).await
}

#[allow(clippy::too_many_lines)]
async fn handle_authorized(
    stream: TcpStream,
    peer: SocketAddr,
    id: u64,
    mut state: watch::Receiver<Option<RoomPresentation>>,
    roster: Arc<Mutex<RoomRoster>>,
    authorized: Option<(Ipv4Addr, String)>,
) -> Result<(), ControlError> {
    let (mut reader, mut writer) = stream.into_split();
    let hello = tokio::time::timeout(Duration::from_secs(2), read_envelope(&mut reader))
        .await
        .map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::TimedOut, "room registration deadline")
        })??;
    check_version(&hello)?;
    let Some(Payload::HelloStudent(hello)) = hello.payload else {
        return Err(ControlError::UnexpectedMessage);
    };
    if authorized.as_ref().is_some_and(|(_, token)| {
        hello.instance_id != *token && hello.instance_id != format!("{token}:view")
    }) {
        return Err(ControlError::UnexpectedMessage);
    }
    let endpoint = SocketAddrV4::new(
        media_endpoint_ip(peer, hello.selected_local_ip.parse().ok()),
        u16::try_from(hello.media_udp_port).map_err(|_| ControlError::UnexpectedMessage)?,
    );
    if hello.instance_id.is_empty() || endpoint.port() == 0 {
        return Err(ControlError::UnexpectedMessage);
    }
    let student = RegisteredStudent {
        client_id: id,
        endpoint,
        hostname: hello.hostname.chars().take(64).collect(),
        pointer_overlay: hello.pointer_overlay,
    };
    roster.lock().unwrap().register(RoomStudent {
        client_id: id,
        instance_id: hello.instance_id.chars().take(128).collect(),
        hostname: student.hostname.clone(),
        endpoint,
        online: true,
        thumbnail: None,
        monitoring_state: "waiting".into(),
        media: None,
    })?;
    let _peer = PeerGuard {
        roster: Arc::clone(&roster),
        client_id: id,
    };
    info!(client_id=id,hostname=%student.hostname,%endpoint,"teacher room Student online");
    let mut media = MediaGuard {
        active: state
            .borrow_and_update()
            .clone()
            .filter(|session| session.includes(&hello.instance_id)),
        client_id: id,
        endpoint,
    };
    let mut welcome = Envelope::welcome(id);
    if let Some(Payload::WelcomeStudent(value)) = &mut welcome.payload {
        value.presentation_active = media.active.is_some();
    }
    send(&mut writer, &welcome).await?;
    if let Some(session) = &media.active {
        send(
            &mut writer,
            &Envelope::start_presentation(session.config.clone()),
        )
        .await?;
        dispatch(session, TeacherMediaEvent::Registered(student.clone()))?;
    }
    // Framed reads are never canceled by a state update: a dedicated reader
    // preserves partial TCP prefixes/bodies, with a bounded message channel.
    let (incoming, mut messages) = tokio::sync::mpsc::channel(128);
    let _reader = ReaderGuard(tokio::spawn(async move {
        while let Ok(value) = read_envelope(&mut reader).await {
            if incoming.send(value).await.is_err() {
                break;
            }
        }
    }));
    let cursor_changed = Arc::clone(&roster.lock().unwrap().cursor_changed);
    let mut heartbeat = tokio::time::interval(Duration::from_secs(1));
    let mut heard_at = std::time::Instant::now();
    loop {
        // A class publisher has two viewers from one Teacher: compressed relay
        // and Control preview. Only the latter may consume input acknowledgements
        // and cursor metadata; otherwise the relay can steal the mode response.
        let input_peer = authorized.as_ref().is_none_or(|(_, token)| {
            let view = format!("{token}:view");
            !media
                .active
                .as_ref()
                .is_some_and(|s| s.recipients.as_ref().is_some_and(|ids| ids.contains(&view)))
                || hello.instance_id == view
        });
        tokio::select! {
            _ = cursor_changed.notified(), if input_peer => {
                let cursor = roster.lock().unwrap().cursor_status.take();
                if let Some(cursor) = cursor {
                    send(&mut writer,&Envelope {protocol_version:protocol::PROTOCOL_VERSION,payload:Some(Payload::Observe(cursor))}).await?;
                }
            }
            _ = heartbeat.tick() => {
                let status=if input_peer {roster.lock().unwrap().input_status.take()} else {None};
                if let Some(status)=status {
                    send(&mut writer,&Envelope {protocol_version:protocol::PROTOCOL_VERSION,payload:Some(Payload::Observe(status))}).await?;
                }
                if heard_at.elapsed() >= Duration::from_secs(10) { return Err(ControlError::ControlClosed); }
            }
            changed=state.changed()=>{
                if changed.is_err() {return Ok(())}
                let next=state.borrow_and_update().clone().filter(|session|session.includes(&hello.instance_id));
                if let Some(previous)=media.active.take() {
                    let _=previous.events.try_send(TeacherMediaEvent::Disconnected {client_id:id,endpoint});
                    send(&mut writer,&Envelope::stop_presentation(previous.config.stream_id)).await?;
                }
                if let Some(session)=&next {
                    send(&mut writer,&Envelope::start_presentation(session.config.clone())).await?;
                    dispatch(session,TeacherMediaEvent::Registered(student.clone()))?;
                }
                media.active=next;
            }
            value=messages.recv()=>{
                let Some(value)=value else {return Ok(())}; heard_at = std::time::Instant::now(); check_version(&value)?;
                match value.payload {
                    Some(Payload::ControlPing(ping))=>{
                        let mut response=Envelope::control_pong(ping.nonce);
                        if let Some(Payload::ControlPong(value))=&mut response.payload {
                            value.sender_unix_us=ping.sender_unix_us;
                            value.teacher_received_unix_us=media_core::unix_us(); value.teacher_sent_unix_us=media_core::unix_us();
                        }
                        send(&mut writer,&response).await?;
                    }
                    Some(Payload::ReceiverReport(report))=>{
                        if let Some(session)=media.active.as_ref().filter(|session|session.config.stream_id==report.stream_id) {
                            roster.lock().unwrap().update_media(id, report.clone());
                            dispatch(session,TeacherMediaEvent::ReceiverReport {client_id:id,report})?;
                        }
                    }
                    Some(Payload::Nack(message))=>{
                        if let Some(session)=media.active.as_ref().filter(|session|session.config.stream_id==message.stream_id) {dispatch(session,TeacherMediaEvent::Nack {client_id:id,endpoint,message})?;}
                    }
                    Some(Payload::RequestKeyframe(message))=>{
                        if let Some(session)=media.active.as_ref().filter(|session|session.config.stream_id==message.stream_id) {dispatch(session,TeacherMediaEvent::KeyframeRequested {client_id:id,message})?;}
                    }
                    Some(Payload::Observe(input)) if input_peer && media.active.as_ref().is_some_and(|s| s.config.stream_id == input.stream_id
                            && s.recipients.as_ref().is_some_and(|ids| ids.contains(&hello.instance_id))) => {
                            let mut r = roster.lock().unwrap();
                            // Only adjacent moves are replaceable. Never cross
                            // a button/key barrier, preserving drag/click order.
                            if input.action == "move" && r.inputs.back().is_some_and(|v| v.action == "move") {
                                r.inputs.pop_back();
                            }
                            if r.inputs.len() >= 64 { return Err(ControlError::ControlClosed); }
                            r.inputs.push_back(input);
                    }
                    _=>return Err(ControlError::UnexpectedMessage),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn cursor_changes_are_latest_only_and_do_not_replace_mode_ack() {
        let mut roster = super::RoomRoster::default();
        roster.input_status(protocol::ObserveMessage {
            action: "status".into(),
            key: "control".into(),
            ..Default::default()
        });
        for serial in 0..100 {
            roster.cursor_status(protocol::ObserveMessage {
                action: "cursor".into(),
                key: serial.to_string(),
                ..Default::default()
            });
        }
        assert_eq!(roster.input_status.as_ref().unwrap().key, "control");
        assert_eq!(roster.cursor_status.as_ref().unwrap().key, "99");
    }

    use super::*;
    use crate::{register_student_for_presentation, StudentRegistration};

    async fn wait_until(mut predicate: impl FnMut() -> bool) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while !predicate() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
    fn presentation(
        id: u64,
    ) -> (
        RoomPresentation,
        std::sync::mpsc::Receiver<TeacherMediaEvent>,
    ) {
        let (events, receiver) = std::sync::mpsc::sync_channel(256);
        (
            RoomPresentation {
                config: StartPresentation {
                    stream_id: id,
                    width: 1920,
                    height: 1080,
                    max_fps: 30,
                    bitrate_bps: 6_000_000,
                    rtp_payload_type: 96,
                    rtp_clock_hz: 90_000,
                    teacher_media_udp_port: 47811,
                },
                events,
                recipients: None,
                excluded_source: None,
            },
            receiver,
        )
    }
    #[tokio::test]
    async fn selected_rejoins_but_excluded_late_join_never_receives_presentation() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let SocketAddr::V4(address) = listener.local_addr().unwrap() else {
            panic!("IPv4")
        };
        let (mut session, events) = presentation(91);
        session.recipients = Some(Arc::new(HashSet::from(["selected".into()])));
        let (publisher, updates) = watch::channel(Some(session));
        let roster = Arc::new(Mutex::new(RoomRoster::default()));
        let server_roster = Arc::clone(&roster);
        let server = tokio::spawn(async move {
            let mut tasks = JoinSet::new();
            for id in 1..=3 {
                let (socket, peer) = listener.accept().await.unwrap();
                tasks.spawn(handle(
                    socket,
                    peer,
                    id,
                    updates.clone(),
                    Arc::clone(&server_roster),
                ));
            }
            while let Some(result) = tasks.join_next().await {
                result.unwrap().unwrap();
            }
        });
        let registration = |instance: &str| StudentRegistration {
            hostname: instance.into(),
            instance_id: instance.into(),
            selected_local_ip: Ipv4Addr::LOCALHOST,
            media_udp_port: 47810,
            feedback_udp_port: 47811,
            pointer_overlay: false,
        };
        let (selected, _, initial) =
            register_student_for_presentation(address, registration("selected"))
                .await
                .unwrap();
        assert_eq!(initial.unwrap().stream_id, 91);
        let (excluded, _, initial) =
            register_student_for_presentation(address, registration("excluded"))
                .await
                .unwrap();
        assert!(initial.is_none());
        excluded.probe_rtt().unwrap();
        assert!(excluded.is_connected());
        assert!(
            matches!(events.try_recv().unwrap(), TeacherMediaEvent::Registered(s) if s.hostname == "selected")
        );
        assert!(events.try_recv().is_err());
        drop(selected);
        wait_until(|| !roster.lock().unwrap().snapshot()[0].online).await;
        assert!(matches!(
            events.try_recv().unwrap(),
            TeacherMediaEvent::Disconnected { client_id: 1, .. }
        ));
        let (rejoined, _, initial) =
            register_student_for_presentation(address, registration("selected"))
                .await
                .unwrap();
        assert_eq!(initial.unwrap().stream_id, 91);
        assert_eq!(roster.lock().unwrap().snapshot().len(), 2);
        assert!(
            matches!(events.try_recv().unwrap(), TeacherMediaEvent::Registered(s) if s.client_id == 3)
        );
        publisher.send_replace(None);
        wait_until(|| rejoined.presentation().is_none()).await;
        assert!(excluded.presentation().is_none());
        assert!(excluded.is_connected());
        drop(rejoined);
        drop(excluded);
        tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .unwrap()
            .unwrap();
    }
    #[tokio::test]
    async fn idle_start_stop_restart_keep_same_connection_and_drop_removes_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let SocketAddr::V4(address) = listener.local_addr().unwrap() else {
            panic!("IPv4")
        };
        let (publisher, updates) = watch::channel(None);
        let roster = Arc::new(Mutex::new(RoomRoster::default()));
        let server_roster = Arc::clone(&roster);
        let server = tokio::spawn(async move {
            let (socket, peer) = listener.accept().await.unwrap();
            handle(socket, peer, 42, updates, server_roster).await
        });
        let (control, id, initial) = register_student_for_presentation(
            address,
            StudentRegistration {
                hostname: "student-test".into(),
                instance_id: "persistent-test".into(),
                selected_local_ip: Ipv4Addr::LOCALHOST,
                media_udp_port: 47810,
                feedback_udp_port: 47811,
                pointer_overlay: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(id, 42);
        assert!(initial.is_none());
        assert!(control.is_connected());
        for stream_id in 1..=3 {
            let (session, events) = presentation(stream_id);
            publisher.send_replace(Some(session));
            wait_until(|| control.is_presenting(stream_id)).await;
            assert!(
                matches!(events.try_recv().unwrap(), TeacherMediaEvent::Registered(student) if student.client_id == id)
            );
            assert_eq!(roster.lock().unwrap().snapshot().len(), 1);
            control.probe_rtt().unwrap();
            publisher.send_replace(None);
            wait_until(|| control.presentation().is_none()).await;
            assert!(control.is_connected());
            assert!(
                matches!(events.try_recv().unwrap(), TeacherMediaEvent::Disconnected {client_id,..} if client_id == id)
            );
        }
        let (session, events) = presentation(4);
        publisher.send_replace(Some(session));
        wait_until(|| control.is_presenting(4)).await;
        assert!(matches!(
            events.try_recv().unwrap(),
            TeacherMediaEvent::Registered(_)
        ));
        let last_owner = control.clone();
        drop(control);
        assert!(last_owner.is_connected());
        last_owner.probe_rtt().unwrap();
        drop(last_owner);
        tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(!roster.lock().unwrap().snapshot()[0].online);
        assert!(matches!(
            events.try_recv().unwrap(),
            TeacherMediaEvent::Disconnected { client_id: 42, .. }
        ));
    }
    #[tokio::test]
    async fn presentation_updates_do_not_cancel_a_partial_tcp_frame() {
        use prost::Message;
        use tokio::io::AsyncWriteExt;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (publisher, updates) = watch::channel(None);
        let roster = Arc::new(Mutex::new(RoomRoster::default()));
        let server = tokio::spawn(async move {
            let (socket, peer) = listener.accept().await.unwrap();
            handle(socket, peer, 1, updates, roster).await
        });
        let mut socket = TcpStream::connect(address).await.unwrap();
        write_envelope(
            &mut socket,
            &Envelope::hello(protocol::HelloStudent {
                hostname: "fragmented".into(),
                instance_id: "fragmented".into(),
                selected_local_ip: "127.0.0.1".into(),
                media_udp_port: 47810,
                feedback_udp_port: 47811,
                pointer_overlay: false,
            }),
        )
        .await
        .unwrap();
        assert!(matches!(
            read_envelope(&mut socket).await.unwrap().payload,
            Some(Payload::WelcomeStudent(_))
        ));
        let bytes = Envelope::control_ping(17).encode_to_vec();
        let size = u32::try_from(bytes.len()).unwrap().to_be_bytes();
        socket.write_all(&size[..2]).await.unwrap();
        let (session, _events) = presentation(1);
        publisher.send_replace(Some(session));
        assert!(matches!(
            read_envelope(&mut socket).await.unwrap().payload,
            Some(Payload::StartPresentation(_))
        ));
        publisher.send_replace(None);
        assert!(matches!(
            read_envelope(&mut socket).await.unwrap().payload,
            Some(Payload::StopPresentation(_))
        ));
        socket.write_all(&size[2..]).await.unwrap();
        socket.write_all(&bytes).await.unwrap();
        let response = tokio::time::timeout(Duration::from_secs(2), read_envelope(&mut socket))
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(response.payload, Some(Payload::ControlPong(value)) if value.nonce == 17));
        drop(socket);
        server.await.unwrap().unwrap();
    }
    #[test]
    fn reconnect_does_not_let_old_connection_mark_new_one_offline() {
        let mut roster = RoomRoster::default();
        let row = RoomStudent {
            client_id: 1,
            instance_id: "same-machine".into(),
            hostname: "student".into(),
            endpoint: "127.0.0.1:47810".parse().unwrap(),
            online: true,
            thumbnail: None,
            monitoring_state: "waiting".into(),
            media: None,
        };
        roster.register(row.clone()).unwrap();
        roster
            .register(RoomStudent {
                client_id: 2,
                ..row
            })
            .unwrap();
        roster.disconnect(1);
        assert_eq!(roster.snapshot().len(), 1);
        assert!(roster.snapshot()[0].online);
        roster.disconnect(2);
        assert!(!roster.snapshot()[0].online);
    }
    #[tokio::test]
    async fn class_control_and_relay_share_source_without_stealing_mode_or_cursor() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let SocketAddr::V4(address) = listener.local_addr().unwrap() else {
            panic!("IPv4")
        };
        let (mut session, _events) = presentation(88);
        session.recipients = Some(Arc::new(HashSet::from([
            "secret".into(),
            "secret:view".into(),
        ])));
        let (_state, updates) = watch::channel(Some(session));
        let roster = Arc::new(Mutex::new(RoomRoster::default()));
        let server = tokio::spawn(run_authorized_listener(
            listener,
            updates,
            Arc::clone(&roster),
            Ipv4Addr::LOCALHOST,
            "secret".into(),
        ));
        let reg = |token: &str, port| StudentRegistration {
            hostname: "Teacher".into(),
            instance_id: token.into(),
            selected_local_ip: Ipv4Addr::LOCALHOST,
            media_udp_port: port,
            feedback_udp_port: port,
            pointer_overlay: false,
        };
        assert!(
            register_student_for_presentation(address, reg("secret:other", 49001))
                .await
                .is_err()
        );
        let (relay, _, first) = register_student_for_presentation(address, reg("secret", 49001))
            .await
            .unwrap();
        let (view, _, second) =
            register_student_for_presentation(address, reg("secret:view", 49002))
                .await
                .unwrap();
        assert_eq!(first.unwrap().stream_id, second.unwrap().stream_id);
        assert_eq!(
            roster
                .lock()
                .unwrap()
                .snapshot()
                .iter()
                .filter(|r| r.online)
                .count(),
            2
        );
        view.observe_input(protocol::ObserveMessage {
            action: "control".into(),
            stream_id: 88,
            ..Default::default()
        })
        .unwrap();
        wait_until(|| !roster.lock().unwrap().inputs.is_empty()).await;
        assert_eq!(
            roster.lock().unwrap().take_input().unwrap().action,
            "control"
        );
        roster
            .lock()
            .unwrap()
            .input_status(protocol::ObserveMessage {
                action: "status".into(),
                key: "control".into(),
                ..Default::default()
            });
        roster
            .lock()
            .unwrap()
            .cursor_status(protocol::ObserveMessage {
                action: "cursor".into(),
                key: "cursor-data".into(),
                ..Default::default()
            });
        let mut mode = None;
        wait_until(|| {
            mode = view.take_input_status();
            mode.is_some()
        })
        .await;
        assert_eq!(mode.unwrap().key, "control");
        let mut cursor = None;
        wait_until(|| {
            cursor = view.take_cursor_status();
            cursor.is_some()
        })
        .await;
        assert_eq!(cursor.unwrap().key, "cursor-data");
        assert!(relay.take_input_status().is_none());
        assert!(relay.take_cursor_status().is_none());
        assert!(relay.is_presenting(88));
        drop(view);
        wait_until(|| {
            roster
                .lock()
                .unwrap()
                .snapshot()
                .iter()
                .filter(|r| r.online)
                .count()
                == 1
        })
        .await;
        assert!(relay.is_presenting(88));
        server.abort();
    }
    #[tokio::test]
    async fn reverse_publisher_requires_session_token_and_scopes_input_to_stream() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let SocketAddr::V4(address) = listener.local_addr().unwrap() else {
            panic!("IPv4")
        };
        let (mut session, _events) = presentation(77);
        session.recipients = Some(Arc::new(HashSet::from(["secret-session".into()])));
        let (_state, updates) = watch::channel(Some(session));
        let roster = Arc::new(Mutex::new(RoomRoster::default()));
        let server = tokio::spawn(run_authorized_listener(
            listener,
            updates,
            Arc::clone(&roster),
            Ipv4Addr::LOCALHOST,
            "secret-session".into(),
        ));
        let reg = |token: &str| StudentRegistration {
            hostname: "Teacher Observe".into(),
            instance_id: token.into(),
            selected_local_ip: Ipv4Addr::LOCALHOST,
            media_udp_port: 47810,
            feedback_udp_port: 47811,
            pointer_overlay: false,
        };
        assert!(
            register_student_for_presentation(address, reg("wrong-session"))
                .await
                .is_err()
        );
        let (control, _, initial) =
            register_student_for_presentation(address, reg("secret-session"))
                .await
                .unwrap();
        assert_eq!(initial.unwrap().stream_id, 77);
        control
            .observe_input(protocol::ObserveMessage {
                action: "control".into(),
                stream_id: 77,
                ..Default::default()
            })
            .unwrap();
        wait_until(|| !roster.lock().unwrap().inputs.is_empty()).await;
        assert_eq!(
            roster.lock().unwrap().take_input().unwrap().action,
            "control"
        );
        control
            .observe_input(protocol::ObserveMessage {
                action: "key".into(),
                stream_id: 76,
                ..Default::default()
            })
            .unwrap();
        wait_until(|| !control.is_connected()).await;
        assert!(roster.lock().unwrap().take_input().is_none());
        server.abort();
    }
}
