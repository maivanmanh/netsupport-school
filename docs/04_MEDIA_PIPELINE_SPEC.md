# 04 — Media pipeline specification

## Teacher hot path

```text
DXGI AcquireNextFrame
        |
        v
D3D11 texture + dirty/move/pointer metadata
        |
        v
Frame scheduler / activity estimator
        |
        v
LatestFrameSlot (effective capacity 1)
        |
        v
Color conversion / preprocessing
        |
        v
H.264 encoder
        |
        v
Encoded access unit / NAL units
        |
        v
RTP/H.264 packetizer
        |
        v
Unicast fan-out to registered student media endpoints
```

## Latest-frame rule

Windows capture resolves its output dimensions at startup: fit the complete source within the configured bounds and the Phase 1 1920×1080 cap, do not upscale, and round to even pixels for NV12. Cursor composition precedes resizing. The application uses these actual dimensions for both H.264 and `StartPresentation`; it must not announce the configured bounds as the stream size. Windows Student stretches the received picture to its complete client rectangle by default (owner override 2026-10-13); Teacher can choose aspect preservation instead. Decoder storage padding is distinct from the visible image; honor a matching zero-origin Media Foundation display aperture and skip padded luma rows before reading chroma. See [Microsoft's display-area guidance](https://learn.microsoft.com/en-us/windows/win32/medfound/picture-aspect-ratio).

A waiting unencoded frame is disposable. If capture produces F101, F102, F103 while the encoder is busy and only one waiting slot is available, the slot should contain F103 when the encoder becomes available. Old waiting frames must not create accumulated latency.

Encoded-frame queues and sender queues must also be bounded. If network send cannot keep up, expose the overload and shed work rather than buffer seconds of presentation.

## Screen-content scheduling

The application may use dirty-region ratio and timing to lower the effective frame rate when the screen is mostly static. Example behavior, subject to measurement:

- static slide/code: 3–5 fps,
- typing/small changes: around 10–15 fps,
- scrolling/moderate motion: around 20–30 fps,
- large motion/video: up to 30 fps.

Do not hard-code these values deep in the capture implementation. Put them in a quality/scheduling policy.

## H.264 profile behavior

Optimize for low latency and broad Windows decoder compatibility:

- no B-frames,
- low/no lookahead,
- short enough GOP for recovery but not wasteful,
- default keyframe interval around 2 seconds as a starting point,
- immediate IDR on join/recovery request,
- retain latest SPS/PPS and ensure a joining decoder receives required parameter sets before/with the recovery keyframe.

Normalize Media Foundation output into a stable internal encoded-access-unit representation before RTP packetization so backend-specific byte-stream details do not leak across the codebase.

## Cursor

The teacher cursor must be visible to students. Desktop Duplication pointer metadata may require explicit cursor composition. Handle pointer shape changes and visibility correctly.

## Hardware acceleration policy

Use hardware encode/decode if Windows reports a usable backend and runtime initialization succeeds. Never fail the application simply because a hardware MFT is unavailable. Software fallback is required.

No NVIDIA/discrete GPU dependency is allowed.

## Student presentation surface

An active Teacher presentation uses a borderless primary-display Student window, stretches the
picture to fill by default (or preserves aspect with bars when selected), and stays above ordinary application
windows. The presentation surface is removed when the media/control session ends. The diagnostic
`--headless` mode intentionally creates no window, and the M2 local codec preview remains a normal
resizable window. This application-level presentation behavior does not replace the future
privileged service needed for process protection or operating-system input restrictions.

## Decoupled Student rendering (2026-09-28)

Student's receive/jitter/depacketize/decode worker owns codec initialization and
teardown. The window owner thread consumes a mutex-protected decoded frame slot
of capacity one. Locks are released before codec/render calls; intermediate decoded
frames are replaced while H.264 reference decoding remains in sequence. Window
close on control disconnect does not wait for decoder teardown.

Windows prefers D3D11 hardware rendering (including Intel integrated graphics):
reusable NV12 luma/chroma upload textures, GPU linear aspect scaling, and a two-buffer
DXGI flip swap chain with driver frame latency one. Decoder output stays NV12 with GPU color conversion; this is not hardware
decode or zero-copy. If GPU initialization or
presentation fails, retain the filtered GDI path. A 1 ms timer request is balanced
for the window lifetime; fullscreen/topmost maintenance occurs once per second
rather than every receive tick. GPU/CPU identity, decode/render p95, decoded-frame
wait p95 and pending slot depth are retained in compact log.txt.
