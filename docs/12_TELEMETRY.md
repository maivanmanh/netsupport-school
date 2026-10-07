# Runtime telemetry — schema 3, protocol 5

## Teacher adaptive diagnostics — 2026-10-07 (protocol 8 unchanged)

`process_cpu_percent` retains summed logical-core usage. New
`process_cpu_capacity_percent` divides it by host logical CPU count, clamps to
0–100, and feeds `adaptive_pressure_cpu_percent`. A serial encoder bottleneck
remains independently detected by encode latency; low host CPU share alone
does not prove that every pipeline stage is fast.

Teacher media intervals include encode/CPU pressure flags, pressure FPS ceiling,
bitrate proposal/reason, refinement state and proposal deferral. Applied bitrate
logs distinguish local pressure, receiver degradation and recovery. New
`teacher receiver cadence interval` computes decode/render FPS from successive
existing receiver report counters and records the report window and missing
sequence delta. It is not optical latency or direct receiver ingress measurement.

Fan-out and repair log cumulative deadline-exhausted and retry-budget-exhausted
counters, plus repair WouldBlock counts. Existing error counters stay cumulative.
The exhaustion counters identify sends entering retry with no allowance; they
are not additional packet-loss counts to add to send errors.

These additions need no wire-schema change or new Student commands.

## Collection and delivery

No extra flags or measurement mode are required. Use the same current executable
on Teacher and every Student: protocol 5 intentionally rejects older peers.
Each process writes `logs/teacher-<control-port>/log.txt` or
`logs/student-<media-port>/log.txt` beside its executable. `--log-file PATH`
overrides that location. The Windows loopback batch file uses the Mac shared
Downloads folder even though the executable is staged locally.

The previous nonempty file is moved into an adjacent `archive/` directory before
the new run. Five most recent archives are retained per filename. At 64 MiB the
current file rotates using the same archive policy. Copy the whole role folder
when a run crosses rotation; otherwise its current `log.txt` is sufficient.

File logging includes all INFO/WARN/ERROR fields, independently of the console
verbosity; it has no metric-field allowlist. Main summaries and stage rows are
reported approximately once per second and at clean session completion. Detailed
stage/per-Student rows go to the file without flooding CMD. Repeated warning,
NACK and keyframe-request text is limited to one occurrence per message per ten
seconds; cumulative counters retain the relevant error/loss/recovery totals.

A dedicated writer, bounded 1024-record channel and 64 KiB write buffer keep
filesystem/UNC writes off media threads. Flush occurs every 250 ms and on clean
shutdown (up to two seconds grace); abrupt process termination can lose the
buffered tail. Queue overflow is explicitly recorded as `dropped_records`.
Values are bounded (configuration and ReceiverReport up to 4096 characters,
errors 512, other fields 256). This is a bounded diagnostic log, not a packet
capture or an unlimited history. System/disk failures cannot guarantee delivery.

## Audit: existing and added coverage

| Area | Existing measurements retained | Added in this release |
| --- | --- | --- |
| Identity | Selected adapter/IP, CPU model/cores/RAM, source/stream dimensions, codec hardware/software, render backend/GPU | Build ID, protocol/schema, full effective configuration, OS/version/architecture/PID, adapter link rates and MTU when available, actual receive socket buffer |
| Capture | FPS, capture latency summary, dirty/activity ratio, replaced/waiting frames, cursor/protected-content statistics | Capture API latency distribution per interval, capture-to-encode age and Teacher scheduling lateness |
| Encode | FPS, p50/p95 whole-call latency, target bitrate/FPS and adaptation decisions | CPU color conversion, sample input copy, MFT submission/output, output copy/normalization distributions; encoded Mbps, keyframe count and maximum AU size per interval. VideoToolbox records every encode call and session wait (including calls returning no sample), explicit frame-drop flags, missing-sample-without-drop-flag cases, and IOSurface input-copy time |
| Teacher transport | Packets/Mbps, fan-out counts, send errors/WouldBlock/retry/abandonment, pacing/retry wait, retransmissions | Packetization/fan-out timing distributions; NACK requested sequence and cache-miss totals; full ReceiverReport with client ID for each reporting Student |
| Student transport | Received Mbps/packets, missing/late aggregate, NACK/IDR counts, jitter budget, packet/AU gaps and RTT | Independent wire arrival timestamps/gaps, ingress wait/depth/high-water/oldest age/drop/malformed/wrong-peer/socket-error counts; separate late/missing/duplicate/out-of-order-arrival/overflow counters and actual jitter residence/depth |
| Decode | FPS, whole-call p50/p95, errors and pre-IDR discards | Input copy, MFT submission/output, output copy, layout and CPU conversion distributions; AU/keyframe counts, depacketizer errors, requested NACK sequence count, source-repeat and timestamp-mapping coverage |
| Presentation | FPS, render latency/gaps, decoded-slot wait/replacement/pending count, CPU/RAM | Decoded wait distribution, upload/draw submission/Present CPU distributions, optional asynchronous GPU upload+draw timestamps, CPU fallback conversion timing and GPU query health |
| Recovery/lifecycle | Join, keyframe request, disconnect/preview close/reconnect events | First decoded/presented frame timing; IDR arrival recovery duration/episodes/peak, stale compressed backlog drops, final partial stage rows |
| Relative latency | Control RTT | Clock offset and uncertainty, send-to-UDP-marker/AU/Present estimates, capture-to-Present estimate when source/output timestamps can be associated |

## Interpretation rules

- Stage rows use microseconds and include `samples`, `retained_samples`, `mean_us`,
  `p50_us`, `p95_us`, `p99_us`, `max_us`. Count/mean/max cover the entire interval;
  percentiles cover at most its latest 1024 observations. Session whole-call
  latency percentiles retain the latest 4096 samples. Do not add stage p95 values
  together; each stage has a different sample population, including MFT polls
  returning no output. Zero or absent samples do not prove zero cost.
- `capture_api` follows the capture backend's existing capture-latency definition;
  it is not the screen's optical refresh timestamp. CPU process percentage is
  summed across cores (100% represents one fully busy logical CPU).
- `wire_*` counters cover datagrams arriving from the expected Teacher endpoint,
  before parsing/queue drops. Their arrival timestamp is userspace `recv_from`,
  not a NIC hardware timestamp. Ingress drops are local overload, not proof of
  physical link loss. Legacy `missing_packets` combines expired sequence gaps and
  later arrivals; use separate counters for diagnosis and never count both as
  distinct lost packets. Out-of-order-arrival count means arrival differed from
  the next release sequence, including ordinary queued packets. It does not by
  itself prove packet reordering on the wire.
- `keyframe_recovery` ends on arrival of an acceptable IDR, before successful
  decode/visible output. Decode failures and presentation timing must be checked
  separately. First-decode timing starts after decoder initialization; first-
  presentation timing starts in the preview owner loop. Neither is total process
  startup time.
- GPU timestamp queries are optional, four reusable sets, `DONOTFLUSH` polls:
  unavailable/not-ready/disjoint measurements never block presentation. GPU
  upload+draw time excludes compositor, monitor scanout and photons. CPU Present
  duration is the API call, not visible display latency. Query pending/skipped/
  invalid counts expose sampling coverage.
- Clock estimation uses four TCP timestamps with offset correction, half network
  RTT plus a 1 ms uncertainty allowance and expiry after ten seconds. Estimates
  assume no abrupt clock jump during the exchange; network asymmetry and clock
  changes affect accuracy. Treat variation within the logged uncertainty as
  inconclusive. Source timing is carried by a 20-byte RTP header extension;
  retransmissions preserve original timing. Max RTP datagram is 1220 bytes.
- Windows encoder input/output sample timestamps are associated in a bounded
  128-input timeline, including synchronous MFTs that buffer output. Repeated
  source captures retain exactly the same wall timestamp. Capture time is omitted
  (zero) if an output has no valid timestamp or its association has expired. Decoded output without a usable MF sample timestamp is
  counted as unmapped, and has no source-to-present estimate. macOS now associates
  actual VideoToolbox encoded output timestamps with bounded input source timing,
  and preserves the decoded callback timestamp. Invalid output times remain
  unmapped rather than borrowing the newest input. Capture-to-Present remains an
  application estimate rather than optical latency.
- VideoToolbox stage rows distinguish all submissions (`vt_encode_call`), time
  inside its synchronous session/callback wait (`vt_session_encode_wait`), calls
  returning a CMSampleBuffer, calls flagged `FrameDropped`, missing samples, and
  missing samples without the drop flag. This allows submitted/output/drop rates
  and latency distributions to be compared over the same one-second interval;
  input IOSurface copy and successful output normalization are separate stages.
- Actual optical glass-to-glass latency/text readability still require an external
  observation. AP contention, NIC/driver packet drops and GPU-driver internal
  queues cannot be uniquely attributed from application logs. Link rate is OS-
  reported capacity, not available bandwidth. A virtual NIC's rate does not
  describe the host Wi-Fi path. No speculative causes are presented as measured.

## Improvements bundled with collection

1. Windows MF output remains padded NV12 and uploads luma/chroma directly to D3D11.
   The pixel shader uses the existing BT.709 limited-range coefficients. Chroma
   samples use point filtering to preserve the existing 2x2 block color mapping;
   aspect-fit luma/BGRA scaling stays filtered. No scalar full-frame NV12-to-BGRA
   conversion or four-byte-per-pixel BGRA upload is needed on the GPU path.
   GDI fallback converts to the established BGRA representation. For a controlled
   comparison, setting `CLASSROOM_CPU_CONVERSION=1` restores decoder CPU conversion.
2. A dedicated UDP worker continues receiving while MF is busy. Its packet queue
   has capacity 2048 and maximum age 80 ms. Jitter storage is also capped at 2048.
   Backlog older than configured jitter budget + 120 ms is abandoned with an IDR
   request. Compressed reference chains are not arbitrarily skipped and played;
   H.264 recovery is explicit. The decoded waiting slot still holds one latest
   frame, with the existing presentation cap.
3. Both ends use TCP_NODELAY. One write carries the length prefix and Protobuf body.
   Student control output is bounded to 256 messages and connect attempts have a
   two-second timeout. RTP timestamp generation wraps correctly for long sessions;
   missing-sequence selection handles RTP sequence rollover, including reordered
   startup packets straddling 65535 -> 0.
4. After startup, a complete contiguous RTP access unit can leave jitter storage
   immediately. Incomplete units retain `video.jitter_buffer_ms` (20 ms default).
   A gap queues its NACK immediately; its independent `video.nack_repair_ms` budget
   defaults to 25 ms (valid 1–100 ms). Expiry still triggers normal H.264 reference
   recovery rather than arbitrary compressed-frame skipping. `repaired_gaps`,
   `fast_released_frames` and `nack_repair_wait` expose the actual path.
5. The Teacher dispatcher uses a bounded 4096-packet/TTL cache and a nonblocking
   socket to retransmit while capture/encode/tick pacing are busy. Media control
   forwarding is bounded at 256 queued events plus 256 pending critical events;
   excess reports are counted, critical overflow ends the session explicitly.
   IDR requests are coalesced independently and applied by the codec owner just
   before its next encode, even while the forwarded event queue is delayed.
   `nack_service`, `teacher_control_dispatch`, cache misses, repair errors and
   dropped report counts are logged. Send Mbps includes retransmission bytes.
6. Presentation is driven by fresh decoded output rather than a fixed independent
   phase. Its single pending frame remains latest-wins; minimum spacing is 90%
   of the configured frame interval and a rolling one-second budget strictly
   caps admission at the configured maximum (up to 30 FPS). No accumulated burst
   credit exists. `decoded_wait` and render-gap statistics measure actual effects.
7. The H.264 encoder uses the documented `CODECAPI_AVLowLatencyMode` VT_BOOL
   property before media-type negotiation, with a generic fallback if rejected.
   Acceptance/rejection is logged; low-latency buffering behavior must still be
   verified with the selected Windows driver. Only actual IDR NALs count as
   keyframes, rather than trusting an MFT clean-point attribute. Additional rows:
   `encode_keyframe_response` (request to encoded IDR, not receiver recovery),
   `encoder_input_to_send`, `fresh_capture_to_encode_age`,
   `fresh_source_capture_to_send`, and `teacher MF source association` with
   pending/evicted/unmapped inputs/outputs and keyframe-pending state. These are
   capture/API estimates, not optical latency.

Hardware codecs remain optional. This release does not claim hardware MF decode,
zero-copy surfaces, 40-PC scale acceptance or Windows runtime acceptance from a
cross-build. Benchmark/report console logs remain separate from this diagnostic
format; formal acceptance still follows `06_PERFORMANCE_ACCEPTANCE.md`.

## macOS decode optimization and diagnostics (2026-10-01)

VideoToolbox is asked for BGRA output so native conversion can replace scalar
NV12 conversion. Creation failure retries the default format; the existing
NV12/full-range conversion remains supported. Set `CLASSROOM_VT_NATIVE_BGRA=0`
on Student for a controlled comparison. The actual output pixel format is
logged; a requested format alone is not evidence. `vt_decode_submit`,
`vt_decode_wait` and `vt_decode_copy_convert` report bounded stage distributions.
This still copies pixels into native rendering storage, not zero-copy Metal.
Student topmost maintenance runs once per second rather than every 1 ms poll.

Adapter discovery uses one OS snapshot and logs `adapter_discovery_ms`. Teacher
polls the latest capture once without waiting after its pacing sleep, before
encoding, so a frame that arrived during the sleep can replace the older choice.
The automatic two-Mac runner waits for Student initialization before starting
Teacher's duration. `--student-connect-delay-ms` is a bounded diagnostic join
delay (0 by default), used to test late join without GUI actions.

## Native low-latency H.264 interoperability (2026-10-01)

Mac Teacher requests Apple's `EnableLowLatencyRateControl` encoder specification
at session creation, while retaining real-time operation and disabled frame
reordering. `VideoToolbox encoder mode selected` records the actual selected
mode; creation/configuration failure retries the previous compatible encoder.
Hardware acceleration remains optional for application operation. Set
`CLASSROOM_VT_LOW_LATENCY=0` on Teacher for a controlled compatibility comparison.
This is distinct from changing mutable compression properties after creation.
See [Apple's low-latency encoder API explanation](https://developer.apple.com/videos/play/wwdc2021/10158/).
The upstream0.20 builder did not expose encoderSpecification; the local source
patch is documented at `vendor/videotoolbox/CLASSROOM_PATCH.md`, with licenses
retained. No codec bitstream bytes are rewritten.

Adaptive bitrate updates first set VideoToolbox's live `AverageBitRate`
property. A successful update keeps the current compression session and H.264
reference chain, avoiding a synchronous session rebuild and its forced IDR.
Encoders that reject a live update retain the previous build-new-then-swap
fallback. Logs identify `update=live_property`; a fallback emits a warning.

Windows `decode_input_to_output` measures sample submission through actual output
association, including codec waiting. `student MF input output association`
logs bounded pending/evicted/unmapped inputs. Baseline comparison reduced mean
codec residence from roughly150ms with4pending inputs to9ms with0pending; CPU
decode-call time alone would have missed the old buffering. Stage means and
clock-adjusted source-to-present estimates still require the interpretation
rules above; they are not optical measurements.

Windows Teacher BGRA→NV12 now traverses each2×2 block once for both luma and
chroma, preserving the established BT.709 coefficients, averaging and rounding.
The `encode_color_conversion_cpu` stage continues measuring the actual path.
Byte-equivalence regression covers diverse pixels/row dimensions. An ignored
release timing probe `color::tests::conversion_timing_probe` compares both paths
on the target CPU, alternating order; it prints timings without asserting a
hardware-independent speed threshold.

## Room lifecycle diagnostics (protocol6)

Teacher logs dashboard Start/Stop stream IDs and roster online/offline events.
Student logs room registration separately from each presentation registration,
Start/Stop stream IDs, first presented frame and preview close reason. A room
cycle must preserve one client ID and create a fresh media summary per stream.
`--teacher-test-cycles` additionally logs GUI update timing once per second;
normal users do not receive that diagnostic traffic. Compare receive-Stop and
preview-close unix_ms on the same Student to measure local close responsiveness.
Do not compare raw unix_ms across machines or call this glass-to-glass delay.

## Local expiration versus missing sequences

`ingress_dropped_packets` counts UDP packets received but intentionally expired
or dropped by the bounded local queue. `ingress_reference_loss_events` counts
times the receiver notices new local drops and resets compressed references for
fresh-IDR recovery. It avoids requesting obsolete data via NACK. Both counters
are recorded in recovery intervals/final totals. Jitter missing/late counts are
not a pure network-loss measure; correlate them with local drops and wire counts.

## Silent-control release (2026-10-02)

`teacher control liveness timeout; releasing Student presentation` records the
five-second complete-control-message deadline. Compare it to
`student preview closed after Teacher disconnect` in the same Student log for
native teardown time; this excludes time until the deadline. A subsequent
`student first presented frame` verifies recovery after re-registration.
Process-kill tests intentionally lack a clean final summary in the killed log;
retain its restart log separately. These are lifecycle tests, not optical
latency or classroom-scale acceptance.

## macOS Observe capture and VideoToolbox selection — 2026-10-03

The Observe publisher records `capture_poll_wait` to measure how long the media
loop blocks waiting for a ScreenCaptureKit frame. Display-mode checks run on a
background thread; slow checks emit `ScreenCaptureKit display resolution check
completed` with `resolution_probe_ms`, so WindowServer queries cannot stall the
capture/encode scheduler. VideoToolbox requests hardware encoding when
available and logs the selected session backend. Hardware-enabled creation
retains a default-session fallback for machines where VideoToolbox cannot create
that session.

## Aggregate send and recovery diagnostics ? 2026-10-07

- sender_pressure / sender_ceiling_bps: local send abandonment or actual interval
  encoded rate beyond the 80%-of-wire-budget per-recipient ceiling. The controller
  needs sustained pressure before stepping bitrate/FPS; single IDR spikes are not
  proof of sustained link overload. Refinement uses the same ceiling.
- send_au_deadline_drops: recipient AU rejections (predicted budget) or actual paced
  sends reaching their absolute deadline. send_dependency_skipped_packets counts
  dependent datagrams deliberately omitted until an IDR; it is separate from
  send_abandoned_packets/send_errors. A preflight rejection adds abandonment but
  does not fabricate a socket send error.
- aggregate_pacer_wait_us / aggregate_pacer_denied: cumulative scheduled wait across
  video, repair and pointer threads, and rejected/expired scheduling reservations.
  Wait is summed across concurrent threads, not wall-clock session latency.
- udp_au_completion / udp_keyframe_completion: worker start through API send
  completion, not NIC transmission or receipt. fanout_handoff_wait_interval_ms now
  also includes pre-encode admission wait; capture remains latest-wins upstream.
- nack_packet_age: age from cache insertion to repair handling; cache miss counters
  expired/capacity/invalid/unknown use bounded recent eviction metadata. Unknown can
  include an old expiry, an uncached sequence or history eviction. stale_repairs
  counts cached repair packets rejected by TTL or the bounded batch deadline.
- Student recovery_packets_discarded / unrepaired_gap_resets /
  waiting_for_keyframe_ms: discarded packets before a recovery AU anchor, expired-gap
  resets and current waiting-IDR duration. Recovery completion still needs decode
  and render progress; accepting an IDR alone is not a visible-output guarantee.
