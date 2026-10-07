# RTT-bounded second NACK experiment — 2026-10-07

## Evidence and proposed change before implementation

Current visible loss1% run at1512×982 on shared Mac/Windows ARM VM had two
post-first-output zero-decode intervals and1674ms peak recovery. Both gaps were
waiting for reference recovery; two incomplete parameter-set AUs rejected.
Teacher send errors, cache misses and abandoned packets were0, decoder errors0.
No unique cause for the complete delay is proven. Teacher gate widened to800ms;
missing leading parameter packets are a separate limitation.

Existing JitterBuffer retries one open gap halfway through its repair budget.
During IDR recovery this budget is250ms, so a second NACK waits125ms even when
measured RTT is about1ms and the computed RTT-aware repair floor is25ms.
Candidate: cap that existing second-request delay at the RTT-derived repair
budget, still at most half the gap deadline. Normal25ms repair keeps12.5ms retry;
IDR250ms repair on low RTT retries at25ms. High RTT retains a larger delay.
Keep exactly one retry, identical deadline/cache/queue/keyframe gate and codec.
This is a targeted recovery timing experiment, not a redesign or larger buffer.

Plan: focused timing/deadline tests, paired Teacher+Student build, A/B120second
loss1% cases on the same fresh production tree with fixed seed12345 and audited
five-phase workload. Test-only receiver override preserves prior half-budget
retry for control. Run a normal baseline. These runtime branches must be measured
before claiming improvement; random seed does not guarantee identical packet
positions when scheduling/encoded output changes. Physical x64 wired35/40 gates
remain unverified.

## Implementation/checks/build

JitterBuffer now accepts an optional retry ceiling, clamped to at least1ms and
at most half the existing deadline. At most one retry remains. Receiver experiment
uses its existing RTT-aware repair budget as ceiling; logs nack_retry_after_us.
Three focused regressions cover early repair when first retransmission is lost,
unchanged250ms expiry/one-retry ceiling, normal12.5ms timing and high RTT125ms.
media-transport23tests and decoder-health4tests passed; all-bin check and clippy
completed with existing warnings; scoped formatting/Python/diff checks complete.

First paired label early-nack-20261007 built Mac Teacher/Student, Windows x64
Student+Guard and both embedded ZIPs. Embedded manifests/hash checks passed;
Windows embedded executable matches loose executable byte-for-byte. Tracked
Flutter lock restored, build-resolved copy retained in ignored target. Manual
privacy grants were not changed; same Mac parent/helper path worked after rebuild.

## A/B runtime so far

Mac192.168.50.206 → Windows ARM VM192.168.50.215, x64 emulation. Same fresh
binaries, config,120-second Teacher share, seed12345, audited five-phase workload
foreground/visible100%;1512×982, adaptive quality enabled with initial6Mbps target.
Raw target/early-nack-control-20261007 and target/early-nack-rtt-20261007.

| Metric | Prior half-budget control | RTT-derived candidate |
| --- | ---: | ---: |
| Recovery retry timing (logged) |125ms|25ms|
| Normal retry timing |12.5–14.527ms|12.5ms|
| Injected peer-datagram loss |0.9009%|0.9838%|
| Decode FPS |28.27|27.26|
| Render FPS |26.57|25.83|
| Decoded recovery peak |889.02ms|941.95ms|
| Post-first-output zero intervals |0|0|
| Decoder errors |0|0|
| NACK sequences |444|319|
| Incomplete recovery AUs |0|0|
| Watchdog resets |0|0|

Both decode/rendered successfully, Teacher send errors/abandoned/cache misses0.
Teacher encoded stream2.31 vs1.67Mbps, ending target5.808 vs4.224Mbps (three
adaptive changes each). Control had a cold-start liveness timeout and reconnect:
registration unix_ms1791379522611, timeout9527605, receiver ready9527994; second
client produced first output9529608 and completed113.36seconds. Candidate had one
clean session. Control duration/quality differences and timing-dependent packet
positions prevent a clean causal performance claim. Same random seed does not
make actual encoded workload or packet losses identical. Neither case reproduced
previous incomplete-parameter gaps. Lower NACK count alone is not proof of faster
recovery; candidate has no demonstrated runtime improvement.

Decision: do not promote RTT retry to production default. Final source exposes
CLASSROOM_TEST_NACK_RETRY_RTT=1 as disabled-by-default diagnostic experiment;
normal receivers retain original half-budget timing. Runner understands both
old candidate inverse HALF_BUDGET hook and final opt-in RTT hook so raw evidence
is reproducible. Native baseline/new final paired package verification follows.

## Candidate baseline and final package

Candidate RTT baseline120seconds:28.76decode/27.61renderFPS,0decode errors,
0post-first-output zero intervals,597.53ms decoded recovery peak, missing48.
Single clean session, five phases foreground/visible100%. Raw
 target/early-nack-baseline-20261007. Functional pass, not a speedup claim.

Rebuilt paired final label nack-retry-experiment-20261007 from the opt-in source:
Mac Teacher/Mac Student/Windows x64 Student+Guard, fresh embedded ZIPs.
Manifests/hash checks passed, embedded Windows Student byte-identical to fresh
loose executable. Restored tracked Flutter lock, retained build-resolved lock.
Test-owned app/helper stopped and exact paths checked before replacing bundles;
settings/installed Student untouched. No privacy grants automated. Final default
policy50second loss smoke reached Share but failed with ScreenCaptureKit TCC
“No shareable content available…user declined TCCs”. No decoded/presented output
claim for final bundle. Raw target/nack-retry-final-default-20261007 retained;
runner stopped/quit isolated Teacher and cleaned Windows task/process. Owner
asked to manually regrant Screen & System Audio Recording to exact parent/helper
paths and told restart is required. AGENTS.md prohibits privacy automation.

Exact targets:
- /Users/mvmanh/Development/netsupport-school/target/local-recovery-build/Classroom Console.app
- /Users/mvmanh/Development/netsupport-school/target/local-recovery-build/Classroom Console.app/Contents/Helpers/Classroom Agent.app

The planned regrant/restart step was subsequently completed below: both final
policies decoded/presented and emitted their correct markers without another
build. Earlier candidate half-budget control had a cold-start reconnect; final
policy trials each had one clean session.


## Deferred / next experiment

- Controlled native loss of one middle-IDR packet and its first retransmission,
  compare first/second NACK timestamps and repaired output on both policies.
  Timing regressions simulate this already; actual ingress hook is not yet added.
- Repeat fixed-quality A/B with identical foreground workloads, independent runs
  and cold-start/session events recorded. Current adaptive targets diverged.
- Correlate incomplete leading SPS/PPS anchors separately: an earlier second NACK
  cannot repair an unknown leading sequence. Keep strict recovery validation.
- Investigate one cold-start5second control liveness timeout separately; do not
  enlarge the timeout to hide it or attribute it to NACK timing without evidence.
- Physical Windows x64 wired1080p1/5/10/20/35/40 ladder, optical latency,
  fixed-resolution soak, DPI125/150% and sleep/resume remain pending lab access.
  No redesigned Teacher, larger video queue or published asset replacement.

## Final default runtime after owner regrant

Owner granted both paths, same bundle restarted without signing/rebuild. Raw
 target/nack-retry-final-default-regrant-20261007.50second1% loss, measured1.0348%,
1512×982, audited five phases foreground/visible100%. Exactly one clean receiver
session. Log confirms policy=half-budget and recovery retry125000us.
1200decoded frames,24.31decode/23.22renderFPS, decode errors0, incomplete recovery
AUs3, watchdog resets0, two post-first-output zero intervals, decoded recovery
peak2689.25ms. Final missing1221, NACK requested143. Teacher oneencoder,
29.22encodeFPS,2.40Mbps actual stream, target ending3.84Mbps/24FPS after two
adaptive changes, send errors/abandoned/cache misses0;143retransmissions.
Functional runtime/default policy verified; smooth-loss acceptance still fails.
This is another variable-quality short run, not evidence the old policy is
causally worse than the candidate120second trial. The leading-parameter recovery
problem remains a priority. Final opt-in switch runtime verification is recorded below.

## Final opt-in runtime / conclusion

Raw target/nack-retry-final-rtt-20261007. Same final build,50second loss1%,
measured1.0294%, one clean session. Explicit experimental marker and policy=rtt;
recovery retry25000us.1426decoded frames,28.73decode/27.32renderFPS, decoder
errors0,0post-first-output zero intervals, incomplete AUs0, watchdog resets0,
recovery peak1259.33ms, missing262. Teacher29.66encodeFPS, oneencoder,
2.31Mbps actual stream,6Mbps target retained (0bitrate changes); NACK163,
send errors/abandoned/cache misses0. All five phases foreground/visible100%.

The50second final RTT trial looks better than its default-policy trial, while
120second candidate comparison did not. Default50second trial adapted down to
24FPS/3.84Mbps and had3incomplete AUs; RTT kept30FPS/6Mbps and had none.
These different loss positions/quality reactions preclude attributing the result
solely to retry timing. Evidence is promising but inconsistent. Keep RTT opt-in,
verify native controlled first-repair loss and fixed-quality repetitions before
promoting. Both final policies decode/present and log their correct timers;
no smooth-loss or35/40 wired acceptance claimed. Test-owned Windows tasks/processes
and Mac Teacher/helper cleaned; installed Student/profile/logs preserved.

## Controlled repair follow-up completed

The proposed native middle-IDR plus first-repair-loss experiment is now implemented
and repeated twice per policy. See2026-10-07-controlled-repair.md: half-budget
129.6/133.0ms vsRTT29.1/24.7ms for repair-copy wait; all decoded/presented without
codec errors. This closes that targeted experiment, not general random-loss or
physical35/40 acceptance. RTT remains opt-in; leading parameter loss and cold
startup diagnosis remain separate next steps.
