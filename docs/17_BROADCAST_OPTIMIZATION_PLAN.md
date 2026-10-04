# Class broadcast optimization plan (no H.265 requirement)

Status owner: update the **Status** table whenever an item changes. Every agent
(Claude, Codex, …) continues from here plus `checkpoints/PROGRESS.md`.

Scope: Teacher → whole-class screen sharing. One-to-one Observe already has a
capability-negotiated profile (H.265@60 when both ends have hardware; see
`11_DECISION_LOG.md`). Everything below must work on ordinary hardware with
H.264 (hardware H.264 is near-universal; software MFT remains the fallback) and
must not break the AGENTS.md invariants (capture once, encode once, packetize
once, fan out, latest frame wins, bounded queues).

## 1. Current pipeline (2026-10-03 audit)

- Capture once: DXGI Desktop Duplication (Windows) / ScreenCaptureKit (macOS).
- Teacher Windows converts BGRA→NV12 on the **CPU** (SSE4.1), then Media
  Foundation H.264 Main, CBR, low-latency, no B-frames, max keyframe distance
  from `keyframe_interval_ms` (2 s ⇒ periodic IDR), on-demand IDR.
- RTP (RFC 6184) packetized once; **UDP unicast** fan-out to every Student;
  per-Student NACK repair from a retransmission cache; Student keyframe requests.
- Adaptive policy: dirty-region/activity-driven FPS (idle 5, typing 24, max 30),
  receiver-feedback bitrate quorum, latest-frame slot.
- **Mouse pointer is composited into the video**: pointer-only motion forces new
  encoded frames and the pointer moves at video cadence/latency.
- Student Windows decodes to system-memory NV12, uploads to D3D11 textures and
  converts in a shader (not zero-copy).

## 2. Comparison with commercial / mature products (public knowledge)

| Product / approach | What they do that we did not (yet) |
|---|---|
| NetSupport School, LanSchool | Changed-region capture/compression; pointer sent separately; optional broadcast/multicast "Show" so the Teacher sends one stream for the room. Excellent for static text, weaker for video. |
| Veyon (VNC/RFB) | Tile encodings, cursor pseudo-encoding (pointer separate). Unicast per client; scales poorly for 30–40 clients with motion. |
| Teams / Zoom / Meet / RDP | H.264 (or AV1/AVC444) "screen content" behaviour: low fps + high quality when static, switch to motion mode; pointer channel separate; loss recovery via LTR/intra-refresh instead of class-wide IDR; FEC. |

Our single-encode, freshness-first design already matches the modern approach;
the gaps are listed below in implementation order.

## 3. Work items (in order)

### O1 — Separate pointer channel
Teacher sends pointer position/visibility (high rate, tiny UDP datagrams) and
the pointer image (on change, fragmented, periodically repeated) on the media
socket with RTP payload type 100. Students draw the pointer locally over the
latest decoded frame, re-presenting on pointer-only motion. The capture stops
compositing the pointer only while **every** receiver advertises pointer-overlay
support (`HelloStudent.pointer_overlay`); a legacy receiver turns compositing
back on for the shared encode.
Expected: pointer moves at display rate regardless of video fps; pointer-only
motion no longer produces encoded frames.
Telemetry: `pointer_packets_sent`, `pointer_shape_packets_sent`,
`pointer_updates_per_second`, `pointer_mode` (overlay|composited) on Teacher;
`pointer_packets_received`, `pointer_redraws`, `pointer_age_ms` p50/p95 on Student.

### O2 — Static-content refinement ("sharpen after settle")
When activity stops (adaptive idle), encode one refinement frame at a raised
bitrate (`video.refine_bitrate_percent`, default 250 %) and then return to the
normal target; idle cadence continues as before. Text becomes crisp a moment
after the slide/code stops moving, without raising the motion bitrate.
Telemetry: `refinement_frames`, `refinement_bytes`, `refinement_ms`.

### O3 — Keyframe-storm control
One Student's loss or late join must not repeatedly force class-wide IDRs.
Coalesce keyframe requests within `video.keyframe_request_coalesce_ms`
(default 400 ms; a pending IDR satisfies all requesters), lengthen the periodic
IDR interval for broadcast (`keyframe_interval_ms` default raised 2 s → 4 s;
NACK + on-demand IDR cover recovery), and pace IDR packets (already via
`send_pacing_mbps`). Later: MF LTR / intra-refresh where the MFT supports it.
Telemetry: `keyframe_requests_received`, `keyframes_forced`,
`keyframe_requests_coalesced`, `periodic_keyframes`, `keyframe_bytes` p95.

### O4 — GPU zero-copy (Windows)
Teacher: DXGI texture → D3D11 VideoProcessor BGRA→NV12 → hardware MFT with a
DXGI device manager (no CPU readback). Student: MF decode into D3D11 textures
and present directly. Fallback to the current CPU path.
Telemetry: existing `color_conversion`, `render_upload_cpu` stages go to ~0.

### O5 — Multicast distributor + FEC (needs lab evidence first)
`MediaDistributor` multicast backend (one Teacher stream for the room), NACK
aggregation/repair multicast, optional XOR FEC, automatic unicast fallback when
multicast/IGMP is unavailable. Do this after the 1/5/10/20/40-PC ladder in
`06_PERFORMANCE_ACCEPTANCE.md` shows where the bottleneck is.

### Not recommended now
H.264 4:4:4 (not supported by common hardware). Prefer native resolution when
the Teacher screen is ≤ 1080p and a future "share window/region" mode.

## 4. Status

| Item | Status | Notes |
|---|---|---|
| O1 pointer channel | live-verified 2026-10-03 (Mac Teacher → Mac + Windows Students) | `pointer_channel.rs`, `media_transport::pointer`, DXGI/SCK `pointer_source`, renderer `set_pointer`/`redraw` |
| O2 static refinement | works mechanically; small effect on VideoToolbox — see §6 | `refinement.rs` (3 frames at 250 % @15 fps after settle) |
| O3 keyframe-storm control | live-verified 2026-10-03 (joins coalesced) | `keyframe_gate.rs`; periodic IDR 2 s → 4 s |
| O4 GPU zero-copy | planned | needs a Windows machine with a real GPU to verify (Parallels VM is not representative) |
| O5 multicast + FEC | deferred | after physical lab ladder |

## 5. Implementation notes (for the next agent)

- Pointer wire format: RTP PT 100, own SSRC (`presentation_ssrc ^ 0x504f494e`)
  and sequence. Payload kind 1 = state (visible, image serial, x, y, width
  fraction as u16 fixed point); kind 2 = image chunk (serial, w, h, hotspot,
  index, count, ≤1100 BGRA bytes). Images ≤128×128, straight alpha.
- Students advertise `HelloStudent.pointer_overlay = true` only when visible
  (not headless) and `video.pointer_channel`. The Teacher composites the pointer
  into video unless **every** registered receiver advertises it; switching is
  logged as `teacher pointer mode changed`. Observe never uses the channel.
- Windows: position/visibility from `GetCursorInfo`, image from Desktop
  Duplication shape updates (monochrome/masked converted, XOR pixels drawn
  opaque). Pointer-only DXGI updates are skipped (no desktop readback) while in
  overlay mode → `pointer_only_updates_skipped`.
- macOS: position via `CGEventGetLocation` + `CGDisplayBounds`, image via
  deprecated `NSCursor.currentSystemCursor` (polled ~8 Hz, hashed); fallback is
  `PointerImage::default_arrow`.
- Student draws over the latest frame and re-presents on pointer-only motion
  (Windows: alpha-blended D3D11 quad; GDI fallback blends on CPU; macOS: sprite
  save/restore in the minifb buffer).
- Refinement: `Refinement::tick(idle, base_bps)` on every media tick; adaptive
  bitrate changes are deferred while refining. VideoToolbox may rebuild (IDR)
  if it rejects a live bitrate change — watch `keyframes_periodic_or_rebuild`.
- Keyframe gate: route-probe joins and repair escalations call
  `KeyframeGate::request`; `poll` forces at most one IDR per window; emitted
  keyframes satisfy pending requests. Replaced the macOS-only 250 ms cooldown
  that silently dropped requests.
- Telemetry log lines (target `telemetry`): `teacher pointer channel interval`,
  `teacher refinement interval`, `teacher keyframe interval`,
  `student pointer channel interval`.

## 6. First live results (2026-10-03, Mac Teacher, 2 Students, Wi-Fi/LAN lab)

- O1: Teacher switched to `pointer_mode=overlay` once both Students registered
  with `pointer_overlay`; up to ~60 pointer updates/s, 0 send errors. Windows
  and Mac Students: 0 malformed pointer packets, local pointer redraws 6–50/s
  between video frames; video 27–28 decode/render FPS, 0 missing packets.
  `pointer_only_updates_skipped` stays 0 on a Mac Teacher (ScreenCaptureKit does
  not deliver pointer-only frames once `showsCursor=false`); the counter is for
  DXGI Teachers.
- O3: Student joins produced 3 IDR requests → 2 forced IDRs (1 coalesced);
  afterwards only periodic IDRs every 4 s (21–150 KB).
- O2: one refinement: VideoToolbox applied 15 Mbps live (no rebuild/IDR) for 3
  frames, but they totalled only ~2.3 KB — the low-latency VT rate control spends
  almost nothing on repeat frames. Follow-up options: measure on a Windows MF
  Teacher (CBR may behave differently); or make the settle refinement a single
  high-bitrate IDR (costs one keyframe burst per settle; combine with O3 gate).
- Not yet measured: a DXGI (Windows) Teacher, physical 40-PC LAN.

## 7. Lab-scope round (2026-10-03): simulator, latency, loss recovery

Setup: Teacher MacBook M1 Pro and Mac Studio M1 Ultra, **both on Wi-Fi**;
Windows 11 in Parallels on the Studio. Results are Wi-Fi numbers, not wired LAN.

New tooling — `scripts/simulate-class.py`: N headless Students (`HOSTNAME=sim-NN`,
own ports/profiles, monitoring off) on a remote Mac, optional emulated random
loss (`CLASSROOM_TEST_DROP_PERCENT`, test only, no firewall changes), class share
through Teacher IPC, optional on-screen motion workload, report in
`target/sim-class/<stamp>/report.md`. `sim-*` Students are never persisted in the
Teacher console roster.

Changes driven by measurements (10 simulated + 2 real Students, 50 s workload):

| Change | Evidence |
|---|---|
| Capture-aligned encode (`video.capture_aligned_encode`, default on) | fresh capture→encode wait p50 21.3 → 9.8–11.6 ms; capture→send p50 39.8 → 28.4–29.9 ms |
| VT latency properties (power efficiency off; speed-priority/frame-delay are rejected by low-latency RC) | encode wait 18 → 16 ms |
| O2 QP cap (`set_refinement`, VT `MaxAllowedFrameQP` 22; MF `AVEncVideoMaxQP`) | refinement bytes per settle ~0.3 KB → 14–40 KB (encoder now actually refines) |
| Adaptive keyframe gate window (×2 up to 2 s during storms) | forced IDRs 50 → 13 per 50 s; NACK sequences 16.8k → 2.8–4.5k |
| One NACK retry per open gap | `nack_retries` telemetry |
| RTT-adaptive repair budget (1.5×RTT+5 ms, ≤150 ms; ≥250 ms while waiting for a keyframe) | 3 % loss: unrepaired packets/Student 294 → 40; complete keyframes received 0 → 16 |

Final normal run (`n10-final2`): Teacher send p50/p95 9.7/51 Mbps (first run
16/146), Students decode 29 FPS p50, unrepaired packets 17 (first run ~2 400).

Measured and rejected: VT `DataRateLimits` (no effect in either RC mode; see the
ignored measurement test in codec-macos-vt), fixed jitter-buffer reduction
(complete frames already bypass the 20 ms hold), FEC for now (Wi-Fi loss here
was congestion; more bytes would worsen it).

Open: under 3 % uniform random loss Students still freeze ~40 % of intervals
(each depacketizer error waits for an IDR). Next candidates: H.264/HEVC error
concealment (keep decoding through a lost slice), or LTR-based recovery; then
FEC once a wired lab shows random loss. `process_cpu_percent` is unavailable for
the sandboxed Teacher agent (shows 0).


## 8. Wired-link scaling run (2026-10-04)

Setup: Teacher MacBook M1 Pro `192.168.10.100` ↔ Mac Studio M1 Ultra
`192.168.10.101` over a **Thunderbolt Bridge** cable (not 1 GbE; ping ~0.7 ms).
All virtual Students run headless on the one Studio; one real Mac Student was
also connected (hence `connected_students` = N+1). 1080p, motion workload on.

| Run | Teacher encode p95 (interval p50 / p95) | encode FPS p50 | send Mbps p50 / p95 / max | Student decode FPS p50 / p95 | missing pkts | retransmits | IDR requests → forced |
|---|---|---|---|---|---|---|---|
| 10 (`wired-n10`, 45 s) | 20.9 / 26.1 ms | 31.4 | 3.8 / 25 / 52 | 31.0 / 32.9 | 0 | 0 | 12 → 2 |
| 20 (`wired-n20`, 45 s) | 23.4 / 27.7 ms | 30.6 | 7.9 / 50 / 253 | 30.0 / 32.0 | 0 | 0 | 22 → 2 |
| 40 (`wired-n40`, 60 s) | 21.6 / 30.0 ms | 27.5 | 40 / 223 / 272 | 28.0 / 32.0 | 0 | 0 | 47 → 3 |
| Wi-Fi ref. (`n10-final2`) | 21.2 / 26.5 ms | — | 9.7 / 51 / — | 29 | 17 unrepaired | — | — |

Findings:
- Encode cost is flat in N (encode-once invariant holds); send rate scales
  linearly with N. Peaks (≤272 Mbps) are join/IDR bursts × 41 receivers and stay
  well under 1 Gbps.
- Zero loss and zero NACK/retransmission at 40 receivers on the wire.
- Each Student issues ~3 IDR requests, all during join; the keyframe gate
  coalesces 47 requests into 3 forced IDRs.
- 5 FPS decode intervals are the adaptive idle mode on a static screen
  (~0.04 Mbps), not stalls.
- Studio-side decode latency p50 rises 5 → 9.7 ms with 40 decoders on one
  machine; that is the simulator host's load, not a Teacher limit.
- Not yet representative: real 1 GbE switch, 40 distinct Windows PCs.

Harness notes: `simulate-class.py` now drains Teacher IPC while waiting (the
agent correctly drops IPC clients that stop reading for 3 s; at 40 Students the
state pushes filled the socket and the first 40-run aborted before Stop).
After a Student rebuild, macOS Local Network privacy blocked the Studio's
Student with `No route to host` although the toggle showed enabled; toggling it
off/on plus re-launching the app from the GUI restored access.
