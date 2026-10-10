use anyhow::{ensure, Result};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const DATA: u8 = 1;
pub const INPUT: u8 = 2;
pub const RESIZE: u8 = 3;
pub const CLOSE: u8 = 4;
pub const READY: u8 = 5;
pub const EXIT: u8 = 6;
pub const ACK: u8 = 7;
pub const PING: u8 = 8;
pub const AUTH: u8 = 9;
pub const MAX_BYTES: usize = 16 * 1024;
pub const GRACE: std::time::Duration = std::time::Duration::from_secs(15);

#[derive(Clone, Debug)]
pub struct Frame {
    pub kind: u8,
    pub seq: u64,
    pub data: Vec<u8>,
}
impl Frame {
    pub fn new(kind: u8, seq: u64, data: impl Into<Vec<u8>>) -> Self {
        Self {
            kind,
            seq,
            data: data.into(),
        }
    }
}
pub async fn read(r: &mut (impl AsyncRead + Unpin)) -> Result<Frame> {
    let kind = r.read_u8().await?;
    ensure!((DATA..=AUTH).contains(&kind), "Unknown terminal frame");
    let seq = r.read_u64().await?;
    let len = r.read_u32().await? as usize;
    ensure!(len <= MAX_BYTES, "Terminal frame exceeds limit");
    let mut data = vec![0; len];
    r.read_exact(&mut data).await?;
    Ok(Frame { kind, seq, data })
}
pub async fn write(w: &mut (impl AsyncWrite + Unpin), f: &Frame) -> Result<()> {
    ensure!(f.data.len() <= MAX_BYTES, "Terminal frame exceeds limit");
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        w.write_u8(f.kind).await?;
        w.write_u64(f.seq).await?;
        w.write_u32(f.data.len() as u32).await?;
        w.write_all(&f.data).await?;
        w.flush().await?;
        Ok::<(), anyhow::Error>(())
    })
    .await??;
    Ok(())
}
