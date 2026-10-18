//! Bounded ten-minute MP4 muxer. One pending AU establishes real sample durations;
//! mp4's sample tables are bounded by the chunk's <=18,001 samples (30 FPS).
#![cfg_attr(not(any(windows, test)), allow(dead_code))]
use anyhow::{Context, Result};
use mp4::{AvcConfig, Mp4Config, Mp4Sample, Mp4Writer, TrackConfig};
use std::{fs::File, io::BufWriter, path::Path};

pub(super) struct Mux {
    writer: Mp4Writer<BufWriter<File>>,
    pending: Option<Mp4Sample>,
    origin: u64,
    samples: u32,
    max_samples: u32,
}
fn units(bytes: &[u8]) -> Vec<&[u8]> {
    let mut codes = Vec::new();
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let n = if bytes[i..].starts_with(&[0, 0, 0, 1]) {
            4
        } else if bytes[i..].starts_with(&[0, 0, 1]) {
            3
        } else {
            i += 1;
            continue;
        };
        codes.push((i, i + n));
        i += n;
    }
    codes
        .iter()
        .enumerate()
        .filter_map(|(i, (_, start))| {
            let end = codes.get(i + 1).map_or(bytes.len(), |c| c.0);
            (*start < end).then_some(&bytes[*start..end])
        })
        .collect()
}
fn avcc(bytes: &[u8]) -> Result<Vec<u8>> {
    let nals = units(bytes);
    anyhow::ensure!(!nals.is_empty(), "Encoder output is not Annex B H.264");
    let mut out = Vec::with_capacity(bytes.len());
    for nal in nals {
        out.extend_from_slice(&u32::try_from(nal.len())?.to_be_bytes());
        out.extend_from_slice(nal);
    }
    Ok(out)
}
impl Mux {
    pub fn start(
        path: &Path,
        dimensions: (u32, u32),
        au: &[u8],
        pts: u64,
        max_samples: u32,
    ) -> Result<Self> {
        let nals = units(au);
        let sps = nals
            .iter()
            .find(|n| n[0] & 31 == 7)
            .context("First IDR lacks SPS")?
            .to_vec();
        let pps = nals
            .iter()
            .find(|n| n[0] & 31 == 8)
            .context("First IDR lacks PPS")?
            .to_vec();
        anyhow::ensure!(sps.len() >= 4, "Invalid SPS");
        anyhow::ensure!(
            nals.iter().any(|n| n[0] & 31 == 5),
            "Recording must start on an IDR"
        );
        let config = Mp4Config {
            major_brand: "isom".parse()?,
            minor_version: 512,
            compatible_brands: vec!["isom".parse()?, "avc1".parse()?, "mp41".parse()?],
            timescale: 90_000,
        };
        let mut writer = Mp4Writer::write_start(BufWriter::new(File::create(path)?), &config)?;
        let mut track = TrackConfig::from(AvcConfig {
            width: u16::try_from(dimensions.0)?,
            height: u16::try_from(dimensions.1)?,
            seq_param_set: sps,
            pic_param_set: pps,
        });
        track.timescale = 90_000;
        writer.add_track(&track)?;
        let mut mux = Self {
            writer,
            pending: None,
            origin: pts,
            samples: 0,
            max_samples,
        };
        mux.push(au, pts, true)?;
        Ok(mux)
    }
    pub fn push(&mut self, au: &[u8], pts: u64, keyframe: bool) -> Result<()> {
        let time = pts.saturating_sub(self.origin);
        anyhow::ensure!(
            self.samples < self.max_samples,
            "Recording sample table limit exceeded"
        );
        if let Some(mut previous) = self.pending.take() {
            anyhow::ensure!(
                time > previous.start_time,
                "Recording timestamps must increase"
            );
            previous.duration = u32::try_from(time - previous.start_time)?;
            self.writer.write_sample(1, &previous)?;
        }
        self.pending = Some(Mp4Sample {
            start_time: time,
            duration: 3000,
            rendering_offset: 0,
            is_sync: keyframe,
            bytes: avcc(au)?.into(),
        });
        self.samples += 1;
        Ok(())
    }
    pub fn finish(mut self, end_pts: u64) -> Result<()> {
        if let Some(mut last) = self.pending.take() {
            last.duration = u32::try_from(
                end_pts
                    .saturating_sub(self.origin)
                    .saturating_sub(last.start_time)
                    .max(1),
            )?;
            self.writer.write_sample(1, &last)?;
        }
        self.writer.write_end()?;
        let file = self
            .writer
            .into_writer()
            .into_inner()
            .map_err(std::io::IntoInnerError::into_error)?;
        file.sync_all()?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mp4_keeps_real_duration_and_idr_configuration() {
        let path =
            std::env::temp_dir().join(format!("record-mux-{}.mp4", super::super::nonce().unwrap()));
        let idr = [
            0, 0, 0, 1, 0x67, 0x42, 0, 0x1e, 0xab, 0x40, 0, 0, 1, 0x68, 0xce, 0x38, 0x80, 0, 0, 1,
            0x65, 0x88,
        ];
        let mut mux = Mux::start(&path, (1280, 720), &idr, 9000, 18_100).unwrap();
        mux.push(&[0, 0, 1, 0x41, 0x88], 18000, false).unwrap(); // 100 ms, not synthetic 33 ms.
        mux.finish(99000).unwrap();
        let reader = mp4::Mp4Reader::read_header(
            File::open(&path).unwrap(),
            std::fs::metadata(&path).unwrap().len(),
        )
        .unwrap();
        assert_eq!(reader.duration(), std::time::Duration::from_secs(1));
        assert_eq!(reader.tracks()[&1].width(), 1280);
        assert_eq!(reader.tracks()[&1].height(), 720);
        assert_eq!(reader.sample_count(1).unwrap(), 2);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn annex_b_handles_both_delimiters_and_refuses_missing_idr() {
        assert_eq!(
            avcc(&[0, 0, 1, 0x65, 3, 0, 0, 0, 1, 0x41, 4]).unwrap(),
            [0, 0, 0, 2, 0x65, 3, 0, 0, 0, 2, 0x41, 4]
        );
        assert!(avcc(&[1, 2, 3]).is_err());
        assert!(Mux::start(Path::new("unused.mp4"), (1280, 720), &[0, 0, 1, 0x41, 3], 0, 18_100).is_err());
    }
    #[test]
    fn real_h264_can_be_remuxed_without_encoding_or_audio() {
        let au = include_bytes!("fixtures/blue-720p.h264");
        let path = std::env::temp_dir().join(format!(
            "record-real-{}.mp4",
            super::super::nonce().unwrap()
        ));
        let mut mux = Mux::start(&path, (1280, 720), au, 0, 18_100).unwrap();
        mux.push(au, 9000, true).unwrap();
        mux.finish(90000).unwrap();
        let reader = mp4::Mp4Reader::read_header(
            File::open(&path).unwrap(),
            std::fs::metadata(&path).unwrap().len(),
        )
        .unwrap();
        assert_eq!(reader.duration(), std::time::Duration::from_secs(1));
        assert_eq!(reader.tracks().len(), 1);
        assert_eq!(reader.sample_count(1).unwrap(), 2);
        if let Some(output) = std::env::var_os("CLASSROOM_RECORDING_MP4_TEST_OUTPUT") {
            std::fs::copy(&path, output).unwrap();
        }
        std::fs::remove_file(path).unwrap();
    }
}
