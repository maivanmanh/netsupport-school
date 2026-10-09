# Teacher and Student architecture — owner baseline, 2026-10-02

Teacher supports macOS and Windows: Flutter owns the management console,
responsive roster, thumbnails, selection and commands. A separate Rust Teacher
agent owns classroom coordination, capture, encoding, packetization, fan-out,
adaptation and telemetry. Authenticated bounded loopback IPC carries commands,
status and JPEG thumbnails. H.264 video never passes through Flutter/Dart.

Student deployment targets Windows. It is entirely Rust: background registration,
heartbeat/reconnect, monitoring snapshots, H.264 receiving/decoding and native
fullscreen presentation. A tray and optional configuration dialog are operational
controls; Student has no Flutter console or Flutter runtime. Idle startup opens
no main window. Native fullscreen rendering is necessary when Teacher presents.
OS auto-start/service installation remains future work.

Mac Student is a development test peer only. Its privacy permissions and runtime
results are not Windows deployment acceptance. Current integration uses local Mac
Teacher, Windows Parallels Student, and optionally the second Mac as a test Student.
Windows Flutter Teacher is an intended product target; building and validating its
bundle is deferred until a Windows Flutter build environment is available.

## Deliverables and shared implementation

- Teacher: `target/Classroom Console.app` includes Flutter and a Rust helper.
- macOS test Student: `target/Classroom Student.app` contains only the Rust
  `ClassroomStudent` executable; build with `scripts/build-student-macos.sh`.
  Raw diagnostic binary remains `target/release/ClassroomStudent`. Windows equivalent is
  `target/rustup-windows/x86_64-pc-windows-gnu/release/ClassroomStudent.exe`.
- `Classroom` remains the shared Rust helper and development CLI. Its legacy native
  Teacher dashboard is a diagnostic compatibility path, not the product console.

The dedicated Student entry point shares the existing Rust engine source rather
than duplicating media/session logic. It forces Student role before profile loading,
rejects Teacher flags and role setup, and reads the remembered Teacher IP.
It still compiles shared Rust modules; this is a runtime/packaging boundary,
not a claim that every Teacher module was removed from the Student dependency graph.
Future extraction into a reusable app library can follow when needed for size or
maintainability. The performance-sensitive media crates already have independent
interfaces. One capture/encoder/packetizer per presentation remains mandatory.

## Student usage

Configure once:

```sh
ClassroomStudent --teacher-ip 192.168.100.219 --remember
```

Subsequent launches of ClassroomStudent run the background Student immediately,
even if a stale profile had Teacher role. CLI Teacher IP overrides saved IP;
external config and embedded defaults apply when no saved IP exists. Use
`--launch-profile PATH` for isolated tests and `--diagnose` to inspect configuration.
`--headless` is a diagnostic mode that omits fullscreen rendering, not normal
Student operation. Existing Classroom role-based scripts remain compatible.

## Live Observe design (owner authorized)

Independent Flutter Observe windows attach to the same Teacher agent. Rust owns
reverse publishing, RTP recovery, decode and a bounded latest-frame native surface.
The native Flutter texture reads pixels outside Dart; UI controls carry small
commands only. Local Teacher broadcast and reverse Student publish invoke the
same session implementation. Subscriber selection is a routing concern.
Observe is read-only by default. Control is explicit, cooperative by default;
exclusive Windows input requires an acknowledged capability and an expiring lease.
Closing Observe must not close the main console or persistent room.

Owner clarification (2026-10-08): **Control & present Student** automatically opens
cooperative Control for the selected source; Teacher and that Student may both
operate it. This class session cannot be switched to Observe. Closing its Control
window, losing the source, or losing the input lease ends the class presentation.
Ordinary standalone Observe retains its existing default and lifecycle.

Class relay and Teacher Control preview register with separate authorized identities
on one Source publisher. Capture, encode and packetization happen once; Source
fans those packets to the relay and preview, and Teacher fans compressed RTP to
the class. Only the Teacher preview decodes locally. Control acknowledgements and
cursor metadata are addressed to the preview connection. Class mode keeps the
source cursor in the encoded picture so recipients see Teacher/Student actions.
The extra Source-to-Teacher preview stream and Teacher decoder require performance
measurement; this is not evidence of 35–40 physical PC acceptance.

Observe opens in Fill mode at the complete logical work area of the monitor
containing the Teacher window, excluding the menu bar and Dock/taskbar. Bounds
are set before the window is shown. This is a centered ordinary window, not a
macOS fullscreen Space. Display discovery failure falls back to native maximize.


### Observe geometry and input coordinates

Fill stretches the image to the whole viewport. Stretch uses aspect-preserving
cover scaling with horizontal/vertical scrollbars to reach the complete frame.
Local scrollbar gutters are outside the video hit area, so dragging them never
clicks the Student. Control wheel/trackpad events still scroll the remote app.
Flutter normalizes positions in the full scrolled image. Chrome overlays do not
change the image coordinates.
The Student capture backend exposes display bounds using the same display index
and enumeration order as capture (reverse publishing currently uses index0).
macOS bounds are SCDisplay.frame in points; Windows bounds are DXGI physical
DesktopCoordinates, with absolute SendInput over the physical virtual desktop.
This avoids Enigo main-display backing-pixel assumptions on Retina/secondary screens.
Control uses the Teacher OS cursor for immediate pointer feedback. A session-owned
cursor policy disables source-cursor composition in reverse capture during
acknowledged Control, restoring it for Observe, errors or input lease expiry.
Normal class broadcast includes its cursor. Encoding and transport remain shared.
Discrete clicks carry normalized coordinates and preserve movement/click order.
The Teacher coordinator must sustain the 125 Hz motion input rate without queuing
seconds of old commands; the automated IPC dispatch probe checks this separately
from video latency.

Input now retains the selected display's OS identity and queries its current
bounds before each movement/positioned click, rather than freezing geometry at
Control start. This covers live resolution/DPI/desktop-origin changes without
re-enumerating ScreenCaptureKit/DXGI on the high-rate input path. Mac uses
CGDisplayBounds in desktop points; Windows uses DPI-aware GetMonitorInfoW in
physical pixels. Invalid/unavailable geometry releases Control through the
existing error path. Changed bounds are logged, never individual input positions.
Teacher receives Flutter wheel and trackpad pan events and accumulates small
fractions before sending integral scroll lines; Observe never forwards scrolling.
The native window title follows acknowledged Observe/Control state.
An Observe/Control window retains its acknowledged mode while unfocused, with
the existing session keepalive continuing. Blur releases held mouse buttons and
keys and pauses forwarding input, rather than requesting Observe. Focus restores
input forwarding in the same mode. Closing/reopening creates a new read-only
Observe session; genuine connection/lease failure still releases remote input.

### Observe session stats and macOS reopen (2026-10-03)

The speedometer toolbar button toggles a pointer-transparent translucent stats
panel. It reports the current acknowledged mode, actual decoded resolution,
new native preview texture copies per second, received media bitrate, and the
interval median capture-to-receiver-surface estimate with clock uncertainty.
Clock-uncorrected, missing, ended or older-than-three-second receiver values are
not displayed as valid measurements. Optical latency and Flutter/compositor
latency are not inferred from this estimate. FPS is not monitor refresh rate.

Reverse receiving reuses the same receiver pipeline. An optional latest interval
metrics slot is allocated only for Observe. The worker updates received Mbps;
the surface owner samples corrected source timestamps only after successful
publication, with bounded StageMetrics samples. One small JSON status per second
reaches the coordinator and the existing authenticated UI IPC; Dart never reads
log files or transports pixels. Neither Student protocol nor Student deployment
changes are needed for this feature.

macOS Teacher packages Rust in
`Contents/Helpers/Classroom Agent.app/Contents/MacOS/Classroom`, with a separate
bundle identity and LSUIElement. A raw helper directly under the parent bundle
was incorrectly activated by LaunchServices on Finder reopen, producing a Dock
icon without a console. The Flutter Teacher remains alive when its window hides,
and native reopen restores/deminiaturizes it. Reopen delivered to an Observe
process routes through authenticated IPC. The coordinator counts actual console
clients separately from Observe clients, so tray/Finder can recreate a missing
console while Observe remains connected. Existing development ad-hoc/manual
permission policy is unchanged.


### Independent Observe window lifecycle (2026-10-05)

The persistent Rust Teacher coordinator serializes `observe_window` requests and
keeps the child UI process for each Student, including during engine startup.
Repeated clicks focus that process rather than launching another. Exited children
are reaped before reopening; Teacher shutdown also closes unconnected startup
children. The main console grants/focuses the owned PID using native activation
only after an explicit roster click; background state changes do not steal focus.

Observe X hides immediately, releases input, detaches its texture and sends a
session/surface-specific close without awaiting a remote acknowledgement. It then
uses the application's bounded close/process-exit path. A delayed old close cannot
end a newer surface for the same Student. Windows engine destruction retains
texture buffers until engine/raster teardown completes. The media coordinator's
existing single active reverse View remains unchanged by UI instance management.

Windows texture reads never block on the producer. A lock miss/invalid header/full
buffer pool retains the last complete RGBA frame through counted raster leases.
Three buffers bound memory; held buffers are immutable. `cached_frame_returns`
is included with existing copy/lock/sequence counters in the Observe UI log.
