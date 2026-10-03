use prost::Message;

pub const PROTOCOL_VERSION: u32 = 8;
pub const MAX_CONTROL_FRAME_BYTES: usize = 64 * 1024;

/// Independent classroom tools connection (control port + 2). Bulk file bytes
/// never enter Envelope, monitoring, media, or input queues. Versioned separately
/// so installing tools does not change the validated media wire protocol.
#[derive(Clone, PartialEq, Message)]
pub struct ToolPacket {
    #[prost(uint32, tag = "1")]
    pub version: u32,
    #[prost(string, tag = "2")]
    pub action: String,
    #[prost(string, tag = "3")]
    pub id: String,
    #[prost(string, tag = "4")]
    pub text: String,
    #[prost(bool, tag = "5")]
    pub allow_reply: bool,
    #[prost(string, tag = "6")]
    pub name: String,
    #[prost(uint64, tag = "7")]
    pub size: u64,
    #[prost(string, tag = "8")]
    pub token: String,
    #[prost(uint64, tag = "9")]
    pub done: u64,
    #[prost(string, tag = "10")]
    pub status: String,
    #[prost(string, tag = "11")]
    pub hash: String,
    #[prost(string, tag = "12")]
    pub student: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct Envelope {
    #[prost(uint32, tag = "1")]
    pub protocol_version: u32,
    #[prost(
        oneof = "envelope::Payload",
        tags = "10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22"
    )]
    pub payload: Option<envelope::Payload>,
}

pub mod envelope {
    use prost::Oneof;

    use super::{
        ControlPing, ControlPong, HelloStudent, Nack, ObserveMessage, ReceiverReport,
        RequestKeyframe, StartPresentation, StopPresentation, ThumbnailFrame, ThumbnailHello,
        ThumbnailRequest, WelcomeStudent,
    };

    #[derive(Clone, PartialEq, Oneof)]
    pub enum Payload {
        #[prost(message, tag = "10")]
        HelloStudent(HelloStudent),
        #[prost(message, tag = "11")]
        WelcomeStudent(WelcomeStudent),
        #[prost(message, tag = "12")]
        StartPresentation(StartPresentation),
        #[prost(message, tag = "13")]
        ReceiverReport(ReceiverReport),
        #[prost(message, tag = "14")]
        Nack(Nack),
        #[prost(message, tag = "15")]
        RequestKeyframe(RequestKeyframe),
        #[prost(message, tag = "16")]
        ControlPing(ControlPing),
        #[prost(message, tag = "17")]
        ControlPong(ControlPong),
        #[prost(message, tag = "18")]
        StopPresentation(StopPresentation),
        #[prost(message, tag = "19")]
        ThumbnailHello(ThumbnailHello),
        #[prost(message, tag = "20")]
        ThumbnailRequest(ThumbnailRequest),
        #[prost(message, tag = "21")]
        ThumbnailFrame(ThumbnailFrame),
        #[prost(message, tag = "22")]
        Observe(ObserveMessage),
    }
}

/// Separate monitoring TCP channel; images never enter media/control queues.
#[derive(Clone, PartialEq, Message)]
pub struct ThumbnailHello {
    #[prost(string, tag = "1")]
    pub instance_id: String,
}
#[derive(Clone, PartialEq, Message)]
pub struct ThumbnailRequest {
    #[prost(uint32, tag = "1")]
    pub width: u32,
    #[prost(uint32, tag = "2")]
    pub height: u32,
    #[prost(uint32, tag = "3")]
    pub interval_ms: u32,
    #[prost(uint32, tag = "4")]
    pub quality: u32,
}
#[derive(Clone, PartialEq, Message)]
pub struct ThumbnailFrame {
    #[prost(uint32, tag = "1")]
    pub width: u32,
    #[prost(uint32, tag = "2")]
    pub height: u32,
    #[prost(bytes = "vec", tag = "3")]
    pub jpeg: Vec<u8>,
    /// ready, paused (Teacher presentation), or unavailable (capture failure).
    #[prost(string, tag = "4")]
    pub state: String,
    #[prost(uint64, tag = "5")]
    pub sequence: u64,
}

#[derive(Clone, PartialEq, Message)]
pub struct HelloStudent {
    #[prost(string, tag = "1")]
    pub hostname: String,
    #[prost(string, tag = "2")]
    pub instance_id: String,
    #[prost(string, tag = "3")]
    pub selected_local_ip: String,
    #[prost(uint32, tag = "4")]
    pub media_udp_port: u32,
    #[prost(uint32, tag = "5")]
    pub feedback_udp_port: u32,
    /// Receiver draws the Teacher pointer from the pointer channel itself.
    #[prost(bool, tag = "6")]
    pub pointer_overlay: bool,
}

#[derive(Clone, PartialEq, Message)]
pub struct WelcomeStudent {
    #[prost(uint64, tag = "1")]
    pub client_id: u64,
    #[prost(bool, tag = "2")]
    pub presentation_active: bool,
}

#[derive(Clone, PartialEq, Message)]
pub struct StartPresentation {
    #[prost(uint64, tag = "1")]
    pub stream_id: u64,
    #[prost(uint32, tag = "2")]
    pub width: u32,
    #[prost(uint32, tag = "3")]
    pub height: u32,
    #[prost(uint32, tag = "4")]
    pub max_fps: u32,
    #[prost(uint32, tag = "5")]
    pub bitrate_bps: u32,
    #[prost(uint32, tag = "6")]
    pub rtp_payload_type: u32,
    #[prost(uint32, tag = "7")]
    pub rtp_clock_hz: u32,
    /// Teacher UDP port used for the Student's route probe and RTP source.
    #[prost(uint32, tag = "8")]
    pub teacher_media_udp_port: u32,
}

#[derive(Clone, PartialEq, Message)]
pub struct ReceiverReport {
    #[prost(uint64, tag = "1")]
    pub stream_id: u64,
    #[prost(uint64, tag = "2")]
    pub received_packets: u64,
    #[prost(uint64, tag = "3")]
    pub missing_packets: u64,
    #[prost(uint64, tag = "4")]
    pub decoded_frames: u64,
    #[prost(uint64, tag = "5")]
    pub rendered_frames: u64,
    #[prost(uint64, tag = "6")]
    pub receive_bitrate_bps: u64,
    #[prost(uint32, tag = "7")]
    pub jitter_delay_ms: u32,
    /// Most recent measured TCP control round trip; zero until the first reply.
    #[prost(uint64, tag = "8")]
    pub control_rtt_us: u64,
    #[prost(uint64, tag = "9")]
    pub control_rtt_samples: u64,
    /// Whether this Student has an active visible presentation renderer.
    #[prost(bool, tag = "10")]
    pub rendering_enabled: bool,
    /// p95 time spent inside native renderer calls during the report interval.
    #[prost(uint64, tag = "11")]
    pub render_latency_p95_us: u64,
}

#[derive(Clone, PartialEq, Message)]
pub struct ControlPing {
    #[prost(uint64, tag = "2")]
    pub sender_unix_us: u64,
    #[prost(uint64, tag = "1")]
    pub nonce: u64,
}

#[derive(Clone, PartialEq, Message)]
pub struct ControlPong {
    #[prost(uint64, tag = "2")]
    pub sender_unix_us: u64,
    #[prost(uint64, tag = "3")]
    pub teacher_received_unix_us: u64,
    #[prost(uint64, tag = "4")]
    pub teacher_sent_unix_us: u64,
    #[prost(uint64, tag = "1")]
    pub nonce: u64,
}

#[derive(Clone, PartialEq, Message)]
pub struct Nack {
    #[prost(uint64, tag = "1")]
    pub stream_id: u64,
    #[prost(uint32, repeated, tag = "2")]
    pub sequence_numbers: Vec<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct RequestKeyframe {
    #[prost(uint64, tag = "1")]
    pub stream_id: u64,
}

#[derive(Clone, PartialEq, Message)]
pub struct StopPresentation {
    #[prost(uint64, tag = "1")]
    pub stream_id: u64,
}

/// Stable RTP identity derived from the presentation's process-unique stream ID.
#[must_use]
pub fn presentation_ssrc(stream_id: u64) -> u32 {
    let bytes = stream_id.wrapping_sub(1).to_le_bytes();
    0x434c_4153 ^ u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

impl Envelope {
    #[must_use]
    pub fn hello(message: HelloStudent) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            payload: Some(envelope::Payload::HelloStudent(message)),
        }
    }

    #[must_use]
    pub fn welcome(client_id: u64) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            payload: Some(envelope::Payload::WelcomeStudent(WelcomeStudent {
                client_id,
                presentation_active: false,
            })),
        }
    }

    #[must_use]
    pub fn stop_presentation(stream_id: u64) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            payload: Some(envelope::Payload::StopPresentation(StopPresentation {
                stream_id,
            })),
        }
    }

    #[must_use]
    pub fn start_presentation(message: StartPresentation) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            payload: Some(envelope::Payload::StartPresentation(message)),
        }
    }

    #[must_use]
    pub fn receiver_report(message: ReceiverReport) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            payload: Some(envelope::Payload::ReceiverReport(message)),
        }
    }

    #[must_use]
    pub fn nack(message: Nack) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            payload: Some(envelope::Payload::Nack(message)),
        }
    }

    #[must_use]
    pub fn request_keyframe(message: RequestKeyframe) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            payload: Some(envelope::Payload::RequestKeyframe(message)),
        }
    }

    #[must_use]
    pub fn control_ping(nonce: u64) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            payload: Some(envelope::Payload::ControlPing(ControlPing {
                nonce,
                sender_unix_us: 0,
            })),
        }
    }

    #[must_use]
    pub fn control_pong(nonce: u64) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            payload: Some(envelope::Payload::ControlPong(ControlPong {
                nonce,
                sender_unix_us: 0,
                teacher_received_unix_us: 0,
                teacher_sent_unix_us: 0,
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_round_trips() {
        let original = Envelope::hello(HelloStudent {
            hostname: "student-01".to_owned(),
            instance_id: "instance".to_owned(),
            selected_local_ip: "10.0.55.11".to_owned(),
            media_udp_port: 47_810,
            feedback_udp_port: 47_811,
        });
        let encoded = original.encode_to_vec();
        let decoded = Envelope::decode(encoded.as_slice()).unwrap();
        assert_eq!(decoded, original);

        let report = Envelope::receiver_report(ReceiverReport {
            stream_id: 1,
            received_packets: 100,
            missing_packets: 0,
            decoded_frames: 30,
            rendered_frames: 15,
            receive_bitrate_bps: 6_000_000,
            jitter_delay_ms: 20,
            control_rtt_us: 25_000,
            control_rtt_samples: 2,
            rendering_enabled: true,
            render_latency_p95_us: 1_500,
        });
        let encoded = report.encode_to_vec();
        let decoded = Envelope::decode(encoded.as_slice()).unwrap();
        assert_eq!(decoded, report);
    }
}

/// Session-scoped Observe negotiation and remote input. Never carries video.
#[derive(Clone, PartialEq, Message)]
pub struct ObserveMessage {
    #[prost(string, tag = "1")]
    pub action: String,
    #[prost(string, tag = "2")]
    pub token: String,
    #[prost(uint32, tag = "3")]
    pub port: u32,
    #[prost(uint64, tag = "4")]
    pub stream_id: u64,
    #[prost(double, tag = "5")]
    pub x: f64,
    #[prost(double, tag = "6")]
    pub y: f64,
    #[prost(string, tag = "7")]
    pub key: String,
    #[prost(bool, tag = "8")]
    pub down: bool,
    #[prost(string, tag = "9")]
    pub error: String,
    /// Teacher → Student "start": codecs it can decode in hardware, preferred first.
    #[prost(string, repeated, tag = "10")]
    pub codecs: Vec<String>,
    /// Teacher → Student "start": requested frame-rate ceiling (0 = default profile).
    #[prost(uint32, tag = "11")]
    pub max_fps: u32,
    /// Student → Teacher reply: chosen session profile, e.g. "h265@60".
    #[prost(string, tag = "12")]
    pub codec: String,
}
