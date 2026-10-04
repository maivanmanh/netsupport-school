# Windows Flutter Teacher smoke — 2026-10-04

Teacher and Student ran on this Windows 11 Pro Parallels VM: 4 logical CPUs,
8 GB RAM, 1920x1080, Parallels Display Adapter (WDDM). Rust host is ARM64 MSVC
1.99; the delivered bundle, Rust helper and Student are x64 MSVC. Tools: Flutter
3.47.6, Dart 3.13.5, VS Build Tools 2022. The owner enabled Developer Mode after
Flutter rejected plugin symlink creation.

## Verified

- Portable Flutter bundle builds; the console launches and authenticates Rust IPC.
- Student registers and supplies a real DXGI thumbnail. Two successive 1080p
  H.264 Share/Stop cycles decode/render and close fullscreen after Stop.
- Student restart reconnects with the same identity, without a duplicate online row.
- Observe now receives MF NV12 and updates the native Flutter texture continuously.
  Final sequence samples: 37 then 135; native texture copy rate ~12.87 FPS. This
  is not optical display cadence.
- All 18 Flutter tests pass; analyze clean. Rust workspace lib/bin tests pass;
  opt-in measurements remain ignored. Application tests run for both bin targets.
- Formatting/diff checks pass. App-bin Clippy passes with `-D warnings -A deprecated`.
  Strict mode without the exception rejects pre-existing `AtomicUsize::fetch_update`
  under Rust 1.99; no unrelated migration of the Rust 1.82 baseline was made.

## Measurements and interpretation

20-second x64 cycles (`20261004-123255`): one software H264 Encoder MFT, one
encoder instance, 4 fan-out threads, zero send errors. Encode p95 20.00 / 19.33 ms;
Teacher working set ~144 MiB. First Student cycle: 620 decoded / 561 rendered,
31.23 decode / 28.26 render FPS, zero decode/depacketizer errors, zero final
missing packets; 2 NACK sequences retransmitted. Render-call p95 ~31.79 ms.
Some Teacher/decode log FPS exceed the configured 30 target; existing cadence
metrics do not establish a strict maximum-FPS acceptance pass.

Final short cycles (`20261004-124228`): encode p95 16.60 / 20.62 ms, one encoder,
zero send errors; first Student cycle 26.12 render FPS, zero decode errors/missing
packets. Observe remains live, no session error, successive native sequences advance.
Content includes recursive same-screen feedback, not a controlled IDE workload.

## Fixes

1. MSVC /WX rejected screenshot filename wchar_t-to-char conversion: use UTF-8.
2. Protocol round-trip test omitted `pointer_overlay`: exercise the new field.
3. Flutter permission test assumed macOS: assert Windows behavior too. Refresh a
   mocked live receiver report before stats assertions to avoid real-clock expiry.
4. Add missing `classroom/observe` Windows channel: fixed-size read-only mapping,
   nonblocking LockFileEx matching fs2, three reusable RGBA buffers with Flutter
   release callbacks, attach/stats/detach, timer refresh. Use a nonzero timer ID
   and handle it before plugin delegation; the zero-ID timer stalled at frame one.
5. Observe rejected Windows NV12: convert padded NV12 through the existing frame
   abstraction outside the IPC lock. Student broadcast rendering stays NV12/D3D11.
6. Resolve Rust helper path before packager changes directory; include the Student
   binary when available next to the helper.

## Reproduction and artifacts

```powershell
cargo build --release --bins --target x86_64-pc-windows-msvc
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-flutter-windows.ps1 -RustAgent target/x86_64-pc-windows-msvc/release/Classroom.exe
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-flutter-windows.ps1
cargo fmt --all -- --check
cargo test --workspace --lib --bins --offline --quiet
cargo clippy -p classroom-app --bins --no-deps --offline -- -D warnings -A deprecated
# From apps/classroom_console:
flutter analyze
flutter test
```

Bundle: `target/Classroom-Console-Windows-x64/`; launch `classroom_console.exe`
with DLLs/data intact. Manual same-host test:

```powershell
.\target\Classroom-Console-Windows-x64\classroom_console.exe --bind-ip 127.0.0.1 --control-port 48700
.\target\Classroom-Console-Windows-x64\ClassroomStudent.exe --loopback --control-port 48700 --media-port 48710 --feedback-port 48711
```

Local evidence: `target/windows-flutter-smoke/`: `20261004-122534` initial failures;
`123255` channel/NV12 fixed but refresh stuck; `123627` stricter test detects it;
`124115` / `124228` continuous updates pass. Keep complete logs/archives and
results.json; endpoint tokens are not published. Some screenshots show the terminal
because Windows foreground activation was denied; they alone do not verify text
readability. Initial screenshot shows the OS firewall prompt. No script changed
firewall rules. Owned test processes stop on completion.

## Limits and next step

Basic VM integration passes. Physical Windows 1-GbE/40-PC scaling, optical latency,
sustained controlled motion and visual readability remain open. Same-display
capture/render causes recursive feedback. Remote Control input and Desktop
screenshots were not exercised. Windows native file selection is still unsupported
(`classroom/files` implements only saveScreenshot); file UI is outside this
media-focused smoke, not a passed feature.

Next: delivered x64 bundle + one separate physical Windows Student, controlled
text/scroll workload and visual Observe/cursor check, optical latency, then the
1/5/10/20/40 physical wired Windows ladder.
