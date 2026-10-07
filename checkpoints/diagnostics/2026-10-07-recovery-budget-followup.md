# Recovery budget / connected UDP follow-up — 07-10-2026

## Scope and source

Owner requested the three next steps: bound recovery when an IDR exceeds wire
admission, make Mac connected UDP work under default config, then run matched
longer baseline/loss/late-join tests. Baseline commit 6eebc99.
Mac 172.20.10.2 and Windows 11 ARM64 Parallels 172.20.10.3 (x64 emulation).
This is development validation; physical Windows x64 wired acceptance remains open.

## Changes

- Fanout reports a rejected keyframe's suggested bitrate ratio to its single
  encoder owner. Ratio is 80% of deadline / required wire time, clamped 1–80%.
  Coalesced worker hints retain the smallest ratio; the encoder consumes it once
  after previous fanout completion. No extra capture, encoder or packet queue.
- QualityController lowers bitrate immediately for this direct sender evidence,
  without waiting for absent receiver reports or three pressure intervals.
  Existing adaptive opt-out, backend support and configured minimum remain in force.
  Cancel refinement first; keep ordinary bitrate increase cooldown and existing
  KeyframeGate coalescing. Original 120/300 ms deadlines and aggregate 400 Mbps
  pacing are unchanged. Failed reference chains still wait for successful IDR.
- The rate ratio is a hint, not a guarantee that any encoder produces proportional
  IDR bytes. If the backend refuses adjustment, adaptation is disabled; at minimum
  bitrate or adaptive opt-out, log that recovery cannot shrink further. It still
  discards oversized AUs rather than buffering old video. Physical encoder behavior
  at the configured floor must be checked; this patch does not guarantee recovery
  for arbitrarily large IDRs.
- macOS only: when an established per-endpoint connected UDP socket returns
  BrokenPipe, retire it and retry the unsent datagram once on the shared socket.
  The fallback uses the same source port, pacing governor and original AU deadline;
  it persists until the endpoint changes/rejoins. Other socket errors retain normal
  bounded failure handling. Windows connected sockets are unchanged. The underlying
  macOS sandbox/kernel cause is not claimed as diagnosed.
- Separate counters: preflight recipient drops, actual deadline drops, connected
  send fallbacks, and injected ingress drops. The injected / wire-packets ratio
  includes all accepted peer datagrams (pointer traffic included); it is not a
  physical LAN loss measurement nor exact video-only injection percentage.
- Diagnostic probe can combine real QualityController/KeyframeGate with an explicit
  proportional IDR-size model. Default fixed-size mode stays available as negative
  control. This is synthetic transport, not actual capture/codec validation.
- Recovery runner supports up to 600 seconds per scenario, polls IPC at bounded
  cadence, and workload records foreground/visibility once per second, capped 660
  samples. Analyzer reports audited phases and foreground coverage.

## Synthetic evidence

Raw: target/recovery-budget-probe. 13 ms simulated encode, local UDP sinks,
normal 25 KB, 400 Mbps, four workers; no physical NIC or H.264 decoding.

| Case | Submitted | Complete AUs per sink | Rejected recipient AUs | Final modeled bitrate | Submission FPS |
|---|---:|---:|---:|---:|---:|
| 40, fixed 361,003-byte IDR, negative control | 60 | 0 | 2400 | 6 Mbps | 30.21 |
| 40, 361,003-byte initial IDR, proportional model + real gate/policy | 180 | 168 | 40 | 4.62 Mbps | 25.55 |
| 35, 600,000-byte initial IDR, proportional model + real gate/policy | 180 | 168 | 35 | 3.18 Mbps | 27.41 |

Both modeled recovery cases finish the next IDR around 659 ms after startup
(~400 ms gate plus ~240 ms send). They have zero partial AUs, duplicates, send errors and
WouldBlock. First IDR is rejected; dependent frames are skipped while the existing
400 ms gate waits. Subsequent modeled IDRs fit. Do not present these as a physical
35/40 receiver improvement or real encoder bitrate-to-size guarantee.

## Paired build and runtime

Label recovery-budget-20261007. Rebuilt Mac Teacher/Mac Student/Windows x64
Student/Guard and embedded fresh matching Student ZIPs. Also rebuilt Windows
Classroom.exe diagnostic role from the same production source. Exact Teacher:

`/Users/mvmanh/Development/netsupport-school/target/local-recovery-build/Classroom Console.app`

Helper: same parent plus `/Contents/Helpers/Classroom Agent.app`.
Flutter local resolved lockfile retained as target/local-recovery-build/pubspec.resolved.lock;
tracked pubspec.lock restored. No release asset replacement or push.

Initial three-minute attempt stopped immediately at ScreenCaptureKit TCC denial
following signing. Failed evidence retained in target/recovery-budget-run-20261007;
owner manually regranted both exact paths, and Teacher was restarted.

The first granted run is retained in target/recovery-budget-granted-20261007.
Baseline decoded/rendered 5061/4939 over 179.31 s (28.23/27.55 FPS), zero codec
errors but one zero-decode interval, recovery IDR-arrival peak 1254.28 ms and
456 ingress queue drops. Its workload audit had only 4.76% foreground samples;
this is not a controlled workload comparison. The loss scenario was interrupted
and cleaned up; do not call it a completed test. The cause of foreground switches
is not proven (Terminal was foreground when inspected after stopping).

The diagnostic workload now reactivates its window once per second if inactive.
A separate 15-second preflight audited 14/14 foreground and visible samples.
Corrected run: target/recovery-budget-foreground-20261007. Each Share was configured
180 seconds; the late receiver's media interval is shorter due to the 8-second join
delay. All workloads audited 189/189 foreground and visible samples and all phases
A–E. Actual video 1512×982; one VideoToolbox encoder, Windows MFT decoder/D3D11.

| Scenario | Decoded/rendered frames | Decode/render FPS | Zero-decode one-second intervals | IDR-arrival recovery peak ms |
|---|---:|---:|---:|---:|
| baseline | 5239/5163 | 29.22/28.80 | 0 | 208.53 |
| configured 1% random ingress loss | 4625/4464 | 25.78/24.88 | 4 | 910.82 |
| late join (8 s) | 5010/4948 | 28.94/28.58 | 0 | 192.01 |

All zero codec/depacketizer errors and confirmed first-present event; all Stop/Exit
cleanup completed. Mac connected_fanout remained true. Each session logged one
BrokenPipe fallback, then zero RTP send errors/abandoned packets. Teacher NACK
requested/resent 50/50, 343/343, 68/68; zero cache misses/repair send errors.
Loss injected 352 packets; 352/38,903 accepted peer datagrams = 0.905% (pointer
traffic is in denominator). Ingress queue drops 24/0/0. Missing sequences are a
separate recovery/jitter counter, not the measured injected-loss percentage.
Encoded stream average 1.63/1.73/1.60 Mbps despite 6 Mbps target; no sustained
6 Mbps high-motion acceptance. Recovery peaks stop at acceptable IDR arrival,
not visible photons. No same-frame comparison against an older executable.

The loss run is functional but not a smoothness pass: four zero-decode intervals
remain. Next investigation is to align injected loss, repair/jitter expiry,
keyframe gate waits, decode and render events; do not hide stalls with a larger
buffer. This is a separate unresolved receiver/loss recovery issue from oversized
sender-IDR admission. Physical Windows x64 reproduction remains required.

## Real encoder forced-budget negative test

Raw target/recovery-budget-pressure-20261007, 60-second baseline only. Temporary
Teacher override: 40 Mbps aggregate, 10 ms normal and keyframe deadlines. This is
intentionally much stricter than the product's 400 Mbps /120/300 ms defaults.
Original settings were backed up and restored immediately after startup, then
verified byte-for-byte after GUI exit (SHA256 recorded by driver).

Live single VideoToolbox encoder accepted 6 →2.1 →1.5 Mbps in response to rejected
IDR hints (35%, 65%). At minimum bitrate, many IDRs still failed the 10 ms wire
budget: 153 decoded /152 rendered over 59.45 s, 49 zero-decode report intervals,
IDR-arrival recovery peak 29.43 s; no codec errors. Eventually some small IDRs fit,
so first-present exists, but this is clearly a recovery/smoothness failure, not
passed just because processes exit zero or a few frames appear. Latest-frame
slots stayed bounded (688 waiting captures replaced); no extra encoder/queue or
deadline extension. This proves real policy-to-encoder wiring and explicitly
exposes the floor limitation; it does not demonstrate smooth 35/40 recovery.
If real lab 300 ms budgets still reject at the bitrate floor, a separately measured
resolution/encoder-IDR-size policy is required before acceptance.

## Cleanup and package integrity

All isolated Teacher/helper/workload/Student processes and recovery scheduled tasks
stopped. Original Windows Guard PID3248 and installed Student PID5456 at Program
Files remain; no replacement/firewall/privacy automation. Both embedded Student
ZIP executable hashes match their manifests and label recovery-budget-20261007;
Windows ZIP includes Guard. Extra Windows CLI rebuild has a later generated build
ID, so its loose ClassroomStudent.exe is not byte-identical to the Student from
initial paired packaging; production runtime source and label are the same (only
regression-test code was added afterward). Bundle packages are from the fresh
paired build, not copied from an older label. Tracked lockfile remains original.

## Checks

- 42 tests passed / one manual ignored in transport_probe (imported fanout, sender,
  quality, keyframe gate, process metrics), including three new regression tests.
- 20 media-transport tests passed. Quality subset independently 24 passed; it overlaps
  imported tests and is not counted again (62 distinct focused tests total).
- cargo check all Classroom bins and clippy Classroom/probe passed; existing warnings
  and unused APIs from imported diagnostic modules remain. Scoped rustfmt,
  Python compilation and diff checks; Swift workload compiled.

## Deferred physical lab checklist — required, not passed

Keep Teacher/Student revision, actual 1920×1080, target bitrate, resolution,
workload timings and foreground identical between baseline and changed build.
Preserve IDs/settings/logs and deploy through Classroom updater; stop owned apps
before replacement. Confirm all receivers report the intended new build.

| Test | Machines/environment | Required record | Status |
|---|---|---|---|
| Scale ladder 1/5/10/20/35/40 | Windows x64, wired 1 Gbps | Teacher encode count/FPS/p95/CPU/RAM/actual Mbps/NIC Mbps; per-Student decode/render/loss/NACK/jitter/reference recovery | Deferred: no lab |
| Text/typing/scroll/terminal/motion A–E | Same visible workload at each scale | Foreground evidence, actual dimensions/bitrate; same-frame A/B when comparing builds | Deferred |
| Large real IDR recovery | Hardware Quick Sync and software MFT, 35 and 40 receivers | AU bytes/packet count/ratio/bitrate applied; admitted vs rejected IDR; time to IDR, decode and visible presentation | Deferred |
| Encoder floor/unsupported/opt-out | Same lab workloads | Confirm bounded shedding; record if bitrate cannot shrink IDR; do not count as recovery pass | Deferred |
| Loss and recovery | Controlled injected loss plus healthy LAN baseline | Injection count separate from actual loss, NACK/cache miss reason, stalls, recovered visible content | Deferred |
| Late join/rejoin while sharing | One and burst joins at 35/40 | Clean SPS/PPS/IDR, time to first decode/present, no keyframe storm | Deferred |
| Optical glass-to-glass | Teacher and sampled Student displays together | Method/uncertainty; typical 100–150 ms and p95 ≤200 ms goal | Deferred |
| 30-minute fixed-resolution soak | Physical x64 wired, representative 35/40 | No zero-decode intervals/multi-second freeze; CPU/RAM, resolution and encoder count throughout | Deferred |
| Actual sleep/resume and 125%/150% DPI | Supported physical Windows | Resume presentation/geometry/readability/cursor and logs | Still unverified from earlier milestone |

No simulator/VM result closes docs/06_PERFORMANCE_ACCEPTANCE.md.


### Later clarification of the four zero-decode intervals

Subsequent timeline analysis found all four loss-run zero-decode intervals occurred
before the first decoded frame (first_decode_ms=4630.63, first_present_ms=4986.83).
Received AUs increased27→107, with waiting_for_keyframe=false, zero decoder errors
and zero keyframe requests. First output followed a later periodic IDR. There were
zero zero-decode report intervals after first output in this run. Thus the earlier
"four stalls" description needs this startup qualification. Missing leading
parameter sets are a reproducible way to enter this silent-decoder state, but raw
NAL contents of that original first IDR were not recorded, so its exact missing
parameter/slice cause is not proved.
