# Windows Flutter Teacher, Mac + Windows ARM Students over Wi-Fi

Outcome: integration works, two concrete Windows defects fixed. Windows ARM
reception over this Wi-Fi/Parallels topology does **not** meet smoothness or
recovery acceptance. These are diagnostic measurements, not 40-PC acceptance.

## Environment and deployment

- Teacher: 192.168.50.94, Windows 11 Parallels ARM64 host, 4 CPUs / 8 GB;
  x64 Flutter console + x64 Rust agent under emulation, 1920x1080 DXGI,
  software H264 Encoder MFT. One encoder serves both recipients.
- Mac Student: 192.168.50.206, MacBookPro18,3 / 16 GB, macOS 26.6.2,
  native arm64 build of checkpoint 32f02a5, VideoToolbox receiver.
- Windows Student: 192.168.50.205, MAIVANMANH2172, Windows 11 Pro build
  26200 / about 6 GB, native ARM64 Rust, MF H.264 decode / D3D11 render.
- Important topology qualification: `prlctl exec 'Windows 11' hostname` on
  Mac .206 returns MAIVANMANH2172. Thus both Students share that Mac's
  physical CPU/network. Three OS endpoints do not mean three physical PCs.
- Control/media/feedback: 48700/48710/48711; monitoring 48701.
  Neither firewall nor macOS privacy/security settings were changed by scripts.
  Initial Mac connection briefly reported No route to host, then registered;
  the shell TCP probe succeeded. No sustained control-connect fault reproduced.
- Mac app: `/Users/mvmanh/Classroom-windows-teacher-test/Classroom Student.app`.
  Executable: `Contents/MacOS/ClassroomStudent`. Announced before launch;
  ad-hoc integrity verified. Screen capture and native input permission were
  subsequently confirmed by actual capture/Enigo logs, without a TCC bypass.
  SHA256: be3d6e06833614714d81830dae66d9eebb9448a7057b98bb499824a01348448b.
- ARM Student: `C:\Users\mvmanh\Classroom-windows-teacher-test\ClassroomStudent.exe`.
  SHA256: E7E815FB09D770DB0D8D2A981BE90BA671C18BAFAC2D581C83546754040A9F9D.
  Final x64 Teacher agent SHA256:
  EF83DFCC7B769943CD525A2B3547460B2FBCE36B6DD65CF647EA7442CE29A930.

## Defects fixed and executable verification

1. Windows tray popup suspended the Teacher coordinator inside the native
   TrackPopupMenu nested loop. Media continued, but Stop/IPC state/Observe
   lease processing stopped. Closing the popup with Escape immediately restored
   processing. The first controller consequently reached its read timeout.
   Windows Teacher tray now owns a separate message-pump thread, a single
   latest-status slot and an 8-command channel. All native objects stay on that
   thread; shutdown posts WM_CANCELMODE only to its windows and joins it.
   macOS retains its required main-thread UI pump.
   Verified with actual popup open: Stop 1.071 ms, Share Selected 1.078 ms,
   then Quit canceled the popup and the agent exited. Another run used
   GetGUIThreadInfo to verify menu mode, with Observe still live and native
   sequence advancing 339 -> 904, rescan acknowledged, then graceful Quit.
2. Fresh Windows ARM failed before logging with 0xC0000135: MSVC executable
   imported VCRUNTIME140.dll, which was absent. Copying the matching Microsoft
   ARM64 redistributable DLL app-local made it start and connect. Its SHA256
   matches the locally packaged CRT. No machine-wide runtime installer was used.
   Student packaging now reads PE architecture and includes matching VC CRT
   DLLs; Flutter x64 packaging also includes CRT DLLs and rejects ARM64 helpers.
   ARM64 and x64 Student bundles were successfully packaged; updated x64 Teacher
   ran subsequent LAN tests with app-local DLLs.

## Broadcast measurements

Means include deliberate static sections and time after the 60-second workload
closed. Rows have different durations, so the Wi-Fi profile comparison is
directional evidence, not a controlled causal benchmark.

| Run / receiver | Duration | Decode / render FPS | Decode p95 | Missing packets | Depacketizer errors | Keyframe requests | Peak recovery |
|---|---:|---:|---:|---:|---:|---:|---:|
| One Student: Mac | 65.93 s | 23.57 / 20.75 | 12.61 ms | 0 | 0 | 1 | 565.58 ms |
| Two Students, default: Mac | 104.06 s | 17.26 / 15.05 | 12.74 ms | 0 | 0 | 1 | see raw log |
| Two Students, default: ARM Windows | 102.27 s | 8.10 / 7.00 | 43.66 ms | 119 | 56 | 95 | 7399.76 ms |
| Two Students, Wi-Fi profile: Mac | 87.23 s | 20.02 / 18.14 | 11.31 ms | 0 | 0 | 0 | 325.16 ms |
| Two Students, Wi-Fi profile: ARM Windows | 87.15 s | 13.05 / 10.65 | 24.55 ms | 56 | 28 | 61 | 4638.18 ms |

- All rows had zero decoder errors; depacketizer failures are recorded separately.
- Teacher encode p95: 19.14 ms (one Mac), 19.22 ms (two default),
  18.90 ms (two with ARM Wi-Fi profile). Encoder_instances=1, send_errors=0,
  repair_send_errors=0 throughout completed broadcast runs.
- Two default: 629 retransmitted packets, two cache misses; source 1.709 Mbps,
  aggregate 3.511 Mbps. ARM had 5172 stale backlog packets discarded,
  nine ingress drops, 47 recovery episodes. Render p95 itself was only 1.76 ms.
- Two with profile: 540 retransmits, zero cache misses; source 2.174 Mbps,
  aggregate 4.518 Mbps. ARM still discarded 3794 backlog packets, had 31 ingress
  drops and 32 recovery episodes. Render p95 was 1.69 ms. Rendering alone does
  not explain the freezes; receive/reorder/reference recovery and VM scheduling
  need isolation. This evidence does not identify a single root cause.
- Source CPU/receiver feedback caused bitrate downshifts toward 1.5 Mbps;
  resolution stayed 1920x1080. Static content reduced FPS as configured.
- No intentional loss injection was enabled. Repairing every packet eventually
  would not establish smoothness: ARM's multi-second recovery remains a failure.
- First frame: Mac initial 1013 ms; restart while sharing 221 ms. ARM initial
  1855 ms; default restart while sharing 2469 ms. ARM Wi-Fi-profile presentation
  start 372 ms. Both Students retained stable IDs without duplicate online rows.

## Observe, Control and lifecycle

- Registration, thumbnails, repeated Start/Stop, Share Selected, identity-preserving
  Student reconnect and joining an already-running presentation were exercised.
- Mac -> Windows Observe negotiated H.264@60 with actual 1512x982 pixels.
  Final bounded native-texture run sequence progressed 339 -> 904 while the tray
  was open. Earlier controlled workload run progressed 138 -> 1447.
  Capture-to-surface estimates around 45-84 ms, with ~4-6 ms clock uncertainty,
  exclude Flutter/compositor/display latency. They are not optical latency.
- Control mode on Mac was acknowledged; Enigo logged actual permission to
  simulate input. A positioned click was observed during interactive testing.
  Do not infer comprehensive keyboard/scroll/exclusive-control coverage.
- Windows ARM -> Windows Observe negotiated H.264@30 software fallback,
  1510x948 native texture. Sequence progressed 127 -> 1461; recent texture FPS
  about 12-17. The latency estimate was unavailable in the final static snapshot
  (zero delay samples), so no reverse ARM latency claim is made.
- Independent Observe windows were actual Flutter windows. Automated and manual
  windows briefly competed for the same Student; that overlap was cleared before
  final measurements. This is not evidence of a separate reconnect defect.
- Test LaunchAgents on Mac were booted out; ARM interactive task was stopped and
  unregistered; Windows Teacher, media children and test windows exited. The
  existing unrelated Mac Teacher was left alone. New app/bundles/logs remain.

## Checks, reproduction and next step

- Passed Rust app-bin tests (102 passed, 1 opt-in ignored), config tests (5),
  cargo fmt, app-bin Clippy `-D warnings -A deprecated`, PowerShell parser checks,
  packaging for both architectures, and the executable scenarios above.
  The deprecated AtomicUsize warning predates this change / Rust 1.99.
  No Flutter Dart/C++ changes in this checkpoint; its previous analyze/18-test
  verification still applies.
- Reproduction controller: `scripts/test-flutter-lan-windows.ps1`; packaging:
  `scripts/build-student-windows.ps1`, `scripts/build-flutter-windows.ps1`.
  `config/wifi-test.toml` is an opt-in Student overlay (jitter=50 ms,
  NACK repair=100 ms); wired defaults remain 20/25 ms. It is not a cure for ARM loss.
- Evidence: `target/windows-mac-wifi/20261004-125530/` (initial),
  `130319/` (tray fix), `131252/` (menu/Observe/rescan/Quit), `131550/`
  (two default Students), `132330/` (profile comparison, ARM Observe, final logs).
  These shortened names share the `20261004-` prefix. Intermediate harness runs
  failed due to Windows diagnostic-file sharing; final atomic snapshot publishing
  skips contention instead of terminating live IPC. Failures are retained.
  Endpoint descriptors contain authentication tokens and must remain private.
- Next: repeat the same controlled workload with a physically separate x64
  Windows Student on wired Ethernet, retaining both receiver ingress/recovery and
  source metrics. Compare Wi-Fi and wired on the ARM VM, then a headless ARM probe
  / pacing sweep if loss persists; vary one factor per run. Keep these recovery
  failures visible before attempting 1/5/10/20/40 physical Windows acceptance.
  No optical text readability, hardware MFT, or 40-PC acceptance claim is made.
