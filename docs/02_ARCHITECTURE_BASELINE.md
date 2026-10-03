> Current role and packaging baseline: [Teacher/Student architecture](15_ROLE_ARCHITECTURE.md).

# 02 — Phase 1 architecture baseline

## Top-level architecture

There is no dedicated server in Phase 1.

```text
Teacher PC
+----------------------------------------------------+
| Classroom.exe (Teacher role)                       |
|                                                    |
|  Native UI / Session Coordinator                   |
|           |                                        |
|  DXGI Desktop Duplication                          |
|           |                                        |
|  Frame analysis / conversion                       |
|           |                                        |
|  H.264 Encoder (ONE instance for presentation)     |
|           |                                        |
|  RTP packetizer                                    |
|           |                                        |
|  Unicast MediaDistributor                          |
|     |        |        |                  |          |
+-----|--------|--------|------------------|----------+
      v        v        v                  v
    S01      S02      S03                S40
```

Each Student role has one receiver/decode/render pipeline.

## Logical components

Recommended boundaries:

- `app-core`: roles, session state, common types.
- `config`: defaults, overrides, validation.
- `platform-api`: cross-platform traits.
- `platform-windows`: Windows-specific discovery/adapter/system helpers.
- `capture-api`: `CaptureSource` trait and frame contracts.
- `capture-windows-dxgi`: DXGI Desktop Duplication implementation.
- `codec-api`: encoder/decoder interfaces and capability description.
- `codec-windows-mf`: Media Foundation H.264 implementation.
- `media-core`: frames, encoded access units, timestamps, stream state.
- `rtp-h264`: packetize/depacketize, sequence/timestamps, SPS/PPS handling.
- `media-transport`: sender/receiver abstractions and unicast distributor.
- `control-protocol`: Protobuf schema and framed control messages.
- `control-transport`: TCP connection/registration/session messages.
- `quality`: adaptive FPS/bitrate/resolution policy and health aggregation.
- `metrics`: structured runtime metrics.
- `renderer-api` / `renderer-windows`: student presentation rendering.
- `ui`: minimal native role/teacher/student controls.

The exact crate count can be adjusted to avoid unnecessary fragmentation, but preserve these boundaries conceptually.

## Reusability requirement

The future system must support a student becoming presentation source. Therefore do not encode teacher-specific assumptions into the deepest media interfaces.

Conceptually:

```rust
PresentationSource = LocalTeacherDisplay | RemoteStudentDisplay(StudentId)
```

A publisher creates one encoded stream; a distributor routes it to subscribers. Phase 1 only activates the local teacher source.

## No server assumption in core protocol

The teacher currently hosts the coordinator, but protocol/message code should not rely on process identity in a way that prevents a coordinator from being moved to a separate process/server later.
