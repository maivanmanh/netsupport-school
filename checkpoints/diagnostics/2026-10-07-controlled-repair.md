# Controlled first-repair loss — 2026-10-07

## Scope and plan before changes

Previous random-loss A/B was inconsistent:50second RTT retry trial looked better,
120second comparison did not. Keep production half-budget retry. The existing
optional RTT experiment changes only second-NACK timing, not expiry/queue/cache.

Owner authorized continued testing. Add a disabled-by-default bounded ingress
hook selecting one H.264 middle IDR FU-A packet, identified by SSRC/timestamp/
sequence. Drop its original arrival and the next arrival with that identity
(first repair), accept the third. Log exactly these events and time from first
repair drop to accepted copy. No random loss, no parameter loss, no firewall
changes; native decoder/render still exercised. Keep one selected packet per
receive worker and do not rearm after completion. A/B same fresh paired build,
50second foreground five-phase workload, half-budget vs RTT opt-in. This creates
a known repeated gap rather than relying on random loss to hit a retransmission.

Require one clean receiver session, exactly two injected drops, accepted third
copy, matching Teacher NACK/repair evidence and successful decode/presentation.
Measure injected-event times in the same ingress worker clock; application/optical
latency remains separate. Physical Windows x64 wired35/40 validation deferred.

## Implementation and paired build

Disabled hook CLASSROOM_TEST_DROP_FIRST_IDR_AND_REPAIR=1: one H.264 middle IDR
FU-A selected by SSRC/timestamp/sequence; original and next identical-identity
copy dropped, third accepted, no rearm. Two unit tests cover default-off behavior,
FU start/end/non-IDR/pointer exclusions, identity isolation and exactly two drops.
No packet queues or repair budgets changed. Ingress telemetry counts injected
repair drops and accepted second repair; three event logs use one worker clock.
Runner requires baseline-only/no parameter hook for this injection so random loss
is disabled. Analyzer validates three events/same identity, two drops/one accepted,
clean session and functional decode/render. It also records accepted-copy to first
decode event gap (log timing only, not optical latency or proof of frame mapping).

Paired controlled-repair-20261007 rebuilt Mac Teacher/Mac Student/Windows x64
Student+Guard; embedded fresh ZIP manifests/hashes and Windows executables verified
byte-for-byte against loose builds. Tracked Flutter lock restored; local resolved
lock retained. Exact replacement paths/processes checked stopped before build;
owner privacy grants unchanged, new bundle capture worked without regrant.
32focused checks: ingress5, transport23, decode-health4passed. All-bin clippy
completed with existing warnings plus one new receive-function length warning
(112lines); no lint errors. Scoped formatting, Python compilation, diff checks
complete. Test source enabled only by explicit flags; production half-budget retry
and fault injection off remain unchanged.

## First A/B pair

Mac192.168.50.206 → Windows VM192.168.50.215, x64 emulation on ARM;1512×982,
50second audited foreground five-phase workload. Each run has one clean session,
exactly two controlled drops and accepted third copy. Raw folders:
 target/controlled-repair-half-1-20261007 and target/controlled-repair-rtt-1-20261007.

| Metric | Half-budget | RTT opt-in |
| --- | ---: | ---: |
| First repair dropped → second repair received |129.592ms|29.116ms|
| Original dropped → second repair received |151.531ms|51.371ms|
| Accepted copy → first decode log event |32ms|41ms|
| Decode FPS |29.37|29.18|
| Render FPS |28.89|28.18|
| First decode (receiver elapsed) |310.04ms|192.34ms|
| Decoder errors |0|0|
| Post-first-output zero intervals |0|0|

Specific measured repeated-gap wait reduced100.476ms in first pair; this is
single-clock ingress repair timing, not a100ms optical improvement or global
FPS gain. No raw video frame identity mapping was logged for the selected packet.
Further repetition and Teacher/network/load evidence follow below.

## Repetition half-budget2

Raw target/controlled-repair-half-2-20261007: same selected-packet identity across
three copies, original and first repair dropped, third accepted. Clean session,
functional decode/render, foreground/visible100%, all phases. First repair drop
→accepted copy132.989ms, original→accepted155.037ms; accepted→first decode log37ms.
First decode292.30ms, decode29.06/render28.49FPS, decoder errors0, no post-first
output zero intervals. Decoded recovery peak689.53ms is an overall-run metric,
not the selected packet's repair delay. Remaining RTT repetition follows below.

## Second RTT repetition and final outcome

Raw target/controlled-repair-rtt-2-20261007. One clean session, exactly two drops
and one accepted third copy of the selected identity. First repair→accepted24.687ms,
original→accepted46.427ms, accepted→first decode log25ms; first decode191.415ms,29.52decode/28.65renderFPS, decoder errors0,
post-first-output zero intervals0. Overall decoded recovery peak190.954ms.

| Repeat | Half-budget repair-copy wait | RTT repair-copy wait | Difference |
| --- | ---: | ---: | ---: |
|1|129.592ms|29.116ms|100.476ms|
|2|132.989ms|24.687ms|108.302ms|

Mean repeated-gap wait131.291ms vs26.902ms, difference104.389ms (two trials per
policy; not a statistical population estimate). Measured from dropped first
repair arrival to accepted second repair, using one receiver ingress clock per
trial. This directly exercises the targeted repeated-gap branch on native Windows
APIs. It is not glass-to-glass latency, proof of selected-frame output identity,
or a general throughput improvement. Accepted-copy→first decode log25–41ms;
all four runs decoded/presented and kept about28–29renderFPS with no decoder
errors or one-second post-first-output zero intervals.

All four Teacher summaries: oneencoder,6Mbps target,0adaptive bitrate changes,
29.21–29.73encodeFPS, actual1.85–2.35Mbps. NACK requested/resent38/61/91/72,
cache misses/send errors/abandoned0. Additional natural/expired sequence counts
were128/58/0/1 (see raw per-run summaries); zero intentional random loss is not
zero network loss. Controlled identity copies and two injection drops are verified
separately from these background counters. Workload all five phases, foreground/
visible100%. Comparison JSON: target/recovery-budget-probe/controlled-repair-comparison.json.

Decision: RTT retry now has reproducible evidence of about100ms earlier packet
repair when the first retransmission is lost and RTT is low. Keep it opt-in for
now: previous1% random-loss tests remain mixed, and missing leading SPS/PPS after
reference reset is a different failure that earlier second NACK cannot solve.
No production queue/expiry/keyframe policy changed by this milestone. Test-owned
Teacher/helper, Windows tasks/processes cleaned, installed Student preserved.

## Exact next steps / physical lab checklist

1. On the Mac/VM, isolate loss of the leading parameter packet after reference
   reset and measure request→IDR→decode→presentation. Test bounded packet repair/
   prefix protection before changing the shared Teacher gate; keep strict recovery
   validation and latest-frame-wins. Investigate the earlier cold-start liveness
   timeout independently, without hiding it with a larger timeout.
2. On physical Windows x64, repeat this same controlled original+first-repair loss
   with hardware/software MFTs, fixed workload/quality and varying RTT/CPU load.
   Verify125→25ms timing at low RTT, appropriate longer delay at high RTT,
   unchanged250ms expiry and one retry. Optical latency must be measured separately.
3. Wired classroom1/5/10/20/35/40: compare repair bursts/NACK/cache misses, receive
   drops/decode/render, Teacher aggregate utilization and repeated joins under
   simultaneous faults. Do not enable RTT as production default based on a shared
   Mac/ARM VM test alone. Fixed1080p30/6Mbps content, readable text/scroll/motion,
   fixed-resolution soak, DPI125/150%, real sleep/resume remain deferred.
