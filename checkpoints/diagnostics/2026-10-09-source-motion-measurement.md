# Student-source motion measurement — 2026-10-09

## Scope and environment

Owner requested measurement of FPS/pipeline freshness on existing builds, not a
new product feature or app rebuild. Teacher remains the signed bundle at
`target/source-control-fix-build/Classroom Console.app`, LAN192.168.100.171:48900.
Mac Student remains `/Users/lolem/Classroom-source-switch/Classroom Student.app`,
192.168.100.254. No privacy grants, signing, app source changes or display-mode
changes during measurement.

The original Windows192.168.100.102 repeatedly stalls SSH and is intermittently
marked offline by Teacher. Owner attributes this to a weak/lagging machine, then
provides `lolem@192.168.100.155` and explicitly closes Student on .102. Do not infer
a physical network disconnection from the offline/timeout observations alone.

New .155 is Windows11 Pro ARM, four vCPUs, approximately6 GiB RAM, Parallels
VirtIO Ethernet. Bootstrap fresh embedded x64 Student ZIP, version
`paired-source-control-fix-20261009`, executable SHA256
`c093cd43f7eb87fcb83cebf5f63db5a9239a70e3baf86faa6cb5cd79d711661f`.
No existing Classroom process/task/service was found before deployment. Verified
replacement checksum before running old-install migration (nothing to remove).
New retained app:
`C:/Users/lolem/Classroom-Source-Measure-Student-1791524588/ClassroomStudent.exe`.
Its interactive task targets171/48900; inbound rules are scoped to Teacher171.
Defender realtime remains enabled. No production installer/service added.

Mac Student host Mac16,12 has10 logical CPUs/16 GiB RAM and runs this same Windows
guest: guest NIC00-1C-42-34-61-77 matches the Mac's Windows11 VM configuration.
Source and class recipient therefore share CPU/GPU/memory/network resources in
both directions. This is a diagnostic mixed native/ARM-VM workload, not an
independent physical Windows x64 Ethernet test or evidence for35–40 PCs.

## Method and workload validation

New reusable native tools:

- `scripts/source-motion-workload.swift`: Cocoa windows/text, bounded lifetime.
- `scripts/source-motion-workload.ps1`: WinForms windows/text; hidden PowerShell
  host, no terminal foreground requirement and no ExecutionPolicy change.
- `scripts/measure-student-sources.py`: current-role IPC, continuous state reader,
  owned interactive workload task/job, full logs, finite duration, final Stop.
- `scripts/summarize-source-measurement.py`: audit/phase filtering and interval
  summaries, missing-sequence/recovery counter deltas rather than cumulative sums.

Each source140s: two70s cycles; seven10s phases: static text, typing, scrolling,
terminal output, color/text motion, moving a window, switching two workload
windows. Switching third-party applications is not part of this workload. Same
phase timing/content/update target, native rendering differs; not pixel-identical.
Target20 visual updates/s, fast timer plus monotonic latest-state admission.
No old animation ticks are replayed when the workload timer is delayed.

Initial setup attempts with .102 produce no valid workload measurements: first
automation client stopped draining state during SSH copies and hit Teacher's
bounded write timeout (Teacher itself stayed alive); fix only the harness with a
dedicated state reader. Second attempt fails during Windows SSH setup. Evidence
kept in `target/source-motion-measure-20261009` and
`target/source-motion-measure-live-20261009`; neither is a performance pass.

Exploratory .155 run `target/source-motion-155-20261009` completes but Windows
50ms timer actually updates~15Hz and Mac scroll operation changes little visible
content. Exclude its FPS table from the matched conclusion. Instrument scroll
position and correct only the workload cadence/document viewport, then repeat.

Accepted confirmation: `target/source-motion-155-confirm-20261009`.
Windows140/140 foreground audit samples; Mac139/140, nonforeground intervals
excluded. Median update rate Windows20.000, Mac19.920Hz. Scroll positions18/20
distinct across phase audit samples, proving actual viewport movement. Every
phase appears20 times in each source's audit. Source/output sizes measured at
registration: Windows1468×922, Mac1470×956. No claimed fixed1080p run.

Use phase seconds2.5–8.5, require visible/foreground audit, align receiver clocks
by bounded UTC query midpoint. Cross-host phase alignment bound~1030ms; guard
excludes transition intervals. Usually12 source/receiver intervals per phase
(Windows scroll10, switch11 source intervals). Values below are **medians of
interval telemetry**, not pooled frame percentiles or optical latency. Encode
p95 is summarized as median interval-p95, never described as a whole-run p95.

## Accepted results

| Source | Phase | Capture FPS | Encode FPS | Class recipient decode FPS | Class recipient render FPS | Teacher Control preview render FPS |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Windows | Static | 1.89 | 17.44 | 16.98 | 16.99 | 17.00 |
| Windows | Typing | 39.93 | 20.70 | 20.97 | 18.99 | 18.99 |
| Windows | Scroll | 40.24 | 20.39 | 20.49 | 19.00 | 19.49 |
| Windows | Terminal | 39.85 | 20.33 | 20.49 | 18.99 | 18.99 |
| Windows | Motion | 35.90 | 24.96 | 24.99 | 22.97 | 22.93 |
| Windows | Move window | 21.62 | 29.05 | 28.97 | 25.00 | 24.99 |
| Windows | Switch windows | 11.62 | 22.28 | 22.48 | 20.50 | 20.98 |
| Mac | Static | 0.00 | 21.71 | 2.99 | 1.99 | 19.46 |
| Mac | Typing | 19.99 | 29.20 | 8.97 | 4.99 | 26.48 |
| Mac | Scroll | 20.11 | 28.96 | 4.96 | 3.48 | 24.99 |
| Mac | Terminal | 19.81 | 29.25 | 6.98 | 3.99 | 24.99 |
| Mac | Motion | 20.19 | 28.74 | 2.99 | 2.49 | 24.98 |
| Mac | Move window | 19.91 | 29.19 | 3.99 | 2.99 | 25.35 |
| Mac | Switch windows | 1.96 | 27.56 | 8.99 | 5.96 | 23.99 |

Capture counters are source capture-stage reports, not transmitted FPS. Encoder
is the admitted transmitted stream, capped30. Static desktop needs few newly
captured frames; repeated submissions explain encode exceeding fresh capture.
Do not present low static capture FPS as a fault.

### Pipeline findings

- Windows source: encode interval-p95 medians~7.9–12.1ms during nonstatic phases;
  target30, no reported encode pressure in accepted phase samples. Class receiver
  and Teacher preview decode follow encoder rate. Render loses~1–4FPS relative
  to decode under some phases; freshness/render scheduling remains measurable.
- Mac source: capture API interval-p95 medians~3.8–4.8ms; encode~11.7–12.3ms,
  no reported encode pressure. Source produces~28–29FPS under motion/text;
  Teacher preview receives~28–30 decode/~24–26 render. Windows class recipient
  collapses to~2–6 render. The source encoder is not the observed bottleneck.
- Windows recipient during Mac source: missing_packets counter delta1197,
  late_packets457, stale_backlog_packets2025, recovery_episodes102,
  unrepaired_gap_resets104, keyframe_requests337. Logged session recovery peak
  ~3.4s. No decode/depacketizer errors, UDP ingress dropped/socket/wrong-peer
  counter deltas0. Decoder interval-p95~11–15ms; low output accompanies repeated
  recovery/await-keyframe, not evidence of a thrown codec failure.
- Mac recipient during Windows source: missing counter delta1, late1, no stale
  backlog/recovery episode deltas. Logged recovery peak~120ms. These counter
  values are not physical-network packet-loss percentages.
- Source send errors0 in both directions, but Teacher relay logs deadline errors
  2/4 respectively. Zero source send errors does not prove successful delivery.
  Reordered counters are large even on the healthy Mac receive path, so the
  counter alone does not establish network reordering as the cause.
- Source encoder instances1; Teacher relay encoder instances0. Existing capture
  once/encode once/packet forwarding invariants remain; no media queues enlarged.

## Interpretation and exact next investigation

The accepted run reproduces a severe Mac-source→Windows-class-recipient recovery
problem while the Teacher's direct Control preview remains healthy. Prioritize
Teacher relay→Windows ingress/depacketization/freshness recovery, including shared
VM host effects. Do not conclude all classroom Wi-Fi/NICs or the Mac encoder are
faulty, or that this result represents physical Windows x64 PCs.

Code inspection confirms `apps/classroom/src/student_broadcast.rs` already drains
up to256 source UDP packets per iteration, then sleeps1ms per batch, not per
packet. There is no evidence for a one-packet-per-millisecond bottleneck or a
reason to replace its bounded batching blindly. Measure relay residence, batch
size, pacing/deadline failures and burst delivery before changing this path;
retain bounded queues/latest-frame policy, packet cache/NACK and no re-encoding.
Also verify source-versus-relay timestamp/clock-domain treatment rather than
assuming the sender's capture clock equals the relay's control clock.

Next authorized decision should select relay/receiver diagnosis: identical
workload with UDP receive/depacketize sink without decode, then decode enabled;
compare ingress, AU completion, late/stale discard, recovery and host load. This
separates decoder/VM pressure from arrival/repair timing. Any subsequent product
patch requires an unchanged-workload A/B run. No product performance fix or
claimed FPS improvement has been made in this measurement milestone.

## Completion and evidence

Both140s confirmation runs complete; no source crash/disconnect or control error.
Owned workload tasks/jobs removed; final Teacher idle/error:null, Mac254 and
Windows155 online, old Windows102 offline as requested. Student155 task/app and
Teacher-scoped app firewall rules retained for owner testing; Defender realtime
still enabled. Compilation: Swift standalone workload; Python py_compile; diff
whitespace check clean. Classroom Rust/Flutter unchanged, no app rebuild/sign.

Evidence folder contains manifest/hashes/clock bounds, full Teacher/Student and
direct preview logs,140-sample workload audits, state/events, summary.json/md,
cleanup.json (empty), Windows final inventory, shared-host NIC match and final
idle state. Raw logs remain local; report is the durable Git checkpoint.

Deferred: actual app-switch/DPI/sleep cases,30-minute soak, fixed1080p optical
latency and1/5/10/20/35–40 physical Windows x64 Ethernet ladder. Do not describe
this two-endpoint/shared-host measurement as Phase1 performance acceptance.
