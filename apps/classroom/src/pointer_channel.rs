//! O1: Teacher pointer channel. A small worker samples the pointer at up to
//! ~120 Hz, independent of video cadence, and sends tiny state datagrams on
//! change (plus a keepalive) and the pointer image when it changes or a new
//! receiver appears (plus a slow periodic repeat). Nothing here touches the
//! encoder; the media loop only decides whether capture composites the pointer.
use media_core::{PointerImage, PointerSource};
use media_transport::pointer::{PointerPacketizer, PointerState};
use std::{
    net::{SocketAddrV4, UdpSocket},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use tracing::{info, warn};

const SAMPLE_INTERVAL: Duration = Duration::from_millis(8);
const KEEPALIVE: Duration = Duration::from_millis(250);
const IMAGE_REPEAT: Duration = Duration::from_secs(2);

#[derive(Default)]
pub struct PointerStats {
    pub state_packets: AtomicU64,
    pub image_packets: AtomicU64,
    pub moves: AtomicU64,
    pub send_errors: AtomicU64,
}

pub struct PointerBroadcaster {
    stop: Arc<AtomicBool>,
    endpoints: Arc<Mutex<(Vec<SocketAddrV4>, bool)>>,
    pub stats: Arc<PointerStats>,
    worker: Option<thread::JoinHandle<()>>,
}

impl PointerBroadcaster {
    pub fn start(socket: UdpSocket, mut source: Box<dyn PointerSource>, ssrc: u32) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let endpoints = Arc::new(Mutex::new((Vec::<SocketAddrV4>::new(), false)));
        let stats = Arc::new(PointerStats::default());
        let (cancel, targets, counters) = (
            Arc::clone(&stop),
            Arc::clone(&endpoints),
            Arc::clone(&stats),
        );
        let worker = thread::spawn(move || {
            let started = Instant::now();
            let mut packetizer = PointerPacketizer::new(ssrc);
            let mut last_state: Option<PointerState> = None;
            let mut last_state_sent = started;
            let mut last_image: Option<Arc<PointerImage>> = None;
            let mut last_image_sent = started;
            let mut warned = false;
            info!("teacher pointer channel started");
            while !cancel.load(Ordering::Acquire) {
                let tick = Instant::now();
                let (list, joined) = {
                    let mut guard = targets.lock().unwrap();
                    let joined = std::mem::take(&mut guard.1);
                    (guard.0.clone(), joined)
                };
                if let (false, Some(sample)) = (list.is_empty(), source.sample()) {
                    let timestamp = u32::try_from(started.elapsed().as_millis() & 0xffff_ffff)
                        .unwrap_or_default();
                    let mut send = |bytes: &[u8]| {
                        for endpoint in &list {
                            if let Err(error) = socket.send_to(bytes, endpoint) {
                                counters.send_errors.fetch_add(1, Ordering::Relaxed);
                                if !warned {
                                    warn!(%error, %endpoint, "pointer channel send failed");
                                    warned = true;
                                }
                            }
                        }
                    };
                    let image_changed = match (&sample.image, &last_image) {
                        (Some(a), Some(b)) => !Arc::ptr_eq(a, b) && a.serial != b.serial,
                        (Some(_), None) => true,
                        _ => false,
                    };
                    if let Some(image) = sample.image.as_ref().filter(|_| {
                        image_changed || joined || last_image_sent.elapsed() >= IMAGE_REPEAT
                    }) {
                        for chunk in packetizer.image(image, timestamp) {
                            send(&chunk);
                            counters.image_packets.fetch_add(1, Ordering::Relaxed);
                        }
                        last_image = Some(Arc::clone(image));
                        last_image_sent = Instant::now();
                    }
                    let current = PointerState {
                        visible: sample.visible,
                        serial: sample.image.as_ref().map_or(0, |image| image.serial),
                        x: sample.x,
                        y: sample.y,
                        width_fraction: sample.width_fraction,
                    };
                    let changed = last_state != Some(current);
                    if changed || joined || last_state_sent.elapsed() >= KEEPALIVE {
                        if changed {
                            counters.moves.fetch_add(1, Ordering::Relaxed);
                        }
                        send(&packetizer.state(current, timestamp));
                        counters.state_packets.fetch_add(1, Ordering::Relaxed);
                        last_state = Some(current);
                        last_state_sent = Instant::now();
                    }
                }
                thread::sleep(SAMPLE_INTERVAL.saturating_sub(tick.elapsed()));
            }
            info!("teacher pointer channel stopped");
        });
        Self {
            stop,
            endpoints,
            stats,
            worker: Some(worker),
        }
    }

    /// Replaces the receiver list; a newly added receiver gets the image and
    /// state immediately instead of waiting for the periodic repeat.
    pub fn set_endpoints(&self, mut list: Vec<SocketAddrV4>) {
        list.sort_unstable();
        list.dedup();
        let mut guard = self.endpoints.lock().unwrap();
        let joined = list.iter().any(|endpoint| !guard.0.contains(endpoint));
        guard.0 = list;
        guard.1 |= joined;
    }
}

impl Drop for PointerBroadcaster {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// The shared encode may omit the pointer only when every registered receiver
/// draws it from the pointer channel.
pub fn all_receivers_overlay<'a>(
    receivers: impl IntoIterator<Item = &'a u64>,
    overlay: &std::collections::HashSet<u64>,
) -> bool {
    let mut any = false;
    for id in receivers {
        if !overlay.contains(id) {
            return false;
        }
        any = true;
    }
    any
}

#[cfg(test)]
mod tests {
    use super::*;
    use media_core::PointerSample;
    use media_transport::{pointer::PointerReceiver, RtpPacket};
    use std::collections::HashSet;

    struct Fake(f32);
    impl PointerSource for Fake {
        fn sample(&mut self) -> Option<PointerSample> {
            self.0 = (self.0 + 0.01) % 1.0;
            Some(PointerSample {
                visible: true,
                x: self.0,
                y: 0.5,
                width_fraction: 0.01,
                image: Some(Arc::new(PointerImage::default_arrow(3))),
            })
        }
    }

    #[test]
    fn broadcaster_delivers_image_and_moving_state() {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver
            .set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        let std::net::SocketAddr::V4(target) = receiver.local_addr().unwrap() else {
            unreachable!()
        };
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        let broadcaster = PointerBroadcaster::start(sender, Box::new(Fake(0.0)), 9);
        broadcaster.set_endpoints(vec![target]);
        let mut rx = PointerReceiver::default();
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut buffer = [0; 1500];
        while Instant::now() < deadline && (rx.images_completed == 0 || rx.state_packets < 5) {
            if let Ok(n) = receiver.recv(&mut buffer) {
                rx.push(&RtpPacket::parse(&buffer[..n]).unwrap());
            }
        }
        assert_eq!(rx.images_completed, 1);
        assert!(rx.state_packets >= 5);
        assert!(rx.overlay().is_some());
        assert!(broadcaster.stats.moves.load(Ordering::Relaxed) >= 5);
    }

    #[test]
    fn composite_is_dropped_only_when_every_receiver_overlays() {
        let overlay: HashSet<u64> = [1, 2].into();
        assert!(all_receivers_overlay(&[1, 2], &overlay));
        assert!(!all_receivers_overlay(&[1, 2, 3], &overlay));
        assert!(!all_receivers_overlay(&[], &overlay));
    }
}
