//! Receiver liveness: codec acceptance alone is not decoded output. No queue.
use media_core::VideoCodec;
use std::time::{Duration, Instant};

const SILENT_LIMIT: Duration = Duration::from_millis(500);
const MIN_SILENT_SUBMISSIONS: u32 = 3;

#[derive(Default)]
pub struct DecodeHealth {
    silent_since: Option<Instant>,
    silent_submissions: u32,
}
impl DecodeHealth {
    /// Allow normal delayed output, but recover when accepted compressed AUs
    /// keep producing no frames. Idle/no-input time alone never triggers this.
    pub fn observe(&mut self, produced_frame: bool, now: Instant) -> bool {
        if produced_frame {
            self.silent_since = None;
            self.silent_submissions = 0;
            return false;
        }
        let since = *self.silent_since.get_or_insert(now);
        self.silent_submissions = self.silent_submissions.saturating_add(1);
        if self.silent_submissions >= MIN_SILENT_SUBMISSIONS
            && now.saturating_duration_since(since) >= SILENT_LIMIT
        {
            self.silent_since = None;
            self.silent_submissions = 0;
            true
        } else {
            false
        }
    }
}

/// Recovery must carry the parameter sets our encoders prepend to each IDR.
/// A lost leading SPS/PPS can evade sequence-gap detection at a new anchor.
/// Presence is necessary, not a proof of valid slice data; output confirms it.
pub fn complete_recovery_unit(bytes: &[u8], codec: VideoCodec) -> bool {
    let mut sets = 0_u8;
    let mut random_access = false;
    for (index, prefix) in bytes.windows(3).enumerate() {
        if prefix != [0, 0, 1] {
            continue;
        }
        let unit = &bytes[index + 3..];
        random_access |= codec.is_random_access(unit);
        sets |= match (codec, codec.nal_type(unit)) {
            (VideoCodec::H264, Some(7)) | (VideoCodec::H265, Some(33)) => 1,
            (VideoCodec::H264, Some(8)) | (VideoCodec::H265, Some(34)) => 2,
            (VideoCodec::H265, Some(32)) => 4,
            _ => 0,
        };
    }
    random_access && sets == if codec == VideoCodec::H264 { 3 } else { 7 }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lost_first_sps_can_form_an_idr_au_without_a_sequence_error() {
        use media_core::EncodedAccessUnitRef;
        use media_transport::{H264Depacketizer, H264Packetizer, RtpPacket};
        let bytes = [0, 0, 1, 0x67, 1, 0, 0, 1, 0x68, 2, 0, 0, 1, 0x65, 3];
        let packets = H264Packetizer::new(1).packetize(EncodedAccessUnitRef {
            bytes: &bytes,
            timestamp: Duration::ZERO,
            is_keyframe: true,
        });
        let mut receiver = H264Depacketizer::default();
        let mut unit = None;
        for bytes in packets.iter().skip(1) {
            unit = receiver
                .push(&RtpPacket::parse(bytes).unwrap())
                .unwrap()
                .or(unit);
        }
        let unit = unit.unwrap();
        assert!(unit.bytes.windows(4).any(|bytes| bytes == [0, 0, 1, 0x65]));
        assert!(!complete_recovery_unit(&unit.bytes, VideoCodec::H264));
    }

    #[test]
    fn missing_leading_parameter_sets_do_not_finish_recovery() {
        let complete = [0, 0, 0, 1, 0x67, 1, 0, 0, 1, 0x68, 2, 0, 0, 1, 0x65, 3];
        assert!(complete_recovery_unit(&complete, VideoCodec::H264));
        assert!(!complete_recovery_unit(&complete[6..], VideoCodec::H264));
        assert!(!complete_recovery_unit(
            &[0, 0, 1, 0x67, 1, 0, 0, 1, 0x65, 3],
            VideoCodec::H264
        ));
        assert!(!complete_recovery_unit(&complete[..12], VideoCodec::H264));
    }
    #[test]
    fn hevc_recovery_requires_vps_sps_pps_and_irap() {
        let complete = [
            0, 0, 1, 64, 1, 0, 0, 1, 66, 1, 0, 0, 1, 68, 1, 0, 0, 1, 38, 1,
        ];
        assert!(complete_recovery_unit(&complete, VideoCodec::H265));
        assert!(!complete_recovery_unit(&complete[5..], VideoCodec::H265));
    }
    #[test]
    fn silent_codec_recovers_but_delayed_output_and_idle_are_allowed() {
        let now = Instant::now();
        let mut health = DecodeHealth::default();
        assert!(!health.observe(false, now));
        assert!(!health.observe(false, now + Duration::from_millis(100)));
        assert!(!health.observe(true, now + Duration::from_millis(200)));
        assert!(!health.observe(false, now + Duration::from_secs(10)));
        assert!(!health.observe(false, now + Duration::from_millis(10400)));
        assert!(health.observe(false, now + Duration::from_millis(10500)));
        assert!(!health.observe(false, now + Duration::from_secs(20)));
    }
}
