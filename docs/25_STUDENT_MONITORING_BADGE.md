# Student Monitoring Badge

Owner-authorized addition, 2026-10-10. The later owner instruction overrides
initial session-end behavior: network loss and Teacher exit must NOT hide the
badge. Only an explicit off command removes it. Student restart restores its
per-user persisted preference; Teacher restart restores per-device desired state.
A new Teacher without a saved preference adopts the Student's reported state,
rather than sending an implicit off. Killing/uninstalling Student or logging out
still removes its UI; this feature is a notice, not an anti-tamper service.

## Control and UI

Teacher toolbar eye icon opens “Đang giám sát” for checkbox selection or all
known room computers. The Student context menu opens it for one online computer.
The live dialog reports per-device acknowledgement, pending, timeout (10 seconds),
offline and unsupported/error status. Offline targets retain the desired state
and receive it when reconnecting. Per-device grid/list status includes an eye.
The notice is independent of observation, recording, keyboard collection and
activity monitoring. It makes no claim those functions are running.

Existing authenticated private loopback IPC admits the Teacher operator command
`monitoring_badge {enabled:bool}`. Existing tools TCP/Protobuf sends
`monitoring_badge`, ID=random revision, status=on/off. Student's UI-thread owner
creates/destroys/draws exactly one native indicator, persists the preference,
then returns `monitoring_badge_ack` (matching revision, complete/error, actual
visibility). Old revisions cannot overwrite a newer request. Each tools reconnect
sends the current desired state; no video or new listener is involved.

Authorization is the existing Teacher OS login/private IPC token and roster/IP
validation of tools peers. This does not introduce directory RBAC or upgrade the
existing plaintext tools transport to TLS. It does not grant Student a new
remote command endpoint. No administrator/root elevation is needed for the UI.

## Native drawing

Windows: owned minifb HWND with Win32 layered, transparent, no-activate/tool-window
styles; premultiplied per-pixel BGRA and compositor constant alpha 255. Background
alone is rgba(201,31,40,0.75), radius 6 logical px. Text is Segoe UI Semibold 12px,
white opaque; dot 8px, 8px gap, padding 12px horizontal/8px vertical. GetDpiForWindow
scales raster dimensions/font; primary desktop dimensions are queried every paint.
Existing FPS overlay retains its old alpha behavior through a separate API.

macOS: nonactivating NSPanel, cannot become key/main, ignoresMouseEvents,
transparent backing, no shadow, excluded from window cycling, all Spaces/fullscreen
auxiliary. NSView paints alpha background and opaque native system semibold text
separately; AppKit manages Retina scaling. Primary screen frame is queried every
paint. Native coordinates use 16 logical px from primary screen top/right, rather
than cursor screen or work area. The indicator may overlay the menu bar region.

Only the dot changes: `0.65 + 0.35*cos(pi*elapsed_seconds)` yields 1 → .3 → 1
in two monotonic seconds. UI paints at most about 30 Hz, with no media copies and
cached Windows text/background. Normal UI-thread scheduling can skip animation
samples; mathematical period remains two seconds. Hide/drop releases native
resources; failures retry at five-second intervals and report errors.

While a Student is receiving a Teacher or class-source presentation, its
StudentDesktop destroys the overlay window but retains the persisted enabled
preference; the window is recreated when presentation ends. The Student acting
as a class broadcast source also waits for the UI thread to confirm suppression
before starting capture. If that confirmation does not arrive within two seconds,
the Student refuses to start the source broadcast rather than risk including the
badge in its first frames. This is a temporary rendering suppression, not an OFF
command.

## Validation

Focused Rust tests cover period/range, premultiplied alpha/opaque foreground,
explicit state across disconnect/restart, stale/mismatched ACK, timeout/error and
Student preference reload. Flutter tests cover state labels. Real native display,
click-through/focus, monitor/DPI changes and room scale require runtime evidence;
see checkpoint for current results. Secure desktop/lock screens are OS-owned and
are not covered by an ordinary user overlay.
