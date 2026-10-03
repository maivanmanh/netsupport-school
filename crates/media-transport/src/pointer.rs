//! Teacher pointer channel on the media socket (RTP payload type 100).
//!
//! State datagrams are tiny and sent at pointer rate; the image is sent in
//! chunks when it changes and repeated periodically so late joiners and lost
//! datagrams recover without a request path. Receivers keep only the latest
//! state and the latest complete image (bounded).
use crate::RtpPacket;
use media_core::{PointerImage, PointerOverlay};
use std::sync::Arc;

pub const POINTER_PAYLOAD_TYPE: u8 = 100;
const KIND_STATE: u8 = 1;
const KIND_IMAGE: u8 = 2;
const IMAGE_HEADER: usize = 1 + 4 + 2 * 6;
pub const IMAGE_CHUNK_BYTES: usize = 1_100;

/// Latest pointer state as carried on the wire.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerState {
    pub visible: bool,
    pub serial: u32,
    pub x: f32,
    pub y: f32,
    pub width_fraction: f32,
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn unit(value: f32) -> [u8; 2] {
    ((value.clamp(0.0, 1.0) * 65_535.0).round() as u16).to_be_bytes()
}
fn from_unit(bytes: [u8; 2]) -> f32 {
    f32::from(u16::from_be_bytes(bytes)) / 65_535.0
}

/// Builds pointer datagrams with their own RTP sequence space.
#[derive(Debug)]
pub struct PointerPacketizer {
    ssrc: u32,
    sequence: u16,
}
impl PointerPacketizer {
    #[must_use]
    pub const fn new(ssrc: u32) -> Self {
        Self { ssrc, sequence: 0 }
    }
    fn packet(&mut self, payload: Vec<u8>, timestamp_ms: u32) -> Vec<u8> {
        let packet = RtpPacket {
            marker: true,
            payload_type: POINTER_PAYLOAD_TYPE,
            sequence_number: self.sequence,
            timestamp: timestamp_ms,
            ssrc: self.ssrc,
            payload,
            timing: None,
        };
        self.sequence = self.sequence.wrapping_add(1);
        packet.to_bytes()
    }
    pub fn state(&mut self, state: PointerState, timestamp_ms: u32) -> Vec<u8> {
        let mut payload = Vec::with_capacity(12);
        payload.push(KIND_STATE);
        payload.push(u8::from(state.visible));
        payload.extend_from_slice(&state.serial.to_be_bytes());
        payload.extend_from_slice(&unit(state.x));
        payload.extend_from_slice(&unit(state.y));
        payload.extend_from_slice(&unit(state.width_fraction));
        self.packet(payload, timestamp_ms)
    }
    /// All chunks of one image (empty for an invalid image).
    pub fn image(&mut self, image: &PointerImage, timestamp_ms: u32) -> Vec<Vec<u8>> {
        if !image.is_valid() {
            return Vec::new();
        }
        let chunks: Vec<&[u8]> = image.bgra.chunks(IMAGE_CHUNK_BYTES).collect();
        let count = u16::try_from(chunks.len()).unwrap_or(u16::MAX);
        chunks
            .into_iter()
            .enumerate()
            .map(|(index, data)| {
                let mut payload = Vec::with_capacity(IMAGE_HEADER + data.len());
                payload.push(KIND_IMAGE);
                payload.extend_from_slice(&image.serial.to_be_bytes());
                for value in [
                    image.width,
                    image.height,
                    image.hotspot_x,
                    image.hotspot_y,
                    u16::try_from(index).unwrap_or(u16::MAX),
                    count,
                ] {
                    payload.extend_from_slice(&value.to_be_bytes());
                }
                payload.extend_from_slice(data);
                self.packet(payload, timestamp_ms)
            })
            .collect()
    }
}

#[derive(Debug)]
struct Partial {
    serial: u32,
    width: u16,
    height: u16,
    hotspot: (u16, u16),
    bytes: Vec<u8>,
    received: Vec<bool>,
}

/// Receiver-side reassembly; holds one partial and one complete image.
#[derive(Debug, Default)]
pub struct PointerReceiver {
    state: Option<PointerState>,
    image: Option<Arc<PointerImage>>,
    partial: Option<Partial>,
    changed: bool,
    last_sequence: Option<u16>,
    pub state_packets: u64,
    pub image_packets: u64,
    pub images_completed: u64,
    pub malformed: u64,
    /// Latest RTP timestamp (Teacher milliseconds clock) of a state packet.
    pub last_timestamp_ms: Option<u32>,
}
impl PointerReceiver {
    /// Accepts one pointer RTP packet; stale (reordered) state is ignored.
    pub fn push(&mut self, packet: &RtpPacket) {
        if packet.payload_type != POINTER_PAYLOAD_TYPE {
            return;
        }
        let payload = &packet.payload;
        match payload.first() {
            Some(&KIND_STATE) if payload.len() >= 12 => {
                if self.last_sequence.is_some_and(|last| {
                    let delta = packet.sequence_number.wrapping_sub(last);
                    delta == 0 || delta > 0x8000
                }) {
                    return;
                }
                self.last_sequence = Some(packet.sequence_number);
                self.state_packets += 1;
                self.last_timestamp_ms = Some(packet.timestamp);
                let state = PointerState {
                    visible: payload[1] != 0,
                    serial: u32::from_be_bytes([payload[2], payload[3], payload[4], payload[5]]),
                    x: from_unit([payload[6], payload[7]]),
                    y: from_unit([payload[8], payload[9]]),
                    width_fraction: from_unit([payload[10], payload[11]]),
                };
                if self.state != Some(state) {
                    self.state = Some(state);
                    self.changed = true;
                }
            }
            Some(&KIND_IMAGE) if payload.len() > IMAGE_HEADER => self.push_image(payload),
            _ => self.malformed += 1,
        }
    }
    fn push_image(&mut self, payload: &[u8]) {
        self.image_packets += 1;
        let serial = u32::from_be_bytes([payload[1], payload[2], payload[3], payload[4]]);
        if self
            .image
            .as_ref()
            .is_some_and(|image| image.serial == serial)
        {
            return; // periodic repeat of an image we already hold
        }
        let field = |i: usize| u16::from_be_bytes([payload[5 + 2 * i], payload[6 + 2 * i]]);
        let (width, height, hx, hy, index, count) =
            (field(0), field(1), field(2), field(3), field(4), field(5));
        let total = usize::from(width) * usize::from(height) * 4;
        let expected = total.div_ceil(IMAGE_CHUNK_BYTES);
        if width == 0
            || height == 0
            || width > PointerImage::MAX_SIDE
            || height > PointerImage::MAX_SIDE
            || usize::from(count) != expected
            || index >= count
        {
            self.malformed += 1;
            return;
        }
        if self.partial.as_ref().is_none_or(|p| p.serial != serial) {
            self.partial = Some(Partial {
                serial,
                width,
                height,
                hotspot: (hx, hy),
                bytes: vec![0; total],
                received: vec![false; expected],
            });
        }
        let Some(partial) = self.partial.as_mut() else {
            return;
        };
        let start = usize::from(index) * IMAGE_CHUNK_BYTES;
        let data = &payload[IMAGE_HEADER..];
        let end = (start + data.len()).min(total);
        if partial.width != width || partial.height != height || end - start != data.len() {
            self.malformed += 1;
            return;
        }
        partial.bytes[start..end].copy_from_slice(data);
        partial.received[usize::from(index)] = true;
        if partial.received.iter().all(|r| *r) {
            let done = self.partial.take().expect("partial present");
            let image = PointerImage {
                serial: done.serial,
                width: done.width,
                height: done.height,
                hotspot_x: done.hotspot.0.min(done.width - 1),
                hotspot_y: done.hotspot.1.min(done.height - 1),
                bgra: done.bytes,
            };
            if image.is_valid() {
                self.image = Some(Arc::new(image));
                self.images_completed += 1;
                self.changed = true;
            }
        }
    }
    /// What to draw now, when it changed since the last call.
    pub fn take_update(&mut self) -> Option<Option<PointerOverlay>> {
        if !std::mem::take(&mut self.changed) {
            return None;
        }
        Some(self.overlay())
    }
    #[must_use]
    pub fn overlay(&self) -> Option<PointerOverlay> {
        let state = self.state?;
        let image = self.image.as_ref().filter(|i| i.serial == state.serial)?;
        state.visible.then(|| PointerOverlay {
            x: state.x,
            y: state.y,
            width_fraction: state.width_fraction,
            image: Arc::clone(image),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn image(serial: u32, side: u16) -> PointerImage {
        PointerImage {
            serial,
            width: side,
            height: side,
            hotspot_x: 1,
            hotspot_y: 2,
            bgra: (0..usize::from(side) * usize::from(side) * 4)
                .map(|n| (n % 251) as u8)
                .collect(),
        }
    }
    #[test]
    fn state_and_chunked_image_round_trip_in_any_order() {
        let mut tx = PointerPacketizer::new(7);
        let mut rx = PointerReceiver::default();
        let img = image(5, 64);
        let mut chunks = tx.image(&img, 1);
        assert!(chunks.len() > 10);
        assert!(chunks
            .iter()
            .all(|c| c.len() <= 1_200 + crate::RTP_HEADER_BYTES));
        chunks.reverse();
        let state = PointerState {
            visible: true,
            serial: 5,
            x: 0.25,
            y: 0.75,
            width_fraction: 0.02,
        };
        rx.push(&RtpPacket::parse(&tx.state(state, 2)).unwrap());
        // State without its image: nothing drawable yet.
        assert_eq!(rx.take_update(), Some(None));
        for c in &chunks {
            rx.push(&RtpPacket::parse(c).unwrap());
        }
        let overlay = rx.take_update().unwrap().unwrap();
        assert_eq!(overlay.image.bgra, img.bgra);
        assert!((overlay.x - 0.25).abs() < 0.0001);
        assert_eq!(rx.images_completed, 1);
        // Repeats of a held image are cheap no-ops.
        for c in tx.image(&img, 3) {
            rx.push(&RtpPacket::parse(&c).unwrap());
        }
        assert_eq!(rx.take_update(), None);
    }
    #[test]
    fn reordered_older_state_is_ignored_and_hidden_pointer_draws_nothing() {
        let mut tx = PointerPacketizer::new(1);
        let mut rx = PointerReceiver::default();
        for c in tx.image(&image(1, 16), 0) {
            rx.push(&RtpPacket::parse(&c).unwrap());
        }
        let s = |x| PointerState {
            visible: true,
            serial: 1,
            x,
            y: 0.5,
            width_fraction: 0.01,
        };
        let old = tx.state(s(0.1), 1);
        let new = tx.state(s(0.9), 2);
        rx.push(&RtpPacket::parse(&new).unwrap());
        rx.push(&RtpPacket::parse(&old).unwrap());
        assert!((rx.overlay().unwrap().x - 0.9).abs() < 0.001);
        let hidden = tx.state(
            PointerState {
                visible: false,
                ..s(0.9)
            },
            3,
        );
        rx.push(&RtpPacket::parse(&hidden).unwrap());
        assert_eq!(rx.take_update(), Some(None));
    }
    #[test]
    fn malformed_image_chunks_are_rejected() {
        let mut rx = PointerReceiver::default();
        let mut tx = PointerPacketizer::new(1);
        let mut bad = tx.image(&image(2, 16), 0);
        let mut packet = RtpPacket::parse(&bad.remove(0)).unwrap();
        packet.payload[5] = 0xff; // absurd width
        rx.push(&packet);
        assert_eq!(rx.malformed, 1);
        assert!(rx.overlay().is_none());
    }
}
