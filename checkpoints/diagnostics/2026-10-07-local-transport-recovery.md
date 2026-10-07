# Local transport / Windows VM recovery — 07-10-2026

## Scope

Owner authorized coding and local testing after leaving the physical lab. Source baseline f865830.
Mac Teacher 172.20.10.2 and Windows 11 ARM64 Parallels Student 172.20.10.3;
Windows x64 binary runs via emulation. This is not Windows x64 physical Ethernet
acceptance, not 1080p/35/40-PC validation, and not optical latency measurement.

Added diagnostic-only code: `apps/classroom/src/bin/transport_probe.rs`,
`scripts/run-transport-probe.py`, `scripts/run-local-windows-recovery.py`,
`scripts/analyze-local-recovery.py`. Production capture/encode/transport/recovery
algorithms and default config remain unchanged.

## Transport probe

The Rust executable directly imports production Fanout/AggregatePacer. Synthetic
AU datagrams carry frame/index/count for sink accounting; they are not valid H.264/RTP.
One generated packet array is fanned out; no decoder, capture or classroom control.
Effective encoded-work delay defaults to simulated 13 ms, normal AU 25,000 bytes,
IDR 361,003 bytes, 30 FPS goal; four workers, shared 400 Mbps governor, 120/300 ms
normal/IDR deadlines. Local sinks record unique fragments and complete/partial AUs.
Additional synthetic traffic uses the same aggregate governor; it is not a replay
of production NACK scheduling. Default probe uses shared send_to; --connected
selects the connected socket path. Loopback receives do not validate NIC behavior.

Nine matrix cases cover 1/5/10/20/35/40 sinks, oversized normal/IDR AUs and auxiliary
traffic. In the release matrix all socket-accepted datagrams were received by
sinks. At 35 sinks, all 60 AUs completed, one IDR completed in 269.25 ms; submission
26.11 FPS over 2.30 s includes startup IDR and simulated encoding.
At 40 sinks, the same fixed IDR was rejected for every job: 0 packets sent,
2,400 recipient-AU preflight drops. Required wire time is about 308.07 ms:
(361003 + 301*80)*40*8 / 400000. At 35 it is about 269.56 ms.
A deliberately 600 KB IDR is also rejected at 35; an oversized normal frame can
trigger repeated recovery requests. The probe immediately honors recovery with
a fixed-size IDR and does not model the production IDR gate/adaptive bitrate;
this does not prove the live app can never recover at 40. It proves AU-size
admission and recovery need to be evaluated together.

Early debug runs overlapped builds and showed more deadline failures. Cadence
is sensitive to same-host scheduling; do not compare those runs as improvements.
300-frame runs with a 200 KB IDR at 40 received all 300 AUs per sink. Final quiet
repeats are recorded below, separately from the early runs.

### Final focused repeat after builds and integration stopped

| Probe (300 submitted AUs) | Submission FPS | Complete AUs per sink | Abandoned recipient packets | Recipient AU deadline drops | WouldBlock |
|---|---:|---:|---:|---:|---:|
| quiet35 | 27.09 | 300–300 | 0 | 0 | 0 |
| quiet40-small-idr | 25.74 | 300–300 | 0 | 0 | 0 |

These are serialized synthetic encode/send timings on a shared host, not actual codec FPS.

## Mac Teacher to visible Windows receiver

Direct raw CLI first failed ScreenCaptureKit permission. A sandbox-inherited
helper cannot be launched directly: it must be parented by Flutter. Owner manually
granted Screen Recording for both the exact helper and parent bundle, then the
GUI/helper were restarted. No privacy grants were changed by automation.

Exact parent: `/Users/mvmanh/Development/netsupport-school/target/local-recovery-build/Classroom Console.app`.
Exact helper: same parent plus `/Contents/Helpers/Classroom Agent.app`.

With connected_fanout=true the sandboxed Mac publisher encoded but connected UDP
send returned Broken pipe (OS error 32); Windows received zero frames. Do not mark
that run successful merely because the Student process exited zero.
A diagnostic config A/B using connected_fanout=false successfully sent/decoded/rendered.
The stored settings file was backed up, changed only during startup, and restored
byte-for-byte immediately after startup; hash-equivalent contents checked afterward.
This isolates the tested send path/config; a kernel/sandbox trace is still needed
for the underlying Broken pipe cause. It is not evidence explaining Windows lab loss.

Ports 48900/48910/48911, temporary Windows root/profile and Interactive Limited
scheduled tasks isolate the test. Installed Student/Guard binaries were not replaced.
All three successful streams were 1512x982 (no upscale), one VideoToolbox H.264
encoder, D3D11 renderer on Windows. Each used the native 50-second workload script;
foreground content was not independently audited, so this is functional evidence,
not a controlled high-motion stress benchmark. Actual encoded average was
0.63 / 0.79 / 1.60 Mbps, not a sustained 6 Mbps output.

| Scenario | Decoded / rendered | Decode / render FPS | Decode errors | IDR-arrival recovery peak ms | First decode→first present log gap ms |
|---|---:|---:|---:|---:|---:|
| baseline | 1349 / 1330 | 27.48 / 27.09 | 0 | 143.15 | 119 |
| loss | 1438 / 1391 | 28.99 / 28.04 | 0 | 144.17 | 9 |
| latejoin | 1204 / 1169 | 28.21 / 27.39 | 0 | 110.05 | 157 |

All have zero one-second zero-decode intervals and close preview on Stop.
Loss case sets existing CLASSROOM_TEST_DROP_PERCENT=1; it is probabilistic,
not an exactly measured physical 1% loss. The current log omits the dedicated
injection counter, so do not assert its exact achieved percentage. NACK requested/
resent sequences: baseline 39/39, loss 95/95, latejoin 21/21; all Teacher cache misses
and repair send errors zero. Local ingress drops: 0/7/33, separate from injected loss.
Latejoin delays Student registration by configured 8 seconds; Teacher starts before
registration. Recovery peaks end at acceptable IDR arrival, not visible rendering.
First-present_ms was 1196.75 / 490.91 / 1053.09 ms from preview-owner timing;
these differ from first-decode_ms origins and are not end-to-end network latency.

## Native Windows codec smoke

A separate 20-second Interactive Limited task ran local-media-benchmark on the VM:
DXGI, software H264 Encoder MFT, Microsoft H264 Video Decoder MFT and D3D11.
Actual 1510x948; 355 encoded/decoded/rendered frames, 17.75 FPS; encode p95 18.41 ms,
decode p95 9.67 ms, render p95 0.92 ms, local pipeline p95 28.14 ms;
177 schedule overruns. This is functional Windows backend validation under ARM
emulation; no 30 FPS pass or hardware Quick Sync/physical x64 claim.

## Build, checks and cleanup

Paired build label local-recovery-20261007: Mac Teacher, Mac Student, Windows x64
Student/Guard and fresh ZIPs embedded in Teacher. Windows diagnostic Classroom
also rebuilt from the same source/label. Toolchain required explicit rustup rustc/
rustdoc paths; Homebrew rustc otherwise could not find Windows core. Flutter build
required approved access to SDK cache outside workspace. Local older Flutter SDK
resolved six transitive versions differently; the exact resolved lockfile is retained
in target/local-recovery-build/pubspec.resolved.lock, tracked lockfile restored.

31 existing focused tests passed (11 imported sender/fan-out, 20 media-transport),
one manual probe ignored. Harness clippy completed with no harness warnings after
cleanup; existing dependency warnings remain. Scoped rustfmt, Python compile and
git diff --check passed. Workspace cargo fmt --all -- --check fails on pre-existing
unrelated formatting starting apps/classroom/src/activity/collector.rs; no broad rewrite.

Temporary Teacher GUI/helper and all scheduled tasks/test Students stopped.
Final Windows inspection retained original Guard PID 3248 and installed Student
PID 5456 at C:\Program Files\Classroom. No firewall policy changes.
Raw evidence stays ignored under target/transport-probe, target/local-recovery-*
and target/local-windows-codec-20261007; failed attempts retained.

## Next step

Prioritize a bounded, measured AU-size/adaptive/recovery policy: explain what happens
when an IDR itself cannot fit the recipient wire budget, and distinguish preflight
rejection from actual-send deadline and receiver reference skips. Do not simply
increase buffers or claim the synthetic 40-sink result is physical acceptance.
Investigate connected UDP Broken pipe separately before default Mac broadcast tests.
Add injection-count/visible-recovery phase coverage if improving test telemetry.
At the lab, replay matching text/scroll/motion workloads at actual 1080p and
1/5/10/20/35/40 physical Windows x64 PCs; record optical latency and long soak.

## Reproduce diagnostic checks

```sh
cargo build -p classroom-app --release --bin transport_probe --offline
python3 scripts/run-transport-probe.py --binary target/release/transport_probe --output target/transport-probe/new-run
cargo test -p classroom-app --bin transport_probe --offline
cargo test -p media-transport --offline
```

After starting the paired Teacher bundle on isolated port48900 with its own
`--ipc-file`, run (password prompted, never saved):

```sh
python3 scripts/run-local-windows-recovery.py --remote mvmanh@172.20.10.3 --local-ip 172.20.10.2 --seconds 50 --endpoint target/local-recovery-console-endpoint.json --output target/new-recovery-run
python3 scripts/analyze-local-recovery.py target/new-recovery-run
```

The endpoint must belong to the isolated test Teacher: the runner stops/quits that
Teacher during cleanup. Output directory must be new. The successful current
Mac test used connected_fanout=false; the restored product config remains true.
Configured peer addresses must be changed when the local network changes.
