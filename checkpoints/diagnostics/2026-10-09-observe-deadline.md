# Observe recovery deadline and compact crop grid — 2026-10-09

Owner reported Windows logout leaves Observe on the last frame indefinitely;
later login can leave Connecting without recovery. Existing Flutter recovery
allowed only two attempts, consumed even while Student was offline, with no
window-expiry deadline. It also treated receiver phase live as sufficient while
a cached frame could remain visible.

Teacher UI now reconciles recovery every 250ms using a monotonic clock. A single
10s interruption budget is not reset by retry, acknowledgement or new surface.
Recovery requires a new source sequence on the current attached surface with
live coordinator and online Student. No fresh frame for 3s flags a stall; its
deadline is anchored to the last fresh-frame observation. Offline/error events
also trigger waiting. Only online Students receive new requests, with a 3s retry
cooldown; returning online can restart a stuck starting request immediately.
The initial connection also has a 10s bound. Timeout invokes existing close path:
hide window, detach texture, send surface-scoped observe_close and exit UI process.
Cooperative Control intent still restores only after fresh resumed video. Shared
class-broadcast control retains its separate coordinator-owned lifecycle.

Status overlay uses 24px bold white text, dark panel, amber icon/border and a 16px
countdown over a dimmed last frame. Closing prevents late texture attachment from
reactivating the window. No stale input is replayed.

Owner additionally requested a thinner grid overlay and aspect-fill previews.
Grid footer shrinks from 52px to 34px, with compact name/IP typography and buttons.
Extra media text remains in list view/status tooltip. Grid preview now uses cover
cropping to eliminate aspect-ratio bars; list view retains aspect-fit.

Flutter format/analyze and diff whitespace checks passed. No tests added or run.
Paired `paired-observe-timeout-20261009` build completed and Teacher replaced/
reopened at its fixed path. Both embedded Student packages byte-match fresh
outputs; existing compatible Students were not redeployed. Both `.254` and
`.155` are online. Real Windows logout/login, cable/Wi-Fi interruption below 10s,
timeout above 10s, and Control restoration need owner runtime verification.
