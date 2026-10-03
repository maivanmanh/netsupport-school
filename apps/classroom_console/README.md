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
Windows Teacher packaging awaits a Windows Flutter build environment.
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
