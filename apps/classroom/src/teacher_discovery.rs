//! How a Student finds its Teacher when nobody has told it where the Teacher is,
//! and how a Teacher that has just started tells waiting Students it is back.
//!
//! Student side, in priority order:
//!
//! 1. the address configured by hand (installer, `--teacher-ip`, settings);
//! 2. the address that last worked (remembered in the Student profile);
//! 3. the room convention: `.200` of the Student's own subnet.
//!
//! These are probed together and the best one that answers is used. When none
//! answers for a while, the Student scans its own `/24` (at most 254 hosts) for
//! a listener on the control port and tries each one with the real
//! registration, which is what proves it is a Teacher. A Teacher that starts
//! later does not have to wait for that: it sends a small UDP "knock" to the
//! Students it has seen before and the Student tries the sender at once.
//!
//! Nothing here uses broadcast or multicast, and nothing is scanned beyond the
//! Student's own /24. A manual address always wins while it answers; turning
//! `network.teacher_discovery` off makes the Student use only that address.
use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddrV4, UdpSocket},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{net::TcpStream, sync::Semaphore, task::JoinSet};
use tracing::{info, warn};

/// UDP port of the knock is the control port plus this.
pub const KNOCK_PORT_OFFSET: u16 = 20;
const MAGIC: &[u8; 12] = b"CLSRM-KNOCK1";
const PACKET_LEN: usize = MAGIC.len() + 2;
const PROBE_TIMEOUT: Duration = Duration::from_millis(350);
const SCAN_TIMEOUT: Duration = Duration::from_millis(300);
const SCAN_CONCURRENCY: usize = 128;
/// How long the Teacher must have been silent before a subnet scan starts.
const SCAN_AFTER: Duration = Duration::from_secs(6);
const SCAN_EVERY: Duration = Duration::from_secs(45);
const ALTERNATIVES_EVERY: Duration = Duration::from_secs(4);
/// A Teacher verified this recently is preferred even over a manual address
/// that has just stopped working, so the Student cannot flip back and forth.
const RECENT: Duration = Duration::from_secs(60);
const REMEMBER_DAYS: u64 = 90;
const MAX_REMEMBERED: usize = 512;

#[must_use]
pub fn knock_port(control_port: u16) -> Option<u16> {
    control_port.checked_add(KNOCK_PORT_OFFSET)
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

// ------------------------------------------------------------ pure helpers

/// The room convention: `.200` of the Student's own /24.
#[must_use]
pub fn convention(local: Ipv4Addr) -> Ipv4Addr {
    let o = local.octets();
    Ipv4Addr::new(o[0], o[1], o[2], 200)
}

/// Teacher candidates, best first, without duplicates.
///
/// `explicit` means the configured address was chosen by a person (or an
/// installer); it then leads. `last_good_recent` lets a Teacher verified in the
/// last minute lead instead. With discovery off only the configured address is
/// ever used.
#[must_use]
pub fn candidates(
    local: Ipv4Addr,
    configured: Ipv4Addr,
    explicit: bool,
    discovery: bool,
    last_good: Option<(Ipv4Addr, bool)>,
) -> Vec<Ipv4Addr> {
    if !discovery {
        return vec![configured];
    }
    let mut out: Vec<Ipv4Addr> = Vec::new();
    let mut push = |ip: Ipv4Addr| {
        if !ip.is_unspecified() && !out.contains(&ip) {
            out.push(ip);
        }
    };
    let recent = last_good.filter(|(_, recent)| *recent).map(|(ip, _)| ip);
    if let Some(ip) = recent {
        push(ip);
    }
    if explicit {
        push(configured);
    }
    if let Some((ip, _)) = last_good {
        push(ip);
    }
    push(convention(local));
    if !explicit {
        push(configured);
    }
    out
}

/// Every host of the Student's /24 except itself: `.200` first, then the rest
/// ascending. Empty for loopback and link-local addresses.
#[must_use]
pub fn subnet_hosts(local: Ipv4Addr) -> Vec<Ipv4Addr> {
    if local.is_loopback() || local.is_link_local() || local.is_unspecified() {
        return Vec::new();
    }
    let o = local.octets();
    let mut hosts: Vec<Ipv4Addr> = (1..=254u8)
        .filter(|last| *last != o[3] && *last != 200)
        .map(|last| Ipv4Addr::new(o[0], o[1], o[2], last))
        .collect();
    if o[3] != 200 {
        hosts.insert(0, Ipv4Addr::new(o[0], o[1], o[2], 200));
    }
    hosts
}

fn knock_packet(control_port: u16) -> [u8; PACKET_LEN] {
    let mut packet = [0u8; PACKET_LEN];
    packet[..MAGIC.len()].copy_from_slice(MAGIC);
    packet[MAGIC.len()..].copy_from_slice(&control_port.to_be_bytes());
    packet
}

/// The control port a valid knock announces.
#[must_use]
pub fn parse_knock(packet: &[u8]) -> Option<u16> {
    (packet.len() == PACKET_LEN && packet.starts_with(MAGIC))
        .then(|| u16::from_be_bytes([packet[MAGIC.len()], packet[MAGIC.len() + 1]]))
}

// ---------------------------------------------------------------- probing

async fn answers(ip: Ipv4Addr, port: u16, timeout: Duration) -> bool {
    matches!(
        tokio::time::timeout(timeout, TcpStream::connect(SocketAddrV4::new(ip, port))).await,
        Ok(Ok(_))
    )
}

/// The addresses in `ips` that accept a TCP connection on `port`, in the order
/// given, probed `concurrency` at a time.
pub async fn responders(
    ips: &[Ipv4Addr],
    port: u16,
    timeout: Duration,
    concurrency: usize,
) -> Vec<Ipv4Addr> {
    let gate = Arc::new(Semaphore::new(concurrency.max(1)));
    let mut set = JoinSet::new();
    for (index, ip) in ips.iter().copied().enumerate() {
        let gate = Arc::clone(&gate);
        set.spawn(async move {
            let _permit = gate.acquire_owned().await.ok()?;
            answers(ip, port, timeout).await.then_some((index, ip))
        });
    }
    let mut alive = Vec::new();
    while let Some(done) = set.join_next().await {
        if let Ok(Some(found)) = done {
            alive.push(found);
        }
    }
    alive.sort_by_key(|(index, _)| *index);
    alive.into_iter().map(|(_, ip)| ip).collect()
}

// -------------------------------------------------------------- remembered

/// The Teacher that last worked, and whether that was within [`RECENT`].
#[must_use]
pub fn load_last_good(profile: &Path) -> Option<(Ipv4Addr, bool)> {
    let value: serde_json::Value = super::startup::profile_doc(profile, "teacher_last")
        .ok()?
        .read()
        .ok()??;
    let ip = value.get("ip")?.as_str()?.parse::<Ipv4Addr>().ok()?;
    let at = value.get("at").and_then(serde_json::Value::as_u64).unwrap_or(0);
    Some((ip, unix_seconds().saturating_sub(at) < RECENT.as_secs()))
}

pub fn save_last_good(profile: &Path, ip: Ipv4Addr) {
    if ip.is_loopback() || ip.is_unspecified() {
        return;
    }
    let result = super::startup::profile_doc(profile, "teacher_last").and_then(|doc| {
        doc.write(&serde_json::json!({"ip": ip.to_string(), "at": unix_seconds()}))
            .map_err(Into::into)
    });
    if let Err(error) = result {
        warn!(%error, "Could not remember the Teacher address");
    }
}

/// Where a Student starting up should aim: the best candidate that answers
/// right now, otherwise the best candidate (the Teacher may simply not be on).
pub async fn resolve_initial(
    local: Ipv4Addr,
    configured: Ipv4Addr,
    explicit: bool,
    discovery: bool,
    control_port: u16,
    profile: &Path,
) -> Ipv4Addr {
    let list = candidates(
        local,
        configured,
        explicit,
        discovery,
        load_last_good(profile),
    );
    let alive = responders(&list, control_port, PROBE_TIMEOUT, 8).await;
    let chosen = alive.first().copied().unwrap_or(list[0]);
    info!(
        %chosen,
        answering = alive.len(),
        candidates = list.len(),
        "Teacher address chosen"
    );
    chosen
}

// ------------------------------------------------------------------ Finder

/// Used while a Student cannot reach its Teacher: paces the retries and finds
/// other places the Teacher might be.
pub struct Finder {
    local: Ipv4Addr,
    port: u16,
    current: Ipv4Addr,
    configured: Ipv4Addr,
    explicit: bool,
    discovery: bool,
    failing_since: Option<Instant>,
    last_check: Option<Instant>,
    last_scan: Option<Instant>,
    attempt: u32,
}

impl Finder {
    #[must_use]
    pub fn new(
        local: Ipv4Addr,
        port: u16,
        current: Ipv4Addr,
        configured: Ipv4Addr,
        explicit: bool,
        discovery: bool,
    ) -> Self {
        Self {
            local,
            port,
            current,
            configured,
            explicit,
            discovery,
            failing_since: None,
            last_check: None,
            last_scan: None,
            attempt: 0,
        }
    }

    /// The Teacher answered: forget the failures.
    pub fn connected(&mut self) {
        self.failing_since = None;
        self.attempt = 0;
    }

    /// Records a failed attempt and returns how long to wait before the next:
    /// 0.5 s, 1 s, 2 s, then every 3 s, each a little randomised so forty
    /// Students do not retry in step.
    pub fn failed(&mut self) -> Duration {
        self.failing_since.get_or_insert_with(Instant::now);
        self.attempt = self.attempt.saturating_add(1);
        let base_ms: u64 = match self.attempt {
            1 => 500,
            2 => 1_000,
            3 => 2_000,
            _ => 3_000,
        };
        Duration::from_millis(base_ms * (80 + jitter_percent()) / 100)
    }

    #[must_use]
    pub fn failing_for(&self) -> Duration {
        self.failing_since.map_or(Duration::ZERO, |t| t.elapsed())
    }

    /// Other addresses worth trying, best first: Teachers that knocked, then
    /// the known candidates that answer, then (after a while) hosts of the
    /// Student's own /24 with the control port open. Each still has to
    /// register as a Teacher before it is believed.
    pub async fn alternatives(&mut self, knocks: &Knocks) -> Vec<Ipv4Addr> {
        if !self.discovery {
            return Vec::new();
        }
        let mut out: Vec<Ipv4Addr> = Vec::new();
        for ip in knocks.take() {
            if ip != self.current && !out.contains(&ip) {
                out.push(ip);
            }
        }
        if self.last_check.is_some_and(|t| t.elapsed() < ALTERNATIVES_EVERY) {
            return out;
        }
        self.last_check = Some(Instant::now());
        // Whatever the profile remembers may have changed since the start.
        let known: Vec<Ipv4Addr> = candidates(
            self.local,
            self.configured,
            self.explicit,
            true,
            None,
        )
        .into_iter()
        .filter(|ip| *ip != self.current)
        .collect();
        for ip in responders(&known, self.port, PROBE_TIMEOUT, 8).await {
            if !out.contains(&ip) {
                out.push(ip);
            }
        }
        let scan_due = self.failing_for() >= SCAN_AFTER
            && self.last_scan.is_none_or(|t| t.elapsed() >= SCAN_EVERY);
        if scan_due {
            self.last_scan = Some(Instant::now());
            let hosts: Vec<Ipv4Addr> = subnet_hosts(self.local)
                .into_iter()
                .filter(|ip| *ip != self.current && !known.contains(ip))
                .collect();
            let started = Instant::now();
            let found = responders(&hosts, self.port, SCAN_TIMEOUT, SCAN_CONCURRENCY).await;
            info!(
                scanned = hosts.len(),
                found = found.len(),
                ms = started.elapsed().as_millis() as u64,
                "Teacher search on the local subnet finished"
            );
            for ip in found {
                if !out.contains(&ip) {
                    out.push(ip);
                }
            }
        }
        out
    }
}

fn jitter_percent() -> u64 {
    let mut byte = [0u8; 1];
    if getrandom::fill(&mut byte).is_err() {
        return 20;
    }
    u64::from(byte[0]) * 41 / 255 // 0..=40, so the delay is 80%..=120%
}

// ------------------------------------------------------- Student: knock in

/// Addresses of Teachers that knocked and have not been looked at yet.
#[derive(Clone, Default)]
pub struct Knocks(Arc<Mutex<Vec<Ipv4Addr>>>);

impl Knocks {
    fn push(&self, ip: Ipv4Addr) {
        let mut list = self.0.lock().unwrap();
        if !list.contains(&ip) && list.len() < 8 {
            list.push(ip);
        }
    }
    pub fn take(&self) -> Vec<Ipv4Addr> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
    /// Whether a knock is waiting (without consuming it).
    #[must_use]
    pub fn pending(&self) -> bool {
        !self.0.lock().unwrap().is_empty()
    }
    /// Waits out `delay`, but returns as soon as a Teacher knocks.
    pub async fn wait(&self, delay: Duration) {
        let started = Instant::now();
        while started.elapsed() < delay && !self.pending() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

/// Keeps the knock listener alive; dropping it stops the thread.
pub struct Listener(Arc<AtomicBool>);
impl Drop for Listener {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

/// Starts listening for a Teacher's knock. Without it the Student still finds
/// the Teacher by itself, just a little later.
#[must_use]
pub fn listen(control_port: u16) -> (Knocks, Option<Listener>) {
    let knocks = Knocks::default();
    let Some(port) = knock_port(control_port) else {
        return (knocks, None);
    };
    let socket = match UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port)) {
        Ok(socket) => socket,
        Err(error) => {
            warn!(%error, port, "Teacher knock listener unavailable");
            return (knocks, None);
        }
    };
    let _ = socket.set_read_timeout(Some(Duration::from_secs(1)));
    let stop = Arc::new(AtomicBool::new(false));
    let (thread_stop, sink) = (Arc::clone(&stop), knocks.clone());
    let spawned = std::thread::Builder::new()
        .name("teacher-knock-listener".into())
        .spawn(move || {
            let mut buffer = [0u8; 64];
            while !thread_stop.load(Ordering::Acquire) {
                let Ok((length, from)) = socket.recv_from(&mut buffer) else {
                    continue;
                };
                let std::net::SocketAddr::V4(from) = from else {
                    continue;
                };
                if parse_knock(&buffer[..length]) == Some(control_port) {
                    sink.push(*from.ip());
                }
            }
        });
    match spawned {
        Ok(_) => (knocks, Some(Listener(stop))),
        Err(error) => {
            warn!(%error, "Teacher knock listener could not start");
            (knocks, None)
        }
    }
}

// ------------------------------------------------------ Teacher: knock out

/// Keeps the Teacher's knocking thread alive; dropping it stops it.
pub struct Knocker(Arc<AtomicBool>);
impl Drop for Knocker {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

/// Remembers the Students this Teacher has seen and, from the moment it
/// starts, knocks on those that are not connected: after 1, 2, 4, 8 and 16
/// seconds, then every 30 seconds. A Student that is waiting for its Teacher
/// (or still looking at an old address) connects within a second or two.
#[must_use]
pub fn spawn_knocker(
    roster: Arc<Mutex<control_transport::room::RoomRoster>>,
    store: classroom_store::Shared,
    bind_ip: Ipv4Addr,
    control_port: u16,
) -> Option<Knocker> {
    if bind_ip.is_loopback() || bind_ip.is_unspecified() {
        return None;
    }
    let port = knock_port(control_port)?;
    let socket = UdpSocket::bind((bind_ip, 0)).ok()?;
    let doc = store.doc("knock", "students");
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    std::thread::Builder::new()
        .name("teacher-knocker".into())
        .spawn(move || {
            let mut seen: HashMap<Ipv4Addr, u64> = doc
                .read::<HashMap<String, u64>>()
                .ok()
                .flatten()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|(ip, at)| ip.parse().ok().map(|ip| (ip, at)))
                .collect();
            let packet = knock_packet(control_port);
            let steps = [1u64, 2, 4, 8, 16];
            let mut step = 0usize;
            let mut next_knock = Instant::now();
            let mut dirty = false;
            let mut last_saved = Instant::now();
            while !thread_stop.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(500));
                let now = unix_seconds();
                let online: Vec<Ipv4Addr> = {
                    let rows = roster.lock().unwrap().snapshot();
                    rows.iter()
                        .filter(|row| row.online)
                        .map(|row| *row.endpoint.ip())
                        .collect()
                };
                for ip in &online {
                    // Refresh only now and then; this is a "have I met it" list.
                    if seen.get(ip).is_none_or(|at| now.saturating_sub(*at) > 600) {
                        seen.insert(*ip, now);
                        dirty = true;
                    }
                }
                if Instant::now() >= next_knock {
                    let wait = steps.get(step).copied().unwrap_or(30);
                    step += 1;
                    next_knock = Instant::now() + Duration::from_secs(wait);
                    let mut sent = 0;
                    for ip in seen.keys().filter(|ip| !online.contains(ip)) {
                        if socket.send_to(&packet, SocketAddrV4::new(*ip, port)).is_ok() {
                            sent += 1;
                        }
                    }
                    if sent > 0 {
                        info!(sent, "knocked on Students that are not connected");
                    }
                }
                if dirty && last_saved.elapsed() >= Duration::from_secs(10) {
                    seen.retain(|_, at| now.saturating_sub(*at) < REMEMBER_DAYS * 86_400);
                    while seen.len() > MAX_REMEMBERED {
                        let Some(oldest) = seen.iter().min_by_key(|(_, at)| **at).map(|(ip, _)| *ip)
                        else {
                            break;
                        };
                        seen.remove(&oldest);
                    }
                    let document: HashMap<String, u64> =
                        seen.iter().map(|(ip, at)| (ip.to_string(), *at)).collect();
                    if let Err(error) = doc.write(&document) {
                        warn!(%error, "Could not remember the Students seen");
                    }
                    dirty = false;
                    last_saved = Instant::now();
                }
            }
        })
        .ok()?;
    Some(Knocker(stop))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(text: &str) -> Ipv4Addr {
        text.parse().unwrap()
    }

    #[test]
    fn manual_address_leads_then_last_good_then_the_room_convention() {
        let local = ip("10.0.51.111");
        let list = candidates(local, ip("10.0.51.150"), true, true, Some((ip("10.0.51.77"), false)));
        assert_eq!(list, [ip("10.0.51.150"), ip("10.0.51.77"), ip("10.0.51.200")]);
        // Not chosen by a person: what worked, the convention, then the default.
        let auto = candidates(local, ip("10.0.0.5"), false, true, Some((ip("10.0.51.77"), false)));
        assert_eq!(auto, [ip("10.0.51.77"), ip("10.0.51.200"), ip("10.0.0.5")]);
        // Duplicates collapse.
        let same = candidates(local, ip("10.0.51.200"), true, true, Some((ip("10.0.51.200"), false)));
        assert_eq!(same, [ip("10.0.51.200")]);
    }

    #[test]
    fn a_freshly_verified_teacher_beats_a_manual_address_and_discovery_off_pins_it() {
        let local = ip("192.168.1.20");
        let list = candidates(local, ip("192.168.1.5"), true, true, Some((ip("192.168.1.9"), true)));
        assert_eq!(list[0], ip("192.168.1.9"));
        assert_eq!(
            candidates(local, ip("192.168.1.5"), true, false, Some((ip("192.168.1.9"), true))),
            [ip("192.168.1.5")]
        );
    }

    #[test]
    fn the_scan_covers_the_own_subnet_only_with_the_convention_first() {
        let hosts = subnet_hosts(ip("192.168.50.111"));
        assert_eq!(hosts.len(), 253); // .1-.254 without itself
        assert_eq!(hosts[0], ip("192.168.50.200"));
        assert!(!hosts.contains(&ip("192.168.50.111")));
        assert!(hosts.iter().all(|h| h.octets()[..3] == [192, 168, 50]));
        assert_eq!(hosts.iter().filter(|h| h.octets()[3] == 200).count(), 1);
        assert!(subnet_hosts(ip("127.0.0.1")).is_empty());
        assert!(subnet_hosts(ip("169.254.3.4")).is_empty());
        // The Teacher's own .200 machine scanning does not list itself.
        assert!(!subnet_hosts(ip("10.0.51.200")).contains(&ip("10.0.51.200")));
    }

    #[test]
    fn knock_packets_round_trip_and_reject_garbage() {
        assert_eq!(parse_knock(&knock_packet(47800)), Some(47800));
        assert_eq!(parse_knock(b"CLSRM-KNOCK1"), None);
        assert_eq!(parse_knock(b"SOMETHING-ELSE\x00\x01"), None);
        assert_eq!(knock_port(47800), Some(47820));
        assert_eq!(knock_port(65530), None);
    }

    #[tokio::test]
    async fn probing_finds_listeners_in_priority_order() {
        let a = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = a.local_addr().unwrap().port();
        // 127.0.0.1 listens; 127.0.0.2 / .3 are not served on this port.
        let list = [ip("127.0.0.3"), ip("127.0.0.1"), ip("127.0.0.2")];
        let found = responders(&list, port, Duration::from_millis(300), 4).await;
        assert_eq!(found, [ip("127.0.0.1")]);
    }

    #[tokio::test]
    async fn a_knock_is_heard_and_a_wrong_port_is_ignored() {
        let control = 47_900u16;
        let (knocks, listener) = listen(control);
        let Some(_listener) = listener else {
            return; // the port is busy on this machine; nothing to assert
        };
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let target = SocketAddrV4::new(Ipv4Addr::LOCALHOST, control + KNOCK_PORT_OFFSET);
        socket.send_to(&knock_packet(control + 1), target).unwrap();
        socket.send_to(&knock_packet(control), target).unwrap();
        for _ in 0..40 {
            if !knocks.0.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert_eq!(knocks.take(), [Ipv4Addr::LOCALHOST]);
        assert!(knocks.take().is_empty());
    }

    #[test]
    fn retries_slow_down_but_never_stop_and_stay_within_the_jitter() {
        let mut finder = Finder::new(
            ip("10.0.51.111"),
            47800,
            ip("10.0.51.200"),
            ip("10.0.51.200"),
            true,
            true,
        );
        let delays: Vec<u64> = (0..7).map(|_| finder.failed().as_millis() as u64).collect();
        for (delay, base) in delays.iter().zip([500, 1000, 2000, 3000, 3000, 3000, 3000]) {
            assert!(
                (base * 8 / 10..=base * 12 / 10).contains(delay),
                "{delay} vs {base}"
            );
        }
        finder.connected();
        assert!(finder.failed().as_millis() <= 600);
    }

    #[tokio::test]
    async fn alternatives_report_a_knocking_teacher_first_and_nothing_when_pinned() {
        let knocks = Knocks::default();
        knocks.push(ip("10.0.51.77"));
        let mut finder = Finder::new(
            ip("10.0.51.111"),
            47800,
            ip("10.0.51.200"),
            ip("10.0.51.200"),
            true,
            true,
        );
        let list = finder.alternatives(&knocks).await;
        assert_eq!(list.first(), Some(&ip("10.0.51.77")));
        // The current address is never offered back.
        assert!(!list.contains(&ip("10.0.51.200")));
        let mut pinned = Finder::new(
            ip("10.0.51.111"),
            47800,
            ip("10.0.51.200"),
            ip("10.0.51.200"),
            true,
            false,
        );
        knocks.push(ip("10.0.51.77"));
        assert!(pinned.alternatives(&knocks).await.is_empty());
    }
}
