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
Visible same-machine smoke (registration/thumbnail, Share/Stop twice, reconnect,
and continuously updating native Observe; processes stop on completion):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/test-flutter-windows.ps1
```

Logs, screenshots and results stay under `target/windows-flutter-smoke/`.
Windows Observe uses a bounded native texture and a nonblocking shared map;
classroom H.264 video never traverses Dart. This smoke is not 40-PC LAN acceptance.
See [role architecture](../../docs/15_ROLE_ARCHITECTURE.md).

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
