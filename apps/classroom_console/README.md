# Classroom Teacher console

Flutter desktop management UI for Teacher only (macOS and Windows targets).
Rust owns media and classroom networking; bounded authenticated loopback IPC
carries commands, state and JPEG thumbnails. Students install only the Rust
ClassroomStudent agent, without Flutter runtime/assets.

Build macOS Teacher: `scripts/build-flutter-macos.sh` from repository root.
Current development builds use explicit ad-hoc signing with manual macOS
permissions after updates. Optional `CLASSROOM_MAC_SIGN_MODE=apple` requires
trusted Apple signing and Gatekeeper acceptance, with no silent fallback.
See [signing and automation](../../docs/16_AUTOMATION_NOTES.md).
Windows Teacher build (from repository root, Developer Mode and Visual Studio
C++ tools required):

```powershell
cargo build --release --bins --target x86_64-pc-windows-msvc
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-flutter-windows.ps1 -RustAgent target/x86_64-pc-windows-msvc/release/Classroom.exe
```

The portable bundle is `target/Classroom-Console-Windows-x64/`; launch
`classroom_console.exe`. Keep its DLLs and `data/` alongside the executable.
The packager includes the Rust Student executable when available beside the agent.
Both Windows bundles include matching Visual C++ runtime DLLs beside the executables.
Copy the entire bundle; a raw Rust MSVC executable requires `vcruntime140.dll`
on a fresh Windows installation. Package a native ARM64 or x64 Student with:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-student-windows.ps1 -RustStudent target/release/ClassroomStudent.exe
```

The executable's PE architecture selects `target/Classroom-Student-Windows-arm64/`
or `target/Classroom-Student-Windows-x64/`. `-RuntimeDirectory` can override the
Visual Studio redistributable CRT directory; mismatched DLL architectures fail.
Visible same-machine smoke (registration/thumbnail, Share/Stop twice, reconnect,
and continuously updating native Observe; processes stop on completion):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-flutter-windows.ps1
```

Logs, screenshots and results stay under `target/windows-flutter-smoke/`.
Windows Observe uses a bounded native texture and a nonblocking shared map;
classroom H.264 video never traverses Dart. This smoke is not 40-PC LAN acceptance.
See [role architecture](../../docs/15_ROLE_ARCHITECTURE.md).

For a bounded LAN test with already-running remote Students:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-flutter-lan-windows.ps1 -BindIp 192.168.50.94 -Seconds 300
```

It prints its private run directory under `target/windows-mac-wifi/`. Submit one
JSON command at a time in `command.json` (`share_class`, `share_selected` with
`selected` IDs, `stop`, `rescan`, `open_observe_window`, `open_tray_menu`, `quit`).
Read `state.json` for registration, Observe and `tray_menu_open`; retry a read
briefly if Windows has locked it during atomic replacement. Wait for an
acknowledgement/state change before submitting the next command. `results.json`
contains command outcomes; the controller stops its own Teacher processes at
the deadline. It does not launch remote Students. Never publish `endpoint.json`.
Use matching test ports 48700/48710/48711 on Students. The optional Student
`--config config/wifi-test.toml` profile allows 50 ms jitter and 100 ms repair
for diagnostic comparison on Wi-Fi; it does not change wired LAN defaults.

Observe/Control toolbar: **Show stats** (speedometer icon) toggles a small
translucent panel at the upper right. It shows acknowledged mode, decoded video
resolution, preview texture FPS, actual receive Mbps, and estimated capture to
receiver surface delay (median), with clock uncertainty. Delay excludes Flutter,
compositor and physical display latency; it is not optical glass-to-glass latency.
Missing/stale measurements show a dash. Updates are once per second, and the
panel does not intercept remote pointer input. Stats start hidden in each window.

The macOS Rust Teacher helper has its own nested `Classroom Agent.app` identity.
Finder/Dock reopen restores the main Teacher window even when hidden/minimized
or when only an independent Observe window remains. Closing the Teacher window
keeps the room running; Exit Classroom still explicitly quits it.
