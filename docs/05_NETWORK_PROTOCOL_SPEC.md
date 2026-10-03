# 05 — Network and protocol specification

## Control plane

Preferred initial design: TCP with length-prefixed Protobuf messages.

Suggested message families:

- `HelloStudent`
  - protocol version
  - hostname
  - student instance ID
  - selected local IP
  - media UDP port
  - feedback UDP port if separate
  - display size
  - decoder capabilities
- `WelcomeStudent`
  - assigned session/client ID
  - current presentation state
- `ControlPing` / `ControlPong` (nonce echo for per-Student TCP control RTT)
- `StartPresentation`
  - stream ID
  - codec
  - width/height
  - nominal max fps
  - current bitrate
  - RTP clock / payload information
  - Teacher UDP media source/probe port
- `StopPresentation`
- `RequestKeyframe`
- `ReceiverReport`
  - sequence/loss information
  - receive bitrate
  - decode FPS
  - render FPS
  - late/dropped frame counts
  - jitter-buffer delay
  - latest control RTT and number of valid samples
  - whether a visible renderer is active and p95 native render-call duration
  - Teacher derives interval decode and render cadence from cumulative counters. Render cadence is diagnostic and cannot independently lower bitrate; packet loss and slow decode remain feedback signals.
- `Nack`
  - compact recent missing sequence list/ranges if NACK is kept on the control connection; alternatively document a separate UDP feedback design.
- `SetQuality` / telemetry messages as needed.

Every message must include protocol versioning or be wrapped in a versioned envelope so future changes can be handled explicitly.

## Media plane

Use RTP-compatible H.264 packetization over UDP unicast.

Requirements:

- monotonically incrementing RTP sequence numbers,
- timestamps using an appropriate RTP video clock,
- MTU-safe payload sizing (avoid IP fragmentation; choose and document a conservative payload target),
- H.264 NAL fragmentation/aggregation behavior compatible with standard RTP/H.264 expectations,
- one packetized stream copied/referenced to all recipients,
- no per-recipient encode/transcode.

Current M3 implementation uses RTP version 2, dynamic H.264 payload type 96, a 90 kHz video clock, and a conservative maximum RTP payload of 1,188 bytes. It emits single-NAL packets and FU-A fragments, reconstructs Annex-B access units at the receiver, and has a configurable 20 ms default jitter budget for fresher updates on wired LAN. Increase `jitter_buffer_ms` in configuration when a link needs more packet reordering/loss tolerance. Teacher packetizes an encoded access unit once, then sends those same packet bytes to every registered UDP endpoint.

## Registration flow

1. Student selects active physical Ethernet interface.
2. Student infers teacher IP from room convention or uses configured/manual address.
3. Student opens TCP control connection to teacher.
4. Student sends `HelloStudent` with its media endpoint/capabilities.
5. Teacher adds it to roster.
6. If a presentation is active, teacher sends stream configuration and triggers/provides a recovery keyframe so the late joiner can decode quickly.

For IPv4 media registration, Student advertises the local source address selected by the actual TCP route to Teacher. Teacher treats the TCP peer address as authoritative for the initial UDP media endpoint and logs a mismatch instead of sending to an unrelated Wi-Fi/virtual-adapter address. Before Teacher sends RTP, Student sends a small UDP route probe from its bound media socket to the Teacher media source port announced in `StartPresentation`. Teacher uses that datagram's source address and port as the final endpoint, then requests an IDR. Student repeats the probe at a bounded one-second cadence only while it is still waiting for a keyframe. This confirms the actual bidirectional interface/port tuple and establishes the return path through a stateful host firewall. Teacher requests a 256 KiB UDP send buffer (actual size is logged), paces aggregate
RTP fan-out in small batches (`network.send_pacing_mbps`, default 400; 0 disables),
and caps pacing per access unit at the smaller of 8 ms or one third of a frame
interval. On `WouldBlock`, all recipients share at most 2 ms of active retry time
within a 10 ms access-unit retry deadline. There is no sender retry queue. Exhausted
or permanent errors still abandon the remainder of that access unit for the affected
endpoint and explicitly count abandoned datagrams. An RTP send failure is counted and rate-limited in logs; it does not terminate presentation for the rest of the class.

The Student binds its UDP port before registering, so its initial recovery keyframe is not lost while native preview setup is in progress. `ReceiverReport`, `Nack` (at most 32 RTP sequence numbers), and `RequestKeyframe` travel on the existing TCP control connection. Teacher retains a configurable 300 ms RTP cache and retransmits only a requested packet that is still in that cache; if recovery proves insufficient, Student requests a fresh IDR rather than displaying stale video.

Protocol version 2 added `ControlPing`/`ControlPong`. Protocol version 3 adds the Teacher UDP media port and the bidirectional route probe. Protocol version 4 adds visible-renderer state and render-call p95 to `ReceiverReport`, allowing bitrate adaptation to distinguish a slow visible presentation from an intentionally headless Student. Each Student keeps at most one TCP ping outstanding, sends a new ping about once per second, and accepts a pong only when its nonce matches. The receiver report carries the latest measured control RTT in microseconds and its sample count (zero count means unmeasured). This measures the TCP control path, including endpoint scheduling; it is not RTP delay or glass-to-glass latency. Older peers are rejected at registration so mixed executables cannot silently omit required routing or telemetry behavior.

Student also watches the control TCP reader task for EOF/error. If Teacher exits or ends its share-duration session and closes the control socket, Student ends the current media session so dropping its renderer closes the forced fullscreen preview; the existing control reconnect loop remains available afterward.

## Address inference

Convenience rule:

`10.0.X.Y -> teacher 10.0.X.200`

This is not a network-discovery protocol and is not universally correct. It is a configurable campus convention. Manual teacher-IP override is mandatory.

## Adapter selection

Because virtual adapters are installed, use the active default route to identify the preferred interface. Reject/penalize loopback, down, tunnel, Hyper-V/VMware virtual adapters unless the user explicitly selects one.

## Ports

Use configurable high ports. The sample config uses:

- TCP control: 47800
- UDP media: 47810
- UDP feedback: 47811

The implementation may choose a different high-port set if justified, but keep it configurable and document Windows Firewall implications.

## Protocol 5 diagnostic extension (2026-09-28)

Both peers must use protocol 5. ControlPing carries Student send wall time;
ControlPong echoes it with Teacher receive/send times. These support a bounded
clock offset/uncertainty estimate alongside existing monotonic control RTT.
RTP uses optional profile 0x434c, four 32-bit words: capture and send Unix
microseconds (two big-endian u64 values); capture zero means unavailable.
Twenty extra header bytes keep the maximum datagram at 1220 bytes. Retransmission
retains original timing. See `12_TELEMETRY.md` for measurement limitations.

## Persistent room lifecycle (implemented, version 6)

Idle registration returns WelcomeStudent with presentation_active=false. The
TCP connection stays open while idle and pings continue. StartPresentation
creates a fresh increasing stream ID; StopPresentation identifies the stopped
stream. Student discards packets with a different RTP SSRC, derived from the
stream ID, and requests a clean keyframe for each new presentation. Feedback
for old streams is ignored. Stop closes preview before joining the receiver.

Teacher owns one media worker at a time, waits for it to finish before enabling
Start, and creates fresh capture/codec/repair state. Idle rooms capture/encode
nothing. Control limits: 128 peers, 256 roster entries (offline history evicted
first), 128 messages per peer and 256 media events. Framed TCP reads run in an
owned reader task so watch-state updates cannot cancel a partial message.
Write deadlines are two seconds; peers silent for ten seconds are removed.

## Student control liveness (2026-10-02)

Students release the forced presentation when no complete Teacher control message
arrives for five seconds, even when TCP has not reported EOF. Periodic ping/pong
operates independently of screen activity. The expired connection is discarded
(the partially read frame is never resumed); the normal reconnect loop then
registers a fresh session and requests a recovery keyframe. Welcome and initial
presentation reads also have five-second deadlines. Logs distinguish the control
liveness timeout from the subsequent native preview close. This is a control
response deadline, not a media-inactivity timeout or a glass-to-glass measurement.
