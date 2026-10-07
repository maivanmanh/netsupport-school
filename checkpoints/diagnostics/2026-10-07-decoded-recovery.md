# Decoder-confirmed recovery — 07-10-2026

## Scope / baseline

Owner authorized continuing the loss-stall investigation. Source baseline31a0e74.
The prior loss test's four zero-decode intervals were all startup, before the
first decoded output (4630.63 ms) / first-present event (4986.83 ms). They were
not four freezes after playback had begun. After first output there were zero
zero-decode report intervals. Original raw source remains target/recovery-budget-foreground-20261007.

Received AUs rose27→107 during those zero intervals; waiting_for_keyframe=false,
zero keyframe requests, zero decoder/depacketizer errors. First output followed
another periodic IDR. This proves codec acceptance/IDR arrival was mistaken for
usable recovery. The original first AU's raw NALs were not recorded, so a missing
SPS versus another silent-decoder cause cannot be asserted for that exact run.

A focused RTP regression reproduces a relevant cause: drop the leading SPS,
anchor at PPS, then assemble an AU containing IDR with no sequence error. A newly
initialized decoder lacks parameter state. Other silent backend cases require a
liveness check as well.

## Changes

- New decode_health module requires parameter-set presence with random access for
  recovery: H.264 SPS+PPS+IDR, H.265 VPS+SPS+PPS+IRAP. Current encoders prepend these
  sets to random-access AUs. Presence is necessary, not full bitstream validation.
  Reject an incomplete recovery AU, forget obsolete repair state and request a
  fresh IDR through existing250ms receiver throttle /400–800ms Teacher gate.
- Codec Ok(empty) is no longer evidence that video is usable. Allow delayed output
  and continue feeding AUs after an accepted IDR, without adding a queue. At least
  three silent submissions spanning500ms schedule fresh reference recovery.
  No-input idle periods alone do not trigger this; ordinary output clears the timer.
- Keep original accepted-IDR metrics; add decoded-output recovery event/peak,
  awaiting_decoded_recovery, incomplete AU and decoder-no-output-reset counters.
  Actual decoded output confirms the new metric; presentation/optical latency
  remains separate. Receiver jitter/cache/ingress budgets are unchanged.
- Disabled-by-default deterministic test hook drops first SPS or PPS RTP packet;
  random loss can use an explicit nonzero seed. Hooks are not production defaults.
  Recovery runner uses freshly paired ClassroomStudent.exe, avoiding a separate
  later diagnostic build/package ID. Analyzer separates startup zeros from
  post-first-output zeros and records first decode/present and new counters.

## Build / checks

Fresh paired label decoded-recovery-20261007: Mac Teacher/Mac Student/Windows x64
Student+Guard and embedded fresh Student ZIPs. Same production working tree;
no extra Windows CLI rebuild afterward. Local Flutter resolved lock retained
under ignored target; tracked lock restored. Exact Mac paths:

- /Users/mvmanh/Development/netsupport-school/target/local-recovery-build/Classroom Console.app
- /Users/mvmanh/Development/netsupport-school/target/local-recovery-build/Classroom Console.app/Contents/Helpers/Classroom Agent.app

70 focused tests passed: decode_health4, udp_ingress3, local expiration1,
media-transport20, existing probe imports42; one manual benchmark ignored.
All-bin check /Classroom clippy complete with existing warnings; scoped rustfmt,
Python compilation and diff checks. No keyframe-gate or bitrate/default change.

## Runtime attempts / current environment

Network changed since previous run: Mac192.168.50.206, Windows VM Ethernet
192.168.50.215, verified via Parallels guest tools. The old172.20.10.3 timed out.
SSH on new IP initially waited for unknown host-key confirmation; public ed25519
host key was compared directly from the VM and ssh-keyscan, matched, and only
that new IP entry was appended to known_hosts. No private key read or host-key
checking bypass. Intermediate prlctl subprocess capture/quoting attempts failed
before recording an entry; direct guest/public network results were used.

Initial SPS test then reached Share but failed ScreenCaptureKit TCC after new
signing. Owner was asked to manually regrant both exact app/helper paths and told
Teacher must restart. No privacy automation. Failed folders retained under
ignored target/decoded-recovery-*; no successful Mac-to-Windows runtime claim yet.

## Windows-native component runtime completed

While Mac permission remains pending, ran two bounded Windows loopback cases with
`scripts/run-windows-loopback-recovery.py`. Raw logs, manifest and analysis are in
`target/decoded-recovery-native-loopback-20261007`. Each case used a40-second
Windows DXGI/software H264 Encoder MFT Teacher and fresh Windows Student with
Microsoft H.264 decoder. Actual capture/negotiated dimensions1510×948, static
Windows desktop, adaptive idle5FPS. Receiver is headless: rendered0 is expected
and does not verify presentation. No random packet loss was configured.

| Case | Injected drops | First decoded output | Incomplete recovery AUs | Decoded frames | Decode errors | Post-start zero-decode intervals |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| First SPS |1|922.153ms|1|194|0|0|
| First PPS |1|156.845ms|0|224|0|0|

SPS case rejected the incomplete AU and requested fresh reference recovery. PPS
case repaired the missing middle packet with NACK before decode (4 vs9 requested
sequences in SPS/PPS cases; all resent, cache misses0). Both had no depacketizer
errors or final missing packets. Teacher send errors/abandoned packets0. Decode
rates5.02/5.73FPS reflect the static idle workload. The no-output watchdog did not
activate in these cases; its trigger is covered by focused tests only.

Compatibility scope: Teacher is the existing31a0e74 Windows CLI binary, preserved
before receiver changes; Student is fresh decoded-recovery-20261007. This is an
explicit component/compatibility test, not a same-revision native Windows release
build. The newly built Mac Teacher package separately contains its same-source
fresh Student ZIP. VM is Windows ARM running x64 binaries through emulation.
Loopback does not exercise LAN fan-out, visible rendering, optical latency or
physical x64 performance; no before/after performance comparison is claimed.

Mac Teacher → visible Windows Student deterministic SPS/PPS, longer random loss
and late join runs remain pending manual Screen Recording regrant and restart.


## Deferred physical validation

The Windows endpoint is ARM64 Parallels running x64 Student by emulation. This is
not physical wired Windows x64 or35/40 scaling acceptance. At the lab, include
missing leading SPS/PPS during startup/late join and silent no-output recovery
with actual hardware/software MFTs; record time to first decoded and presented
output, parameter rejection, NACK repair and gate timing. Replay actual1080p
visible A–E workloads at1/5/10/20/35/40 and optical/soak/DPI/resume tests from
checkpoints/diagnostics/2026-10-07-recovery-budget-followup.md. None of these gates
is closed by the VM/unit tests.
