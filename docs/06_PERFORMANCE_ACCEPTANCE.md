# 06 — Performance and acceptance criteria

## Primary acceptance scenario

Environment:

- one Windows teacher PC,
- up to 40 Windows student PCs,
- wired 1 Gbps Ethernet,
- 1920×1080 displays,
- teacher presents IDE/browser/slides/terminal content,
- no audio.

## Required properties

### Architecture

- One teacher presentation encoder instance regardless of viewer count.
- No unbounded video queue.
- No cloud/Internet dependency.
- No dedicated server requirement.

### Video

- Target resolution: 1920×1080.
- This is the ideal reference, not a required display mode. Also test Windows Teacher sources at 1366×768, 1280×800, 2560×1600, 3440×1440 and 3840×2160 (as available), paired with Students of different resolutions. Confirm no upscaling of small sources, an output bounding box no larger than 1920×1080, aspect preservation within even-pixel rounding, readable text, visible cursor, and undistorted resized/minimized/restored Student windows. Record the actual encoder/control dimensions and capture latency including resize. Test hardware and software MFTs; cross-compilation does not pass this gate.
- Maximum frame rate: 30 fps.
- Adaptive FPS is acceptable and encouraged.
- Text/code should remain readable; preserving 1080p at a lower FPS is generally preferred over immediately dropping to 720p.

### Latency

Goal on healthy wired LAN:

- preferred typical glass-to-glass: approximately 100–150 ms,
- p95 goal: <= 200 ms.

If measurement method adds known camera/timer uncertainty, state it explicitly.

No scenario should intentionally accumulate seconds of queued old frames.

### Scaling

Run separately at:

- 1 student,
- 5 students,
- 10 students,
- 20 students,
- 40 students.

Record teacher-side and client-side metrics for each scale point.

### Degradation

Under CPU/network pressure:

- stay responsive,
- drop/replace stale frames,
- reduce bitrate before FPS when runtime encoder control is available,
- reduce resolution only after other measures are insufficient,
- avoid freezing or multi-second lag.

## Required benchmark outputs

For each scale point capture at least:

Teacher:
- CPU %,
- working set / memory,
- capture FPS,
- capture p50/p95 if measured,
- encode FPS,
- encode p50/p95 latency,
- bitrate of encoded stream,
- aggregate NIC send Mbps,
- frames captured,
- frames encoded,
- waiting frames replaced/dropped,
- number of active encoder instances.

Student sample(s):
- CPU %,
- receive Mbps,
- packet loss/missing sequences,
- retransmission/NACK count,
- jitter-buffer delay,
- decode FPS,
- render FPS,
- late/dropped decoded frames.

## Suggested workload set

Use a repeatable workload rather than only watching a static desktop:

A. Static slide with text.
B. Typing code in an IDE/editor.
C. Fast vertical scrolling through source code/web page.
D. Terminal output updating continuously.
E. Large-screen motion/video as a stress case.

Phase 1 is primarily optimized for A–D; E verifies graceful behavior rather than defining the whole product.

## End-to-end latency test

Create a reproducible method. Examples include showing a millisecond counter/animation on the teacher screen and photographing teacher/student displays together, or another validated high-speed/clock-aligned technique. Document the method and uncertainty.
