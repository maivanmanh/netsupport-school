> Current role and packaging baseline: [Teacher/Student architecture](15_ROLE_ARCHITECTURE.md).

# Phase 2 — native classroom console and background Student

Owner authorization (2026-10-02) explicitly permits Phase 2 now. Phase 1 physical
Windows wired 1/5/10/20/40 and optical acceptance remain deferred, not passed.
This overrides the original stop condition only for the following scope.

## P2-A — remembered startup and room lifecycle

Persist role and Teacher address after first setup. Subsequent launches directly
open Teacher console or run Student with no launcher/window until presentation.
Explicit CLI roles retain immediate-share behavior for existing automation.
Provide explicit setup/profile flags for recovery and isolated unattended tests.
Stable per-profile Student identity avoids duplicate roster rows after relaunch.
No OS auto-start, installer or privileged service in this milestone.

## P2-B — lightweight monitoring

Dedicated length-prefixed Protobuf TCP channel (control port + 1), independent
of H.264/RTP/control heartbeat. Teacher requests at most one snapshot at a time;
Student captures a maximum 480x270 aspect-fit JPEG about every 2 seconds.
No disk screenshot archive. One latest decoded thumbnail per Student, finite
peer count and frame byte/pixel limits. Decode/capture/JPEG work stays off UI and
async media/control threads. Stop capture while receiving Teacher presentation;
show an explicit paused state. Failed monitoring must not stop broadcasting.
Log snapshot counts/bytes, capture/JPEG/decode latency, age/state and recovery.

## P2-C — Teacher console

Resizable native desktop console with Office-inspired Ribbon grouping and a
large central Grid/List roster, thumbnails, host/IP/online/paused/stale state,
selection, paging, counts and enlarged snapshot inspection. Share/Stop retain
one capture/encoder session. Only implemented operations are actionable;
Files and machine-power controls remain documented future groups.

## P2-D — automated integration and delivery

Build Mac + Windows x64, copy exe to Downloads and byte-verify. Ship to .254 Mac
and .155 Windows via SSH. Test local Teacher/two heterogeneous Students and
Windows Teacher/Mac Student: idle thumbnails, view switching, selection/inspection,
Start/Stop/restart, reconnect and saved-profile startup. Collect full logs and
screenshots of the real console, inspect UI, analyze, fix concrete failures and
repeat affected tests. Preserve failed attempts. Update checkpoint and evidence.

## Deferred

Physical 40-Windows acceptance, Windows Service/OS auto-start, remote input,
shutdown/reboot, file transfer and live Student-to-class broadcast. Enlarged
snapshot inspection is not a high-FPS remote-control feature.

## P2-E — scoped presentation (owner continuation, 2026-10-02)

Add an explicit Share Selected alongside Share Class. Freeze stable Student IDs
when presentation starts; only those peers get Start/UDP registration. Rejoining
selected Students remain eligible; other Students retain control/monitoring and
normal desktop. Changing selection during sharing affects the next presentation.
Keep one capture/encoder and no wire-protocol change. Verify excluded late joins,
selected reconnect and whole-room behavior with focused control tests; deploy
and run class then selected cycles with both Students. Remote Mac thumbnail
permission is deferred by owner and is not a gate for receiving tests.

## P2-F — native startup, interaction, settings and tray (owner approval)

Keep the existing native framework/media separation. Prepare fonts/framebuffer
before showing GUI; extend vendored minifb only with hidden creation, visibility
and explicit close-to-hide (default preview semantics unchanged). Teacher close
hides only if a working tray exists, otherwise normal close exits. Native Windows
tray / macOS menu bar provides Show/Settings/Status/Exit; Student pumps a hidden
native host while idle, no visible launcher or console. No OS auto-start/service.
Persist Grid/List, window size and role/Teacher IP via existing profile. Applying
connection settings is explicit restart/reconnect, never silently interrupts an
active presentation. Use system SF/Segoe UI fonts, sentence-case labels and
hover/pressed/beveled buttons; keep bounded redraw and cached glyphs. Test native
hide/show without ending streams, settings validation, persisted startup and
both Teacher roles; collect first-frame timing and real UI states. Remote Mac
thumbnail permission remains deferred, not a blocker for receiving/control.

## P2-G — owner UI corrections: Retina, aspect fill and quick taps

Render Teacher/setup UI at native macOS backing pixel density while retaining
logical layout/hit-test units and readable system font sizes. Re-evaluate backing
density on resize/display changes. Grid thumbnail region is 16:9, cards bounded in
width; Grid/List center-crop aspect fill, inspection keeps complete aspect fit.
Retain completed native clicks across event pumping so a zero-pressure quick tap
with down/up inside one pump triggers the same action as a physical mouse click.
Validate native input queue, Retina raster dimensions, crop coverage and both OS
Teacher media lifecycles. Media capture/encode/decode quality policy is unchanged.

## P2-H — owner approved Flutter Teacher console

Replace hand-painted management UI with native Flutter desktop widgets, responsive
Grid/List, accessible desktop controls, system font, Settings, inspection, scoped
Share/Stop and clear connection states. Keep Rust capture/codec/transport/native
Student presentation unchanged. Rust Teacher agent is a separate background
process with bounded framed JSON IPC over loopback TCP (not HTTP/WebSocket or a
web server). This pragmatic cross-platform IPC avoids adding native pipe plugins;
endpoint is ephemeral, authenticated and per-profile. Closing the console does not
stop sharing; reopen attaches to the existing agent. Explicit quit stops both.
Snapshot payload reuses existing bounded JPEG; never send full video through Dart.

Deliver a Mac app bundle and Windows folder/zip (Flutter runtime/assets plus Rust
helper). Owner explicitly accepts this packaging change from single-executable
Phase1. Build Mac locally, Windows on an available Windows toolchain, deploy and
verify actual Rust Students through SSH, collect logs, test tap/keyboard/Retina,
responsive layouts, attach/reopen, share scopes and stop. Preserve failed attempts.
Do not claim physical Windows scale or deferred Mac thumbnail TCC acceptance.


### P2-H status — 2026-10-02

Mac Flutter Teacher console, local Rust agent, and real two-Student mixed-machine
smoke have been built and verified. The owner clarified that the Windows and Mac
Students stay on the lightweight Rust executable; no Flutter runtime is needed on
those Student machines. Windows Flutter Teacher packaging remains future work if
Windows becomes a Teacher target. See `checkpoints/evidence/phase2-flutter-2026-10-02/REPORT.md`.

## P2-I — everyday Flutter Teacher console (owner approved)

Add compact no-thumbnail roster, persist current-room identity/name/IP history,
sort online/name/numeric-IP, refresh authoritative connections, preserve selection
through Grid/List/Compact and filtering, select visible online/clear, explicit stale
snapshot and connection states, immediate command progress and friendly capture
permission feedback. Reduce header/action dimensions. No media pipeline changes,
subnet discovery, Student Flutter or machine power/file features. Test persistence,
responsive layouts and real mixed Students; distinguish TCC-blocked media tests.

## P2-J — Teacher classroom tools (owner approved)

Teacher can capture an inspected Student frame to a timestamped Desktop PNG, send
popup messages with one optional Student reply, and send files with progress,
cancel and retry. Rust Students remain GUI-light and use a native message popup.
Tool packets use a separately versioned TCP connection on control port + 2; file
bytes are separately paced and checksummed. Received files save directly to the
Student Desktop on both operating systems. No tools payload enters the video path.
Teacher screenshots save to the Teacher Desktop on both operating systems: native
macOS capture uses the registered frame, while Windows saves the displayed Flutter
image through the Windows Desktop known-folder path.
