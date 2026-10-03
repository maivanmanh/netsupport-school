# Two-Mac automatic tests and fixes — 2026-10-01

Teacher: 192.168.100.219; Student: Apple M4, 192.168.100.254; Wi-Fi.
Evidence: `../evidence/macos-2026-10-01-automated/` contains full logs, process
statuses, text summaries and JSON stage aggregates. No raw logs were discarded.

## Tests actually run

| Scenario | Result | Final-build measurements |
| --- | --- | --- |
| Baseline | Connected, decoded and rendered; recovered from a substantial gap | 22.23 decode / 18.64 render FPS, zero decode errors, 1738 legacy missing packets |
| Intentional loss | NACK/retransmission and IDR recovery exercised | 28.24 decode / 24.97 render FPS, zero decode errors, two legacy missing packets |
| Late join | Clean registration and visible output after Teacher was streaming | 25.67 decode / 23.15 render FPS, zero missing packets or decode errors |
| Teacher end | Preview closed, no second registration | 29.18 decode / 25.25 render FPS during the active session |
| Teacher restart | Same Student re-registered and created a second visible preview | Two decoded/rendered sessions, zero decode errors |
| Three simulated Students | All three decoded; Teacher reported three clients and one encoder | Approximately 20.03 decode FPS each, headless; initial suite approximately 26.13 FPS each |

`MEDIA_OK` means functional delivery/lifecycle checks passed. It is deliberately
not a 1080p30 or latency acceptance verdict. Content/activity differs between
these short runs; FPS comparisons across scenarios are not controlled A/B results.
The final baseline contains a roughly 12-second wall-log discontinuity without
a corresponding reported one-second monotonic interval. That evidence does not
uniquely distinguish clock change, host sleep/suspension or other timing effects.
Do not attribute its 1738 missing packets to codec failure: decode errors stayed
zero, local ingress drops stayed zero, and an IDR restored output. The first
40-second baseline was clean, with 27.98 decode / 24.43 render FPS and no missing
packets; the later Wi-Fi run cannot erase this variability.

Clock-corrected Teacher-completion to preview-close estimates were approximately
20 ms (disconnect) and 17 ms (restart), subject to clock uncertainty. These are
log event estimates, not optical observations or universal shutdown bounds.

## Fixes implemented and verified

1. VideoToolbox can output BGRA directly, retaining default-format/NV12 fallback.
   Actual selected output format is logged. Controlled same-host, same-Teacher
   sequential comparison measured the copy/conversion stage mean at 7.409 ms
   with CPU NV12 conversion and 0.850 ms with native BGRA. Decode p50 changed
   from 8.837 to 4.588 ms. These numbers do not isolate all workload effects or
   promise the same improvement on other hardware. Remote final decode p95 was
   about 7.2–7.7 ms for the visible scenarios. No decode errors were observed.
2. Encoder uses its actual VideoToolbox output timestamp and a bounded source
   timeline, rather than the incremented input counter. Decoder preserves the
   callback timestamp. Student output timing now maps, enabling send/capture to
   render estimates; previously all Mac decoded frames were unmapped.
3. Added bounded `vt_decode_submit`, `vt_decode_wait`, `vt_decode_copy_convert`
   distributions and actual pixel-format events. Capture-to-Present sample-weighted
   means in visible final scenarios were about 67–84 ms. Largest observed interval
   p95 was about 388 ms in the disturbed baseline; this is not session p95 or
   glass-to-glass and does not establish the wired-LAN latency acceptance gate.
4. Teacher refreshes its capacity-one frame selection once, without waiting,
   after pacing sleep. This removes avoidable use of a pre-sleep capture. Fresh
   capture-to-encode means were about 18–21 ms in the final suite, compared with
   about 32 ms in an earlier active restart sample; workloads were not identical.
5. macOS topmost maintenance moved from each roughly 1 ms poll to once per second,
   retaining initial fullscreen/topmost setup. Windows already used this cadence.
6. Adapter discovery now takes one OS snapshot rather than enumerating twice.
   Startup duration is logged. Final startup samples were around 8 ms. A prior
   run spent about 54 seconds before adapter selection; its exact OS cause is
   not proven by these logs. The runner initializes Student before starting the
   Teacher duration, preventing that delay from consuming the Teacher test window.

## Verification and delivery

- `cargo test --workspace --lib --bins --quiet`: 109 passed, zero failed, one
  opt-in real-capture test ignored. Codec bitrate/keyframe integration test passed.
- Native strict Clippy for changed codec/renderer/platform/application targets,
  formatting, Bash syntax and Python compilation passed.
- GNU Windows workspace check and portable release build passed using rustup's
  installed Windows target. MSVC check was unavailable because that target is
  not installed. A known host build-script stripping warning was nonfatal.
- Windows executable copied and byte-compared to
  `/Users/mvmanh/Downloads/Classroom.exe`, SHA-256
  `5b0f94b7a936058c4c8c3c6a470c215b7c975e18c00a7fe6b7b81267eedfa3c6`.
- Mac release SHA-256:
  `e77c4aa0d8a25da8d97dde13eb5653dadcc90605f615a083149ae4045e376d26`.

## Current limits

Three processes on one M4 share hardware and a Wi-Fi link. They verify unicast
fan-out structure and receiver health at that point, not three independent PCs
or 40-PC scaling. Neither optical latency nor controlled text-readability was
automatically measured. macOS still copies capture/decode/render buffers; this
is not zero-copy GPU media. Windows runtime remains to be tested on real PCs.

No bitrate/resolution tuning was justified solely by the visible render cadence.
Healthy source decode and bounded latest-frame presentation remain intact.
Next acceptance work is repeatable workload and optical timing, then wired
Windows 1/5/10/20/40-client evidence when those machines are available.
