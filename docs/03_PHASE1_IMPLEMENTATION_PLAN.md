# 03 — Phase 1 implementation plan

## M0 — Skeleton and environment

Deliverables:

- Rust workspace.
- Windows target configuration.
- logging/error foundation.
- configuration load/override.
- single `Classroom.exe` role selection if practical.
- `--role teacher|student` CLI.
- robust active Ethernet/default-route selection.
- basic Teacher TCP listener and Student connection stub.
- checkpoint documentation.

Exit gate: both roles launch on Windows and report the intended physical adapter/IP reliably despite Hyper-V/VMware adapters.

## M1 — Capture benchmark

Implement DXGI Desktop Duplication.

Requirements:

- primary monitor first,
- 1920×1080 capture,
- include cursor in final presented frame,
- handle no-update timeouts efficiently,
- use dirty/move rectangle metadata when available,
- recover from `DXGI_ERROR_ACCESS_LOST`, mode change, lock/unlock, or monitor reset,
- capture metrics.

Exit gate: stable continuous capture without memory/resource leaks.

## M2 — Local codec/render vertical slice

Implement Media Foundation H.264 encoder/decoder and native render.

Requirements:

- capability enumeration,
- hardware encoder use when healthy/available,
- software encoder fallback,
- low-latency configuration,
- no B-frames,
- input color conversion strategy,
- SPS/PPS extraction/cache,
- on-demand IDR,
- local decode/render.

Exit gate: sustained 1080p30 on the reference Windows machine with measured encode p50/p95 and CPU utilization, or a documented bottleneck and mitigation.

## M3 — 1-to-1 network

Add:

- Student registration over TCP,
- endpoint/capability exchange,
- RTP/H.264 UDP send/receive,
- small jitter buffer,
- remote render,
- when a Teacher presentation is active, use a borderless full-display Student surface that stays
  above ordinary application windows; ordinary window-close commands must not end the active
  presentation,
- receiver health report.

Exit gate: one teacher and one student on separate Windows machines display the presentation smoothly and with measured end-to-end delay.

## M4 — Recovery and freshness

Add:

- latest-frame single-slot back-pressure verified under induced load,
- sequence-loss detection,
- NACK and short retransmission cache,
- late packet drop,
- IDR request,
- late join,
- reconnect.

Exit gate: induced packet loss/reconnect does not create multi-second stale playback.

## M5 — Scale to 40

Scale in measured steps:

1 -> 5 -> 10 -> 20 -> 40.

At each step record:

- encoder instance count,
- capture FPS,
- encode latency p50/p95,
- teacher CPU/RAM,
- teacher network send Mbps,
- dropped/replaced frames,
- client receive bitrate/loss,
- client decode/render FPS.

Do not infer 40-client success from one-client success.

## M6 — Adaptive quality

Only after M5 is stable:

- adaptive bitrate,
- content/dirty-area-aware FPS,
- CPU/encode-pressure response,
- network/receiver aggregate response,
- optional resolution fallback.

Do not overfit adaptation to the i7-14700 lab. Simulate or test constrained CPU/network conditions where practical.
