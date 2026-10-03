# 07 — Recommended repository/module layout

The coding agent may adjust crate granularity, but keep concerns separated.

```text
classroom/
├── Cargo.toml
├── AGENTS.md
├── config/
│   └── default.toml
├── apps/
│   └── classroom/
├── crates/
│   ├── app-core/
│   ├── config/
│   ├── protocol/
│   ├── control-transport/
│   ├── media-core/
│   ├── media-transport/
│   ├── rtp-h264/
│   ├── capture-api/
│   ├── capture-windows-dxgi/
│   ├── codec-api/
│   ├── codec-windows-mf/
│   ├── renderer-api/
│   ├── renderer-windows/
│   ├── platform-windows/
│   ├── quality/
│   └── metrics/
├── proto/
│   └── classroom.proto
├── docs/
└── checkpoints/
```

Avoid creating tiny crates solely for aesthetic purity. If implementation velocity is better with fewer crates, combine adjacent areas while preserving public interfaces/modules.

## Suggested core interfaces

Conceptual examples, not mandatory syntax:

```rust
trait CaptureSource {
    fn start(&mut self, cfg: CaptureConfig) -> Result<()>;
    fn next_frame(&mut self, deadline: Instant) -> Result<Option<VideoFrame>>;
}

trait VideoEncoder {
    fn capabilities(&self) -> EncoderCapabilities;
    fn encode(&mut self, frame: VideoFrameRef) -> Result<Option<EncodedAccessUnit>>;
    fn set_bitrate(&mut self, bps: u32) -> Result<()>;
    fn request_keyframe(&mut self) -> Result<()>;
}

trait VideoDecoder {
    fn decode(&mut self, au: EncodedAccessUnitRef) -> Result<Vec<DecodedFrame>>;
}

trait MediaDistributor {
    fn add_subscriber(&mut self, endpoint: SubscriberEndpoint) -> Result<()>;
    fn remove_subscriber(&mut self, id: SubscriberId);
    fn distribute(&mut self, packet: MediaPacketRef) -> Result<()>;
}

trait Renderer {
    fn render(&mut self, frame: DecodedFrameRef) -> Result<()>;
}
```

Keep Windows handle/resource ownership explicit and safely wrapped.
