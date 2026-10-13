use std::time::Duration;

use codec_linux::LinuxH264Decoder;
use media_core::{
    DecodedFrameRef, DecodedPixelFormat, EncodedAccessUnitRef, H264Config, VideoCodec, VideoDecoder,
};

const FRAMES: [&[u8]; 3] = [
    include_bytes!("fixtures/testsrc-64x48-0.h264"),
    include_bytes!("fixtures/testsrc-64x48-1.h264"),
    include_bytes!("fixtures/testsrc-64x48-2.h264"),
];
const BLUE: &[u8] = include_bytes!("fixtures/blue-128x72.h264");

fn config() -> H264Config {
    H264Config {
        width: 1920,
        height: 1080,
        frames_per_second: 30,
        bitrate: 6_000_000,
        keyframe_interval_frames: 60,
    }
}

fn au(bytes: &[u8], index: u32) -> EncodedAccessUnitRef<'_> {
    EncodedAccessUnitRef {
        bytes,
        timestamp: Duration::from_millis(u64::from(index) * 33),
        is_keyframe: index == 0,
    }
}

#[test]
fn real_idr_and_predicted_frames_return_without_one_frame_delay() {
    let mut decoder = LinuxH264Decoder::new(config()).unwrap();
    for (index, bytes) in FRAMES.iter().enumerate() {
        let output = decoder
            .decode(au(bytes, u32::try_from(index).unwrap()))
            .unwrap();
        assert_eq!(output.len(), 1, "complete AU must produce its own frame");
        let frame = &output[0];
        assert_eq!((frame.width, frame.height), (64, 48));
        assert_eq!(frame.format, DecodedPixelFormat::Bgra);
        assert_eq!(frame.bytes.len(), 64 * 48 * 4);
        assert!(frame
            .bytes
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[3] == 255));
        assert_eq!(
            frame.media_timestamp,
            Some(au(bytes, u32::try_from(index).unwrap()).timestamp)
        );
        DecodedFrameRef {
            format: frame.format,
            bytes: &frame.bytes,
            width: frame.width,
            height: frame.height,
        }
        .validate()
        .unwrap();
    }
    let metrics = decoder.take_diagnostics().summaries();
    assert_eq!(metrics.len(), 2);
    assert!(metrics.iter().all(|metric| metric.samples == 3));
    assert!(decoder.take_diagnostics().summaries().is_empty());
}

#[test]
fn sps_resolution_change_and_bgra_channel_order_are_honored() {
    let mut decoder = LinuxH264Decoder::new(config()).unwrap();
    decoder.decode(au(FRAMES[0], 0)).unwrap();
    let frames = decoder.decode(au(BLUE, 0)).unwrap();
    assert_eq!(frames.len(), 1);
    assert_eq!((frames[0].width, frames[0].height), (128, 72));
    // A blue encoded picture must have blue in byte zero, not byte two.
    assert!(frames[0]
        .bytes
        .as_chunks::<4>()
        .0
        .iter()
        .all(|pixel| { pixel[0] > 240 && pixel[1] < 15 && pixel[2] < 15 && pixel[3] == 255 }));
}

#[test]
fn rejects_bad_inputs_and_recovers_with_fresh_parameter_sets_and_idr() {
    let mut decoder = LinuxH264Decoder::new(config()).unwrap();
    assert!(decoder.decode(au(&[], 0)).is_err());
    assert!(decoder.decode(au(&[1, 2, 3], 0)).is_err());
    assert!(decoder
        .decode(au(&vec![0; 4 * 1024 * 1024 + 1], 0))
        .is_err());
    // Valid Annex B framing with a broken IDR (no parameter sets) must fail.
    assert!(decoder.decode(au(&[0, 0, 0, 1, 0x65, 0xff], 0)).is_err());
    decoder.reset().unwrap();
    assert_eq!(decoder.decode(au(FRAMES[0], 0)).unwrap().len(), 1);
}

#[test]
fn rejects_unsupported_codec_and_initial_resolution() {
    assert!(LinuxH264Decoder::with_codec(config(), VideoCodec::H265).is_err());
    for (width, height) in [(0, 1080), (1920, 0), (1921, 1080), (1920, 1081)] {
        assert!(LinuxH264Decoder::new(H264Config {
            width,
            height,
            ..config()
        })
        .is_err());
    }
}

#[test]
fn rejects_oversized_sps_before_allocating_application_pixels() {
    let mut decoder = LinuxH264Decoder::new(config()).unwrap();
    let error = decoder
        .decode(au(include_bytes!("fixtures/oversized-1922x16.h264"), 0))
        .unwrap_err();
    assert!(error.to_string().contains("1922x16"));
    decoder.reset().unwrap();
    assert_eq!(decoder.decode(au(FRAMES[0], 0)).unwrap().len(), 1);
}
