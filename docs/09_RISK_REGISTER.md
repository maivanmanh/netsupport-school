# 09 — Initial technical risk register

## R1 — Media Foundation encoder behavior varies by machine/driver

Mitigation:
- enumerate capabilities,
- report selected backend,
- fail over to software MFT,
- keep codec backend pluggable,
- benchmark on at least one weaker room before declaring Phase 1 complete.

## R2 — Cross-building on macOS succeeds but Windows runtime integration fails

Mitigation:
- keep a native Windows test loop from M0,
- add Windows CI/build if repository infrastructure permits,
- never accept cross-compile success as runtime validation.

## R3 — Wrong network adapter selected because of Hyper-V/VMware

Mitigation:
- select via default route/active Ethernet,
- surface selected adapter/IP,
- provide manual override,
- unit-test adapter ranking logic with synthetic interface lists where feasible.

## R4 — Unicast fan-out consumes more bandwidth/CPU than expected

Mitigation:
- measure at 1/5/10/20/40,
- reuse immutable packet buffers,
- keep sender path allocation-light,
- multicast remains a future backend if evidence shows it is needed.

## R5 — Queueing creates “smooth but seconds behind” playback

Mitigation:
- latest-frame slot,
- bounded queues everywhere,
- late packet/frame discard,
- explicit buffer-depth metrics,
- overload tests.

## R6 — Static-screen optimization causes visible lag when motion begins

Mitigation:
- motion/activity estimator must ramp quickly,
- force immediate frame/keyframe as needed after large changes,
- make thresholds configurable and benchmark IDE scrolling/typing.

## R7 — Loss recovery harms freshness

Mitigation:
- retransmit only recent/useful packets,
- expire retransmission cache quickly,
- request IDR instead of chasing old loss,
- measure recovery delay.

## R8 — UI framework forces copies or blocks media threads

Mitigation:
- isolate media engine from UI,
- use a dedicated render path/window if necessary,
- never run capture/encode/decode work on the UI event thread.

## R9 — Reference lab is much faster than weaker rooms

Mitigation:
- hardware acceleration optional,
- adaptive FPS/bitrate,
- benchmark representative weaker systems before broad deployment,
- do not optimize solely for i7-14700.
