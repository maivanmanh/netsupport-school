use std::{
    collections::{BTreeMap, VecDeque},
    time::{Duration, Instant},
};

use media_core::{EncodedAccessUnitRef, VideoCodec};

pub mod pointer;
use thiserror::Error;

pub const H264_PAYLOAD_TYPE: u8 = 96;
pub const RTP_HEADER_BYTES: usize = 12;
pub const MAX_RTP_PAYLOAD_BYTES: usize = 1_188;
pub const RTP_VIDEO_CLOCK_HZ: u32 = 90_000;
pub const MAX_NACK_SEQUENCES: usize = 32;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("RTP packet is too short")]
    PacketTooShort,
    #[error("unsupported RTP version {0}")]
    Version(u8),
    #[error("RTP packet has unsupported CSRC or extension fields")]
    UnsupportedHeader,
    #[error("RTP payload type {0} does not match the negotiated video codec")]
    PayloadType(u8),
    #[error("malformed video RTP payload: {0}")]
    MalformedH264(&'static str),
    #[error("RTP sequence gap: expected {expected}, received {actual}")]
    SequenceGap { expected: u16, actual: u16 },
}

#[derive(Clone, Debug)]
pub struct RtpPacket {
    pub marker: bool,
    pub payload_type: u8,
    pub sequence_number: u16,
    pub timestamp: u32,
    pub ssrc: u32,
    pub payload: Vec<u8>,
    pub timing: Option<media_core::FrameTiming>,
}

impl RtpPacket {
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(
            RTP_HEADER_BYTES + self.payload.len() + if self.timing.is_some() { 20 } else { 0 },
        );
        bytes.push(if self.timing.is_some() { 0x90 } else { 0x80 });
        bytes.push((u8::from(self.marker) << 7) | self.payload_type);
        bytes.extend_from_slice(&self.sequence_number.to_be_bytes());
        bytes.extend_from_slice(&self.timestamp.to_be_bytes());
        bytes.extend_from_slice(&self.ssrc.to_be_bytes());
        if let Some(timing) = self.timing {
            bytes.extend_from_slice(&[0x43, 0x4c, 0, 4]);
            bytes.extend_from_slice(&timing.capture_unix_us.to_be_bytes());
            bytes.extend_from_slice(&timing.send_unix_us.to_be_bytes());
        }
        bytes.extend_from_slice(&self.payload);
        bytes
    }

    /// Parses the fixed-header RTP subset emitted by this crate.
    ///
    /// # Errors
    /// Returns an error for malformed or unsupported RTP headers.
    pub fn parse(bytes: &[u8]) -> Result<Self, TransportError> {
        if bytes.len() < RTP_HEADER_BYTES {
            return Err(TransportError::PacketTooShort);
        }
        let version = bytes[0] >> 6;
        if version != 2 {
            return Err(TransportError::Version(version));
        }
        if bytes[0] & 0x0f != 0 || bytes[0] & 0x20 != 0 {
            return Err(TransportError::UnsupportedHeader);
        }
        let (timing, payload_offset) = if bytes[0] & 0x10 != 0 {
            if bytes.len() < 32 || bytes[12..16] != [0x43, 0x4c, 0, 4] {
                return Err(TransportError::UnsupportedHeader);
            }
            (
                Some(media_core::FrameTiming {
                    capture_unix_us: u64::from_be_bytes(
                        bytes[16..24]
                            .try_into()
                            .map_err(|_| TransportError::UnsupportedHeader)?,
                    ),
                    send_unix_us: u64::from_be_bytes(
                        bytes[24..32]
                            .try_into()
                            .map_err(|_| TransportError::UnsupportedHeader)?,
                    ),
                }),
                32,
            )
        } else {
            (None, RTP_HEADER_BYTES)
        };
        let payload_type = bytes[1] & 0x7f;
        Ok(Self {
            marker: bytes[1] & 0x80 != 0,
            payload_type,
            sequence_number: u16::from_be_bytes([bytes[2], bytes[3]]),
            timestamp: u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            ssrc: u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            payload: bytes[payload_offset..].to_vec(),
            timing,
        })
    }
}

/// RTP packetizer for H.264 (RFC 6184, single NAL + FU-A) and H.265
/// (RFC 7798, single NAL + FU). Packetization happens once per access unit.
#[derive(Debug)]
pub struct VideoPacketizer {
    next_sequence_number: u16,
    ssrc: u32,
    timing: Option<media_core::FrameTiming>,
    codec: VideoCodec,
}
/// Historical name; the packetizer is codec-aware.
pub type H264Packetizer = VideoPacketizer;

impl VideoPacketizer {
    #[must_use]
    pub const fn new(ssrc: u32) -> Self {
        Self::with_codec(ssrc, VideoCodec::H264)
    }
    #[must_use]
    pub const fn with_codec(ssrc: u32, codec: VideoCodec) -> Self {
        Self {
            next_sequence_number: 0,
            ssrc,
            timing: None,
            codec,
        }
    }

    pub fn packetize_timed(
        &mut self,
        access_unit: EncodedAccessUnitRef<'_>,
        timing: media_core::FrameTiming,
    ) -> Vec<Vec<u8>> {
        self.timing = Some(timing);
        let packets = self.packetize(access_unit);
        self.timing = None;
        packets
    }

    #[must_use]
    pub fn packetize(&mut self, access_unit: EncodedAccessUnitRef<'_>) -> Vec<Vec<u8>> {
        let timestamp = access_unit
            .timestamp
            .as_millis()
            .saturating_mul(u128::from(RTP_VIDEO_CLOCK_HZ))
            / 1_000;
        let timestamp = u32::try_from(timestamp & u128::from(u32::MAX)).unwrap_or_default();
        let codec = self.codec;
        let units = annex_b_units(access_unit.bytes)
            .into_iter()
            // RTP aggregation/fragmentation types are not valid elementary-
            // stream NAL units. Ignore malformed units rather than putting an
            // undecodable payload on the wire.
            .filter(|unit| codec.is_elementary(unit))
            .collect::<Vec<_>>();
        let packet_count = units
            .iter()
            .map(|unit| packet_count(codec, unit.len()))
            .sum::<usize>();
        let mut packet_index = 0_usize;
        let mut packets = Vec::with_capacity(packet_count);
        for unit in units {
            if unit.len() <= MAX_RTP_PAYLOAD_BYTES {
                packet_index += 1;
                packets.push(self.make_packet(unit, packet_index == packet_count, timestamp));
                continue;
            }
            // H.264 FU-A: indicator + FU header. H.265 FU: 2-byte payload
            // header (type 49, original F/LayerId/TID) + FU header.
            let header = codec.nal_header_len();
            let nal_kind = codec.nal_type(unit).unwrap_or_default();
            let prefix: Vec<u8> = match codec {
                VideoCodec::H264 => vec![(unit[0] & 0xe0) | 0x1c],
                VideoCodec::H265 => vec![(unit[0] & 0x81) | (49 << 1), unit[1]],
            };
            let fragment_payload_size = MAX_RTP_PAYLOAD_BYTES - prefix.len() - 1;
            let fragment_count = (unit.len() - header).div_ceil(fragment_payload_size);
            for (fragment_index, fragment) in
                unit[header..].chunks(fragment_payload_size).enumerate()
            {
                let start = fragment_index == 0;
                let end = fragment_index + 1 == fragment_count;
                let mut payload = Vec::with_capacity(fragment.len() + prefix.len() + 1);
                payload.extend_from_slice(&prefix);
                payload.push((u8::from(start) << 7) | (u8::from(end) << 6) | nal_kind);
                payload.extend_from_slice(fragment);
                packet_index += 1;
                packets.push(self.make_packet(&payload, packet_index == packet_count, timestamp));
            }
        }
        packets
    }

    fn make_packet(&mut self, payload: &[u8], marker: bool, timestamp: u32) -> Vec<u8> {
        let packet = RtpPacket {
            marker,
            payload_type: self.codec.rtp_payload_type(),
            sequence_number: self.next_sequence_number,
            timestamp,
            ssrc: self.ssrc,
            payload: payload.to_vec(),
            timing: self.timing,
        };
        self.next_sequence_number = self.next_sequence_number.wrapping_add(1);
        packet.to_bytes()
    }
}

fn packet_count(codec: VideoCodec, unit_len: usize) -> usize {
    let header = codec.nal_header_len();
    if unit_len <= MAX_RTP_PAYLOAD_BYTES {
        1
    } else {
        // Fragment prefix = payload header (NAL header length) + FU header.
        (unit_len - header).div_ceil(MAX_RTP_PAYLOAD_BYTES - header - 1)
    }
}

#[derive(Clone, Debug)]
pub struct DepacketizedAccessUnit {
    pub timing: Option<media_core::FrameTiming>,
    pub bytes: Vec<u8>,
    pub timestamp: u32,
}

#[derive(Debug, Default)]
pub struct VideoDepacketizer {
    access_unit: Vec<u8>,
    timestamp: Option<u32>,
    fragment: Option<Vec<u8>>,
    last_sequence: Option<u16>,
    dropping_timestamp: Option<u32>,
    codec: VideoCodec,
}
/// Historical name; the depacketizer is codec-aware.
pub type H264Depacketizer = VideoDepacketizer;

impl VideoDepacketizer {
    #[must_use]
    pub fn for_codec(codec: VideoCodec) -> Self {
        Self {
            codec,
            ..Self::default()
        }
    }
    #[must_use]
    pub const fn codec(&self) -> VideoCodec {
        self.codec
    }
    pub fn reset(&mut self) {
        self.access_unit.clear();
        self.timestamp = None;
        self.fragment = None;
        self.last_sequence = None;
        self.dropping_timestamp = None;
    }
    /// Accepts an ordered RTP packet; sheds only the broken AU on loss.
    /// # Errors
    /// Returns malformed H.264 or sequence-gap errors.
    pub fn push(
        &mut self,
        packet: &RtpPacket,
    ) -> Result<Option<DepacketizedAccessUnit>, TransportError> {
        if self.dropping_timestamp == Some(packet.timestamp) {
            self.last_sequence = Some(packet.sequence_number);
            if packet.marker {
                self.dropping_timestamp = None;
            }
            return Ok(None);
        }
        self.dropping_timestamp = None;
        if let Some(expected) = self.last_sequence.map(|sequence| sequence.wrapping_add(1)) {
            if packet.sequence_number != expected {
                self.discard_access_unit(packet);
                return Err(TransportError::SequenceGap {
                    expected,
                    actual: packet.sequence_number,
                });
            }
        }
        self.last_sequence = Some(packet.sequence_number);
        if self.access_unit.len() + self.fragment.as_ref().map_or(0, Vec::len) > 8 * 1024 * 1024 {
            self.discard_access_unit(packet);
            return Err(TransportError::MalformedH264(
                "access unit exceeds bounded size",
            ));
        }
        let result = self.push_payload(packet);
        if result.is_err() {
            self.discard_access_unit(packet);
        }
        result
    }
    fn discard_access_unit(&mut self, packet: &RtpPacket) {
        self.access_unit.clear();
        self.timestamp = None;
        self.fragment = None;
        self.last_sequence = Some(packet.sequence_number);
        self.dropping_timestamp = (!packet.marker).then_some(packet.timestamp);
    }
    fn push_payload(
        &mut self,
        packet: &RtpPacket,
    ) -> Result<Option<DepacketizedAccessUnit>, TransportError> {
        if packet.payload_type != self.codec.rtp_payload_type() {
            return Err(TransportError::PayloadType(packet.payload_type));
        }
        if self.timestamp.is_some_and(|time| time != packet.timestamp) {
            self.access_unit.clear();
            self.fragment = None;
        }
        self.timestamp = Some(packet.timestamp);
        let kind = self
            .codec
            .nal_type(&packet.payload)
            .ok_or(TransportError::MalformedH264("empty payload"))?;
        match (self.codec, kind) {
            (VideoCodec::H264, 1..=23) => append_annex_b(&mut self.access_unit, &packet.payload),
            (VideoCodec::H264, 28) => self.push_fu_a(&packet.payload)?,
            (VideoCodec::H265, 0..=47) if packet.payload.len() >= 2 => {
                append_annex_b(&mut self.access_unit, &packet.payload);
            }
            (VideoCodec::H265, 49) => self.push_hevc_fu(&packet.payload)?,
            _ => {
                return Err(TransportError::MalformedH264(
                    "unsupported packetization mode",
                ))
            }
        }
        if !packet.marker {
            return Ok(None);
        }
        if self.fragment.is_some() {
            return Err(TransportError::MalformedH264(
                "marker ended incomplete FU-A",
            ));
        }
        let timestamp = self.timestamp.take().unwrap_or(packet.timestamp);
        Ok(Some(DepacketizedAccessUnit {
            bytes: std::mem::take(&mut self.access_unit),
            timestamp,
            timing: packet.timing,
        }))
    }
    /// RFC 7798 FU: 2-byte payload header, FU header (S, E, type), data.
    fn push_hevc_fu(&mut self, payload: &[u8]) -> Result<(), TransportError> {
        if payload.len() < 4 {
            return Err(TransportError::MalformedH264("short H.265 FU payload"));
        }
        let start = payload[2] & 0x80 != 0;
        let end = payload[2] & 0x40 != 0;
        if start {
            if self.fragment.is_some() {
                return Err(TransportError::MalformedH264("nested H.265 FU start"));
            }
            let kind = payload[2] & 0x3f;
            self.fragment = Some(vec![(payload[0] & 0x81) | (kind << 1), payload[1]]);
        }
        let fragment = self.fragment.as_mut().ok_or(TransportError::MalformedH264(
            "H.265 FU continuation without start",
        ))?;
        fragment.extend_from_slice(&payload[3..]);
        if end {
            let unit = self
                .fragment
                .take()
                .ok_or(TransportError::MalformedH264("H.265 FU end without start"))?;
            append_annex_b(&mut self.access_unit, &unit);
        }
        Ok(())
    }
    fn push_fu_a(&mut self, payload: &[u8]) -> Result<(), TransportError> {
        if payload.len() < 3 {
            return Err(TransportError::MalformedH264("short FU-A payload"));
        }
        let start = payload[1] & 0x80 != 0;
        let end = payload[1] & 0x40 != 0;
        if start {
            if self.fragment.is_some() {
                return Err(TransportError::MalformedH264("nested FU-A start"));
            }
            self.fragment = Some(vec![(payload[0] & 0xe0) | (payload[1] & 0x1f)]);
        }
        let fragment = self.fragment.as_mut().ok_or(TransportError::MalformedH264(
            "FU-A continuation without start",
        ))?;
        fragment.extend_from_slice(&payload[2..]);
        if end {
            let unit = self
                .fragment
                .take()
                .ok_or(TransportError::MalformedH264("FU-A end without start"))?;
            append_annex_b(&mut self.access_unit, &unit);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct JitterStatistics {
    pub late_or_missing_packets: u64,
    pub reordered_packets: u64,
    pub late_packets: u64,
    pub missing_sequences: u64,
    pub duplicate_packets: u64,
    pub overflow_packets: u64,
    pub repaired_gaps: u64,
    pub fast_released_frames: u64,
    pub nack_retries: u64,
}

pub struct JitterBuffer {
    target_delay: Duration,
    repair_budget: Duration,
    emitted: bool,
    fast_release_until: Option<u16>,
    next_sequence: Option<u16>,
    packets: BTreeMap<u16, (Instant, RtpPacket)>,
    missing_sequences: VecDeque<u16>,
    gap_started: Option<Instant>,
    /// One more NACK for a gap still open halfway through the repair budget
    /// (the first retransmission itself can be lost).
    gap_retried: bool,
    statistics: JitterStatistics,
    diagnostics: media_core::diagnostics::StageMetrics,
}
impl JitterBuffer {
    #[must_use]
    pub fn new(target_delay: Duration) -> Self {
        Self::with_repair_budget(target_delay, target_delay)
    }
    #[must_use]
    pub fn with_repair_budget(target_delay: Duration, repair_budget: Duration) -> Self {
        Self {
            target_delay,
            repair_budget,
            emitted: false,
            fast_release_until: None,
            next_sequence: None,
            packets: BTreeMap::new(),
            missing_sequences: VecDeque::new(),
            gap_started: None,
            gap_retried: false,
            statistics: JitterStatistics::default(),
            diagnostics: media_core::diagnostics::StageMetrics::default(),
        }
    }
    pub fn push(&mut self, packet: RtpPacket, received_at: Instant) {
        if let Some(expected) = self.next_sequence {
            if self.emitted && sequence_is_before(packet.sequence_number, expected) {
                self.statistics.late_or_missing_packets += 1;
                self.statistics.late_packets += 1;
                return;
            }
            if packet.sequence_number != expected {
                self.statistics.reordered_packets += 1;
            }
        }
        if self.packets.contains_key(&packet.sequence_number) {
            self.statistics.duplicate_packets += 1;
            return;
        }
        if self.packets.len() == 2048 {
            self.statistics.overflow_packets += 1;
            return;
        }
        // Only accepted startup packets can move the initial sequence backwards,
        // including reordering across 65535 -> 0. Rejected overflow cannot do so.
        if !self.emitted
            && self
                .next_sequence
                .is_some_and(|expected| sequence_is_before(packet.sequence_number, expected))
        {
            self.next_sequence = Some(packet.sequence_number);
        }
        self.next_sequence.get_or_insert(packet.sequence_number);
        self.packets
            .insert(packet.sequence_number, (received_at, packet));
    }
    #[must_use]
    pub fn pop_ready(&mut self, now: Instant) -> Option<RtpPacket> {
        let expected = self.next_sequence?;
        if let Some((received_at, _)) = self.packets.get(&expected) {
            if self.emitted && self.fast_release_until.is_none() {
                self.fast_release_until = self.complete_marker(expected);
                if self.fast_release_until.is_some() {
                    self.statistics.fast_released_frames += 1;
                }
            }
            if self.fast_release_until.is_none()
                && now.saturating_duration_since(*received_at) < self.target_delay
            {
                return None;
            }
            if let Some((arrived, packet)) = self.packets.remove(&expected) {
                self.diagnostics
                    .record("jitter_residence", now.saturating_duration_since(arrived));
                self.next_sequence = Some(expected.wrapping_add(1));
                self.emitted = true;
                if self.fast_release_until == Some(expected) {
                    self.fast_release_until = None;
                }
                if let Some(started) = self.gap_started.take() {
                    self.statistics.repaired_gaps += 1;
                    self.diagnostics
                        .record("nack_repair_wait", now.saturating_duration_since(started));
                }
                return Some(packet);
            }
            return None;
        }
        let (next_available, received_at) = self
            .packets
            .iter()
            .min_by_key(|(sequence, _)| sequence.wrapping_sub(expected))
            .map(|(&sequence, (received_at, _))| (sequence, *received_at))?;
        if self.gap_started.is_none() {
            self.queue_nack(expected, next_available);
            self.gap_started = Some(now);
            self.gap_retried = false;
            return None;
        }
        if !self.gap_retried
            && now.saturating_duration_since(self.gap_started?) >= self.repair_budget / 2
        {
            self.gap_retried = true;
            self.statistics.nack_retries += 1;
            self.queue_nack(expected, next_available);
        }
        if now.saturating_duration_since(self.gap_started?) < self.repair_budget
            || now.saturating_duration_since(received_at) < self.target_delay
        {
            return None;
        }
        let missing = next_available.wrapping_sub(expected);
        self.statistics.late_or_missing_packets += u64::from(missing);
        self.statistics.missing_sequences += u64::from(missing);
        self.next_sequence = Some(next_available);
        self.fast_release_until = None;
        self.gap_started = None;
        self.pop_ready(now)
    }
    /// Adjusts how long a sequence gap waits for NACK repair (e.g. from RTT).
    pub fn set_repair_budget(&mut self, budget: Duration) {
        self.repair_budget = budget;
    }
    #[must_use]
    pub const fn repair_budget(&self) -> Duration {
        self.repair_budget
    }
    #[must_use]
    pub const fn statistics(&self) -> JitterStatistics {
        self.statistics
    }

    /// Returns missing RTP sequence numbers as soon as a gap is observed.
    #[must_use]
    pub fn take_missing_sequences(&mut self) -> Vec<u16> {
        self.missing_sequences.drain(..).collect()
    }

    /// Discards buffered compressed data only as part of an IDR recovery.
    pub fn discard_backlog(&mut self) -> usize {
        let count = self.packets.len();
        self.packets.clear();
        self.next_sequence = None;
        self.emitted = false;
        self.fast_release_until = None;
        self.gap_started = None;
        self.gap_retried = false;
        self.missing_sequences.clear();
        count
    }
    #[must_use]
    pub fn oldest_age(&self, now: Instant) -> Duration {
        self.packets
            .values()
            .map(|(at, _)| now.saturating_duration_since(*at))
            .max()
            .unwrap_or_default()
    }
    #[must_use]
    pub fn queued_packets(&self) -> usize {
        self.packets.len()
    }
    pub fn take_diagnostics(&mut self) -> media_core::diagnostics::StageMetrics {
        std::mem::take(&mut self.diagnostics)
    }

    // Only release a contiguous, complete AU early. Incomplete data keeps the
    // configured reorder budget; sequence loss never bypasses reference recovery.
    fn complete_marker(&self, expected: u16) -> Option<u16> {
        let timestamp = self.packets.get(&expected)?.1.timestamp;
        for offset in 0..self.packets.len() {
            let seq = expected.wrapping_add(u16::try_from(offset).ok()?);
            let packet = &self.packets.get(&seq)?.1;
            if packet.timestamp != timestamp {
                return None;
            }
            if packet.marker {
                return Some(seq);
            }
        }
        None
    }
    fn queue_nack(&mut self, expected: u16, next_available: u16) {
        let missing = next_available.wrapping_sub(expected);
        if usize::from(missing) <= MAX_NACK_SEQUENCES {
            for offset in 0..missing {
                if self.missing_sequences.len() == MAX_NACK_SEQUENCES {
                    break;
                }
                self.missing_sequences
                    .push_back(expected.wrapping_add(offset));
            }
        }
    }
}

fn sequence_is_before(sequence: u16, expected: u16) -> bool {
    sequence != expected && sequence.wrapping_sub(expected) > u16::MAX / 2
}

fn append_annex_b(output: &mut Vec<u8>, unit: &[u8]) {
    output.extend_from_slice(&[0, 0, 0, 1]);
    output.extend_from_slice(unit);
}
fn annex_b_units(bytes: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut index = 0;
    while index + 3 <= bytes.len() {
        let length = if index + 4 <= bytes.len() && bytes[index..index + 4] == [0, 0, 0, 1] {
            4
        } else if bytes[index..index + 3] == [0, 0, 1] {
            3
        } else {
            index += 1;
            continue;
        };
        starts.push((index, index + length));
        index += length;
    }
    starts
        .iter()
        .enumerate()
        .filter_map(|(position, (_, start))| {
            let end = starts
                .get(position + 1)
                .map_or(bytes.len(), |(code_start, _)| *code_start);
            (*start < end).then_some(&bytes[*start..end])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    fn test_packet(seq: u16, timestamp: u32, marker: bool) -> RtpPacket {
        RtpPacket {
            timing: None,
            marker,
            payload_type: H264_PAYLOAD_TYPE,
            sequence_number: seq,
            timestamp,
            ssrc: 1,
            payload: vec![0x61],
        }
    }
    #[test]
    fn rejected_overflow_packet_cannot_change_startup_sequence() {
        let start = Instant::now();
        let mut jitter = JitterBuffer::new(Duration::from_millis(20));
        for seq in 0..2048 {
            jitter.push(test_packet(seq, u32::from(seq), true), start);
        }
        jitter.push(test_packet(65535, 0, true), start);
        assert_eq!(jitter.statistics().overflow_packets, 1);
        assert_eq!(
            jitter
                .pop_ready(start + Duration::from_millis(20))
                .unwrap()
                .sequence_number,
            0
        );
        assert!(jitter.take_missing_sequences().is_empty());
    }
    #[test]
    fn fragmented_reference_frame_survives_repair_without_partial_decode() {
        let start = Instant::now();
        let mut jitter =
            JitterBuffer::with_repair_budget(Duration::from_millis(20), Duration::from_millis(25));
        // Prime the preceding sequence, then drop the middle of a fragmented IDR.
        jitter.push(test_packet(65535, 0, true), start);
        let now = start + Duration::from_millis(20);
        assert!(jitter.pop_ready(now).is_some());
        let mut bytes = vec![0, 0, 0, 1, 0x65];
        bytes.extend(vec![77; MAX_RTP_PAYLOAD_BYTES * 3]);
        let packets = H264Packetizer::new(1).packetize(EncodedAccessUnitRef {
            bytes: &bytes,
            timestamp: Duration::from_millis(33),
            is_keyframe: true,
        });
        for (index, wire) in packets.iter().enumerate() {
            if index != 1 {
                jitter.push(RtpPacket::parse(wire).unwrap(), now);
            }
        }
        let mut depacketizer = H264Depacketizer::default();
        // The incomplete FU-A waits for reordering; no partial AU reaches decode.
        assert!(jitter.pop_ready(now).is_none());
        let held = now + Duration::from_millis(20);
        assert!(depacketizer
            .push(&jitter.pop_ready(held).unwrap())
            .unwrap()
            .is_none());
        assert!(jitter.pop_ready(held).is_none());
        assert_eq!(jitter.take_missing_sequences(), vec![1]);
        let repaired = held + Duration::from_millis(3);
        jitter.push(RtpPacket::parse(&packets[1]).unwrap(), repaired);
        let mut output = None;
        while let Some(packet) = jitter.pop_ready(repaired) {
            output = depacketizer.push(&packet).unwrap().or(output);
        }
        assert_eq!(output.unwrap().bytes, bytes);
        assert_eq!(jitter.statistics().missing_sequences, 0);
        assert_eq!(jitter.statistics().repaired_gaps, 1);
    }
    #[test]
    fn complete_frame_releases_early_but_incomplete_frame_keeps_reorder_budget() {
        let start = Instant::now();
        let mut jitter =
            JitterBuffer::with_repair_budget(Duration::from_millis(20), Duration::from_millis(25));
        jitter.push(test_packet(1, 0, true), start);
        assert!(jitter.pop_ready(start).is_none());
        assert!(jitter
            .pop_ready(start + Duration::from_millis(20))
            .is_some());
        let next = start + Duration::from_millis(33);
        jitter.push(test_packet(2, 1, false), next);
        assert!(jitter.pop_ready(next).is_none());
        jitter.push(test_packet(3, 1, true), next + Duration::from_millis(1));
        assert_eq!(
            jitter
                .pop_ready(next + Duration::from_millis(1))
                .unwrap()
                .sequence_number,
            2
        );
        assert_eq!(
            jitter
                .pop_ready(next + Duration::from_millis(1))
                .unwrap()
                .sequence_number,
            3
        );
        assert_eq!(jitter.statistics().fast_released_frames, 1);
    }
    #[test]
    fn nack_is_immediate_and_repaired_frame_does_not_wait_another_jitter_period() {
        let start = Instant::now();
        let mut jitter =
            JitterBuffer::with_repair_budget(Duration::from_millis(20), Duration::from_millis(25));
        jitter.push(test_packet(10, 0, true), start);
        let now = start + Duration::from_millis(20);
        assert!(jitter.pop_ready(now).is_some());
        jitter.push(test_packet(12, 1, true), now);
        assert!(jitter.pop_ready(now).is_none());
        assert_eq!(jitter.take_missing_sequences(), vec![11]);
        jitter.push(test_packet(11, 1, false), now + Duration::from_millis(3));
        assert_eq!(
            jitter
                .pop_ready(now + Duration::from_millis(3))
                .unwrap()
                .sequence_number,
            11
        );
        assert_eq!(
            jitter
                .pop_ready(now + Duration::from_millis(3))
                .unwrap()
                .sequence_number,
            12
        );
        assert_eq!(jitter.statistics().repaired_gaps, 1);
        assert_eq!(jitter.statistics().missing_sequences, 0);
    }
    #[test]
    fn missing_reference_expires_at_repair_deadline_and_late_repair_is_rejected() {
        let start = Instant::now();
        let mut jitter =
            JitterBuffer::with_repair_budget(Duration::from_millis(20), Duration::from_millis(25));
        jitter.push(test_packet(10, 0, true), start);
        let now = start + Duration::from_millis(20);
        assert!(jitter.pop_ready(now).is_some());
        jitter.push(test_packet(12, 1, true), now);
        assert!(jitter.pop_ready(now).is_none());
        assert!(jitter.pop_ready(now + Duration::from_millis(24)).is_none());
        assert_eq!(
            jitter
                .pop_ready(now + Duration::from_millis(25))
                .unwrap()
                .sequence_number,
            12
        );
        jitter.push(test_packet(11, 1, false), now + Duration::from_millis(26));
        assert_eq!(jitter.statistics().missing_sequences, 1);
        assert_eq!(jitter.statistics().late_packets, 1);
    }
    #[test]
    fn startup_reordering_across_sequence_wrap_preserves_fragment_order() {
        let now = Instant::now();
        for sequences in [[65535, 0], [0, 65535]] {
            let mut jitter = JitterBuffer::new(Duration::from_millis(20));
            for seq in sequences {
                jitter.push(test_packet(seq, 1, seq == 0), now);
            }
            let ready = now + Duration::from_millis(20);
            assert_eq!(jitter.pop_ready(ready).unwrap().sequence_number, 65535);
            assert_eq!(jitter.pop_ready(ready).unwrap().sequence_number, 0);
            assert_eq!(jitter.statistics().missing_sequences, 0);
        }
    }
    #[test]
    fn timed_fragmentation_preserves_timing_and_mtu_and_wraps_timestamp() {
        let timing = media_core::FrameTiming {
            capture_unix_us: 123,
            send_unix_us: 456,
        };
        let mut bytes = vec![0, 0, 0, 1, 0x65];
        bytes.extend(vec![77; MAX_RTP_PAYLOAD_BYTES * 3]);
        let mut packetizer = H264Packetizer::new(1);
        let packets = packetizer.packetize_timed(
            EncodedAccessUnitRef {
                bytes: &bytes,
                timestamp: Duration::from_secs(48_000),
                is_keyframe: true,
            },
            timing,
        );
        let mut depacketizer = H264Depacketizer::default();
        let mut output = None;
        for wire in packets {
            assert!(wire.len() <= MAX_RTP_PAYLOAD_BYTES + RTP_HEADER_BYTES + 20);
            let packet = RtpPacket::parse(&wire).unwrap();
            assert_eq!(packet.timing, Some(timing));
            assert_eq!(packet.timestamp, 25_032_704);
            let mut malformed = wire.clone();
            malformed[15] = 3;
            assert!(RtpPacket::parse(&malformed).is_err());
            output = depacketizer.push(&packet).unwrap().or(output);
        }
        let output = output.unwrap();
        assert_eq!(output.bytes, bytes);
        assert_eq!(output.timing, Some(timing));
    }
    #[test]
    fn jitter_gap_and_late_counters_survive_sequence_wrap_and_reset() {
        let now = Instant::now();
        let mut jitter = JitterBuffer::new(Duration::ZERO);
        let packet = |sequence_number| RtpPacket {
            timing: None,
            marker: true,
            payload_type: H264_PAYLOAD_TYPE,
            sequence_number,
            timestamp: 0,
            ssrc: 1,
            payload: vec![0x65],
        };
        jitter.push(packet(65534), now);
        assert_eq!(jitter.pop_ready(now).unwrap().sequence_number, 65534);
        jitter.push(packet(0), now);
        jitter.push(packet(1), now);
        assert!(jitter.pop_ready(now).is_none());
        assert_eq!(jitter.pop_ready(now).unwrap().sequence_number, 0);
        assert_eq!(jitter.statistics().missing_sequences, 1);
        jitter.push(packet(65535), now);
        assert_eq!(jitter.statistics().late_packets, 1);
        jitter.push(packet(1), now);
        assert_eq!(jitter.statistics().duplicate_packets, 1);
        assert_eq!(jitter.discard_backlog(), 1);
        assert_eq!(jitter.queued_packets(), 0);
        jitter.push(packet(300), now);
        assert_eq!(jitter.pop_ready(now).unwrap().sequence_number, 300);
    }

    use super::*;
    #[test]
    fn fragmented_idr_round_trips() {
        let mut unit = vec![0, 0, 0, 1, 0x65];
        unit.extend(std::iter::repeat_n(0x55, MAX_RTP_PAYLOAD_BYTES + 30));
        let mut packetizer = H264Packetizer::new(1);
        let packets = packetizer.packetize(EncodedAccessUnitRef {
            bytes: &unit,
            timestamp: Duration::from_millis(7),
            is_keyframe: true,
        });
        assert!(packets.len() > 1);
        let mut depacketizer = H264Depacketizer::default();
        let mut output = None;
        for packet in packets {
            output = depacketizer
                .push(&RtpPacket::parse(&packet).unwrap())
                .unwrap()
                .or(output);
        }
        assert_eq!(output.unwrap().bytes, unit);
    }

    #[test]
    fn hevc_parameter_sets_and_fragmented_irap_round_trip() {
        // VPS(32) SPS(33) PPS(34) then a large IDR_W_RADL (19) slice.
        let mut unit = vec![0, 0, 0, 1, 0x40, 0x01, 0x0c];
        unit.extend_from_slice(&[0, 0, 0, 1, 0x42, 0x01, 0x01, 0x02]);
        unit.extend_from_slice(&[0, 0, 0, 1, 0x44, 0x01, 0xc1]);
        unit.extend_from_slice(&[0, 0, 0, 1, 0x26, 0x01]);
        unit.extend(std::iter::repeat_n(0x5a, 3 * MAX_RTP_PAYLOAD_BYTES + 7));
        let mut packetizer = VideoPacketizer::with_codec(9, VideoCodec::H265);
        let packets = packetizer.packetize(EncodedAccessUnitRef {
            bytes: &unit,
            timestamp: Duration::from_millis(33),
            is_keyframe: true,
        });
        assert!(packets.len() > 4);
        let mut depacketizer = VideoDepacketizer::for_codec(VideoCodec::H265);
        let mut output = None;
        for packet in &packets {
            let parsed = RtpPacket::parse(packet).unwrap();
            assert!(parsed.payload.len() <= MAX_RTP_PAYLOAD_BYTES);
            assert_eq!(parsed.payload_type, 98);
            output = depacketizer.push(&parsed).unwrap().or(output);
        }
        assert_eq!(output.unwrap().bytes, unit);
        // An H.264 receiver rejects the H.265 payload type instead of misparsing.
        let mut h264 = VideoDepacketizer::default();
        assert!(h264.push(&RtpPacket::parse(&packets[0]).unwrap()).is_err());
        assert!(VideoCodec::H265.is_random_access(&[0x26, 0x01]));
        assert!(!VideoCodec::H265.is_random_access(&[0x02, 0x01]));
    }

    #[test]
    fn missing_fragment_discards_only_its_access_unit_without_error_storm() {
        let mut unit = vec![0, 0, 0, 1, 0x65];
        unit.extend(std::iter::repeat_n(0x55, MAX_RTP_PAYLOAD_BYTES * 4));
        let mut packetizer = H264Packetizer::new(1);
        let broken_packets = packetizer.packetize(EncodedAccessUnitRef {
            bytes: &unit,
            timestamp: Duration::from_millis(7),
            is_keyframe: true,
        });
        let next_packets = packetizer.packetize(EncodedAccessUnitRef {
            bytes: &[0, 0, 0, 1, 0x65, 0xaa],
            timestamp: Duration::from_millis(40),
            is_keyframe: true,
        });
        let mut depacketizer = H264Depacketizer::default();
        let mut errors = 0;
        for (index, bytes) in broken_packets.iter().enumerate() {
            if index == 1 {
                continue;
            }
            match depacketizer.push(&RtpPacket::parse(bytes).unwrap()) {
                Ok(None) => {}
                Ok(Some(_)) => panic!("broken access unit was emitted"),
                Err(TransportError::SequenceGap { .. }) => errors += 1,
                Err(error) => panic!("unexpected transport error: {error}"),
            }
        }
        assert_eq!(errors, 1);
        let recovered = depacketizer
            .push(&RtpPacket::parse(&next_packets[0]).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(recovered.bytes, [0, 0, 0, 1, 0x65, 0xaa]);
    }

    #[test]
    fn missing_fu_a_start_reports_once_then_accepts_next_access_unit() {
        let mut unit = vec![0, 0, 0, 1, 0x65];
        unit.extend(std::iter::repeat_n(0x55, MAX_RTP_PAYLOAD_BYTES * 3));
        let mut packetizer = H264Packetizer::new(1);
        let fragmented = packetizer.packetize(EncodedAccessUnitRef {
            bytes: &unit,
            timestamp: Duration::from_millis(7),
            is_keyframe: true,
        });
        let next = packetizer.packetize(EncodedAccessUnitRef {
            bytes: &[0, 0, 0, 1, 0x65, 0xaa],
            timestamp: Duration::from_millis(40),
            is_keyframe: true,
        });
        let mut depacketizer = H264Depacketizer::default();
        let mut errors = 0;
        for bytes in fragmented.iter().skip(1) {
            match depacketizer.push(&RtpPacket::parse(bytes).unwrap()) {
                Ok(None) => {}
                Ok(Some(_)) => panic!("fragmented access unit missing its start was emitted"),
                Err(TransportError::MalformedH264("FU-A continuation without start")) => {
                    errors += 1;
                }
                Err(error) => panic!("unexpected transport error: {error}"),
            }
        }
        assert_eq!(errors, 1);
        assert!(depacketizer
            .push(&RtpPacket::parse(&next[0]).unwrap())
            .unwrap()
            .is_some());
    }
    #[test]
    fn unrepaired_gap_gets_one_second_nack_halfway_through_repair_budget() {
        let start = Instant::now();
        let mut jitter =
            JitterBuffer::with_repair_budget(Duration::from_millis(20), Duration::from_millis(40));
        jitter.push(test_packet(1, 1, true), start);
        assert!(jitter
            .pop_ready(start + Duration::from_millis(21))
            .is_some());
        jitter.push(test_packet(3, 2, true), start + Duration::from_millis(22));
        assert!(jitter
            .pop_ready(start + Duration::from_millis(22))
            .is_none());
        assert_eq!(jitter.take_missing_sequences(), vec![2]);
        assert!(jitter
            .pop_ready(start + Duration::from_millis(30))
            .is_none());
        assert!(
            jitter.take_missing_sequences().is_empty(),
            "too early to retry"
        );
        assert!(jitter
            .pop_ready(start + Duration::from_millis(43))
            .is_none());
        assert_eq!(jitter.take_missing_sequences(), vec![2], "second chance");
        assert!(jitter
            .pop_ready(start + Duration::from_millis(50))
            .is_none());
        assert!(jitter.take_missing_sequences().is_empty(), "only one retry");
        assert_eq!(jitter.statistics().nack_retries, 1);
    }

    #[test]
    fn jitter_buffer_holds_packet_for_budget() {
        let now = Instant::now();
        let mut jitter = JitterBuffer::new(Duration::from_millis(40));
        jitter.push(
            RtpPacket {
                timing: None,
                marker: true,
                payload_type: H264_PAYLOAD_TYPE,
                sequence_number: 2,
                timestamp: 1,
                ssrc: 1,
                payload: vec![0x61],
            },
            now,
        );
        assert!(jitter.pop_ready(now).is_none());
        assert_eq!(
            jitter
                .pop_ready(now + Duration::from_millis(40))
                .unwrap()
                .sequence_number,
            2
        );
    }

    #[test]
    fn jitter_buffer_reports_a_small_gap_for_nack() {
        let now = Instant::now();
        let mut jitter = JitterBuffer::new(Duration::from_millis(40));
        for sequence_number in [10, 12] {
            jitter.push(
                RtpPacket {
                    timing: None,
                    marker: sequence_number == 12,
                    payload_type: H264_PAYLOAD_TYPE,
                    sequence_number,
                    timestamp: 1,
                    ssrc: 1,
                    payload: vec![0x61],
                },
                now,
            );
        }
        assert_eq!(
            jitter
                .pop_ready(now + Duration::from_millis(40))
                .unwrap()
                .sequence_number,
            10
        );
        assert!(jitter.pop_ready(now + Duration::from_millis(40)).is_none());
        assert_eq!(jitter.take_missing_sequences(), vec![11]);
        assert_eq!(
            jitter
                .pop_ready(now + Duration::from_millis(80))
                .unwrap()
                .sequence_number,
            12
        );
    }

    #[test]
    fn jitter_buffer_discards_a_retransmission_that_arrived_too_late() {
        let now = Instant::now();
        let mut jitter = JitterBuffer::new(Duration::from_millis(1));
        for sequence_number in [10, 12] {
            jitter.push(
                RtpPacket {
                    timing: None,
                    marker: true,
                    payload_type: H264_PAYLOAD_TYPE,
                    sequence_number,
                    timestamp: 1,
                    ssrc: 1,
                    payload: vec![0x61],
                },
                now,
            );
        }
        let _ = jitter.pop_ready(now + Duration::from_millis(1));
        let _ = jitter.pop_ready(now + Duration::from_millis(2));
        let _ = jitter.pop_ready(now + Duration::from_millis(3));
        jitter.push(
            RtpPacket {
                timing: None,
                marker: true,
                payload_type: H264_PAYLOAD_TYPE,
                sequence_number: 11,
                timestamp: 1,
                ssrc: 1,
                payload: vec![0x61],
            },
            now + Duration::from_millis(3),
        );
        assert_eq!(jitter.statistics().late_or_missing_packets, 2);
    }
}
