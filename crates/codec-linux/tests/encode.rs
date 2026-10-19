use std::time::{Duration, Instant};

use codec_linux::{LinuxH264Decoder, LinuxH264Encoder};
use media_core::{
    EncodedAccessUnitRef, H264Config, VideoDecoder, VideoEncoder, VideoFrameRef,
};

fn config(width: u32, height: u32) -> H264Config {
    H264Config { width, height, frames_per_second: 30, bitrate: 2_000_000, keyframe_interval_frames: 120 }
}

fn frame(width: u32, height: u32, shade: u8) -> Vec<u8> {
    let mut bytes = vec![0_u8; (width * height * 4) as usize];
    for (index, px) in bytes.chunks_exact_mut(4).enumerate() {
        let x = (index as u32 % width) as u8;
        // BGRA: blue-ish gradient with a red block in the corner.
        px.copy_from_slice(&[shade.wrapping_add(x), 40, if index < 2000 { 220 } else { 30 }, 255]);
    }
    bytes
}

#[test]
fn encoder_output_round_trips_through_the_decoder() {
    let (w, h) = (320_u32, 180_u32);
    let mut encoder = LinuxH264Encoder::new(config(w, h)).unwrap();
    let mut decoder = LinuxH264Decoder::new(config(w, h)).unwrap();
    let mut decoded = 0;
    let mut keyframes = 0;
    let mut first = true;
    for index in 0..30_u8 {
        let bytes = frame(w, h, index * 3);
        let au = encoder
            .encode(VideoFrameRef { bytes: &bytes, captured_at: Instant::now(), width: w, height: h })
            .unwrap()
            .expect("access unit");
        assert!(au.bytes.starts_with(&[0, 0, 0, 1]) || au.bytes.starts_with(&[0, 0, 1]), "Annex B");
        if first {
            assert!(au.is_keyframe, "first frame is an IDR");
            // SPS (type 7) and PPS (type 8) must travel with the IDR for clean joins.
            let types: Vec<u8> = au.bytes.windows(4).filter(|v| v[..3] == [0, 0, 1]).map(|v| v[3] & 31).collect();
            assert!(types.contains(&7) && types.contains(&8), "SPS/PPS present: {types:?}");
            first = false;
        }
        keyframes += usize::from(au.is_keyframe);
        let frames = decoder
            .decode(EncodedAccessUnitRef { bytes: &au.bytes, timestamp: au.timestamp, is_keyframe: au.is_keyframe })
            .unwrap();
        for f in frames {
            assert_eq!((f.width, f.height), (w, h));
            decoded += 1;
        }
    }
    assert_eq!(decoded, 30, "every access unit decodes immediately (no B-frames)");
    assert_eq!(keyframes, 1);
}

#[test]
fn forced_keyframe_and_bitrate_change_produce_a_decodable_idr() {
    let (w, h) = (320_u32, 180_u32);
    let mut encoder = LinuxH264Encoder::new(config(w, h)).unwrap();
    let mut decoder = LinuxH264Decoder::new(config(w, h)).unwrap();
    let bytes = frame(w, h, 10);
    let mut encode = |encoder: &mut LinuxH264Encoder| {
        encoder
            .encode(VideoFrameRef { bytes: &bytes, captured_at: Instant::now(), width: w, height: h })
            .unwrap()
            .unwrap()
    };
    encode(&mut encoder);
    assert!(!encode(&mut encoder).is_keyframe);
    encoder.request_keyframe().unwrap();
    assert!(encode(&mut encoder).is_keyframe);
    encoder.set_bitrate(1_000_000).unwrap();
    let au = encode(&mut encoder);
    assert!(au.is_keyframe, "bitrate change restarts with an IDR");
    // A late joiner that only sees the post-change IDR decodes it.
    let mut fresh = LinuxH264Decoder::new(config(w, h)).unwrap();
    let out = fresh
        .decode(EncodedAccessUnitRef { bytes: &au.bytes, timestamp: Duration::ZERO, is_keyframe: true })
        .unwrap();
    assert_eq!(out.len(), 1);
    drop(decoder.decode(EncodedAccessUnitRef { bytes: &au.bytes, timestamp: Duration::ZERO, is_keyframe: true }));
}

#[test]
fn rejects_odd_dimensions() {
    assert!(LinuxH264Encoder::new(config(321, 180)).is_err());
}
