# 11 — Locked decisions and rationale

## Owner override — Windows Student fill and Observe toolbar (2026-10-13)

Windows Student class presentation defaults to stretching the complete picture to
the whole display, accepting distortion on mismatched ratios. Teacher Settings →
Phát màn hình offers stretch-to-fill and preserve-aspect; the saved selection
applies to the next presentation. `[video].presentation_preserve_aspect` defaults
to false, including old profiles. Additive `StartPresentation.preserve_aspect`
(Protobuf tag 9, protocol 8 unchanged) conveys the policy; old Students ignore
it and retain their old aspect behavior until updated. GPU and GDI share the
same geometry; capture dimensions, encode-once, RTP and bounded queues remain.
macOS/Linux renderers retain their current behavior.

Observe/Control removes the Native/Gốc toolbar choice and uses English Fill and
Stretch. Fill is selected initially and resizes the picture with the window.
Initial native window sizing still fits source pixels/DPI within the Teacher
work area. Stretch retains the existing aspect-preserving cover/scroll behavior.
This overrides the earlier Student aspect-preservation display default below.

## Owner-approved Windows smoothness optimization (2026-09-28)

The first physical Windows LAN run exposed render-call p95 of 15–53 ms and
Teacher UDP `WSAEWOULDBLOCK` errors. The owner approved separating receive/decode
from native presentation, GPU rendering with software fallback, bounded UDP burst
pacing/retries, and retaining bitrate when only presentation cadence is slow.
The weaker school rooms remain first-class targets alongside i7-14700 / Intel
integrated graphics rooms. No hardware encode/decode or discrete GPU dependency
is introduced. Render cadence remains diagnostic; packet loss, decode pressure
and encoder/CPU pressure still drive adaptation.

## Owner override — forced Student presentation surface (2026-09-27)

While Teacher sharing is active, a visible Student must show the stream in a borderless
primary-display window that remains above ordinary application windows. The Student cannot close
the active Windows presentation through the normal window-close command. The surface ends with the
Teacher/control session. Keep M2 local preview and `--headless` benchmark behavior unchanged. This
is presentation UI enforcement; privileged process protection and keyboard/mouse restrictions
remain separate future service work.

## Owner clarification — variable Windows display resolution (2026-09-25)

1920×1080 is the ideal reference and maximum transmitted bounding box, not a required Teacher or Student display mode. Windows Teacher fits the captured desktop within that box and any smaller configured width/height, without upscaling smaller sources. Preserve the aspect ratio to the nearest supported even pixel dimensions for H.264/NV12. Student fits the announced video into its own window with black bars when needed. The stream dimensions are chosen before encoder initialization and control announcement. Changing Teacher display mode during a presentation still requires restarting that presentation when its fitted dimensions change; live renegotiation is a separate recovery task.

## Rust over Go

The performance-critical client requires deep Windows-native capture, codec, rendering, networking, and later privileged-control integration. Rust is chosen as the main implementation language because it is well suited to system-level FFI/resource ownership while still supporting cross-platform architecture.

## Windows first

The actual university labs are Windows-based. macOS is a portability target, not a Phase 1 implementation target.

### Owner override — macOS M2 development path (2026-09-23)

The owner no longer has access to a Windows test machine outside the school. To keep M2 measurable, macOS now has native ScreenCaptureKit capture, VideoToolbox H.264 encode/decode, and native preview backends behind the same media interfaces. Windows remains the deployment target and its DXGI/Media Foundation acceptance gate remains independent and pending; macOS measurements must not be presented as Windows validation.

## Native desktop, not web

Later classroom controls need native OS access and the owner explicitly wants a desktop application. The media path must not depend on a browser runtime.

## One portable binary initially

A single binary with selectable Teacher/Student roles simplifies development and lab testing. Production-specific installers can come later.

## Teacher PC acts as coordinator

There is no dedicated server in the lab. Phase 1 therefore colocates session coordination and media distribution on the teacher PC.

## H.264 first

H.264 offers broad Windows support and a practical low-latency path through Media Foundation. AV1/H.265 are not needed for the first proof.

## Media Foundation first

It reduces external binary/deployment complexity and supports both hardware and software paths. The architecture remains codec-backend pluggable if later benchmark evidence favors x264/OpenH264/other backends.

## UDP unicast fan-out first

The lab has 1 Gbps wired NICs. One 6 Mbps stream fanned out to 40 students is about 240 Mbps before overhead, which is within a 1 Gbps teacher link. Unicast is easy to debug and does not depend on unknown institutional multicast behavior. Multicast remains a future optional backend.

## TCP control first

Control traffic is small and benefits from simple reliability. QUIC/WebRTC are unnecessary complexity for the first LAN-only implementation.

## No broad broadcast discovery

The observed `/20` network and unknown campus VLAN/broadcast topology make room-wide broadcast discovery undesirable. Use the campus `.200` teacher convention plus manual override.

## Adaptive screen-content delivery

The target workload is mostly code, slides, terminals, and browsers. Resolution/readability matters more than forcing 30 fps continuously. Frame rate and bitrate may adapt to activity/load while resolution is preserved where possible. Under sustained encoder pressure, packet loss, or receiver decode slowdown, reduce bitrate first; reduce the active FPS ceiling only after bitrate reaches its configured minimum or runtime bitrate control is unavailable.

## VideoToolbox bitrate changes

Mac Teacher applies adaptive average-bitrate changes to the live VideoToolbox
compression session when supported. This avoids rebuilding the encoder and
forcing a new keyframe during a routine bitrate step. If the encoder rejects a
runtime update, retain the previous build-before-swap session fallback. Windows
Media Foundation already updates its active encoder property when available.

## Owner override: Flutter management UI (2026-10-02)

Owner rejects minifb custom management controls after actual quick-click failures,
and approves Flutter UI + Rust agent. Native desktop Flutter is not a browser,
Electron, WebView or web-server product. Minifb remains in working native media
surfaces. Existing portable Rust CLI remains usable for diagnostics/Student;
Teacher GUI packaging includes Flutter runtime/assets and a Rust helper. IPC is
framed JSON over authenticated ephemeral loopback TCP, separate from classroom
control/media, with bounded messages/client counts and latest state. UI closing
can leave the agent alive; explicit exit stops room/presentation. No full video
frame transfer via Dart. This changes the earlier single-binary GUI decision.


## Student runtime remains Rust-only (2026-10-02)

Owner clarified the active deployment: local Mac is Teacher with Flutter console;
Windows Parallel and the other Mac are Students and run the existing Rust agent in
the background. Do not add Flutter to Student installs for this milestone. Flutter
is a management console, not the video/render process. A Windows Flutter Teacher
bundle is only needed if Windows is later assigned the Teacher role.

## Owner clarification: permanent role boundaries (2026-10-02)

Teacher management UI is exclusively Flutter, with Rust Teacher media/network
agent, on macOS and Windows. Student production target is Windows and its agent
is entirely Rust, idle in the background with native fullscreen only for
presentation. Mac Student exists solely for development tests. Add a dedicated
ClassroomStudent entry point to eliminate accidental role selection during
Student launch; retain Classroom as helper/diagnostic compatibility executable.
Windows Flutter Teacher build is deferred, not excluded from the product.
See 15_ROLE_ARCHITECTURE.md for current packaging and launch behavior.

## Capability-negotiated one-to-one media profile (2026-10-03)

Owner request: when a single Teacher↔Student Observe/Control stream runs on
hardware that supports better options, use them automatically; otherwise keep
the current behavior. Classroom broadcast is unchanged (one H.264 encode, fan-out
to every Student) — the AGENTS.md H.264 baseline still applies to it.

- `media_core::VideoCodec` (H.264 = RTP PT 96, H.265 = PT 98). RTP packetizer/
  depacketizer are codec-aware (RFC 6184 FU-A / RFC 7798 FU); VideoToolbox
  encoder/decoder take a codec; keyframe detection uses H.264 IDR / H.265 IRAP.
- Teacher `start` offer (`ObserveMessage.codecs`, `max_fps`) lists the codecs it
  can decode in hardware, preferred first, plus `[observe].max_fps` (default 60).
- Student picks the first offered codec it can encode; the frame-rate ceiling and
  bitrate are raised only when that codec has a hardware encoder. The chosen
  profile is announced via `StartPresentation.rtp_payload_type`/`max_fps` and
  echoed as `ObserveMessage.codec` ("h265@60"). Old peers ignore new fields and
  stay H.264@30.
- Windows: Media Foundation HEVC/hardware probing is not wired yet, so Windows
  Students/Teachers keep H.264 at the broadcast profile (extension point:
  `stream_profile::hardware_encode` / `local_decodable`).
- Config `[observe]`: `enhanced`, `codec` (auto|h265|h264), `max_fps`,
  `bitrate_kbps` (0 = derived). Receiver pacing ceiling raised 30 → 60 (broadcast
  config stays 30); FPS ladder gains 48/30 steps that only matter above 30.
- UI: negotiated codec + fps ceiling shown in Session stats and the Observe footer.

## D-DevUpdate — 2026-10-05, owner authorization

Owner explicitly authorized Student self-update for development iteration,
including unattended transfer with no UI/SSH interaction after bootstrap.
Reuse the separately paced tools TCP downloads and authenticated local Teacher
IPC; keep media protocol/queues unchanged. Use per-OS/architecture ZIP manifests,
pre-exit Student compatibility probe, same-volume supervised swap, receipt-confirmed
reconnect and rollback. This is not production release authentication or OS
permission automation. See `19_DEV_STUDENT_UPDATES.md`. Physical Windows wired
1/5/10/20/40 acceptance remains pending and is not altered by this scope.

## D-LanBootstrap — 2026-10-06, owner authorization

Owner approved optional Teacher-hosted HTTP ZIP download at `/` for initial LAN
Student setup, browser/extract/double-click rather than shell piping. This is a
package endpoint, not a web product/UI or media transport. Rust Student receives
room IP/port via adjacent config; Teacher controls native connection diagnostics.
Windows Student tray omits Exit/Restart; process protection remains service work.
Existing unattended logs/update flow remains after bootstrap. See doc20.

## Owner override — Ubuntu Student receiver (2026-10-13)

The owner authorized Ubuntu Student implementation, prioritizing the media
receiver before further platform features. Linux reuses control, RTP/H.264,
recovery and the latest decoded slot, with bundled OpenH264 software decode and
a native X11/XWayland presentation backend. Software decoding avoids a new
hardware/runtime-codec dependency. Native Wayland, Linux Teacher, capture/reverse
publishing and platform controls remain separate tasks. This is an added receiver
backend, not a redesign of Teacher distribution or Windows performance acceptance.
See [Ubuntu Student tasks and verification](28_UBUNTU_STUDENT.md).
