# minifb 0.28.0 local patch

Upstream sources and licenses are retained. Windows message pumping is bounded
to 256 messages or four milliseconds per update. Remaining messages stay queued;
individual callbacks can still block, so this is not a real-time guarantee.

Native macOS/Windows lifecycle additions support hidden creation, show/hide,
visibility queries, and opt-in close-to-hide. Defaults preserve normal preview
visibility and closing. Teacher enables close-to-hide only after tray creation
succeeds. A hidden Student host initializes/pumps the native event loop. macOS
background hosts use accessory activation policy; prepared Metal content is drawn
before a hidden window is exposed. Native objects remain owned on the UI thread.
The framebuffer must remain alive until the window is dropped, and buffer
replacement is published before another native update.

These small extensions keep native desktop lifecycle outside the media engine.
No browser or network dependency is added to this vendor crate.

macOS exposes native view backing density through convertSizeToBacking, independent
of logical window dimensions. Completed left clicks are latched until the next
native pump with release coordinates; SharedData Rust/C layout is kept in sync.
An explicit diagnostic posts a zero-pressure mouse-down/up pair to NSApp's queue.
Normal runtime never injects input. All native source/header changes rebuild.

Notification windows can use `show_inactive_topmost`: Windows SetWindowPos with
HWND_TOPMOST and SWP_NOACTIVATE, macOS floating level + orderFrontRegardless.
Unlike the existing topmost method this does not make a hidden notification
visible before its first paint or steal foreground keyboard focus.
- Windows titleless borderless windows explicitly use `WS_POPUP`: style zero
  means `WS_OVERLAPPED` and Windows otherwise adds `WS_CAPTION` automatically.

### Passive FPS badge (2026-10-06)

`set_passive_overlay(alpha)` makes a native titleless overlay semi-transparent and
click-through. Windows uses LAYERED/TRANSPARENT/NOACTIVATE/TOOLWINDOW and constant
alpha; macOS sets window alpha/ignoresMouseEvents and excludes window cycling.
The presentation owns the badge's lifetime; it never takes keyboard focus.
