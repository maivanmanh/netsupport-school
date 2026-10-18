# Student screen recording — 2026-10-12

Owner-authorized implementation. A paired LAN validation on 2026-10-12 exercised
one Windows 11 ARM Parallels Student running the x64 build: two ten-minute rollovers,
automatic collection, short Stop, Teacher restart and concurrent Observe/presentation.
Three MP4s (600/600/19 seconds) passed independent decode/timestamp/hash checks.
See [runtime evidence and limits](../checkpoints/diagnostics/2026-10-12-student-recording-lan.md),
including a separate black one-shot screenshot observation. Physical Windows x64,
hardware MFT and multi-machine/classroom-scale validation remain pending.

## Operator workflow

Teacher toolbar → Screen recording. With a nonempty selection, Start targets the
selected online computers; otherwise it targets all online computers. Every target
must advertise recording capability (updated Windows Student). Unsupported peers
cause Start to fail explicitly. The dialog separates retained sessions into **Đang ghi / chờ tải** and **Lịch sử**
tabs, with live counts. A Stop request stays in the first tab until every Student
reports a terminal phase and all completed chunks are received; offline/stale
capture is not inferred to be finished. History includes completed sessions and
terminal recording errors, with explicit error labels. Each session card shows
machine name/count, local start date/time, phase, per-machine elapsed time/output
FPS/resolution, received/pending chunks, upload activity, errors and its selectable
folder path. Stop applies to that session, including all selected computers.
Closing the dialog does not stop capture. Open folder reveals its destination.

Right-click an online Student → **Ghi màn hình** starts recording immediately for
that computer and opens the dialog. Checkbox selection does not change this target.
The toolbar retains the selected/all-online workflow.

No recording popup, badge or recording window is introduced on Student. Existing
monitoring badges and OS permission behavior are independent and are not changed.
The helper is an ordinary owned background process, not an anti-tamper component.
No audio is captured.

Output paths:

- One machine: `Desktop/<Student name>/Recording-<time>-<session>/Part-0001.mp4`.
- Multiple machines: `Desktop/Classroom Recordings-<time>-<session>/<Student name>/Part-0001.mp4`.
- Subsequent chunks use `Part-0002.mp4`, etc. Duplicate/sanitized machine names
  receive distinct folder suffixes. Session IDs prevent previous files being replaced.

Each ten-minute chunk remains a separate playable MP4. The completed chunk uploads
while the next chunk records; Stop finalizes and uploads the shorter last chunk.
No whole-session merge, live video playback or cloud storage is included.

## Capture, encoding and MP4

Student owns one recording controller with a four-command bounded channel, one
capture helper per active session, one transfer worker and one latest-status slot
per retained session. Windows runs the helper as the same executable with an
internal startup flag and CREATE_NO_WINDOW, before ordinary UI/network startup.
This isolates DXGI duplication from existing monitoring/screenshot/Observe owners.
It does not create a classroom server or stream all screens into Teacher decoders.

DXGI captures display index 0 including the cursor. Existing aspect-preserving
`fit_even` bounds output to 1280×720, without upscaling; H.264 requires even pixel
sizes, so aspect ratio has at most the existing even-pixel rounding. Examples:
1920×1080 → 1280×720; 1600×1200 → 960×720; 1080×1920 → 406×720;
2560×1080 → 1280×540; 800×600 stays 800×600.

The existing Media Foundation encoder selects a usable hardware MFT with software
fallback, H.264, no B-frames, target 3 Mbps and a 30 FPS ceiling. Slower machines
produce their achievable rate; the Teacher displays measured FPS, not an assertion
that every machine sustains 24–30 FPS. One latest captured frame is retained; static
content can reuse it at the capped cadence. Missed ticks do not queue old frames.
The existing source-timestamp association maps delayed encoder outputs to actual
submission time. MP4 sample durations use that real timeline rather than the
encoder's nominal frame counter, so overload does not speed up the recording.

The pinned Rust `mp4` muxer converts Annex B NAL units to length-prefixed AVC
samples and stores SPS/PPS in avcC. Every chunk recreates the encoder and starts
at a verified IDR with SPS/PPS. One pending access unit establishes sample duration;
metadata/sample count is bounded to 18,100 samples per chunk. Completed files are
flushed/synced and renamed from `.partial` to `.mp4` before transfer can see them.
Rollover has encoder initialization cost; gapless/frame-exact rollover has not been
proven. Buffered MFT output at teardown is not drained; the last emitted picture is
held to the segment end. This must be evaluated on hardware before claiming seamless
recording. Ten minutes is the rotation target, not a hard real-time scheduler guarantee.

If capture geometry changes incompatibly, the current part is finalized when
possible and the session reports an error instead of stretching pixels. A forced
kill or power loss may leave the current `.partial` unplayable (up to ten minutes).
Already-completed parts remain transferable. Partial salvage is not implemented.

## Control, transfer and recovery

Existing authenticated Teacher loopback IPC accepts `recording_start`,
`recording_stop`, `recording_open`. Tools capability `recording:1` is Windows-only.
Durable desired session commands are delivered on the normal bounded Protobuf/TCP
poll. Repeated Start is idempotent. Status is built on a disk worker, replaces a
latest-status slot and uses the already-open tools connection. No frame bytes go
through Flutter or control messages. Report age over ten seconds is marked stale.
Terminal reports refresh less often; freshness is not inferred from an online roster.

Completed MP4 files use separate outgoing Student→Teacher TCP connections on the
existing tools endpoint. Admission checks the registered Student/IP plus a random
per-Student/per-session token, chunk index (1–49), length (≤1 GiB) and SHA-256.
Teacher chooses the destination filename; Student-supplied paths are not accepted.
Two uploads can run across the entire Teacher, at up to approximately 10.4 MiB/s per
slot (64 KiB per ≥6 ms). The cap is about 175 Mbps combined before framing overhead;
actual throughput/impact on presentation requires measurement. No classroom-scale
performance or automatic network-load adaptation is claimed.

Teacher streams into a temporary file, checks SHA-256, syncs and publishes it,
persists the receipt, then acknowledges completion. Student deletes only after that
ACK. Lost ACK/reconnect can retransmit; an existing receipt checks destination size
**and hash** before acknowledging without another body. A damaged/missing destination
is restored while Student still has the source. Retry restarts that chunk from zero;
byte-offset resume is not implemented. Status remains live during upload.

Teacher stores its session journal beside its profile/IPC files. Student spools
metadata and pending video under its per-user profile's `recordings/`, outside the
binary/update directory. LAN loss/Teacher exit does not end an active recording:
Student continues to the supplied deadline and retries completed chunks on reconnect.
An explicit Stop persists at Teacher and is delivered after reconnect. Sessions
have an eight-hour maximum as a disk/runaway bound. Agent shutdown asks its helper
to finalize, waits up to ten seconds, then kills/reaps only that helper. Abrupt Agent
loss stops the helper after a fifteen-second heartbeat timeout. A restarted Student
resumes pending uploads, reports interrupted capture, and does not restart capture
from a repeated Start for the same session ID.

Capture stops with a visible Teacher error when free Student disk falls below
512 MiB or that session's spool reaches 8 GiB. Teacher refuses a chunk unless its
Desktop has the chunk size plus 256 MiB free. Up to 32 sessions are retained; when
full, Teacher can evict a fully transferred terminal session from its journal while
preserving Desktop recordings. Student removes terminal metadata more than 24 hours
after its deadline only when no `.mp4` or `.partial` remains; at the 32-session
limit it may prune such terminal metadata after at least sixty seconds. Pending media is never
pruned to make space. Disk/queue errors are reported; files are not silently discarded.
Limits are currently implementation defaults; the first UI has no quality settings.

Authorization is the existing trusted classroom roster/IP and tools transport;
this feature does not upgrade plaintext LAN tools to TLS or introduce directory RBAC.

## Validation and next Windows checks

Commands/results are in `checkpoints/PROGRESS.md`. Local tests cover selection,
authentication, persisted Stop/receipts, interrupted uploads, checksums, idempotent
retry, global slot limits, real MP4 muxing/time and UI actions/staleness. Synthetic
H.264 fixture provenance is in `recording/fixtures/README.md`; it contains no screen
capture or personal content. ffprobe and ffmpeg independently inspect/decode the
muxed output. These checks do not exercise DXGI or MFT on this macOS host.

The one-VM runtime test above supplements the local tests; it does not replace
the remaining tests on an updated Teacher/Student pair from the same revision:
fully moving >20-minute workload; runtime 16:9/portrait/small displays; hardware
and software MFT; Observe and fullscreen presentation while recording; LAN drop,
Teacher restart, Agent crash/restart; full disks/backlog; resolution/lock changes;
and 1/5/10/20/40 physical-PC resource/network measurements. Preserve the existing
broadcast acceptance criteria. Fresh paired packages were built and the two test
Students updated through Classroom; no public release or classroom-scale pass is claimed.

## Automatic stop and disk guards — 2026-10-16

A recording a teacher forgot, or one that can no longer be collected, must not run
until a disk fills. These guards end capture by themselves; each records a reason
that the dialog explains in plain language. Anything already recorded stays queued on
the Student and uploads once the cause is fixed.

| Situation | Behaviour |
| --- | --- |
| Session length | New sessions stop after **60 minutes** by default, set in Teacher Settings → *Truyền tệp* (`tools.recording_max_minutes`, 5–480; the video is cut into 10-minute MP4 parts as it goes). The eight-hour value is now only the protocol/journal ceiling; journals written before the change keep their old deadline. The dialog shows the stop time and warns in the last 10 minutes. Reason `time_limit`. |
| Teacher disk at Start | Refused unless the Desktop drive has 2 GiB plus 0.5 GiB per recorded Student free. |
| Teacher disk while recording | Checked every 5 s (on status reports and polls). Under 5 GiB the dialog warns; under **1 GiB** every live session is stopped, reason `teacher_disk_low`. Students keep their finished chunks and retry the upload automatically after space is freed. |
| Student disk at Start | The helper refuses to start with under 1 GiB free. |
| Student disk while recording | Reports `free_bytes`; under 2 GiB the dialog warns for that machine; under 512 MiB capture stops after finalizing the current part (`student_disk_low`). |
| Student backlog | 8 GiB of unuploaded video stops capture (`spool_full`), now with an explanatory message. |
| Teacher unreachable | A Student that has had no live Teacher connection for **30 minutes** stops its capture (`teacher_unreachable`): nobody can see, stop or collect it. |

Not covered (see the owner's list): rolling over on a resolution change instead of
stopping, locked/black-screen detection, per-session size caps, Student sleep/logoff
(the next start reports `interrupted`), low memory, (the session length is now selectable in Settings).
The Student-side guards need an updated Student; the Teacher-side ones do not.

### Dialog behaviour (2026-10-16)

- Before a Student's first progress report the card says it is waiting calmly; it
  turns red only after 45 s of silence or a real fault.
- Running time is shown in red while recording; the status mark blinks.
- A session that was recording or waiting stays in the first tab while the dialog is
  open, so a Stop does not make it vanish; it appears under History when the dialog
  is reopened. Delete exists only in History.
- History shows one status icon per session (complete, error, interrupted, automatic
  stop by reason); the text is its tooltip.
- Stopping one computer from the right-click menu shows where the video is saved
  with an *Open folder* action.
