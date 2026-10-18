## 2026-10-12 — Screen recording dialog and context action

Owner requested clearer separation of active recording and finished content,
more readable items, and a recording command on a Student's right-click menu.

- Added counted **Đang ghi / chờ tải** and **Lịch sử** tabs. Stop-requested,
  offline/unknown capture and terminal sessions with pending/completed-but-unreceived
  chunks remain in progress until terminal capture and transfer completion.
- Session cards now lead with machine name/count, local start time and phase;
  per-machine rows separate capture metrics, file collection, upload activity and
  warnings. Completed history hides irrelevant FPS/freshness warnings; recording
  failures remain explicit. Folder path is selectable, with labeled Open/Stop.
- Right-click **Ghi màn hình** immediately starts only the context Student and
  opens the dialog; checkbox selection has no effect on that target. Request
  failures remain in the dialog; closing it does not send Stop.
- Validation: Dart format and diff whitespace checks; focused Flutter analysis
  of `lib/recording.dart` clean. Nine tests passed across `recording_test.dart`,
  `source_switch_test.dart`, `widget_test.dart`, including real context-menu
  interaction with another machine selected, pending→history transition, offline/
  interrupted/uploading classification, folder/Stop commands and 740×520 layout.
- Source/UI validation only: no paired app rebuild, signing or LAN redeployment
  in this change; existing running packages and recording backend are unchanged.
  Next step: paired build and visually verify the new dialog in the native Teacher
  before deploying it to the room. Previous MP4 runtime evidence remains separate.

# 2026-10-12 — Paired LAN recording validation on Windows ARM and Mac Student

Owner supplied Windows Student `.166` and Mac Student `.2`. Built Teacher, Mac
Student and Windows x64 Student/Guard together from `f3c1062`, label
`recording-validation-20261012`; verified fresh embedded ZIPs/manifests. Replaced
only the stopped existing Teacher bundle and used Classroom updater for both
existing Students; paths, device IDs and separate profiles/settings were preserved.
Owner granted macOS permissions manually; Mac screenshot/monitoring works after
restart. Both Students remain connected to current Teacher `.28` on the new build.

Windows 11 ARM Parallels (4 CPUs/8 GB, x64 Student emulation, software MFT) recorded
one >20-minute session with two actual rollovers and a mid-chunk Stop. Desktop
contains three independent H.264 960×720 MP4s, retaining the 1440×1080 source ratio:
599.979878 / 600.012067 / 19.056733 seconds, averages 24.18 / 25.44 / 26.60 FPS,
202,957,971 bytes total. Each chunk transferred while the next recorded; all ACKed
sources were removed. All files begin on keyframes, have strictly increasing input
PTS/DTS, match durable SHA-256 receipts and fully decode with VFR/demux time base.
Teacher restart near rollover did not change the recording helper PID; the pending
chunk uploaded after reconnect. Basic status UI, concurrent screenshot/Observe and
selected fullscreen Teacher presentation were exercised. Workload/task cleaned up;
Teacher idle, recording stopped, no pending uploads. Scope: functional validation,
not hardware-MFT/physical-x64, bulk or 40-PC performance acceptance.

A separate one-shot Windows screenshot was black although the VM was visibly normal;
recording MP4 samples remained correct and chunk 2's full black scan found no defined
>=0.2-second near-black interval. Root cause/reproduction remains open. Mac recording
is unsupported and rejected; its screenshot support is verified independently.
See [full evidence, exact paths, commands and limits](diagnostics/2026-10-12-student-recording-lan.md).
No recording implementation changes were needed. Verification: paired release
builds, updater reconnect confirmation, real runtime tests, independent MP4 tools
and `git diff --check`; previous implementation unit/lint results remain below.

Exact next step: reproduce/isolate the one-shot black screenshot with the same
foreground workload; then validate physical Windows x64, hardware MFT, additional
aspect ratios and two or more Windows recording targets before any scale claim.

# 2026-10-12 — Selected Student screen recording and automatic MP4 collection

Owner authorized implementation without available Windows test hardware. Added
Teacher toolbar/dialog Start for selected online computers (all online when no
selection), per-session Stop/Open folder and per-computer recording/transfer
status. Windows Student records display 0 locally, includes cursor, keeps source
aspect/no upscale inside 1280×720, caps at 30 FPS/achievable speed and uses the
existing hardware-MFT/software fallback H.264 encoder, no audio. No new recording
UI/notice is displayed on Student. Existing badge/permissions remain independent.

A private helper isolates DXGI from existing capture owners. Ten-minute parts
are independent IDR/SPS/PPS MP4s; completed parts upload while the next records.
Stop finalizes the short last part. Teacher writes Desktop/<machine>/<session>
for one target or a common session folder with machine subfolders for bulk Start.
Video/file work stays off Flutter/control/media threads; latest-frame and status
slots, command queue, chunk sample table, transfer slots and disk spool are bounded.
Two global upload slots are paced separately (~175 Mbps combined maximum before
framing). There is no new classroom server and no collection of full live streams
into Teacher encoders/decoders.

Added durable session/Stop state, per-Student random tokens and SHA-256 receipt
journal, checksum validation, ACK-before-Student-cleanup, retry after reconnect,
ACK-loss dedup with destination hash verification, interrupted capture reporting,
helper heartbeat/shutdown/reap, eight-hour deadline and disk/backlog limits. A
failed receipt persistence cannot authorize cleanup on a later retry. Interrupted
uploads restart that chunk from zero; partial-offset resume is not implemented.
Student restart resumes completed uploads, not capture. See
[recording design/limits](../docs/27_SCREEN_RECORDING.md).

Verified locally (source working tree, no deployment/package release):

- `cargo test -p classroom-app --bin Classroom`: **259 passed, 2 existing ignored**;
  includes nine recording tests for selection/auth/Stop, timing/AVC mux, interrupted
  TCP upload, retry/dedup, global slots, checksums, durable receipts and Student
  restart/idempotency. Log: `target/recording-full-rust-tests.log`.
- `cargo test -p capture-windows-dxgi`: **11 passed**, including 720p 16:9,
  4:3, portrait, ultrawide and smaller-source limits. This tests pure helpers,
  not real Windows capture. Log: `target/recording-capture-tests.log`.
- `flutter test test/recording_test.dart`: **2 passed**; selected Start,
  session Stop, closing without stopping and honest offline/stale/error status.
  Log: `target/recording-flutter-tests.log`.
- `flutter analyze lib/recording.dart test/recording_test.dart`: no issues.
  Including `lib/main.dart` reports its two pre-existing info-level lints
  (unnecessary typed_data import, null-aware element suggestion); no new errors.
- `cargo clippy -p classroom-app --bin Classroom --tests` completed with existing
  repository warnings. Windows role checks used the rustup stable compiler and
  matching cargo-clippy driver explicitly (Homebrew tools do not own the installed
  Windows std target):
  `RUSTC="$(rustup which --toolchain stable rustc)" CARGO_TARGET_DIR="$PWD/target/recording-cross-stable" /Users/mvmanh/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo-clippy clippy -p classroom-app --bin Classroom --bin ClassroomStudent --bin ClassroomGuardService --target x86_64-pc-windows-gnu`.
  Completed for all three roles; existing warnings remain, no warnings in the new
  recording modules. Log: `target/recording-windows-clippy.log`.
- Scoped Rust formatting and `git diff --check` pass. Full repository formatting
  already differs in unrelated source; those files were preserved.
- The real synthetic H.264 mux test produced `target/recording-mux-test.mp4`.
  `ffprobe` reports H.264 1280×720, two frames, one-second duration and no audio;
  `ffmpeg -v error -i target/recording-mux-test.mp4 -f null -` decodes successfully.
  Fixture is generated solid blue, with provenance beside it. This is file-format
  evidence, not Windows MFT runtime evidence.

Remaining risks: **no real Windows test**; ten-minute rollover reinitializes MFT
and may introduce a gap, buffered final MFT outputs are not drained, and forced
kill/power loss may lose the current unfinished part (up to ten minutes). Capture
geometry failure stops with an error. Actual 24–30 FPS, concurrent Observe and
presentation, disk limits, sleep/lock and 40-machine recording/network overhead
remain unverified. No existing broadcast acceptance result is upgraded by this work.

Exact next step: when Windows hardware is available, build Teacher and fresh
Student/Guard packages from this revision together per PROJECT_NOTES.md, then
record >20 minutes of moving/text content with two rollover boundaries, Stop
mid-chunk and verify playable Desktop files/timestamps/FPS/cursor. Follow with LAN
loss/Teacher restart/Student restart and parallel Observe/presentation, then the
physical-PC ladder. No build package was shipped or running deployment replaced.

# 2026-10-10 — Lock Mac Student to Teacher presentation

Incoming Teacher screen presentation on macOS now temporarily engages AppKit
presentation options: hide Dock and menu bar, suppress Cmd+Tab/app hiding, and
disable force-quit/session-termination shortcuts while the presentation window is
active. The borderless minifb window is activated/keyed so input stays on the
presentation surface. A scoped guard stores and restores the exact original
presentation options on normal stop, control disconnect, renderer failure, or
window teardown; the separate Student-source broadcast path is unchanged.

`cargo test -p renderer-macos` passed (the crate currently has zero tests).
The paired release build `paired-macos-share-focus-20261010` completed for Mac
Teacher, Mac Student, Windows x64 Student and the two fresh bundled Student ZIPs.
Artifacts are isolated under `target/mac-focus-share-build`; the active Teacher
and Student deployments were not overwritten. `cargo fmt --all -- --check`
still reports formatting differences in existing unrelated workspace files, so
only the changed Rust file was formatted. Runtime lock/unlock verification on a
real Mac Student is pending; deployment must first use the owner's stable Student
path and any macOS privacy prompt must be granted manually.

Owner then requested deployment to Mac `.2`. The Teacher updater replaced only
`/Users/mvmanh/Classroom/Classroom Student.app` and launched build
`1791625411-paired-macos-share-focus-20261010`; `.94` and `.106` were untouched.
The first attempt failed to reconnect (`No route to host` to Teacher `.206:48900`)
and the updater automatically restored the previous Student. After the owner
granted Local Network to the exact bundle path above and the old app was restarted,
the updater retry completed; Teacher confirms `.2` online on the new build.
Presentation hide/restore behavior still needs a live Teacher-share test on the
Mac Student; deployment confirmation alone is not runtime verification of focus
locking.

# 2026-10-10 — Windows browser discovery and saved website suggestions

Headless browser discovery reads Windows environment variable names without
assuming their casing, then checks the standard Program Files and Local AppData
locations for Chrome/Edge. Chrome and Edge headless mode are supported on
Windows; a configured executable path remains available for nonstandard installs.
The website textbox now autocompletes saved groups by URL/host, shows the owning
list and dependency count, switches to the saved list when selected, and reuses
the existing scan without creating a duplicate or rescanning. A matching saved
group changes the action to “Use saved group”; explicit Rescan remains available.

Dart analyzer reported no issues and `git diff --check` passed. The Dart CLI then
reported that it could not update its analytics timestamp under the sandboxed
home-directory permissions. This UI follow-up has not been rebuilt or deployed.

# 2026-10-10 — Website groups and bounded dependency scan

The Internet Allow list UI now supports saved website groups. Allowing a new
HTTP(S) site starts an isolated headless Chrome/Edge scan with a 15 s deadline,
three scrolls, an inactivity cutoff, a 64-domain cap and 30 MB/2000 request caps.
A fresh temporary browser profile is closed and removed after each scan. The
Teacher persists scanned time, status, resource kinds and per-domain selection
in `console-settings.json`; adding an already saved site reuses it without a
scan. Rescan is explicit. Partial results survive timeout/cancellation.

Successful CSS, JS, images, fonts and manifests from the main frame are selected
by default. Same-site top-level redirects only are followed; external navigation,
iframes, ad/tracker hosts and non-GET/HEAD requests are excluded. Main-frame API,
Fetch and WebSocket domains are reported as unchecked review entries. A domain
serving both static files and APIs is still granted as a whole if selected,
reflecting the WFP/DNS domain-level rule granularity. Domain unions are deduped;
removing a group preserves a domain used by another group. Existing manual rules
and saved groups without browser scan remain available.

Focused Flutter tests pass (URL validation, static/API classification, tracking,
cap, persistence, legacy list loading and shared-domain removal). Dart analyzer
is clean. A real local Chromium fixture verifies static resources, API reporting,
iframe and external navigation rejection, POST rejection, timeout preserving
partial dependencies, and Stop/cancel. A bounded real `https://flutter.dev`
scan completed in about 3 seconds and found `cdn.jsdelivr.net`,
`fonts.googleapis.com`, `fonts.gstatic.com`, `www.google.com`, and
`www.gstatic.com`. Results are in ignored `target/website-live-scan.json`.

Windows `.94` was initially offline; its pre-existing
`Classroom-Manual-Dev-Student` task referenced a deleted executable. The paired
build completed, and only that exact missing executable was restored at the
existing task path. The task started the new Student and Teacher confirmed it
online with Windows x64 and Guard capabilities. Other historical tasks and
binaries were left untouched. A temporary allow-only website policy was sent
to `.94`, its Student completion was confirmed in Teacher job state, and it was
then restored to `open`; Teacher reports no active internet restriction. The
follow-up HTTPS check over SSH was blocked by SSH authentication rejection, so
network-level allow/deny behavior on Windows is not claimed as runtime-verified.
No saved website preferences were changed by this policy check. The Internet
access window is open for `.94` for owner validation. The feature is Teacher-only;
the existing protocol and Student policy handler required no source changes.

Next: owner can enter a site in the open Internet access window and confirm the
saved dependency group and student browsing behavior. If Windows network
verification is needed, restore SSH access and retry the HTTPS allow/deny probe.

# 2026-10-10 — Repair Observe startup texture readiness deadlock

Owner reported Observe stuck at Connecting after the blurred-preview change.
Runtime observe-ui logs confirmed source_sequence advancing (145 to 405),
connected=true, but sequence=0 and copy_attempts=0. The earlier diagnosis that
native connectivity was delaying readiness was incomplete: Flutter Texture was
not mounted until its copied sequence advanced, which requires rendering that
Texture first. Keep Texture mounted below the loading overlay, then fade away
only the overlay over 500 ms after copied pixels are ready. Initial/reconnect
labels remain separate. The regression now models source frames arriving before
copied frames and asserts Texture is present during loading. Focused startup
regression, Dart format and git diff --check passed.

Paired build `paired-observe-texture-startup-fix-20261010` completed (Teacher,
Mac Student, Windows x64 Student and bundled fresh ZIPs), installed at the stable
Teacher path and restarted. Actual Observe windows passed on Mac `.2` and
Windows `.94`: 1920x1080 native pixels copied, sequences 98/108 and measured
render intervals 15.31/19.73 FPS. This is startup repair evidence, not a sustained
performance benchmark. Both test windows were closed after verification;
Teacher remains running. No Student redeploy or privacy grant change.
Evidence: `target/observe-texture-startup-runtime.json` and
`target/build-observe-texture-startup-fix.log`. Exact next step: owner visually
checks initial blur/fade and normal Observe/Control use.

# 2026-10-10 — Blurred thumbnail preview while opening Observe/Control

Observe/Control windows now use the selected Student's latest roster thumbnail
as a temporary blurred background while the live media path initializes. The
preview is staged only when its reported age is at most 10 seconds, deleted
after the child window reads it, and omitted when unavailable (including the
ListView/no-thumbnail case). A 500 ms fade transitions to the live texture only
after a fresh frame is available. “Control & present Student” carries the same
preview through the deferred source-switch handoff. Existing reconnect UI is
unchanged.

Paired build `paired-observe-thumbnail-loading-20261010` completed for Teacher,
Mac Student and Windows x64 Student; Teacher bundles the fresh Student ZIP.
Teacher was installed at `target/present-recovery-build/Classroom Console.app`,
build ID `1791607385-paired-observe-thumbnail-loading-20261010`, and IPC became
ready with Mac Student `.2` online. The current roster did not report Windows
`.94` online. No Student was redeployed. Dart format and `git diff --check`
passed. No tests were run. Runtime visual confirmation of the blurred preview
and 500 ms live transition remains pending.

# 2026-10-10 — Keep initial Observe loading state until native media is live

The initial Observe surface could briefly present reconnect UI because a cached
native sequence was treated as enough to end fast startup polling, even while
the receiver reported `connected=false`. `live` now requires the native receiver
to be connected, startup polling stays at 100 ms until live/error/timeout, and
the blurred thumbnail remains until the first usable live frame. Reconnect UI
and the last Student frame are retained only after the session has been live
once. A regression test covers initial `connected=false` followed by true.

Teacher presentation controls now distinguish sharing the Teacher screen from
presenting a Student. Labels explain the audience and action, a divider separates
the Student-specific controls, and the Control/present action is hidden while
the Teacher screen is being shared; that state exposes Stop sharing. Student
source switching and Back to Teacher remain available while a Student source is
being presented. The per-Student context menu uses the same clearer label and
is disabled during Teacher-screen sharing.

Focused Flutter verification passed: the new Observe startup regression,
`source_switch_test.dart`, and `widget_test.dart`. The full
`observe_test.dart` suite still has failures in older cases whose test agents do
not declare the Student online and whose 800 px test window overflows the
existing Observe toolbar by 57 px; those are outside this patch. `dart format`
and `git diff --check` passed.

Paired build `paired-observe-first-live-toolbar-v2-20261010` completed for
Teacher, Mac Student and Windows x64 Student. Teacher is installed at
`target/present-recovery-build/Classroom Console.app`, build ID
`1791608444-paired-observe-first-live-toolbar-v2-20261010`. IPC became ready;
Mac `.2` and Windows `.94` are online. Students were not redeployed. Runtime
visual confirmation of the startup transition remains pending. See
`apps/classroom_console/lib/observe.dart` and `main.dart`.

# 2026-10-10 — Keep Teacher capture errors out of Observe/Control windows

The Observe UI previously fell back to `Agent.error`, which mirrors the
Teacher-wide snapshot error. A Teacher ScreenCaptureKit/TCC failure could
therefore appear inside an unrelated Student Observe/Control window. The window
now renders only its own view/mode/native errors and shows the shared connection
error only when its IPC connection is down. The Teacher console continues to
receive the global error. Paired build
`paired-observe-error-scope-20261010` completed and was installed at the stable
Teacher path, build ID `1791606965-paired-observe-error-scope-20261010`. Both
`.94` and `.2` reconnected online. Dart formatter and `git diff --check` passed;
no automated/runtime reproduction was run. Manual confirmation with an open
Observe window and a separate denied Teacher share remains. See `observe.dart`.

# 2026-10-10 — Add Student IP to Observe/Control window title

Teacher now passes the selected Student endpoint IP alongside its hostname when
opening the independent Observe window. The native window title keeps the IP
while changing its mode label between `Observe` and `Control`. Paired build
`paired-observe-window-title-ip-20261010` completed and was installed at the
stable Teacher path; build ID `1791606608-paired-observe-window-title-ip-20261010`.
Mac `.2` and Windows `.94` reconnected online. Dart formatter reported no changes
and `git diff --check` passed. No runtime title screenshot or automated tests run.
See the current change in `apps/classroom/src/teacher_agent.rs`,
`apps/classroom_console/lib/main.dart`, and `apps/classroom_console/lib/observe.dart`.

# 2026-10-10 — Temporarily hide monitoring badge during classroom sharing

When a Student receives a Teacher or class-source presentation, its badge
overlay is destroyed for the presentation duration while the desired ON state
and persisted preference remain unchanged. A Student selected as class source
sets suppression before capture and waits for UI-thread confirmation that the
overlay is hidden; if it cannot confirm within two seconds, capture fails closed
instead of broadcasting badge pixels. Overlay is recreated after sharing ends.

Paired build `paired-badge-hide-during-sharing-v2-20261010` completed for Teacher,
Mac Student and Windows x64 Student/Guard. Teacher installed at
`target/present-recovery-build`, build ID
`1791605670-paired-badge-hide-during-sharing-v2-20261010`. Windows `.94` updated
and confirmed build ID `1791605697-paired-badge-hide-during-sharing-v2-20261010`.
Mac `.2` received the update but did not reconnect, so Classroom automatically
restored its previous binary; it is currently online on the prior build. Build
and runtime share-flow behavior remain unverified on Mac. No tests were run per
workflow direction; `git diff --check` passed. Teacher/student suppression still
needs a manual share/stop check on the available endpoints. See badge diagnostics.

# 2026-10-10 — Monitoring Badge toggle UI refinement

Distinguished badge controls from Activity Monitoring by using a badge icon in
the toolbar/menu/dialog. The bulk dialog now has one state-aware action (turn on,
turn off, or turn on for all when a selection is mixed). Right-click toggles one
Student directly with a state-aware label and no confirmation. Flutter format,
analyze and `git diff --check` passed. Paired build
`paired-badge-toggle-ui-20261010` rebuilt Teacher and both Student packages;
Teacher is installed at the stable path and running build ID
`1791604992-paired-badge-toggle-ui-20261010`. Both `.2` and `.94` are online.
Student packages were built but not deployed because this revision only changes
Teacher UI. Manual visual confirmation of the new controls remains. See the
earlier badge diagnostics below.

# 2026-10-10 — Student Monitoring Badge, bulk control and 75% background

Teacher toolbar now explains that the eye button opens the badge controls for
selected Students, or the whole room when none are selected. Per-computer context
menu remains available. The red background is composited at 75%; text and pulsing
dot stay opaque. Badge state persists through disconnect and Teacher restart, and
only an explicit Teacher OFF removes it.

Paired release `paired-monitoring-badge-final-20261010` rebuilt Teacher, Mac
Student and Windows x64 Student/Guard from one source tree. Teacher was installed
at `target/present-recovery-build`. Windows `.94` and Mac `.2` updated through the
Teacher updater; the first Mac attempt rolled back after a reconnect timeout, so
it was retried after Teacher restart. The retry completed with the exact version
and build ID, and both Students are online.

Verification: five focused Rust badge tests passed; Flutter analyze and the
focused dialog test passed. On both real Students, the runtime test passed bulk
enable/native display acknowledgement, individual OFF isolation, latest-command
ordering, and five repeated native create/destroy cycles; prior preferences were
restored. A Teacher restart while both badges were ON also preserved desired state
and both Students reconnected with ON acknowledgements. The subsequent Mac retry
was again verified by the same dual-Student runtime test. An external Windows
EnumWindows probe via scheduled task did not return its JSON; Windows Student's
native overlay/style checks and display acknowledgement ran successfully, but the
separate external style enumeration remains unverified. 35–40 physical Windows
PC validation remains a classroom follow-up. See
`diagnostics/2026-10-10-student-monitoring-badge.md`.

## 2026-10-10 — Windows .94 Copilot enforcement retest

Fixed invalid WFP_UINT8 weight 16 and Guard mutex blocking tools heartbeat during
policy application. Paired Teacher/Students rebuilt; .94 updated through Classroom
and confirmed reconnected with its original identity. 15 Guard tests passed.
Actual Windows DNS/HTTPS probes confirm Copilot blocked while GitHub/ordinary
websites remain accessible, then restored on unblock; policy jobs now complete.
Restored elevated Student connection to Teacher .50.206:48900 and saved adjacent
room configuration for direct launch. Signed-in VS Code Chat and real 35–40 x64
lab checks remain. See [evidence and next checks](diagnostics/2026-10-10-copilot-wfp-windows94.md).

## 2026-10-09 — Independent Copilot network block and extension Rescan

Added independent all-capable-online Copilot checkbox with confirmed X/Y, shared
Guard DNS + WFP IPv4/IPv6 blocking, policy coexistence/persistence, bounded endpoint
refresh and transactional filter replacement. Rescan snapshots all online Students,
supports Stop, deduplicates IDs, highlights newly found rows at the top; initial
load puts checked rows first. Extension Apply now confirms VS Code restart.
Mac extension listing is separate from Windows-only enforcement. Cargo check and
Flutter analyze and paired build passed, no automated tests run. Teacher updated;
both Student updates pending because `.155` and `.254` are unreachable via
ping/TCP22/SSH. Owner was asked to confirm power/network/IPs. Inline Chat blocking
and scan interactions are not yet runtime-verified. See
[implementation, limits and deferred lab cases](diagnostics/2026-10-09-copilot-network-and-extension-rescan.md).

## 2026-10-09 — Editable native Rust textboxes

Audited all three Rust input UIs and replaced append/replace-on-focus handling
with shared cursor/selection editing, native clipboard shortcuts and bounded
undo/redo. IP/numeric validation and reply limits retained. Mac cargo check and
Flutter analyze passed; no tests run. Paired build and all three deployments completed;
both Students confirmed new builds and are online. Embedded ZIPs match outputs;
manual Windows/Mac input checks remain. Also includes right-aligned toolbar.
Mac capture requires owner to regrant Screen Recording after signing to
`/Users/lolem/Classroom/Classroom Student.app` on `.254`, then restart Student.
See [details](diagnostics/2026-10-09-native-text-editing.md).

## 2026-10-09 — Align roster toolbar to the right

Owner requested the filter textbox, Rescan, selection and view icons stay flush
right. Split business tools from roster controls, with the former in an expanded
horizontal scroller and the latter anchored at the right edge in both wide and
two-row layouts. Flutter format/analyze and diff checks passed; no tests run.
Included in paired native-editor build and deployed to Teacher. Both Students
also updated for native text editing. Visual review remains manual.

## 2026-10-09 — Observe 15s wait and distinct initial loading

Owner confirmed reconnect works. Increased wait/auto-close to 15s and separated
initial dark-blue/white Connecting screen from interrupted-session reconnect panel
and countdown. Flutter format/analyze passed; no tests run. Paired build and
Teacher-only update completed; both Students online, fresh packages embedded.
Manual revised UI/15s expiry verification pending.
See [details](diagnostics/2026-10-09-observe-deadline.md).

## 2026-10-09 — Observe 10s recovery deadline and thinner crop grid

Replaced two-shot Observe reconnect with online-aware bounded retry and a
monotonic 10s deadline that closes the window/session. Recovery requires fresh
video; cached frames and acknowledgements do not extend waiting. Enlarged status
panel/countdown. Grid overlay reduced 52→34px; previews now crop to fill without
aspect bars. Flutter analyze/format passed; no tests run. Paired build and Teacher
deployment completed, fresh packages bundled; both existing Students online.
Real logout/network interruption verification remains.
See [details](diagnostics/2026-10-09-observe-deadline.md).

## 2026-10-09 — Compact grid and sharper preview tiers

Moved title/actions into a translucent white bottom overlay, removed footer
height, and preserved full screen aspect ratio. Negotiated sharper 480/720/960px
snapshots from capable Students, keeping legacy dimensions for older peers.
Raised bounded JPEG budget for larger snapshots and requested quality85 with
progressive fallback. Flutter analyze and native cargo check passed; no tests run.
Paired build and Teacher/both Student updates completed; both endpoints online.
Windows delivers expanded 860×540 previews. Mac TCC denies capture after signing:
paused for manual Screen Recording grant to `/Users/lolem/Classroom/Classroom Student.app`
on `.254`, then restart. Real classroom load/readability and visual review pending.
Follow-up: permissions alone did not restore LAN; after owner rebooted Mac Student,
same bundle connects and sends 830×538 previews. Both Students online/ready.
See [details](diagnostics/2026-10-09-sharp-grid.md).

## 2026-10-09 — Website icon and saved URL suggestions

Owner confirmed Open website working. Changed toolbar to matching icon-only
button and added saved-URL autocomplete; selecting a suggestion fills the input
without dispatching. Flutter analyze/format passed, no tests run. Teacher-only
UI deployment uses fresh paired packages without restarting compatible Students.
Paired build completed and Teacher reopened at the existing path; both Students
online, embedded packages match fresh outputs, saved URL history survives restart.
See [details](diagnostics/2026-10-09-url-suggestions.md).

## 2026-10-09 — Open website and short screenshot filenames

Added audience-aware Open website dialog with durable, one-click recent URLs
(20, deduplicated), validation and per-Student delivery status. Student opens
HTTP/HTTPS through the OS default browser. Screenshot names now omit Classroom
prefix/random suffix on both platforms. Mac/Windows Rust checks and Flutter
analyze passed; no tests run. Runtime browser/history restart verification and
physical 35–40-machine launch remain pending. See
[details](diagnostics/2026-10-09-open-websites.md).
Paired `paired-websites-20261009` build and both Student updates completed;
Teacher reopened at the same path. Mac Student log explicitly reports TCC
capture denied after signing: owner must grant Screen Recording at
`/Users/lolem/Classroom/Classroom Student.app`, then restart Student. No privacy
automation performed. Open website does not require capture permission.

## 2026-10-09 — Corrected direct Student Screenshot

Owner clarified that Screenshot must request a new capture on Student and
receive its binary, with no Observe/Control window. Removed the earlier bridge;
implemented token-bound tools request, bounded Student capture/PNG file upload,
and Teacher native Desktop save of the exact bytes. Rust check and Flutter
analyze passed; paired build completed. Teacher restarted at the existing path.
After owner deleted both home deployments, rebootstrap uses `/Users/lolem/Classroom`
and `C:\Users\lolem\Classroom`. Manual menu-to-Desktop verification remains.
See [details](diagnostics/2026-10-09-direct-student-screenshot.md).

## 2026-10-09 — Student context menu Screenshot

Added icons to all student context items and Screenshot, which opens/focuses
Observe, waits for a fresh image and saves PNG to Teacher Desktop automatically.
Includes one-shot acknowledgement, timeout and save-error feedback; preserves
existing Control mode. Flutter analyze and Rust check passed. No tests run.
Paired build `paired-screenshot-20261009` succeeded; both embedded Student ZIPs
match fresh packages. Teacher replaced at the same path and restarted; IPC and
Mac Student registration confirmed. Actual screenshot saving remains manual
verification pending (Desktop access may need the owner's first selection).
See [details](diagnostics/2026-10-09-student-screenshot-menu.md).

## 2026-10-09 — Teacher deployment cleanup

Removed 41 unused app bundles after full recoverable archiving; consolidated
remaining old home/build folders under `target/retired-deployments-20261009`.
Only `target/present-recovery-build` remains among the test `*build` folders.
Current Teacher processes were preserved. Student updater source now prunes its
backup after successful reconnect; not built/tested/deployed yet. Next: grouped
build and success/rollback verification, then remote deployment cleanup.
See [cleanup details](diagnostics/2026-10-09-deployment-cleanup.md).

## 2026-10-09 — Control & present Student admission fix

Follow-up after Accessibility grant: direct LaunchAgent restart still failed
input permission. Opened the same Mac Student bundle through LaunchServices
with the same explicit arguments; cooperative Control and Windows recipient
rendering passed. Both Student source directions now have functional evidence.
Teacher and both Students left online/idle; Mac's direct LaunchAgent unloaded.
See the report's follow-up section for the runtime launch requirement.

Confirmed the apparently inert toolbar/context action was rejected because an
already-ended Observe view still occupied the admission guard. Completed/error
views now retire before an explicit new Student presentation; active views stay
protected. Both UI actions retain rejection feedback in a SnackBar, and Teacher
logs rejection reasons. Rust coordinator regressions 4 passed; Flutter focused
regressions 7 passed; analyze clean. Grouped paired build
`paired-present-recovery-20261009` verified and launched from
`target/present-recovery-build/Classroom Console.app`, with fresh embedded Mac
and Windows Student packages.

Windows source → Mac receiver passed functional Control acknowledgement and
rendering after a retained failed Observe session. Mac source reached the input
initialization path but failed on missing Accessibility for
`/Users/lolem/Classroom-control-recovery-20261009/Classroom Student.app`.
Pause native Mac testing for the owner's manual grant, restart Student after
grant, then retry. Both Students remain connected; Teacher is idle.
See diagnostics/2026-10-09-present-recovery.md for evidence and limitations.

## 2026-10-02 — Flutter build bundle missing helper fixed

- Fixed `scripts/build-flutter-macos.sh` to embed and sign Rust helper in the Flutter build product before copying the completed bundle into target. Both app paths now include `Contents/Helpers/Classroom`.
- Verified executable helper and deep strict codesign on the compiled app. Restarted compiled GUI; fresh Teacher log confirms agent ready on 192.168.100.219, Flutter attached, and both .155/.254 Students connected with thumbnails received.
- Screen recording permission remains a separate permission banner. Live Observe/Control runtime validation is still in progress; this packaging fix does not establish those results.

## Live Observe / shared media / optional remote input — in progress

Owner authorizes live Observe, independent Teacher window, aspect fit by default,
and explicit Control with cooperative input default and optional exclusive input.
Plan: reuse the existing publisher/receiver session functions (no codec/RTP fork),
request one reverse publisher through the registered monitoring channel, and render
a native texture in a separate Flutter process/window. Video pixels stay out of
Dart. Bounded control and latest-frame shared-memory surface; stop/release on
window close, control EOF, room disconnect and input lease expiration.
Windows production Student remains Rust-only; Mac is a test Student. Student-to-class
routing is a future session topology using the same publisher, not implemented now.

## Grid size slider and Auto — 2026-10-02

Added bottom-right Grid-only slider, Auto and remembered size/mode. Equal columns
fill viewport width; height preserves16:9 thumbnail. Auto considers width/count/
height, avoiding title hidden below viewport. List/Compact sizing unchanged.
Flutter analyze clean,4tests passed incl slider/Auto/responsive740x520. Mac release
build/signature and visual QA recorded in checkpoints/evidence/grid-size-2026-10-02.
No Rust/Student/media changes; prior Inspect/Select/dots/Settings retained.
Next: owner visual review; real-time Observe/control deferred per explicit choice.

## Inspect/Select, status dots and native permission shortcut — 2026-10-02

Owner selected UI-only milestone; live Observe/control/files/messages deferred.
Default cards/rows open read-only Inspect; checkboxes only in toolbar Select mode.
Inspect follows snapshot updates; visible connected/ready labels replaced by
colored tooltip dots. Native startup permission advisory and Open Settings button
request first-use consent/open correct macOS Screen Recording pane.
Flutter analyze clean,3tests passed; Swift/Flutter release/signature passed.
Real two-online-Student UI: click Inspect and Select on/off verified; Open Settings
opened actual Screen & System Audio Recording pane. No OS toggle changed. Kept
Teacher engine/Student sessions; no new media acceptance claim. App/zip refreshed.
Evidence: checkpoints/evidence/inspect-select-permissions-2026-10-02/REPORT.md.
Next: owner UI review; live Observe implementation only under next authorization.

## Teacher minimizes on confirmed Sharing — 2026-10-02

Flutter console tracks presentation transitions and minimizes once only after
agent confirms Sharing. Initial attach to an already sharing agent does not
minimize; explicit tray Show restores then shows/focuses. Starting/capture failure
keeps error visible. Flutter analyze and2tests passed; regression checks no minimize
in Starting and exactly one on Sharing despite repeated notifications. Actual CUA
Share click hit TCC after rebuild; console stayed visible with permission error,
so successful native minimize during live sharing remains unverified this run.
Mac app/zip rebuilt; no Rust/Student/media change. Next: restore capture permission
for current build and verify native minimize on successful Start.

## Teacher toolbar redesign — 2026-10-02

Replaced oversized upper bands with38px brand/count/status bar and unified compact
action/search/rescan/selection/view toolbar. Contextual Share/Selected/Stop;
removed Ready-to-teach chip and duplicate classroom/selection bands. Responsive
selected toolbar wraps only when needed. Existing roster cards/rows unchanged.
Flutter analyze +2tests passed including visibility state/740x520. Mac app built,
signed, zip refreshed. CUA target launch timed out; identical build output launched
and screenshot visually verified, attached existing Teacher agent. No Rust/media
or Student changes; no new remote media acceptance claimed.
Evidence: checkpoints/evidence/teacher-toolbar-2026-10-02/REPORT.md.
Next: owner review of actual compact toolbar; deferred Windows/LAN tests unchanged.

## Compact roster / everyday Flutter Teacher console — 2026-10-02

Implemented Compact List without thumbnails, persisted current-room256-entry known
roster, online/name/numeric-IP sorting, Grid/List/Compact saved view and selection,
visible-online select/clear, rescan authoritative roster, command feedback and
permission message; reduced header/actions. No media pipeline or protocol change.
Flutter analyze +2tests passed; Rust fmt/strict Clippy passed; Mac Flutter build and
Windows Rust bins built (Downloads byte-verified). Real UI view/selection/offline
restart observed. Two-Student final class/selected/restart cycles received video,
zero decode errors after owner restored TCC; failed attempts preserved. Remote Mac
thumbnail capture unavailable; Windows thumbnail ready. Static content adaptation
means no FPS benchmark claim. Temporary jobs cleaned; Teacher open idle Compact.
Evidence: checkpoints/evidence/compact-roster-final-2026-10-02/REPORT.md.
Next: owner review; sustained activity/physical LAN acceptance separate.

## Silent stall timeout / automatic active-stream rejoin — 2026-10-02

Suspended Teacher helper9s without socket closure, then resumed. Both Students
released preview after5s control silence; closure6ms Mac/51ms Windows after local
timeout log. Both automatically rejoined same active stream without another Start,
decoded324 frames each, zero decode errors; RTP gap recovered via keyframe.
This is application silence simulation, not physical LAN packet blackhole proof.
Jobs cleaned; Teacher idle. No code/build change needed.
Evidence: checkpoints/evidence/silent-stall-2026-10-02/REPORT.md.
Next: sustained controlled activity benchmark; physical LAN outage/40-PC evidence
remains deferred until suitable hardware/access is available.

## Active-share crash / Student rejoin verified — 2026-10-02

SIGKILL Teacher helper during broadcast: both Students closed previews; Flutter
restarted helper; both reconnected in ~2–3s after their own disconnect logs.
Explicitly started new stream successfully. Force-restarted Windows Student while
Teacher kept sharing: joined existing stream automatically, decoded/rendered video.
Zero completed-session decode errors; no source/build change warranted. Cross-host
wall-clock skew prevents exact crash-to-close latency claim. No silent LAN outage
test in this run. Temporary jobs/processes cleaned; Teacher idle.
Evidence: checkpoints/evidence/active-crash-rejoin-2026-10-02/REPORT.md.
Next: bounded silent traffic interruption timeout test; sustained activity separate.

## Flutter Teacher / two Rust Students media verified — 2026-10-02

After owner permission and full restart, class/selected/Stop and idle Teacher
agent restart cycles delivered decoded and rendered video. Both Students rejoin;
selected Mac stream absent on Windows; preview closes 2–14 ms after Stop receipt.
Final class decode ~28.5 FPS, render ~24 FPS on both; one initial Windows join
FU-A error recovered via NACK/keyframe. No code/build change needed for this run.
Teacher CPU/memory telemetry unavailable in app sandbox; no resource conclusion.
Evidence: checkpoints/evidence/roles-integration-after-permission-2026-10-02/REPORT.md.
Next: controlled sustained activity and active-share interruption/rejoin tests;
physical 40-PC wired LAN / optical acceptance remains deferred.

## Two Rust Students / Flutter Teacher integration — 2026-10-02

Windows password SSH works outside sandbox; prior authentication failure was
/dev/tty sandbox denial, not invalid credentials. Windows temp folders cleaned,
latest binaries deployed and hash verified. Mac GUI-session Student and Windows
interactive Student both registered; both reconnect after Teacher agent restart.
Commands allocate distinct process stream IDs but all Share attempts hit Teacher
ScreenCaptureKit TCC denial, so no video/selective/Stop acceptance claimed.
Temporary jobs/processes cleaned up. Evidence and exact limits:
checkpoints/evidence/roles-integration-2026-10-02/REPORT.md.
Next: owner allow current rebuilt Teacher app Screen Recording, restart, rerun.
Reusable SSH/GUI/battery notes: docs/16_AUTOMATION_NOTES.md.

## Cleanup and role delivery refresh — 2026-10-02

Removed 53 disposable local target test artifacts, archived prior logs/configs/
images in checkpoints/evidence/cleanup-2026-10-02/prior-test-records.zip.
Removed 39 Mac .254 test folders, staged /tmp files and redundant binary backups;
kept stable Classroom-two-mac deployment and retained collected evidence locally.
Rebuilt Rust Mac/Windows x64 bins and signed Flutter Mac Teacher app; refreshed
Teacher zip. Updated both Rust binaries on .254 with matching SHA256. Copied and
byte-verified Classroom.exe/ClassroomStudent.exe in Downloads. No new functional
change or media runtime acceptance claimed. Windows .155 refused SSH key and
provided lolem password, so its cleanup/deployment is pending working credentials.
Build receipts and hashes: checkpoints/evidence/cleanup-2026-10-02.
Next: deploy dedicated Rust Student on Windows when SSH is restored, then verify
Flutter Teacher thumbnails, class/selected Start/Stop and Teacher restart with
both Students (Mac test peer launched in GUI session).

## Mac Student thumbnail permission verified — 2026-10-02

Same ClassroomStudent binary denies capture when launched directly under SSH,
but succeeds in gui/501 via an ephemeral LaunchAgent. Three 414x270 real JPEGs
received by Teacher with state=ready. Test ended and job booted out; no auto-start
installed. Initial intermittent TCP unreachable recovered during GUI run.
Next: use GUI-session launch for Mac integration automation. Evidence:
checkpoints/evidence/student-network-retry-2026-10-02/thumbnail-gui-session.log.

## Mac Student connection retry — 2026-10-02

Two fresh ClassroomStudent headless idle runs from .254 to .219 succeeded:
TCP connected, Hello/Welcome completed, client IDs5/6. Installed dedicated binary
at ~/Classroom-two-mac/bin/ClassroomStudent. No system permission/firewall change.
Prior intermittent EHOSTUNREACH cause remains unproven; these are successful
registration retries, not video/restart acceptance. Thumbnail Screen Recording
still denied on the Mac test peer. Bounded test processes exited normally.
Evidence: checkpoints/evidence/student-network-retry-2026-10-02/REPORT.md.
Next: presentation integration using Flutter Teacher and Rust Student.

## Teacher/Student role boundary audit — 2026-10-02

Owner clarified permanent deployment: Teacher GUI is Flutter + Rust on Mac or
Windows; Student is Rust only and production targets Windows. Mac Student is
only a test peer. Recorded baseline in docs/15_ROLE_ARCHITECTURE.md and decision
log; linked existing architecture/plan/usage docs and replaced Flutter template
README. Windows Flutter Teacher build remains deferred until its build host exists.

Audit confirms Flutter only connects to Teacher IPC; full video stays in Rust.
Added ClassroomStudent Rust binary using the shared engine entry point. It forces
Student before reading profiles, uses remembered Teacher IP and rejects Teacher
role/agent/dashboard/setup flags. Idle starts without role launcher; native tray
and fullscreen presentation remain Rust. Classroom continues as Teacher helper
and legacy diagnostic CLI; shared modules still compile into Student, so this is
not a complete dependency-level split or removal of the old diagnostic dashboard.

Verified: formatting, strict project Clippy for both binaries, classroom tests
65 passed/1 ignored, Mac and Windows x64 release builds. Actual Student diagnose
with a stale Teacher profile logs role=student and saved IP; explicit Teacher
request fails before startup. Evidence: target/role-boundary-test. Windows
Classroom.exe and ClassroomStudent.exe copied byte-identically to Downloads.
No remote integration run required for this launch-only change; the existing
Mac .254 process-specific TCP failure remains unresolved, and stream-ID restart
acceptance remains incomplete. No Git repository/commit available.

Next: use Flutter Teacher + dedicated Rust Student for integration; resolve
Mac test-peer networking only if it is needed, prioritize available Windows
Student. Native fullscreen is required for receiving, not a Student console.

# Stream ID epoch follow-up — 2026-10-02

The owner granted Teacher Screen Recording and reopened the console. Teacher
capture now works: logs show about 29–30 capture/encode FPS and one encoder.
The macOS Student on `.254` registered successfully once (`client_id=5`), but a
fresh run continued returning `EHOSTUNREACH` while concurrent shell `ping`,
`nc`, and Python TCP probes to `.219:47800` succeeded.

Tried source-binding Student TCP to its selected adapter IP `.254`, retaining a
bounded macOS system-connect fallback. This did not resolve the failure. Strict
Clippy and focused Rust suites pass (Classroom 65 passed/1 ignored,
control-transport 8, protocol 1); both Release builds complete. Latest Mac
binary is deployed on `.254`; latest Windows x64 exe is copied and byte-verified
at `/Users/mvmanh/Downloads/Classroom.exe`. Hashes and logs are in
`checkpoints/evidence/streamid-process-epoch-2026-10-02/REPORT.md`.

No end-to-end stream-ID restart acceptance yet: `.254` stayed offline in the
Teacher roster, so no Student observed a presentation ID. Temporary sharing
was stopped and Student test processes were stopped; Teacher console is idle.
Firewall inspection on `.254` shows firewall enabled and an exception for
`~/Downloads/Classroom`, not the deployed path. This is not proven to explain
outbound TCP failure, and no firewall rule was changed.

Exact next step: check macOS Local Network/privacy and per-app policy for the
deployed Student executable against Terminal on `.254`; then perform one
registration, presentation, and Teacher restart test. Do not claim acceptance
until Student logs show the old and new stream IDs and resumed reception.

## Phase 2-H Flutter Teacher console — 2026-10-02

Implemented and built a native Flutter Teacher console, packaged at
`target/Classroom Console.app` and `target/Classroom-Console-macOS.zip`. Rust
remains the media engine and Student agent; bounded authenticated loopback IPC
carries roster/status and existing JPEG thumbnails. Responsive Grid/List, search,
selection, inspection, Settings, Share class/selected, Stop, hide-to-agent lifecycle,
saved view/connection settings and explicit Exit are wired. Sandbox-denied process
telemetry is marked unavailable rather than terminating media.

Flutter analyze clean; widget test passes repeated view/button clicks, selection
and 740x520 layout. Rust fmt/Clippy pass; 71 focused tests pass, one ignored.
Actual native GUI/agent run passed: local Mac `.219` Teacher with Rust Students on
Windows Parallel `.155` and Mac `.254`; both appeared online with thumbnails,
class sharing and Windows-only selective sharing produced receiver reports, and
Stop returned UI to Ready. Existing `.155` Classroom at `\Mac\Home\Downloads`
was already bound to default Student ports, so an extra Student was not launched
and no existing process was stopped. See
`checkpoints/evidence/phase2-flutter-2026-10-02/REPORT.md` and its logs.

The Windows Rust release was rebuilt, copied to `/Users/mvmanh/Downloads/Classroom.exe`
and byte/hash verified. Windows Flutter Teacher packaging was not required for
this setup: the owner specified Mac as Teacher and Rust-only Students. Physical
Windows Teacher GUI and 40-PC wired-LAN acceptance remain open. Mac `.254` showed
a large initial packet sequence/reordering gap before decoding/rendering; track if
it repeats. Exact next step: use the local Mac console in normal manual operation,
then separately investigate the Mac Student receive gap if reproducible.

## Phase 2-G Retina and input corrections — 2026-10-02

Completed: separate logical UI/backing framebuffer, 2x Retina font rasterization,
dynamic density refresh, 16:9 Grid preview regions and Grid/List center-crop fill.
Inspection retains complete aspect fit. Native completed-click latch fixes quick
taps lost between polls; zero-pressure queued down/up activates List in runtime.
70 focused tests pass,1 ignored; host/Windows strict Clippy/fmt/syntax and releases
pass. Both Teacher roles pass two-Student class/selected/tray/stop regression,
using separate runs; failed SSH and occupied-port attempts retained and harness
bounded/isolated. Windows copied/cmp Downloads and shipped/hash-verified stable
.155; Mac shipped/hash-verified .254. Native 2400x1520 frame for logical1200x760.
Owner physical trackpad/perceived Retina observation remains the exact next step;
physical scale/TCC remain deferred. Evidence: `checkpoints/evidence/phase2-retina-2026-10-02/REPORT.md`.

## Phase 2-F desktop lifecycle and interface — 2026-10-02

Implemented, built and shipped: prepared hidden first frame, OS system fonts,
hover/pressed/beveled buttons, native tray/menu bar/Settings/status, Teacher native
close-to-hide/reopen, hidden Student event host and persisted view/window size.
Both Teacher roles PASS three-machine class/selected sharing with native close
while presenting, reopen, Settings cancel and preview release. Mac actual saved
settings/restart PASS. 68 focused tests pass,1 ignored; host/Windows strict Clippy,
fmt and both releases pass. Windows Downloads cmp and stable remote hash verified.
No claimed fix for simultaneous local Mac ScreenCaptureKit callback timeouts;
remote Mac TCC and physical 40-PC acceptance remain deferred. Windows Settings
save/restart specifically still lacks runtime validation (cancel tested).
Evidence: `checkpoints/evidence/phase2-desktop-2026-10-02/REPORT.md`.
Next: owner UX feedback, then agreed Phase 2 roadmap; physical scale separate.

## Phase 2-E selected presentation — 2026-10-02

Status: implemented, built, shipped and real cross-machine verified. Share Selected
freezes selected online stable IDs, so excluded peers keep their desktop and are
never registered for UDP. Selected reconnect/late join stays eligible. Class Share
retains whole-room behavior; one capture/encoder per session. Protocol remains7.
Scope: owner authorized continuing Phase2; .254 thumbnail permission deferred.

Class then selected cycles PASS with Mac Teacher/two Students and Windows Teacher/
two Mac Students. Full receiver logs prove both get class stream1 and exactly one
gets selected stream2. No logged process/decode errors, one encoder, Stop releases
recipients. 67 focused tests pass,1 ignored; host/Windows strict project Clippy,
fmt, syntax and both Release builds pass. Windows copied/cmp to Downloads and
hash-verified stable remote path; latest Mac shipped to .254 stable path.

Two simultaneous-local-Mac fallback runs hit ScreenCaptureKit startup callback
timeouts; an experimental pump failed to resolve them and was reverted. Ordinary
single Mac idle capture passed in the final real-machine case. Do not call this
simulation issue fixed. Failed attempts retained; physical scale not benchmarked.

Evidence/limitations/checksums: `checkpoints/evidence/phase2-selected-2026-10-02/REPORT.md`.
No Git metadata/commit. Next: Teacher Settings/connection status and persisted view
preferences; investigate simultaneous local capture separately if needed.

## Phase 2 native console and background Student — 2026-10-02

Status: implemented and verified on local Mac .219 / Windows Parallels .155;
three-machine receiving verification complete. P2-A/B/C implemented: saved per-user role
and Teacher IP, direct startup, stable Student identity, bounded independent JPEG
thumbnail channel, native Teacher Ribbon/Grid/List/selection/inspection/Share/Stop.
Protocol7 requires updating all peers. Files/Devices are future placeholders.
No OS startup service, tray or live Student remote-control feature claimed.

Two-role pair, Windows Student kill/relaunch and two-Student mixed-OS tests PASS: two Start/Stop
cycles each, preview release, thumbnail pause/resume, one encoder and no logged
decode errors. Fixed accepted monitoring socket inheritance of nonblocking mode;
added delayed-read regression. System-font caching and redraw-on-visible-change
reduce repeated native UI work. Thumbnail capture/decode/size/age telemetry added.

72 targeted tests pass,1 explicit capture test ignored; fmt, Python syntax, host
and Windows strict project Clippy pass. Both releases built. Windows executable
copied/cmp to Downloads and hash-verified at .155 stable path; latest native binary
shipped to .254 stable path. Default profiles: local Teacher; remotes Student/.219.
All remote processes/test rules/tasks owned by runs are cleaned up.

Remote Mac .254 currently denies Screen Recording, so its thumbnail acceptance
requires owner action. Automatic approval review rejected persistent Windows
Firewall changes; stable deployment proceeded without them. Cross-machine tests
use temporary peer-restricted firewall rules; stable-path permission remains an
OS setup check. No physical 40-PC/optical Phase1 acceptance claimed.

Scope/tasks: `docs/13_PHASE2_PLAN.md`; usage: `docs/14_PHASE2_USAGE.md`.
Evidence, full logs, UI PNG exports, checksums and precise limitations:
`checkpoints/evidence/phase2-console-2026-10-02/REPORT.md`.
No Git metadata; no commit invented. Exact next step: grant .254 Screen Recording and verify both
Student thumbnails; stable-path firewall permission remains an OS setup check.

## Silent Teacher release and crash recovery — 2026-10-02

Status: fixed and runtime-verified. Student now expires silent Teacher control
reads after five seconds, releases fullscreen and re-registers; initial Welcome
and presentation reads have bounded deadlines. Partial framed reads are discarded
with their expired connection. Static screens remain alive through ping/pong.

Eight cross-machine lifecycle cases passed on Mac .219, Mac .254 and Windows
Parallels .155: Teacher suspension with TCP intact, Teacher SIGKILL/Stop-Process
and restart, Student kill/relaunch. Three visible local loopback cases also passed.
Timeout-to-preview-close was 69 ms on Windows Student and 3 ms on local Mac.
No logged decode errors in the successful Windows runs. This does not establish
physical cable-pull/OS-crash or 40-PC wired acceptance.

Verification: control-transport 7 passed; Classroom 54 passed, 1 explicit real
capture test ignored; fmt and strict host control/Windows app Clippy passed.
Mac and Windows releases rebuilt; Windows exe copied to Downloads with cmp.
Fault runners now handle log creation, real readiness markers, SSH warnings and
process-local PowerShell script policy. Full successful and failed-attempt logs,
checksums, exact reproduction and limitations:
`checkpoints/evidence/lifecycle-2026-10-02/REPORT.md`.

No Git metadata exists; no commit created. Exact next step: physical Windows
wired-LAN scale/optical acceptance when the lab is available; retain this build.

## Windows x86 SIMD BGRA to NV12 experiment — 2026-10-01

Status: complete for this optimization experiment. Windows x64 equivalence,
three-Student A/B, and one direct conversion probe were run; stop further tests
per owner request. SIMD is retained for its measured conversion-stage and CPU
reduction. The system-wide encode FPS did not show a stable increase.

Added runtime-dispatched SSSE3/SSE4.1 conversion of both luma and chroma in
`codec-windows-mf/src/color.rs`, preserving the scalar fallback and exact color
math. Windows startup logs the selected backend. Windows `.189` ran the direct
SIMD/scalar byte comparison across 2x2, odd-vector-tail 14x6, and 64x32 fixtures:
all 9 codec tests passed. A final direct 1080p comparison on `.94` also reported
`byte_identical=true`.

Before-change baseline: Windows .94 Teacher and three GUI Students, 60-second
student interval, passed with three connected viewers and zero receiver decode
errors. Analyzer reported Teacher encode 21.8 FPS; per-interval median while all
three were connected was 22.0 FPS. Color conversion stage per-interval median
of p50 was 6.98 ms; median of p95 was 7.77 ms. Baseline logs are preserved in
`target/four-machine-runs/20261001-215133-winteacher-75309`.

In the final Release A/B run, conversion p50 median fell from 6.96 ms to 3.96 ms
(about 43%); p95 median fell from 7.71 ms to 4.62 ms. Teacher encode mean for
intervals with all three Students was 22.0 FPS before and 21.4 FPS after; the
analyzer reported 21.8 and 21.1 FPS respectively. That is no demonstrated
overall FPS gain. Teacher median process CPU was 77.4% before and 66.2% after.
Student render FPS was effectively unchanged (baseline/post: local Mac 18.8/18.5,
remote Mac 22.1/21.6, Windows 19.8/19.9). All runs passed with three receivers
and zero decode errors. Keep the conversion change, but do not claim a whole-app
FPS improvement from these measurements.

The final direct timing probe ran in an unoptimized debug test executable and
reported scalar 216 ms versus SIMD 361 ms per 1080p frame, while bytes matched.
This conflicts with the optimized Release stage telemetry and is not a valid
Release performance comparison; the probe should be optimized before using its
timings. Per owner instruction, no more tests are scheduled in this experiment.

Formatting passes; Windows GNU strict Clippy passes with warnings denied for
project code; the Windows test target compiles; Windows x64 Release PE builds.
The full workspace test run has one unrelated VideoToolbox test failure on the
Mac host (`VTCompressionSessionCreate -12903`); 54 classroom-app tests and all
other tests reached before that failure passed. Final Windows release SHA-256:
`cc19808c5b921e47113308116435d56bfede07fc7c0db2bb9f0b5374ca8f2140`; it was
copied and byte-verified at `/Users/mvmanh/Downloads/Classroom.exe`.

Full logs: baseline `target/four-machine-runs/20261001-215133-winteacher-75309`,
luma-only intermediate `target/four-machine-runs/20261001-222037-winteacher-77992`,
and final Y+UV SIMD `target/four-machine-runs/20261001-222624-winteacher-78574`.
The analyzer and local benchmark show conversion was a real CPU cost; overall
encode throughput remains limited by other capture/MFT/scheduling factors under
this workload. Physical wired-LAN results may differ.

## Mac Teacher live bitrate update — 2026-10-01

VideoToolbox bitrate adaptation now updates the active session AverageBitRate
property first, keeping the existing H.264 reference chain and avoiding an
encoder rebuild/forced IDR. Rejected updates use the old build-before-swap
fallback. The focused native test confirms this host selects the live path and
the same decoder continues decoding; all6 codec tests pass. Strict Mac Clippy
and formatting pass. A fresh 3-GUI-Student Mac Teacher release integration
passes with one encoder and no decode errors; stable teacher encode28.2FPS.
That healthy run had no adaptive bitrate change, and is not a controlled A/B
FPS gain measurement. Windows Media Foundation already updates its active
bitrate property. Full evidence and limits in
[evidence report](evidence/macos-vt-live-bitrate-2026-10-01/REPORT.md).

Mac release rebuilt; Windows x64 release built and copied byte-identically to
Downloads/Classroom.exe. Windows exe hash is f7f77ee509c002fd6c64363dc58da46d1c6cef0e456b40e31748e8335d94a4e9.
No Windows behavior change. Next candidate is profile and reduce ScreenCaptureKit
BGRA copies/callback cadence before considering a larger native pixel-buffer
pipeline; avoid claiming a measured FPS gain from the bitrate setter.

## Windows Teacher with three GUI Students — 2026-10-01

Windows .94 Teacher with local Mac .188, MacStudio .2 and Windows .189 Students.
Baseline90s, controlled loss45s and three GUI-handler Start/Stop cycles all pass:
one encoder, three distinct endpoints, all Student sessions render with zero
decode errors, every process exits0. Normal Student18.8–20.0FPS; application
capture→present estimates42–74ms; explicit Stop closes1–81ms after receipt.
Windows VM Teacher uses software H264 Encoder MFT, encode p50/p95≈16/28ms.
No production defect demonstrated; existing release unchanged. Added reusable
Windows Teacher runner/shared analyzer mapping and worker roster barrier;
Python compile and complete real remote integration verified. First runner
PowerShell quote typo corrected before measurements. Full logs, hashes and
limits preserved in [report](evidence/windows-teacher-three-2026-10-01/REPORT.md).

Exact next step: preserve clean-image/latency baseline. Physical Windows
Teacher hardware/software coverage, wired1/5/10/20/40 scaling and optical
latency remain open/deferred by owner; VM/Wi-Fi results do not close these gates.
No Git metadata, no notifications, no rebuild for automation-only changes.

## Four-machine 1Teacher/3Student validation — 2026-10-01

LocalMac192.168.50.188 Teacher, MacStudio192.168.50.2 and Windows11VMs .94/.189
as simultaneous GUI Students. Full logs preserved under
[evidence report](evidence/four-machine-2026-10-01/REPORT.md).
Final90s workload,45s controlled loss and three Start/Stop cycles all pass:
three distinct endpoints, one encoder, all GUI sessions render, zero decode
errors and all processes exit0. Normal render24–27FPS; application latency
estimates56–75ms; explicit Stop closes3–41ms after receipt. These are not
optical timing or40-PC acceptance. FPS varies with VM/network/adaptation.

Fixed recovery after known local ingress expiry: discard obsolete reference
repair state and request a fresh IDR instead of retransmitting old frames.
Added ingress_reference_loss_events to interval/final logs and an RTP regression.
Added diagnostic roster barrier for lifecycle tests and reusable four-machine
runner/analyzer, with memory-only credentials and scoped resource cleanup.
115native tests pass,2ignored; strict platform Clippy/format/builds pass.
Final Windows exe copied and byte-verified in Downloads; final deployment
hashes verified on all remote machines. No notifications, no Git metadata.

Exact next step: normal use with these builds. Physical WindowsTeacher wired
scale1/5/10/20/40, optical latency and hardware/driver coverage remain deferred;
this mixedMacTeacher run does not replace those gates. Prior notes are historical.

## Clean release rebuild — 2026-10-01

At owner request, deleted the entire7.5GiB target directory (including its
runtime/test logs and caches), then rebuilt both releases from scratch.
Source and permanent checkpoints/evidence were retained. Mac release verified
as Mach-O64arm64 and `Classroom --version` succeeds. Windows release verified
as PE32+x86-64 and copied byte-identically to Downloads/Classroom.exe.
Mac build44.76s; Windows build49.58s. No source changes or additional media tests.
Existing compiler/optional strip warnings remain nonfatal.

Artifacts: target/release/Classroom and
target/rustup-windows/x86_64-pc-windows-gnu/release/Classroom.exe.
Mac SHA256 c6a127b647b2b5a3b8d66c0903ac430c24ddd9b5e9533b148d8c02685f7ab966.
Windows SHA256 1dc68f930dc655bdda764ad6816f7b1b0791dca5f367a085b21f5b3e55a0c200.
Build logs: /private/tmp/classroom-clean-mac-build.log and
/private/tmp/classroom-clean-windows-build.log.
Exact next step: use these fresh builds; physical LAN scaling/optical acceptance
remains deferred. No Git metadata, no checkpoint commit.

## Persistent Teacher room completed — 2026-10-01

Owner approved native roster and reusable Start/Stop; email/ntfy notifications
were disabled at owner's request. Dashboard and persistent control plane now
keep Students connected across presentations. New stream IDs/SSRC and feedback
filtering isolate sessions; one cancellable media worker, bounded stages and no
capture/encode while idle. Protocol6 requires matching executables.

Final both-direction Mac/Windows11VM run passes three Start/Stop cycles each,
one Student registration, three previews/Stop closes, zero decode errors and
clean process exits. Stop received→preview closed1–39ms (localStudentclock).
Windows GUI message-pump starvation found and fixed with a documented minimal
minifb patch; final sampled GUI update maximum49.58ms including pacing.
114native tests pass,2ignored; strict platform Clippy/format/release builds pass.
Windows release copied and byte-verified in Downloads. No Git metadata.

Evidence: [full report](evidence/windows-2026-10-01-room/REPORT.md).
Exact next step: normal use of this build; physical wired Windows scale/optical
and hardware coverage when available. Those formal gates remain deferred;
no Phase2 expansion. Prior checkpoints below are historical.

## Low-latency / 1080p optimization completed — 2026-10-01

Owner delegated autonomous code/test/analyze iteration; significant progress
sent through private Gmail and generic ntfy alerts. Formal physical LAN/optical
and further4K tests explicitly deferred. No Phase2 expansion.

Fixed observed Mac→MF four-frame hold using native Apple low-latency encoder
specification, with compatible fallback. Same-binary old-mode comparison:
154.18ms decoder residence and4pending versus8.74ms/0pending new baseline;
3-minute new run8.79ms/0pending. Capture→Present application estimate222.20ms
old versus52.81ms new, sustained52.20ms; not optical latency. Render26.7FPS
baseline/26.6sustained; no decode errors. Loss/lifecycle/fullscreen checks pass.
Windows Teacher conversion fused per2×2block with identical bytes; native
Windows alternating timing probe11.60→6.52ms/frame (43.77% lower conversion
cost). Full Windows Teacher FPS remains variable in VM; no throughput guarantee.

Evidence and limits: [report](evidence/windows-2026-10-01-latency/REPORT.md).
111native tests pass,2ignored;9Windows native correctness tests pass and the
ignored timing probe passes separately. Formatting/strict changed-platform
Clippy/release builds pass. Final Windows executable copied and byte-verified
in Downloads; exact-final-binary smoke check is recorded with the report.
No Git metadata, no commit. Vendored videotoolbox0.20 retains original licenses
and a minimal documented builder extension; no codec replacement/SPS rewriting.

Exact next step: use current optimized builds for normal use; physical wired
Windows ladder/optical latency/hardware-driver coverage when machines become
available. Avoid further speculative tuning with this VM alone. Historical
checkpoints below do not override this measured current status.

## Interop latency milestone verified — 2026-10-01

Native Apple low-latency encoder specification is accepted. First connected
baseline: Windows decoder residence9.12ms mean,0pending/evicted/unmapped,
source capture→present estimate52.17ms, render26.64FPS, zero decode errors.
Loss test residence8.75ms,render26.26FPS,zero decode errors; recovery succeeds.
This fixes the observed four-frame codec buffer without SPS rewriting.
Results: target/mac-windows-runs/20261001-182156-49931/analysis.json.

Windows color conversion now fuses luma/chroma into one2×2 traversal; byte-exact
regression passes. First full Windows Teacher test completed but shared VM
scheduling spikes prevent attributing overall FPS change. Next: controlled
Windows timing probe and final1080p UI/lifecycle/soak/compatibility regression.
Strict changed-platform Clippy and111tests passed (timing probe added afterward).
No physical LAN or4K expansion; those gates remain deferred by owner.

## Interop latency optimization in progress — 2026-10-01

Measured Mac→MF decoder retains four inputs despite enabled MF low latency.
Apple documents a distinct EnableLowLatencyRateControl encoder specification
at session creation. Existing Rust binding passes a null specification.
Plan: retain upstream videotoolbox0.20 source/licenses with a minimal builder
option, request native low-latency mode with compatible fallback, measure the
actual SPS and decoder residence. No SPS rewriting or codec replacement.
Formal physical LAN/optical gates deferred by owner; no further4K tests.

## Phase 1 Windows / Mac automated verification — 2026-10-01

Owner authorized both roles on Windows11 Parallels192.168.50.94 and local
Mac192.168.50.188. Functional tests, lifecycle/loss/fallback/UI/dimensions,
three simulated receivers and3-minute soaks ran; full evidence and limits:
[report](evidence/windows-2026-10-01-phase1/REPORT.md).

Fixed confirmed DXGI access-loss crash on transient DuplicateOutput denial;
connected retest survives1080p→4K→1080p, two capture recoveries and video resume.
Optimized exact2:1 resize with unchanged pixels;4K render7.31→12.26FPS and
capture mean74.42→26.59ms across two workload runs. Owner requested no further
4K testing. Added bounded decoder residence telemetry and configuration logs,
automated Windows interactive-session runner and repeatable native workloads.
110 tests pass,1ignored; formatting/strict platform Clippy/release builds pass.
Windows release copied and byte-verified in Downloads. No Git metadata/commit.

Remaining: Mac→Windows MF buffering about4frames; thread/profile experiments
were ineffective, defaults unchanged. Rejected VideoToolbox delay experiment
removed. Phase1 formal acceptance remains open for real wired Windows1/5/10/20/40,
optical latency/readability, hardware MFT/Win10/session transitions. Exact next
step: isolate codec buffering with existing residence metrics; use physical lab
for deployment/scale gates before Phase2. Earlier checkpoint statements below
are historical and do not override this verified status.

## Windows Student .94 retest — 2026-10-01

Teacher Mac 192.168.50.188; Windows Student mvmanh@192.168.50.94.
Existing latest release deployed; SHA256 verified as
5b0f94b7a936058c4c8c3c6a470c215b7c975e18c00a7fe6b7b81267eedfa3c6.
Interactive limited scheduled task launched Student in active console session;
Student task exit 0 and Teacher exit 0. Temporary task removed. Credentials
not saved. No production code or firewall changes.

Student: 1138 decoded /1075 rendered over 43.742 media seconds; decode26.02FPS,
render24.58FPS; zero decode errors, 2 depacketizer sequence-gap events,
4 legacy missing packets, 6 NACK sequences and 2 keyframe recovery requests.
Teacher retransmitted all6 requested packets, zero cache misses/send errors.
Render recovered after each gap; no unrecovered decoder failure.
Decode p50/p95 8.62/12.82ms; render CPU p95 0.466ms; final control RTT6.271ms.
Sample-weighted capture-to-present estimate207.34ms (906samples),
send-to-present169.57ms (1075samples). These are clock-dependent application
estimates, not optical latency. Largest interval capture-to-present p95 530ms,
maximum sample568ms: occasional freshness spikes remain.

Host telemetry identifies Parallels Display Adapter (WDDM),4virtual CPUs,
8GB RAM; this remains a VM test. No controlled moving-content workload or
optical measurement. Similar mean latency to previous .155 test, but workload
and network differ, so no causal hardware/performance comparison is claimed.
Full logs: checkpoints/evidence/windows-2026-10-01-student94/.
Next: investigate stage timing and recovery spikes with repeatable content on
this automated endpoint; real wired Windows scaling acceptance remains open.

## Mac Teacher to Windows 11 Parallels Student — 2026-10-01

Teacher 192.168.100.219; Student 192.168.100.155. SSH authenticated and
SHA256 verified Windows binary 5b0f94b7a936058c4c8c3c6a470c215b7c975e18c00a7fe6b7b81267eedfa3c6.
Student launched with CLI through an Interactive/limited scheduled task in the
active console session. Battery-safe task settings were necessary; initial task
attempts did not launch. Temporary task removed after completion. No firewall
settings or production code changed; credentials were not saved.

Student completed successfully: 689 decoded / 576 rendered in 24.047 seconds
of media, 28.65 decode FPS / 23.95 render FPS, zero decode/depacketizer errors
and zero final missing packets. One NACK sequence was retransmitted by Teacher.
Decoder: Microsoft H264 Video Decoder MFT (hardware decode is not established).
Renderer: D3D11, Parallels Display Adapter (WDDM), GPU NV12 conversion.
Decode p50/p95 8.19/16.16 ms; render CPU p50/p95 0.277/0.454 ms.
Sample-weighted source capture-to-present estimate 205.3 ms (512 samples);
source send-to-present estimate 168.3 ms (576 samples). These estimates depend
on clock synchronization and are not optical glass-to-glass measurements.
An initial control deadline retried successfully while Teacher was starting.

Full logs and derived stage metrics: checkpoints/evidence/windows-2026-10-01-parallels/.
This is a functional VM/Wi-Fi smoke test, not physical Windows LAN or scaling
acceptance. Next: automate repeatable workload and lifecycle/recovery scenarios
on this Windows endpoint; investigate timing from the complete collected logs
before changing codec settings. No claim of visually verified text quality.

## Two-Mac automatic coverage and measured fixes — 2026-10-01

Status: baseline, loss, late join, disconnect, reconnect and three remote headless
receivers exercised. All final functional checks passed; performance acceptance
is separate. See `checkpoints/diagnostics/2026-10-01-two-mac-analysis.md` for
measurements, caveats, exact verification and delivered binary hashes. Full
evidence is retained in `checkpoints/evidence/macos-2026-10-01-automated/`.

Changes: VideoToolbox native BGRA with NV12 fallback; actual encoded/decoded
timestamp association and bounded codec stage telemetry; one-second Mac topmost
maintenance; one adapter snapshot plus startup timing; post-pacing Teacher latest
capture refresh; Student-ready test handshake and bounded test-only delayed join.
The portable Windows release was rebuilt and copied to Downloads with a matching
byte comparison. macOS and Windows GNU checks passed; 109 native tests passed,
one explicit real-capture test ignored. No Git metadata exists, so no commit was
created. No optical/40-PC acceptance is claimed.

Repeat the complete small suite:

```text
CLASSROOM_LOCAL_IP=192.168.100.219 CLASSROOM_DURATION=30 CLASSROOM_SCENARIOS=baseline,loss,latejoin,disconnect,reconnect,fanout3 scripts/run-two-mac-test.sh
```

Next: use repeatable content/optical timing for a comparable performance verdict;
then real wired Windows scale evidence. Do not chase Wi-Fi jitter by lowering
bitrate solely because rendering is slower than decoding.

## Automated two-Mac CLI connectivity — 2026-10-01

Status: automatic deployment, visible Student presentation and loss-recovery
smoke tests verified between Teacher 192.168.100.219 and Student 192.168.100.254.

### Changes and verification

- `scripts/run-two-mac-test.sh` builds the native release, deploys with SSH key
  authentication, launches both roles with explicit CLI arguments, keeps SSH
  connected for Student's lifetime, and collects separate scenario logs. It
  waits for Teacher readiness, records exit codes and preserves full evidence.
- Detached `nohup` Student failed with OS error 65. Keeping the SSH session open
  succeeded with the same binary and IPs. No firewall or global privacy settings
  were changed; the precise OS policy attribution remains an inference.
- The analyzer validates each scenario separately: actual transmitted packets,
  decoded and visible rendered frames, final summaries and successful exit codes.
  An old failed run correctly returns failure, rather than treating TCP reachability
  or a roster entry as evidence of successful video.
- Native release build, `cargo fmt --all -- --check`, Bash syntax check and Python
  compilation passed. Production Rust/media code is unchanged.
- Evidence: `target/two-mac-runs/20261001-151956/`. Baseline: 388 decoded,
  348 rendered, 26.08 decode FPS / 23.39 render FPS, zero decode errors and missing
  packets. Loss scenario: 408 decoded, 342 rendered, 27.44 / 23.00 FPS; 12 induced
  packet drops, 12 requested/retransmitted packets, zero decode errors. One
  depacketizer error and one keyframe request show recovery was exercised.
  Both scenarios report `MEDIA_OK`; the owner confirmed visible sharing.

### Run again

```text
CLASSROOM_LOCAL_IP=192.168.100.219 CLASSROOM_DURATION=60 scripts/run-two-mac-test.sh
```

### Remaining limits and next step

These short Wi-Fi runs are connectivity/recovery smoke evidence, not standardized
workload, optical latency, Windows performance or 40-client acceptance. Keep the
full logs and run a longer repeatable workload when performance comparison is
needed. Final runner validation completed with exit code 0 in
`target/two-mac-runs/20261001-152146-30834/`: `MEDIA_OK`, 210 decoded / 184 rendered
frames, zero decode errors; four local ingress drops were repaired. This validates
the complete final build/deploy/start/collect/analyze command without manual clicks.

## Windows freshness and recovery scheduling fixes — 2026-09-28

Status: owner-approved fixes implemented; Windows release built and delivered.
Real Windows performance/runtime validation remains pending. This addresses the
latest wired-LAN findings below without changing the validated NV12/GPU shader
or CPU conversion fallback.

### Changes

- Healthy contiguous complete RTP access units bypass the fixed jitter hold
  after startup. Incomplete units retain the existing 20 ms reorder budget;
  missing sequences queue NACK immediately once observed and get an independent
  configurable 25 ms repair window. Unrecoverable compressed references still
  require IDR recovery. Startup reordering handles sequence rollover; rejected
  overflow packets cannot change the startup sequence. Storage remains bounded.
- A Teacher control/repair dispatcher retransmits cached RTP through a
  nonblocking cloned socket independently of capture, MFT and encode pacing.
  It caches each packet once (4096 cap/300 ms TTL), shares immutable bytes on
  repair, and uses the validated UDP route-probe endpoint. Event forwarding is
  bounded; dropped reports and critical overflow are explicit. IDR requests are
  coalesced outside the event queue and applied by the codec owner just before
  the next encode. Codec calls are never made concurrently or preempted.
- Teacher capture wait is capped at 2 ms; encode tick pacing remains unchanged.
  Capture/encode is not repeatedly polled to service NACK. Aggregate Mbps now
  includes retransmission bytes; repair cache/service/errors and control dispatch
  latency are measured. A blocked MFT call can still delay actual IDR generation.
- Student keeps one latest decoded frame and admits it based on source arrival,
  a 90% frame-period minimum spacing and a strict rolling one-second FPS ceiling.
  No independent fixed presentation phase or saved burst credit is retained.
  H.264 inputs are not arbitrarily dropped. Recovery timers retain the original
  episode start while further broken AUs are discarded.
- Windows H.264 sets CODECAPI_AVLowLatencyMode as VT_BOOL before media-type
  negotiation (generic fallback on rejection), matching the Microsoft contract:
  https://learn.microsoft.com/en-us/windows/win32/medfound/codecapi-avlowlatencymode
  Actual acceptance is logged; selected-driver behavior is not proven by build.
  Keyframes are counted from IDR NALs rather than a clean-point attribute.
- Source timing uses a bounded 128-entry input/output sample-time association,
  including buffering synchronous MFTs. Repeat captures retain one wall time.
  Unmapped/missing timestamps omit source capture metadata instead of attributing
  output to the newest input. RTP timing follows actual encoded sample time.
  New source association/fresh-only age/input-to-send/keyframe-response rows
  distinguish processing from static repeats. Schema is 3; wire protocol stays 5.

### Verification and delivery

- Formatting check and native strict Clippy passed.
- Strict Windows GNU workspace/all-target Clippy check passed.
- Workspace suite: 107 passed, zero failed, one ignored native integration test;
  one unchanged VideoToolbox hardware test filtered because this host previously
  fails VTCompressionSessionCreate with -12908. This is not a full hardware pass.
- Regression coverage includes fragmented IDR repair without partial AU output,
  incomplete-frame hold, early complete-frame release, timely/expired repair,
  startup RTP rollover/reordering, overflow safety, latest-frame/no-burst/rolling
  FPS constraints, delayed MFT association/stable repeats/evicted metadata, and
  actual localhost UDP retransmission while the media control queue is full.
- Native executable `--role teacher --loopback --diagnose` passed, confirming
  schema 3 and effective nack_repair_ms=25. No live native capture was attempted.
- Windows x64 GNU release build succeeded. Existing host rust-objcopy/libLLVM
  stripping warning is nonfatal. `/Users/mvmanh/Downloads/Classroom.exe` is
  byte-identical to the build (cmp exit 0), SHA-256:
  `6d5cb60086a0ff41982f047b0ffb2c67ecbd0e4a0503b2e1dd23a89531410559`.
- Evidence: `checkpoints/diagnostics/2026-09-28-recovery-scheduling-*`.
  No Git repository exists, so no checkpoint commit was created.

Exact next step: use this same EXE on Teacher and Student in the next available
normal Windows LAN session. Existing automatic logs will show jitter residence,
decoded wait, repaired/expired gaps, IDR response/recovery and valid source age;
no extra telemetry-only run is required. Do not claim measured FPS/latency gain,
hardware property acceptance, 1080p acceptance or 40-PC capacity before runtime
evidence. Phase 1 performance acceptance remains open.

## Real wired Windows telemetry release validation — 2026-09-28

Status: owner supplied protocol-5/schema-2 logs, build ID 1790560709, both real
Windows 10 Education PCs, physical 1 Gbps Ethernet/MTU 1500, 1600x900 stream.
Teacher i7-12700/16 GiB uses software H264 MFT after NVIDIA activation 0x8000FFFF;
DXGI adapter is Microsoft Basic Render Driver, encoder low-latency property is
rejected E_NOTIMPL. Student i7-4770/8 GiB/Intel HD4600 uses D3D11 NV12 shader.
These logs validate runtime execution, not 1080p or 40-PC acceptance.

### Measurements and comparison

- Student final: 38.1526 s, 990 decoded, 957 presented; 25.948 decode FPS and
  25.083 render FPS averaged over startup, idle and recovery. Median 1-second
  intervals: decode 29.840 FPS, render 27.982 FPS. Previous supplied run averaged
  26.174/24.318 FPS with 1017/945 frames over 38.86 s. Render average improves
  about 3%; workload and idle/recovery differences prevent a controlled FPS claim.
- Whole-call decode median/p95 = 8.537/26.3798 ms, versus previous 17.25/45.47 ms:
  about 50.5%/42.0% lower. CPU conversion is effectively absent (max 0.022 ms).
  Output copy mean 1.155 ms; MF output polls mean 4.358 ms across 1980 polls for
  990 frames, including no-output polls (not per-frame decode latency).
- GPU upload/draw mean 2.453 ms, max 4.671; 957 completed query samples match
  presented frames. Query skipped/invalid both zero, pending at most one. CPU
  render median/p95 = 0.8945/1.594 ms. GPU/render is not the dominant current cost.
- Wire ingress: 21,265 packets / 23,745,879 bytes; zero malformed/wrong-peer/socket
  errors, zero ingress age/capacity drops, zero jitter overflow/stale-backlog
  drops. Ingress wait mean 4.751 ms, max 55.759; high-water 246 packets. Mean
  jitter residence 21.934 ms; decoded presentation-slot wait mean 11.571 ms,
  median interval p95 20.321 ms, max 32.887. Fixed jitter and presentation phase
  are measurable low-latency improvement candidates.
- Control RTT median interval sample 0.653 ms, range 0.447–25.779. Clock uncertainty
  median 1.327 ms (max 13.890 during outlier); Teacher offset about -2.868 s is
  corrected, never subtract the PCs' raw wall times. Source-send-to-Present
  estimate mean 45.696 ms, median interval p95 56.735 ms, max 93.678. Source-send-
  to-UDP-marker estimate mean 1.320 ms is comparable to clock uncertainty.
  No claim of optical glass-to-glass latency or precise sub-ms link delay.
- Teacher active interval encode median 30.054 FPS; encode interval p95 median
  3.872 ms, no send errors/WouldBlock/abandoned packets in captured intervals,
  bitrate stays 6 Mbps. Last available interval totals: 20 requested NACK
  sequences, 20 retransmissions, zero cache misses. Teacher log ends without
  completion summary; these are last observed totals, not guaranteed final totals.

### Remaining issues supported by data/code

1. Four expired sequence gaps and four later arrivals (legacy missing_packets=8)
   trigger four depacketizer errors, zero codec decode errors. Five measured
   IDR waits include startup: 501.701 ms at join, then 607.367, 605.230, 170.956,
   771.802 ms. Student sends 7 IDR requests/21 NACK sequences in 21 messages and
   discards 72 pre-IDR AUs. These recovery episodes materially lower FPS despite
   the very small sequence-gap fraction. Do not count late retransmissions as
   additional independent physical losses. Full NACK events are rate-limited;
   the logs cannot time every retransmission or prove physical link loss.
2. NACK repair window is 20 ms after gap detection while Teacher control handling
   shares a loop with capture wait (up to20 ms), pacing/encode and tick sleep.
   This budget mismatch can let retransmissions arrive after jitter expiry even
   on a low-RTT LAN. Prioritize repair dispatch and make the short repair budget
   consistent with sender scheduling before dropping picture reference state;
   confirm IDR requests take effect promptly rather than assume they do.
3. Teacher capture-to-encode age averages 163.785 ms across all submissions, with
   many repeated/static source frames, so it cannot represent live input latency.
   capture_unix_us is recomputed from two clocks each encode, permitting tiny
   differences for the same capture; exact equality undercounts repeats. Also,
   synchronous MFT does not itself prove zero internal buffering, and Teacher
   currently ignores actual encoded output timestamp when assigning RTP timing.
   Cache capture identity/time once and associate MF input/output timestamps
   for reliable freshness/keyframe accounting. This is a concrete measurement
   correction alongside scheduling improvements, not a need for another lab
   visit solely to add telemetry. Raw capture-to-present all-run mean150.755 ms
   and max925.255 ms must not be called measured glass-to-glass lag.
4. Low frame intervals include adaptive idle/typing and recovery, not just decode
   slowness. Capture>=20FPS subset (9 Teacher intervals) has capture-to-encode
   mean24.605 ms. Clock-aligned nearest-interval subset Student capture-to-Present
   mean81.972 ms over246 estimates is exploratory: coarse interval association,
   repeats and MFT association caveats prevent treating it as ground truth.

Preview closes about35 ms after Student logs control EOF; reconnect attempted,
then timed out against stopped Teacher. A successful reconnect is not demonstrated.

Exact next step: a combined scheduling/recovery correction: reduce fixed LAN
jitter/presentation phase wait without increasing reference corruption, service
NACK/IDR promptly outside capture wait, and fix source/output timing association.
Retain working NV12 shader, GPU query coverage, bounded ingest and clean image
baseline. Existing logs are enough to choose this work; no instrumentation-only
lab rerun is requested. Detailed aggregate stage statistics are retained in
`checkpoints/diagnostics/2026-09-28-windows-lan-analysis.json`. No code/build changes
in this analysis turn. Owner-specified two source log files are deleted afterward.

## Comprehensive Windows telemetry and latency improvements — 2026-09-28

Status: implementation complete; native/GNU static checks, automated regressions,
executable logging smoke and portable Windows release verified. Real Windows
GPU/MF runtime and performance remain to be validated on the owner's external PCs.

Owner requested a substantial combined release with default diagnostics, so a lab
visit is not needed just to add each missing metric. The complete coverage audit,
measurement semantics and remaining physical measurement boundaries are documented
in `docs/12_TELEMETRY.md`. Historical compact-field filtering is superseded.

### Changes

- Windows MF decoder returns validated padded NV12, preserving sample timestamps.
  D3D11 uploads Y/UV planes and converts with the existing BT.709 limited-range
  coefficients. UV storage offset respects padded luma height; point chroma
  sampling preserves 2x2 mapping. GDI retains CPU BGRA fallback; optional
  `CLASSROOM_CPU_CONVERSION=1` selects the comparison path. Hardware codecs remain
  optional. This removes scalar full-frame conversion and BGRA-sized upload on
  the GPU path, without claiming hardware MF decode or zero-copy.
- Separate scoped UDP ingress worker timestamps/receives while decode is busy.
  Ingress is bounded to 2048 packets/80 ms; jitter to 2048 packets. Stale compressed
  backlog beyond jitter budget + 120 ms is discarded with IDR recovery; the single
  decoded waiting slot remains latest-frame-wins. Fatal socket errors propagate.
- TCP_NODELAY on both peers, a combined length-prefix/body write, bounded 256-message
  Student output and two-second connect timeout. RTP timestamp and sequence-gap
  ordering handle wrap. Control-error exits retain partial Student summary rows.
- Default file captures every INFO-level telemetry field (no metric allowlist),
  approximately 1 Hz distributions and lifecycle events. Manifest, OS/CPU/RAM,
  GPU, OS-reported NIC speed/MTU, actual socket buffers, per-Student full reports,
  every MF/copy/conversion/render/network stage, queue ages/drops, separate packet
  classifications, NACK/cache/recovery and first-image counters are covered.
  Stage samples are bounded to 1024; count/mean/max remain interval-wide and
  percentile coverage is explicitly logged. Session latency windows cap at 4096.
- Optional four-set nonblocking D3D11 GPU timestamp queries record upload/draw
  duration and pending/skipped/disjoint coverage. Unsupported queries retain CPU
  measurements without blocking or making graphics support mandatory.
- Protocol 5 carries source timing in a 20-byte RTP extension (datagram <=1220 bytes)
  and four control wall timestamps. Offset/uncertainty-adjusted source-to-receive,
  AU and Present estimates are recorded when timestamps map. Capture time is
  unavailable for unassociated asynchronous output; unmapped output is counted.
  These estimates are not optical glass-to-glass acceptance.
- Dedicated bounded asynchronous file writer (1024 records, 64 KiB buffer,
  250 ms flush, bounded shutdown grace), explicit overflow reports, 64 MiB rotation
  and five archives. New runs back up the previous file before creating log.txt.
  Detailed telemetry goes to file rather than flooding CMD. README/protocol docs
  updated. Existing benchmark report still uses its complete console log format.

### Verification

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --offline -- -D warnings`: passed.
- GNU Windows strict Clippy/all-target check via rustup rustc/clippy-driver: passed.
- `cargo test --workspace --offline -- --skip bitrate_change_keeps_a_decodable_keyframe_boundary`:
  95 passed, one hardware integration test ignored and one filtered. Newly covered:
  padded NV12/fallback storage, invalid/overflow layouts, bounded sampling totals,
  RTP timing/MTU/long-session wrap, jitter sequence wrap/late/duplicate/reset,
  real UDP ingress without a running consumer, real TCP RTT/clock/EOF and log
  preservation/archive retention. Test output: `/private/tmp/classroom-telemetry-tests.txt`.
- Full-suite attempt still fails the unchanged native VideoToolbox runtime test
  with VTCompressionSessionCreate -12908 in this environment; the filtered result
  does not claim hardware runtime verification.
- Native Classroom `--role teacher --loopback --diagnose --log-file ...` passed;
  full configuration/schema persisted, clean shutdown flushed, and a second run
  preserved the preceding log in archive. No screen capture/UI benchmark claim.
- GNU Windows release built offline with x86_64-w64-mingw32-gcc. Nonfatal existing
  rust-objcopy/libLLVM stripping warning remains in the build-script toolchain;
  release link succeeds. PE32+ x86-64 verified. Delivered to owner Downloads and
  `cmp` verified identical.
- Release SHA-256: `f50222b21668370eb49bd4b30d7b79aa87d45cb30cbab212644995b401f993ce`.
  Paths: `target/x86_64-pc-windows-gnu/release/Classroom.exe` and
  `/Users/mvmanh/Downloads/Classroom.exe`.

### Remaining boundaries and exact next step

Use this same protocol-5 release on both Windows endpoints. A normal run now
collects the diagnostics by default; copy Teacher/Student role folders, including
archives if the session rotated. Analyze this combined optimization's software
stage/freshness evidence before proposing another change. No extra diagnostic
flag or instrumentation-only lab visit is planned. Optical latency/readability,
actual GPU/driver behavior, physical NIC/AP loss attribution and 1/5/10/20/40-PC
wired scaling still require real observations per performance acceptance. Archive
retention is finite and abrupt termination can lose the buffered tail. No Git
metadata exists; no checkpoint commit was created.

# Implementation progress

## Physical Windows LAN retest after GPU/render decoupling — 2026-09-28

Status: owner-supplied compact logs analyzed; first actual Windows runtime evidence for the new D3D11 Student renderer and paced sender. Owner perception: FPS slightly better, delay still noticeable. Source paths were /Volumes/Transcend/Downloads/studentlog.txt and teacherlog.txt; removed after analysis under the owner's standing instruction.

- Teacher: Intel Core i7-12700, 20 logical CPUs, 12 physical cores, about 16 GiB RAM, IP 10.0.79.200. Student: Intel Core i7-4770, 8 logical CPUs, 4 physical cores, about 8 GiB RAM, IP 10.0.79.117. Both select Ethernet 4. Actual stream remains 1600x900 (decoder NV12 storage 1600x912), so this is still not a 1080p or forty-Student acceptance run.
- Student confirms renderer_backend=d3d11 on Intel HD Graphics 4600, driver_frame_latency=1, fullscreen/topmost and receiver_render_decoupled=true. No GPU fallback occurs in supplied logs. Render p95 falls from the previous 15–53 ms sampled range to 1.65–3.12 ms per sampled interval; session render p50=1.213 ms and p95=2.416 ms. Thus this older integrated GPU successfully runs the GPU presentation path.
- In five sampled intervals with decode >=28 FPS, render is 26.95–28.06 FPS (median 27.23). Across all eight sampled intervals, render median=27.00 FPS. Session total: 1017 decoded, 945 rendered in 38.859 seconds, equivalent to 26.17 decode and 24.32 render FPS including startup/recovery/variable source cadence. Previous log had 4–13 render FPS. Workload and run duration differ, so these are observed comparisons rather than a controlled benchmark gain.
- Teacher active sampled encode roughly 29–30 FPS with p95 3.35–4.53 ms, except source-idle/transition intervals (~18 and ~23 FPS; target_fps=5). Bitrate target stays 6 Mbps throughout sampled logs. Hardware encoder activation still fails 0x8000FFFF and software low-latency BOOL is rejected E_NOTIMPL; functional software fallback remains. Lower actual send Mbps during a static scene is not proof of a target-bitrate cut.
- Teacher actual UDP send buffer=262144. All supplied intervals show send_errors=0, send_would_block=0, send_retry_recovered=0 and send_abandoned_packets=0. Pacing wait per sampled one-second interval ranges 0.002–13.231 ms (median 2.516 ms; this is cumulative interval wait, not per-frame latency). This contrasts with 63 cumulative WSAEWOULDBLOCK errors before the fix. Because no WouldBlock occurred here, retry recovery was not exercised by this runtime. Teacher file has no final summary; no complete-session zero-error guarantee beyond supplied records is claimed.
- Student final: received_packets=19095, late/missing sequence counter=10, NACK messages=5, keyframe requests=5, decode_errors=0, slot replacements=66 by the last sampled interval. Most of the session's missing counter remains at 4, with a few later small gaps and IDR recoveries. The former run had 3137 detected late/missing sequences and 37 NACKs over a different/longer duration. Counter is not a unique unrecovered packet-loss percentage.
- Remaining measured cost: total decode-call p50=17.252 ms and p95=45.473 ms; sampled interval p95=32.23–41.00 ms. A 30 FPS frame interval is 33.33 ms, so slow decode calls can occasionally exceed that budget. Existing decode-call measurement includes MF processing, sample allocation/copy, output layout queries and scalar CPU NV12-to-BGRA conversion, as verified in codec-windows-mf/windows_impl.rs and color.rs. It does not isolate H.264 codec time from color conversion. Student process CPU is typically 74–88% of one logical CPU (roughly 9–11% of total eight-thread capacity); it is not 80–90% total CPU saturation.
- Decoded frame waiting p95 is 7.29–28.75 ms (about 17–29 ms in motion samples). Slot pending count is always 0 or 1. Render gaps p95 remain ~44–62 ms and session p95=62.657 ms, showing occasional uneven cadence despite higher average FPS. Receive and decode still share one worker; costly decode/conversion can delay socket reads. Packet interarrival max up to 95 ms does not by itself establish physical LAN jitter or actual OS socket backlog.
- TCP control RTT is 84–106 ms in supplied samples (median 96.84 ms), including application scheduling. There is no glass-to-glass measurement or clock synchronization evidence; do not infer end-to-end video delay from RTT, add stage p95 values, or subtract two PCs' wall-clock timestamps.
- Student summary, preview close and reconnect events show normal teardown/reconnect activation; summary-to-preview-close timestamps differ by 26 ms, not a measured TCP EOF-to-close delay. A successful reconnect is not shown.

Exact next step: instrument the Windows decode call into MF submit/output, sample copy/layout and NV12-to-BGRA conversion timings, and add bounded receive/decode backlog/relative freshness evidence. Use measured stage costs to move NV12 color conversion to GPU or optimize CPU conversion with fallback; evaluate hardware MF decode afterward as needed. Consider reducing decoded-slot scheduling wait while retaining the 30 FPS ceiling and latest-frame semantics. Keep the successful D3D11/paced-send baseline and text/color correctness. Repeat the weaker-room typed text/scroll case and compare with i7-14th-generation / integrated-GPU room; separately obtain an external glass-to-glass measurement. No production code or new build in this analysis turn.


## Windows Student smoothness optimization — 2026-09-28

Status: implemented, tests/static checks/release builds verified; Windows GPU/runtime performance acceptance remains pending on the owner's external PCs. Owner confirmed the school has both weaker rooms and i7-14th-generation / Intel integrated GPU rooms. No discrete GPU or hardware codec requirement was added.

### Changes

- Student receive/jitter/depacketize/decode now owns a scoped worker, including codec initialization/teardown. Native window/presentation stays on its owner thread. A mutex protects only the single waiting decoded frame and bounded render telemetry; no lock is held through decode or render. H.264 access units still decode in dependency order; only obsolete decoded output is replaced. UI can close on TCP disconnect before joining an in-flight decoder. Headless diagnostics retain zero rendered frames.
- Windows renderer now prefers D3D11 hardware (feature levels 11.0/10.0): one reusable BGRA upload texture, HLSL full-screen triangle with linear aspect-fit scaling, black bars, DXGI two-buffer flip-discard swap chain and maximum driver frame latency one. The adapter name and backend are logged. Initialization/presentation errors fall back to the existing filtered-color GDI path. Decoder conversion remains CPU BGRA: no claim of hardware decode, zero-copy, or GPU encoder selection. The shader compiler dependency is the Windows system d3dcompiler_47.dll.
- Balanced timeBeginPeriod(1)/timeEndPeriod(1) around the Windows window lifetime avoids 1 ms render polling becoming coarse timer ticks. Fullscreen/topmost bounds are checked once per second; SetWindowPos is invoked only when enforcement is actually needed. Drop consumes the old window's WM_QUIT so reconnect cannot immediately close its replacement.
- Teacher requests a bounded 256 KiB UDP send buffer and logs the actual size. Aggregate RTP batches are paced with configurable network.send_pacing_mbps (400 default, 0 disables). The total pacing span per AU is capped at min(8 ms, frame_interval/3). Transient WouldBlock gets up to 2 ms active retry time shared by all recipients, inside a 10 ms AU deadline. No retry queue exists. Unrecovered/permanent errors still shed that endpoint's remaining AU packets and explicitly count them. Capture/encode/packetize-once and fan-out are retained.
- Render-only slowdown no longer lowers bitrate; packet loss, slow decode and encoder/CPU pressure still trigger existing adaptation. Low render cadence remains diagnostic.
- Compact log.txt allowlists now retain CPU model/logical and physical cores/RAM, GPU adapter/backend, actual send buffer, interval decode p95, decoded-frame waiting p95, pending render slot depth, receiver_render_decoupled, send_would_block, send_retry_recovered, send_abandoned_packets, and pacing/retry wait. Global render samples and interval wait/call samples are capped at 4096. Existing five-second compact sampling remains. Sender retry errors and actual abandoned datagrams are distinct counters.
- Updated README, media/protocol specs and decision log. A small platform-neutral launcher path conversion was corrected to satisfy the current host Clippy version.

### Verification

- cargo fmt --all -- --check: passed.
- Focused app/config/renderer tests: passed, including renderer/decoder concurrency and recovery-slot clearing, transient/permanent/exhausted UDP retries, IDR/fan-out pacing caps and disable switch, and compact log retention. Current app suite: 47 passed, one opt-in native test ignored.
- cargo test --workspace --offline --quiet: attempted inside and outside sandbox. The unchanged VideoToolbox bitrate-boundary test fails at initial encoder creation with VTCompressionSessionCreate -12908, before any changed policy/pipeline code is called. All remaining workspace tests passed with only that named native-codec test filtered: 87 passed, one opt-in test ignored, one filtered. Do not present the full unfiltered suite as passing.
- Strict cargo clippy --workspace --all-targets on macOS: passed. Strict Windows workspace/all-target checks with the rustup clippy-driver and -Dwarnings: passed for x86_64-pc-windows-gnu. Homebrew rustc lacks the Windows sysroot, so cross-build explicitly used rustup's installed rustc. Native MSVC alias could not download its uncached dependency due DNS; GNU target is cached and builds the portable Windows executable successfully.
- macOS release build and executable --diagnose --loopback: passed. Attempted visible seven-second local Teacher/Student smoke with separate /private/tmp logs: Teacher capture initialization did not finish within 20 seconds; both spawned processes were cleaned up. No media/render runtime success is claimed from this host attempt. VideoToolbox creation failure and ScreenCaptureKit initialization need a usable local capture/codec environment to validate the shared macOS path.
- Windows GNU release build: passed, with the previously recorded nonfatal host rust-objcopy/libLLVM stripping warning. Output is PE32+ x86-64, no non-system DLL imports detected. Copied to /Users/mvmanh/Downloads/Classroom.exe and cmp/hash verified. SHA-256: b18d53fd778406a9e8651c6b1decbab9eaf98b5e1126ccd4e46b861cf655479c.
- No Git metadata exists; no checkpoint commit was invented.

### Remaining risks and exact next step

Run the new executable on BOTH Teacher and Student first in the weaker physical Windows room, then the i7/Intel room. Use the same source dimensions and 60-second typing/scrolling/terminal workload and retain both log.txt files. Confirm D3D11 selection or the explicit GDI fallback, colors/text/aspect ratio, close/reconnect behavior, decode versus render FPS, frame-wait p95, and reduction of send backpressure/abandoned packets. Packet pacing caps and software fallback are build/test verified, not a measured 1080p30 or 40-student guarantee. Integrated GPU rendering is independent of the still-unverified hardware MFT encoder path. Repeat 1920x1080 and wired 1/5/10/20/40 acceptance after the one-Student runtime path is healthy; glass-to-glass remains an external measurement.


## First physical Windows-to-Windows school LAN run — 2026-09-28

Status: analyzed owner-supplied Teacher and Student logs from two real Windows 11 PCs on the school's LAN. This is first runtime evidence for several Windows-native media paths outside Parallels. Logs were removed from `/Volumes/Transcend/Downloads/` after analysis at the owner's standing request.

- Connection/route: Teacher `10.0.79.200` on `Ethernet 4`, Student `10.0.79.117`; TCP registration and UDP route probe completed. Teacher initially waited roughly 99 seconds for the Student, then the active interval lasted roughly 60 seconds. The logs show zero Windows Teacher send errors during startup but then 63 cumulative RTP send errors (`WSAEWOULDBLOCK`, OS error 10035) during the active session.
- Actual capture dimensions were **1600×900**, and Student registration/decoder used 1600×900. Therefore this run confirms sub-1080 source support and dimension negotiation, but is not a 1920×1080 runtime test despite the intended display target. Teacher's failed hardware H.264 MFT activation (`0x8000FFFF`) correctly fell back to the software `H264 Encoder MFT`; its low-latency property was rejected. The software path kept encode near 30 FPS with p95 encode latency about 3.2–4.7 ms and process CPU mostly 20–31% (one interval about 41%). This validates the fallback's functional/runtime behavior at 1600×900; it does not validate hardware encoding or 1080p30.
- Teacher feedback/adaptation activated: bitrate went 6 Mbps → 4.8 → 3.84 → 3.072 → 2.458 → 1.966 → 1.573 → 1.5 Mbps. It remained at target FPS 30. Receiver reports show high packet loss and render-slow state. Encoded stream bitrate changes were applied, but the compact log does not establish that they improved image quality or smoothness.
- Student Windows path: Microsoft H.264 Decoder MFT initialized, fullscreen/topmost preview opened, and decode errors stayed at zero. It recovered from keyframe waits, sent 37 NACK messages, and triggered keyframe requests. Teacher successfully resent packets on several NACKs (logged resend counts 1, 11, 13, 11, 6, 27, etc.), so the retransmission path demonstrably ran. Student eventually reported 3,137 cumulative late/missing RTP sequence numbers; this metric is the receiver's detected sequence-gap count and should not be interpreted as 3,137 unique packets that remained unrecovered at session end.
- Student decode ranged 0–31 FPS (median sampled interval about 16 FPS); zero-decode intervals corresponded with waiting for a keyframe. When decoding recovered, it sometimes reached 23–31 FPS. Render was about 4–13 FPS. Crucially, Windows render-call p95 ranged about 15–53 ms, showing local presentation work is now directly implicated, in addition to network loss. Render-gap p95 was about 98–125 ms in intervals with frames. Latest-frame replacement reached 579 cumulative frames, confirming stale decoded images are being shed. Control RTT sampled about 100–278 ms; it is control RTT, not video glass-to-glass latency.
- New sender-side evidence: `WSAEWOULDBLOCK` occurs during Teacher `send_to` and cumulative `send_errors` rises to 63. The sender currently sends each access unit's RTP packets in a tight loop on a non-blocking UDP socket and abandons the remainder of that access unit for the affected Student after a send error. This is a concrete application-side burst/socket-backpressure contributor to sequence gaps, alongside any LAN path loss. The logs cannot assign every missing sequence to that sender error.
- Confirmed on real Windows: DXGI capture and actual dimension announcement; software Media Foundation H.264 encode fallback after hardware activation failure; Media Foundation decode; UDP route probe; fullscreen/topmost Student surface and close-on-disconnect; receiver reports; NACK/retransmission and IDR request paths; adaptive bitrate; frame-slot replacement. Not confirmed: hardware MFT operation, 1920×1080 sustained runtime, UI launcher IP persistence, actual NIC link speed/duplex, external glass-to-glass latency, or multiple-Student scaling.
- Exact next step: (1) profile and improve the Windows GDI Student renderer, since call p95 15–53 ms is a measured local bottleneck; (2) pace Teacher RTP datagrams or add a bounded latest-access-unit sender stage with explicit overload accounting so `WSAEWOULDBLOCK` does not discard most of a frame's remaining packets; (3) repeat a controlled 1920×1080 typed-text/scroll run. Keep adaptation from reducing bitrate solely for render cadence unless it produces measured improvement; packet loss remains a distinct valid feedback signal. Then proceed to wired scale testing.

## Parallels Windows-to-Windows Wi-Fi bridge retest — 2026-09-27

Status: analyzed the owner's new two-Windows-11 run. Both guests were configured for 1920×1080 and Parallels bridged their guest Ethernet adapters over the two MacBooks' Wi-Fi. The adapter label `Ethernet` in these logs is therefore consistent with the guest-side bridge and does not contradict the owner's Wi-Fi setup. The two source logs were deleted after analysis as requested.

- Both endpoints registered and completed media route setup with zero Teacher send errors. DXGI captured exactly 1920×1080; Media Foundation decoded the same dimensions and produced no decode errors. Student fullscreen/topmost preview opened and closed 12 ms after Teacher control disconnect.
- Windows Teacher again selected software `H264 Encoder MFT`; the low-latency property was rejected. Capture was about 18–25 FPS after startup, encode about 26–30 FPS, and process CPU telemetry was about 112–127% of one logical CPU during sampled intervals. Target bitrate stepped down 6 → 4.8 → 3.84 → 3.072 → 2.458 Mbps after the single Student repeatedly reported slow visible rendering.
- Student decoded about 25.8–28.8 FPS after the first damaged access unit, but rendered only 4.6–6.3 FPS. Renderer-call p95 was about 3.6–3.7 ms, while render-gap p95 was 276–325 ms and the latest-frame slot replaced 304 cumulative frames. Student counted 18 missing RTP packets and 7 NACKs, with zero decode errors. Teacher logs show one NACK retransmitted 1 packet and another with no packet available. Packet/access-unit max gaps reached roughly 400–446 ms; control RTT ranged from 104 to 892 ms and is not video glass-to-glass latency.
- This confirms full-HD Windows capture, Windows software-MFT fallback, Teacher bitrate/FPS policy activation, visible Student telemetry, and disconnect-close behavior through a Parallels Wi-Fi bridge. Repeated bitrate cuts still did not raise render FPS; the sender-side response is therefore not yet shown to solve this bottleneck and may worsen text quality. Windows Student decoder hardware acceleration cannot be concluded from the generic MFT name. One Student and a Wi-Fi bridge do not validate wired LAN or multi-Student quorum/scaling.
- Exact next step: investigate why visible rendering remains near 5 FPS with ~3.6 ms renderer calls while decoded frames are being replaced; separately prioritize a usable hardware encoder on the Windows Teacher. Keep bitrate from stepping down solely on render cadence until evidence shows that it improves the displayed result. Repeat with a wired link when available.

## First cross-platform and Windows-to-Windows Wi-Fi trials — 2026-09-27

Status: analyzed owner-provided logs from `/Users/mvmanh/Desktop/logs-case1` (Mac Teacher → Windows 11 Student) and `/Users/mvmanh/Downloads/logs-case2` (Windows 11 Teacher → Windows 11 Student). The owner reports the image displayed in both cases. Source logs were deleted after analysis as requested; the findings below preserve the evidence summary.

- Case 1: VideoToolbox hardware encoder held about 28–29 encode FPS after join, with zero Teacher send errors. Windows Student registered, completed the UDP route probe, opened the forced fullscreen/topmost presentation, and decoded without errors. However, Student render cadence was mostly 3.5–6.9 FPS despite decode intervals near 21–32 FPS. Render-call p95 was about 7.8–9.9 ms; render-gap p95 reached 150–408 ms and RTP/access-unit max gaps reached about 445 ms. The run counted 5 missing packets and 4 NACKs; Teacher retransmitted 1 packet on one request and 0 on others. Control RTT samples were about 240–511 ms. Teacher's render-slow adaptation stepped target bitrate from 6 Mbps to 1.57 Mbps, but Student render FPS stayed low, so this path did not demonstrate a cadence improvement and may reduce text quality.
- Case 2: Windows DXGI scaled a 3024×1674 source to 1920×1062, preserving the expected aspect ratio and staying within the 1920×1080 maximum. Teacher selected the software `H264 Encoder MFT`; Media Foundation rejected its low-latency property and continued with MFT defaults. Teacher CPU often exceeded 100%, capture varied about 4–14 FPS, and encode varied about 9–22 FPS. Adaptive bitrate fell from 6 Mbps to the 1.5 Mbps minimum, then adaptive FPS fell to 20 and 15. Student decoded about 8–23 FPS and rendered about 3–8 FPS; renderer-call p95 stayed around 7.8–9.8 ms, while render-gap p95 reached 109–460 ms. Four RTP packets were missing with four NACKs; no decode errors or Teacher send errors were logged. Access-unit/packet gaps and control RTT again reached several hundred milliseconds.
- Confirmed on Windows Student: Media Foundation decoder initialized; dimensions were accepted; visible fullscreen/topmost preview opened; route registration/probe and telemetry worked; NACK/keyframe recovery was exercised; preview closed 13 ms after Teacher disconnect in case 1 and 27 ms after in case 2; the Student then retried registration as designed. Windows capture sizing, software MFT fallback, CPU/FPS pressure adaptation, and protocol-v4 render telemetry now have real Windows runtime evidence.
- Adapter naming needs verification: both Windows logs report `friendly_name=Ethernet` even though these trials were described as Wi-Fi. This may be the Windows interface's configured name, so the logs alone do not prove which physical medium carried traffic. Control RTT is control-plane round-trip time, not video glass-to-glass latency. One Student per case does not validate multi-student quorums or wired-LAN scaling.
- Interpretation: low FPS is not explained by packet loss alone. Case 2 has clear Teacher-side software-encoder/CPU pressure; both cases also show long packet delivery or processing gaps. Since lower bitrate did not improve render cadence in either test and reached the configured floor, the render-only bitrate response should be reevaluated before further tuning. The perceived text degradation may reflect that bitrate reduction as well as Wi-Fi quality.
- Exact next step: verify the selected Windows network adapter against the actual Wi-Fi NIC; then, when practical, repeat on wired LAN. For a Wi-Fi comparison before that, keep the same display/content and collect one visible Student run with a Teacher using a hardware Media Foundation encoder if available. Do not count these one-receiver Wi-Fi trials as Phase 1 wired 1/5/10/20/40 acceptance.

## Native launch screen and shared Windows logs — 2026-09-27

Status: implemented and cross-build verified. Normal startup without `--role` now opens a native role selector with Teacher/Student radio choices, an editable Teacher IP field, and Start. `192.168.100.115` is the initial Teacher address; the most recently started address is saved in per-user settings (`%APPDATA%\Classroom\teacher-ip.txt` on Windows). The GUI Teacher flow binds to the automatically selected active adapter. CLI role and diagnostic flags still bypass the UI.

- Default compact logs are again created beside the executable. Startup deletes the prior file for that same role/port, then recreates it, while preserving the other role's log for simultaneous Teacher/Student runs. This lets a Windows executable launched from `\\Mac\Home\Downloads` write under the shared `logs` folder. The Windows loopback batch launcher keeps staging its EXE locally but now directs its distinct Teacher/Student logs to that same Mac-shared folder.
- Changed the embedded and launcher default Teacher address to `192.168.100.115`. Updated README and the batch launcher.
- Verification: formatting; focused app/config suites (42 app tests passed, 1 ignored; 4 config tests passed); strict x86_64 Windows-target Clippy/check; macOS release build; Windows x64 GNU release cross-build. Windows build retains the known non-fatal host `rust-objcopy`/`libLLVM.dylib` warning. Copied `Classroom.exe` to `/Users/mvmanh/Downloads/Classroom.exe`; SHA-256 is `31e6c794d56f22d313e0a81b050bebcb48da7c8f165bf15aa1cc526b1206a8b8` and matches the build output. No Git repository is present.
- Remaining validation: open the native start screen and exercise radio selection, IP editing/persistence, and Start on Windows. Run Student from the shared Downloads path and confirm its compact log appears at `\\Mac\Home\Downloads\logs\student-47810\log.txt`; the batch launcher uses `logs\student.log` and `logs\teacher.log`.
- Exact next step: run the new `/Users/mvmanh/Downloads/Classroom.exe` on Windows once interactively and confirm the start screen plus shared Student log location.

## Windows compact log location — 2026-09-27

Status: superseded by the owner instruction below. The first Windows-specific LocalAppData adjustment was reversed so logs can be collected from the Mac Downloads share; see “Native launch screen and shared Windows logs” above for the current behavior.

- Added path-selection tests for Windows LocalAppData and the executable-directory fallback; focused compact-log tests passed (3/3). `cargo fmt --all -- --check` and strict x86_64 Windows-target Clippy/check passed.
- Rebuilt x86_64 Windows release and copied `/Users/mvmanh/Downloads/Classroom.exe`; source and copy SHA-256 are both `74e50c6528c0ec65c56096893b1d4b186a50134b42e1516fce62616c1c305720`. The known non-fatal host `rust-objcopy` warning remains. Native macOS Clippy was attempted but the macOS Swift package build was blocked by the sandbox denying SwiftPM cache access outside the workspace; this Windows-only path was linted for its target.
- This historical test location is no longer the active default.

## Adaptive bitrate for slow visible Student presentation — 2026-09-27

Status: implemented and verified by focused tests, strict Clippy, and macOS/Windows release builds. Receiver reports now distinguish headless Students from visible presentations and include p95 native renderer-call latency. Teacher counts a visible Student as render-slow only when render cadence is below the configured receiver FPS threshold while renderer-call p95 is at or below the configured latency ceiling. Three consecutive degraded intervals trigger the existing bounded bitrate step-down/cooldown; healthy reports retain gradual recovery.

- The owner's latest Wi-Fi sample had Teacher encode at 29–30 FPS, Student decode near 30 FPS but render at 13–14 FPS, no packet loss, and renderer-call p95 around 1.3 ms. RTP interarrival and access-unit p95 were typically 0.06 ms and 33–48 ms respectively, but each sampled second included maximum gaps around 270–423 ms. This supports trying a bitrate reduction when visible render cadence is low and native render calls remain fast.
- Bumped control protocol to version 4 and added `rendering_enabled` plus `render_latency_p95_us` to `ReceiverReport`. Added configurable `adaptive_receiver_render_latency_max_ms` (default 12 ms). Headless Students and high-latency renderers do not trigger this new adaptation; packet loss and slow decode handling are unchanged.
- Added quality-policy tests proving sustained low render cadence with fast renderer calls lowers bitrate after the existing three-interval streak, while low cadence with slow renderer calls does not. Added protocol round-trip coverage and config validation.
- Verification: `cargo fmt --all -- --check`; focused tests for `classroom-app`, `classroom-config`, `protocol`, and `control-transport` passed (38 app tests, 1 ignored; 4 config; 1 protocol; 3 transport); strict native and x86_64 Windows-target Clippy passed. macOS release build and x86_64 Windows GNU release cross-build passed. The Windows build has the known non-fatal host `rust-objcopy`/`libLLVM.dylib` strip warning. Copied `Classroom.exe` to `/Users/mvmanh/Downloads/Classroom.exe`; source and copy SHA-256 both `26e957b55deb43b4b3b9350250a9ecb178bdd60a5372c66f3dcf10d6ecc544bc`. Windows executable is PE32+ x86-64. No Git repository is present, so no checkpoint commit was made.
- Next: replace Teacher and Student with matching protocol-v4 binaries and repeat the Wi-Fi test. Look for `receiver_render_slow_students` followed by a bitrate change, then compare Student `render_fps`, `render_gap_interval_p95_ms`, receive bitrate, and text readability. Both endpoints must use protocol v4. This is a Wi-Fi improvement experiment, not wired-LAN or Windows scale acceptance.

## Student latest-frame drain policy — 2026-09-27

Status: revised after the first Wi-Fi retest did not improve presentation cadence. Student now drains every currently-ready RTP packet and decodes access units in sequence before one render check. The single decoded-frame slot replaces stale frames throughout that drain, so the render tick uses the newest decoded frame available after catching up instead of showing an older pending image mid-drain. The owner's retest shows active render cadence remains 14–15 FPS, so added receiver-arrival timing metrics to isolate Wi-Fi batching from render scheduling.

- The prior between-packet render change was not retained: its retest showed 10–18 render FPS and 224–354 ms interval render-gap p95. In the drain-before-present retest, Teacher encoded about 29–30 FPS; Student decoded about 30 FPS and rendered about 14–15 FPS, with render-gap p95 about 283–285 ms. Packet loss was zero, RTT was 24–25 ms after startup, and renderer-call p95 was 1.8–2.25 ms. Student also measured a 246 ms last-packet age, consistent with a delivery gap/burst, though this compact sample cannot locate it in the Wi-Fi path.
- Compact Student samples include interval and cumulative render-slot replacements, jitter-buffer delay, and now packet interarrival and completed access-unit gap p95/max. These determine whether the next bottleneck is packet delivery cadence or the Student processing/render loop. H.264 packets remain decoded in sequence; only decoded presentation frames are replaced. Arbitrarily dropping compressed P-frames could corrupt dependent pictures until an IDR.
- Verification after diagnostics: format check; 37 classroom-app tests passed (1 Screen Recording test ignored); strict app Clippy; macOS release build; Windows x64 release cross-build. Windows cross-build retained the known nonfatal `rust-objcopy`/`libLLVM.dylib` strip warning. Copied `/Users/mvmanh/Downloads/Classroom.exe` and verified SHA-256 `5a0f20d3fcb98b5a2776f2a9e63d0d034429cd8fa01f1cf0db5559f2d3843400`. No Git repository is present.
- Next: rerun the same Wi-Fi workload with this diagnostic build. Compare access-unit gap p95/max to render-gap p95 and render-call latency; large access-unit gaps would confirm packet delivery bursts, while regular access units with sparse renders would isolate Student scheduling.

## Student preview disconnect close timing — 2026-09-27

Status: implemented and build-verified. On control disconnect, Student now explicitly drops the native renderer immediately before entering the reconnect delay. Compact logs record the start and completion of the close operation so the next remote run can distinguish renderer teardown time from the network detection time.

- The owner's latest two-Mac logs show Teacher media completion at `1790491840344` and Student control disconnect/reconnect warning at `1790491840539` (about 195 ms later). The existing compact log did not include the info-level close event, so it could not establish when the preview window was actually destroyed.
- Added compact event records `teacher control connection closed; closing Student preview` and `student preview closed after Teacher disconnect`, both with `client_id`; renderer destruction now happens explicitly between them.
- Verification: formatting, focused compact-log classification test, strict app Clippy, and macOS release build passed. Windows x64 release cross-build passed with the known nonfatal `rust-objcopy`/`libLLVM.dylib` warning; copied to `/Users/mvmanh/Downloads/Classroom.exe` and byte-verified (SHA-256 `2efbae9f529d0c282026fb0f88e09113241919558d353c9216dd3775013c452c`). Remote Windows UI timing needs a new owner-run test; this host cannot control that PC.
- Next: rerun on Windows and compare the two new `unix_ms` events. If they are close together but visible disappearance still takes 2–3 seconds, the delay is in OS/window presentation rather than Student's disconnect path.

## macOS Student BGRA render-copy optimization — 2026-09-27

Status: implemented and locally benchmarked. The macOS Student renderer now copies each decoded BGRA frame into minifb's persistent backing buffer with a contiguous memory copy instead of repacking every pixel channel on the media thread. Windows rendering and the macOS native window/GPU presentation APIs are unchanged.

- This removes the per-pixel byte extraction and channel-shift loop while retaining the backing allocation minifb uses for its asynchronous Metal texture upload. The copy runs only after decoded dimensions and byte length are validated.
- Verification: formatting, 19 focused adaptive-policy tests, macOS renderer crate test/build, and strict Clippy for `renderer-macos` and `classroom-app` passed. A real eight-second local native-media benchmark with the optimized macOS release rendered all 231 decoded frames at 28.78 FPS; measured renderer-call latency p50/p95 was 1.24/1.47 ms, with three schedule overruns. This validates the local 1080p path but does not reproduce two-Mac Wi-Fi scheduling.
- macOS release SHA-256: `021f925b79de960ced53cb50b56375dd7b26971a775f38168c1a3c39cc80c65d`. Windows x64 release was cross-built and copied byte-identically to `/Users/mvmanh/Downloads/Classroom.exe`; SHA-256 `ea6c605aac6fb348351f720bd5392c6ebd3e2ced82fb92aada2044016a62c51f` is unchanged because the code change is macOS-only. No Git repository is present, so no checkpoint commit was made.
- Next: copy this macOS `target/release/Classroom` to the Student MacBook Air, rerun the visible Wi-Fi test, and compare `decode_fps`, `render_fps`, `render_gap_interval_p95_ms`, and `render_latency_interval_p95_ms`. The new measurement decides whether remaining gaps come from renderer submission cost or packet/decode scheduling.

## Close Student preview when Teacher session ends — 2026-09-27

Status: implemented. Student observes EOF/error from the Teacher control TCP reader and exits its current media loop, which drops the native renderer and closes the forced fullscreen preview. The existing outer reconnect behavior remains for a Teacher that becomes reachable again.

- The control transport now exposes the Student control connection state. Both reader and writer tasks mark it closed when their socket/channel ends, and subsequent control sends fail immediately.
- Student checks the state in its media loop and logs `teacher control connection closed; ending Student preview` before returning. This handles Teacher process exit after `--share-seconds`; no media inactivity timeout was added, so a static/shared screen does not look like a disconnected session.
- Added a control transport regression test that verifies successful RTT and then server-side socket closure becomes observable by Student. Verification: formatting, all 3 focused control transport tests, 19 focused adaptive quality tests, strict Clippy for app/config/control crates on macOS and Windows x64, and macOS/Windows release builds passed. The Windows cross-build emitted the known nonfatal host `rust-objcopy`/`libLLVM.dylib` warning.
- macOS release SHA-256: `cc09f5cb86c020f99aac61cc8dcbd34985362d020b5e6cd00b1fb80fcc134d44`. Windows PE32+ x64 release SHA-256: `ea6c605aac6fb348351f720bd5392c6ebd3e2ced82fb92aada2044016a62c51f`; verified byte-identical at `/Users/mvmanh/Downloads/Classroom.exe`. No Git repository is present, so no checkpoint commit was made.
- Next: test a finite Teacher share with an unbounded Student and confirm Student logs the control disconnect and closes preview without waiting for a user action. The remote Windows machine is not available to control from this host.

## Wi-Fi adaptation and Student render timing — 2026-09-27

Status: revised based on the owner's Wi-Fi retest. Teacher now uses packet loss or low decode cadence for bitrate adaptation; render cadence alone is diagnostic because the supplied run showed decode near 29 FPS while rendering was 15–19 FPS. New Student compact samples expose renderer-call p95 so the display path can be measured directly.

- Owner's retest logs are in `target/release/logs/{teacher-47800,student-47810}/log.txt`. Teacher preserved about 29 FPS and changed target bitrate 6.0 → 4.8 → 3.84 Mbps after `receiver_slow_students=1`. Student decoded 28–30 FPS after startup, rendered 15–19 FPS, and reported zero missing packets, NACKs, and decode errors. TCP RTT was about 24–28 ms. Thus the bitrate changes occurred, but the run does not show a rendering FPS improvement; it points to a display-side cadence gap.
- Teacher now derives each reporting Student's interval decode cadence from cumulative `ReceiverReport` counters. A Student is degraded when packet loss exceeds the configured threshold or decode cadence falls below 75% of the active Teacher target FPS. Render count remains available in the report but cannot independently trigger a bitrate reduction. This avoids spending bitrate against a renderer-only bottleneck.
- After three consecutive degraded eligible intervals, Teacher asks the single live encoder to reduce bitrate by 20% (default 6.0 to 4.8 Mbps), with a five-second cooldown and a 1.5 Mbps floor. Ten healthy eligible intervals raise bitrate by 10% toward the 6 Mbps target. This preserves the current maximum 1920x1080 output and the active 30 FPS target where the encoder can apply bitrate changes.
- Sustained encode-latency or CPU pressure now follows the same bitrate-first route. It only reduces the active FPS ceiling to 24/20/15 when runtime bitrate control is unavailable or bitrate is already at the configured floor.
- Student compact intervals now include `render_latency_interval_p95_ms`, the time spent inside the native renderer call; the existing `render_gap_interval_p95_ms` reports spacing between submitted frames. Both metrics are logged at most once per five seconds in compact logs, with one-second sampling in full logs.
- Configuration adds `adaptive_receiver_min_fps_percent`, bitrate-down/up step percentages, and the shorter cooldown. Teacher logs and benchmark summaries retain `receiver_slow_students` for packet/decode degradation.
- Verification: formatting; 19 focused quality-policy tests including render-only-vs-decode-slow cases; 5 benchmark-report tests; 4 config tests; strict Clippy for macOS and Windows x64; macOS and Windows release builds all passed. The Windows cross-build emits the known nonfatal host `rust-objcopy`/`libLLVM.dylib` stripping warning.
- macOS release SHA-256: `b02c99ba43d1c6ab3bbdb7f1a53b12d97be94aee8427ae7be0a084e786c7355e`. Windows PE32+ x64 release SHA-256: `812f0c3073ac648a39f1b80cab434f40cc8fb559d2e34e3805384f7f9a829e64`; verified byte-identical at `/Users/mvmanh/Downloads/Classroom.exe`. No Git repository is present, so no checkpoint commit was made.
- Next: install matching new Mac binaries, repeat the visible Wi-Fi test, and compare `decode_fps`, `render_fps`, `render_gap_interval_p95_ms`, and `render_latency_interval_p95_ms`; use long renderer call times versus large render gaps to choose whether pixel conversion/window submission or frame scheduling needs optimization. Windows test can use the copied `/Users/mvmanh/Downloads/Classroom.exe` when a machine is available.

## Forced full-screen Student presentation — 2026-09-27

Status: implemented and locally runtime-verified on macOS; Windows x64 compile/release verification passed, while real Windows UI validation remains pending until a machine is available. This is an explicit owner-requested Phase 1 UI behavior after the USB-C media path reached acceptable smoothness and preliminary latency.

- Added a dedicated Student presentation constructor to both native renderers. The M2 codec preview remains a normal resizable window and `--headless` remains windowless.
- macOS Student now creates a borderless, non-resizable, always-on-top surface at the CoreGraphics main-display bounds, positions it at the display origin, and keeps the topmost level asserted while the session is active. The current development display reports 1512×982 points.
- Windows Student now uses a borderless `WS_POPUP` at the primary-display dimensions with `WS_EX_TOPMOST`/`HWND_TOPMOST`. The renderer reapplies its full-display topmost placement while active and its presentation window procedure ignores the ordinary `WM_CLOSE`; renderer teardown still destroys the window when the Teacher/control session ends. Re-registering the native class after a Student reconnect no longer causes an early renderer error.
- Student compact logs retain `student forced presentation window ready fullscreen=true topmost=true`. The video continues to aspect-fit with black bars, so full-screen enforcement does not stretch a Teacher source with a different ratio.
- Verification: formatting, the focused compact-log event test, the Windows aspect-fit test, and focused strict Clippy passed for the application and native renderer crates on macOS and Windows x64. macOS and Windows x64 release builds passed; the Windows build emitted the known nonfatal host `rust-objcopy`/`libLLVM.dylib` warning. A real six-second visible local macOS Teacher/Student run passed with the forced-window event, hardware VideoToolbox decoding, 609 received packets, 148 decoded/144 rendered frames, and zero missing packets, NACKs, or decode errors. A second visible session also exited cleanly after its configured duration.
- macOS release SHA-256: `2a75213b6dd14c493cb0026d098aff84e87303580c5f1ac0da9d123063b33f8f`. Windows PE32+ x64 release SHA-256: `0b513844ca9ec86f68a0c1a4fea9b95656103b84177ae77a9b70d7c2767f06e3`; verified byte-identical at `/Users/mvmanh/Downloads/Classroom.exe`. No Git repository is present, so no checkpoint commit was made.
- Limitation: this is application-level presentation enforcement. A user with operating-system privileges can still terminate the process or enter protected system UI. Strong process/input protection requires the future privileged agent/service and is outside this increment.
- Next: run the same version on the two Macs to visually confirm full-display coverage, aspect fit, topmost behavior against ordinary apps, and automatic release when Teacher stops. Perform the equivalent Windows UI validation when a real machine becomes available.

## USB-C/Thunderbolt bidirectional UDP rendezvous — 2026-09-27

Status: implemented and runtime-validated on two separate Macs connected directly over USB-C/Thunderbolt networking. The owner confirmed that the visible Student preview appeared, was smooth, and was satisfactory from the UI perspective.

- Preserved the owner-supplied compact logs under `checkpoints/evidence/macos-2026-09-27-usbc-udp-stall/`. Teacher correctly targeted `192.168.10.102:47810`, kept `send_errors=0`, and sent 10,589 RTP packets. Student kept a healthy TCP control connection with sampled RTT of 0.701–3.326 ms, but received about 1.16 Mbps only in its first sampled second and exactly 0 Mbps afterward. It decoded/rendered zero frames and never initialized VideoToolbox. Thus the missing preview was a media-ingress failure before decode/render, not a renderer failure. Kernel-accepted Teacher sends do not prove delivery. The evidence is consistent with a one-way UDP route or stateful host-firewall path, but these compact logs cannot prove which external component dropped the packets.
- Protocol version 3 adds `teacher_media_udp_port` to `StartPresentation`. Teacher binds the configured UDP feedback port as its RTP source. Student sends a 12-byte route probe from the same bound socket that receives RTP; Teacher uses the datagram's observed IPv4 source address and port as the final endpoint and only then requests the joining IDR and begins fan-out to that Student. This establishes the actual bidirectional interface/port tuple instead of relying only on the TCP peer address. Student repeats the probe at most once per second only while waiting for its first keyframe; Teacher handles at most 64 probes per media-loop iteration and does not rebuild the encoder for an unchanged ready endpoint.
- Compact Student samples now retain cumulative `received_packets`, `udp_route_probes_sent`, `waiting_for_keyframe`, and `last_packet_age_ms`. Teacher retains the route-established event. This makes a future no-preview run distinguish no UDP, partial access-unit delivery, and decoder/render failure without per-frame logging.
- Updated the checked-in protocol reference and network/README documentation. Version 2 and version 3 executables intentionally reject each other; both Macs must use this build.
- Verification: format check passed; focused protocol/control tests passed (4 total); two focused UDP probe tests passed, including observation of the Student socket's real source endpoint; strict Clippy passed for the application on macOS and Windows x64. A nine-second real local ScreenCaptureKit/VideoToolbox/RTP smoke run outside the command sandbox passed: Teacher observed the route probe at `127.0.0.1:47910`, Student initialized the hardware decoder, received 893 packets, decoded 107 frames in five seconds, sent only one route probe, and reported zero missing packets, NACKs, or decode errors. The first sandboxed attempt could not start ScreenCaptureKit because its completion callback did not fire within 30 seconds; that environment-only attempt supplied no media evidence. macOS and Windows x64 release builds passed. The Windows build emitted the known nonfatal host `rust-objcopy`/`libLLVM.dylib` warning.
- Two-Mac acceptance evidence is preserved under `checkpoints/evidence/macos-2026-09-27-usbc-rendezvous-success/`. Teacher established the probed endpoint as `192.168.10.102:47810`, used hardware VideoToolbox encoding, completed 20.005 seconds with 590 captures and 568 encodes, and reported zero send errors or retransmissions. Student used one UDP route probe, initialized the hardware VideoToolbox decoder about 157 ms after sending it, reached 5,543 received packets in the last compact sample, and reported zero missing packets, NACKs, and decode errors. Active sampled decode/render cadence was approximately 23.87–29.62/24.77–28.99 FPS, render-gap p95 was 36.60–60.35 ms, and TCP control RTT was 1.045–3.418 ms. The reconnect warnings after Teacher completion are expected. These process metrics and the owner's visual result validate the route/media/UI correction, but they are not an external glass-to-glass latency measurement.
- Owner-reported preliminary glass-to-glass observations on this direct-link build were 140 ms (`7.42 s` Teacher versus `7.28 s` Student) and 120 ms (`7.91 s` versus `7.79 s`), with a simple mean of 130 ms. Both samples fall within the preferred 100–150 ms target. With only two observations and no recorded camera frame-rate/measurement uncertainty, this is encouraging spot evidence rather than a p95 acceptance result.
- macOS release SHA-256: `0106ffe36191f23ba9d0216913bffc62c5fe522b07b7df1cae640302c48e8819`. Windows PE32+ x64 release SHA-256: `dcc175ff13bd284da5149d4b5ceac443c9b4fc9b5b4d021e81102da4e8afa1a1`; verified byte-identical at `/Users/mvmanh/Downloads/Classroom.exe`. No Git repository is present, so no checkpoint commit was made.
- Next: preserve this media path and continue Phase 1 acceptance with external glass-to-glass timing and the physical Windows wired 1/5/10/20/40 Student ladder. No further USB-C routing change is indicated by this run.

## USB-C/Thunderbolt media-route correction — 2026-09-27

Status: fixed a reproducible multi-homed Mac failure where TCP control used the USB-C/Thunderbolt bridge but Student advertised its default Wi-Fi address for UDP media. Teacher no longer terminates the whole presentation when one UDP endpoint is temporarily unreachable. Retest on the two connected Macs remains required.

- Failure evidence preserved at `checkpoints/evidence/macos-2026-09-27-usbc-route-failure/teacher.log`. Teacher bound `192.168.10.100`; the TCP peer was `192.168.10.102`, but the old Student advertised Wi-Fi `192.168.100.254:47810`. Teacher used that unrelated endpoint and the first UDP send ended the session with `No route to host (os error 65)`.
- Student media registration now obtains the actual IPv4 source address from the connected TCP socket and advertises that route-selected address. Teacher defensively treats the TCP peer IPv4 address as authoritative for the UDP endpoint and warns if the advertised address differs. For the observed case, the endpoint becomes `192.168.10.102:47810`.
- RTP fan-out now handles a per-Student `send_to` error without returning from the Teacher loop. It skips the remaining packets for that Student's current access unit, retries on the next access unit, rate-limits the warning to five seconds, and exposes cumulative `send_errors` in interval/final logs and benchmark reports. Other Students continue receiving the single packetized stream.
- Verification: focused protocol/control tests passed (4 total), including the exact mismatched Wi-Fi-advertisement/Thunderbolt-peer case and live TCP RTT test. Formatting and strict workspace Clippy passed on macOS and Windows x64. macOS and Windows x64 release builds passed; Windows emitted the known nonfatal host `rust-objcopy`/`libLLVM.dylib` warning. A full workspace test escalation was automatically rejected because this turn did not explicitly request broad testing; no workaround was used.
- macOS release SHA-256: `67a38202363277f602c6b8c75876879a7e438c753d79c18a1842440c87076c2d`. Windows PE32+ x64 release SHA-256: `72420b2143d05ca5a69b0f1e9e280ae6a0e78a2d7cbf7ece5dc6acdd577d022b`; verified matching `/Users/mvmanh/Downloads/Classroom.exe`. No Git repository is present, so no checkpoint commit was made.
- Next: replace `Classroom` on the MacBook Air with this new protocol-compatible Mac release, rerun Teacher at `192.168.10.100` and Student against that address, then confirm Teacher logs endpoint `192.168.10.102:47810`, nonzero RTT samples and `send_errors=0`.

## Two-Mac Wi-Fi RTT/hardware observation — 2026-09-27

Status: owner ran visible Teacher on the local Mac and Student on a MacBook Air M4 using protocol version 2. Both live VideoToolbox sessions reported hardware acceleration and the stream had no sampled RTP loss or decode error. Startup/keyframe recovery and Student presentation cadence still need investigation; this Wi-Fi run does not satisfy the wired-LAN gate.

- Preserved compact logs under `checkpoints/evidence/macos-2026-09-27-two-machine-wifi/`. Teacher used `192.168.100.115`; Student used `192.168.100.254`, both selected Wi-Fi.
- Teacher completed 20.012 s at 1920×1080 with 590 captured and 570 encoded frames, capture p95 4.291 ms, encode p95 26.076 ms, 19 freshness replacements, no bitrate change and no retransmission. Active sampled encode cadence was 28.63–28.99 FPS after Student joined; VideoToolbox encoder reported hardware.
- Student VideoToolbox decoder reported hardware. Active sampled decode cadence was 26.85–27.90 FPS, but render cadence was only 14.95–17.90 FPS. The first active render-gap p95 was 213.45 ms and the next was 48.60 ms. Compact logs omit per-second detail and render-slot replacement counts until final completion, so they cannot distinguish callback batching from preview scheduling with enough confidence.
- Student sampled zero missing packets, NACKs and decode errors. Teacher nonetheless received a recovery keyframe request about 0.9 s after registration; decoder initialization appeared about 2.0 s after registration, with several intentional VideoToolbox encoder rebuild events. This indicates startup/late-join delay without evidence of RTP loss.
- The latest RTT samples were 234.269 ms at startup, then 22.230 and 40.449 ms during active media. The later 137.267 ms sample occurred after Teacher media completion and should not characterize steady-state media. These are TCP control RTT samples over Wi-Fi, not video latency.
- No code changed from this observation. Next code task: add bounded receiver timing that records access-unit completion-to-decoder-output timing and interval render-slot replacements in the compact log, then use full console logs on the next two-Mac run. This will localize the Student render-rate gap before changing pacing or VideoToolbox behavior.

## Local Mac RTT and hardware-backend observation — 2026-09-27

Status: owner ran the version 2 release as visible Teacher and Student on the same Mac. RTT collection and live VideoToolbox backend queries worked. The active media interval was healthy; this remains localhost evidence rather than two-Mac LAN acceptance.

- Preserved compact logs under `checkpoints/evidence/macos-2026-09-27-rtt-hardware/`.
- VideoToolbox reported `encoder_backend=hardware` and `decoder_backend=hardware` from the active sessions at 1920×1080. Repeated encoder-selection events coincide with intentional session rebuilds for initial/join IDRs.
- Student RTT advanced from 1 to 20 valid samples; sampled latest values were 0.177–0.263 ms on `127.0.0.1`. This validates the telemetry path, not classroom-network RTT.
- During Teacher transmission, sampled encode FPS was 26.80 initially then 28.90–30.01; encode p95 was 17.69–22.56 ms. Teacher completed 20.034 s with 591 captures, 575 encodes, capture p95 3.407 ms, encode p95 19.595 ms, 15 freshness replacements, no bitrate change and no retransmission.
- Student active samples decoded about 28.99–32.98 FPS and rendered 25.98–28.99 FPS, with zero missing packets, NACKs and decode errors; render-gap p95 was 39.64–42.24 ms. The later zero-FPS interval occurred after Teacher completed and sent no media. Student then entered the expected reconnect loop because its own run remained active.
- No code changed from this observation. Next: run the same version 2 release on two separate M1 Macs over LAN, preserve full console logs in addition to compact logs, and measure external glass-to-glass latency during static text, typing, scrolling and terminal output.

## Control RTT and actual macOS codec backend telemetry — 2026-09-27

Status: Phase 1's missing per-Student control round-trip metric is implemented. VideoToolbox encoder/decoder selection logs now query the active session for actual hardware use and report `unknown` if the property is unavailable. Real two-Mac and physical Windows LAN acceptance remain open.

- Protocol version 2 adds nonce-matched `ControlPing`/`ControlPong`. Each Student keeps at most one pending probe, normally sends one per second, times out a stalled probe after three seconds, and includes the latest RTT in microseconds plus its sample count in `ReceiverReport`. Teacher logs the per-Student values. Student full and compact logs report milliseconds/count; the native CSV and legacy PowerShell CSV add per-Student RTT columns. A zero sample count means no valid measurement. Mixed version 1/2 executables fail the registration version check.
- The RTT covers the TCP control exchange and scheduling at each endpoint. It does not measure RTP transit, decode/render, or glass-to-glass latency. No per-frame timestamp or unbounded telemetry queue was added.
- macOS VideoToolbox queries `UsingHardwareAcceleratedVideoEncoder` and `UsingHardwareAcceleratedVideoDecoder` on the respective live sessions; missing/wrong-type properties yield `unknown`. Decoder selection is logged when SPS/PPS starts its lazy session. This corrects the earlier unconditional encoder `hardware` label and capability-based decoder label. Video frame copies and the codec policy are unchanged.
- Verification: format check passed; full workspace tests passed (71 passed, one opt-in native test ignored); a real localhost TCP registration/ping/pong test passed; native report tests passed; strict workspace Clippy passed for macOS and Windows x64; macOS and Windows x64 release builds passed. The first macOS test attempt was blocked by Swift cache sandbox permissions and passed when rerun with the required cache access. The Windows build emitted the known nonfatal host `rust-objcopy`/`libLLVM.dylib` warning.
- macOS release: `target/release/Classroom`, SHA-256 `886aa3cd099443a75056d40aea08162f78cfb6bca1f3294a5fd290586f31a2d1`. Windows PE32+ x64 release: `target/rustup-windows/x86_64-pc-windows-gnu/release/Classroom.exe`, SHA-256 `09e108f88416b80a2c2468dabb9e891418c54c0d197f4a80bda051700627c858`; verified matching copy at `/Users/mvmanh/Downloads/Classroom.exe`. The updated PowerShell CSV script was not run because PowerShell is unavailable on this Mac. No Git repository is present, so no checkpoint commit was made.
- Next: run a visible two-Mac LAN session using matching version 2 binaries; collect both full console logs plus the compact logs and verify codec backend selection, nonzero RTT samples, typing/scrolling continuity, and external glass-to-glass timing. Then continue the Windows M2/M3/M4 checks and 1/5/10/20/40 wired ladder.

## Owner Windows retest — 20 ms jitter default — 2026-09-25

Status: owner reports stable video and only slight, acceptable delay. Logs show the new 20 ms Student jitter setting, clean sampled packet/decode counters, and a single recovered NACK. The Teacher's software encoder still adapted down to 15 FPS in some intervals; this is the clearest remaining measured cadence constraint.

- Preserved the supplied compact logs under `checkpoints/evidence/windows-2026-09-25-jitter-20ms/`.
- This was a localhost loopback run (`127.0.0.1`) at 1920×1080 with Microsoft's software `H264 Encoder MFT` and `Microsoft H264 Video Decoder MFT`; it is not evidence of physical LAN performance.
- Across eight sampled intervals (simple, unweighted mean): Teacher capture 21.37 FPS, encode 20.88 FPS, target 21.5 FPS (range 15–30), encode-latency p95 mean 17.76 ms (range 14.78–29.81 ms), process CPU mean 78.42% of one logical core (samples 55.07–116.92%).
- Across eight sampled intervals: Student decode 20.54 FPS (14.81–24.95), render 18.76 FPS (12.87–23.95), render-gap p95 mean 67.25 ms (52.18–93.19), process CPU mean 72.63% of one logical core (53.03–93.75%).
- All sampled Student `missing_packets` and `decode_errors` were zero. One NACK was sent and Teacher logged retransmission of one packet. The compact logs expose interval samples rather than complete final counters. Render-gap p95 is presentation spacing, not end-to-end delay; no external glass-to-glass measurement was made.
- No code or tests changed in this retest. Next: compare on wired physical Windows machines if available; if subjective delay remains important, capture full per-second logs while testing fast typing and obtain external glass-to-glass timing before tuning encoder adaptation further.

## Student freshness tuning — 2026-09-25

Status: lowered the default Student RTP jitter-buffer packet hold from 40 ms to 20 ms to reduce receiver-side waiting during fast screen changes. The setting remains configurable so a noisier LAN can trade freshness for packet reordering/loss tolerance. Updated the network protocol note to match.

- Inspection showed each in-order RTP packet is withheld until it has aged by the configured target delay, so 40 ms was a real per-packet freshness cost, not merely a gap-recovery ceiling.
- A missing-packet gap also waits one target delay after NACK scheduling; lowering the default shortens recovery tolerance as well. The previous Windows sample had one recovered NACK, so this needs observation on the owner's Windows VM.
- This is a conservative wired-LAN default change, not proof of reduced glass-to-glass delay. The formal external-timer measurement remains required; keep raising the setting if packet reordering causes visible artifacts.
- No tests were added or run in this change. After the owner requested a clean Windows-only rebuild, `cargo clean` removed 39,002 files (6.1 GiB) and the Windows x64 release cross-build passed. PE32+ x86-64 executable and `/Users/mvmanh/Downloads/Classroom.exe` have SHA-256 `0dff2ed9e4da03cd715072fb882b6111682d61af86ebe29bfd221b3e9156a1d1`. Build emitted the existing nonfatal host `rust-objcopy`/`libLLVM.dylib` warning. Next: have the owner compare fast typing while watching NACK/missing-packet counts and visual continuity.

## Owner Windows retest — image artifacts resolved in observed run — 2026-09-25

Status: owner-confirmed visual improvement on the actual Windows VM on another Mac. All previously described purple/blue lines and noise around text are gone according to the owner. Fast Notepad typing still has perceptible delay; overall motion is acceptable to the owner with no major issue. Latency/scale acceptance is not closed.

### Evidence and interpretation

- Owner supplied `/Users/mvmanh/Downloads/logs/{teacher-47800,student-47810}/log.txt`. Preserved exact copies under `checkpoints/evidence/windows-2026-09-25-image-quality/{teacher,student}.log`. These are compact sampled logs, not full benchmark logs.
- Teacher/Student used localhost, one receiving endpoint, 1920×1080 source and stream. Encoder: software `H264 Encoder MFT`; decoder: `Microsoft H264 Video Decoder MFT`. The encoder rejected the optional low-latency BOOL property with `E_NOTIMPL` and continued. This warns about a capability, but does not establish an internal buffering delay or prove a root cause.
- Native decoder output layout: visible 1920×1080, storage 1920×1088, 3,133,440 bytes, reported/selected stride 1920, `packed_2d=false`. This confirms the padded luma-height path was used successfully; the packed-2D branch was not exercised. Both the preceding build and this build already handled storage-height padding, so this log alone cannot attribute the visual improvement to the stride change. The renderer filter correction remains consistent with the owner's result.
- Student duration 44.720418042 s: 17,142 received packets, 985 decoded frames (~22.03 FPS), 796 rendered frames (~17.80 FPS), zero final missing packets, zero decode errors and zero keyframe requests. One NACK was sent; Teacher logged one retransmitted packet. This was a recovered gap, not proof that no packet ever arrived late.
- Sampled Teacher encode FPS was ~18.3–24.0; its adaptive FPS target moved among 30/24/20 with a 6 Mbps bitrate target. Sampled encode p95 was ~15–46 ms. CPU samples use percent of one logical core and must not be described as whole-machine saturation. These sparse intervals do not support a full-session Teacher average.
- Student render-gap p95 over the run was 84.25 ms; sampled interval p95 reached 121.89 ms. This measures spacing between presentations, not source-to-screen latency. Configured jitter budget was 40 ms, not a measured constant delay. Decoded minus rendered counts cannot all be labeled lost frames because latest-frame replacement/pacing intentionally discards stale previews.
- Teacher's final EOF warning coincides with Student completion and removal of that endpoint. It is consistent with the receiver being closed at the end, not evidence of a mid-session disconnect. Teacher has no final completion summary in this compact log.

### Next step

Keep the current image-quality correction as the observed working baseline. When latency tuning resumes, measure source-to-screen delay with an external timer/camera and inspect receiver decode/render scheduling alongside software-encoder latency and the configurable jitter budget. Use full per-second logs for timing/quality comparisons and separate physical wired Windows machines for the formal scaling ladder. Do not infer end-to-end latency by adding stage percentiles or by treating render-gap p95 as latency.

This checkpoint is evidence/documentation only; no code, configuration or executable changed, so no rebuild was needed. The owner's rule remains: after each successful Windows rebuild, copy `Classroom.exe` into `/Users/mvmanh/Downloads/`. No Git repository is present.

## Windows colored artifacts and text rendering — 2026-09-25

Status: code corrections and build verification complete; visual improvement on the owner's Windows machine is not yet verified. The owner reports the preceding resolution/launcher build runs successfully with acceptable subjective smoothness/delay, but scattered blue/purple lines and noisy, sometimes unreadable Terminal text. No numerical performance acceptance follows from that report.

### Findings and changes

- Windows preview used a common GDI device context without setting its stretch mode. The documented default is `BLACKONWHITE`, which combines color values with Boolean AND during reduction; this is inappropriate for the color desktop, especially when reduced to the initial 960×540 window. Set `HALFTONE` and the required brush origin on every draw (a common DC loses settings on release), with error handling. This is a plausible contributor, not a confirmed explanation for every reported artifact. Sources: [DC defaults](https://learn.microsoft.com/en-us/windows/win32/gdi/display-device-context-defaults), [SetStretchBltMode](https://learn.microsoft.com/en-us/windows/win32/api/wingdi/nf-wingdi-setstretchbltmode).
- Decoder previously preferred `MF_MT_DEFAULT_STRIDE` even after copying into a packed buffer. Added an explicit `IMF2DBuffer::ContiguousCopyTo` path; its packed result uses storage width as pitch. For ordinary CPU buffers, select pitch only when it matches actual byte length, with packed size checked first; reject inconsistent layouts rather than interpreting padding as image content. Keep visible-area cropping and padded luma-height handling from the preceding patch. Source: [IMF2DBuffer layout contract](https://learn.microsoft.com/en-us/windows/win32/api/mfobjects/nn-mfobjects-imf2dbuffer).
- Added regression cases for packed bytes with a larger advertised surface pitch, genuinely padded CPU rows, missing stride metadata and invalid lengths. Compact logs now retain the once-per-decoder output-layout event with byte count, reported/selected stride, packed-2D flag, storage and visible dimensions. This will help interpret the next owner-supplied logs.
- Changed `crates/renderer-windows/src/windows_impl.rs`, `crates/codec-windows-mf/src/{windows_impl,color}.rs`, and `apps/classroom/src/compact_log.rs`. Bitrate, frame rate and macOS media behavior are unchanged.

### Verification and delivery

- Full workspace tests: 70 passed, one opt-in capture test ignored. Focused codec/renderer/application tests: 40 passed, one ignored. Formatting and strict workspace Clippy passed on macOS and Windows x64.
- Windows x64 release cross-build passed (the existing nonfatal host rust-objcopy/libLLVM warning persists). No Windows executable or visual test was performed for this change.
- Copied the newly built `Classroom.exe` to `/Users/mvmanh/Downloads/Classroom.exe` and verified byte-for-byte equality with `target/rustup-windows/x86_64-pc-windows-gnu/release/Classroom.exe`. SHA-256: `c33ca3c86b5ae86283285267e5f393d1e981476523d1eb3e6a6554d9a15f215c`.
- Owner instruction for subsequent work: after EVERY successful Windows rebuild, copy the new `Classroom.exe` to `/Users/mvmanh/Downloads/`.
- Owner clarified the test Windows VM is on another Mac. Do not use the local Parallels VM as that test machine. The local VM briefly started during investigation was shut down normally, restoring its initial stopped state. Guest command attempts could not connect; Computer Use access to Parallels was not approved. No current target logs were obtained; the owner will provide them. The Downloads Windows log was from 2026-09-23 and was not treated as evidence for the latest report.
- No Git repository is present; no commit created.

### Exact next step and risks

- Owner reruns the new executable using the existing BAT on the actual Windows test machine. Compare Terminal/Notepad/icons at the normal preview size and maximized; inspect both compact logs under `%LOCALAPPDATA%\Classroom\Loopback\logs` after the owner supplies them.
- Filtered GDI scaling can cost more render time; measure on Windows. The test suite validates portable layout selection, not a real MFT's packed-surface behavior. H.264 4:2:0 compression, capture resizing and source font antialiasing may still affect fine text; do not increase bitrate or change codec based only on the current description. The root cause of all purple/blue lines and visual acceptance remain open pending target evidence.

## One-click Windows loopback launcher — 2026-09-25

- Added `scripts/Run-Classroom.bat` (ASCII, CRLF). It reads `\\Mac\Home\Downloads\Classroom.exe`, stages a local copy under `%LOCALAPPDATA%\Classroom\Loopback`, replaces the executable only after copying succeeds, unblocks that file, and launches two CMD windows with explicit Teacher/Student roles and `--loopback`. Student's existing retry handles Teacher startup. CMD `/K` retains errors and console output; normal startup needs no input or administrator elevation.
- Copy/preparation/launch failures pause with a visible explanation. A running previous executable is not killed; close its Teacher/Student instances before replacing it. Existing per-role compact logs live beside the local executable.
- Verified ASCII/CRLF encoding, all GOTO targets, and both role commands against the actual CLI. Windows CMD/PowerShell are unavailable on this Mac, so native double-click execution remains unverified. Rust code and binaries are unchanged; prior Rust checks still apply. No Git repository is present.
- Delivered `/Users/mvmanh/Downloads/Run-Classroom.bat` and the current Windows executable as `/Users/mvmanh/Downloads/Classroom.exe`; both copies matched the workspace artifacts byte-for-byte.
- Next: double-click `Run-Classroom.bat` from Windows and verify the local copy, two CMD windows, Student preview and separate role logs.

## Windows variable display resolution — 2026-09-25

Status: implementation, portable logic tests, macOS regression smoke and Windows x64 cross-build complete. Real Windows capture/codec/render and performance acceptance remain pending.

### Finding and changes

- The old DXGI backend captured the actual display size and only warned about mismatched configuration. The fatal mismatch was later in the Media Foundation encoder: the application initialized its encoder and advertised `StartPresentation` using fixed configured dimensions before inspecting capture. Windows rendering also stretched video to the whole client rectangle, distorting different aspect ratios.
- Added `CaptureSource::output_dimensions()`. Teacher starts capture before announcing the stream and initializing its sole encoder; the local media benchmark uses the same actual dimensions. Student already consumes the announcement. macOS implements the new query for its existing configured capture output; its sizing behavior is unchanged.
- Windows capture fits the entire source within configured bounds capped at 1920×1080, never upscales, and rounds to even pixels for NV12. Examples: 3840×2160 → 1920×1080, 2560×1600 → 1728×1080, 3440×1440 → 1920×804, 1366×768 unchanged. Odd dimensions incur a small ratio rounding error. Cursor composition occurs before bilinear resize. Resize is bypassed at matching dimensions and its duration is included in capture latency. Startup and compact logs retain source and stream dimensions.
- Windows renderer fits video into the current client rectangle, fills only the unused bars in black, and safely skips minimized windows. This avoids clearing the image before every draw. Added tests for 4:3, 16:10, ultrawide and tiny windows.
- Media Foundation decoder now accepts larger storage dimensions only with a matching zero-origin visible display aperture. NV12 conversion places chroma after all stored luma rows, then emits only the announced visible region. This handles row/height padding without mistaking it for a stream-size change; malformed or incompatible layouts remain errors.
- Changed modules: `media-core`, `capture-windows-dxgi` (new `scale.rs`), the capture query in `capture-macos-sck`, application initialization/compact logging, `codec-windows-mf`, and `renderer-windows` (new `fit.rs`). Updated README, default config comments, product/media/acceptance/decision documents and `project_spec.yaml` with the owner's clarification.

### Verification and artifacts

- Full `cargo test --workspace --quiet`: 69 passed, one opt-in real-capture test ignored. The initial restricted run hit the known macOS VideoToolbox `-12903`; the complete suite passed outside the sandbox. After the final integer-interpolation optimization and logging adjustment, the affected capture/renderer/Windows-codec/application tests passed again (46 passed, one ignored).
- Final `cargo fmt --all -- --check` and strict workspace Clippy for macOS and `x86_64-pc-windows-gnu` passed. Windows verification used the rustup toolchain directory first in PATH and `CARGO_TARGET_DIR=target/rustup-windows`; Homebrew Rust alone lacks the Windows sysroot. Final Windows release build passed with `CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc`. The known host build-script `rust-objcopy`/missing `libLLVM.dylib` warning was nonfatal.
- Windows portable executable: `target/rustup-windows/x86_64-pc-windows-gnu/release/Classroom.exe`, verified PE32+ x86-64. SHA-256: `5891782f58587b9dc81fb481193fc64be4a4ca05b7563653b9769783f18e6fdc`.
- macOS release build passed. Executable regression: `target/release/Classroom --benchmark-students 1 --benchmark-seconds 5 --benchmark-output /Users/mvmanh/Development/netsupport-school/target/resolution-check/loopback`. Saved run: `target/resolution-check/loopback/run-1790306598606-25534/`. One encoder, 120 decoded frames, 598 received packets, zero missing packets and decode errors. Student was headless, so this does not verify rendering or Windows resolution behavior.
- Scaler-only optimized CPU harness (`target/resolution-check/scale_bench.rs`) on this Mac: 35 synthetic 3840×2160 → 1920×1080 resizes, p50/p95 4.399/6.177 ms after replacing per-channel floating-point interpolation with cached horizontal coordinates and integer weights. Initial floating-point version was 18.689/19.584 ms. These measurements exclude DXGI, cursor composition, encoding and transport and are not Windows performance evidence.
- No Git metadata is present; no checkpoint commit was created.

### Remaining risks and exact next step

- Copy the new executable to real Windows machines and run `Classroom.exe --local-media-benchmark-seconds 60 --display-index 0` at 1366×768, 1280×800, 2560×1600, ultrawide and 4K where supported. Then run Teacher/Student on different display sizes; inspect actual source/stream dimensions, text, cursor, black bars and resize/minimize/restore behavior. Record hardware/software MFT selection, capture/encode latency and frame rate.
- CPU resizing adds work only when source/output sizes differ; its cost on weaker Windows PCs and driver-specific padded output must be measured. Rotation and driver/display-mode transitions still need native coverage.
- Changing Teacher display mode during a session requires restarting presentation if the fitted output dimensions change. This is now an explicit error rather than feeding frames inconsistent with the announced codec. Automatic stream renegotiation remains a separate recovery task.
- The existing wired Windows 1/5/10/20/40-client and glass-to-glass acceptance gates remain open.

## Compact per-run diagnostic log — 2026-09-25

Status: normal app runs now overwrite a small `log.txt` without removing the existing console telemetry. This is a diagnostic convenience, not a substitute for complete benchmark evidence or Windows LAN acceptance.

### What changed

- Added a separate structured-log layer that writes only startup/network/codec/connection/adaptive-bitrate/recovery/error events, a Teacher or Student metric sample no more often than every five seconds, and final session totals. Sparse samples retain CPU and core media/network health; final totals retain working-set memory. Repeated warnings, NACK events, and keyframe-recovery events of the same type are written no more often than every ten seconds. No per-frame records are written to the compact file; console logging is unchanged.
- Default file paths are next to the executable under `logs/teacher-<control-port>/log.txt` and `logs/student-<media-port>/log.txt`, so simultaneous Teacher/Student runs on one computer do not truncate one another's files. Other operations use their own `logs/<mode>/log.txt`. `--log-file PATH` overrides the destination. A normal startup truncates the previous file at that path, and the console prints the absolute path. A fatal post-initialization error is also recorded.
- Files changed: `apps/classroom/src/compact_log.rs`, `apps/classroom/src/main.rs`, `README.md`, this checkpoint. No media pipeline or control protocol changes.

### Verification

- `cargo fmt --all -- --check`, `cargo test --workspace --quiet` (64 default tests passed; one opt-in real-capture test ignored), and strict macOS/Windows-x64 workspace Clippy passed. A focused test checks event selection and five-second sampling. The macOS test suite ran with VideoToolbox access outside the restricted sandbox.
- macOS and `x86_64-pc-windows-gnu` release builds passed, including a final rebuild after the recovery-event/CPU/memory selection was added. The Windows cross-build again emitted the known nonfatal host `rust-objcopy`/`libLLVM.dylib` warning. Mac executable: `target/release/Classroom`, SHA-256 `b6140386b8899e0fffa55fd4ec7a582ef2b144448ff92bffff2114970465235c`. Windows PE32+ x86-64 executable: `target/rustup-windows/x86_64-pc-windows-gnu/release/Classroom.exe`, SHA-256 `4b3e269d2a147f2e235c7e9ab0e051e70f17f2595b6778c43690b72ae38c1396`.
- Two consecutive `--diagnose --loopback --role teacher --log-file PATH` invocations left a 289-byte file rather than appending to 578 bytes. A real same-host, one-Student, five-second headless benchmark completed and produced normal CSV reports; its compact logs were 1,868 bytes for Teacher and 981 bytes for Student, with sparse metric samples, connection events, and final totals. The Student decoded 128 frames with zero reported missing packets or decode errors. This is a local logging smoke, not visible quality or LAN scale acceptance. No Git repository is present; no commit was made.
- After adding rate-limited NACK/keyframe event selection and sparse CPU/memory fields, the complete default test suite, macOS strict Clippy, Windows-x64 strict Clippy, and both release rebuilds passed. The earlier media smoke predates these final logging-selection-only changes.

### Remaining risks and exact next step

- The default location assumes the portable executable's directory is writable; use `--log-file` for a protected installation path or custom collection folder. Explicitly choosing the same override path in two concurrent processes can still cause collision. The compact file intentionally omits most per-second metrics, so preserve original benchmark logs for `--benchmark-report` and formal acceptance. CLI/config errors before logger initialization cannot be recorded in this file.
- Next: use these compact files to triage a fresh two-Mac M6 run, then collect the required visible, wired Windows 1/5/10/20/40-client performance and external glass-to-glass latency evidence. Do not treat this Mac smoke as Windows acceptance.

## M5/M6 benchmark evidence coverage — 2026-09-25

Status: the offline benchmark report now exposes more of the already logged Phase 1 media and adaptive-quality evidence. This is a reporting/observability milestone, not performance acceptance; real Windows wired-LAN scale and glass-to-glass latency remain unverified.

### What changed

- Extended `--benchmark-report` with weighted full-scale Teacher target FPS/bitrate, receiver-report coverage, high-loss receiver count, and feedback/recovery eligibility; added final capture/encode counts and latency, encoded/send bitrate, final target bitrate, and bitrate-change count. Student rows now include received packets, keyframe requests, pre-keyframe discards, render-slot replacements, render-gap p95, and jitter delay.
- New Student completion logs include the configured jitter delay. The report accepts numeric Boolean eligibility fields from structured logs and rejects incomplete mixed-version interval telemetry or impossible receiver counts. Older logs show `not_recorded` for missing fields rather than fabricated zeroes. `EndToEndLatencyEvidence=not_measured_by_process_logs` explicitly prevents process-log metrics from being mistaken for the required glass-to-glass measurement.
- Files changed: `apps/classroom/src/benchmark_report.rs`, `apps/classroom/src/main.rs`, `README.md`, this checkpoint.

### Verification

- `cargo fmt --all -- --check`, `cargo test --workspace` (63 default tests passed; one opt-in real-capture test ignored), and strict workspace Clippy on macOS and `x86_64-pc-windows-gnu`: passed. The first sandboxed test attempt could not create a macOS VideoToolbox session (`VTCompressionSessionCreate -12903`); the complete suite passed outside that sandbox, with no source change required.
- macOS and Windows x64 release builds passed. The Windows cross-build emitted the known nonfatal host `rust-objcopy`/`libLLVM.dylib` warning; the final PE32+ x86-64 executable was produced. Mac executable: `target/release/Classroom`, SHA-256 `8fb627567b73ba5f4d2e5902479c355880a22307b25bde7f0510cc84ca705044`. Windows executable: `target/rustup-windows/x86_64-pc-windows-gnu/release/Classroom.exe`, SHA-256 `eafef31d046278e9d0482dd50238bfb6102e723a57328b0757bd6ca3348c09e2`.
- Ran the release executable's report command on a temporary copy of the existing five-Student Mac benchmark logs. It generated both CSVs, found eight full-scale intervals totaling 8.281576 s, and displayed absent M6 fields as `not_recorded`. Those old logs were headless and do not prove image quality or real classroom scale. No original benchmark log/report was overwritten. No Git repository is present; no commit was made.

### Remaining risks and exact next step

- The report does not measure glass-to-glass latency, visual artifacts, RTT, actual physical wired-LAN contention, or Windows Media Foundation runtime adaptation. CPU percentages and FPS from this Mac smoke are not Windows acceptance results. `missing_packets` remains a late-or-missing observation, not settled packet loss.
- Next: collect a fresh two-Mac M6 run to populate the new feedback fields and assess visible continuity; when physical Windows lab machines are available, run the required 1/5/10/20/40 wired ladder with non-headless students and an external glass-to-glass timing measurement under `docs/06_PERFORMANCE_ACCEPTANCE.md`. Keep Phase 2 features gated until this evidence exists or the owner explicitly changes the stop condition.

## M6 multi-receiver feedback quorum — 2026-09-25

Status: shared Teacher adaptation policy now evaluates per-registered-Student report deltas and configurable class-wide quorums. Deterministic 1/5/40-receiver cases and the Mac native one-Student bitrate-switch/decode diagnostic pass. Real two-Mac/Windows classroom acceptance remains open.

### What changed

- Replaced one aggregate receiver packet counter with one bounded feedback record per registered Student, removed on disconnect. A report from an unknown/disconnected client is ignored; counter resets establish a new baseline. Each evaluation consumes and clears interval deltas, so stale reporters do not remain evidence of loss or health.
- With defaults, at least 50% of connected Students must report in an interval before loss can influence bitrate. At least 25% of the class, and at least two Students when more than one is connected, must independently exceed the late-or-missing threshold before a shared bitrate reduction can accumulate. Upward recovery requires healthy reports from at least 75% of connected Students. One-Student operation retains its original adaptation behavior. These percentages are exposed and validated in `config/default.toml`.
- Teacher interval logs now include reporting Student count, high-loss Student count, reduction eligibility, and recovery eligibility alongside aggregate loss percent. Updated the M6 README to describe the new policy and caveats.
- Files changed: `apps/classroom/src/quality.rs`, `apps/classroom/src/main.rs`, `crates/classroom-config/src/lib.rs`, `config/default.toml`, `README.md`, this checkpoint.

### Verification

- Deterministic tests cover one Student's existing reduction/recovery, five Students with one versus two noisy reporters, 40 Students with one noisy reporter or only one current reporter versus ten noisy reporters, 40-Student healthy-report coverage for recovery, disconnect/re-registration, and invalid quorum config.
- `cargo fmt --all -- --check`, `cargo test --workspace --quiet` (61 default tests passed; one opt-in native test ignored by default), and strict workspace Clippy for macOS and `x86_64-pc-windows-gnu`: passed. The opt-in six-second Mac Teacher/RTP/Student decode-across-bitrate-switch test also passed after the policy change.
- macOS and Windows x64 release builds passed. Mac executable: `target/release/Classroom`, SHA-256 `95674d8c1a2e7f10177523c2e7a6459532dd8a5455f2401f479d186678887e48`. Windows PE32+ x86-64 executable: `target/rustup-windows/x86_64-pc-windows-gnu/release/Classroom.exe`, SHA-256 `8d356aff47f21c325c35de5255840cfc78874d772fb0bcd7375cdd9cb78482d1`. Cross-build again emitted the known nonfatal host `rust-objcopy` warning. No Git repository is present; no commit was made.

### Remaining risks and exact next step

- The 5/40-client cases are deterministic report sequences, not concurrent live clients or physical LAN measurements. The default 50/25/75% quorums are conservative starting parameters, not calibrated classroom thresholds. `missing_packets` still represents late-or-missing observations, not settled packet loss. The Mac native test covers one receiver; Windows Media Foundation dynamic bitrate is still untested at runtime.
- Next: collect one two-Mac M6 run with real typing/static/motion segments and log the new quorum fields; then complete the real Windows wired 1/5/10/20/40 ladder and latency/visual acceptance required by `docs/06_PERFORMANCE_ACCEPTANCE.md`. If owner testing remains deferred, limit further coding to Phase 1 media/measurement improvements rather than unrelated Phase 2 features.

## M6 receive/decode across bitrate switch and upward recovery — 2026-09-25

Status: combined Mac diagnostic and shared-policy test batch complete. No production behavior changed in this checkpoint; formal two-Mac and Windows wired-LAN acceptance is still open.

### What changed

- Extended the opt-in six-second native diagnostic to bind a real localhost UDP receiver, parse the Teacher's RTP/H.264 packets, depacketize them, and decode with VideoToolbox while the Teacher changes bitrate under injected high-loss reports. It asserts an initial and post-switch IDR, decoded frames overall and after the second IDR, plus exactly one 1.2 → 1.02 Mbps Teacher transition. The test remains ignored by default because it needs Screen Recording permission and real media APIs.
- Added a deterministic shared-policy test that first lowers 6.0 → 5.1 Mbps on sustained loss, then feeds ten healthy report intervals and verifies a single bounded 5.1 → 5.61 Mbps recovery after cooldown. This covers the rising branch without adding a lengthy native run.
- Files changed: tests in `apps/classroom/src/main.rs` and `apps/classroom/src/quality.rs`. The old opt-in test name was replaced by the new decode-across-switch test; the previous checkpoint records the test as it existed at that time.

### Verification

- `cargo test -p classroom-app student_decodes_across_real_teacher_adaptive_bitrate_change -- --ignored --nocapture`: passed with real Mac capture, RTP fan-out, VideoToolbox encode and decode across the switch.
- Focused upward-recovery test passed. `cargo fmt --all -- --check`, final `cargo test --workspace --quiet` (56 default tests passed; one native opt-in test ignored by default), and strict workspace Clippy for macOS and `x86_64-pc-windows-gnu` passed.
- As this checkpoint changed only test code, release executables were not rebuilt and remain the previous checkpoint's versions: macOS `target/release/Classroom` SHA-256 `2bac95f25de8088d7973833999f4333cf963f938a524cff0d3363bc2e454483f`; Windows x64 `target/rustup-windows/x86_64-pc-windows-gnu/release/Classroom.exe` SHA-256 `1d2b988d428af5c5d3a9b3107d640b03667ef3699958b04b08324509d42cc509`. No Git repository is present, so no commit was made.

### Remaining risks and exact next step

- The forced reports are synthetic and the receiver is localhost, so this does not establish true network-loss adaptation, LAN smoothness, or Windows Media Foundation behavior. The upward path is policy-tested, not yet exercised through a real encoder. A real two-Mac visual/log check and the physical Windows 1/5/10/20/40 wired ladder remain required for M5/M6 acceptance.
- Next code task: strengthen multi-receiver adaptation decisions so one noisy/stale reporter cannot destabilize quality for all viewers, while preserving a single encoder and bounded feedback state; test with deterministic 1/5/40-receiver report sequences. Then collect two-Mac evidence only after this larger M6 batch is ready.

## M6 real Teacher-loop adaptive-bitrate diagnostic — 2026-09-25

Status: one controlled bitrate reduction and first post-switch IDR were verified through the real macOS capture/VideoToolbox/Teacher media loop. This is a local diagnostic, not two-Mac or Windows performance acceptance.

### What changed

- The Teacher media loop now returns a small summary and logs the number of applied bitrate changes and how many first access units after those changes were keyframes. Normal role commands and the single-encoder/latest-frame pipeline are unchanged.
- Added an opt-in ignored test that runs six seconds of real macOS screen capture and VideoToolbox encoding on localhost, injects three synthetic cumulative high-loss receiver reports into the same Teacher event channel used by live control connections, and asserts exactly one 1.2 → 1.02 Mbps bitrate reduction. It also asserts that encoding continues and the first access unit after the switch is a keyframe. The test is ignored by default because it needs Screen Recording permission and real media hardware; it is not a runtime simulation enabled for users.
- File changed: `apps/classroom/src/main.rs`.

### Verification

- `cargo test -p classroom-app teacher_applies_synthetic_receiver_loss_to_real_videotoolbox_encoder -- --ignored --nocapture`: passed on this Mac with a real six-second capture/encode loop. This confirms the full Teacher-side event-to-policy-to-VideoToolbox application path, but uses injected reports rather than actual packet loss or a receiving Student.
- `cargo fmt --all -- --check`, `cargo test --workspace --quiet` (55 default tests passed, one opt-in native test ignored), and strict workspace Clippy on macOS and `x86_64-pc-windows-gnu`: passed. macOS and Windows x64 release builds passed. The cross-build emitted the existing nonfatal host `rust-objcopy` warning; there is no new Windows runtime evidence.
- macOS executable: `target/release/Classroom`, SHA-256 `2bac95f25de8088d7973833999f4333cf963f938a524cff0d3363bc2e454483f`. Windows x64 PE executable: `target/rustup-windows/x86_64-pc-windows-gnu/release/Classroom.exe`, SHA-256 `1d2b988d428af5c5d3a9b3107d640b03667ef3699958b04b08324509d42cc509`. No Git repository is present; no commit was made.

### Remaining risks and exact next step

- This validates one downward bitrate step on VideoToolbox and the corresponding first IDR. It does not validate a real receiver's decode across that switch, bitrate recovery upward after healthy reports, or adaptation under 40-client wired-LAN load. Media Foundation adaptive bitrate remains cross-compiled only.
- Next code task: add a controlled receive/decode check across the forced bitrate boundary and cover upward recovery after sustained healthy reports, while keeping diagnostic traffic opt-in. Then obtain a two-Mac M6 visual/log comparison and eventually the required real-Windows 1/5/10/20/40 wired acceptance ladder. Do not move to unrelated Phase 2 features until the package's stop condition is met or explicitly changed by the owner.

## M6 Teacher feedback application and sampling hot path — 2026-09-25

Status: the adaptive receiver-feedback decision is wired through a testable Teacher encoder-application boundary; same-host executable smoke and cross-platform builds passed. M6 two-Mac dynamic-bitrate behavior and M5 wired-Windows scale acceptance remain open.

### What changed

- Moved the Teacher's bitrate-application decision into `QualityController::apply_bitrate`, which receives the one existing encoder. It commits the tracked bitrate only after the backend accepts it; on rejection it keeps the old bitrate and disables further adaptive proposals for that run. The Teacher media loop uses this same method and retains nonfatal warning behavior.
- Added deterministic tests that feed three high-loss receiver intervals through policy evaluation and the encoder-application boundary, confirm exactly one 6.0 → 5.1 Mbps update, and confirm that an encoder rejection preserves 6.0 Mbps and suppresses further proposals.
- The content sampler now reuses two bounded pixel-sample buffers instead of allocating a fresh sample vector for every captured frame. A test checks unchanged/changed ratios and stable capacities after warm-up. Capture/encode remain single-instance and latest-frame behavior was not changed.
- Files changed: `apps/classroom/src/quality.rs`, `apps/classroom/src/main.rs`.

### Verification

- `cargo fmt --all -- --check`, final `cargo test --workspace --quiet` (55 tests), and strict workspace Clippy for macOS and `x86_64-pc-windows-gnu` passed. Both macOS and Windows x64 release builds passed. Windows cross-build again emitted the known nonfatal host `rust-objcopy` warning; this is not a Windows runtime result.
- A same-host 8-second Teacher / headless Student executable smoke on macOS exited successfully. The Student joined late in that run and decoded about 17 frames before Teacher's timer ended, with zero missing packets or decoder errors in the observed interval. This is only a startup/receive/decode smoke, not a sustained bitrate transition or visual/latency acceptance test; subsequent connection-refused retries were expected after Teacher exited.
- macOS executable: `target/release/Classroom`, SHA-256 `8ca5bfb5f0412558ba785b2aeaed9b3f656eee47655972bc337893e56b72b200`. Windows x64 PE executable: `target/rustup-windows/x86_64-pc-windows-gnu/release/Classroom.exe`, SHA-256 `5121f7250688b1dd55132834baa70fe7fd5c0ef245ebf6301f76d90f295abd55`. No Git repository is present, so no commit was made.

### Remaining risks and exact next step

- Deterministic tests validate the Teacher's receiver-report-to-encoder handoff, and the prior checkpoint validates a real VideoToolbox codec bitrate transition, but no live LAN run has forced the adaptive threshold and observed the full stream after a switch. Receiver `missing_packets` remains a late-or-missing proxy rather than final network loss.
- Next: implement a bounded, opt-in diagnostic feedback scenario that can force and observe one real Teacher bitrate change on the same Mac without requiring a second machine; use it for one focused end-to-end run. Then reserve the two-Mac check for a larger M6 batch and the formal 1/5/10/20/40 acceptance ladder for physical Windows PCs.

## M6 bitrate transition and quality-policy bounds — 2026-09-25

Status: focused macOS codec transition verified, macOS and Windows x64 release builds complete. M6 two-machine runtime and M5 real-Windows wired-LAN performance acceptance remain open; no user-operated retest was required for this checkpoint.

### What changed

- VideoToolbox now creates the replacement H.264 compression session before committing an adaptive bitrate change. If session creation rejects the requested bitrate, the live session/configuration stay intact instead of failing on the next encoded frame and stopping the presentation. A successful change clears stale SPS/PPS and starts a new IDR boundary; explicit recovery keyframe behavior remains unchanged.
- Added a native codec test that encodes and decodes before and after a 1.2 Mbps → 0.6 Mbps change and verifies the new access unit is a decodable keyframe. The test also checks that a zero-bitrate request leaves the existing configuration intact.
- Shared quality control now keeps recovery FPS at or below a configured cap below 20 FPS. A receiver-report gap resets the sustained-loss/healthy streaks, so separated samples cannot cause a false bitrate decision. Both cases have focused tests.
- Files changed: `crates/codec-macos-vt/src/macos_impl.rs`, `apps/classroom/src/quality.rs`.

### Verification

- `cargo fmt --all -- --check`, final `cargo test --workspace --quiet` (52 tests, including the native VideoToolbox transition and both policy bounds), the focused 7-test quality suite, and strict macOS workspace Clippy passed. The focused native VideoToolbox transition test also passed independently.
- `cargo build --release --bin Classroom` passed after the final policy changes. macOS executable: `target/release/Classroom`, SHA-256 `92ad725081a18bb1264ae8404c8425f81b5fe93225a416607d954c920a396167`.
- Installed the `x86_64-pc-windows-gnu` Rust target and used the matching rustup toolchain for the cross-check. Strict Windows x64 workspace Clippy and release cross-build passed after the final policy changes. PE32+ x86-64 executable: `target/rustup-windows/x86_64-pc-windows-gnu/release/Classroom.exe`, SHA-256 `ef7bec7f700dc05261a2ded115944670f81ba7b72c2154a3bca41a292bfe8e01`. The Rust toolchain emitted its previously known nonfatal `rust-objcopy`/`libLLVM.dylib` warning while stripping a host build-script artifact; the final Windows executable was produced. Cross-build is not Windows runtime validation.
- No Git repository is present, so no checkpoint commit was made. A separate Cargo target directory was used for the rustup Windows toolchain to avoid mixing its artifacts with Homebrew Rust artifacts.

### Remaining risks and exact next step

- This proves a VideoToolbox codec-level bitrate switch, not that the Teacher's real receiver-report policy will trigger and recover smoothly during a two-Mac LAN presentation. Media Foundation runtime bitrate changes and the 1/5/10/20/40 real Windows-PC ladder remain untested.
- Next code step: exercise a deterministic receiver-report/loss sequence through the Teacher media loop (beyond the controller's unit tests) and verify that the requested bitrate is actually applied and IDR recovery remains bounded. After a meaningful batch, check on two Macs; defer formal Windows acceptance until physical lab PCs are available.

## M6 adaptive quality, first implementation — 2026-09-25

Status: shared policy, Teacher integration, configuration, and same-host macOS runtime verification complete. M6 performance acceptance remains open. The owner explicitly requested this next planned feature without another two-Mac recovery retest; this overrides development order only. M5's Windows wired-LAN 1/5/10/20/40, visual, and latency gates remain open; no unrelated Phase 2 feature is claimed complete.

### What changed

- Added a shared Teacher quality controller in `apps/classroom/src/quality.rs`. It samples a bounded 4-pixel grid from captured BGRA frames because macOS ScreenCaptureKit can report `dirty_region_ratio=1.0` even for small screen changes. Static content targets 5 FPS after 1.2 s, small/typing-like changes target 24 FPS briefly, and large motion immediately returns to the configured maximum (30 FPS default). Capture itself stays at maximum cadence to detect new activity promptly; the latest-frame slot remains capacity one.
- Three consecutive high encode-p95 (28 ms default) or process-CPU (80% default) intervals lower the active FPS ceiling in steps toward 24/20/15; five healthy intervals raise it. The policy keeps 1080p and one encoder.
- Teacher aggregates deltas from receiver reports. At least three high late-or-missing-packet intervals can propose a 15% bitrate reduction bounded by configured minimum; ten low-loss intervals can raise bitrate toward the configured target. Changes have a 10 s cooldown. Encoder rejection disables automatic bitrate changes for that run rather than interrupting sharing. On macOS, bitrate updates use the existing VideoToolbox session rebuild/IDR path; this needs separate two-Mac validation.
- RTP timestamps on the Teacher network path now use actual elapsed wall-clock time so variable-FPS packet streams retain real-time 90 kHz presentation timing. Local benchmark behavior is unchanged. Added M6 metrics to Teacher intervals and configuration defaults/validation. `video.adaptive=false` restores fixed FPS/bitrate behavior.
- Files: `apps/classroom/src/quality.rs`, `apps/classroom/src/main.rs`, `crates/classroom-config/src/lib.rs`, `config/default.toml`, `README.md`, this checkpoint.

### Verification

- `cargo fmt --all -- --check`, `cargo test --workspace` (49 tests), and `cargo clippy --workspace --all-targets -- -D warnings`: passed on macOS. Focused tests cover static idle, immediate wake, small-change tier, pressure downgrade/recovery, bitrate proposal, and adaptive-off behavior. `cargo build --release --bin Classroom`: passed.
- The first loopback run exposed that treating every changed sampled pixel as motion kept the live Mac desktop at 30 FPS; this was corrected with configurable small/large activity thresholds and separate hold times before final verification.
- Final same-host 1 Teacher / 1 headless Student Mac run: Teacher interval `target_fps` changed from 30 to 24 and encoded about 24 FPS during ongoing small desktop changes; one encoder remained active. Student decoded 213 frames over 9.00 s (~23.67 FPS), with zero missing packets, NACKs, keyframe requests, or decoder errors. Both processes exited successfully. Logs: `target/benchmarks/manual-m6-tiered/{teacher,student}.log`. This verifies the 24-FPS runtime tier and receive/decode compatibility, not visible image quality or static 5-FPS runtime on this live desktop.
- macOS release executable: `target/release/Classroom`, SHA-256 `0917f948383ab7a22b8bc25d990ee75de5e9353e0e65c9c0d2674b8602d9b18c`. No Git repository is present, so no checkpoint commit was created.

### Remaining risks and exact next step

- The real desktop continued to show small pixel changes, so 5-FPS static mode and 30-FPS motion wake were exercised in unit tests but not visually validated in this runtime pass. Adaptive bitrate decisions passed policy tests but no actual VideoToolbox or Media Foundation bitrate transition was triggered in the loss-free loopback. The receiver `missing_packets` metric includes late arrivals, so its 2% default threshold is a proxy rather than precise packet-loss measurement.
- On the two Macs, compare typing/scrolling/large motion and static slides with this build and record `target_fps`, actual encode/render FPS, bitrate, and any IDR/recovery pauses. If the owner wants to avoid another immediate LAN test, next code task is a controlled runtime bitrate-transition exercise plus Windows x64 cross-check when its Rust target is available. Do not mark M5/M6 or the Windows 40-PC requirement accepted from same-host evidence.

## M5 RTP fragment-loss quarantine and repeated IDR recovery — 2026-09-25

Status: implementation and focused same-host macOS loss test complete; two-Mac Wi-Fi and wired-LAN acceptance remain open.

### Evidence and change

- The owner's latest two-Mac Wi-Fi run had Teacher encode mostly near 30 FPS but Student receive events rose to 142 late-or-missing packets, a VideoToolbox `-12909` decoder error, a burst of `FU-A continuation without start` warnings, and interval preview rates commonly ~20–24 FPS. Those logs do not prove that every decoder error is network-caused; the present change specifically closes the known loss/recovery handling gaps.
- `H264Depacketizer` now detects RTP sequence discontinuity after jitter-buffer loss, discards the affected access unit, and quarantines its remaining fragments through the RTP marker. Malformed FU-A starts are likewise reported once per damaged access unit instead of producing a warning for every continuation fragment. The next intact access unit can be accepted.
- The Student no longer resets the depacketizer's quarantine state after such an error. It requests a recovery keyframe as before, and while still waiting for a usable keyframe it retries that request after 500 ms with the existing rate limit. This avoids an indefinite wait if the first recovery IDR or request is lost.
- Files: `crates/media-transport/src/lib.rs`, `apps/classroom/src/main.rs`, this checkpoint.

### Verification

- `cargo fmt --all -- --check`: passed; `cargo test -p media-transport -p classroom-app`: 6 transport and 13 app tests passed, including missing middle fragment and missing FU-A start; strict Clippy passed for both crates.
- `cargo build --release --bin Classroom`: passed. macOS release binary SHA-256: `fce490712ce716899215dcf1f0985507904aa9fa3f1baf7084933eeeb7d28d06`.
- One 1-Teacher / 1-visible-Student same-host run with `--drop-every-nth-rtp-packet 200` intentionally omitted 46 original packets. Teacher retransmitted 46; Student sent 46 NACKs, logged one RTP sequence-gap warning and no FU-A continuation storm or decoder errors, then completed with 461 decoded / 363 rendered frames over 15.89 s. Logs: `target/benchmarks/manual-fragment-recovery/{teacher,student}.log`. This validates the recovery path under sparse controlled loss, not the owner's Wi-Fi burst-loss pattern or 40-PC wired scaling.
- This workspace has no Git repository, so no checkpoint commit was created.

### Remaining risks and exact next step

- The same-host loss run still rendered only ~22.8 FPS while decoding ~29.0 FPS, with 97 latest-frame replacements and a ~100 ms render-gap p95. The patch targets damaged access-unit handling and long recovery stalls; it does not establish smooth 30 FPS preview or fix independent VideoToolbox failures without RTP loss.
- Re-run one Teacher and one visible Student on the two Macs using this exact binary and retain full logs. Check whether a FU-A warning storm disappears, whether any freeze lasts multiple seconds, and whether `decode_errors`, keyframe requests, interval render FPS and gap p95 improve. If a `-12909` still occurs with zero preceding RTP sequence gaps, investigate the macOS decoder/bitstream path separately. Run the formal 1/5/10/20/40 Windows wired-LAN ladder when the lab machines are available; unrelated Phase 2 features remain deferred by the package's stop condition.

## M5 Student RTP-drain preview responsiveness — 2026-09-24

Status: shared receiver-path change and same-host macOS verification complete; two-Mac LAN and Windows scale acceptance remain open.

### Evidence and change

- The owner's two-Mac log from the previous build showed Teacher capture/encode mostly ~29 FPS, but Student decode ~25–30 FPS and render only ~18–24 FPS during the shown intervals. `render_slot_replacements` rose from 166 to 203 while the cumulative render-gap p95 remained ~125–137 ms. This is not evidence that a 60 FPS Dock animation is required: Phase 1 caps the stream at 30 FPS, and smoothness at that cap is still not fully demonstrated.
- Previously the Student decoded every ready RTP access unit in a burst before checking whether any preview frame was due. It can now present a due newest frame after each successfully processed access unit as well as at the end of the drain. This preserves H.264 decode order, the one-frame pending preview bound, and the no-catch-up pacing policy while preventing a long drain from suppressing every intermediate display opportunity.
- Student interval telemetry now includes `render_slot_replacements_interval` and `render_gap_interval_p95_ms`; the existing cumulative values remain available. This separates current one-second cadence from startup/recovery history. The gap windows remain bounded.
- Files: `apps/classroom/src/main.rs`, `apps/classroom/src/presentation_pacer.rs`, this checkpoint.

### Verification

- `cargo fmt --all -- --check`, `cargo test -p classroom-app` (13 passed), `cargo clippy -p classroom-app --all-targets -- -D warnings`, and `cargo build --release --bin Classroom`: passed.
- One 1-Teacher / 1-visible-Student same-host macOS loopback run exited successfully. Student decoded 316 and rendered 309 frames over 11.87 s (~26.6 / 26.0 FPS), with zero missing packets, NACKs, and decoder errors; only 7 preview frames were replaced. Teacher encoded 428 frames over 16.01 s (~26.7 FPS). This is a mostly static live desktop workload, not a controlled motion comparison. Logs: `target/benchmarks/manual-drain-render/{teacher,student}.log`.
- macOS release executable: `target/release/Classroom`, SHA-256 `a344a024dcbde257efd11a56868c44682eb3e9abd690c4a5a2b4718ea3198dbc`. No Git repository is present; no commit was made.

### Remaining risks and exact next step

- Repeat the fast-typing/animation segment on two Macs using this build, comparing Student interval render FPS, interval gap p95, and replacement count against the owner's previous logs. A better localhost result does not prove LAN responsiveness or glass-to-glass latency. If render still trails decode markedly, profile receive/jitter/decode scheduling rather than raising the 30 FPS cap blindly.
- The 1/5/10/20/40 separate-PC Windows acceptance ladder, Windows image validation, and end-to-end latency measurements remain unverified. Per `AGENTS.md` and `docs/06_PERFORMANCE_ACCEPTANCE.md`, do not start unrelated Phase 2 features such as file transfer until Phase 1 acceptance or an explicit owner change to that stop condition.

## M5 Student preview cadence — 2026-09-24

Status: shared Student preview pacing implemented and verified on same-host macOS; two-Mac LAN validation remains open.

### What changed

- The Student continues decoding every H.264 access unit to preserve reference state, but its visible preview now holds only the latest decoded frame and presents at the advertised maximum FPS. A late tick preserves the nominal clock when only slightly delayed; after a longer stall it resets without replaying old frames in a burst. Headless receive/decode behavior is unchanged.
- Decoder failure or depacketizer loss clears the waiting preview frame. Renderer failures now propagate as rendering errors instead of being counted as codec errors.
- Added cumulative `render_slot_replacements` and a bounded-window `render_gap_p95_ms` (last 4096 presentation gaps) to Student interval/final logs. Added unit tests for stale-frame replacement, stall behavior, and small-tick-delay drift.
- Files: `apps/classroom/src/main.rs`, `apps/classroom/src/presentation_pacer.rs`, this checkpoint.

### Verification

- `cargo fmt --all -- --check`: passed; `cargo test -p classroom-app`: 13 passed; `cargo clippy -p classroom-app --all-targets -- -D warnings`: passed; `cargo build --release --bin Classroom`: passed.
- A first same-host visible loopback run exposed that scheduling every tick from actual render time reduced preview to ~25.5 FPS even though decode was ~28.4 FPS. The clock was corrected and the same short test repeated.
- Corrected same-host 1 Teacher / 1 visible Student run: Teacher encoded 524 frames over 18.03 s (~29.1 FPS); Student decoded 408 and rendered 393 frames over 13.89 s (~29.4 and ~28.3 FPS), with zero missing packets, NACKs, and decoder errors. Preview replaced 14 stale frames; render-gap p95 was 45.94 ms. Both processes exited successfully. Logs: `target/benchmarks/manual-pacer-2/{teacher,student}.log`. The owner also observed faster typing response in this same-machine preview.
- macOS release executable: `target/release/Classroom`, SHA-256 `cb8bccbc3da27b292e55ce2e66d03f2f2aa1c18299e3f9f540bf0f9c042bf626`.
- Windows x64 cross-check was attempted but this current Homebrew Rust sysroot has only the macOS target; `cargo check --target x86_64-pc-windows-gnu` stopped at `can't find crate for core`. No new Windows build or runtime result is claimed. This workspace has no Git metadata, so no commit was made.

### Remaining risks and exact next step

- The reported two-Mac LAN run had Teacher mostly near 29–30 capture/encode FPS but Student decode/render intervals fluctuating around 20–37 FPS, plus 172–177 cumulative late-or-missing packet events and two decoder errors. Same-host results cannot prove LAN smoothness or glass-to-glass latency, and this patch does not resolve packet loss/NACK cache misses.
- Repeat one Teacher / one visible Student on the two Macs with the new executable, a fast-typing/animation segment, and both full logs. Compare subjective responsiveness plus `render_fps`, `render_slot_replacements`, `render_gap_p95_ms`, missing/NACK/decode-error deltas. If LAN cadence remains low, isolate UDP receive/jitter/decode timing rather than expanding to unrelated features. Windows physical-PC image and 1/5/10/20/40 scaling acceptance remain pending.

## M5 macOS VideoToolbox recovery and late-join — 2026-09-24

Status: codec-health and one-/two-Student localhost tests pass on the current Mac; M5 acceptance remains open.

### What changed

- The macOS encoder now obtains AVCC NAL length-field width from the encoded sample's CoreMedia H.264 format description and parses the sample using that declared width. It no longer guesses Annex-B from a few leading bytes, which could misclassify valid length-prefixed AVCC data.
- The encoder now rejects missing H.264 sample descriptions, unsupported AVCC width, malformed NAL lengths/types, and invalid SPS/PPS rather than emitting a suspect access unit. Decoder format-description creation also validates parameter-set NAL types.
- When VideoToolbox reports a decoder callback error, the decoder drops that decompression session so the app's existing keyframe recovery can initialize a clean reference state.
- macOS now honors late-join and recovery keyframe requests by scheduling a rebuild of its existing encoder wrapper before the next frame. Requests are rate-limited to one rebuild per 250 ms so simultaneous/repeating clients cannot continuously churn the encoder. Windows retains its existing immediate keyframe behavior.
- Files: `crates/codec-macos-vt/src/macos_impl.rs`, `apps/classroom/src/main.rs`, this checkpoint.

### Verification and remaining evidence

- `cargo fmt --all -- --check`: passed.
- `cargo clippy -p classroom-app -p codec-macos-vt --all-targets -- -D warnings`: passed.
- `cargo test -p classroom-app -p codec-macos-vt`: 15 tests passed (11 app, 4 VideoToolbox codec), including AVCC parsing and the 250 ms keyframe-rebuild rate limit.
- `cargo build --release --bin Classroom`: passed. macOS executable SHA-256: `eb79dac2780fd34fb5d782d3965c19e97e92025f5bd773f84f1ce2f4977f02fb`.
- The first two 5-Student runner attempts were launched inside the restricted shell and failed with `VTCompressionSessionCreate failed: -12903`. The pre-IDR-change release build (`ac8163095e2c86fe73cedb68672c000524c853c3df7cab9464db46141fe85e09`) run outside that sandbox completed: `target/benchmarks/run-1790250084179-38279/`. All 5 receivers reported 0 missing packets and 0 decoder errors; each decoded 126 frames over 10 seconds (~12.6 FPS), with 1 encoder. The low same-host rate reflects running Teacher plus five native decoders on this Mac and is not a LAN scale result. This establishes that the prior `-12903` was caused by restricted test execution, not a persistent encoder defect.
- Visible-path local test (Student `headless=false`) completed with 282 decoded/rendered frames, zero missing packets/decode errors, and 0 pre-keyframe discards after the teacher logged an immediate IDR request. A second bounded run added a late second Student while the first remained connected: both Students completed (349 and 177 rendered frames), each with zero missing packets/decode errors/pre-keyframe discards; Teacher remained at exactly one encoder. Full logs: `target/benchmarks/manual-idr2-{teacher,student-01,student-02}.log`. The app-reported rendered-frame counts verify the renderer path ran; no screenshot-based subjective image-quality check is claimed.
- This workspace has no Git metadata, so no checkpoint commit was created.

### Exact next step

Continue M5 with a measured one-/two-client visible image review on the second Mac when useful, then run the formal 1/5/10/20/40 ladder on separate Windows PCs when they are available. Keep all loopback results labeled as same-host diagnostics; do not start M6 or Phase 2 features or mark M5 complete until its performance acceptance evidence exists.

## M5 Stage 3 — native benchmark runner and reports (2026-09-23)

Status: Stage 3 implemented and exercised on macOS; Windows x64 cross-build complete. M5 media-health/scale acceptance remains open. The owner has deferred further Parallels/Windows image-quality testing until physical Windows PCs are available.

### What changed

- Added `--benchmark-students N --benchmark-seconds S --benchmark-output DIR` to the existing executable on both platforms. It starts one real Teacher and N headless Student processes from the same executable, waits for Teacher media readiness, assigns distinct UDP ports, saves effective configuration and unmodified logs, and generates CSV reports.
- Each invocation creates a fresh absolute run directory. Port preflight, bounded startup/run timeouts, child exit checks, and RAII cleanup on error/Ctrl+C prevent successful-looking incomplete runs and orphaned owned children. The runner never kills by process name or touches separately started Classroom sessions.
- Added offline `--benchmark-report DIR --expected-students N` without requiring PowerShell/Python, a network adapter, or screen-capture permission. It requires exactly N Student logs, completed single sessions, one encoder, and a complete interval with N endpoints throughout. Compacted/incomplete/reconnected session logs are rejected rather than approximated.
- Teacher interval telemetry now includes actual interval duration and minimum/maximum endpoint counts. Teacher rates/CPU in the CSV use duration-weighted full-count intervals; p95/replacement totals and Student summaries are explicitly whole-run. Receiver health is reported separately from process success. Headless/loopback evidence is never promoted to visible-render, glass-to-glass, or physical-LAN acceptance.
- Redirected output is now plain text on Mac as well as Windows. Existing manual Teacher/Student commands remain available.
- Files: `apps/classroom/src/benchmark.rs`, `benchmark_report.rs`, `main.rs`, app `Cargo.toml`, `Cargo.lock`, serializable configuration structs, `README.md`, this checkpoint.

### Verification and measurements

- `cargo fmt --all -- --check`: passed.
- `cargo test --workspace`: **37 tests passed**, including port range limits, CLI mode validation, weighted report calculation excluding join intervals, rejection of incomplete/compacted/reconnected logs, encoder invariant, and no-frame receiver health.
- Strict Clippy passed for the entire workspace on macOS and `x86_64-pc-windows-gnu`.
- Release builds passed for Mac and Windows x64. Native build/tests used Homebrew Cargo plus temporary Cargo/Swift/Clang caches and allowed host execution because SwiftPM cannot start its compiler sandbox inside the restricted shell. Windows cross-build retains the known nonfatal host `rust-objcopy` warning.
- Real run: `target/release/Classroom --benchmark-students 5 --benchmark-seconds 12 --benchmark-output /Users/mvmanh/Development/netsupport-school/target/benchmarks`.
- Evidence: `target/benchmarks/run-1790172869519-55776/`. Automatic and offline report generation both succeeded. Teacher had 1 encoder and 10 full-count intervals totaling 10.156 s: capture 19.595 FPS, encode 26.094 FPS, UDP send 4.613 Mbps, resident memory peak 81.5 MiB, whole-run encode p95 22.261 ms.
- All 5 headless Students received 1,482 packets with zero missing/NACKs, but each decoded only 78 frames (~6.50 FPS) and reported 11 decoder errors. The CSV correctly marks all 5 `needs_attention`. Errors include the previously documented VideoToolbox `-12909` and H.264 format-description `-12712`; zero packet loss does not establish a healthy encoded stream. No improved codec-health or scale acceptance is claimed.
- Interruption check: started 1 Student on control/media ports 47900/47910, sent Ctrl+C, observed exit status 1 with a clear interruption message, and confirmed no child remained with configuration path `target/benchmarks/run-1790172940224-56106/config.toml`. Partial logs were preserved.
- Mac executable: `target/release/Classroom`, SHA-256 `2b8bc60aa48a696695a3556295bc10d56f2e957d45cda3bb383d75c56d30ee6f`.
- Windows executable: `target/x86_64-pc-windows-gnu/release/Classroom.exe`, SHA-256 `580af1ee871dd175b07d3c630704ee626041c9dca10b43949b369bd78be8ee06`. Runtime validation deferred by owner.
- No checkpoint commit: the package has no Git repository.

### Exact next step

Use this repeatable local harness to address the outstanding shared H.264/VideoToolbox format/decoder errors before calling the local M5 receiver path healthy. Preserve the saved failing logs. Windows image correctness and the formal 1/5/10/20/40 separate-PC ladder remain pending until real Windows machines are available. M6 adaptive quality and Phase 2 features have not been started.

## M5 Windows visual-quality investigation — 2026-09-23

Status: code mitigation built; Windows visual verification pending.

### Evidence from owner-provided `logs_compact.zip`

- One visible Windows Student received 20,793 RTP packets with only 2 missing and decoded/rendered 1,487 frames in 49.86 seconds (~29.82 FPS). Teacher used one software Media Foundation H.264 encoder. Severe black/noisy preview regions are therefore unlikely to be primarily caused by packet loss in this one-viewer run.
- Five Student processes in the same Parallels VM contended for CPU: Teacher capture fell to ~7 FPS and each headless receiver decoded ~18 FPS. This is a local VM stress result, not evidence for or against five separate classroom PCs.
- The Windows decoder emitted `MF_E_TRANSFORM_STREAM_CHANGE` (`0xC00D6D61`) on initial output. Previously the app discarded that access unit instead of renegotiating the MFT output type. The NV12 converter also assumed a tightly packed row stride without checking the decoder output layout.

### What changed

- On an MFT output stream change, enumerate new decoder output types, select NV12 at the configured dimensions, call `SetOutputType`, and retry `ProcessOutput` with a bounded attempt count.
- Validate the current decoder subtype, read or infer the positive NV12 row stride, convert padded NV12 rows correctly, and log the first output layout for Windows diagnosis.
- Added a focused unit test for padded NV12 rows. No transport, fan-out, or macOS media behavior was changed.

### Verification

- `cargo fmt --all`: passed.
- `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`: passed.
- `cargo test -p codec-windows-mf`: 6 tests passed, including padded-row conversion.
- `cargo build --release --target x86_64-pc-windows-gnu --bin Classroom`: passed. PE32+ x86-64 executable: `target/x86_64-pc-windows-gnu/release/Classroom.exe`, SHA-256 `f910431a22292300585e236701f39790a5b2ffba621aac1eb08aff94f40cd238`.
- `cargo test --workspace` could not complete in this restricted macOS environment because SwiftPM's `sandbox-exec` failed while building `apple-metal`/`apple-cf`; this is unrelated to the Windows codec code. The Windows executable was cross-built but has not been run on Windows.

### Remaining risks and exact next step

- A format-change warning or padded NV12 may not explain all visible artifacts; there is no screenshot or decoded-frame dump to localize the remaining corruption. The current code cannot guarantee that `IMFMediaBuffer::Lock` reports a 2D surface's true pitch in every MFT backend. Do not claim Windows visual quality fixed yet.
- Retest one Teacher and one visible Student on Windows with this exact SHA-256 executable. Compare preview and capture the new `renegotiated NV12 decoder output` / `Media Foundation decoder output layout` lines plus a screenshot if artifacts remain. Only after the one-viewer image is clean should the 5/10/20/40-client acceptance ladder resume on separate PCs.

## Current milestone

`M5 — scale fan-out (Stage 3: native runner and cross-platform CSV reports)`

Status: Stage 3 tooling is implemented and runtime-verified on macOS (details above). M5 is not complete: multi-receiver codec health, Windows visual quality, the physical 1/5/10/20/40-client ladder, and end-to-end latency/pressure evidence remain pending. Windows runtime tests are deferred at the owner's request.

M0, M1, and macOS M2 remain complete. The owner explicitly overrode the earlier Phase 1 macOS deferral because development is now being done away from the Windows lab. This does not change Windows as the production target or allow macOS results to stand in for DXGI/Media Foundation validation.

## What changed

- Changed embedded development defaults to localhost: Teacher now binds `127.0.0.1` and Student connects to `127.0.0.1` when no address flags or adjacent `config.toml` override are supplied. A localhost Student now advertises `127.0.0.1:47810` as its UDP endpoint, preventing the mixed loopback/Parallels-adapter route that caused the Teacher to exit on its first media send. LAN use remains explicit with Teacher `--bind-ip <teacher-LAN-IP>` and Student `--teacher-ip <teacher-LAN-IP>`.
- Increased the Student RTP UDP receive socket buffer to 4 MiB using `socket2`; this addresses burst loss observed on Windows/Parallels loopback where the preview worked but receiver missing-sequence counts grew rapidly.
- Preserved the existing Windows DXGI capture, Media Foundation H.264, and Win32 preview backends.
- Added `capture-macos-sck`: ScreenCaptureKit display capture at the configured resolution/FPS, cursor inclusion, dirty-rectangle ratio, packed BGRA output, bounded latest-frame replacement, deadline waits, and capture metrics.
- Added `codec-macos-vt`: real-time VideoToolbox H.264 encode/decode, no frame reordering, target bitrate/FPS/GOP, SPS/PPS extraction and caching, AVCC/Annex-B conversion, keyframe recovery, BGRA IOSurface input, and BGRA/NV12 decoded-frame handling.
- Added `renderer-macos`: resizable native preview window with aspect-ratio preservation and BGRA presentation.
- Moved shared `H264Config`, `CodecDescriptor`, and backend-kind types into `media-core` so platform codecs implement one interface.
- Made the benchmark select native platform backends automatically. On macOS the preview loop stays on the main thread as required by the window system; Windows media work remains on a blocking worker away from Tokio.
- Added the macOS Swift runtime rpath required by ScreenCaptureKit bindings, scoped only to macOS target builds.
- Updated the owner decision log and README commands/permissions.
- Added `media-transport`: RTP v2/H.264 packetization (single-NAL and FU-A), Annex-B depacketization, bounded 40 ms jitter buffer, loss counters, and fragmentation/jitter unit tests.
- Extended the length-prefixed Protobuf control plane with `StartPresentation` and `ReceiverReport`. Teacher registers a student media endpoint and triggers an IDR; Student binds UDP before registration, receives/depacketizes/decodes/renders remotely, then reports health over the TCP control connection.
- Added `--loopback` and `--share-seconds` for the same-Mac M3 runtime test. The explicit loopback override uses `127.0.0.1` and does not affect normal physical-adapter selection.
- Added M4 control messages `Nack` and `RequestKeyframe`, plus Teacher-side forwarding of NACK, keyframe, and receiver-report events from each TCP connection to the media loop.
- Added a bounded, configurable 300 ms teacher RTP retransmission cache. The receiver holds a small detected gap for one jitter budget, NACKs at most 32 useful sequences, discards packets that arrive after playback has advanced, and asks for an IDR when depacketization/decoding becomes unrecoverable.
- Added `--drop-every-nth-rtp-packet` as a disabled-by-default Teacher test hook for reproducible loss tests; it never affects retransmitted packets.
- Added Student automatic reconnect: it retains its bound UDP port, retries Teacher registration every 500 ms while unavailable, and starts a fresh presentation/decode/render session after control loss or Teacher restart without extending the original `--share-seconds` deadline.
- Added Student `--headless` mode for local fan-out smoke tests. It still performs UDP receive, jitter buffering, RTP/H.264 depacketization, and native decode, but creates no preview window and reports decode FPS separately from zero render FPS.
- Changed the Teacher endpoint roster to be keyed by `client_id`, and added connection-lifetime cleanup so a disconnected Student is removed from UDP fan-out instead of remaining as a stale endpoint.
- Added explicit `encoder_instances=1`, actual interval packet counts, and byte-accurate aggregate UDP send Mbps to Teacher logs. Fan-out still packetizes once and sends the same encoded packets to every active endpoint.
- Added an IDR-aware receiver gate: a late/recovering Student discards inter frames until an Annex-B IDR is present. IDR detection now accepts every valid NAL priority-bit combination rather than assuming only the `0x65` header.
- On macOS, late-join and recovery requests wait for the next scheduled VideoToolbox IDR because the current backend implements forced IDR by rebuilding the compression session, which disrupts existing receivers. Windows Media Foundation retains immediate on-demand IDR behavior.
- Added safe cross-platform process sampling via `sysinfo` 0.33.1: CPU percent and resident working-set bytes are logged by both Teacher and Student each second and at session completion. CPU percent uses one fully busy logical CPU as 100%, so a process may exceed 100%.
- Added Teacher capture FPS/frame count, capture p50/p95, current encoded-stream bitrate, and explicit capture-slot replacement count. The macOS ScreenCaptureKit callback increments the replacement count whenever its bounded latest-frame slot overwrites an unconsumed frame.
- Added Student receive Mbps, per-interval decode/render FPS, decode-error count, and pre-keyframe discard count. A Teacher now labels repeated submissions of the latest frame separately from newly captured frames; continued submissions keep scheduled GOP/IDR generation alive for late join on a quiet desktop.
- Added `scripts/m5-report.ps1` to validate a complete scale interval and one-encoder invariant, then create per-scale Teacher summary and per-Student CSV files from finished Windows logs. It keeps Teacher full-run encode p95 and frame-replacement totals explicitly labeled as whole-run metrics.

## Files/modules changed

- `Cargo.toml`, `Cargo.lock`
- `apps/classroom/Cargo.toml`, `apps/classroom/build.rs`, `apps/classroom/src/main.rs`
- `crates/media-core/src/lib.rs`
- `crates/codec-windows-mf/src/lib.rs`
- `crates/capture-macos-sck/`
- `crates/codec-macos-vt/`
- `crates/renderer-macos/`
- `crates/media-transport/`
- `crates/protocol/src/lib.rs`
- `crates/control-transport/src/lib.rs`
- `config/default.toml`
- `README.md`
- `docs/11_DECISION_LOG.md`
- `checkpoints/PROGRESS.md`
- `apps/classroom/src/process_metrics.rs`
- `scripts/m5-report.ps1`

## Commands run

```text
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings
cargo build --release --target x86_64-pc-windows-gnu --bin Classroom
cargo test --workspace  # 28 tests after localhost-default regression fix
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings
cargo build --release --target x86_64-pc-windows-gnu --bin Classroom
cargo test --workspace  # 29 tests after M5 IDR detection coverage
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings
cargo build --release --target x86_64-pc-windows-gnu --bin Classroom
target/release/Classroom --role teacher --share-seconds 25
target/release/Classroom --role student --headless --media-port 47810 --share-seconds 10
# repeated concurrently with media ports 47812, 47814, 47816, and 47818
cargo test --workspace  # 29 tests after M5 Stage 2 telemetry
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings
cargo build --release --bin Classroom
cargo build --release --target x86_64-pc-windows-gnu --bin Classroom
target/release/Classroom --role teacher --share-seconds 25
target/release/Classroom --role student --headless --share-seconds 10
cargo run --bin Classroom -- --capture-benchmark-seconds 3 --display-index 0
cargo run --release --bin Classroom -- --local-media-benchmark-seconds 5 --display-index 0
target/release/Classroom --local-media-benchmark-seconds 30 --display-index 0
target/release/Classroom --role teacher --loopback --share-seconds 22
target/release/Classroom --role student --loopback --share-seconds 10
target/release/Classroom --role teacher --loopback --share-seconds 22 --drop-every-nth-rtp-packet 500
target/release/Classroom --role student --loopback --share-seconds 34
target/release/Classroom --role teacher --loopback --share-seconds 5  # stop, then launch Teacher again
```

The local dependency builds used temporary Cargo/Swift/Clang caches under `/private/tmp` because the managed development environment cannot write normal user caches.

## Verification

- 28 unit/integration tests passed on macOS, including RTP FU-A fragmentation/depacketization, NACK-gap detection, and late-retransmission discard tests, Windows cursor/color/H.264 tests, and macOS AVCC/Annex-B tests.
- Strict Clippy passed with warnings denied for all macOS workspace targets.
- Strict Clippy passed for all `x86_64-pc-windows-gnu` workspace targets after adding the macOS crates, confirming the platform selection did not type-break Windows.
- Strict Clippy and a release cross-build passed again for `x86_64-pc-windows-gnu` after the M3 control/RTP integration; the result is a PE32+ x86-64 Windows executable. This is build evidence only, not Windows M3 runtime evidence.
- The Windows release executable cross-built as PE32+ x86-64. Current SHA-256: `fec5954d2f10922a9027028db2ff16dcd65ef75115a1092b33c8f0c3a545cecc`.
- The localhost-default regression fix passed all 28 workspace tests and strict Clippy for macOS and the Windows GNU target. Its rebuilt PE32+ x86-64 executable has SHA-256 `6ab4dc91bf86265a4169f77549a608a92d6fc70fbd221a494b75532d5887cdb7`; runtime verification on Windows remains pending.
- The RTP receive-buffer fix passed all 28 workspace tests and strict Clippy for macOS and `x86_64-pc-windows-gnu`. Rebuilt PE32+ x86-64 executable SHA-256: `469578a7e2b3faadd7a4978f91584894ef86e395d1619cdc9e8a3b964c2ce5db`.
- M5 Stage 1 passed all 29 workspace tests and strict Clippy for macOS and `x86_64-pc-windows-gnu`. The rebuilt PE32+ x86-64 executable SHA-256 is `9b02695300dd5f4be47cc66ec4a5ecb3133a34761e1a953f1cf77ebd4d1e57e5`.
- M5 Stage 2 passed all 29 workspace tests, strict Clippy on macOS and `x86_64-pc-windows-gnu`, and release builds for both platforms. The new PE32+ x86-64 executable SHA-256 is `94c22cac9866b32f68b6ba9454726013f8f6c100b0c3148e34fe4d5ac91f1f85`.
- Windows 11 ARM Parallels loopback runtime retest succeeded with the rebuilt executable: Teacher software Media Foundation H.264 encoded about 28.8--29.0 FPS; Student received 11,743 packets with `missing_packets=0`, reached 29.08 FPS, and decoded/rendered 485 frames before the short run ended. This confirms the enlarged receive buffer fixes the previous burst-loss behavior in this environment.
- A real macOS ScreenCaptureKit capture run acquired 85 1920x1080 frames in 3.009 s (28.244 fps), with capture p50/p95 2.296/3.362 ms and cursor included in all 85 frames.
- A 5-second optimized local loop encoded, decoded, and rendered 142/142/142 frames. After startup, per-second intervals held approximately 30 fps; pipeline p50/p95 was 26.680/28.578 ms.
- A 30-second optimized soak completed normally at 1920x1080: 885 encoded, 883 decoded/rendered, 29.478 encode fps, 29.412 render fps, encode p50/p95 17.410/17.779 ms, decode p50/p95 8.435/9.560 ms, render p50/p95 1.441/3.025 ms, and local-pipeline p50/p95 26.958/29.781 ms.
- A real M3 same-Mac loopback release run completed normally: TCP registration at `127.0.0.1:47800`, RTP/H.264 UDP to `127.0.0.1:47810`, 2,071 received packets, zero missing packets, 294 decoded/rendered frames in 10.002 s (29.4 fps), 1.826 Mbps receive bitrate, 40 ms jitter budget, decode p50/p95 8.521/9.522 ms, and render p50/p95 1.439/1.753 ms.
- A real M4 same-Mac loss-recovery release run with Teacher dropping every 500th original RTP packet completed without a freeze: Student sent 4 NACKs, reported 2 packets that ultimately missed the recovery deadline, requested 1 IDR after an unrecoverable H.264 access unit, then continued to decode/render 263 frames in 10.014 s. Decode/render p95 was 9.735/1.657 ms.
- A same-Mac Teacher restart test completed: Student began while Teacher was unavailable and retried registration; it registered once Teacher started, lost the first control connection when that Teacher exited, then registered again with the restarted Teacher and continued to send receiver reports/render frames. The Student test deadline was preserved across retries.
- A five-Student same-Mac headless fan-out run registered five distinct UDP endpoints while Teacher continued to report exactly one encoder instance. Every receiver reported zero missing RTP packets, all five produced decoded frames, and Teacher removed an endpoint when its control session ended. Teacher averaged about 22.2 encode FPS while also hosting five native VideoToolbox decoders; this single-machine contention result is not representative of five separate PCs.
- The focused Stage 2 one-Student late-join run completed after restoring repeated encoding of the latest screen frame: Student received 1,640 packets with zero missing and decoded 259 frames in 10.012 s (25.87 FPS overall), while Teacher reported one encoder, capture FPS and frame replacements, CPU/working-set samples, and 171 repeated-frame submissions in its 25.024 s run. This specifically verifies that a quiet desktop still produces periodic IDRs for a late-joining receiver.
- The Windows PowerShell report script has not been executed on Windows or PowerShell because neither runtime is accessible from this Mac shell; its runtime validation is the next gate. No real Windows scale acceptance is claimed from the cross-build.
- No Git checkpoint commit was created because this package is not a Git working tree.

## Measurements

| Metric | Result | Environment |
|---|---:|---|
| Tests (M2) | 24 passed | macOS development host |
| Tests | 26 passed | macOS development host after M3 |
| Tests | 28 passed | macOS development host after M4 core recovery |
| Tests | 29 passed | macOS development host after M5 Stage 1 |
| Strict Clippy | passed | macOS + Windows GNU target |
| macOS capture | 1920x1080, 28.244 fps | 3-second functional run |
| macOS capture latency p50/p95 | 2.296 / 3.362 ms | ScreenCaptureKit |
| macOS M2 encode/render FPS | 29.478 / 29.412 | 30-second release soak |
| macOS encode p50/p95 | 17.410 / 17.779 ms | VideoToolbox H.264 |
| macOS decode p50/p95 | 8.435 / 9.560 ms | VideoToolbox H.264 |
| macOS render p50/p95 | 1.441 / 3.025 ms | native preview |
| macOS pipeline p50/p95 | 26.958 / 29.781 ms | local loop |
| macOS encoded bitrate | 4.885 Mbps | 6 Mbps target, activity-dependent |
| macOS M3 loopback | 294 decoded/rendered in 10.002 s | Teacher and Student processes, `127.0.0.1` |
| macOS M3 packet loss | 0 / 2,071 packets | 40 ms jitter buffer |
| macOS M3 decode/render p95 | 9.522 / 1.753 ms | VideoToolbox + native remote preview |
| macOS M4 controlled loss | 4 NACK, 1 IDR request, 263 rendered / 10.014 s | `--drop-every-nth-rtp-packet 500` |
| macOS M4 reconnect | Student registered before and after Teacher restart | 500 ms retry, same UDP port |
| macOS M5 fan-out | 5 endpoints, 1 encoder instance | same-Mac headless smoke test |
| macOS M5 receiver loss | 0 missing at all 5 receivers | distinct localhost UDP ports |
| macOS M5 teacher encode | about 22.2 fps | Teacher plus 5 VideoToolbox decoders on one Mac |
| macOS M5 Stage 2 late join | 259 decoded frames / 10.012 s, 0 / 1,640 missing packets | one headless Student, localhost |
| macOS M5 Stage 2 Teacher | 1 encoder, 3 capture-slot replacements, 171 repeated-frame submissions | 25.024 s run with one late join |
| Windows cross-build | passed | macOS -> x86_64-pc-windows-gnu |
| Windows DXGI M1 | passed previously | real teacher PC |
| Windows Media Foundation M2 | pending | no Windows machine currently available |

## Architecture deviations / owner decisions

- The original package deferred macOS backends. The owner explicitly requested them for M2 on 2026-09-23 because only a Mac is currently available. The implementations use the already-planned future native technologies and preserve all shared interfaces.
- macOS capture copies ScreenCaptureKit BGRA buffers into packed CPU memory, and decode converts NV12 to BGRA on CPU when necessary. The measured 30-second release run is close to 1080p30, so zero-copy Metal rendering remains an optimization rather than a prerequisite for continuing.
- VideoToolbox keyframe/bitrate changes rebuild the real-time compression session before the next frame. This reliably makes the next frame independently decodable but is more disruptive than an in-session force-keyframe property; M3 recovery testing should measure the impact.
- M5 therefore defers macOS keyframe requests to the next scheduled IDR. The Windows Media Foundation backend still performs immediate keyframe requests; a future VideoToolbox backend should use an in-session per-frame force-keyframe option before macOS late-join latency is considered complete.

## Known issues / risks

- The 30-second macOS run averaged 29.41 rendered fps rather than exactly 30 because of startup and ten schedule overruns. Steady-state one-second intervals were generally 29.9–30.1 fps, with occasional 27–29 fps intervals.
- VideoToolbox retained two startup frames in the 30-second run (885 encoded vs. 883 rendered). The deficit remained constant after startup and did not form a growing queue. A decoder drain API should be added if exact end-of-stream accounting becomes important.
- ScreenCaptureKit requires macOS Screen Recording permission. A first launch may require approval and relaunch.
- The macOS backend depends on current ScreenCaptureKit/VideoToolbox Rust bindings that compile small Swift bridges. Xcode command-line tooling is therefore required to build the Mac development binary.
- Windows Media Foundation runtime behavior, selected hardware/software MFT, and sustained Windows 1080p30 remain unverified. The Mac result must not close that deployment acceptance gate.
- The GNU Windows cross-build emits a non-fatal host `rust-objcopy` strip warning because the temporary Rust toolchain copy lacks `libLLVM.dylib`; the resulting executable is still reported as stripped PE32+ and built successfully.
- M3 has only been exercised over localhost. It verifies the control/media/decode/render vertical slice but cannot measure wired-LAN loss, end-to-end delay, Windows firewall behavior, or teacher fan-out scaling.
- M3 packetization drops malformed elementary-stream NAL units whose type is outside 1–23 rather than sending an undecodable RTP/H.264 payload. The occurrence was observed during the initial Mac loopback soak; the successful final soak confirms continued decode/render, but the VideoToolbox output anomaly should be characterized before treating this as a hardened production receiver.
- The M4 retransmission and reconnect paths are verified only on same-Mac loopback with a deterministic drop pattern. They have not been measured against genuine Ethernet loss/reordering, Windows Firewall behavior, or high fan-out.
- The existing latest-frame slot prevents capture backlog in the current single-threaded Teacher loop, but induced encoder/network overload has not yet been separately benchmarked; M5 scale tests must retain the freshness counters and add this pressure evidence.
- Five simultaneous VideoToolbox decoders on the same Mac produced repeated decoder status `-12909` and only 12--15 decoded frames per 10-second receiver run, despite zero RTP loss. This confirms the fan-out/control structure but fails receiver-health acceptance; separate Windows PCs are required for meaningful scale evidence.
- The new report script and process sampling have been built for Windows but have not been run there. Teacher-side encode p95 in the report covers the full session, including time before and after the requested viewer count; interpret it alongside the per-interval scale metrics.
- The focused Stage 2 late-join run encountered one VideoToolbox format-description error (`-12712`) but recovered and reached about 29 decode FPS in later intervals. This macOS codec issue remains open for multi-client testing.

## Exact next step

- Continue on Mac with the native benchmark runner; investigate the saved 5-Student H.264/VideoToolbox failures before accepting receiver health. Keep Windows image testing deferred until physical machines are available, as requested by the owner.
- Then repeat M4 loss/restart tests and the formal 1/5/10/20/40 scale ladder on separate wired Windows PCs; do not infer Windows or 40-client success from the Mac localhost run.
- When a real Windows teacher PC is available, also run `Classroom.exe --local-media-benchmark-seconds 60 --display-index 0` and record the Media Foundation encoder/decoder names, hardware/software selection, sustained FPS, and p95 latency before declaring the Windows M2 gate complete.

## Stream identity across Teacher process restarts (2026-10-02)

- Implemented randomized per-process presentation-ID epochs with monotonically increasing IDs and overflow protection; both Flutter Teacher agent and legacy native Teacher UI use the shared generator. RTP SSRC continues to derive from the full `stream_id`.
- Verification: full project tests, strict Clippy, macOS release build, Windows x64 GNU release build, signed Flutter app build, and focused ID tests (2/2) passed. The Windows `Classroom.exe` in `/Users/mvmanh/Downloads` byte-matches the build artifact (SHA-256 `757d65a113b6243e8f4095d78233cf42021522823ac5f08eaf23175611b0ed25`). Evidence receipts and report: `checkpoints/evidence/streamid-process-epoch-2026-10-02/`.
- Runtime restart acceptance remains open. Local Teacher `.219` started; Student Mac `.254` ping and TCP-connect to port 47800 succeeded, but the Student application repeatedly failed registration with `No route to host (os error 65)` and did not appear in the Teacher roster. Student log is retained. Student process and local Teacher started for this attempt have been stopped.
- Windows Student `.155` could not be inspected/launched: SSH is reachable on port 22 but denied the previously supplied credentials. The owner confirms the previously provided Mac `.254` credentials still work; this is a separate host/account from Windows `.155`.
- Exact next step: instrument control registration phases (TCP connect, Hello write, Welcome read) to locate the Mac `.254` failure; verify/correct Windows `.155` SSH authentication; then rerun one cross-platform test leaving both Students alive across a Teacher restart and compare pre/post `stream_id` plus receiver resumption. Do not claim live restart acceptance until that completes.

### Follow-up: registration phase diagnostics (2026-10-02)

- Added logs inside `register_student_for_presentation` to separate TCP connect, Hello write, Welcome read/timeout and successful control handshake.
- Full tests/clippy passed after this change: classroom 65 passed/1 ignored; control transport 8 passed; protocol 1 passed. macOS and Windows x64 release builds passed; Flutter bundle rebuilt and strict signature verification passed.
- Latest Windows build is copied to `/Users/mvmanh/Downloads/Classroom.exe` and verified identical (SHA-256 `dbb6e26e088f258b1b46d9b9163b5a4002c71e50ebe2f9a8796003798cc4740b`). Latest Mac Student binary was deployed to `.254` at its existing path; old file is backed up remotely as `Classroom.pre-streamid-20261002`.
- Diagnostic result from `.254` to `.219:47800`: ping, `nc`, Python, Rust `std::net`, and standalone Tokio connect succeeded; Classroom Student fails at its Tokio TCP connect with `EHOSTUNREACH`, even when run from the old approved path. Thus this is specific to the Classroom process/socket setup or launch context, not general IP reachability or Tokio by itself. No live Student registration or stream ID was observed.
- Latest Flutter UI relaunch showed the Teacher console but did not expose TCP listener 47800, so it was not used for the headless Student check. Test Student processes were stopped; the most recent Flutter window may remain open without an active Teacher listener.
- Current exact next step: make the latest Flutter Teacher helper reliably start/listen, then run one updated Mac Student registration test and inspect the new phase-specific log; fix the cause if reproducible. After Mac registration works, repeat across a Teacher process restart and compare IDs. Windows `.155` remains untested because SSH authentication is unresolved; Mac and Windows credentials must not be conflated.
## 2026-10-02 — Observe Width/Fill/Full screen and latest-pointer input

- Reworked the independent Observe window into three display modes: Width keeps
  the source ratio, matches the viewport width and scrolls the full source
  vertically; Fill stretches to every viewport pixel; Full screen enters the
  native full-screen window and selects Fill automatically. Observe windows now
  have an 800x600 minimum size, while the main console keeps its existing size.
- Header/status chrome remains an overlay and auto-hides; the new mode and full
  screen state are included in the compact Observe telemetry line.
- Remote pointer movement is now latest-wins at the Teacher coordinator. High-rate
  move events replace the pending move instead of filling the 64-entry control
  queue behind old positions. The input receiver still bounds commands and keeps
  the three-second fail-open lease.
- Added geometry and zero-size startup regression coverage previously used for the
  initial-frame bug. Rust Classroom tests: 69 passed, 1 ignored. Flutter source
  was formatted, but this environment's Flutter SDK cache update is currently
  blocked by filesystem permission/approval quota, so a fresh Flutter analyze/test
  and bundle build must be rerun when that cache is writable.
- Runtime logs showed the existing Observe receiver reaching 1920x1080 with
  native texture updates; a separate Mac Student restart attempt hit an already
  running UDP 47810 process and was not treated as a new acceptance run.
- Added a macOS direct `CGEvent` pointer path for Observe movement, bypassing the
  20ms per-event pacing used by the generic Enigo movement method. Button/key
  injection remains on the bounded session adapter. Windows keeps physical
  virtual-desktop absolute injection.
- Windows x64 Rust binaries rebuilt successfully after the latest coordinator and
  pointer changes. Copying them to `/Users/mvmanh/Downloads/` was blocked by the
  current filesystem approval quota; the release files remain in
  `target/rustup-windows/x86_64-pc-windows-gnu/release/`.

## 2026-10-02 — Control pointer responsiveness follow-up

- Added Flutter-side 8ms pointer coalescing and retained Rust-side latest-wins
  replacement, preventing stale TCP mouse positions from accumulating during fast
  movement. macOS movement now posts direct `CGEvent` mouse events rather than
  Enigo's generic paced movement path; Windows continues to use absolute physical
  `SendInput` coordinates.
- Rust classroom tests passed (69 passed, 1 ignored); Windows x64 cross-build and
  macOS Rust release build completed. The target app bundle was restored after an
  interrupted Flutter packaging attempt and has a valid deep strict signature with
  the new Rust helper.
- Deployment to remote Mac `.254` and Windows `.155`, and a fresh Flutter GUI
  rebuild, were blocked by the environment's automatic approval quota for SSH,
  Flutter cache writes and external file operations. Existing remote binaries were
  not claimed as updated.

## 2026-10-02 — Stretch mode and pointer coordinate correction

- Renamed the aspect-preserving Width choice to Stretch and made it use
  `BoxFit.cover`, removing side bars by scaling/cropping the source. Pointer
  normalization now applies the exact cover crop transform, so a click or move
  maps to the same source pixel that is visible. Fill remains the full-viewport
  mode; Full screen still selects Fill.
- Mac and Windows Rust release builds completed and the new Mac helper was
  re-signed inside the target app. Remote deployment was blocked again by the
  automatic approval quota; manual SCP/SSH commands are provided in the handoff.

## 2026-10-02 — Observe attachment investigation and rebuild

- Fresh logs separated the failure cleanly: both reverse Observe receiver
  processes connected, decoded and presented frames, while the Flutter UI
  remained at native texture sequence 0. Screen sharing therefore remains
  healthy; this was a console surface/state handoff problem.
- Hardened the handoff in both layers. Flutter now starts Observe through an
  awaited command path, retries surface attachment after the acknowledgement,
  polls for a late surface, and records whether the IPC state contains the
  surface. The Rust agent publishes `observe` state immediately when opening a
  view instead of waiting for the periodic state tick.
- Rebuilt and signed `target/Classroom Console.app` at 18:52. Flutter's
  dedicated Observe tests run but contain stale expectations for the renamed
  Stretch default (they expect the old Width/scroll layout); the source
  analyzer reports no Dart issues. Runtime acceptance still requires opening
  this newly rebuilt bundle and checking that Observe starts without pressing
  Fit.

## 2026-10-02 — Observe surface attach race hardening

- The latest Observe logs proved the reverse receiver was registering,
  decoding and presenting frames, while the Flutter native texture remained at
  sequence 0. Added a periodic surface-attachment retry in the Observe window
  so state/receiver startup ordering no longer depends on a single listener
  callback or pressing Fit.
- Native macOS texture attachment now validates the returned texture ID,
  primes the mmap immediately, retries through the existing timer, and exposes
  connection diagnostics in native texture stats. Attach failures are written
  to the Observe UI log.
- At this checkpoint the Flutter SDK cache blocked a rebuild. The later
  Stretch rendering regression follow-up below supersedes that status.

## 2026-10-02 — Stretch rendering regression investigation

- The subsequent user run showed `phase=starting surface=true`, followed by
  `phase=live`, while native `sequence=0`; receiver logs independently
  confirmed decoded and presented frames. This disproved the callback race as
  the remaining cause. The regression began after the Stretch/Fill render
  change.
- Replaced the `FittedBox` wrapper around Flutter's native `Texture` with an
  explicitly sized texture inside a clipped positioned stack for cover mode.
  This keeps the cover crop and inverse pointer mapping while avoiding
  `FittedBox` around the native texture. Native diagnostics now report source
  sequence, texture notifications, copy attempts/failures, lock misses, mmap
  connection status and connection errors.
- Updated stale Observe widget expectations for Stretch as the default,
  removed obsolete Width-scroll assertions, and reduced toolbar horizontal
  inset to avoid overflow at the 800 px minimum width.
- Verification: Observe widget tests pass; Flutter analyze reports no issues;
  Rust release check passed; `target/Classroom Console.app` rebuilt and signed.
  Runtime confirmation on this latest bundle is pending; the added counters
  will distinguish a render-tree problem from an mmap/copy failure.

## 2026-10-03 — Interactive Observe pointer responsiveness

- User confirms Observe works after the Stretch texture fix. Investigation of
  delayed Control feedback found that hiding the Teacher OS pointer made its
  visible position depend on the entire reverse video path. Pointer-only activity
  could also select the ordinary static-slide 5 FPS policy.
- Control now shows the immediate local OS pointer. Reverse Observe uses a
  20 FPS idle target (capped by configured maximum), without changing class
  broadcast policy. The captured remote pointer is still part of the video;
  delayed remote-pointer imagery and menu updates remain subject to video latency.
- Flutter enables TCP_NODELAY and flushes pending motion before discrete input.
  Rust uses one bounded ordered queue, coalescing only adjacent moves. Room input
  likewise coalesces adjacent motion without crossing click/key barriers.
  Positioned button events apply the exact normalized position before the click,
  avoiding clicks at an older cursor position. Click injection time is logged.
- Verification: Rust app suite 69 passed/1 ignored; added ordered-motion
  regression passed separately; Flutter Observe tests 4 passed, including exact
  click coordinates; Flutter analyze clean. macOS Teacher/Student and Windows x64
  Teacher/Student release builds completed, signed Flutter bundle rebuilt.
- Deployed Students via graphical Mac LaunchAgent and Windows interactive task:
  Mac ~/Classroom-two-mac/bin/Classroom, Windows
  C:/Users/mvmanh/Classroom-two-mac/bin/Classroom.exe. Binary SHA256 was verified.
  Local Teacher is running with 192.168.50.206. Windows Classroom.exe also copied
  to /Users/mvmanh/Downloads/Classroom.exe.
- Live validation is incomplete: Windows Student registered and supplies
  thumbnails, but reverse Observe TCP timed out. Read-only firewall inspection
  found two explicit Block rules for the deployed Windows executable. Automatic
  approval review rejected changing those rules without specific user approval;
  approval requested for allowing only Teacher .206. Mac Student at .2 reports
  No route to host while SSH nc to Teacher TCP succeeds, consistent with app
  Local Network denial; asked owner to grant permission. No claim of measured
  end-to-end improvement until both blockers are resolved and live test repeated.
- Collected diagnostic logs and deployment hashes are in
  target/control-responsiveness. Exact next step: restart Mac Student after its
  permission grant, apply the narrowly scoped Windows rule after approval, then
  repeat Observe/Control movement and click checks on both Students.

## 2026-10-03 — Confirmed mouse-rate IPC backlog and edge-only chrome

- The previous change did not eliminate perceived lag. Measured the existing
  Teacher process with 300 harmless Observe input commands at 125 Hz. IPC's
  per-client one-in-flight request/ack path waited on the coordinator's 20 ms
  poll for each command: p50 1962 ms, p95 3719 ms, max 3900 ms. Coalescing later
  in the pipeline could not eliminate this upstream backlog.
- Coordinator now polls at 2 ms and drains up to 32 commands per bounded pass.
  The same probe on the rebuilt Teacher measured p50 2.56 ms, p95 7.71 ms,
  max 37.81 ms, with no seconds-long accumulation. Added a reproducible
  scripts/test-observe-ipc-latency.py regression probe (p95 gate 25 ms).
  These measurements are local command dispatch, not end-to-end latency.
- Added a session-owned capture cursor policy shared between Observe input and
  capture workers. Acknowledged Control disables source cursor composition so
  a lagging cursor in H.264 cannot compete visually with the immediate Teacher
  OS cursor. Observe, errors and lease expiry restore the Student cursor.
  DXGI skips separate cursor composition; ScreenCaptureKit updates its existing
  stream configuration on the capture worker, preserving format/dimensions/FPS.
  Ordinary Teacher broadcast still includes its cursor. Corrected Mac cursor
  composition telemetry and disabled-click telemetry. No input lock by default.
- Hidden header/footer now IgnorePointer outside their MouseRegion: invisible
  controls no longer trigger hover or intercept remote clicks. Activation bands
  are exactly 5 logical pixels. Tests verify 30 px does not reveal controls and
  the edge does. Also guard clicks in read-only Observe and update position on
  button-up before releasing a drag.
- Verification: Rust application 70 passed/1 ignored; capture/control core tests
  passed; full Flutter suite 8 passed; Flutter analyze clean; formatting passed.
  Rebuilt/signed macOS Teacher and Rust Students, rebuilt Windows x64 binaries,
  copied Windows Classroom.exe to Downloads, deployed and SHA-verified both
  Students, restarted local Teacher .206. Logs confirm real Windows Control
  cursor policy false then true; reverse decoded frames have no decode errors.
- Mac Student was restarted after owner granted Screen Recording/Local Network.
  Real reverse frames arrived, but Enigo reported missing input-simulation
  permission. Accessibility grant for the deployed Mac binary is requested;
  Mac input/cursor-mode runtime validation remains pending that OS permission.
- Evidence and logs: target/control-responsiveness (before/after IPC aggregates,
  deployment hashes, Windows/Mac live state, remote logs). No claim of optical
  latency, perfect pointer shape synchronization, or physical Windows scaling.
  Next step: restart permission-granted Mac Student and repeat cooperative
  Control, verify cursor restoration and no capture errors; retain evidence.

## 2026-10-03 — Stable Mac identities, Observe startup and revoked-certificate blocker

- Added stable Apple-signed Teacher and Rust-only Student app build scripts.
  Bundle identifiers and default designated requirements remain stable; no
  ad-hoc fallback or weakened custom requirements. Embedded Teacher agent uses
  the same signing team. Student is an LSUIElement bundle, not a Flutter app.
  Default bundled Rust logs now go to ~/Library/Logs/Classroom outside signed
  resources; explicit --log-file and raw Windows logging remain supported.
- Observe starts in Fill at the Teacher monitor's complete logical work area,
  centered and excluding menu bar/Dock, without entering fullscreen. Set bounds
  before showing; fall back to native maximize. Retina/multiple-display tests
  cover logical coordinates and a negative-coordinate secondary display.
- Verification before the blocker: full Flutter suite 9 passed, analyze clean;
  Rust compact-log suite 7 passed, formatting passed. Both Mac bundles built;
  integrity verification and a two-version Student designated-requirement
  compatibility check passed. Those checks do not prove OS trust or retained
  permissions in a real update.
- Launching Teacher triggered "Malware Blocked and Moved to Trash". Gatekeeper
  and security verify-cert independently returned CSSMERR_TP_CERT_REVOKED for
  certificate 95B6A4A210BB43FAC1ABAD294306762C2BCAAF34. Initial find-identity
  had listed it without a revocation error; relying on that and integrity
  verification was insufficient. Current identity listing rejects it too.
  No conclusion about malware absence is inferred from this certificate error.
- Remote Mac Student using the same certificate was stopped with LaunchAgent
  bootout; verified STUDENT_JOB_STOPPED. Teacher deliverable was moved to Trash
  by macOS. An automatic review rejected a command that would have restarted
  revoked Student; only the separate stop command was subsequently executed.
  No quarantine/trust/Gatekeeper settings were changed and no blocked app was
  relaunched after identifying revocation.
- Fixed build preflight to require positive OS certificate revocation trust
  before building/signing. Post-sign and pre-deployment require Gatekeeper
  acceptance as well as signature integrity. Remote deployment also assesses
  before launching. Four safety regression tests passed: revoked identity
  cannot reach signing, integrity cannot override revocation, Gatekeeper
  rejection blocks deployment, and revocation timeout fails closed. Live
  verification rejects the existing Student and all eligible signing identities.
- Status: source/UI changes implemented; runnable signed delivery and real
  update/Observe permission validation blocked by invalid signing credentials.
  No Git repository available, so no checkpoint commit.
- Exact next step: owner obtains a valid Apple signing certificate through
  Xcode account management. Recheck trust and Gatekeeper acceptance; development
  signing alone may still require proper distribution signing/notarization.
  Then rebuild both Mac apps, ship Student to its stable app path and validate
  permission continuity across two builds plus maximized Observe startup.

## 2026-10-03 — Replacement development certificate verified; distribution gate remains

- Owner created "Mai's MacBook Pro" in Xcode. Keychain signing identity is
  Apple Development: mvmanh@gmail.com (U4RRV2T2BQ), fingerprint
  A9F06E151E345F70B12858DF2BC32945BFF60945. Positive OS code-signing certificate
  trust/revocation verification succeeds; expiry is 2027-10-03. Three older
  identities remain revoked and are excluded from selection.
- Rebuilt Mac Teacher and Rust Student and signed with the new identity outside
  the sandbox. Initial sandbox Student signing failed errSecInternalComponent;
  the approved outside-sandbox attempt succeeded without Keychain policy edits.
  Both complete app signatures verify; app and Teacher agent designated
  requirements match the previous manifests and Team JM63BUYLH3.
- Gatekeeper assessments still fail (initially "rejected", subsequently
  "internal error in Code Signing subsystem"). A positive certificate check
  does not prove app distribution acceptance. The exact internal-error cause
  is not established; Apple Development alone does not provide Developer ID
  notarized distribution. No current replacement app was launched or deployed.
  The old Teacher deliverable remains absent; Student staging is newly signed,
  while the previous exported Student remains blocked and is not a deliverable.
  Evidence: target/signing-verification/new-certificate-check.json.
- Prepared Developer ID signing path with secure timestamp and hardened runtime,
  opt-in notarytool submission using a configured Keychain profile, accepted
  ticket stapling/validation, followed by existing Gatekeeper gate. No upload
  performed. No credentials/private keys written to the repository. Signing
  requirements must still pass migration review before notarization.
- Verification: seven signing safety tests passed; Python compilation and shell
  syntax checks pass. Live certificate check passes; replacement Student app
  verify-only correctly fails at Gatekeeper. Previous Flutter 9-test/UI geometry
  and Rust 7-test/log verification remains applicable (those sources unchanged).
- Next step requires owner input: determine whether Developer ID Application
  certificate is available in Xcode and configure a notarization Keychain profile
  if using that distribution route. Then review signing identity migration,
  complete valid distribution signing, rebuild/deploy and test actual Observe
  work-area geometry and permissions across updates. Do not bypass the current
  Gatekeeper failure or claim retained permissions were runtime-validated.

## 2026-10-03 — Owner defers certificate continuity; Teacher-only mouse button fix

- Owner accepts manual privacy approval and defers stable certificate work.
  Teacher build now defaults to explicit ad-hoc development mode; optional
  Apple/distribution mode remains opt-in. No Keychain certificate deletion,
  quarantine removal or Gatekeeper setting change. The freshly built local
  Teacher opens normally and Windows .94 registers with its existing Student.
  Mac Student may remain stopped. Future Teacher-only UI changes do not require
  Student builds, copying or restarting; protocol/Student changes still do.
- Local 125 Hz IPC probe on the running Teacher: 300 commands, p50 2.47 ms,
  p95 6.44 ms, max 14.84 ms. This is command dispatch, not network/optical
  latency. Owner confirms actual Windows Control/mouse motion is now acceptable.
- Owner reports intermittent context menus after left clicks. Found a concrete
  Teacher UI bug: every PointerUp sent left/right/middle button-up, including
  buttons never pressed. An unsolicited right-up can trigger Windows context
  menu handling. Flutter now tracks held button bits and sends only actual
  transitions, including chord changes carried by PointerMove. Release/cancel
  only releases held buttons; mode changes release before switching, and
  disconnect/read-only state clears the local mask. Student protocol unchanged.
- Regression proved failure before the fix (left click generated right/middle
  releases). After fix, left click emits left down/up only, genuine right click
  emits right down/up only, and a primary/secondary chord preserves independent
  transitions. Fixed test mouse device fixture overlap; full Flutter 9 tests
  pass, analyze clean. Seven existing signing safety tests also pass.
- Rebuilt only Mac Teacher; target/Classroom Console.app executable timestamp
  10:04:08 local. Artifact strict signature verification passed. Runtime GUI
  restart closes existing Observe via authenticated IPC before replacing just
  the Teacher Flutter processes. No Student code changes/builds/deployments or
  restarts in this milestone. No Git repo available.
- Evidence: target/control-responsiveness/{left-click-before.txt,
  left-click-after.txt,teacher-left-click-build.txt,ipc-current.json}.
  Native UI automation timed out, so no claim of automated Windows visual
  context-menu verification; owner should check left-click/menu behavior using
  the rebuilt Teacher. Screen Recording approval may be required for the new UI.
- First open kept an older frontend registered at the Flutter build-product
  path. Restart was corrected to include that exact owned frontend and use
  open -n. Verified a new frontend start at 10:07:08 and successful IPC attach;
  Windows .94 remains online. The running build-product native executable and
  Flutter AOT are byte-identical to the exported target app (hash evidence in
  teacher-ui-artifact-check.json). Teacher agent may reattach/restart during GUI
  relaunch; no commands restart or replace the Student process. This completes
  delivery of the actual fixed UI rather than merely opening an old instance.

## 2026-10-03 — Owner confirms Windows click fix; Mac Student relaunched

- Owner manually validates the Teacher mouse-button fix as satisfactory.
- At owner's request, reused current target/release/ClassroomStudent (09:33)
  without Rust source edits or rebuilding. Packaged a fresh explicit ad-hoc
  development bundle to replace the remote old certificate-signed artifact.
  No certificate deletion, quarantine removal or OS security setting changes.
- Deployed only to mvmanh@macstudio at
  /Users/mvmanh/Classroom-two-mac/Classroom Student.app. Local/remote SHA256
  matched; strict signature integrity passed, Signature=adhoc verified.
  Started com.classroom.dev.student in gui/501, PID 13009. Windows untouched.
- Remote log confirms 192.168.50.2 registered to Teacher 192.168.50.206,
  Welcome received and room connection ready client_id=2. Initial network
  denial cleared on retry; capture still reports TCC permission unavailable.
  Observe/Control on Mac is not yet validated with this artifact.
- Evidence: target/mac-student-development/launch-result.txt and signing.json.
  Next: owner enables Screen Recording for this Student bundle (Accessibility
  also required for Control), then restart the owned Student job and test.
- After owner granted permissions, first restart hit Address already in use:
  another manually opened copy of the same Student bundle (PID 13082) owned
  UDP 47810. At owner's request stopped Student copies and relaunched only
  the owned GUI LaunchAgent. PID 13366 registers successfully (client_id=3);
  fresh log shows successive successful thumbnail captures, confirming Screen
  Recording now works. Windows and Teacher untouched. Evidence overwritten at
  target/mac-student-development/restart-result.txt. Ready for manual Control.

## 2026-10-03 — Observe title, trackpad scroll and live input geometry

- Owner confirms cooperative Control now feels acceptable; requests three
  correctness fixes, preserving Student builds for Teacher-only changes.
- Flutter native title now follows acknowledged Observe/Control (including
  exclusive Control), returns to Observe for read-only/ended/disconnected state.
  Pending mode requests do not falsely label an unacknowledged Control session.
- Found scroll UI handled only PointerScrollEvent. Flutter desktop trackpads
  also generate PointerPanZoom events; small wheel deltas were rounded to zero
  by Student's integral line injector. Handle trackpad pan with inverse content
  direction, accumulate fractional lines, clamp emitted events to existing
  protocol limits, position the remote pointer before wheel input, and never
  forward scrolling in Observe. Wire protocol and OS scroll adapters unchanged.
- Found Student Controller cached display bounds only at Control entry. Store
  the exact selected capture display's stable OS identity; query live bounds
  before each move and positioned click via CGDisplayBounds or DPI-aware
  GetMonitorInfoW. Expensive SCK/DXGI enumeration occurs only at Control entry,
  preserving the existing fast motion path. Log changed bounds only; no input
  coordinates or key contents in product logs. Existing error path releases
  Control if the selected display becomes unavailable.
- Split Control enabling/bounds refresh into small helpers; no changes to
  capture/codec/media scheduling or class fan-out. Student changes are required
  for the resolution fix, so rebuilt/deployed Mac and Windows Students once
  with final artifacts, byte-verified. Fresh Mac Student uses explicit local
  development ad-hoc signing; build/deploy scripts now support the same explicit
  mode as Teacher, Apple trust gates remain opt-in without silent fallback.
- Deployment now stops the GUI LaunchAgent and manually opened Student copies
  in its owned install tree before replacing the bundle. Owner explicitly
  authorizes stopping old Classroom processes when they occupy the ports.
  No Keychain deletion, quarantine stripping, or OS security setting changes.
- Verification: 9 Flutter tests passed (title acknowledgment/reversion, wheel
  fractions, trackpad pan, Observe suppression and crop after frame-shape change);
  Flutter analyze clean; 71 Rust tests passed, 1 existing ignored; strict Clippy
  passed on macOS and Windows GNU for affected packages (vendor warnings remain).
  Seven signing safety tests passed; format/shell/Python syntax checks passed.
  Built Teacher bundle, Mac Student bundle, Windows Classroom/Student releases;
  copied Classroom.exe to /Users/mvmanh/Downloads/Classroom.exe.
- Synthetic Windows GUI probe was inconclusive: initial harness hit process-local
  script execution policy/CLIXML parsing, then concurrent manual input/session
  changes interfered with cursor sampling. Do not count these as coordinate or
  scroll runtime passes. Diagnostic harness handles the policy in its own
  PowerShell process only; it does not change system policy. Temporary test task
  removed and original display mode restoration attempted in its finally block.
- Owner subsequently explicitly reports manual Windows resolution changes above
  AND below 1920x1080 work without discovered click errors. Collected Windows
  input log independently shows live geometry updated to 2560x1440. This is
  manual/runtime evidence, not formal optical latency or physical lab acceptance.
- New Mac package needed manual privacy approval. After owner grants, stopped
  duplicates and restarted owned GUI Student (PID15894); registration and fresh
  successful thumbnail captures confirm Screen Recording now works. No further
  Student build/redeploy after that grant. Mac live scroll/coordinate visual
  acceptance remains manual; OS scrolling on both peers is not claimed as
  automatically validated by the interrupted synthetic probe.
- Evidence: target/control-responsiveness/{observe-input-tests.txt,
  observe-input-analyze.txt,input-rust-tests.txt,input-clippy.txt,
  input-windows-clippy.txt,input-signing-tests.txt,input-*-build.txt,
  deployment.json,windows-input-probe-diagnostic.txt,input-live-results.json};
  target/mac-student-development/restart-result.txt. No Git repo to commit.
- Delivered running Teacher and both background Students for owner checks.
  Next: confirm wheel/trackpad up/down and title transitions in normal Control;
  continue with the existing completed mouse-button/motion baseline.
- Final owner feedback: manual Mac Student Control also works without discovered
  errors after the permission restart. Both Windows and Mac now have owner
  acceptance of normal Control with the delivered fixes. End the synthetic
  input probing for this milestone; leave the running applications available.

## 2026-10-03 — Teacher Finder reopen and Observe/Control session stats

- Investigated the Dock-icon/no-GUI report with PID-specific native window
  checks. A raw Rust helper under the Teacher bundle was registered as that
  parent app; Finder reopen activated the helper (no GUI), leaving Flutter hidden
  or absent. Separate nested `Classroom Agent.app`, its own identifier and
  LSUIElement fix this identity collision. Strict inside-out signing includes
  the nested helper. Agent resolution prefers it and retains legacy fallback.
- Teacher Flutter no longer terminates when its last window hides. Native
  reopen restores/deminiaturizes it. Observe process reopen requests the console
  through authenticated IPC; separate console counts also repair tray reopening
  when only Observe is attached. Background room ownership remains Rust.
- Added a default-hidden speedometer Stats toggle to the Observe/Control toolbar.
  Compact upper-right translucent panel, tabular digits, five metrics, mode from
  acknowledged state, real decoded resolution, preview texture FPS and received
  Mbps. Panel ignores pointer input and stays visible while chrome auto-hides.
  Compact icon tap targets prevent toolbar overflow at the 800px test viewport.
- Delay is corrected capture-to-receiver-surface interval median with measured
  clock uncertainty, not optical latency. Missing, stale (>3s), disconnected or
  unsynchronized measurements display a dash. Samples count only successful
  surface publications and unique source captures. Optional latest receiver
  metrics slot plus bounded StageMetrics, one JSON status/second over existing
  pipe/IPC. No per-frame Dart payload or log parsing; no Student protocol change.
- Verification: 10 Flutter tests pass, analyze clean; 71 Rust tests pass, one
  existing ignored; strict macOS Clippy passes (existing vendor warnings);
  Windows GNU check passes with rustup rustc; seven signing safety tests pass;
  formatting, shell syntax and exported deep/strict code signature verified.
  Initial Windows check used Homebrew rustc and lacked target std; corrected to
  installed rustup rustc with its Windows target. This is compile evidence, not
  a new Windows Teacher runtime test.
- Native runtime: cold Teacher visible; close-to-background then Finder reopen
  returns same Teacher PID/window; minimize then reopen is not minimized.
  Reopen with Observe alive also restores Teacher. Stopping only the frontend
  while Observe lives then Finder reopen recreates Teacher without ending Observe.
  macOS now reports separate Console and Agent bundle IDs; Agent stays accessory.
- Existing Windows .94 and Mac .2 Students both supply live reverse-session
  receive Mbps and non-null corrected delay samples to the new Teacher. Actual
  Windows preview capture confirms readable stats panel (15 FPS / 1.88 Mbps /
  approx80ms in that screenshot; short diagnostic snapshot, not benchmark).
  Missing/stale values, toggle/show-hide, native title/mode/input regressions and
  pointer-transparent overlay are covered by Flutter tests. No Student rebuild,
  deployment, remote synthetic input or privacy/trust setting changes.
- Evidence: target/control-responsiveness/teacher-reopen-runtime.json,
  teacher-reopen-observe-runtime.json, observe-stats-live.json,
  observe-mac-stats-live.json, observe-stats-panel.png,
  teacher-reopen-stats-build.txt and teacher-stats-{flutter-tests,analyze,
  rust-tests,clippy,windows-check,signing-tests}.txt.
- Delivered `target/Classroom Console.app` and left its Teacher GUI visible.
  Both existing Student agents remain running. No Git repo to commit.
  Next: owner uses Show stats in normal Observe/Control and reports any display
  issue; retain the owner-accepted input/scroll/resolution baseline.

## Observe stats, control recovery, and Grid thumbnail flush — 2026-10-03

- Mac Student Observe capture now starts at the actual display size, capped at
  1920x1080 with aspect ratio preserved and no upscaling. While observing, a
  1-second display-mode check ends the current source session on resolution
  change so the Teacher can renegotiate a fresh stream/encoder. Observe retries
  a terminal session at most twice per window.
- Control-mode requests now time out after 4 seconds and re-enable mode controls
  with a visible retry message if no acknowledgement arrives. Mode controls are
  gated by the live connection phase rather than requiring a newly decoded frame,
  so a stale preview alone cannot disable both choices.
- Grid thumbnail is flush with card edges; footer retains compact inner padding.
  Grid extent and Auto-fit math now use the full thumbnail width.
- Verified `cargo check -p capture-macos-sck`, Dart analysis (no issues), Flutter
  Teacher release build and Mac Student release app build; both bundles pass
  deep/strict codesign verification. No live Observe/Control session was run and
  no remote Student was copied/restarted; manual runtime confirmation remains.
- Artifacts: `target/Classroom Console.app`, `target/Classroom Student.app`.
  No Git repository is present for checkpoint commits.
- Next: manually Observe a Mac Student at its non-1080p mode, change its display
  mode once during Observe, check Stats renegotiation and verify Control buttons
  recover after a lost/no-response mode request; inspect Grid card edges.

## Refresh deployment for manual Observe check — 2026-10-03

- Installed and restarted the updated Rust Student on `mvmanh@macstudio` at
  `~/Classroom-two-mac/Classroom Student.app`; installed and restarted Windows
  Student `.94` at `C:/Users/mvmanh/Classroom-two-mac/bin/Classroom.exe`.
  Deployment script verified both remote file hashes and started the GUI-session
  Mac LaunchAgent plus Windows interactive scheduled task.
- Stopped the old local Teacher agent/console, opened `target/Classroom Console.app`
  and verified the new Teacher agent/console processes. Closed a restored stale
  Observe window while preserving the main console and both Students.
- Both Students registered with Teacher `192.168.50.206`. Windows DXGI thumbnail
  capture works. Mac Student registration works, but ScreenCaptureKit currently
  reports TCC capture permission declined; Mac Observe capture needs that
  permission re-enabled in Student System Settings before its manual check.
- Remote binary SHA256: Mac `2a90445cbaa791b3b418dfdc0ff7fbb47d085544231a6ef5f423c463693445ca`;
  Windows `3c95845e5a5d5bc03681b37c7ddf9290d88781e93a4dc278f62239cb40050839`.
  Deployment receipt/log copies: `target/control-responsiveness/deployment.json`
  and `target/control-responsiveness/{mac,windows}/`.

## Mac Student launch-state recovery — 2026-10-03

- Follow-up to manual handoff: remote `com.classroom.dev.student` LaunchAgent had
  exited (`state = not running`) after a prior `Address already in use` on UDP
  47810, explaining the Teacher's red/offline dot. Once the stale process was
  gone and the port was free, `launchctl kickstart -k` started one Student PID
  (21440) and bound UDP 47810.
- Fresh log confirms Student registration to Teacher `192.168.50.206`, and
  successive thumbnail frames were captured; Teacher reports `online=2` and
  receives Mac Student thumbnails. ScreenCaptureKit permission is currently
  working in this LaunchAgent session. Ready for manual Observe/Control check.

## VideoToolbox per-call encode telemetry — 2026-10-03

- Owner reports YouTube playback remains near 10 FPS on the M1 Ultra Mac Student,
  including after reducing the display mode to 1280x720. The Windows 11 Student
  previously compared is a Parallels VM on that same Mac, so its ~20 FPS is a
  different DXGI/Media Foundation path and not a native Windows PC baseline.
- Added bounded per-interval VideoToolbox stages for IOSurface input copy,
  synchronous encode/callback wait, every completed encode call (including a
  no-output return), successful sample returns, VideoToolbox's explicit dropped
  flag, missing samples without that flag, asynchronous callback flags, and
  successful output normalization. Existing interval logging reports sample
  counts and latency distributions, so submitted/output/drop rates can be
  compared directly without per-frame log spam or protocol changes.
- Updated `docs/12_TELEMETRY.md` with stage semantics. No encoding policy or
  capture scheduling behavior changed; this is instrumentation only.
- Verification: `cargo fmt --all -- --check`; strict Clippy for
  `codec-macos-vt`; release builds for `Classroom` and `ClassroomStudent`; CLI
  help smoke check; five deterministic codec tests pass. The existing hardware
  VideoToolbox round-trip test currently fails during session creation with
  OSStatus `-12903` on this run (it passed in an earlier run before the helper
  extraction); no assertion reached. This is recorded as a runtime test limit,
  not treated as a passing encoder test.
- No new YouTube/720p log was available in the local evidence directory, and
  the updated Student was not deployed/restarted on the remote M1 Ultra during
  this instrumentation change. The owner-reported ~10 FPS therefore remains
  unmeasured by the new fields.
- Exact next step: run the updated `ClassroomStudent` on the M1 Ultra Student
  while YouTube is playing, collect its one-second `media stage interval`
  records alongside the Teacher Observe stats, then determine whether VideoToolbox
  drop flags, no-sample callbacks, session wait, or capture cadence explains the
  output rate before changing encoder policy.

## Ship telemetry builds; leave all apps stopped — 2026-10-03

- Rebuilt the macOS `Classroom Student.app`, Windows x64 `Classroom.exe` and
  `ClassroomStudent.exe`, and the local Flutter `Classroom Console.app` with the
  new native telemetry. Windows build is PE32+ x86-64. The new Windows
  `Classroom.exe` was also copied to `/Users/mvmanh/Downloads/Classroom.exe`.
- Replaced the Mac Student bundle at
  `macstudio:~/Classroom-two-mac/Classroom Student.app` and Windows Student at
  `192.168.50.94:C:/Users/mvmanh/Classroom-two-mac/bin/Classroom.exe`. Retained
  each old installed artifact as `.previous-<timestamp>` for rollback. Remote
  hashes matched their locally built Student binaries.
- Stopped old Classroom Student processes on both remote machines; Mac Student
  LaunchAgent is unloaded and the Windows development task is stopped. Stopped
  the old local Teacher Console and Rust agent. The new Console is built at
  `target/Classroom Console.app` but was not opened; neither Student was started.
- Windows `Classroom.exe` hash (and Downloads copy):
  `17d706067fd4957ba8452eaec4ef2940efebd559f094dfcf845384a0e882ca71`.
  Mac Student executable hash: `1f73519c5f3789cf877edf2d8ccf1d3fbe0b28351f348c693746aefa5594c1d6`.
  Windows Student executable hash: `99b05efbb072214aa497876dd521481e8272d4050441c7314f6d95f86b4d8b4d`.
- Deep/strict codesign verification passed for both Mac bundles. This is build,
  copy and stop-state verification only; no app was launched and no media
  runtime test was performed.
- Exact next step: manually start Mac Student while YouTube is playing, then
  collect its `media stage interval` lines; start Teacher Console separately
  only when ready to use it. Windows Student remains stopped and its binary has
  no VideoToolbox-specific telemetry because this instrumentation is macOS-only.

## Restart Students with corrected Teacher IP — 2026-10-03

- Stopped the prior Student processes on Mac Studio and Windows Student, then
  restarted them through their interactive LaunchAgent / Scheduled Task sessions.
- Updated both persistent Student profiles to Teacher IP `192.168.50.206` using
  `--remember`; updated the Mac LaunchAgent and Windows task arguments to keep
  using that IP after future restarts or double-click launches.
- Mac Student: LaunchAgent `com.classroom.dev.student` is `running`; profile at
  `~/Library/Application Support/Classroom/profile.toml` now records the new IP.
  Its log shows TCP registration, Welcome, room connection ready, and thumbnail
  capture from local address `192.168.50.2`.
- Windows Student: task `Classroom-Dev-Student` is `Running`; profile at
  `%APPDATA%\\Classroom\\profile.toml` records the new IP. Its log shows TCP
  registration, Welcome, room connection ready, and DXGI thumbnail capture from
  `192.168.50.94`.
- Both Students successfully connected to Teacher `192.168.50.206:47800`.
  This verifies restart and registration; no screen-sharing or Observe session
  was started as part of this request.

## Mac Student Observe FPS diagnosis and capture/encoder fix — 2026-10-03

- Latest Mac Student Observe run on the M1 Ultra reports 26.84 capture FPS but
  9.72 encoded FPS over 20.27 seconds. Capture p95 is 2.02 ms, VideoToolbox
  encode wait p50 is about 10.0 ms, encode p95 is 15.58 ms, RTP missing packets
  are 0, and UDP fan-out is sub-millisecond in the sampled intervals. Windows
  Student Observe source reached 26.14 encode FPS in its latest log, with MFT
  output p95 about 13.9 ms and color conversion p95 about 6.1 ms. This localizes
  the Mac gap to its source-side capture/encode scheduling path, not network
  loss or packet fan-out.
- Mac logs identify Apple M1 Ultra but report `encoder_backend=unknown`. The
  previous VideoToolbox session specification only requested low-latency rate
  control; it did not explicitly enable hardware encode. Added explicit
  hardware-encoder enablement with compatibility fallback. Also moved the
  one-second `SCShareableContent::get()` display-resolution query out of the
  capture hot path into a stoppable background monitor, and added
  `capture_poll_wait` plus slow resolution-query telemetry.
- Verification: formatting, `cargo check` for the affected crates/app, strict
  Clippy for `codec-macos-vt`, `capture-macos-sck`, and `classroom-app`, and
  release build/signing for `Classroom Student.app` pass. Five pure codec tests
  pass. The real VideoToolbox round-trip test cannot initialize a session on
  this local run (`VTCompressionSessionCreate` OSStatus `-12903`), so no local
  encode-throughput claim is made.
- Deployed/restarted only Mac Student `.254`; its binary hash is
  `70eb62dd176890d019aba13fc48766434a8f6ce1e4566629038d3ed4c718d97a`. It
  registered to Teacher `192.168.50.206:47800`. Its new ad-hoc signature reset
  Screen Recording permission; the fresh log records TCC declining capture, so
  the new encoder/FPS path has not yet been exercised on the M1 Ultra.
- Exact next step: reapprove Screen Recording for the updated Classroom Student
  on `.254`, restart it in the GUI session, then run one Observe session and
  compare `capture_fps`, `encode_fps`, `encoder_backend`, `vt_encode_sample_returned`,
  and `capture_poll_wait` stage intervals.

## Mac Observe pacing root cause and scheduling fix — 2026-10-03

- Read the owner's fresh Mac session: 85.68 s, 21.24 capture FPS but only
  9.42 encode FPS. Encoder p50/p95 was 11.06/14.93 ms, no RTP loss or send
  errors, so encoder processing/network did not explain a roughly 100 ms cadence.
  Pre-sleep `teacher_tick_lateness` did not measure oversleep. The latest Windows
  source evidence is still the earlier 26.14 FPS run, not a fresh Windows benchmark.
- The development Mac LaunchAgent used the default Standard process type.
  Apple's local launchd.plist(5) documents resource throttling and timer coalescing
  for that default; Interactive plus LegacyTimers opts out. Changed only these
  job settings on the remote M1 Ultra, retaining the same binary and permissions.
  A bounded 21.49 s authenticated Observe probe achieved 28.57 capture FPS and
  22.06 encode FPS. Teacher receiver interval median decode was 21.98 FPS and
  native surface publication 20.86 FPS, with zero missing packets/decode errors.
  Mostly static content caused the existing adaptive target to move to 20 FPS
  at the end; configured bitrate stayed 6 Mbps rather than collapsing to 1.5 Mbps.
  Scene/activity and durations differ, so this is scheduling diagnosis evidence,
  not a controlled 30 FPS animation or glass-to-glass benchmark.
- Persisted Interactive/LegacyTimers in the deployment script. Added a scoped
  NSProcessInfo user-initiated/latency-critical activity to the shared publisher
  and receiver entry points, covering broadcast, Observe, manual launches and
  minimized windows. Drop ends the activity on normal completion/error; an idle
  connected agent holds no activity. Added `publisher_pacing_wait` and
  `publisher_pacing_overshoot` to expose actual sleep duration and wakeup lateness.
  No busy wait, unbounded queue, quality/resolution change or new media topology.
- Verification: cargo fmt check, cargo check and strict application Clippy pass
  (existing vendor minifb warnings remain). Mac Student and Flutter Teacher release
  builds/integrity checks pass; Windows x64 Student/Classroom cross-build passes
  using explicit rustup RUSTC (Homebrew rustc lacks Windows std despite rustup's
  installed target). Deployment verifies both Student hashes and restarts them;
  old Mac previous bundles removed. New Teacher opened and reports two online
  Students. Classroom.exe was also copied to /Users/mvmanh/Downloads/Classroom.exe.
- New Mac Student SHA256:
  573575ec6fec68611b0b05435b0dafab6228e15c6249c1c53c684d3d3c34ded6.
  Windows Student SHA256:
  4c68c0b689cbc3353ddd035f593a066647a768ab5e88d249b5509f578c1b3c78.
  The new ad-hoc Mac signature requires Screen Recording reapproval: registration
  works but the fresh thumbnail log records TCC declined. Therefore the measured
  improvement belongs to the same-binary LaunchAgent comparison; the added Rust
  activity guard is build-verified, not yet source-throughput validated remotely.
  Windows thumbnails work; no new Windows performance improvement is claimed.
- Evidence retained in checkpoints/evidence/2026-10-03-mac-observe-scheduling/
  (full source/receiver logs and comparison.json). Exact next step: approve Screen
  Recording for the updated remote Mac Student and restart its GUI LaunchAgent;
  one active-content Observe run can verify pacing overshoot and the new guard.

## Student Exit/Restart lifecycle and Teacher connection state — 2026-10-03

- Owner reported Student tray connected after Exit/Finder reopen while Teacher
  appeared disconnected, then confirmed connection recovered and FPS improved.
  Before making changes, remote process inspection showed one manually launched
  Student and the Teacher's authoritative IPC roster showed both Students online.
  The transient mismatch was not reproduced, so no single historical cause is
  asserted. Found concrete lifecycle/state defects: restart spawned a replacement
  before old runtime teardown; control reader/writer tasks had no last-owner
  cancellation; duplicate launches could truncate logs before failing a port bind;
  obsolete Flutter IPC callbacks could clear a newer connection.
- Added Student tray Restart Classroom (also available during presentation).
  Exit/Restart is pumped during pending registration; presentation Exit reaches
  the same teardown path. Monitoring/reverse publisher joins, media sockets,
  tray/native windows and Tokio runtime shut down before restart. Mac exec keeps
  the launchd PID/scheduling; Windows starts the replacement after teardown.
  Restart retains runtime/log/port arguments, uses the saved profile, and the tray
  saves the effective Teacher IP. Per-user/media-port lock prevents duplicate
  Student launches before compact-log initialization. No additional Student
  helper processes are introduced. Last StudentControl owner cancels both TCP
  tasks. Teacher ignores obsolete IPC chunks/disconnect/error callbacks.
- Deployment now remembers Teacher IP for Finder launches and prunes only its
  verified previous Mac bundle backups. Updated both Students and local Flutter
  Teacher, stopped old versions and reopened the new Teacher. Classroom.exe
  copied to /Users/mvmanh/Downloads/Classroom.exe. Authoritative IPC reports both
  Students online after deployment. Mac's fresh signature again requires Screen
  Recording reapproval (monitoring unavailable); Windows thumbnails are ready.
- Owner's fresh manually launched Mac Observe log validates the previous media
  activity fix: 42.09 s, encode 29.10 FPS, capture 21.69 FPS, 335 repeated frame
  submissions/1225 encoded frames, encode p95 13.55 ms, no loss/send failures.
  A later 5.64 s mostly static session encoded 24.29 FPS with 29.26 capture FPS.
  Pacing overshoot in sampled intervals is about 2–5 ms rather than the previous
  roughly 100 ms cadence. Encode cadence includes intentional repeats, not all
  new visual states; no optical latency or full Flutter display FPS claim.
- Verification: format/check/strict Clippy pass (existing minifb dependency
  warnings remain); three startup tests, one TCP lifetime/clone teardown test,
  and two Flutter IPC/roster tests pass. Mac Student, Windows x64 Student/Classroom
  and Flutter Teacher release builds/integrity checks pass. Remote duplicate
  Finder-equivalent executable launch is checked separately; actual tray click
  Exit/Restart during live capture is not automated in this run.
- Evidence: checkpoints/evidence/2026-10-03-student-lifecycle/ contains full
  manually launched Mac log and focused test outputs. Next step: reapprove the
  updated Mac Student capture permission, then try its new Restart Classroom
  menu during Observe; reopening should retain identity/IP and one process.

## Preserve Control across Observe-window blur/focus — 2026-10-03

- Root cause is explicit Flutter behavior: onWindowBlur called
  switchMode('observe') on every loss of foreground focus, including switching
  to another Teacher app to paste shared clipboard text. Both Student platforms
  therefore behaved identically; media, clipboard and Student input backends do
  not need changes.
- Blur now retains the acknowledged Observe/Control/exclusive mode and periodic
  keepalive. It releases tracked keys/buttons, clears scroll fractions and pauses
  input forwarding. Focus restores keyboard focus/input forwarding in the same
  mode. Mode switches still require explicit user action; closing/reopening still
  creates the existing default read-only Observe session. Genuine connection/
  lease failure retains the existing release behavior.
- Verification: five targeted Observe Flutter tests pass, including holding
  Shift/mouse during blur, checking releases and continued keepalive, suppressing
  movement while inactive, then moving/typing after focus without sending a new
  control/observe command. Flutter analyze reports no issues. Mac Flutter Teacher
  release build and bundle integrity checks pass. Only Teacher GUI is replaced/
  reopened; Rust agent and both Students remain running so Student capture
  permissions and connections are retained. No Student rebuild/deployment.
- Exact next step: manually repeat Student copy → Teacher paste → focus Control
  window; the same Control mode should remain and input should resume immediately.

## Teacher Restart and faster Observe startup — 2026-10-03

- Teacher tray now includes Restart Classroom. Flutter agent returns a restart
  request after stopping broadcast/Observe, listeners, monitoring and its IPC
  endpoint; main drops the runtime before same-arguments replacement. Teacher
  role, bind/ports, console executable and IPC path are retained. Authenticated
  restart IPC exercises the same command/teardown path. Runtime verification:
  new endpoint/nonce/listener ready in 329.9 ms, with the same Mac PID through
  exec; GUI and both Students reconnected. Legacy Rust dashboard also handles
  the newly visible tray Restart rather than ignoring it.
- Found avoidable startup waits: Teacher serviced Observe only after thumbnail
  intervals (2 s), Flutter checked first-frame stats every 1 s, and Student
  thumbnail acquisition/teardown blocked Observe start. Added targeted condition
  variable wakeup, immediate live-state publication and bounded 100 ms first-frame
  polling (returns to 1 s after readiness/error/10 s budget). Logs measure Observe
  queue delay, handshake, receiver start and first native frame detection.
- Teacher-only runtime changes reduced one Windows live-state wait from 2517 ms
  to 951 ms, but actual GUI probes still reached ~3 s in other phases of the
  thumbnail cycle. Therefore also updated Student: cached thumbnail reads use
  20 ms rather than waiting 1200 ms for a new static frame; one old thumbnail
  capture retires on a bounded, joined cleanup worker instead of delaying the
  Observe reply. Initial thumbnail acquisition keeps its 1200 ms budget.
  Full-resolution media/encoding, quality and protocol are unchanged. Retirement
  owns the Send capture and joins on exit; no detached cleanup or unbounded queue.
- Built/deployed both Students, stopped old apps and pruned owned backups.
  Windows Classroom.exe was copied to Downloads. The updated Mac initially
  needed TCC reapproval, then later Teacher logs show a successful Mac reverse
  session/first native frame. Both Students are online with ready thumbnails.
- GUI evidence: before Student changes, Windows first-frame event ~2995 ms from
  GUI process launch (2757 ms from widget creation); after changes it was 1546 ms
  from launch (1356 ms from widget creation). Corresponding request queue wait
  5.65 ms and source handshake 689.55 ms. A subsequent manually opened Mac window
  logged first native frame at 2165 ms from widget creation; later Windows opens
  varied (1537–2257 ms), with source handshake up to ~1.85 s. Thus startup is
  improved but native capture/encoder startup and previous-session teardown
  still vary. These are small integration samples, not optical latency or a
  controlled workload benchmark; no claim of instant startup for every open.
- Verification: five monitoring tests (including targeted wake/no lost wakeup/
  shutdown), five Flutter Observe tests (including first frame within 100 ms),
  format and strict application Clippy pass; Flutter analyze reports no issues.
  Mac Teacher/Student and Windows x64 Rust release builds/integrity checks pass.
  Actual Windows GUI Observe and both-platform registration verified. Optional
  final SSH full-log collection was not executed because automatic approval
  review timed out; local Teacher and Observe UI logs plus focused test/probe
  results were retained instead in
  checkpoints/evidence/2026-10-03-observe-startup/.
- Next step: owner can use Teacher Restart and open Observe normally. Further
  startup reduction would target native source initialization/rapid reopen
  teardown, using the new stage timings; preserve the validated live media path.


## 2026-10-03 — Windows Control pause and Mac Observe raster crash

- Collected current Windows Student log and retained both Mac crash reports.
  Crashes are in Flutter Metal external texture resolution on Teacher, including
  objc_retain/EXC_BAD_ACCESS; close/end events coincide. Windows logs contain no
  input lease expiry in the reported sessions. Exact historical input gating
  cannot be confirmed because original focus flags were not logged.
- Teacher-only fixes: one texture registration for engine lifetime, synchronized
  map rebinding, bounded four-buffer Core Video pool; remove Dart texture layer
  before detach/teardown. This avoids unsafe unregister/resource churn without
  editing Flutter SDK; its internal CVMetalTexture lifetime defect remains an
  upstream risk (flutter/flutter#157379).
- Input heartbeat now independent at 500 ms; stats timeout/recovery; reconcile
  actual native key-window focus with event state, preserve mode while blurred
  and release held input. Mode switches wait for first frame. Added focus/gating
  and pool pressure fields to existing compact UI diagnostics.
- Verified six Flutter tests and clean analyze, native Core Video retention/
  allocation-pressure/1500-frame/resize/pause test, Mac release build. Isolated
  Windows Observe first frame 2391.9 ms; scripted Control remained acknowledged
  30.3 s, GUI alive, no session errors, native ~17-20 FPS, pool_misses=0.
  Initial live-marker-only probe sent Control too early before receiver media
  registration; repeated only after first frame passed. Initial mixed old agent/
  rebuilt helper caused reverse TCP errno65 until Teacher restart.
- Students unchanged. Evidence and detailed limitations:
  checkpoints/evidence/2026-10-03-control-freeze/. No Git metadata available.
- Next: owner can verify actual GUI input/focus/fullscreen and normal close/reopen
  with rebuilt Teacher; short automated check is not proof against all rare
  crashes or Windows privilege boundaries.
- Final Teacher GUI restarted; UI visibly shows 2 online / 2 computers.
  Existing Rust agent stays running. Teacher Screen Recording consent banner
  reappears after ad-hoc GUI rebuild; owner must reapprove for teacher broadcast.
  Reverse Windows Observe/Control check succeeded without rebuilding Students.


## 2026-10-03 — P2-J Desktop delivery and screenshot follow-up

- Confirmed the expected destination policy: received files go to the Student's
  Desktop; screenshots go to the Teacher's Desktop. Rust resolves Windows Desktop
  with SHGetKnownFolderPath (including redirected folders), and macOS uses the
  user's Desktop. Screenshot names include Student and local timestamp.
- A live mixed-Student send exposed publication failures after transfer on the
  existing hard-link path (Windows error 87; macOS error 35). Replaced hard-link/
  rename publication with create-new, checksum-verified copying to the final
  Desktop filename. Existing filenames are preserved and a numeric suffix is
  selected; partial data is removed on errors. Added collision coverage.
- Windows Teacher screenshot handling now captures the displayed Observe image
  in Flutter and writes PNG bytes through a native Windows channel to the OS
  Desktop known folder. macOS keeps its native captured-frame and Desktop consent
  implementation. Shared Flutter tests: 12 passed; analyze clean. Rust tool tests:
  7 passed; Windows x64 Student release cross-build and macOS Teacher/Student
  release builds pass. Formatting passes.
- First live transfer test used a 1,260,000-byte smoke file; both receivers errored
  during Desktop publication. No user file was overwritten. A second end-to-end
  run could not be done: SSH authentication to both configured Student hosts
  rejected the supplied passwords. Updated binaries are local only; Students still
  run the earlier build. The Windows Flutter Runner C++ addition cannot be built
  on this macOS host, so it remains unverified on Windows.
- Next: restore SSH access, deploy the rebuilt Student binaries, retry file delivery
  to both Desktop folders and verify matching SHA-256 plus absence of partial files;
  then test timestamped screenshots on each Teacher OS. Windows Runner should be
  compiled on Windows before shipping a Windows Teacher build.


## 2026-10-03 — Workspace cleanup / GitHub state (recorded after the fact)

- Earlier today the workspace was cleaned (Rust/Flutter build output, caches,
  logs, `checkpoints/evidence/`, temp files). Evidence paths cited by earlier
  entries (`checkpoints/evidence/...`, `target/...`) no longer exist locally.
- A private GitHub repo exists: `https://github.com/maivanmanh/netsupport-school`
  with one commit `525a592a chore: initialize Classroom repository`.
- The local `.git` was found re-initialized (no commits, no remote). Re-linking
  (`git remote add origin …; git fetch; git reset --soft origin/main`) was
  blocked by the agent's permission policy and is left to the owner.

## 2026-10-03 — P2-K File delivery naming/dedupe, compact Student message, Teacher reply inbox

Owner-reported issues: Windows file delivery failed; Student message dialog too
large with an always-visible reply box; replies/failures re-appeared after
reopening the Teacher app; received filenames got prefixes.

- Root cause of Windows failure (from live Teacher IPC snapshot): `The parameter
  is incorrect. (os error 87)`. Windows Student Desktop is `C:\Mac\Home\Desktop`
  — a Parallels shared folder that is the Mac Student's (.2) own Desktop. The
  deployed (old) Windows build published with hard links, which that share
  rejects. Current code publishes with create_new + copy (no hard link/rename).
  Note: in this lab the Mac and Windows Students therefore share one Desktop.
- File naming (`classroom_tools.rs`): original filename kept (no `received-` or
  job-id prefix); only unsafe characters replaced and Windows device names
  (CON, COM1…) get `_`. Collisions become `name-1.ext`, `name-2.ext`, ….
- Dedupe: Teacher hashes the staged file (SHA-256, off the IPC thread; targets
  show `preparing`) and sends the hash in the offer. Student checks `name`,
  `name-1`, … before connecting; an existing file with same size+hash → new
  terminal status `skipped` (no transfer). SHA-256 is used instead of MD5 (same
  purpose, already in the protocol). If a folder peer publishes the same name
  during download, the Student waits for its size to settle and compares hashes
  instead of creating a duplicate.
- Downloads stage in `<temp>/Classroom-incoming/<job>.part`, not on the Desktop.
  Student touches the Desktop (`read_dir`) before downloading so a macOS
  first-use Desktop-access prompt or denial happens before the transfer.
- Teacher waits up to 120 s + 10 MiB/s for the Student's "stored" ack (was the
  4 s control timeout — large copies or the macOS prompt caused false errors).
- Student message window (`student_tools_ui.rs`): sized to the message
  (340–560 px wide, ≤12 lines then scroll), buttons OK / Reply. OK (or Enter/
  Esc) closes; Teacher only records `dismissed`, no notification. Reply reopens
  the window taller with a 3-line box and Send / Cancel (Esc collapses). Added
  `Canvas::text_width` for pixel-accurate word wrap.
- Teacher Flutter (`classroom_tools_ui.dart`): new `ToolActivity` turns agent
  snapshots into one-shot notifications. Everything already finished in the
  first snapshot a console sees is history (the Rust agent outlives the GUI),
  so nothing re-appears after reopening; nothing is persisted. Replies aggregate
  into one toast ("N new replies", latest 3 as `Name (IP): text`) plus a toolbar
  inbox button with unread badge; the inbox dialog groups replies by message
  with machine name · IP and time. File results announce once (success auto-
  hides after 5 s; failures keep Retry until dismissed). Same in Observe window.
  Agent snapshot now includes message `text` (≤240 chars).
- `scripts/deploy-responsive-control.py`: `--mac-host/--windows-host`, and
  passwords from `CLASSROOM_MAC_SSH_PASSWORD` / `CLASSROOM_WINDOWS_SSH_PASSWORD`
  for unattended deployment (still prompts if unset).

Verification:
- `cargo fmt --all -- --check`; `cargo clippy -p classroom-app -- -D warnings`
  clean (`--all-targets` still reports the pre-existing "items after test
  module" in `observe_coordinator.rs`). `cargo test -p classroom-app`: 86 passed,
  1 ignored, incl. new name/dedupe/numbering/peer-publish and dialog layout tests.
- `flutter analyze` clean; `flutter test` 16 passed (new tools_activity_test:
  no re-announce of history, reply aggregation with machine identity, one-shot
  file results, inbox dialog). Toolbar icons made compact (740 px test fit).
- Built Windows x64 (gnu, rustup rustc) Student/Classroom, Mac Student and
  Flutter Teacher (ad-hoc); deployed + started both Students via SSH; restarted
  local Teacher (.206).
- Live (first build, before the ack-timeout fix): Windows .94 received v1
  (`complete`), resend → `skipped`, changed v2 → `complete` as `NSS file test-1.txt`;
  shared Desktop contains exactly the two expected files with no duplicate.
  Mac .2 downloaded the full partial but blocked writing to ~/Desktop (new
  ad-hoc signature → macOS Desktop-access consent), Teacher timed out at 4 s →
  fixed as above, rebuilt and redeployed.

- Live after owner granted Mac permissions (final build, both Students):
  v1 → both `complete` (concurrent download into the shared Desktop produced a
  single file, peer-publish detection worked); resend v1 → both `skipped`;
  changed v2 → both `complete`, stored once as `NSS file test-1.txt`; 8 MiB
  random file → both `complete` in 9.5 s, single copy; staging dir empty.
  Test files removed from the Mac .2 Desktop afterwards. A test message
  (allow_reply) was sent to both Students for owner visual review.

Remaining / next:
- Owner visual check of the new Student message window (OK / Reply / Send) on
  Mac and Windows and the Teacher reply toast/inbox; adjust sizing if needed.
- Windows Flutter Teacher runner (screenshot channel) still uncompiled.

## 2026-10-03 — Mac first-click fix and keyboard focus in Rust windows

- Owner: OK on the Mac Student message window was unreliable (Windows fine).
  Cause: the click hits minifb's MTKView subview, which by default refuses the
  first click while the accessory (LSUIElement) app is inactive — that click
  only activated the app. Vendored minifb: `MFBMetalView` (MTKView subclass)
  and `OSXWindowFrameView` return YES from `acceptsFirstMouse:`; showing a
  window also calls `orderFrontRegardless` and `[NSApp activate]` on macOS 14+
  (`activateIgnoringOtherApps` is deprecated/ignored there).
- Owner request: Tab focus in Rust-drawn windows.
  - Message window: Tab / Shift+Tab cycle Reply → OK (default OK) or reply box
    → Cancel → Send (default box); Enter/Space activate the focused button,
    Enter in the box sends; focus ring + blue box border; click in box focuses it.
  - Start/Settings window (`launcher.rs`): Tab order Teacher → Student → IP →
    Start/Save (default Start/Save, so Enter confirms); Left/Right switch role;
    Enter/Space activate; focus rings via new `Canvas::focus_ring`.
- Verification: fmt, strict clippy (app bins), 87 Rust tests passed (new
  focus-order test). Built Windows x64 + Mac Students, deployed and restarted;
  sent a keyboard test message to both. Mac needs Screen Recording re-approval
  again (new ad-hoc signature). Visual/keyboard check pending owner.

## 2026-10-03 — Mac click offset fix, Student file window, enhanced 1-1 Observe profile

Owner reports/requests: Mac message OK/Reply unreliable (X fine); Student-side
file-transfer window; investigate better codecs/FPS for one-to-one Observe
(generalized: use any better option the hardware supports, else keep current);
show the codec in Observe stats and footer.

- Mac click root cause (vendored minifb): click coordinates used the child
  contentView, which is a title-bar height (~28 pt) shorter than the content
  area, so every click landed ~28 pt above the pointer; only the bottom few
  points of a button hit. Now measured against the full-content frame view (same
  space as hover/get_mouse_pos); synthetic test tap uses the same space.
- minifb additions: `outer_frame`/`set_outer_origin`/`work_area` (consistent
  top-left coordinates, both OSes) and `show_inactive` (no focus steal). Message
  window reopen (Reply/Cancel) keeps its exact top-left via these.
- Student file window (`student_file_ui.rs`, state in `Inbox::transfers`):
  bottom-right of the work area, native title bar (movable), collapse/expand
  keeping the bottom-right corner, newest 3 files with name + progress bar/%,
  then "Saved to Desktop"/"Already on Desktop"/failure; "Show" reveals the file
  in Finder (`open -R`) / Explorer (`/select,`) without opening it. Close and the
  OS close button are refused while any transfer is active (OS close collapses
  instead). Shown without stealing keyboard focus.
- Enhanced Observe profile: see docs/11_DECISION_LOG.md (capability-negotiated
  one-to-one media profile). New `stream_profile.rs`; codec-aware RTP and
  VideoToolbox (H.265 hardware round trip test passes on this Mac); config
  `[observe]`; proto ObserveMessage fields 10–12; receiver reports `codec` and
  `fps_cap` in metrics; Flutter Session stats "Codec" row and footer label.

Verification: cargo fmt; strict clippy (app bins); Rust tests 94 passed + new
media-transport HEVC RTP test + VT HEVC hardware round trip; Flutter analyze
clean, 18 tests (new codec label/panel test caught and fixed a 25 px overflow:
panel 206→236 px). Built Windows x64/Mac Student/Teacher, deployed, restarted.
Live: Teacher logs offer (["h265","h264"], 60); automated 20 s Observe of the
Windows Student negotiated `h264@30` (no Windows hardware probing) and the UI
metrics carried codec "H.264", fps_cap 30. Mac Student needs Screen Recording
re-approval before the H.265@60 Observe run.

Next: owner approves Mac Student Screen Recording; run Observe on Mac Student and
compare FPS/bitrate h265@60 vs previous h264@30; owner checks file window and
message buttons on Mac; Windows MF HEVC/hardware probing is the extension point.
- Live after owner granted Mac permissions: 6 MiB file → Windows `complete`, Mac
  `skipped` (shared Parallels Desktop already held the identical file — correct).
  Automated 25 s Observe of Mac Student .2 negotiated `h265@60` (Teacher log
  `profile=h265@60`; UI metrics codec "H.265", fps_cap 60). Teacher receiver
  intervals: decode 54.5–62.0 FPS, native surface publish 47.6–55.0 FPS, 0
  missing packets, 0 decode/depacketizer errors, receive 0.36–1.77 Mbps (mostly
  static desktop; cadence includes the publisher's intentional repeat frames).
  Earlier h264@30 Mac Observe reference: ~22 decode / ~21 surface FPS. Not a
  controlled motion benchmark; a YouTube/animation comparison is still open.

## 2026-10-03 — Class broadcast optimizations O1–O3 (docs/17)

Owner approved the no-H.265 optimization plan; recorded in
docs/17_BROADCAST_OPTIMIZATION_PLAN.md (analysis vs commercial products, work
items O1–O5, status table, implementation notes). Implemented in order:

- O1 separate pointer channel (RTP PT 100) with Student-side drawing; capture
  stops compositing only when all receivers support it; DXGI pointer-only
  updates skipped in overlay mode. New: `media_core::{PointerImage,
  PointerSample, PointerSource, PointerOverlay}`, `Renderer::{set_pointer,
  redraw}`, `media_transport::pointer`, `apps/classroom/src/pointer_channel.rs`,
  `capture-macos-sck/src/pointer.rs`, DXGI `to_image`, D3D11 pointer quad,
  minifb sprite; proto `HelloStudent.pointer_overlay` (6).
- O2 refinement (`refinement.rs`), config `video.refine_bitrate_percent` (250).
- O3 keyframe gate (`keyframe_gate.rs`), config
  `video.keyframe_request_coalesce_ms` (400); `keyframe_interval_ms` 2000→4000.
- Telemetry lines listed in docs/17 §5.

Verification so far: fmt; strict clippy (app bins, capture/media crates);
tests: app 99 (+pointer channel, refinement, keyframe gate), media-transport 18
(+pointer wire), capture-windows-dxgi 10 (+shape conversion), media-core 6
(+overlay math/blend), control-transport 9. Windows x64 / Mac Student / Teacher
built, Students deployed. Live class-share run blocked: new ad-hoc Teacher build
lost Screen Recording permission (TCC declined); Mac Student needs re-approval.

Next: owner grants Teacher + Mac Student Screen Recording; run class share and
check telemetry (pointer mode overlay, pointer_only_updates_skipped, refinement
counts, keyframe coalescing) plus visual pointer on both Students. O4 needs a
real-GPU Windows machine; O5 after the physical lab ladder.
- Live after owner granted permissions (owner had started a share from the UI;
  the automated script's final Stop ended that share): Teacher pointer mode
  overlay for 2/2 Students, ≤60 updates/s; Students 0 malformed, redraws 6–50/s,
  video ~28 FPS 0 loss; joins 3 IDR requests → 2 forced (1 coalesced); periodic
  IDR 4 s. Refinement fired once, VT live bitrate (no IDR) but only ~2.3 KB over
  3 frames → limited effect on macOS; follow-up in docs/17 §6.

## 2026-10-03 — Lab-scope optimization round (simulator, latency, loss recovery)

Details and numbers: docs/17_BROADCAST_OPTIMIZATION_PLAN.md §7.
- New `scripts/simulate-class.py` (+ `CLASSROOM_TEST_DROP_PERCENT` ingress loss
  emulator; Flutter roster ignores `sim-*`).
- Teacher: capture-aligned encode scheduling (`video.capture_aligned_encode`),
  VT latency properties, QP-capped refinement (`VideoEncoder::set_refinement`),
  adaptive keyframe-gate window; telemetry `teacher encode scheduling interval`,
  `keyframe_gate_window_ms`, `refine_qp_supported`.
- Student/transport: NACK retry per gap, RTT-adaptive repair budget
  (`JitterBuffer::set_repair_budget`), telemetry `nack_retries`, `repair_budget_ms`.
- Rejected with evidence: VT DataRateLimits (removed), jitter-hold reduction, FEC now.
- Verification: fmt; strict clippy; Rust app 100, media-transport 19, VT 7 (+1
  ignored measurement), media-core 6, DXGI 10, control 9; Flutter 18. Built and
  deployed Windows/Mac Students and Mac Teacher; 6 simulator runs with 10 virtual
  + 2 real Students over Wi-Fi (reports under target/sim-class/).
- Gotcha: closing the Teacher console leaves the old Rust agent running; after a
  Teacher rebuild send IPC `quit` (scripts/test-flutter-ipc.py --action quit)
  before reopening, otherwise the new console attaches to the old agent.
- Next: error concealment / LTR for random-loss freezes; O4 on a real-GPU Windows
  PC; wired-LAN and physical-lab runs of simulate-class.py.


## 2026-10-03 — Git re-linked and session committed

- Local repo re-attached to `origin/main` (GitHub private repo) with
  `git reset --soft origin/main`; working tree unchanged.
- Committed the whole session as `baeef69` (feat: classroom tools, enhanced
  Observe, broadcast optimizations O1-O3) and pushed. Scanned for leaked SSH
  passwords before committing (none). Build output/logs stay ignored.
- From now on: commit + push after each verified milestone (AGENTS.md §12).

## 2026-10-04 — Wired scaling run: 10 / 20 / 40 simulated Students

- Teacher MacBook `192.168.10.100` ↔ Mac Studio `192.168.10.101` over
  Thunderbolt Bridge. Results table and findings: docs/17 §8.
- 40/40 registered; 0 missing packets, 0 retransmissions; Student decode p50
  28 FPS (p95 32); Teacher encode p95 ~22–30 ms independent of N; send p50 40 /
  max 272 Mbps; 47 join IDR requests coalesced to 3 forced IDRs.
- Fixed `scripts/simulate-class.py`: keep draining IPC during the share window
  (select-based; the first 40-Student run lost its IPC socket and never sent Stop).
- Gotcha: stale macOS Local Network grant after a Student rebuild → `No route
  to host`; owner toggled the permission and relaunched the app via GUI.
- Remaining risks: link is Thunderbolt not 1 GbE; all virtual Students share
  one host; Windows Students not in this run.
- Next: same ladder on the physical lab (1 GbE switch, real Windows PCs), and
  error concealment / LTR for random-loss freezes (docs/17 §7 open item).

## 2026-10-04 — RTP fan-out moved off the encode thread

- New `apps/classroom/src/fanout.rs`: 4 worker threads (`network.fanout_threads`)
  with one-AU bounded queues; per-Student connected UDP sockets on the shared
  media port (`network.connected_fanout`, reuse-port/addr). Teacher-only; no
  protocol or Student change, Students not redeployed. Details: docs/17 §9.
- Verified: fmt; strict clippy for app/config bins on macOS and
  x86_64-pc-windows-gnu (`--no-deps`; pre-existing lints remain in
  renderer-windows `too_many_lines` and app test target `items_after_test_module`);
  tests 102 + 3 new fan-out tests on macOS; fan-out tests also on Windows 11 VM
  (first run caught the missing Windows `SO_REUSEADDR`, fixed).
- Wired re-run 10/20/40: encode-thread send p95 17 ms → 0.1 ms at 40; encode FPS
  27.5 → 31.3; Student decode 28 → 31 FPS; replaced frames 135 → 34; 0 loss.
- Harness: `simulate-class.py --student-interface bridge0` pins the Students
  when the default route is Wi-Fi; Teacher console `bind_ip` set to
  192.168.10.100 for the run, then restored to `auto`.
- Next: physical lab ladder with a Windows Teacher (1 GbE, 40 PCs); error
  concealment / LTR for random-loss freezes.

## 2026-10-04 — Windows Flutter Teacher build and same-host smoke

Status: Windows VM integration verified; physical LAN acceptance remains pending.

- Built ARM64 Rust diagnostics and delivered x64 MSVC Teacher/Student with Flutter
  3.47.6 on Windows 11 Pro Parallels (4 CPUs, 8 GB, 1920x1080).
- Fixed MSVC screenshot filename conversion; added missing Windows Observe native
  texture/channel with 3 bounded reusable buffers and nonblocking map locks;
  converted MF NV12 at the Observe sink; fixed refresh timer dispatch/ID.
- Fixed protocol round-trip fixture and platform/real-clock assumptions in Flutter
  tests. Packager resolves relative helper paths and includes the Student binary.
- Verified actual console-spawned agent, Student registration/thumbnail, two
  Share/Stop cycles, Student identity-preserving reconnect, continuous Observe.
  Final native sequence samples 37 -> 135, no Observe error. Test processes stopped.
- 20 s x64 broadcast: render ~28.26 FPS, encode p95 ~19–20 ms, one encoder,
  zero decode errors/final missing packets/send errors; two NACK sequences repaired.
- Passed Rust workspace lib/bin tests, Flutter 18 tests/analyze, formatting;
  app-bin Clippy passes with -D warnings -A deprecated (pre-existing fetch_update
  warning on Rust 1.99; retain Rust 1.82 compatibility). No firewall script changes.
- Commands and full limits: diagnostics/2026-10-04-windows-flutter-teacher.md.
  Evidence: target/windows-flutter-smoke/ (failed attempts retained; final 124228).
  Bundle: target/Classroom-Console-Windows-x64/classroom_console.exe.
- Limits: VM, x64 emulation, same-screen feedback; no optical/40-PC acceptance.
  Some captures show foreground terminal, so readability is not claimed. Control
  input/screenshot not exercised; Windows file picker still unimplemented.
- Exact next step: one separate physical Windows Student + controlled text/scroll
  and visual cursor/Observe check, optical latency, then wired 1/5/10/20/40 ladder.

## 2026-10-04 — Windows Teacher with Mac and ARM Windows Students over Wi-Fi

Status: integration exercised; ARM/Wi-Fi smoothness and recovery fail acceptance.

- Built/deployed Mac Student to the exact announced app path under
  /Users/mvmanh/Classroom-windows-teacher-test; native ARM Student to Windows .205.
  Mac .206 hosts that Windows VM (confirmed by prlctl hostname): two Student OS
  endpoints share CPU/NIC. Teacher .94 remains x64 Flutter/Rust on Parallels ARM.
- Fixed Windows tray popup blocking coordinator/IPC/Observe leases by moving
  Teacher tray native ownership to its own bounded worker. Actual open-menu
  Stop/Share Selected, continuous Observe/rescan and graceful Quit verified.
- Fresh ARM Windows exposed missing VCRUNTIME140.dll (0xC0000135). App-local
  matching CRT fixed launch; both Student architectures and Flutter packager now
  include VC runtime DLLs. PE architecture gates reject mismatched runtime/helper.
- One Mac broadcast: render 20.75 FPS, encode p95 19.14 ms, zero missing/decode/
  depacketizer errors. Two Students still use one encoder; source send errors zero.
- ARM default: render 7.00 FPS, 119 missing packets, 56 depacketizer errors,
  95 keyframe requests, 7.40 s worst recovery. Opt-in Wi-Fi 50/100 ms profile:
  render 10.65 FPS, 56 missing, 28 depacketizer errors, 4.64 s recovery. Different
  run durations/load prevent a causal claim; this profile does not resolve failure.
- Mac receiver had zero missing/decode/depacketizer errors in both two-peer runs.
  Actual Flutter Observe updated from both platforms; Mac Control acknowledged
  with native input permission. Reconnect retained IDs; Share/Stop repeated.
- Passed app tests (102 + 1 ignored), config tests (5), fmt, Clippy with existing
  deprecated warning allowance, PowerShell syntax, packaging and native scenarios.
  All owned test processes/tasks stopped; artifacts and private logs retained.
- Details: diagnostics/2026-10-04-windows-teacher-wifi.md; evidence under
  target/windows-mac-wifi/. Do not publish IPC endpoint tokens.
- Exact next step: isolated physical x64 Windows Student + wired Ethernet with
  controlled workload; compare wired/Wi-Fi ARM ingress/recovery, then headless/
  pacing probes if loss persists. Multi-second ARM recovery remains a blocker to
  smoothness acceptance; physical 40-PC and optical latency gates remain pending.

## 2026-10-04 — Complete Exit, role settings, pointer and diagnostic ZIP collection

Status: implementation and Windows/LAN integration verified; macOS native menu click and wired scaling remain pending.

- Ordered Teacher lifecycle closes Flutter console/Observe and joins media/Observe children before publishing Exit. Observe cannot resurrect Teacher. Atomic endpoint publication prevents partial startup descriptors. Windows tray Exit with live Observe removed console, coordinator, Observe UI and receiver child; both apps restart from executable launches without CLI flags.
- Added role-fixed Student settings and Teacher sidebar configuration; validated/persisted network, presentation, Observe and log options. Actual Save/restart and saved Wi-Fi 50/100 ms profile verified. Local presentation pointers hide on Windows/macOS and restore after Stop; owner confirmed Mac, native Windows cursor flags confirmed 0 -> 1.
- Corrected IDR repair versus freshness expiry while preserving bounded latest-frame queues. ARM/Wi-Fi still shows loss and multi-second recovery; no performance acceptance claim.
- Implemented missing Windows native Send file picker/staging and visible error reporting; actual class transfers verified, with SHA-256 and identical-file skip. Parallels shares Mac/Windows Student Desktop, so file observations require receiver-log evidence.
- Added Teacher Diagnostics ZIP collection, selected/class request, optional auto after Stop, separate paced upload (512 KiB/s, one Student, bounded 2 MiB tails), explicit missing recipients and privacy filtering. Found/fixed asynchronous Stop race by retaining one Student request until presentation cleanup finishes. Final automatic ZIP collected both Students; read-back privacy and metadata checks passed.
- Normalized macOS entitlement XML before signing to resolve Windows CRLF/AMFI rejection without changing entitlements. Both signed macOS apps rebuilt. Mac Teacher graceful IPC Exit/no-argument reopening verified; physical macOS status-menu Exit not independently verified because SSH GUI automation is denied Accessibility.
- App tests 105 passed + 1 ignored; config 5; transport 20; fmt/Clippy, Flutter analyze + 21 tests and Windows x64/ARM + macOS builds passed. Test automation cleaned up; diagnostic archives retained privately on Teacher.
- Details: diagnostics/2026-10-04-lifecycle-settings-diagnostics.md.
- Exact next step: owner manual Mac status-menu Exit/Finder reopen; physical x64 Windows wired controlled workload and optical latency, then 1/5/10/20/40 acceptance ladder. Separate ARM/Wi-Fi ingress/recovery investigation remains necessary.

## 2026-10-04 — Classroom v1 Windows Teacher report and GitHub release

Status: functional packages published; Phase 1 wired scaling acceptance remains pending.

- Added the owner-authorized Windows 11 ARM Teacher + macOS/Windows ARM Student evidence to AGENTS.md and docs/18_WINDOWS_TEACHER_TEST_AND_V1_RELEASE.md. The report records lifecycle, role settings, cursor hiding, native file send and post-Stop diagnostics, plus Parallels and ARM/Wi-Fi limitations.
- Packaged Windows Teacher x64, Windows Student x64, Windows Student ARM64, macOS Teacher and macOS Student into five ZIP assets. Excluded runtime logs, preserved macOS bundles, and generated SHA-256 checksums.
- Created and published https://github.com/maivanmanh/netsupport-school/releases/tag/v1 with all five ZIPs, RELEASE-NOTES-VI.md and SHA256SUMS.txt.
- Verified local ZIP listings, absence of log paths, asset sizes and generated checksums. Documentation commit f2ef43d was pushed to main before publishing.
- Remaining: native macOS status-menu Exit/reopen is unverified; ARM/Wi-Fi recovery remains poor; physical Windows x64 wired test and 1/5/10/20/40 performance ladder are still required by docs/06_PERFORMANCE_ACCEPTANCE.md.
- Exact next step: manually verify macOS status-menu Exit and Finder relaunch, then test an isolated physical Windows x64 Student over wired Ethernet before beginning the acceptance ladder.

## 2026-10-04 — Windows Teacher four-group follow-up

Status: functional integration and faults exercised; shared-host 20/40 decode fails; wired acceptance remains pending.

- Enforced a bounded rolling one-second encode admission budget after measuring >30 FPS under fresh capture. Prefer the documented Windows MF GOP-size property, with fallback for other transforms. Final 1488×768 A/B Teacher encode ~29.44–29.59 FPS, Windows ARM decode/render ~28.49/24.31 FPS, Mac ~29.45/26.34; recovery on ARM still exceeds one second.
- Exercised desktop size changes and restart-on-fitted-size-change, owner-confirmed unlock, and an actual 5-second helper pause/resume. Native Windows sleep returned ERROR_NOT_SUPPORTED; custom scaling disables the DPI selector, so real sleep and 125%/150% remain unverified.
- Completed a 30-minute functional soak with reconnect/late join and Share/Stop. Parallels changed resolution until owner pinned 1080p; simulation selected idle real Students for some phases. This is not 30 minutes of uninterrupted 1080p rendering.
- Same Teacher PID/one encoder through 90-second 1080p fan-out levels 1/5/10/20/40. Shared Mac headless receivers decode ~29 FPS through 10, ~2.19–4.14 FPS at 20, and 0–0.011 FPS at 40. Teacher send errors remain zero; shared decoder capacity and Wi-Fi burst/loss require separation. Foreground was terminal/light content, not a controlled 6 Mbps motion workload.
- Native UI kill/reopen retained its helper. Helper kill exposed stale IPC timeout preventing respawn; Flutter now queries endpoint PID before attach and only discards ownership confirmed dead. New tests cover live/exited/unknown owners; native post-fix recovery and shutdown evidence is recorded in the detailed report.
- Rust app x64 107 tests passed + 1 ignored; fmt/release passed. Non-strict Clippy completed with existing warnings; strict warnings remain failing. Flutter analyze and 24 tests passed; Windows release rebuilt. No raw IPC endpoints, passwords or logs are committed.
- Detailed evidence and limitations: diagnostics/2026-10-04-windows-teacher-followup.md. Mac permission path is /Users/mvmanh/Classroom-teacher-validation/Classroom Student.app on .205; Windows ARM Student moved to .105. Local deletion review rejection was handled using a new isolated package, without replacing v1 release assets.
- Exact next step: separate raw UDP/depacketization sinks from shared-host decode, then isolated physical Windows x64 over Ethernet with confirmed foreground workload, optical latency and the required 1/5/10/20/40 ladder. Run actual sleep and additional DPI cases in a supporting environment.

## 2026-10-04 — Owner-requested agent handoff conclusions

Status: documentation updated; no new runtime changes or test results.

- Expanded AGENTS.md with the Windows Teacher conclusion, three verified fixes and ordered follow-up priorities: separate shared-host receiver/network bottlenecks, reduce ARM recovery/render lag, measure controlled physical wired performance, then complete real sleep/DPI/continuous 1080p soak cases.
- Explicitly retained bounded latest-frame queues and Phase 1 acceptance gates. Zero Teacher send errors and shared-host simulations do not establish successful receiver rendering or physical 40-PC acceptance; no redesign is justified by the evidence collected so far.
- Verified the documentation against diagnostics/2026-10-04-windows-teacher-followup.md and checked the Git diff for whitespace errors. Existing generated plugin files are outside this documentation checkpoint.
- Exact next step: measure UDP/depacketization without decode on the simulator host, then compare with decode under the same confirmed workload before changing receiver/network code.

## 2026-10-05 — Windows Student Observe/Control encoder stream-change fix

Status: Observe stays live and Control mode is reachable on one Windows 10 Education x64 Student over the 10.0.79.0/24 Ethernet LAN; this is a functional single-peer check, not Phase 1 performance acceptance.

- Reproduced Observe ending before its first frame on the physical i7-4770 Student. Added source/worker error logging exposed Intel Quick Sync returning `MF_E_TRANSFORM_STREAM_CHANGE`; the shared Media Foundation runner treated this encoder event as a decoder-only format error and stopped the publisher.
- Added H.264 encoder output renegotiation after `METransformStreamChange`, including frame-size matching. A second run showed asynchronous QSV also requires waiting for a new `METransformHaveOutput` event after renegotiation; retrying `ProcessOutput` immediately returned `0x8000FFFF`. The runner now waits for the event before retrying. Publisher/source failures are logged with the Observe lifecycle reason instead of being silently discarded.
- Actual Windows Teacher + Student verification on isolated test packages: Observe reached `live`, accepted Control mode with no reported input error, and continued streaming for 117.78 seconds. The Student source reported about 25.68 encode FPS; the Teacher-side Observe receiver interval reported about 5.96 Mbps, 28.82 decode FPS and 29.82 render FPS, with zero decode/depacketizer errors and zero missing packets in the sampled interval. The desktop was 1280x1024; no 1080p/scaling or multi-student acceptance claim is made.
- Verification: `cargo fmt --all -- --check`; Windows x64 `cargo check -p codec-windows-mf --target x86_64-pc-windows-msvc`; `cargo test -p classroom-app --bin Classroom` (107 passed, 1 ignored); Windows x64 release build; Clippy on the codec and app (existing vendor/pedantic and deprecated-method warnings remain). Teacher, Observe children, Student process and temporary scheduled task were stopped after the run; isolated test logs were preserved.
- Exact next step: repeat Observe/Control on a software-MFT-only endpoint and confirm pointer movement visibly on the Student, then validate the fixed path at the intended display size. Keep the wired 1/5/10/20/40 physical-PC acceptance gates unchanged.


## 2026-10-05 — Owner-authorized dev Student update over Classroom

Status: implemented; isolated Mac lifecycle tests and two successive Windows LAN updates passed. Production updater and physical wired media acceptance remain out of scope.

- Added `apps/classroom/src/dev_update.rs`: bounded ZIP staging, explicit OS/architecture/version, full download + executable SHA checks, Student compatibility probe, helper preflight before commit, same-volume installation swap, retained previous build, reconnect receipt validation and automatic restore/relaunch if the new child exits or fails confirmation. Preserve launch arguments, profile/device identity and logs; never modify OS privacy grants.
- Reused existing tools TCP/file transport in classroom_tools.rs and Teacher IPC; UI Settings → Student updates has progress. `scripts/classroom-dev-update.py` packages bundles/raw binaries and deploys unattended through authenticated local Teacher IPC. An automation connection does not take ownership of the Teacher GUI. Success requires new Student room/tools reconnect, not merely a completed transfer.
- Added Windows normal-user manifest and renamed helper ClassroomSwap.exe after initial bootstrap hit os error 740. Owner explicitly approved one additional SSH bootstrap repair. Fixed helper is launched/probed while the old Student is alive. No policy/UAC changes were made.
- Mac local tests: final raw binary and whole .app both rejected a bad executable checksum while the original stayed alive, then swapped/relaunched/reconnected with the same identity through the actual deploy CLI. These are local lifecycle tests, not remote Mac capture-permission validation.
- Windows 11 Pro 10.0.26100 ARM64 Parallels at 192.168.50.94 ran the x64 Student under emulation; routed local Teacher IP was 192.168.50.205. Isolated bootstrap C:/Users/mvmanh/Classroom-dev-update-20261005-194235/ClassroomStudent.exe, ports 49700/49710. Two successive different binaries updated through old Students: build bootstrap-fixed PID11700 → next-fixed PID6432 → next-2 PID5940, stable Student ID1791204169611592400-11700-49710. Both jobs complete; no SSH/SCP after bootstrap. Teacher collected one Student diagnostic archive with zero missing Students. Restart-time tools EOF records are expected; final Student log reaches room/tools ready without ERROR.
- Evidence (ignored runtime files): target/dev-update-tests/final-raw-mac/REPORT.json; final-bundle-mac/REPORT.json; windows-fixed-two-updates/REPORT.json plus deploy-1.stdout, deploy-2.stdout and diagnostics ZIP. Bounded remote test worker stops only its own processes/task after 300 seconds; isolated files/profile/logs retained.
- Verification: cargo fmt --all -- --check, cargo test -p classroom-app --bin Classroom: 114 passed/2 ignored; Flutter analyze and 24 tests passed; final native Mac release and bundles plus Windows GNU x64 release built. Windows PE resource inspection confirms asInvoker. Existing vendor warnings remain. Restored tracked Flutter lockfile after local SDK resolution to avoid unrelated dependency changes.
- Artifacts: target/dev-update-build/Classroom Console.app, Classroom Student.app; target/rustup-windows/x86_64-pc-windows-gnu/release/ClassroomStudent.exe. No published v1 assets replaced. Details and CLI examples: docs/19_DEV_STUDENT_UPDATES.md.
- Remaining limits: trusted-LAN dev packages are not publisher-authenticated; OS grants/Gatekeeper can still require human action; Teacher crash loses active job state and may trigger rollback. Windows ARM native updater build and actual remote Mac permissions across updates were not tested. Physical Windows x64 wired 1080p/40-PC acceptance is unchanged.
- Exact next step: use this CLI to deploy subsequent compatible Student dev builds through existing agents, without SSH. Resume the documented Phase 1 receiver/network isolation and physical acceptance plan when appropriate hardware is available.


## 2026-10-05 — Manual Windows Student Observe setup correction

- Launching the updated Student from the isolated updater test directory auto-loaded its adjacent config.toml with monitoring.enabled=false. Room/tools worked, but Observe timed out because its monitoring control channel was disabled. Corrected that exact flag to true and restarted the existing exe via an interactive task; no binary deployment occurred.
- A stale Observe view initially had a receiver No route to host error. Closing the old session and opening a fresh one after restart restored real decoding/rendering: sampled ~16.93 FPS, zero decode/depacketizer errors and missing packets, pending render frames0. Leave Teacher/Student running for owner manual testing.
- Future manual tests must enable monitoring or explicitly supply a normal config; do not reuse updater-only disabled-monitoring fixtures without checking them. No firewall/privacy policy changes or new build were needed.


## 2026-10-05 — Windows Teacher Win shortcuts and Control footer

Status: owner-reported Win+R local dispatch fixed and tested Windows-to-Windows.

- Added foreground ControlKeyboard WH_KEYBOARD_LL hook in Windows Flutter runner,
  with message-only callback and MethodChannel delivery to existing Rust input.
  Flutter synthetic Meta events are ignored to avoid duplicate release. Unhook
  immediately on native deactivation, Observe/mode transitions, disconnect and
  teardown; other apps and Observe retain their normal Win+R behavior. No Rust
  Student/protocol change and no OS policy changes.
- Hook/shell harness on .94: all 9 checks pass, including exactly ordered Win/R
  events, no Teacher Run in Control, normal local Run when disabled/unfocused.
  Actual Flutter Windows Teacher .94 → Windows Student .206: Control active,
  injection exit0, Teacher Run absent and Student interactive Run count1. Student
  bootstrap only once by SSH; no subsequent Student binary copy/update. Native
  input test helper/launch scripts are separate instrumentation.
- Corrected mistaken new-host IP per owner to 192.168.50.206. First SSH key is
  accepted only for that specified new host; reused outside-sandbox password PTY
  helper. Corrected Teacher test's remembered 48700 port via CLI47800; settings
  preserved. Student .206 path C:/Users/mvmanh/Classroom-Student-dev/ClassroomStudent.exe.
- Owner additionally requested no footer during Control: removed bottom status
  bar and 5px reveal hit target in both Control/exclusive; Observe unchanged.
  Updated existing regression test covers hover at bottom and restore in Observe.
- Verification: Flutter analyze no issues; 24 tests passed; native C++ strict
  warning build and actual Windows/MSVC Flutter release; Mac release .app built
  and opened at target/control-shortcuts-build/Classroom Console.app. Windows
  Teacher test bundle is C:/Users/mvmanh/Classroom-shortcut-validation/Teacher.
  Mac old owned GUI/helper stopped; no Student rebuild/restart required by code.
- Evidence/limits: diagnostics/2026-10-05-windows-control-shortcuts.md. Windows
  binaries are x64 under ARM64 Parallels; native physical Windows performance and
  secure-desktop/Ctrl+Alt+Delete remain unverified/out of scope. Preserve v1 assets.
- Exact next step: owner manually exercises Win+R/Win+E on the updated Windows
  Teacher with .206 Student; deploy future Student versions through Classroom.

- Final deployment complete: old isolated Windows Teacher folder removed only
  after its owned processes stopped and executable lock cleared; final bundle
  restarted on .94, including app-local x64 MSVC CRT. Collected portable ZIP at
  target/shortcut-test/Classroom-Console-Windows-shortcuts.zip. No execution-policy,
  firewall or privacy grant changes. Final Control-footer regression suite passes
  all24 tests; tracked Flutter lockfile restored after local SDK resolution.


## 2026-10-05 — Observe window lifecycle, transfer and Windows rendering

- Rust coordinator owns one Observe UI child per Student, including startup;
  repeated requests activate the existing instance. Native Teacher foreground
  activation is explicit; closing hides immediately and exits its UI process,
  with old surface notifications unable to close a newer session.
- Stretch exposes the full cover-size frame through two scrollbars. Final gutters
  isolate local scrollbar dragging from remote pointer input. Added native drop,
  streamed folder ZIP/file staging, Desktop delivery, configurable 300 MiB default
  ordinary-transfer limit (1..4096), preparation and transfer progress.
- Windows texture keeps bounded last-good leased pixels on transient lock/pool
  contention; engine teardown precedes pixel-buffer disposal. Student unchanged.
- Verification: Flutter analyze clean, 27 tests pass; Rust 12 tools and 6 config
  tests pass; Mac and Windows x64 Rust releases built. Native MSVC texture harness
  passes. Windows .94 Teacher to .206 Student IPC run: single child across repeated
  requests, 1875 ms cold UI startup, X hide 30 ms and process exit, ZIP delivered
  to Student Desktop. Actual OS drag/drop and final native UI-click activation
  remain manual checks; physical x64 black-flicker acceptance remains unverified.
- Final Mac bundle: target/observe-tools-build/Classroom Console.app. Earlier
  Windows Teacher bundle deployed at C:/Users/mvmanh/Classroom-shortcut-validation/Teacher;
  final Flutter scrollbar-gutter update not built/deployed there because .94 was
  shut down. Latest Windows Rust helper cross-build exists under target/rustup-windows.
- Evidence: diagnostics/2026-10-05-observe-window-tools.md; raw evidence ignored
  under target/observe-tools-test. Preserve published v1 assets and Student builds.
- Owner requested stop testing once sufficient; no extra .206 self-host run.
  Exact next step: owner manually checks final Mac UI. When Windows Teacher is
  available, build/deploy final UI gutter correction and smoke actual click/drop;
  remove temporary Observe instrumentation tasks then. No new performance claims.

## 2026-10-06 — LAN Student bootstrap and connection diagnostics

- Owner approved Teacher HTTP ZIP download at `/`, unzip/double-click Student,
  room configuration included; connection popup controlled by Teacher; Windows
  Student tray must not offer ordinary quit. This is initial dev lab setup, not
  a web application/media rewrite or privileged process protection.
- Added student_bootstrap.rs and student_connection_ui.rs; integrated startup,
  tools capability/preference, tray, Teacher snapshot/commands, Flutter Settings,
  config and Mac/Windows bundle scripts. Docs20 and decision log describe limits.
- Verified Rust117 pass/2ignored (final targeted ZIP/HTTP2 pass), Flutter27 pass,
  analyze clean, Rust/Mac bundle/Windows Student x64 release builds. Fixed small
  desktop sidebar test scrolling. Restored unrelated SDK pubspec.lock entries.
- Actual local IP206. `.94` Student updated through old Classroom agent (no SSH
  exe copy), receipt complete; then HTTP-downloaded ZIP independently extracted
  and opened without args in user desktop. Connected to206:47800, native popup
  auto-hidden ~3.03s after room-ready. Teacher live toggle state and Student received
  true observed; rapid off/on may coalesce, separate native-off visibility unmeasured.
  Collected1/1 Student diagnostics via Classroom,0missing. No prolonged media tests.
- Port80 bind denied on this Mac;8080 works without changing OS policies. Final
  running Mac bundle target/student-bootstrap-build/Classroom Console.app, HTTP
  http://192.168.50.206:8080/. Student path on .94:
  C:/Users/mvmanh/Classroom-browser-bootstrap-20261006/ClassroomStudent.exe.
  Native measurement task removed, apps remain running for owner. Preserve v1.
- Evidence/limitations: diagnostics/2026-10-06-lan-student-bootstrap.md and ignored
  target/bootstrap-tests/REPORT.json + Classroom-diagnostics.zip. No physical x64,
  Chrome click or40-PC acceptance claim. Windows Teacher Flutter UI build deferred.
- Exact next step: owner uses Settings→Student setup/browser ZIP when adding a lab
  Student. Save Teacher settings to retain popup preference across restart; update
  and diagnostics afterwards use existing Classroom TCP/IPC. Production protected
  Student service and port80 privileged setup remain separate work.

## 2026-10-06 — smaller topmost notification, paired builds, Observe error recovery

- Student popup300×78, bottom/right16px work-area margin, no title/control buttons,
  topmost without activation, auto-hide3s. Native Windows probe exposed automatic
  WS_CAPTION with style zero; explicitly use WS_POPUP for titleless borderless windows.
- Owner build rule now in PROJECT_NOTES.md/AGENTS.md. Build Mac Teacher+Mac Student+
  Windows x64 Student together and bundle matching ZIPs; Windows script also builds
  paired Rust roles before Flutter (native Windows Teacher validation deferred).
- Observe receiver logs: first2connections NoRoute65, third received live frames.
  Fixed stale error latched in Flutter after recovery; receiver failure now includes
  actual fatal error from bounded log tail. Regression covers fresh source recovery.
- Verified Rust119pass/2ignored,Flutter28pass,analyze clean; paired release builds
  and Teacher ZIP identity. Windows deployment through Classroom updater confirms
  restarted version. Evidence:diagnostics/2026-10-06-popup-paired-build-observe.md.
- Exact next step: owner tests existing Observe/control UI on final paired bundle.
  No extended media benchmark or physical40-PC acceptance in this milestone.
- Final native Windows verification:style94000000,no caption/system menu/min/max,
  exstyle8 TOPMOST;300×78,16px margins. Updater complete paired-20261006-082848.
  Running Teacher:target/popup-final-build/Classroom Console.app. Matching fresh
  Mac Student:target/popup-final-build/Classroom Student.app; Windows updated .94.

## 2026-10-06 — retire duplicate offline roster identity after reconnect

- Actual Teacher cache contained two IDs for MAIVANMANHED30/192.168.50.94,
  one historical offline and one connected. Student identity persists per profile;
  bootstrap/profile changes can leave a historical roster entry under another ID.
- Agent roster migration: current online entry supersedes offline cached/current
  aliases only when BOTH normalized hostname and IP match. Remove stale thumbnail
  and seen-online state; persist migrated cache. Never merge concurrent live peers,
  unknown-host or simulated identities; preserve unrelated/different-IP machines.
- Flutter regression covers offline alias→live replacement, restart persistence,
  concurrent peers and distinct IP. Flutter29passed; analyze clean.
- Build paired Mac Teacher+Mac Student+Windows x64 Student per PROJECT_NOTES.md.
  Teacher-only fix: existing Students need no redeploy. New isolated Teacher bundle:
  target/roster-fixed-build/Classroom Console.app, matching packages bundled.
- Exact next step: use current Teacher roster and observe/control as before;
  no extended media/performance retest needed for this roster-only correction.
- Runtime verified after new Teacher launch: actual known-students.json now contains
  exactly1 MAIVANMANHED30/192.168.50.94 record, retaining current Student ID
  1791248125994874600-5256-47810. Both bundled Student ZIPs byte-match fresh packages.

## 2026-10-06 — immediate Observe control and debounced thumbnail tiers

- Student previously slept up to2s after thumbnail response before reading Observe.
  Move bounded snapshot rate limiting before snapshot work; read next control
  immediately. Teacher condition-variable wake already bypasses idle interval.
  No warm encoder pool/continuous capture on40Student machines; only requested
  Observe source starts. Existing thumbnail work may still finish first.
- Teacher grid actual item width/Auto selects240/360/480px bounding box;900ms
  debounce, deduplicate unchanged tier/reversals. Rust validates three widths and
  atomically keeps only latest tier; next normal request sends it at existing2s
  cadence, preserving JPEG48KiB bound and source aspect/even dimensions.
- Rust120pass/2ignored (new real-TCP regression bypasses2s cadence);Flutter30pass
  including fast slider reversals;analyze clean. Fresh paired Mac Teacher+Mac
  Student+Windows x64 Student built. Windows .94 updated via Classroom, confirms
  paired-20261006-092013; no SSH binary deployment.
- Running isolated Teacher:target/observe-thumbnail-build/Classroom Console.app.
  Student ZIPs bundled. No need to warm all endpoints or extended benchmark loop.
- Short live .94 check:received thumbnail tiers240→214×134,360→324×202,
  480→432×270 (source1728×1080 aspect/even rounding). Observe queue0.048ms,
  source handshake27.06ms,source-ready61.04ms; first decoded surface frame571.7ms
  from observe_open command. Previous source-ready3213ms was another cold session,
  not matched repeated benchmark. New570ms measure excludes Flutter window startup
  and is NOT glass-to-glass/control latency. Evidence ignored target/
  observe-thumbnail-runtime.json. Closed probe session,restored360tier,apps running.
- Exact next step:owner checks grid slider and normal Observe UX. Existing Student
  builds can accept new size requests, but Observe cadence fix requires newStudent;
  .94 already updated. Mac Student built/bundled,not remotely deployed/validated.

## 2026-10-06 — responsive remote cursor shape and Student rendered-FPS badge

- Control retains local Teacher position, receives changed Student BGRA cursor+
  hotspot through latest-only bounded TCP slot/Notify, separate mode acknowledgments.
  Reuse capture PointerSource, sample shape20Hz only in active Control; limit96px
  and64KiB message. Publish changed cursor immediately through Teacher IPC. Flutter
  decode once/serial and move via ValueNotifier/RepaintBoundary without full UI
  rebuild; normal arrow fallback and Observe OS cursor preserved.
- Student broadcast badge88×28,semi-transparent alpha200,topmost/click-through/no
  title/buttons,16px bottom/right; render-video FPS each1s, excluding pointer-only
  redraw. Owner UI thread; no frame copies or new process; closes with preview.
  [diagnostics].student_fps_badge=false disables. No badge in Observe/headless.
- Rust121pass/2ignored;control-transport10pass/doctests with stable rustdoc;Flutter
  30pass/analyze clean. Windows .94 update through Classroom confirmed build
  paired-20261006-093510. Paired build/bundled fresh Students verified.
- Short live cursor fixture:received two32×32 bitmaps/serials with different hashes
  and hotspots. A first probe sent Control before first video and was discarded by
  existing readiness path; corrected fixture waited first frame. No prolonged test.
- Teacher ScreenCaptureKit share rejected by macOS TCC after new ad-hoc build;
  owner asked to grant Screen Recording to target/cursor-fps-build/Classroom Console.app
  and restart before video FPS badge validation. Permission not automated. Native
  badge validation pending; Mac Student build/bundle only. See docs/21_CONTROL_CURSOR_AND_STUDENT_FPS.md.
- Exact next step:restart Teacher after owner grant,run one short class broadcast
  to verify badge/alpha/click-through/close; keep apps available for owner's tests.

## 2026-10-06 — round top-left FPS badge; stable thumbnails across tier changes

- Owner requested round compact FPS:56×56,12px top/left margin,centered FPS label,
  true native elliptical region (Windows)/clear clipped content layer (Mac).
  Keep alpha200,topmost,click-through/noactivate and1s update cadence.
- Found tier resize destroys capture+cached frame; restart can emit a transitional
  black snapshot. Keep a single native capture at max480×270,cache/latest frame,
  bilinearly downsample before JPEG to requested bounding box. Tier changes never
  restart capture or clear prior pixels. Small tiers save encoding/network/decode;
  capture remains max size, avoiding churn. Actual mode/Observe/permission failures
  still retain existing lifecycle behavior; no masking black intentional content.
- Rust122pass/2ignored including resize/color/no-upscale regression. Paired Teacher,
  Mac Student and Windows x64 Student build. Windows .94 Classroom updater confirms
  paired-20261006-095109. No SSH binary deployment; test-only native probe task/files
  removed. Student settings/logs preserved.
- Native badge test using existing owner-authorized Teacher:56×56,left/top12,
  style94000000,exstyle80800A8,alpha200,region3; corner outside and center inside.
  40samples,17visible,last absent after Stop. Previously privacy-blocked badge
  validation now completed after owner grant/restart, not automated TCC modification.
- Short live tier sequence240→360→480→240 produced216×134,322×202,432×270,216×134;
  all four JPEG snapshots nonblack (maxchannel255,channel means~93–111).
  No full-screen black image observed in that short probe. Evidence ignored target/
  round-fps-native-result.txt and thumbnail-no-black-result.json. Not prolonged
  benchmark or physical40PC claim; Mac native round badge only build-verified.
- Exact next step:owner tests normal slider/Observe/share with the paired bundle
  target/round-fps-thumbnail-build/Classroom Console.app. Existing .94 already new.

## 2026-10-06 — Smooth circular FPS badge

- Owner reported jagged circular badge. Windows binary elliptical region cannot
  represent partial pixel coverage; replace badge-only paint with premultiplied
  BGRA per-pixel alpha via UpdateLayeredWindow. Transparent corners, one physical
  pixel transition, opacity200,56×56/top-left12 retained. Owned GDI resources
  released after each tiny1Hz paint; layered style reset only on initial switch.
- Mac native circular layer explicitly enables edge antialiasing. No video-path
  changes, no additional media frames or high-frequency painting.
- Rust123pass/2ignored; focused mask test verifies transparent corners, opaque
  center, fractional edge alpha, symmetry and premultiplied white channels.
- Paired Mac Teacher/Mac Student/Windows x64 Student release build succeeded at
  target/smooth-fps-build/Classroom Console.app. Windows .94 updated through
  Classroom (not SSH binaries), confirms paired-20261006-100422. Mac Student is
  build/bundle-verified only; no new remote Mac deployment.
- Short Windows native broadcast probe passed56×56/margins12; layered/click-through/
  noactivate/topmost flags retained and GetWindowRgn=0 confirms hard clip removed.
  Alpha now belongs to per-pixel compositor: GetLayeredWindowAttributes alpha is
  not a valid opacity measurement here; no claims from invalid region point tests.
  Test-only task/files cleaned. Evidence target/smooth-fps-native-result.txt.
- Open matching Teacher for owner visual verification. Native probe verifies
  lifecycle/geometry, not visual smoothness on all DPI settings. Exact next step:
  owner checks badge edge during normal broadcast; no prolonged benchmark needed.

## 2026-10-06 — Drop files directly onto Student roster items

- Added StudentFileDrop to Grid/List/Compact rows, stable device-ID keys. Subtle
  teal highlight/upload hint identifies hovered recipient. Drop requires explicit
  confirmation with machine name/IP and Desktop destination; Cancel/dismiss does
  not stage or submit. Ignores drops under modal routes; busy item rejects duplicate
  drops. Target is the dropped item alone, independent of presentation selection.
- Only online tools-ready receivers accept drops. Recheck after confirmation and
  staging; shared sendDroppedFiles path preserves security-scoped bookmarks,
  streamed staging/ZIP isolate,32-item batch and configured max_file_mb(300MiB).
  Observe reuses same helper. Hub still owns progress/cancel/retry/Desktop delivery.
- Added Agent.commandChecked while preserving existing command API/behavior.
  File submission errors/disconnected socket stop the batch and show a message;
  unacknowledged staged payloads retain existing24h cleanup policy (ACK timeout
  may mean Hub accepted it, so do not delete a potentially in-flight payload).
- Flutter analyze clean;31 tests pass. New widget regression covers hover exit,
  cancel/no send, recipient name/IP, explicit send and disconnect during confirm.
  Existing grid/list/compact test verifies one drop wrapper in each view;
  existing real file/ZIP content/size tests pass. No media-engine changes.
- Paired release build: target/roster-file-drop-build/Classroom Console.app,
  Mac Student+Windows x64 Student packages paired-20261006-101531 bundled freshly.
  Teacher restarted via IPC; native runtime confirms idle and Windows .94 online
  with tools ready. Student unchanged/running: feature needs no Student update.
- No native Finder/Explorer drag-to-send end-to-end claim this turn; owner can
  test directly with this opened Teacher. No prolonged LAN/performance tests.
  Exact next step: drop a file/folder onto .94, confirm destination, check existing
  transfer progress and Student Desktop. Mac Student not redeployed/tested.

## 2026-10-06 — Simplify Student file/message popups and click-to-copy

- Rust Student message/file windows are borderless without OS title/minimize/
  maximize/close chrome. Match connection toast light background and narrow accent;
  smaller spacing/file width320. Keep essential OK/Reply/input, file progress,
  Show/collapse/Close actions and existing transfer lifecycle semantics.
- Message placement centers outer frame on monitor work area in both axes, including
  Reply expand/collapse (same display). Files stay bottom-right16px, now clamp small
  work areas and show topmost without focus activation. No media-path changes.
- Message body/background click copies full original Unicode/multiline message,
  excluding buttons/reply editor. Show Copied2s; native clipboard unavailable/busy
  retries via UI pump up to400ms with no sleeping. Final failure shows click-again
  hint; logs contain no message or clipboard content. Vendor minifb exposes native
  Win32 movable Unicode clipboard ownership and Cocoa NSPasteboard text only.
- Rust124pass/2ignored: center geometry negative origins/small screens, copy/action
  hit separation, existing wrapping/reply/file progress tests. Paired Teacher,
  Mac Student and Windows x64 Student release build succeeded; packages version
  paired-20261006-102317, Teacher target/simple-student-popup-build/Classroom Console.app.
- Windows .94 updated/restarted through Classroom updater and confirms new version.
  Short native test: message320×150, center_dx=0/center_dy=0, no caption/topmost;
  file320×170 (two rows), right/bottom16, no caption/topmost. Simulated body click
  followed by Get-Clipboard exactly matches synthetic Vietnamese/multiline text.
  File used normal Teacher tool_file channel. Evidence target/simple-popup-native-
  result.txt and simple-popup-deploy.txt. Temporary tasks/scripts/test Desktop
  payloads removed, completed test message dismissed and file popup cleanup requested.
- Initial instrumentation read a UTF-8 script as Windows ANSI, so result was missing;
  corrected test script to UTF-8 BOM and reused displayed test message. App required
  no fix from that instrumentation issue. Future Unicode PowerShell workers must
  include UTF-8 BOM or explicit Get-Content -Encoding UTF8.
- Matching Teacher opened. Mac clipboard/UI compiled/bundled, not native-tested or
  remotely deployed in this turn (only .94 active). Exact next step: owner verifies
  simplified popups with normal file/message actions; no prolonged media test needed.

## 2026-10-06 — Popup outline, single-window Reply and stale tools events

- Owner clarified OS-borderless still needs custom outline; both Student popups
  now draw a thin1logical-pixel gray-blue perimeter, including collapsed file view.
- Teacher log at unix_ms1791257640529 showed Unknown classroom job, followed by
  disconnect marking pending message d985e98d9ec9ba229d206b8bb865d435 error. A message
  event from previous Teacher history/session must not close an otherwise healthy
  tools channel or fail unrelated current jobs. Hub ACK/discards unknown job events;
  known-job recipient/status/size/reply rules unchanged. Safe event IDs/status logged,
  never message/reply text. Pre-fix evidence target/reply-before-teacher-log.txt.
- Add per-Hub tools session tag in existing handshake token field. New Student
  clears obsolete message queue/events and active popup on a changed Teacher tag;
  same-session reconnect preserves pending messages. File events retained, legacy
  Teachers without tags accepted. No protobuf/version change, no video changes.
- Reply now resizes one borderless native window in place and preserves callback/
  HWND/Cocoa object. Send/OK hides immediately then queues final event and drops
  popup; no second reply window/leftover parent created. Reply Cancel recenters and
  collapses the same window. Actual subsequent inbox messages still display normally.
- Rust126pass/2ignored, including stale-event/current-job isolation and only-new-
  session cleanup regressions; previous center/copy/reply/progress tests pass.
  Paired Teacher/Mac Student/Windows x64 Student release build successful,
  paired-20261006-103941. Windows .94 updater confirms1791258003 version. Teacher
  target/student-reply-fix-build/Classroom Console.app opened, matching packages.
- Native Windows Reply probe: count1 before and expanded, same HWND=true, height
  129→236; after Send visible message windows=0, Teacher status=replied and exact
  synthetic reply match. Restart test had one pending old message; after Teacher
  restart count0 and tools ready=true. Evidence target/reply-native-result.json.
  First immediate LaunchServices open after quit returned -600 (old app finishing
  shutdown); retry after it exited succeeded, restart verification completed with a
  separate short native scan. Temporary tasks/scripts/results removed remotely.
- Mac implementation build/bundle-verified only, not deployed/native-tested this
  turn; active endpoint was Windows .94. Exact next step: owner tests custom border
  and normal reply once; no additional prolonged media/LAN benchmark required.

## 2026-10-06 — v1.1.0 release packaging

- Owner explicitly confirmed keeping GitHub Public and authorized push/release.
  Preserve v1 assets; prepare a separate v1.1.0 release.
- Version Rust workspace 1.1.0, Flutter 1.1.0+2; derive Mac Student/nested Agent
  bundle versions from these sources. Fresh paired Mac Teacher, Mac Student and
  Windows GNU x64 Student built with label v1.1.0 into target/release-v1.1.0.
- Rust application regression:126 passed/2 ignored; Flutter analyzer clean and
  31 tests passed. macOS signatures and both embedded Student manifest hashes
  verified; clean Mac archives checked. Teacher root HTTP test downloaded fresh
  Windows Student with bootstrap config and matching executable SHA256; endpoint
  stopped after check. No logs/settings/tokens included in release archives.
- Old test IP192.168.50.206 no longer assigned; adapter now172.16.49.233.
  Release Teacher opened with current local IP for smoke test. Windows .94 SSH
  actually timed out; no credential failure inferred. Manual GitHub Actions
  windows-2022 workflow builds fresh paired x64 Teacher/Student using Flutter3.38.6
  and app-local VC runtime instead; source push99887f4, run37418693586.
- Native performance acceptance unchanged: this packaging verification is not
  40physical-PC wired-LAN acceptance. See docs/21_RELEASE_V1_1_0.md.
- Windows CI run37418693586 succeeded; native MSVC x64 Teacher and portable
  Student archives retrieved. Windows Teacher includes freshly built matching
  student-packages/student-windows-x64.zip. No further runtime tests per owner.
  Public v1.1.0 release packages: Teacher/Student macOS arm64 and Windows x64,
  Vietnamese release notes and SHA256SUMS.txt. Release source target40f9d95;
  packaging workflow source99887f4 differs only in documentation.
  Next step: owner downloads release and tests normal classroom workflow when
  convenient; no additional benchmark implied by successful builds.
## 2026-10-06 — Real Windows classroom telemetry follow-up

- Implemented a macOS process-metrics fallback in `apps/classroom/src/process_metrics.rs` using Mach task CPU-time deltas and resident memory. macOS CPU percentage keeps the project convention that one fully busy logical CPU is 100%; other platforms retain the sysinfo path. `cargo check -p classroom-app --bin Classroom` and paired release build succeeded. Fresh paired artifacts are in `target/real-lan-telemetry-build`; Windows Student was rebuilt and embedded in the Mac Teacher package as required. No Student deployment was necessary for this Teacher-only metrics change.
- The existing six-physical-Windows run remains valid for media behavior (report `target/real-windows-6-lan/analysis.txt`): six stayed connected for ~35 seconds, decode/render averaged ~24 FPS, no Teacher send errors or decode errors. It was mostly static content and source capture was 1512x982, so it is not 1080p/40 acceptance.
- A short active scroll/typing retest could not start: macOS ScreenCaptureKit returned `user declined TCCs for application, window, display capture`. Investigation shows Screen Recording settings list `Classroom Console` as allowed, but capture runs in the separately identified nested helper app `Contents/Helpers/Classroom Agent.app` (`com.classroom.classroomConsole.agent`), which is not listed. This is the likely permission mismatch; no media or active-workload telemetry was produced. Do not automate the privacy grant. Teacher is left idle and the HTTP Student download server remains available at `http://10.0.68.10:8080/`.
- Five Students (`.116`, `.117`, `.118`, `.120`, `.135`) reconnected; `.115` did not respond to ping and was not included in the retest. The Teacher interface reports the physical Ethernet adapter at 100 Mbps, below the assumed 1 Gbps; verify link negotiation before making a classroom-capacity claim. Diagnostics were collected from the five reachable Students, but because capture permission was denied these logs are not an active media-run result.
- Exact next step: owner adds/allows the nested `Classroom Agent.app` at `target/real-lan-telemetry-build/Classroom Console.app/Contents/Helpers/Classroom Agent.app` under Screen Recording (the existing allowed Console entry alone does not authorize the capture helper), then restarts Teacher. Run one short scroll/typing broadcast to the five reachable Windows PCs, collect their logs, and verify non-placeholder Teacher CPU/RAM fields. Keep the six-client old-build media evidence separate from this telemetry retest. Changes are currently uncommitted.
- CPU follow-up on the next rebuild found the initial Mach API accounted only terminated-thread time. Replaced the CPU counter with `TASK_ABSOLUTETIME_INFO` totals for live + terminated threads and compare against `mach_absolute_time` ticks; kept resident memory from `MACH_TASK_BASIC_INFO`. Focused regression `process_cpu_sample_includes_live_thread_work` passed (1/1), confirming a live busy thread reports >10% CPU.
- Fresh paired build label `paired-real-lan-cpu-fix` completed after Flutter SDK needed to refresh its external `bin/cache/engine.stamp` (approved build escalation). Mac app and both Student packages were rebuilt and embedded; Windows x64 Student archive SHA-256 `4355c0a5dabbe06edeb0f506b200de50450fe775916d2b094ceaae3f0180945b`. Incidental Flutter lockfile resolution changes were restored; source lockfile is unchanged.
- Reopened the rebuilt Teacher: five Windows Students reconnected. HTTP package endpoint returned `200 OK`, 4,172,465 bytes, at `http://10.0.68.10:8080/`. No Student deployment was needed. A capture attempt on the rebuilt Teacher was blocked by TCC: System Settings lists `Classroom Console` allowed but no `Classroom Agent`; the nested helper has an ad-hoc code signature identified by a changing CDHash, so rebuilding invalidated its previous Screen Recording grant. Do not automate adding/toggling this privacy permission. Teacher is idle, HTTP sharing is on.
- The just-collected 38-second previous-build active workload run (scroll + typing) had five Students at decode 24.13–24.15 FPS and render 23.49–23.72 FPS, zero missing packets and zero decode errors. Its CPU implementation was the flawed Mach-basic version: 35/37 one-second Teacher CPU samples were exactly zero, despite memory values around 112–130 MB, then the completion sample jumped to 266.9%. Do not treat those CPU samples as valid; use only as evidence motivating the fix. The new CPU fix has focused test + build evidence, but still needs one short five-Student runtime sample after the owner grants Screen Recording to the rebuilt helper.
- Owner enabled Screen Recording for the rebuilt capture helper and asked that future permission requests include the full path and exact permission name. Restarted the paired Teacher; capture succeeded and all five reachable Windows Students reconnected. Started a short broadcast, then stopped it after collecting diagnostics from all five Students (0 missing diagnostic uploads). Diagnostics archive: `~/Library/Containers/com.classroom.classroomConsole/Data/Library/Application Support/com.classroom.classroomConsole/diagnostics/Classroom-diagnostics-1791270438158-session-8924799680914049997.zip`.
- Runtime validation of the fixed Mach CPU sampler: 98 one-second Teacher intervals reported `process_metrics_available=true`; CPU ranged about 16.2–53.3% (latest samples ~17–21%) and resident memory stayed around 114–132 MB. In the stable five-Student portion, capture was roughly 18–26 FPS, encode stayed ~29.7–30 FPS, packet loss was 0%, and Teacher send errors were 0. This validates the counter and live capture/fan-out path; it is not a 1080p/40-PC performance result. Collected Student logs showed no `ERROR` entries. The stream was mostly unchanged synthetic text, so these logs validate telemetry and connectivity, not active-motion quality.
- HTTP Student download endpoint was restored after the broadcast and returned `200 OK` at `http://10.0.68.10:8080/`. Teacher is running idle and ready; five Students remain reachable. Student `.115` is still offline. No Student redeployment was needed.
- Exact next step: use the corrected live-thread CPU metric in the next real workload comparison. For any future macOS privacy request, state the exact full app/helper path and permission (for this build: `/Users/mvmanh/Development/netsupport-school/target/real-lan-telemetry-build/Classroom Console.app/Contents/Helpers/Classroom Agent.app` — **Screen Recording**), then restart Teacher after the grant. Source changes remain uncommitted.
- Follow-up 2026-10-06 — expanded real Windows LAN cadence sample to ten Students, including newly online `.101`. Raised `adaptive_small_hold_ms` from 400ms to 1200ms so ordinary short pauses during typing do not immediately switch the target cadence into idle; added a regression for sparse text-like changes about 900ms apart. `cargo fmt --all -- --check` passed and the focused quality suite passed 21/21. Built paired Mac Teacher, Mac Student and Windows x64 Student from the same tree (`paired-20261006-typing-cadence`); Windows x64 Student ZIP SHA-256 `8828fbc340d6c53b5b9c385cbd5e875a6366525230f55c2037caf997afdad891`. The old Teacher updater delivered/restarted and confirmed this build on all ten selected physical Windows Students (`.101`, `.102`, `.116`, `.117`, `.118`, `.120`, `.124`, `.126`, `.135`, `.136`). The remembered roster entry `.115` did not connect and was not part of this ten-machine test. After the owner enabled Screen Recording and the Teacher agent was fully stopped/restarted, capture succeeded. One 64.3s session included paced text entry (~900ms between lines) and scrolling; all 10 Students reconnected throughout and all 10 diagnostic uploads completed (0 missing). Archive: `~/Library/Containers/com.classroom.classroomConsole/Data/Library/Application Support/com.classroom.classroomConsole/diagnostics/Classroom-diagnostics-1791271780824-session-5738164720521578537.zip`. New build IDs match `paired-20261006-typing-cadence` on Teacher and all 10 Students. Teacher source capture was 1512×982, not 1080p. Across the session, capture averaged 25.0 FPS, encode 27.0 FPS (p95 encode call 24.7ms), CPU 27.3%, RSS about 120.5MB; target cadence sampled 24FPS 49 times and 30FPS 14 times, never fell to the 5FPS idle tier during the paced typing sample. Aggregate send averaged 6.0Mbps and peaked about 24.9Mbps with 10 receivers; send errors and repair cache misses were 0. Student averages were 26.8 decode / 25.6 render FPS; decode errors and UDP ingress drops were 0, packet repair had no cache misses, and one interval reported 0.41% receiver loss. This validates a short ten-physical-Student run over the current lab LAN, not 1080p/40-PC acceptance. Teacher uplink is still the previously observed 100Mbps USB Ethernet link; no 1Gbps capacity claim. No additional code change is justified by this run; the 1200ms typing hold is runtime-verified to avoid falling into 5FPS while the paced text workload continues. Teacher is left idle with all ten test Students online. Exact next step: keep this build for the next classroom use; before scaling claims, repeat with a verified 1Gbps link and higher-motion 1080p content on more physical PCs (20/40 ladder when available).
- After the ten-Student run, restarted Teacher's HTTP Student sharing at `http://10.0.68.10:8080/`, explicitly pointing at the newly bundled `student-windows-x64.zip`. GET `/` returned HTTP 200, `application/zip`, attachment `Classroom-Student-x86_64.zip` (4,172,808 bytes); its exe SHA-256 `e43c08be9166dbb83de9dede80b85b66bbaf8489e447ce5efc093b2118707362` matches the current paired build manifest. The generated ZIP bootstrap contains `teacher_ip = "10.0.68.10"`, control port 47800 and the current Student settings. Teacher remains idle with 10 Students online and the HTTP package server sharing. Only one local verification download has been counted; actual classroom-browser downloads are not yet observed.
- Follow-up 2026-10-06 — ran one 50-second five-phase active workload (static, paced typing, scrolling, terminal output, full-screen motion) to the same ten physical Windows Students over the current 100 Mbps Ethernet link. Test harness `scripts/phase1-mac-workload.swift` initially crashed because `NSScreen.main` is nil before a window becomes key; switched to the first attached screen with a bounded fallback and reran successfully. All ten diagnostic uploads completed (0 missing). Archive: `~/Library/Containers/com.classroom.classroomConsole/Data/Library/Application Support/com.classroom.classroomConsole/diagnostics/Classroom-diagnostics-1791272409827-session-5738164720521578540.zip`. Teacher source remained 1512×982 (not 1080p): capture 29.28 FPS, encode 27.17 FPS, encode p50/p95 17.55/24.75 ms, CPU 58.7%, RSS 136 MB; target cadence was 24 FPS for 34/50 intervals and 30 FPS for 16/50. Mean encoded stream 1.31 Mbps; aggregate unicast send mean 13.47 Mbps, peak 53.99 Mbps, with 0 send errors, would-blocks or abandoned packets. Thus the observed 100 Mbps link was not saturated by this ten-client workload, but a 40-client claim cannot be extrapolated from it. Students averaged about 24.0 decode / 22.4 render FPS across available one-second reports; lower observed clients were `.124` and `.135` at about 21 decode / 20 render FPS. No decode errors; packet sequence gaps and teacher repair cache misses were present but small in total impact (105 misses / 726 requested sequences, 0.23% aggregate reported receiver loss at the last interval). This is a bounded-recovery/continuity opportunity, but current evidence does not establish that the configured 300 ms cache expiry caused the misses; no speculative product tuning was made. Student `.126` had one isolated control liveness timeout after about 12 seconds, then reconnected and continued; the other nine stayed present. Teacher was left idle, HTTP sharing still listens on `10.0.68.10:8080`. No confirmed product bug or safe performance fix was demonstrated by this run. Exact next step: keep the paired build; at the next performance window, first verify a 1 Gbps link and use a fixed 1920×1080 source, then run one equivalent ten-client workload. If repair misses or sequence gaps persist, instrument per-client NACK age/cache miss before changing the 300 ms retention; if render remains below decode, profile the Windows render path separately from Teacher/network.

## 2026-10-06 — Real-room 20-Student run and bounded UDP retry

- One real-room 20-Student broadcast completed before the owner left the LAN. Teacher reported 20 connected recipients (21 remembered roster entries); all 20 diagnostics were collected with no missing uploads. Archive: `~/Library/Containers/com.classroom.classroomConsole/Data/Library/Application Support/com.classroom.classroomConsole/diagnostics/Classroom-diagnostics-1791273600023-session-5738164720521578543.zip` (43 files).
- Session duration 145.17s, one encoder, capture 28.95 FPS (capture p50/p95 1.77/2.79ms), encode 29.05 FPS (encode p50/p95 9.00/19.01ms), encoded stream 1.76 Mbps mean, aggregate unicast send 34.99 Mbps mean, CPU 31.0%, RSS 120.7MB. Teacher counted 4 UDP send errors, 34 abandoned packets and 6,405 retransmitted packets; NACK requests 6,781, with 376 repair-cache misses. Student diagnostics show zero decoder errors; packet gaps/depacketizer errors and intervals of lower render cadence remain visible. Teacher log is truncated at its configured 2MB tail, so this run does not establish source resolution or 1080p acceptance. Do not infer 40-PC capacity from 20 clients; source capture, activity phases and a verified 1Gbps link were not established by this archive.
- Evidence-led Teacher sender fix: macOS returned `ENOBUFS` (`No buffer space available`, errno 55) for UDP sends during the 20-client run. `SendBudget` previously retried only `WouldBlock`, so temporary kernel send-queue exhaustion could fail that recipient for the rest of the current access unit. Extend bounded retry classification to macOS/FreeBSD `ENOBUFS`, Linux `ENOBUFS`, and Windows `WSAENOBUFS`, sharing the existing maximum 2ms access-unit retry budget. Add `send_resource_backpressure` telemetry separately from `WouldBlock` and a macOS regression test. Focused test `cargo test -p classroom-app --bin Classroom udp_sender::tests`: 4 passed; `git diff --check` passed.
- Fresh paired release build from the same working tree succeeded into isolated `target/20pc-enobufs-build/Classroom Console.app`; it contains newly built Mac Student and Windows x64 Student packages, both labelled `paired-20261006-20pc-enobufs`. Embedded ZIP manifests and executable hashes verified; macOS codesign strict verification passed. This build was not launched or deployed. Owner left the LAN, so no second classroom run was attempted; current performance impact of the retry fix remains unverified. The current live HTTP endpoint, if still running, belongs to the old Teacher build and has not been changed.
- Exact next step: at a future LAN window, run one short broadcast on this paired Teacher and inspect `send_resource_backpressure`, `send_errors`, and `send_abandoned_packets`; no need to repeat the full workload unless errors persist.

## 2026-10-06 — File collection and two-panel file manager (Teacher ⇄ Student)

- New Rust modules `apps/classroom/src/classroom_tools/file_tree.rs` (listing, filtered walk, checksummed tree stream both directions, path sanitising, never-overwrite publish with identical-content skip) and `fs_jobs.rs` (Hub jobs `collect` / `fetch` / `push`, listing requests, Teacher endpoints `fs_upload` / `fs_download` / `fs_result`, Student workers). Students always connect to the Teacher; media/control threads untouched. Bulk streams are paced (~4 MiB/s each, max 6 at once, busy → retry); interactive ~16 MiB/s. Student capability `fs:1` gates the feature (older Students get a clear "update first" error).
- Teacher agent commands: `fs_collect`, `fs_list`, `fs_result`, `fs_transfer`, `fs_cancel`, `fs_open`, `files_window`. File windows are dependent UIs like Observe (`--files-id`), max 2.
- Flutter: toolbar “Collect files…” (path preset/custom, extension filter + presets, subfolders, flat option, size cap, progress dialog with per-Student bars, cancel, open folder); per-Student folder icon, right-click menu and Observe toolbar icon open the two-panel window (drag & drop both ways, folder-row drop targets, Send/Get buttons, Finder/Explorer drop onto the Student panel, progress strip that auto-hides after success).
- Collection output: `Desktop/Collected YYYY-MM-DD HH.MM/<Student name>/<student's own structure>`; flat option puts everything in one folder per Student with `name-1.ext` numbering.
- macOS: Teacher agent is sandbox-inherited, so Release/DebugProfile entitlements gained home read-write and absolute read-only temporary exceptions, and the real home is read from the account database (not the container `$HOME`). Mac Student touches Desktop/Documents/Downloads once at start so TCC prompts appear early; denials return an actionable message.
- Verified: `cargo test` 139 passed (new unit tests incl. stream, flat, traversal, token/capability checks, end-to-end over the Teacher endpoint); `flutter analyze` clean; paired build `scripts/build-flutter-macos.sh` (needs rustup toolchain first in PATH; Homebrew rustc lacks the Windows target). Runtime E2E with a headless Mac Teacher agent (isolated ports 47900+) and the new x64 Student on Windows ARM64 VM: listing, structured/flat/shallow collect with extension filter, cancel mid-transfer, fetch folder, push file+folder, error paths all passed; test artifacts removed.
- NOT verified: visual layout and drag/drop of the Flutter windows (only IPC traffic from the real file window observed), the sandboxed Mac Teacher bundle reading/writing Desktop (entitlement change untested), Mac Student as target, 20-Student collection load, Windows Teacher build. No commit made.
- Next step: open `target/Classroom Console.app` normally, try Collect from the toolbar and the file window on a Mac/Windows Student, grant any macOS folder prompt, report issues.

## 2026-10-06 — Remote application launch (class-wide)

- New tools job `launch` (Teacher toolbar rocket icon → dialog with saved items: add/edit/delete, note, path, args, OS filter, presets; Launch starts the selected item on every selected online Student). Items persist in Teacher `console-settings.json` (`launch_items`). Student side `classroom_tools/launch.rs`: Windows starts the exe directly (raw args, working dir = exe folder) or resolves bare names/URLs via `cmd start`; macOS `open -a` for `.app`, direct exec for files, `open` for URLs; `%VAR%` expansion; result (`complete` / `error: Program not found`) returns to the Teacher dialog. Gated by Student capability `launch:1` (older Students refused with a clear message).
- Verified: unit tests (parse/expand/capability gate); real run Mac Teacher → Windows ARM64 Student (172.16.49.120): notepad.exe and an https URL launched in the interactive session, missing path reported as error, empty path refused. Dialog UI itself not visually inspected.
- Test-environment note: Windows scheduled tasks created with `schtasks` default to “don't start on battery” and stay Queued on this VM; use `Set-ScheduledTask -Settings (New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries)`. Task `CRfs2` (my test) runs `C:\Users\mvmanh\Classroom-fs-test\ClassroomStudent.exe` against Teacher 172.16.49.233:47800.

## 2026-10-06 — Power and screen-lock controls

- Teacher toolbar "Power" menu (Lock screens… with message, Unlock, Log out…, Restart…, Shut down… with confirmations; badge/lock icon shows locked Students). Student job `power` gated by capability `power:1`; restart/shutdown get a 15 s grace (`shutdown /r|/s /f /t 15` on Windows, loginwindow Apple events on macOS), logout is immediate; result is reported before the command runs. Launch/power results no longer raise the Teacher overlay card (launch status is a compact line in its dialog).
- Lock screen (`student_lock.rs`): borderless topmost window over the whole virtual screen (all monitors); on Windows two low-level hooks swallow every keyboard and mouse event system-wide (only Ctrl+Alt+Del remains, Windows kernel limit), running on a dedicated thread with a continuous message loop (a hook on the 250 ms UI pump was skipped by Windows, which let Win+R through in the first version). Fail-safes: lock lifts when the Teacher is silent >60 s, on a new Teacher session, or if the UI heartbeat stops >10 s. macOS Student: overlay only, no system shortcut blocking (Cmd+Tab etc. still work) — not tested.
- Verified: unit tests; Windows target compiles; real Mac Teacher → Windows ARM64 Student lock/unlock round trip, log shows both hooks installed. The user's earlier Win+R report was against the first (hook-on-UI-thread) version.
- Mistake: my restart test sent `shutdown /a` before the Student's 2.5 s delayed shutdown ran, so the Windows test VM (172.16.49.120) actually rebooted once (~18:27); Student restarted via task CRfs2 afterwards. Logout/shutdown/macOS paths not executed.

## 2026-10-06 — Internet access control (Guard), first slice

- Owner decisions: Student runs elevated/admin (assumed installed by admin); domain/wildcard granularity (HTTPS hides paths); Windows first; design must extend to a future privileged service + anti-kill. Architecture and roadmap: `docs/21_GUARD_ARCHITECTURE.md`.
- Implemented: `apps/classroom/src/guard/` (Policy/Enforcer/Guard with persistence + lease + restore; `rules.rs` host/wildcard/CIDR rules; `dns.rs` UDP+TCP forwarding proxy; `internet.rs` enforcer with Firewall/DnsRedirect traits; `windows.rs` WFP dynamic-session filters + NRPT rule + elevation check). Modes: open (unblock all), block_all, allow_only(list), block_only(list). Loopback, private/LAN, Teacher and upstream DNS always reachable. Hub job `policy`, capability `guard`, per-Student mode shown on the Teacher; Teacher UI `internet_access.dart` (toolbar globe icon + badge, saved named lists persisted in `console-settings.json`, presets, lease selector, compact status; red icon on restricted cards). `scripts/windows-guard-reset.ps1` for emergencies.
- Verified: 160+ unit tests (rules, DNS wire + proxy end to end, enforcer behaviour for all modes with fake firewall, Guard persist/restore/lease, Hub capability gate), Windows target compiles, `flutter analyze` clean, paired macOS build.
- NOT verified: anything against the real Windows Filtering Platform / NRPT / PowerShell (needs the elevated Windows Student; the Windows VM became unreachable when this Mac moved to another network, 172.16.49.x → 172.20.10.x). Elevation: run the Student scheduled task with RunLevel Highest. Unknowns to check first on the VM: NRPT namespace `.` accepted, port 53 free, first-connection latency after a learned address, ssh/LAN stays reachable under block_all, localhost:8080 works, curl/PowerShell/native app blocked.
- Exact next step: reconnect Mac and VM on one network, start the Student elevated, apply Block all → Allow list → Block list → Unblock all while checking `curl.exe`, `Test-NetConnection 1.1.1.1 -Port 443`, a local `HttpListener` on 127.0.0.1:8080, and the Student log; use a 1–5 minute lease for the first runs.

- Update (same day, runtime-verified on Windows ARM64 VM 192.168.50.194, Student elevated via scheduled task RunLevel Highest, Mac Teacher 192.168.50.206): Block all, Allow-only (example.com + *.wikipedia.org: those load, google blocked), Block-only (example.com blocked, others open), Unblock all all behaved correctly for curl.exe, `Test-NetConnection 1.1.1.1:443`, and a PowerShell HttpListener on 127.0.0.1:8080 stayed reachable in every mode; Teacher LAN link and ssh stayed up. Bug found and fixed: Windows' DNS cache kept an old answer so Block-only let example.com through once; the policy change now flushes the DNS cache. Crash test: killing the Student drops the WFP filters by itself (IP traffic open) while the NRPT redirect remains (names fail) until the Student restarts, which resumes the saved policy automatically; known gap closed only by the future service. Reset script needs `powershell -ExecutionPolicy Bypass -File`.
- Not exercised: Teacher GUI dialog (UI only compiled/analyzed), Chrome/Messenger-style apps with their own DoH, long lease expiry, many students.

## 2026-10-06 — Advanced monitoring: open windows, browser tabs/downloads, clipboard block

- Scoped from the owner's request: window/browser/download monitoring implemented; full keystroke logging and unrestricted clipboard-content capture declined (replaced with clipboard *blocking*, which never reads what was copied). Owner agreed to this scope.
- New `apps/classroom/src/activity/` (Windows only; `supported:false` elsewhere): `windows.rs` lists top-level visible app windows (`EnumWindows`, excludes the Student's own windows and owned/tool popups), reads Chromium (Chrome/Edge/Brave) browser tab titles and the *active* tab's address via UI Automation (`IUIAutomationElement` tab-strip scan + `OmniboxViewViews` address bar; background tabs never expose a URL — not available through accessibility), and reads recent downloads (name + source URL + size + time) from a staged copy of Chrome/Edge's `History` SQLite file (`rusqlite`, bundled, cross-compiles to `x86_64-pc-windows-gnu`). Each browser window's UIA scan runs on its own thread with a 1.5 s watchdog so one busy browser can't stall the report. On-demand only (Teacher-requested snapshot), never a background stream; reuses the existing `fs_list`/`fs_result` request-and-poll wire plumbing (`classroom_tools/activity_jobs.rs`), capability `activity`.
- `apps/classroom/src/clipboard_guard.rs`: class-wide clipboard *block* (not capture) — a message-only window + `AddClipboardFormatListener` empties the clipboard on every `WM_CLIPBOARDUPDATE` while active; no admin rights needed. Added as two more `power` ops (`clipboard_block`/`clipboard_allow`), tracked like screen-lock (`clipboard_blocked` set, reported at hello-time, cleared on a new Teacher session).
- Teacher UI: roster/Observe icon (eye) opens an Activity dialog (collapsible Open windows / Browser tabs (grouped by window, active tab marked, URL shown when known) / Recent downloads, Refresh button); Power menu gained "Block clipboard (exam mode)" / "Allow clipboard" with a roster-card indicator next to the lock icon.
- Found and fixed my own bug before any user-facing claim: the Teacher's capability whitelist copy (`st.capabilities.insert(...)`) was missing the new `activity` field, so the capability silently never reached the Hub despite the Student reporting it — caught by actually querying a real Student, not just by compiling.
- Verified on the real Windows ARM64 Student (192.168.50.194) via a headless Mac Teacher agent: window listing picked up real app/Settings windows; opening two Edge windows (1 tab, then 3 tabs) was read back with exactly correct per-window tab titles, correct `active` flag, and the active tab's URL only (background tabs' `url` correctly `null`); a synthetic `History` SQLite profile (built and copied in, unrelated real profiles untouched) round-tripped exam-flavoured fake downloads (Drive/Gmail URLs) correctly, and — unexpectedly but confirmingly — three *real* GitHub zip downloads from earlier test attempts were also read back correctly (right byte size, right URL), proving the live-browser path too; clipboard block/allow verified behaviourally (content set after blocking never survives; the same sequence survives once allowed) using a `tool_launch`-spawned PowerShell in the Student's own interactive session (plain SSH `Set-Clipboard`/`Get-Clipboard` turned out to run in a different, non-interactive window station and is not a valid way to test this — a dead end worth remembering, not a product bug). `cargo test` 161 passed throughout; `flutter analyze` clean.
- NOT verified: Firefox/non-Chromium tab titles (window title only, documented as a limit, not implemented further), a browser with many tabs/windows at once, `windows_truncated`/tab/download truncation paths, the dialog's "unsupported" (non-Windows) branch, and the real UI by eye (only its IPC traffic and resulting state were exercised end-to-end; no screenshot taken this session per the owner's earlier clipboard-privacy caution about screen captures).
- Paired Teacher+Student rebuilt and redeployed from this final source after all fixes; `target/Classroom Console.app` is current. Windows ARM64 VM 192.168.50.194 still has the test `Classroom-fs-test` deployment and scheduled task `CRfs2`; no production endpoint was touched.

## 2026-10-06 — Activity monitoring: dedicated window, redesign, icons, favicons, open/closed history

- Owner feedback addressed: (1) dialog → dedicated OS window like Observe/Files; (2) redesigned from one long expandable-scroll list to a tabbed window (Open windows / Browser tabs / Downloads) with card/wrap layouts instead of a tall list; (3) curated icon+colour set for common Windows apps (Edge, Chrome, Firefox, Notepad, Word/Excel/PowerPoint, VS Code, Visual Studio, PyCharm/IntelliJ, Terminal, PowerShell, Explorer, Acrobat, Teams/Zoom, …) — Material icons + brand-ish colours, not real extracted .exe icons (out of scope); (4) real browser-tab favicons for the active tab (only tab with a known URL) read from Chromium's own `Favicons` SQLite (same staged-copy technique as downloads/History), base64 over the wire, rendered via `Image.memory` with a globe fallback; (5) downloads got file-type icons grouped by category (archive/audio/video/image/code/text/office/installer); (6) new open/closed **history** for both apps and browser tabs — `activity/history.rs`, Student-process-lifetime in-memory tracker keyed by process name (apps) / tab title (tabs, the only stable-ish signal available), recording first-open, closed-at, accumulated duration across reopens, and a reopen counter; merged on every snapshot the Teacher actually requests (the window auto-refreshes every 6 s while open — bounded like Observe, not a 40-student background cost) — explicitly documented in the UI as having gaps when nobody was watching, not hidden.
- New: `apps/classroom/src/activity/history.rs`; Report gained `apps`/`tabs` (full history) alongside the existing live `windows`/`browsers`; `activity::build` now takes `&History`. New Flutter `activity_window.dart` (dedicated window, 3 tabs, auto-refresh toggle, per-section cards) and `app_icons.dart` (app look-up table, site-icon widget, file-type look-up, duration/clock formatting); `activity_view.dart` (the old dialog) deleted. Teacher plumbing: `activity_window` IPC command (mirrors `files_window`, `MAX_ACTIVITY_WINDOWS=2`), `--activity-id`/`--activity-name` CLI flags, `dependent` window handling (macOS `AppDelegate.swift`, `agent.dart`).
- Also fixed in passing (reported by the owner): the "Share Student app" HTTP download port field always showed "80" when the Settings dialog reopened regardless of what was actually running or last chosen, which looked like "Save & reconnect" wasn't persisting it — root cause was that "Save & reconnect" never touched this feature at all (it's a separate on-demand action, the "Share bundled Student app" button); fixed the field to read the live share's port or the last-chosen one (now saved to preferences) and added an inline note clarifying which button applies it.
- Verified end to end on the real Windows ARM64 Student (192.168.50.194) through the real dedicated window (not just IPC): screenshots confirm the window opens correctly sized/titled, tab bar with live counts, card-grid "Open windows" view with correct per-app icons (Notepad/Terminal/Settings/generic fallback) and a correctly-labelled, correctly-icon-coloured "closed" Edge entry with accurate elapsed duration (54 s) and timestamp, auto-refresh visibly advancing open durations (1m → 2m) on its own. Favicon fetch independently confirmed via IPC + saved PNG: a freshly opened github.com tab yielded a real, correctly-decoding 32×32 PNG (the actual GitHub icon). Full `tabs`/`downloads` history inspected via IPC: structurally correct, includes real pre-existing browsing history already on that dev machine (TDTU e-learning, AWS/w3schools tutorial, local dev server) — most of those historical entries have no favicon/URL simply because they were last captured without one (a background tab or a scan before this feature existed), which is the documented, expected "gaps" behaviour, not a bug.
- Caught and corrected during testing, not swept past: my first "activity_window" request accidentally targeted a *different*, unexpected online Student (`MAIVANMANHED30` at 192.168.50.94, not the intended `.194` VM) because this headless test Teacher binds the real lab subnet on the standard control port; checked the roster, recognised `.94` from the owner's own saved lab-machine notes (not a third party), closed that window immediately, and switched to the intended target. Flagging this for the owner: ad-hoc test Teachers on this LAN/port can pick up the owner's other already-configured Student machines unintentionally.
- Cross-compiles clean for `x86_64-pc-windows-gnu`; `cargo test` 163 passed throughout (incl. 2 new `history` tests — open/close/reopen accumulation, eviction keeps open entries); `flutter analyze` clean.
- NOT verified: the Browser Tabs and Downloads tabs' on-screen rendering specifically (blind macOS UI automation to switch Flutter tabs proved unreliable — clicked through to the wrong overlapping window instead; relied on IPC/data-level verification plus the already-confirmed shared card/row components instead of forcing more screenshot attempts); Firefox; very large numbers of tracked apps/tabs in the real UI (only the eviction logic itself was unit-tested); multiple Activity windows open at once.
- Paired Teacher+Student rebuilt from this final source and redeployed; `target/Classroom Console.app` is current. Windows ARM64 VM 192.168.50.194 test deployment (`Classroom-fs-test`, task `CRfs2`) and its real Edge download/browsing history are unchanged/untouched beyond what's described above; no scheduled tasks left behind.

## 2026-10-06 — Activity monitoring: fast background collection, UIA tab fix, class-wide toggle, disconnect alert

- Owner feedback addressed, in priority order:
  1. **Slow to open** — root cause was scanning (EnumWindows + UIA tab walk + SQLite) synchronously inside the Teacher's request/response round trip. Fixed with `activity/collector.rs`: a Student-side background thread (gated by a new class-wide monitoring toggle) keeps `history::History` and a cached `Report` current on its own, every 6 s (windows/tabs) with downloads read every 5th cycle (real disk I/O, kept less frequent); the on-demand `activity_jobs::run` now only hands over that already-current cache — no scan happens inside the request anymore. Measured on the real VM: round trip dropped from multi-second to **3–7 ms**.
  2. **Class-wide on/off switch** — new Hub `activity_monitoring` flag (`activity_monitoring_set` command), pushed to each Student like `connection_popup` (`activity_status` on/off, capability-gated); while off, Students run no collector at all (confirmed: on-demand request returns a `monitoring_enabled:false` sentinel with zero scanning). Toolbar eye icon toggles it; Students connecting or restarting while it's on start collecting automatically (same push-on-handshake mechanism).
  3. **Teacher-side cache + instant reopen** — Hub now caches each Student's last-known report (`Hub::activity_cached`/`activity_ingest`, updated whenever an upload lands) and `activity_view` returns it synchronously in the same round trip that also queues a background refresh; the Activity window shows the cached report immediately (no spinner) while a small "syncing" indicator updates it when the fresh one lands. This is a deliberately simpler stand-in for a full incremental per-field sync protocol — explained to the owner as a choice, not hidden.
  4. **Wrong/duplicated browser tabs** (`33 tabs`, repeated "Câu chuyện tin tức" titles) — root cause found: the UIA scan searched the *entire window* for `TabItem`-typed elements, which also matched Edge's unrelated sidebar news-feed cards (apparently built from the same WinUI tab-like control). Fixed in `activity/windows.rs`: now finds the actual tab-strip (`UIA_TabControlTypeId`) first and only looks for `TabItem`s inside it; as a second, independent safety net, the window's own title (Chromium formats it as `"<tab> and N more pages - Profile X - Edge"`, confirmed from real captured data) gives an authoritative expected tab count — a scan that disagrees is discarded rather than shown. Favicon-always-generic turned out to be a symptom of this same bug (the wrong "active" tab was being picked), not a separate fault.
  5. **Disconnect alert while monitoring is on** — `Hub::disconnected()` now logs an alert (bounded to 50) when a Student goes offline while monitoring is on; included once per Teacher snapshot (`new_alerts`), never resent. *Revised after first implementation*: the owner found the initial self-dismissing red SnackBar unusable ("mất rồi còn đâu" — gone before they could look); replaced with the same durable pattern as **Student replies** — a toolbar badge (red, "link off" icon) showing the unread count, opening a dialog that lists every disconnect this session with name/IP/time, `Clear`/`Close`, mirroring `showRepliesDialog` exactly (`ToolActivity.disconnects`/`unreadDisconnects`/`markDisconnectsRead`/`clearDisconnects`, `showDisconnectsDialog`).
- Verified end to end on the real Windows ARM64 Student (192.168.50.194) via the real Teacher IPC (not just compilation): monitoring-on round trip timing (3–7 ms) and cache-hit-on-reopen confirmed; a clean two-separate-Edge-windows test (1 tab + 2 tabs, matching the owner's own repro) now reports **exactly** the right windows, tab counts, active flags and URLs — no duplicates, no sidebar junk; favicons fetched correctly for two different real sites (GitHub 1736 B, python.org 1776 B PNGs, both decoded and visually confirmed earlier in the session); disconnect alert fired within the poll window for the killed Student only, not the other one that stayed online. `cargo test` 163 passed; cross-compiles clean for `x86_64-pc-windows-gnu`; `flutter analyze` clean.
- Process note: two of my own test runs during this investigation gave a false "window not found" result — traced to PowerShell `Set-ScheduledTask -Action` reuse on the same task name being flaky when re-triggered in quick succession (not a product bug); switched to one freshly registered-and-unregistered task per window for reliable repros, worth remembering for future scripted Windows tests.
- NOT verified: the new toolbar toggle/badge and disconnect dialog rendered on screen (checked via `flutter analyze` + the underlying IPC/state fields only, no screenshot this round); behaviour with many (10+) simultaneous monitored Students; the "cached-instant-display" path specifically through the Activity window UI itself (confirmed at the Hub/IPC level: `has_cached=True` on reopen).
- Paired Teacher+Student rebuilt from this final source and redeployed; `target/Classroom Console.app` is current, Windows ARM64 VM 192.168.50.194 Student (`Classroom-fs-test`, task `CRfs2`) updated and running. Activity monitoring was left **off** afterward (collector stopped) and all test browser windows/tasks removed — no lingering background collection left running on that machine.

## 2026-10-06 — Found the real cause of the disconnect-alert spam, ApplicationFrameHost noise, Internet access window

- **Root cause found (not guessed) for the "disconnects every second" the owner reported**: `.94` (the owner's second Windows machine, `\\Mac\Home\Downloads\Classroom-Student-x86_64 (1)\ClassroomStudent.exe`, a Parallels-shared-folder deployment) was running a Student build from *between* two of today's changes: new enough to report `activity:1`, too old to understand the new `activity_status` push message. Its log showed the smoking gun: `Student tools disconnected; retrying e="Invalid classroom tool"` — the old Student's catch-all `_ => anyhow::bail!(...)` tore down the *entire* tools connection (messages, files, diagnostics — not just activity) on one unrecognized push, then reconnected into the same wall a second later, forever. Two fixes: (1) the Student's catch-all now logs and ignores an unrecognized action instead of killing the connection — general forward-compatibility, not an activity-specific patch; (2) `Hub::disconnected()` now also debounces alerts per Student (30 s cooldown) as a second line of defense against any future flapping connection. Confirmed fixed: 20 s of live observation after redeploying both Students produced zero alerts.
- **`ApplicationFrameHost.exe` in Open windows**: it's the generic UWP app-frame host — reporting it duplicates the real app (we saw both `ApplicationFrameHost.exe - Settings` and `SystemSettings.exe - Settings` for one window). Added a small exclusion list in `activity/windows.rs` (`ApplicationFrameHost`, `TextInputHost`, `ShellExperienceHost`, `StartMenuExperienceHost`, `SearchHost`, `LockApp.exe` — Windows shell infrastructure, never something a Teacher is looking for). Confirmed on `.94`'s real window list: gone, `SystemSettings.exe` still reported correctly on its own.
- **Internet access redesigned into its own window** (owner: no more dialog, no dead space on Allow all/Block all, must feel finished/professional, Apply must actually close it instead of sitting there looking stuck): new `InternetWindow` (left sidebar of the 4 modes as selectable cards; right panel is either a centered icon+explanation for Allow all/Block all — no more leftover empty box — or the full list editor, stretched, for the two list modes; bottom bar has the lease dropdown and Cancel/Apply). Apply now disables the form, submits, shows a brief "Applied" confirmation, and the window **closes itself** (`agent.finishApplication()`, the same clean dependent-window teardown Observe/quit already use) — matching the rest of the app's pattern where a configuration UI closes on submit and ongoing status lives on the roster card, not in a lingering dialog. New Teacher command `internet_window` (spawns `--internet-ids`/`--internet-audience`, up to 2 at once, mirrors `files_window`/`activity_window`); `agent.dart`'s `dependent` and macOS `AppDelegate.swift` updated for the new dependent-window kind.
- Verified end to end: both Windows Students (`192.168.50.94`, `192.168.50.194`) redeployed from the identical latest build (confirmed matching build id in `capabilities`), following the owner's deployment rule for `.94` exactly (stopped the exact running process, verified its exact `\\Mac\Home\...` path, replaced the binary in place, restarted it with its original no-argument command line via a one-shot elevated-interactive scheduled task, logs/profile untouched); monitoring-on 20 s window showed no alerts; `.94`'s live window list no longer includes `ApplicationFrameHost.exe`; the new Internet access window opened as its own process and was visually confirmed (screenshot) to have no dead space on Block all, with a clean sidebar/content layout. `cargo test` 163 passed, Windows cross-compile clean, `flutter analyze` clean. Activity monitoring left off and no restrictions left applied afterward.
- NOT verified this round: the list-editor (Allow only/Block only) panel by eye (blind macOS click-automation to switch the sidebar selection proved unreliable again, same as last time; the code path is the same, previously-working `TextField`/dropdown widgets just moved into a bigger `Expanded` area, and the Block all screenshot already confirms the layout mechanism — no empty space by construction, since the right panel is always `Expanded` and filled either way); the self-close-after-Apply behavior specifically (did not run Apply to completion in this session, to avoid leaving a real restriction applied on the owner's machines without them watching).

## 2026-10-06 — "Apply where possible, skip and report the rest"; VS Code extension allowlist control

- **Owner decision, applied to every Internet-blocking-related job**: a selected computer that cannot enforce a job (not elevated / old build) must no longer block the whole batch. `Hub::submit()`'s old blanket `anyhow::ensure!` for `action == "policy"` was removed; the filtering moved up to the caller (`teacher_agent.rs`'s `tool_policy`/`tool_vscode_policy` handling), which now pre-filters `recipients` to only those reporting `guard:1`, collects the rest as `{"name","ip"}` into the ack's `result.skipped`, and only returns a hard error if *every* selected computer had to be skipped. `internet_access.dart`'s `apply()` reads `result.skipped`; if non-empty it no longer auto-closes the window (it used to close ~900 ms after any success) — it shows an amber "skipped N computers: name (ip), …" panel and waits for the Teacher to read it and press Close/Cancel manually.
- **New feature: VS Code extension allowlist + forced Copilot block**, from the owner's request (checkbox-managed allowlist of real installed extensions fetched from the Students, GitHub Copilot always force-disabled rather than a normal checkbox, VS Code Desktop only — no portable/WSL). Researched the real mechanism against microsoft/vscode's own source via `gh` (not a guess): `AllowedExtensions` is a genuine, registry-enforced Enterprise Policy (`HKLM\SOFTWARE\Policies\Microsoft\VSCode\AllowedExtensions`, REG_SZ JSON `{"publisher.name":true,…}`, VS Code ≥1.96); `AllowedExtensionsService.isAllowed()` denies by default once the policy object is set non-trivially, so Copilot (`github.copilot[-chat][-nightly]`) is reliably blocked purely by omission — no negative entry needed. `chat.disableAIFeatures` (the Copilot Chat panel toggle) has **no** registry policy tier yet (confirmed: no `policy:` block in its setting registration; open upstream issue #268012 requests one), so it is handled as an explicitly best-effort second layer only: written into the Student's own `User/settings.json` and the file's write/change-permissions ACL denied for that user via `icacls` — documented in code comments as *not* a guaranteed enforcement, unlike the registry policy.
- New `apps/classroom/src/vscode_control.rs` (Windows-only real impl, stub elsewhere): `list_extensions()` scans `%USERPROFILE%\.vscode\extensions\*\package.json` (name/displayName/description/publisher, deduped, Copilot ids excluded from the list since it's never a normal checkbox) via `reg.exe`/`icacls.exe` shell-outs (consistent with the project's existing pattern for one-off admin operations, e.g. `power.rs`); `apply()`/`clear()`/`is_applied()`. New `classroom_tools/vscode_jobs.rs` (on-demand scan-and-upload, reuses the `fs_result` upload channel exactly like `activity_jobs.rs` but with no background collector — cheap enough to run per request). Wiring: capability `vscode` (always-available listing) + `vscode_blocked` (elevation-gated enforcement state, reported at hello time so a Teacher restart doesn't lose track of which Students are currently restricted); Hub action `vscode_policy` (validates `VsCodePolicy{enabled,allowed}`, same shape as `policy`); `State.vscode_blocked` tracked/cleared alongside `locked`/`clipboard_blocked`/`internet`; Teacher IPC `vscode_list` (mirrors `activity_request`) and `vscode_window` (mirrors `internet_window`, `MAX_VSCODE_WINDOWS=2`, `--vscode-ids`/`--vscode-audience`). New Flutter `vscode_access.dart`: dedicated window (not a dialog) with a fixed "GitHub Copilot — Always blocked" row (lock badge, no checkbox), an enforce-allowlist switch, a search box, and a checklist of the union of extensions found across every selected Student (name + description + id); Apply sends the same `tool_vscode_policy` request pattern as Internet access, including the same skipped-list panel. New toolbar icon (`extension_outlined`, badge = count of Students currently enforcing an allowlist) next to the existing Internet-access icon.
- Updated an existing unit test that depended on the removed blanket check (`policy_jobs_need_the_guard_capability_and_a_valid_document` → rewritten to `..._the_guard_capability_filter_is_the_callers_job`, asserting only document validation is still `Hub::submit()`'s job) and added a new one for the VS Code job's validation + `vscode_blocked` tracking through a full submit→event→snapshot cycle.
- Verified: `cargo test -p classroom-app` 166 passed (was 163; +2 new, 1 rewritten), native macOS build and Windows cross-compile (`x86_64-pc-windows-gnu`) both clean with no warnings from the new files, `flutter analyze` clean.
- NOT verified this round: anything against a real Windows machine (`.94` is now shut down by the owner; `.194` was not touched this round) — the registry policy write, `icacls` lock, and real VS Code picking up `AllowedExtensions` and hiding/graying Copilot are all unverified against the actual OS/VS Code behaviour, only against the documented/source-level mechanism and unit tests; the new window's on-screen layout (no screenshot taken this round, only `flutter analyze`); multiple Students with genuinely different installed-extension sets merging correctly in the checklist UI.
- Exact next step: once a Windows machine with VS Code is available again (owner noted both `.94` and `.194` already have VS Code installed, `.94` is currently shut down), redeploy the paired build to `.194`, open the VS Code window from the Teacher, confirm the fetched list matches what's actually installed, Apply an allowlist, and confirm in VS Code itself that unlisted extensions (and Copilot) are greyed out/disabled and that the Install button is blocked for a non-allowlisted extension.

## 2026-10-07 — Moved both machines to a new LAN, found and fixed two real bugs on real hardware

- Owner moved networks: Windows Student now `172.16.49.25` (the other Windows machine, `.94`, is shut down), Mac now `172.16.48.237`/22. Redeployed from v1.2.0 source using the established discipline (found the exact running process/path via `Get-CimInstance Win32_Process`, stopped it, `scp`'d the new exe in place with a SHA-256 hash check before and after, restarted through a uniquely-named one-shot `Register-ScheduledTask`/`Start-ScheduledTask`/`Unregister-ScheduledTask` run (`RunLevel Highest`, `LogonType Interactive`, `UserId mvmanh`) with `--teacher-ip` corrected to the Mac's new address — confirmed in the Teacher's own log ("Student online", capabilities matching). Mac Teacher: killed the stale agent process (old IP baked into its own already-running state) and relaunched the freshly built `Classroom Console.app`. SSH access used the project's existing `scripts/run-four-machine-test.py` `Remote` helper (password via `CLASSROOM_WINDOWS_SSH_PASSWORD`, pty-injected) — not a new mechanism.
- Owner reported two VS Code window problems and asked for them to be tested for real on `172.16.49.25`, not just unit-tested. Both root-caused on the actual machine, not guessed:
  1. **Extensions list showed empty.** A raw direct IPC call to the Teacher's local socket (bypassing Flutter) returned all 12 real installed extensions correctly, proving the Rust/Student side was fine. Screenshotting the actual open `VsCodeWindow` (both the owner's already-open one and a freshly spawned one) showed "Teacher agent is not connected." — `VsCodeWindow.initState()` called `load()` unconditionally, racing the dependent window process's own async socket connect to the Teacher's local IPC; every other data-fetching dependent window (`files_window.dart`, `activity_window.dart`) already guards this with `if (agent.connected) { ... } else { agent.addListener(waitForAgent); }` — `vscode_access.dart` was simply missing that established pattern (my own oversight when it was written, not a pre-existing bug elsewhere). Fixed to match; also changed the empty-list message to distinguish "still connecting" from "genuinely found nothing" so this failure mode reads clearly if it ever recurs.
  2. **Copilot was still usable despite the "Always blocked" badge.** Not a backend bug: the badge was always shown regardless of whether anything had actually been applied, and since the extensions list was empty (bug 1), the client-side guard in `apply()` (`enabled && selected.isEmpty` → refuses to submit) meant no policy had ever actually been sent — so of course nothing was blocked on the real machine. Fixed the badge to reflect real state (`Blocked now` vs `Not applied yet`, computed from `vscodeBlockedStudents(agent)` intersected with the window's actual audience) with an explanatory line, so the UI can no longer imply enforcement that hasn't happened.
  3. Rebuilt and redeployed (fix only, same session) and **applied a real allowlist to `172.16.49.25`**: `AllowedExtensions` registry value confirmed written (`{"ms-vscode.cmake-tools":true,"ms-vscode.cpptools":true}`), `chat.disableAIFeatures:true` merged into `settings.json` preserving the Student's other keys, `icacls` DENY confirmed on the file for the Student's own account. Launched real VS Code (1.140.0, arm64) via an interactive scheduled task and read **VS Code's own log** (`NativePolicyService#_onDidPolicyChange`) — it loaded exactly that policy value at startup, independent confirmation beyond "the registry key exists." Found, while verifying, that the bundled/built-in Copilot Chat extension's real id is `github.copilot-chat` (lives in VS Code's own `resources/app/extensions/copilot`, not the user `.vscode/extensions` folder) — already covered by the existing `COPILOT_IDS` exclusion list, so no code change was needed there, just confirmed the id assumption against the real install. Cleaned up afterward: cleared the test policy (registry value removed, `chat.disableAIFeatures` removed from `settings.json`, ACL restored to inherited-only, confirmed by reading the file back), closed VS Code and the test windows.
- **New gap the owner raised, fixed before it was reported as a failure**: a Student already running VS Code when the Teacher applies a policy (or who deliberately keeps VS Code open to dodge it) would never pick up `AllowedExtensions`, since VS Code only resolves machine policy at process start (confirmed directly in its own log: `PolicyConfiguration#initialize` runs once at launch). `vscode_control::apply()` (Windows) now detects a running `Code.exe` via `tasklist`, force-closes it (`taskkill /F /IM Code.exe /T`) and relaunches it from its fixed launcher path (`%LOCALAPPDATA%\Programs\Microsoft VS Code\Code.exe`) whenever enforcement turns on, so the new policy is picked up immediately rather than whenever the Student next happens to restart VS Code themselves. Relies on VS Code's default hot-exit to restore unsaved buffers — not a guarantee if a Student has customized `files.hotExit`, documented as such in the code comment. Deliberately **not** done on `clear()` (relaxing a restriction is never urgent enough to justify force-closing the Student's editor). Not yet verified against a real already-open VS Code instance (ran out of session time after the policy-apply/log verification above); only compiles clean on both targets and passes the existing 166 tests.
- Answered the owner's question about a future Windows-service-based Guard (`docs/21_GUARD_ARCHITECTURE.md` already documents this as planned-not-started): today's model is the Student `.exe` itself running elevated — the project's own standing assumption, confirmed in that doc, is that an **admin pre-installs/pre-elevates it before Students use the machine** (e.g. the scheduled-task launch pattern used throughout this session), not the Student self-electing "Run as administrator." A Student who *can* self-elevate can also undo the protection, which defeats the anti-tamper goal; the documented future Windows-service plan (SYSTEM account, auto-start, Student talks to it over local IPC, `PROCESS_TERMINATE` denied to standard users) requires the same one-time admin-context install, just packaged as a service registration instead of a scheduled task — not implemented this round, no code changed for it.
- Verified: `cargo test -p classroom-app` 166 passed, both targets compile clean, `flutter analyze` clean, screenshots of the real fixed window on the real Mac Teacher connected to the real `172.16.49.25` Student (extensions populate, Copilot badge now says "Not applied yet" instead of falsely implying active enforcement).
- NOT verified: the force-restart-VS-Code-on-apply behavior against a real already-running VS Code instance (added at the very end of this session); whether VS Code's hot-exit actually preserves a Student's unsaved work through the forced restart in practice; the scenario of a Student opening VS Code fresh *after* a policy was already applied in an earlier session (should work via ordinary startup policy resolution, same as this round's verified case, but not re-tested end to end this round).
- Exact next step: with a real already-open, actively-being-edited VS Code window on `172.16.49.25` (or any Windows Student), apply a policy from the Teacher and confirm (a) VS Code force-restarts within a few seconds, (b) any unsaved buffer is restored via hot-exit, (c) Copilot/unlisted extensions are now actually greyed out/disabled in the restarted window's Extensions view, not just in the policy log.

## 2026-10-07 — New: `ClassroomGuardService`, a Windows Service that keeps the Student running

- Owner's request, explicit and unambiguous: make the Student behave like NetSupport School / endpoint-management or antivirus agents — end-task it and it comes back; find every way to protect it. This is the first concrete piece of the "chống tắt Student" plan `docs/21_GUARD_ARCHITECTURE.md` already listed as planned-not-started; the owner's own standing assumption there (an admin pre-installs/pre-elevates before Students use the machine, Students never self-elevate) is exactly what this implements — the install step genuinely requires an administrator, same as today's elevated-launch model, just made self-healing and much harder to turn off.
- New third bin target `ClassroomGuardService` (`apps/classroom/Cargo.toml`, same `src/main.rs` dispatched by `env!("CARGO_BIN_NAME")` like the existing `Classroom`/`ClassroomStudent` split) and `apps/classroom/src/guard_service.rs`, using the `windows-service` crate (new dependency) for the SCM control-plane boilerplate rather than hand-rolling it. What it does, concretely:
  - `--install [--teacher-ip IP] [--control-port N] …` (run once, elevated, by an admin): copies nothing itself (the admin places `ClassroomGuardService.exe` + `ClassroomStudent.exe` together first), registers a `LocalSystem`, auto-start Windows service; persists the Student's launch arguments next to the service exe so every *future* relaunch — including ones the running service triggers on its own, long after install — reuses them without being told again.
  - Hardens its own service ACL via `sc sdset` with Microsoft's own documented hardened-service SDDL shape: `Authenticated Users` get query/read rights only, `SYSTEM`/`BUILTIN\Administrators` keep full control (including Stop/`WRITE_DAC`) — a standard-user Student gets Access Denied from `sc stop`, Services.msc, PowerShell's `Stop-Service`, etc.
  - Registers (and keeps idempotently re-registering, every time it notices the Student missing — covers a Student who deletes the task) a `RunLevel Highest` / `LogonType Interactive` scheduled task that launches `ClassroomStudent.exe` into the active console session — the exact mechanism already proven this session for manual deploys, now automated and running as whoever is actually logged into the console (queried fresh each time via `query user`, not hard-coded).
  - Supervises every 2 seconds: if `ClassroomStudent.exe` is not running, triggers it again via `schtasks /Run`.
  - Locks the install folder down beyond Program Files' own default protection, as an explicit SYSTEM+Administrators-only allow-list (not a deny-list — see the bug below).
  - `sc failure`-configures the *service itself* to auto-restart if it ever crashes, on top of the scheduled-task-level Student supervision.
  - Deliberately does **not** add a deny-`PROCESS_TERMINATE` ACL to the live Student process (would need hand-rolled `SetSecurityInfo`/`SetEntriesInAclW` FFI — judged too much unsafe surface to get right and verify blind this round for uncertain payoff over the respawn-based approach) — documented in the module doc comment as a known, deliberate limitation, not hidden. A Student *can* still end-task the visible process for a few seconds; the claim is "it always comes back and the thing that brings it back cannot itself be stopped," not literal unkillability.
- Two real bugs found and fixed **through actual deployment on `172.16.49.25`**, not just compiling:
  1. `query.exe user`'s exit code is not a reliable success signal — it is commonly non-zero even when it printed a perfectly good session list (a known quirk of that specific utility). The first install attempt's `active_console_user()` gated on `output.status.success()` and failed every time with "No interactive console user is logged on" despite `query user` printing the right session. Fixed by parsing stdout unconditionally.
  2. `lock_down_folder()`'s first version used `icacls /deny *S-1-5-11` (the well-known SID for "Authenticated Users") on the install folder. Authenticated Users includes administrators — Windows DENY ACEs win over ALLOW regardless of other group memberships — so this silently locked the admin account itself out of `C:\Program Files\Classroom`, including the same admin account needed to reinstall or update it; a `Copy-Item -Force` onto that folder started failing silently with no error ever surfacing (the install script didn't check). Caught by noticing the installed binary's hash never changed across two redeploy attempts despite the staging copy updating correctly. Fixed by replacing the deny-list with an explicit allow-list (`icacls /inheritance:r /grant:r SYSTEM:F /grant:r Administrators:F`, both locale-independent well-known SIDs) — no entry for anyone else means no access for anyone else, and admins are never blocked by their own membership in a broader group again. Had to manually `icacls /reset` the already-locked folder on the real machine before the fixed binary could even be copied into place.
- **Verified end to end on the real machine** (`172.16.49.25`, after fixing both bugs above): service installs, starts, and is visible as `Running`/`Automatic`; `sc sdshow` on the live service matches the intended SDDL exactly; the scheduled task launches `ClassroomStudent.exe` from the protected `C:\Program Files\Classroom` with the persisted `--teacher-ip`/`--control-port` arguments, and the Teacher's own log confirms it actually connects (`Student online`, `Student connected`). **The core ask**: killed the running Student process (`Stop-Process -Force`) — the service noticed and relaunched it within ~4–5 seconds (confirmed via the service's own log timestamps and the Student's new process creation time), and the Teacher's log shows it reconnecting within another ~2 seconds. Also confirmed the service can still be stopped/started by the admin account used for testing (`Stop-Service`/`Start-Service` succeeded), i.e. the hardening does not accidentally lock out an administrator.
- **NOT verified**: whether a genuine *standard* (non-administrator) Windows account is actually denied — the only account available on this test machine (`mvmanh`) is a local administrator, so `Stop-Service`/`sc stop`/deleting the install folder could only be confirmed to *succeed* for an admin, never confirmed to *fail* for a standard user. The SDDL and ACL shapes used are Microsoft's own documented patterns for exactly this purpose, so there is good reason to expect them to hold, but this is "verified against the documented mechanism," not "verified against an actual standard-user token" — same honesty standard as the VS Code policy work earlier. Also not exercised: reboot/logoff-logon behaviour (does the scheduled task correctly pick up a *different* user logging into the console later), multiple Students/service instances, and the interaction between this service and the existing in-Student Guard/VS Code enforcers (they are unchanged and still run inside the elevated Student process itself, not inside the service — this round only adds supervision, it does not move enforcement into the service).
- Verified: `cargo test -p classroom-app` 166 passed, native build and Windows cross-compile (`x86_64-pc-windows-gnu`) both clean for all three bin targets (`Classroom`, `ClassroomStudent`, `ClassroomGuardService`), paired macOS build rebuilt successfully.
- Exact next step: get access to (or have the owner create) a standard, non-administrator Windows account on a test machine and confirm `Stop-Service ClassroomGuardService`, `sc stop`, and writing to the install folder all fail with Access Denied under that account specifically. After that, the next roadmap item from `docs/21_GUARD_ARCHITECTURE.md` is persistent (not dynamic-session) WFP filters so network blocking survives even if this service itself is stopped — not started this round.

## 2026-10-07 — Live with 37 real Windows 10 x64 Students mid-exam: read-only analysis, then three Teacher-only bug fixes

- Owner was running a real exam against 37 real Student machines and was explicit and absolute: read logs and analyze connections only, never show anything on or restart/redeploy any Student — except their own machine `505-GV` (`10.0.55.200`), which has no student on it.
- **Connection analysis (Teacher-side logs + live roster only, zero Student-side interaction)**: all 37 online, 0 offline, 0 stale thumbnails, all 37 on the identical build label (no version drift). 113 "monitoring peer ended" events but 112/113 were the benign `"no active room registration"` reconnect cycle (1 genuine `Connection reset by peer`, inconsequential on 37 machines over ~1 hour). The `round_trip_ms` figures I first flagged as high (~1.5 s p50) are not a problem — traced the metric in `monitoring.rs` to confirm it measures the full `interval_ms`-paced request cycle (default 2000 ms), not network latency; the numbers match that config value, as expected. Confirmed the Student-side diagnostics-collection feature (`collect_diagnostics`) is a silent background log upload with no Student-visible UI and that it auto-skips while a Student is being presented to — but it was not actually used this round; the live roster/Teacher-log read alone answered the ask without sending anything to the 37 machines at all.
- **505-GV Observe failure** (`"No route to host"`, owner-authorized machine): ping and the regular control/tools channel both work fine for this machine; only the Teacher-initiated Observe connection (into a port the Student listens on) fails, which on a healthy LAN link almost always means the Student's Windows Firewall is rejecting that specific inbound port rather than a real routing problem. Not yet fixed — needs direct access to that machine to inspect/correct the firewall rule; asked the owner for SSH credentials the same way as the earlier `172.16.49.25` session.
- **Three Teacher-only bugs reported by the owner, all found and fixed with hard evidence, not guesses**:
  1. **VS Code allowlist and Internet "Allow only listed sites" forgot their state on every reopen.** Root-caused precisely: `agent.preferences` is only populated once `Agent.start()`'s disk read completes, which happens *after* `main()`'s `runApp()`/first-frame sequencing — so `VsCodeWindow`/`InternetWindow` reading `agent.preferences` directly in `initState()` (or in `InternetWindow`'s `late` field initializer for `lists`) **always** saw the empty default, 100% reproducible, never actually a race in practice. Fixed by deferring that read to the same `agent.connected`-gated path already used for the network-fetch race fixed earlier in the session (`applyPreferences()` called once connected, not at construction). Separately and additionally, read the live `console-settings.json` directly and found `internet_lists`, `vscode_allowed` and `vscode_enabled` persisted as empty/default — confirmed a **second, independent bug**: `Agent.savePreferences()` serialized *this process's own in-memory* `preferences` map to disk, and since the main console and every dependent window are separate OS processes each loading that file once at their own startup with no lock, a long-running console's stale in-memory copy would silently overwrite whatever a short-lived dependent window had just written. Fixed by re-reading the file fresh and merging `changes` onto *that* immediately before every write, not onto the caller's possibly-stale in-memory copy.
  2. **VS Code windows were misclassified as the main console.** `AppDelegate.swift`'s `isObserve` and `Agent.dependent` both listed `--observe-id`/`--files-id`/`--activity-id`/`--internet-ids` but not `--vscode-ids` — added it to both; without it, a VS Code window's own `applicationShouldHandleReopen`/focus-forwarding behaved as if it were the main console.
  3. **Internet access and VS Code windows never came forward when reopened while already open** — unlike Observe/Files/Activity, their Teacher-agent handlers (`internet_window`/`vscode_window`) only ever tried to spawn a new process, with no "already open → bring it forward" branch and no `observe_focus` bump at all, so clicking the toolbar icon again did nothing visible if the window was hidden behind the console. Added the same existing-window branch and `observe_focus`/`pendingObserveFocus` wiring (`"internet"`/`"vscode"` literal keys, since these windows aren't scoped to one Student the way Files/Activity are) that Files/Activity already had.
  - Investigated the owner's third report (console "chiếm priority", windows not coming to front) further: the underlying cross-process activation mechanism (`classroom/observe` → `activateWindow` → `NSRunningApplication.activate`) already existed for Observe specifically and looks correct by inspection (standard, well-documented API); found no periodic/competing focus-stealing code anywhere in the native layer. Fixed the two confirmed, concrete gaps above; **did not** attempt a deeper blind fix of the Observe-specific case beyond that, since it was already using the mechanism and I have no way to interactively verify a macOS window-manager fix without live testing — flagged honestly as unresolved rather than guessed at.
- **Process discipline**: before touching anything, re-verified zero risk to Students — `collect_diagnostics` confirmed silent/no-UI by reading its implementation before considering it (ultimately not used); confirmed only the Teacher Agent process needed restarting (a stray second console process from an earlier session had already exited on its own); checked `37/37 online` immediately before and after the restart to prove the fix didn't disrupt a single Student mid-exam.
- Verified: `cargo test -p classroom-app` 166 passed, both Rust targets clean, `flutter analyze` clean, paired macOS build rebuilt and the Teacher Agent restarted — confirmed 37/37 Students reconnected cleanly within seconds, 0 offline before or after.
- NOT verified: that the VS Code/Internet windows actually remember a real Apply now (no Apply has happened since the fix deployed — the owner needs to Apply once and reopen to confirm in their own live workflow); whether the Observe-to-front behavior is now any better in practice; 505-GV's firewall still unfixed, pending SSH access.
- Exact next step: get SSH credentials for `10.0.55.200` to diagnose/fix the Observe firewall rule; once the owner applies an Internet/VS Code policy again, confirm the window now shows it checked/selected on reopen; watch for further owner feedback on window-focus behavior before attempting anything deeper there.

## 2026-10-07 — Silent Student connection loss (source + unit tests only)

- Owner requested no Student popup on Teacher connection loss during a real exam; explicitly prohibited application rebuild/package/deploy and interaction with running classroom endpoints.
- Student ConnectionStatus now returns before native window creation for every non-connected status, including initial connecting and reconnect retries, regardless of persisted popup preference. Connection loss releases an existing confirmation and clears its timer/draw/dismissal state. Optional Connected confirmation retains the existing three-second behavior.
- Teacher reporting remains intact: new pure unit test verifies tools disconnect removes the peer from ready and queues an activity alert when activity monitoring is enabled, without duplicate alerts. No live classroom IPC/network/process actions were performed.
- Verified locally: student_connection_ui tests 3 passed (2 new + existing placement test); Teacher disconnect reporting test 1 passed. Commands: cargo test -p classroom-app --bin Classroom student_connection_ui::tests --offline; cargo test -p classroom-app --bin Classroom disconnect_still_reports_offline_and_alerts_to_teacher --offline. These compile/run only filtered unit-test harnesses, not portable/release apps. rustfmt --check --edition 2021 apps/classroom/src/student_connection_ui.rs and scoped git diff --check passed. Workspace cargo fmt --all -- --check fails on pre-existing formatting across unrelated files; no broad reformat applied.
- Updated docs/20_LAN_STUDENT_BOOTSTRAP.md to reflect silent connecting/reconnecting. No release build, packaging, deploy, app restart or runtime UI validation. Existing classroom binaries have not received this source change.
- Exact next step: after the exam and explicit owner authorization, build the paired Teacher/Student package and validate native disconnect/reconnect on an isolated test endpoint before deployment.

## 2026-10-07 — Teacher CPU adaptation / UDP pacing; compatible Teacher-only restart

- Owner authorized Teacher/Student source changes and a new Teacher build/run, conditional on compatibility with the old Students taking a real exam. No Student deployment or active classroom test was authorized. Updated Teacher protocol remains 8, IPC remains 1; protocol/control/media transport crates have no diff. No new Student command or wire field is needed.
- Fixed Teacher adaptation input to normalized host-capacity CPU percent while preserving the summed-core CPU log. Encode latency still independently detects serial encoder pressure. Suppressed bitrate recovery during local pressure. Added applied/proposed bitrate reasons, refinement deferral/pressure ceiling, and per-receiver counter-derived cadence/window logs.
- Changed fan-out pacing to individual datagrams across recipients with nominal burst budget divided across workers (formerly each worker had its own ~16 KiB burst). Kept existing pacing span and bounded queues. Excluded measured intentional pacing wait from the 10 ms send-work retry deadline; active retry remains capped at 2 ms per worker/AU. Added deadline/budget exhaustion counters for fan-out and repair; repair also exposes WouldBlock. This does not prove all observed send errors fixed, and repair/pointer still lack a shared aggregate governor.
- Verification: 41 focused unit tests passed: quality 23, UDP budget/pacing 6, CPU normalization 1, fanout 4 (manual benchmark ignored), repair 2, popup 3, Teacher disconnect alert 1, protocol 1. Network unit tests used ephemeral 127.0.0.1 only. Scoped rustfmt and git diff --check passed. Clippy completed exit 0 with existing project/vendor warnings (79 app warnings); workspace formatting has pre-existing unrelated differences. No opt-in benchmark, Student executable, screen-share, Observe, remote input, file job or updater test was launched.
- Paired MSVC x64 Rust/Flutter build label teacher-adaptive-20261007 completed in isolated target/Classroom-Teacher-adaptive-20261007, fresh Student ZIP included. SHA256 Teacher helper 926c647d7f3aa1ed09fbdb2d5f9b2e6200f7a2377731ebc98cae7e6f127f5ea1; Student aadb6aba59460f2a8faff0ff0eeafff3c592a252b90834b616cc127ee647fe9c; bundled ZIP b5a5b4cee584b1816af364b84324dde69290d1d112df132efbd1697ab96c37ff. Copies match build outputs. Build script now supports explicit OutputDirectory so a running Teacher bundle is not overwritten.
- Read-only IPC baseline: Teacher idle, 36 online, activity monitoring false, connection popup false, all Student builds 1791341267-paired-20261007-094727. Issued owner-authorized graceful Teacher quit only after checking idle/no Observe, then launched new isolated Teacher with --bind-ip 10.0.55.10. Old GUI/helper exited normally. New Teacher GUI PID 12064, helper 3764, build_id 1791347790-teacher-adaptive-20261007; TCP ports 47800/47801/47802 bind to 10.0.55.10.
- Startup initially showed 0 online. New executable path had a separate Windows Firewall approval context; later read-only netsh inspection shows Private allow/Public block for the new path. Agent did not change Firewall policy or elevate. A proposed local old-bundle replacement was automatically rejected before execution because it included removal of old build files; this limitation was announced and the isolated new bundle retained. No deletion/replacement happened after rejection, and no running binary was overwritten.
- Runtime compatibility verified by passive Teacher state: all 36 previous Student IDs returned, no missing IDs, and all still on old build 1791341267-paired-20261007-094727. New helper ready at unix_ms 1791347891273; first Student online 1791347942615 (~51.3 s), last 1791347960196 (~68.9 s). Do not characterize this as a seamless/instant reconnect. Teacher remains idle; no screen sharing was initiated by the agent. Existing settings/profile/logs retained. Old Student popup behavior remains until a separately authorized update.
- Exact next step: analyze a later owner-initiated share to the unoccupied selected machines, comparing normalized CPU/pressure reasons, sender/repair exhaustion and receiver cadence with the 16-machine baseline. Media/latency improvement and 40-PC acceptance remain unverified. Keep Student deployments/tests prohibited during the exam.
- Checkpoint commit attempted but Git refused because user.name/user.email are unset. No author identity was invented; source/checkpoint changes remain staged for an owner-configured commit.

## 2026-10-07 — Real LAN 1080p / 35 Student: two owner-run workloads

- Read-only analysis of streams 8994680672841960415 (94.925 s, 11:47:45–11:49:20 +07) and 8994680672841960416 (37.449 s, 11:49:25–11:50:03 +07). Owner clarified first run included YouTube video on Teacher; second was basic desktop actions without video. These are different workloads, not a same-workload A/B. Both used actual 1920×1080, Intel Quick Sync, one encoder and 35 Windows receiver endpoints. Local physical Ethernet reports 1 Gbps. Detailed evidence and per-Student tables: checkpoints/diagnostics/2026-10-07-35-student-real-lan-performance.md.
- Teacher encode 28.68 / 26.94 FPS, p95 13.17 / 12.98 ms, normalized CPU median 3.24 / 3.62%; no CPU/encode pressure. Video run receiver render 16.47–17.33 FPS with counter decode stalls up to 15.015–16.090 s per endpoint; basic-actions run 25.38–25.82 FPS and no zero-decode report interval. First run abandoned 316,730 recipient datagrams (13.60% sent+abandoned), 18,156 send errors and 56,623 / 58,198 NACK cache misses. Second run abandoned 5,795 (0.67%), 245 send errors, no cache miss but 1,088 repair send errors. UDP counters continue during widespread decode stalls; this supports transport/reference recovery failure over a stopped encoder, without Student-local decoder proof.
- Source confirms pacing span clamped to 8 ms despite the largest AU × 35 recipients requiring >=101 ms payload serialization at 1 Gbps. Shared aggregate pacing including repair/pointer, AU freshness/byte admission, bounded reference recovery and NACK/cache-age telemetry are priorities; do not just enlarge retry/queue/jitter buffers. Average Mbps does not establish burst safety. No 40-endpoint/optical-latency acceptance claim.
- No code changes, executable tests, build, app restart or deployment performed for this analysis; wrote report/local analysis artifacts only.
- **Owner authorization changed after the exam ended:** future newly built Students should be deployed to the classroom machines as well, so Teacher and Student can be tested at the same fresh revision. The previous exam-time prohibition is lifted. Continue paired build/bundled ZIP rule and use Classroom updater, preserving settings/device IDs/logs; verify build ID for each reachable endpoint and report unavailable machines. Do not treat authorization as evidence deployment has already happened.
- Exact next step: implement sender aggregate pacing / AU budget and instrument recovery with bounded semantics, build paired roles, deploy Student through Classroom updater under the new authorization, then compare fixed basic-actions/video workloads and verify actual endpoint builds before performance claims. Read all project documents before coding. Existing Git identity limitation remains; no checkpoint commit attempted again.

## 2026-10-07 — Sender/recovery implementation plan (owner approved)

- Evidence/decision before implementation: the 8 ms pacing clamp cannot serialize the observed 35-recipient IDR on 1 GbE. Replace it with one session aggregate rate governor shared by video, repair and pointer, counting conservative wire overhead. Keep bounded wall-time AU admission (120 ms normal / 300 ms IDR initially, configurable), bounded retry and no retry queue. Failed endpoint chains wait for a successful IDR, with a coalesced request to the encoder owner.
- Fan-out workers remain off the encode thread. Wait for the previous AU to finish before acquiring the next latest capture, removing the extra encoded AU backlog during large IDRs. This deliberately trades encode/send overlap for a measured bounded stage; normal 6 Mbps ×35 pacing at 400 Mbps plus ~13 ms encode is close to a 33 ms tick. Log the wait and verify cadence; adjust from evidence, never restore burst-clamping to conceal pressure.
- Receiver recovery will abandon old gap repair state on unrecoverable loss and re-anchor on parameter-set/IDR-start packets, rather than serially requesting expired gaps. Instrument recovery wait, packet discards, cache age/eviction reasons and AU completion. Limit refinement by fan-out bandwidth and suppress it under pressure. No protocol/schema change planned.
- Files expected: udp_sender.rs, fanout.rs, teacher_repair.rs, pointer_channel.rs, main.rs, quality.rs, refinement.rs, classroom-config, config/default.toml and corresponding docs. Build paired roles into a new isolated Teacher package; update Students through the existing Classroom updater; preserve identity/settings/logs. User authorized deployment after exam completion.

## 2026-10-07 ? Sender/recovery source milestone verified; paired build pending

- Implemented session-wide aggregate wire pacing for video/repair/pointer and every
  socket retry; removed the per-AU 8 ms acceleration. Added wire-time preflight and
  configurable 120 ms normal / 300 ms keyframe send deadlines. Workers retain bounded
  channels; publisher waits for prior AU completion before acquiring latest capture.
  Endpoint send failures suppress dependent frames until a successful IDR and schedule
  recovery on the encoder owner. Capture/encode/packetize still occur once.
- Adaptive now considers sender abandonment and actual encoded Mbps versus per-recipient
  wire headroom. Refinement is capped by bandwidth/video limits and suppressed during
  pressure/pending recovery. Default storm coalescing ceiling reduced 2 s ?800 ms;
  Student request throttle 500?250 ms. No wire/schema/port/version change (protocol 8).
- Student clears expired repair/reference state on the first unrecoverable gap and
  anchors recovery at parameter-set/IDR-start packets instead of repeatedly chasing
  stale P-frame gaps. Existing silent disconnect fix is included in fresh Student.
- Added bounded eviction metadata and expired/capacity/invalid/unknown cache-miss
  counters, NACK packet age, stale repair counters, aggregate wait/denial, endpoint
  AU deadline/reference skips, worker AU/keyframe completion and Student waiting-IDR
  metrics. TTL/cache capacity/jitter/ingress limits are unchanged.
- Local verification: 46 filtered Classroom tests passed (UDP 6, fan-out 4, repair 2,
  pointer 2, quality 23, refinement 2, keyframe gate 3, local reference expiration 1,
  silent popup 3), one existing manual benchmark ignored. Config 6, transport 20 and
  protocol 1 passed: total 73 distinct tests. Existing test cases were extended for
  real rate/deadline math, rejected AU/reference suppression, sender pressure,
  refinement cancellation, cache reason and H.264/H.265 recovery anchors. All sockets
  were ephemeral localhost; no screen share or Student deployment yet. Evidence
  target/lan-pacing-20261007/test-*.txt and transport-config-tests.txt.
- cargo check -p classroom-app --bins --offline and cargo clippy -p classroom-app
  --bin Classroom --offline passed (existing warnings retained). Scoped rustfmt with
  skip_children=true and git diff --check passed; no broad formatting of other modules.
- Read-only Teacher preflight: idle/no Observe, 35 online compatible Windows x64 Students,
  all old build 1791341267-paired-20261007-094727; 505-25 /10.0.55.125 offline. Snapshot
  saved without IPC token to target/lan-pacing-20261007/baseline.json.
- Exact next step: paired fresh MSVC/Flutter build, graceful Teacher replacement at
  verified owned paths, then Classroom updater deployment and per-endpoint receipts.
  No physical post-fix performance result yet. Global pacing may reduce cadence or
  shed over-budget AUs; compare controlled video/basic workloads after deployment.


## 2026-10-07 - Paired LAN deployment and minimal runtime checkpoint

- Paired build lan-paced-20261007 succeeded; fresh Student ZIP embedded. Teacher
  restarted at existing approved path target/Classroom-Teacher-adaptive-20261007,
  bind 10.0.55.10. Classroom updater completed 35/35 online Students; all verified
  build 1791350009-lan-paced-20261007 with baseline IDs. 505-25 was offline/unupdated.
- Owner shortened runtime testing due to lab departure: 10-second desktop smoke
  reached all 35 receivers (decoded 158; rendered 152-158 at last reports), stopped
  cleanly, Teacher idle. Zero socket errors/cache misses; 23,415 abandoned recipient
  datagrams and AU admission/reference skips remain. Encode 22.42 FPS, p95 13.88 ms.
  Functional smoke only; smooth video improvement and 40-PC acceptance unverified.
- Full record: checkpoints/diagnostics/2026-10-07-lan-pacing-deployment.md.
  Native workload foreground precheck prevented an earlier attempted benchmark
  before any Share. No further build/long test after owner's final instruction.
- Owner requested repository cleanup and local-only commit. Archive raw evidence
  outside repo, remove generated/temp/duplicate packages, preserve active ignored
  Teacher bundle and tracked source/config/lockfiles. No push; owner Git identity
  requested because repository/global user.name and user.email are unset.
- Exact next step: controlled same-workload video/basic comparison concentrating
  on remaining AU shedding and serialized encode/send cadence when lab time allows.

- Final owner steering: no rebuild; commit waived (Git identity unset). Teacher
  binary removal requested: graceful quit from idle, GUI/helper exited. Automatic
  review rejected recursive deletion; generated output and stopped Teacher bundle
  moved outside repo to ../netsupport-school-local-evidence-20261007 using checked
  paths. No target or ignored generated output remains in repo. External archive
  was not deleted. Source/config/reproducible lockfile/reports retained. No push.

## 2026-10-07 — Local transport probe and Windows ARM VM recovery

- Owner authorized coding/testing on this Mac and Windows SSH endpoint 172.20.10.3 after leaving the lab. Added standalone diagnostic transport_probe using production Fanout/AggregatePacer, a sequential sink ladder runner, an isolated interactive Windows recovery runner, and an analysis script. Production media algorithms/defaults unchanged. Detailed durable evidence: checkpoints/diagnostics/2026-10-07-local-transport-recovery.md.
- Confirmed Windows 11 ARM64; x64 executables run under emulation. Synthetic loopback tests are not physical 40-PC acceptance. Fixed 361,003-byte IDR ×40 plus conservative framing requires ~308.07 ms at 400 Mbps, exceeding current 300 ms admission; fixed-size recovery in the harness repeats rejection. At35 the IDR took ~269 ms. Final quiet 300-AU repeats:35 sinks with361KB IDR27.09 submission FPS,40 with200KB IDR25.74; all300 AUs received per sink, no abandoned/deadline/WouldBlock. Simulated13ms encode is not actual encoder performance; harness does not model production adaptation or keyframe coalescing.
- Paired local-recovery-20261007 Mac Teacher/Mac Student/Windows x64 Student/Guard build succeeded; fresh matching ZIPs embedded. Explicit rustup RUSTC/RUSTDOC resolved Homebrew compiler/Windows target mismatch; approved host execution allowed Flutter SDK cache. Local Flutter transitive resolution preserved separately under target/local-recovery-build; tracked pubspec.lock restored to baseline.
- Raw CLI capture first failed TCC; direct sandbox-inherited helper launch also fails without Flutter parent. Owner manually granted exact helper and parent bundle paths, then GUI/helper restarted. Connected fan-out Mac sends produced Broken pipe with0 received frames. Temporary connected_fanout=false diagnostic A/B through shared socket worked; original stored settings restored byte-for-byte immediately after startup and checked afterward. Underlying connected-socket cause remains unproven; do not attribute physical Windows lab loss to it.
- Three50-second Mac Teacher→visible Windows VM tests at actual1512x982: baseline1349decoded/1330rendered,27.48/27.09FPS; configured probabilistic1% ingress-loss1438/1391,28.99/28.04FPS; latejoin1204/1169,28.21/27.39FPS. All0decode errors and0zero-decode report intervals; accepted-IDR recovery peak143.15/144.17/110.05ms, not visible/optical latency. Teacher NACK requested/resent39/39,95/95,21/21;0cache misses/repair send errors. Foreground workload not independently audited and output0.63/0.79/1.60Mbps; no6Mbps/1080p stress pass. Injection counter is not currently logged, so achieved loss percentage is not independently measured.
- Native Windows local-media20-second smoke: DXGI/software H264 MFT/Microsoft decoder/D3D11,1510x948,355encoded/decoded/rendered,17.75FPS,177schedule overruns. Encode/decode/render p95 18.41/9.67/0.92ms. Functional VM backend evidence, not30FPS or physical x64 hardware acceptance.
- Verification: cargo test -p classroom-app --bin transport_probe --offline (11passed/1ignored); cargo test -p media-transport --offline (20passed); harness clippy exit0 without harness warnings; scoped rustfmt, Python py_compile and git diff --check passed. Workspace cargo fmt --all -- --check fails pre-existing unrelated formatting (activity/collector.rs onward); no broad reformat. Raw test/build logs are ignored in target/transport-probe and target/local-recovery-*.
- Cleanup: Teacher GUI/helper and test scheduled tasks/processes stopped. Installed Windows Guard PID3248 and Student PID5456 remain at original Program Files paths; no replacement, firewall or privacy automation. Failed attempts retained.
- Exact next step: measure bounded behavior when IDR itself exceeds AU wire admission; include adaptive/coalescing and explicit visible-recovery phases/injected-drop counts. Investigate Mac connected UDP Broken pipe separately. Physical lab same-workload1080p 1/5/10/20/35/40 and optical/soak acceptance remains required. No push from this task.


## 2026-10-07 — Oversized IDR feedback, Mac UDP fallback and longer VM tests

- Owner authorized steps 1–3 and asked to defer/note physical 35–40-PC tests. Implemented coalesced rejected-IDR rate hint to the single encoder owner, immediate adaptive bitrate reduction respecting minimum/opt-out/backend rejection, refinement cancellation and existing keyframe gate. Kept 400 Mbps aggregate /120/300 ms defaults, bounded slots and one capture/encode/packetize. Added preflight/fallback/injection counters.
- macOS EPIPE on established connected UDP retires only that endpoint socket and retries the unsent datagram once via shared socket under same pacing/deadline. Windows path unchanged. Default connected_fanout=true now runtime-functional in three VM sessions; each had one fallback then zero RTP send errors/abandonment. Kernel/sandbox root cause is not claimed.
- Synthetic real policy/gate plus proportional-size model: 40×361KB and35×600KB initial IDRs rejected once, next IDR finishes ~659 ms, all168 admissible AUs received per sink out of180 submissions, zero partial/duplicate/send errors. Fixed 40×361KB negative control still rejects all60. Model is not an actual codec or physical scaling proof.
- Fresh paired recovery-budget-20261007 Mac Teacher/Mac Student/Windows x64 Student+Guard with fresh embedded ZIPs; manifests/hashes verified. Extra Windows diagnostic role built from same production source/label. Flutter locally resolved lock preserved under ignored target; tracked lock restored. Owner manually regranted exact app/helper Screen Recording after signing. First TCC-denied and low-foreground attempts retained, not treated as benchmark passes.
- Corrected three180-second shares, actual1512×982, one VideoToolbox encoder and Windows ARM VM x64-emulated decoder/D3D11. Foreground/visible audit100%, all A–E phases. Baseline29.22/28.80 decode/render FPS, zero codec errors/zero-decode intervals, IDR arrival peak208.53 ms. Loss25.78/24.88FPS, zero codec errors but4 zero-decode intervals, peak910.82ms;352 injected drops/38903 accepted peer datagrams(~0.905%, includes pointer denominator). Latejoin28.94/28.58FPS, zero codec errors/zero-decode intervals, peak192.01ms. Teacher NACK all requested resent50/50,343/343,68/68; no misses/repair send errors. Output1.63/1.73/1.60Mbps, not6Mbps/1080p acceptance or old-build same-frame A/B.
- Forced 40Mbps/10ms real-encoder negative test accepted6→2.1→1.5Mbps, but IDRs at floor often still unsendable:49 zero-decode intervals/29.43s recovery peak. This explicitly fails smooth recovery under that artificial constraint; bounded queues alone are not a pass. Settings restored byte-for-byte. Do not claim guaranteed bitrate→IDR-size proportionality. Real Windows hardware/software encoder floor behavior requires lab measurement.
- Focused checks62 distinct tests passed (42 imported probe +20 media-transport; one manual ignored), Classroom all-bin check/clippy and probe clippy completed with existing/imported warnings. Scoped rustfmt, Swift compile, Python compile, diff check passed. Only regression-test code/diagnostic scripts added after production paired build; no later runtime implementation change.
- Cleanup verified original installed Windows Guard/Student retained, no recovery tasks/test processes and Mac GUI/helper/workload stopped. Detailed report/checklist: checkpoints/diagnostics/2026-10-07-recovery-budget-followup.md; raw evidence target/recovery-budget-*.
- Exact next step: investigate four loss-run stalls by aligning ingress loss, repair expiry, keyframe gate, decode/presentation events; preserve bounded buffers. In physical lab run identical visible workloads at actual1080p ladder1/5/10/20/35/40, real IDR floor/hardware/software cases, optical latency,30-minute fixed-resolution soak, sleep/resume/DPI. None of those physical gates is passed here. No push.

## 2026-10-07 — Confirm recovery by decoded output

- Corrected prior loss interpretation: four zero-decode intervals were startup
  before first output, not four freezes after playback began. Original missing-SPS
  cause remains unproven because raw NALs were not retained.
- Reject incomplete parameter-set/random-access recovery AUs; request a fresh IDR.
  Add bounded decoder no-output watchdog and decoded-output recovery telemetry.
  Preserve existing queue, jitter, repair and keyframe request budgets.
-70 focused tests passed; check/clippy/scoped formatting/Python checks complete.
  Fresh paired Mac Teacher/Student and Windows x64 Student+Guard built with label
  decoded-recovery-20261007; embedded packages/hashes verified.
- Windows VM headless loopback: first SPS drop recovered to decoded output922ms,
  first PPS drop157ms; zero decode errors.1510×948 static desktop, about5FPS.
  Existing31a0e74 CLI Teacher + fresh Student: compatibility/component validation,
  not same-revision native Windows release or visible-render acceptance.
- Mac Teacher live run blocked by Screen Recording after signing; owner asked for
  exact parent/helper paths and restart. No privacy grants automated.
- Next: after owner regrant, restart exact paired Mac bundle and run visible
  Mac→Windows SPS/PPS plus longer loss/late-join cases. Then physical Windows x64
  wired1080p ladder1/5/10/20/35/40, optical latency, fixed-resolution soak,
  DPI125%/150% and real sleep/resume. These lab gates remain unverified.
- Detailed evidence: checkpoints/diagnostics/2026-10-07-decoded-recovery.md.

## 2026-10-07 — Visible recovery follow-up completed

- Restarted same paired Mac Teacher; Screen Recording now works, Windows IP
  192.168.50.215. No product rebuild or automatic privacy changes.
- Visible1512×982 tests: deterministic first SPS/PPS drop50seconds each; incomplete
  recovery rejected, first decode696/220ms, render26.94/27.38FPS, decoder errors0.
-120seconds each: baseline29.12decode/28.47renderFPS, latejoin29.04/28.35FPS,
  decoder errors0 and no post-first-output one-second zero-decode intervals.
-1% deterministic random loss measured0.9763%:25.61decode/24.36renderFPS,
  decoder errors0 but two zero-decode intervals, recovery peak1674ms. Waiting for
  complete recovery IDR during those gaps; two incomplete AUs rejected. No claim
  that smooth loss handling is solved. No runtime watchdog trigger.
- Audited workload all five phases foreground/visible100%. Teacher oneencoder;
  all runs send errors/abandoned/cache misses0. Actual1.57–2.73Mbps,6Mbps initial
  target; dimensions below1080p, VM/x64 emulation on shared Mac, not wired lab.
- Corrected diagnostic log collection: another Console rotated shared log; recover
  matching source/port/time session from archive instead of copying stale idle log.
  Python compilation/diff check complete. Full raw evidence retained under target.
- Next: identical-seed/workload comparisons of recovery parameter/IDR loss and
  request gate/repair timing. Physical1080p ladder1/5/10/20/35/40, optical latency,
  fixed-resolution soak, DPI125/150%, real sleep/resume remain deferred.
- Report: checkpoints/diagnostics/2026-10-07-decoded-recovery.md.

## 2026-10-07 — Second-NACK timing A/B; candidate not promoted

- Added optional RTT-aware retry ceiling to jitter, without changing gap expiry,
  queues/cache or one-retry limit.23transport tests incl3timing/deadline regressions,
  4decode-health tests, all-bin check/clippy/scoped formatting/Python checks pass.
- Fresh paired early-nack-20261007: A/B120seconds, same source/seed/workload,
 1512×982 on Mac→Windows ARM VM. Half-budget control125ms vsRTT25ms retry.
  Render26.57vs25.83FPS, recovery peak889vs942ms, decode errors0, no post-first
  output zero intervals in either. Control cold-start5second liveness timeout and
  reconnect; quality/bitrate varied under adaptation. No demonstrated speedup.
- Candidate baseline120seconds27.61renderFPS, no decode error/zero intervals,
  single clean session. Original earlier two incomplete-parameter gaps were not
  reproduced; second NACK does not repair an unknown leading sequence.
- Keep production half-budget default. RTT mode is disabled test-only env
  CLASSROOM_TEST_NACK_RETRY_RTT=1. Analyzer exposes session starts/completions,
  liveness timeouts and actual retry delays to preserve comparison limitations.
- Rebuilt final paired nack-retry-experiment-20261007 inclfresh Student ZIPs/hash
  checks. Final visible default-policy smoke blocked by new signing/TCC; owner
  asked for exact full parent/helper paths and manual regrant. No privacy changes.
- Exact next step: after regrant restart same bundle and rerun default loss50s;
  then controlled middle-IDR+first-repair loss, fixed-quality repetitions and
  separate cold-start diagnosis. Physical1080p x64 wired35/40/optical/soak/DPI/
  sleep remain deferred to lab. Report: diagnostics/2026-10-07-early-nack-retry.md.

- Owner regranted Screen Recording; same final bundle restarted, no rebuild.
  Default50second loss smoke now decoded/presented successfully, one clean session,
  policy half-budget125ms verified.24.31decode/23.22renderFPS, no decode errors,
  but3incomplete recovery AUs, two zero intervals,2689ms recovery peak. Functional
  verification only; loss smoothness is still unresolved. Raw
  target/nack-retry-final-default-regrant-20261007.

- Final opt-in RTT50second run also verified: policy RTT25ms, one clean session,
  28.73decode/27.32renderFPS, no decode error/zero intervals/incomplete AUs;
  recovery peak1259ms. This shorter trial looks better than default50s, but120s
  comparison did not. Different loss positions and adaptive quality prevent a
  causal speedup claim. RTT stays opt-in; controlled repair-loss repetitions are
  the next experiment. Both final policies runtime-verified; TCC block resolved.

## 2026-10-07 — Controlled first-repair loss verified twice per policy

- Added disabled-by-default bounded hook dropping original and first repair of
  one middle H.264 IDR FU-A, accepting third copy; keyed SSRC/timestamp/sequence,
  no rearm/queue. Analyzer verifies three events, exactly2drops/1accepted,
  one clean session and decode/render. Runner excludes random/parameter loss.
- Paired controlled-repair-20261007 rebuilt Teacher+Mac/Windows Students+Guard,
  embedded hashes/executables verified. No privacy automation/regrant required
  this build. Installed Student/settings preserved; tracked Flutter lock restored.
-32focused tests passed (ingress5, transport23, decoder-health4); all-bin clippy
  completes, one new function-length warning recorded alongside existing warnings.
  Scoped formatting/Python/diff checks complete.
- Four50second Mac→Windows ARM VM runs,1512×982, five-phase foreground/visible100%.
  Half-budget repair-copy wait129.592/132.989ms; RTT29.116/24.687ms. Targeted wait
  reduced100.476/108.302ms; all decode/rendered without errors or post-first-output
  zero intervals, render28.18–28.89FPS. This is packet repair timing, not optical
  latency or proof all freezes solved. All Teacher send errors/abandoned/cache
  misses0, oneencoder, target6Mbps with no bitrate changes.
- RTT remains opt-in. Exact next: isolate missing leading parameter after reset
  on available VM, separate cold-start timeout; physical x64 controlled-repair
  tests/high RTT/CPU and wired1080p1/5/10/20/35/40 simultaneous-fault/optical/soak/
  DPI/sleep tests remain deferred. Full report: diagnostics/2026-10-07-controlled-repair.md.

## 2026-10-08 — Defender investigation and first-connection-only Student popup

Owner reports Windows 10 Student quarantine and disconnect only during Show;
original affected machines/sample/threat name unavailable. Read-only SSH at new
Windows .106 confirms Defender/realtime on and latest existing test EXE NotSigned;
no detection/event records returned in queried history. No AV policy changes or
false-positive submissions. See diagnostics/2026-10-08-defender-and-student-status.md
for signing/submission plan, disconnect hypotheses and exact lab evidence needed.
Added read-only scripts/collect-windows-defender.ps1 (full script runtime deferred).
Fixed ConnectionStatus latch: Connected only on first connection per app run;
reconnect/Stop cannot rearm popup. Policy handlers remain silent. Five focused
UI tests pass, targeted rustfmt/diff check pass, clippy passes with warnings;
workspace fmt finds unrelated existing formatting differences. No paired release
build/deployment this milestone, no claim Defender or room disconnect resolved.
Next: obtain original Windows 10 Defender event/sample; paired build and visible
Windows popup reconnect/Show/Stop validation, then exact lab regression.

## 2026-10-08 — Student broadcast / receiver health / diagnostics work in progress

Owner authorizes these features and asks to reuse established Teacher media path.
Implemented Student single-encoder source → compressed RTP Teacher relay → existing
class receiver path; source excluded, late joins included. Added session-scoped
receiver progress states and manual diagnostic collection UI with queue-until-Stop.
Paired Mac/Windows Student + Guard build succeeds, bundled Windows hash matches.
189 Classroom tests pass (2 ignored), 13 control tests and 1 protocol test pass.
Latest Flutter analysis has 2 informational braces lints; clippy completes with
warnings. Native verification incomplete: one trial starts source but relay drops;
latest exact-path trials fail reverse TCP EHOSTUNREACH while Terminal reaches same
endpoint. Local Network attribution after ad-hoc rebuild is a hypothesis; owner
asked to refresh grants manually and Teacher must restart. No privacy automation.
Next: rerun same bundle after grant, diagnose original source-control disconnect,
verify render/latejoin/reconnect/Stop/restart/ZIP, then final paired build.
35–40 physical Windows x64/Ethernet workload, latency, soak/DPI/sleep and Defender
reproduction remain deferred. Full evidence/limits:
diagnostics/2026-10-08-student-broadcast.md.

- Owner refreshed Local Network: reverse TCP opens on unchanged bundle; native
  relay then fails UDP EHOSTUNREACH after receiver joins (not yet localized).
  Added per-stage contexts, first-RTP/route logs and existing Teacher-style
  nonfatal class-route receive handling. Paired build/tests pass again, but new
  signature again precedes reverse TCP EHOSTUNREACH; Terminal exact-port probe
  succeeds. Four focused Flutter tests pass; lock restored. Next is manual
  permission refresh and same-bundle rerun without rebuild. Feature remains
  incomplete/runtime-unverified; latest evidence native-regrant/native-route.


## 2026-10-08 — macOS permission onboarding and grouped build

Owner requested first-launch checklist with reasons/status/buttons and batching
macOS builds. Added startup/toolbar dialog, process-specific Console/Agent screen
permission queries/requests, LAN probe from both identities, Settings/Finder/full
paths, Continue and idle-only explicit helper + GUI restart. No privacy grant,
reset, old-item deletion, paid signing or notarization automation. LAN success is
endpoint-specific connectivity evidence, never a claimed general policy readout.
PROJECT_NOTES and docs/22_MACOS_PERMISSION_ONBOARDING.md record this workflow.

Final grouped paired ad-hoc build succeeds; Mac/Windows Student + Guard fresh ZIPs
are embedded and hashes match. 194 Rust tests pass (2 ignored), focused Flutter
11 pass, analyzer clean, diff check clean. Six unchanged Observe layout failures
reproduce separately; broad suite must not be described as passed.

Native popup renders, banner reopens it, Screen Recording button opens the right
Settings page. Native LAN test .106:22 correctly shows Console timeout and Agent
EHOSTUNREACH as unverified, with no observed Allow dialog. Actual new screen grants
and full GUI/Agent restart remain pending owner action (requested with full paths).
Keep this bundle unchanged for that check. Broader Student relay lifecycle/rejoin/
diagnostic ZIP native test and 35–40 x64 Ethernet acceptance still incomplete.
Details: diagnostics/2026-10-08-macos-permissions.md.


## 2026-10-08 — native Student broadcast lifecycle passed after Local Network Yes

Same paired-permissions-20261008 bundle after owner Allow: Source .94 -> Mac relay
-> Receiver .106 passed late join, 20s receive, receiver reconnect, Stop/queued ZIP
(collected2/missing0), new stream/repeat Stop. Exit0; isolated task/rule/process
cleanup succeeds. Report medians decode18.959/render18.802 FPS, missing max0,
Teacher errors0, relay send errors0, keepalive errors0. 1728x1080 from1920x1200;
ARM Parallels endpoints running x64. Functional evidence only, not 1080p30/40-PC
wired or motion/latency acceptance. Full raw evidence/summary and ZIP recorded in
diagnostics/2026-10-08-student-broadcast.md. Recording and LAN probes visibly green.

Found native custom-flag Restart loss, creating GUI/default helper47800 alongside
restarted helper48900. Fixed source with explicit Dart args -> bounded macOS open
launcher; no rebuild/sign per owner batching rule, so native fix verification is
pending the next grouped build. Duplicate sessions cleaned, one test Teacher left
idle. Next: grouped build/native custom restart test when more source changes are
ready; fixed1920x1080 foreground workload and 35–40 physical x64 Ethernet ladder,
optical latency/soak/source failure/sleep/DPI and Defender lab regression deferred.

## 2026-10-08 — owner clarifies shared Control for Student presentation

Owner requires selecting Student A to open Teacher Control automatically while
the class watches A; explicitly chose cooperative Control (Teacher and A can both
use input), not an exclusive input lock. Implementing a second authorized viewer
of the same Source publisher for Teacher preview/control: one Source capture,
encoder and packetizer remain; class RTP relay stays compressed. Teacher decodes
only its interactive preview. Distinct session-token identities prevent replacing
the relay registration; mode/cursor responses go only to the Control viewer.
Class broadcasts retain the source cursor in video. Closing/failing Control ends
the class session and releases input. Ordinary Observe remains read-only by default.

Before this change, a 90s fixed1920x1080 foreground workload on .94 recorded median
decode27.883/render24.946 FPS, source crash -> Teacher idle in1.55s and successful
restart. Evidence: target/student-broadcast-1080p-retry-20261008. Script errors
prevented network-loss/swap completion; these are not product passes. Workload
restored1920x1200. New harness preserves logs before process restart, records
foreground/mode each second, and will verify harmless Teacher Home-key input.

Control source changes and pending custom-argument Restart fix are being grouped
for one paired build. No new native Control pass claimed yet. Physical x64 wired
35–40-PC scaling, optical latency and long soak remain required in the lab.


## 2026-10-09 — resume cooperative class Control and final grouped build

Verified retained native artifacts from both .94/.106 source directions: default
cooperative Control, harmless Teacher input, foreground fixed1080p workloads,
late join/reconnect, source crash/network-loss recovery, Stop/Control-close cleanup
and diagnostic ZIP2/2. See diagnostics/2026-10-08-student-broadcast.md and its
explicit VM/loss/lab limitations. Prior native custom Restart still dropped args
through open; source now starts the exact bundle executable with explicit args.
Added relay deadline/last-error counters only. Final paired20261009 build and
native Restart verification are in progress; no pass claimed for that fix yet.


## 2026-10-09 — final grouped milestone verified

Paired-class-control-20261009 builds fresh Teacher/Mac Student/Windows x64 Student
and Guard; embedded package bytes/manifests verified. Native direct Restart
preserves bind/ports/IPC/log and leaves one GUI/helper. Both apps connect to .94:22.
Owner granted Recording; reopening exact bundle shows both Granted and actual
Teacher capture/encode runs then stops successfully (no receiver acceptance claim).
Final native Student class-Control regression exit0: late join, receiver reconnect,
queued ZIP2/2, repeat stream and Control-close ending class. Final Rust195 pass,
2 ignored; focused relay5 pass. No further signing/build after grants. Teacher is
left idle for owner use. Detailed reports preserve motion losses and deferred
physical35–40 x64 Ethernet, optical latency, fixed1080p soak/sleep/DPI/Defender tests.
Next: lab acceptance ladder and source-upload/preview load measurements; use added
relay timeout/error counters to diagnose send loss before changing media policy.


## 2026-10-09 — quick source switching, startup checklist correction

Implemented switch_source with one pending target, validation/no-op before Stop,
worker exit/join before new start, class Control cleanup, fresh stream IDs, bounded
pending-stop deadline and Stop cancellation. Toolbar works during presentation,
returns to Teacher and keeps main UI from minimizing on Student Control start.
Automatic permission checklist now once per persistent profile, only with both
Recording grants explicitly missing; manual toolbar/banners retained. Owner asks
manual full-path permission handoff and no Computer Use for permission actions.
Rust198 pass/2 ignored, Flutter13 pass, analyzer clean, paired-source-switch-20261009
build succeeds. Next prepare mixed Windows.102/Mac.254 packages and manually grant
any missing Mac permissions before native switching. No native handoff pass yet;
see diagnostics/2026-10-09-source-switch.md for exact paths, tests and lab deferrals.


Native mixed-peer prepare/hash/codesign finished. Mac Student GUI read-only check
reports Recording false, so paused native handoff at owner's explicit permission
boundary. Full paths are recorded in diagnostics/2026-10-09-source-switch.md.
No Computer Use/privacy automation. Teacher onboarding marker persisted true.
Exact next step: owner grants Mac Student Recording/Accessibility and verifies
Teacher Console/helper Recording, then restart unchanged Teacher and run prepared
mixed-endpoint handoff harness. Native switching and lab scaling remain unverified.


## 2026-10-09 — native handoff exposes receiver teardown race

After owner grants: Teacher to two receivers and Teacher→Windows Control pass,
Windows→Mac Source rejected while old Mac receiver was completing teardown (reply
1ms before idle log). Archived first-run logs/states; cleanup no errors. Teacher
monitoring now retries only transient class-source watching rejection within the
original4s budget and ignores stale request-generation responses.201 Rust pass,
2 ignored. Paired handoff build in progress; keep remote Students unchanged for
Teacher-only fix and permission continuity. Next native rerun; pause for manual
full-path grant if Teacher signing invalidates Recording. No Computer Use.


Paired-source-switch-handoff build and embedded package checks pass. Native rerun
blocked at Teacher capture by TCC refusal after signing; owned test cleanup errors0.
Paused at manual permission boundary (no Computer Use), exact Console/Agent paths
in diagnostic report. Remote Mac Student remains unchanged/granted. Next owner
regrants Teacher Recording, restart unchanged bundle, rerun prepared mixed-peer
handoff.201 Rust tests pass; native retry fix and full source-switch cycles still
unverified. No further build planned before that confirmation.


After manual Teacher Recording regrant/restart, unchanged bundle captures and
Teacher→Windows native handoff passes. Mac-source retry reaches Control setup,
then explicit input permission/init failure blocks Mac Student. Paused per owner
instruction; Accessibility target `/Users/lolem/Classroom-source-switch/Classroom Student.app`
on192.168.100.254. Cleanup errors0; bundles unchanged, no Computer Use. Next owner
checks Accessibility, harness restarts Mac Student and reruns full cycles. Full
native handoff acceptance still pending; diagnostics report preserves each trial.


## 2026-10-09 — mixed-endpoint source switching native pass

After owner grants Mac Student Accessibility, unchanged Teacher/Student bundles
complete two Teacher→Windows→Mac→Windows→Teacher cycles, runner exit0. Ten fresh
streams (two starts/eight handoffs); all non-Source recipients report positive
render FPS, six Student-source sessions acknowledge cooperative Control. Current
target no-op, invalid target rejection without interruption, stale Control-close
and both pending-switch Stop cancellations pass. Final idle with no source,
pending target, Control or error; harness cleanup errors0. No further build/sign
or privacy automation. Evidence and limitations in diagnostics/2026-10-09-source-switch.md.

Health-ready reports take2.505–6.042s (median3.025s), not optical/first-frame latency.
Mac actual input interaction/native toolbar clicks/second-launch popup visual check
remain unverified in this run. Prior Rust201 pass/2 ignored and Flutter13 pass
apply unchanged. Exact next step: physical Windows x64 Ethernet ladder1/5/10/20/
35–40, fixed1080p workload, optical/source-switch latency and source-upload/preview
load, cooperative input and soak; retain deferred sleep/DPI/Defender cases. Current
VM/mixed-LAN functional pass is not Phase1 40-PC performance acceptance.

## 2026-10-09 — manual class-Control faults, active corrective milestone

Owner manual Windows-source and Mac-source runs collected without restart. Mac
crash report SIGTRAP at HIToolbox TISGetInputSourceProperty -> Enigo Unicode key
lookup on Publisher input worker confirms a native thread-affinity failure.
Teacher all Mac TCP channels EOF/offline together; toolbar excludes offline targets,
explaining missing button after checkbox reselection. Correct lookup on main queue
with bounded deadline/capacity and native-code key releases; no media buffer change.
Windows installed Guard/Student/task targeted172.16.48.237 removed with settings/logs
preserved; current new Student and Defender realtime remain. Old dev autostart also
removed. Migration added to development deploy/prepare paths, hash-before-removal.

Owner explicitly requests Cmd+Tab/Cmd+Space as well as app shortcuts. Current Mac
Teacher App Sandbox/inherited helper entitlements conflict with assistive system
control (Apple Protecting user data with App Sandbox documents unavailable APIs).
Proposed implementation: standalone native Teacher build without App Sandbox,
foreground Control-only event tap requiring manual Accessibility. Keep signed app
identities and exact legacy container data path; no TCC edits, no privacy bypass,
no suppression of OS permission checks. Record this packaging deviation before
changing entitlements. Local app-only monitoring would not cover the requested
system shortcuts. New paired build/native permission handoff pending.

Corrective build completes at target/source-control-fix-build; both embedded
Student ZIP hashes/version verified, deep Teacher signature verified. Rust203
pass/2 ignored, final focused Flutter11 pass, analyzer clean, Python compile and
whitespace checks pass. Windows old install + dev autostart removal audited with
Defender realtime still enabled. Replaced only stopped Mac Student test bundle;
GUI-session permission probe screen:false. Native tests now paused for manual
Recording/Accessibility grants; no privacy automation or further runtime test.
Full evidence/limitations: diagnostics/2026-10-09-manual-source-control.md.
Next after grants: restart both roles, actual Mac character/system shortcuts,
repeated handoff + controlled motion/pacing comparison. 35–40 wired x64 lab and
optical latency remain deferred; no claimed FPS improvement or native key pass.


2026-10-09 native follow-up after manual grants: Mac GUI probe Recording true;
restarted Teacher/Mac Student unchanged signed bundles. Actual right-click source
action works without checkboxes; owner confirms Cmd+Tab/Cmd+Space act only on Mac
Student. Corrected Mac PID stable through source and input; no repeat SIGTRAP.
Windows initial SSH/banner access stall recovered; inventory retains correct new
Student PID1748/Teacher171. One Teacher→Windows→Mac cycle passes, health-ready
3.983/3.541/3.009s, positive render on all class recipients, cooperative Control,
no error/disconnect during bounded holds; final Stop idle, apps left open. No
rebuild/sign, forced VM restart or privacy automation. Native functional evidence
now recorded; matched motion FPS improvement and physical35–40-PC wired1080p
acceptance still pending. Exact next step: manual class/text interaction with these
bundles, then lab ladder/optical latency; detailed follow-up in manual-source-control.

## 2026-10-09 — matched Student-source motion measurement

Owner requests FPS/pipeline measurement with unchanged builds. Weak Windows102
stalls SSH/intermittently offline; owner adds Windows155 and closes Student102.
Fresh embedded paired Student bootstrapped155 (ARM VM4vCPU/~6GiB), old install
migration finds no existing Classroom runtime. Defender remains enabled. NIC
match confirms Windows155 shares MacStudent254 host (Mac16,12/10LP/16GiB).

Two native workload cycles/source, seven10s phases,140s each. Exploratory run
excluded from matched FPS conclusions: Windows timer15Hz and unaudited Mac
scroll. Correct only workload timing/viewport; confirmation audits Windows140/140
and Mac139/140 foreground, actual update medians20.000/19.920Hz, scroll movement
verified. Exact sizes1468x922/1470x956, not fixed1080p or independent PCs.

Accepted Windows→Mac class render~19–25FPS nonstatic; Mac→Windows~2–6 despite
source encode~28–29 and direct Teacher preview render~24–26. Windows receiver
recovery/keyframe/stale discard high; missing counter delta1197, keyframe337,
logged recovery peak~3.4s; decoder errors0/ingress drops0. Source send errors0,
relay deadline errors2/4. No proven unique cause or product performance fix.
Relay code already drains up to256 UDP packets/batch then sleeps1ms per batch;
no evidence for a one-packet/ms cap. Measure relay residence/burst, deadline,
clock-domain and VM scheduling before patching.

Swift workload compiled/native140s runs pass, Python compile/whitespace checks
clean. Workload jobs/tasks removed, Teacher idle/error:null,254/155 online,
102 offline per owner; apps retained. No Classroom rebuild/sign/privacy edits.
Exact next step: same workload UDP/depacketize sink without decode vs decode,
relay residence/burst and freshness recovery instrumentation; then patch/A/B if
proven.30-minute soak, optical/fixed1080p and wired35–40 physical x64 deferred.
Full methods/limits/evidence: diagnostics/2026-10-09-source-motion-measurement.md.
## 2026-10-09 — Control recovery and shifted Windows keys

- Fixed the independent Observe window losing the teacher's cooperative Control
  intent after a transient receiver restart or a Student input lease expiry.
  The intent is retained only after the teacher explicitly selected Control;
  recovery sends one fresh Control request after a live frame is attached and
  waits for the Student acknowledgement before forwarding input. Held keys and
  mouse buttons are not replayed, and an explicit Observe selection cancels the
  retained intent.
- Fixed Windows character injection for shifted punctuation. `VkKeyScanExW`
  returns a virtual key in its low byte and modifier state in its high byte;
  the previous Unicode path passed the combined value to Enigo, so `Shift+1`
  could be rejected. The low-byte virtual key is now used while the existing
  Shift press/release events remain authoritative.
- Focused Flutter recovery tests passed 8/8 and Flutter analyze reported no
  issues. Rust Classroom tests passed 204/204 with 2 ignored, including the
  new virtual-key masking tests. A paired Teacher/Mac Student/Windows x64
  Student build was produced at `target/control-recovery-build` with embedded
  package version `paired-control-recovery-20261009`; codesign verification and
  package hash checks passed. Windows `.155` was updated through the supported
  updater and confirmed the new version.
- Runtime keyboard typing through a temporary Windows text sink remains pending:
  the external command approval service hit its usage limit while trying to
  launch the bounded sink. The current Teacher endpoint also became unavailable
  after the isolated GUI restart, so no runtime result is claimed for Shift+1.

## 2026-10-09 — Live VS Code policy confirmation

Fixed stale dialog state and premature Apply success; retain the dialog and wait
for Student completion. Badge now describes extension allowlist state rather
than promising all AI is blocked. Added AI setting readback and restart errors;
preserve invalid/JSONC settings instead of silently overwriting them. Windows
`.155` inspection confirmed an allowlist but no AI-disable setting; exact cause
of the mismatch remains unproven. Mac check and Flutter analyze passed; paired
build and Teacher/Windows deployment complete, no automated tests run. Windows
reconnected and confirmed the new build; Apply/Chat checks remain manual. See
[evidence and limits](diagnostics/2026-10-09-vscode-policy-confirmation.md).

## 2026-10-10 — Native Remote Terminal, runtime work in progress

Implemented native ConPTY/POSIX PTY with pinned TLS, managed pairing,
non-elevated Windows shell, bounded streaming/reconnect and native Flutter xterm;
updated Copilot dialog labels and dirty-state Apply. Paired build
`paired-native-terminal-interrupt-20261010` deployed to Teacher, Windows `.94`,
and Mac `.2`. Both Students online. Windows native commands, resize, ordinary
user token, UI reconnect and immediate output passed. **Ctrl+C does not yet stop
ping; full terminal acceptance and network reconnect fixture remain pending.**
Existing Flutter 9 failures also reproduce at clean HEAD; Guard 15 tests pass.
See [runtime checkpoint](diagnostics/2026-10-10-native-remote-terminal.md).

Mac `.2` requires manual privacy grants for:
`/Users/mvmanh/Classroom/Classroom Student.app` (Screen Recording, Accessibility).
Teacher recording after signing requires:
`/Users/mvmanh/Development/netsupport-school/target/present-recovery-build/Classroom Console.app`
and its `Contents/Helpers/Classroom Agent.app`; restart after grants.
Do not automate privacy changes. Owner additionally requested a Windows CMD
Unix-like command compatibility layer with tests; this is not implemented or
shipped yet. Resume with permission/restart, Ctrl+C repair, Mac zsh validation,
then compatibility implementation and one consolidated paired build.

## 2026-10-10 — In-place Accessibility request from Observe

Observe Control now keeps the macOS Accessibility warning compact and shows an
icon-only request button beside it. The button invokes the existing native
`requestAccessibility` prompt; after approval it immediately retries system
shortcut capture. Focused Flutter test passed. The full `class_control_test.dart`
file still has an unrelated failure in `class presentation keeps acknowledged
shared Control selected` (expected a channel call that was not recorded).
Paired release build `paired-observe-accessibility-request-20261010` completed
for Mac Teacher, Mac Student and Windows x64 Student, with fresh Student ZIPs
bundled and codesign verification successful. Artifacts are isolated under
`target/mac-accessibility-request-build`; active Teacher remains at
`target/present-recovery-build/Classroom Console.app` and was not replaced or
restarted. Runtime prompt/shortcut retry still needs verification on macOS.

Owner reported the request button was not visible because the running Teacher
had not yet been updated. The paired bundle was then copied to the stable
Teacher path after verifying and stopping the old Console/helper. It was
re-registered with Spotlight/LaunchServices and started successfully; processes
now point to `target/present-recovery-build`. Observe should be opened again for
runtime verification. Accessibility prompt and shortcut retry remain to be
confirmed.

## 2026-10-10 — Compact labeled Accessibility request

Replaced the far-right icon with an inline, compact `Request permission` text
button immediately after the Accessibility warning. Focused Flutter widget test
passed and `git diff --check` passed. Paired build
`paired-observe-accessibility-link-20261010` completed for Mac Teacher, Mac
Student and Windows x64 Student. Only the Teacher bundle was updated at
`target/present-recovery-build/Classroom Console.app`; Student endpoints were
not deployed. The new Teacher and helper are running from the stable path.

## 2026-10-11 — Hardware Control implementation (no Windows runtime test)

- Owner explicitly authorizes Hardware Control and says no Windows machine is
  available: source implementation/checks only; no endpoint deployment or runtime
  acceptance. Existing media milestones/physical lab acceptance remain unchanged.
- Added one dedicated Flutter Hardware Control window, toolbar entry and existing
  dependent-window focus/cleanup. Five Block/Unblock switches always address the
  whole room, including future joiners. Group counts and per-PC Pending/Applied/
  Failed/Unsupported details use real Student OS acknowledgements.
- Reused private Teacher IPC, tools Hub/Protobuf poll/ack, roster and Guard Service.
  Teacher persists a revisioned class policy; offline/restarted/new Students get
  the latest full document. Old ACKs cannot overwrite a newer policy. Hardware
  has no day/lease/disconnect expiry. Optional restore requires explicit hardware
  session end; Teacher exit and stopping presentation never imply Unblock.
- Added bounded Student OS worker and the service's pre-login/offline reconciler.
  Windows executes an embedded fixed PowerShell script with JSON stdin, 45s
  deadline, protected mutex, admin/SYSTEM-owned ProgramData state and a flushed
  atomic rollback journal before mutation. Fresh service heartbeat prevents an
  older running Guard binary from claiming durable hardware support. Errors
  before persistence retry, periodic reports refresh status, and stale status fails.
- Windows backend: removable-disk read/write machine policy; positively identified
  Wi-Fi/tether adapters; Bluetooth radios with input ancestry protection; camera
  and microphone capture PnP endpoints. Refuses active/unknown/virtual management
  route changes. Preserves USB controllers, keyboard/mouse, speakers, previously
  disabled devices and unrelated registry policies. Unplugged restore entries
  remain durable. Local Administrator emergency CLI: --hardware-reset.
- Verified: full Rust suite 249 passed / 2 ignored; focused hardware filter 5
  passed (includes 2 pre-existing media hardware tests). Windows x64 cross-target
  cargo check passed for Classroom, ClassroomStudent and ClassroomGuardService.
  Non-strict Clippy passed with 197 existing warnings and none in the new hardware
  modules. New Rust modules formatted; git diff --check passed.
- Flutter analyzer --no-fatal-infos passed (4 existing informational findings);
  3 Hardware Control tests plus 1 badge regression passed. Actual PowerShell
  enforcer AST/functions passed mocked contract cases: service heartbeat,
  management/virtual route, tether and unknown USB Ethernet classification,
  journal-before-mutation, camera/microphone versus speaker/input, pre-disabled
  baseline, Bluetooth ancestry, unplug/replug restore, exact storage baseline,
  reboot and fixed USB media. These are mock/source checks, NOT Windows runtime.
- Limits: elevated Student and updated installed/running Guard service required
  for new policy commands; standard-token Agent is Unsupported (no new privilege
  broker). Hotplug uses periodic scans with a possible access window. USB open
  handles/cached decisions stay Pending until reboot; fixed USB media/unknown
  tether drivers report Unsupported. No guarantee against local Administrator or
  active LAN impersonation; existing tools LAN transport/auth model is unchanged.
- Details and Windows references: docs/26_HARDWARE_CONTROL.md. No release package
  built, no active Teacher replaced, no Student deployed, no privacy changes.
  Exact next step when Windows becomes available: paired fresh roles/updated
  service, then execute the documented real-device/reboot/standard-user/restore
  matrix. Do not claim OS runtime or performance acceptance from these checks.

## 2026-10-12 — Windows x64 paired build requested by owner

- Built unchanged application source at f5f25a0 with scripts/build-flutter-windows.ps1; label paired-20261012-073748. Fresh MSVC release Teacher agent and Student, Flutter Teacher, matching app-local CRT and bundled Student update ZIP completed.
- Outputs: target/Classroom-Console-Windows-x64/classroom_console.exe; target/Classroom-Teacher-Windows-x64-20261012.zip; target/Classroom-Student-Portable-Windows-x64-20261012.zip. Keep all files beside the Teacher executable. Student portable ZIP includes app-local CRT.
- Verified bundled Student update ZIP matches freshly generated ZIP (SHA256 FB4E48295C85BEC3C92C7D36CC55057CAFE8C5F7B1BD3A30EFBF234FCB774529). Flutter SDK refreshed six resolved dependencies for this build; generated tracked lock/registrant changes were restored afterward. No application source changes or endpoint deployment.
- Windows local executable smoke passed registration/thumbnail, two share-stop cycles, stable-identity Student reconnect and live native Observe frames (sequence samples 72, 176). Evidence: target/windows-flutter-smoke/20261012-074151/results.json. Script cleaned up its owned processes.
- Checks are NOT fully green: cargo fmt --check reports existing formatting differences; strict Clippy fails on three items_after_statements findings in apps/classroom/build.rs; cargo test --workspace stops at Classroom tests (246 passed, 1 failed, 1 ignored), failing activity::windows::tests::chrome_timestamps_convert_to_unix_milliseconds (1704470400000 versus 1704067200000). Flutter tests: 56 passed, 9 failed in class Control/Observe expectations and layout overflow. Logs remain under target/build-tests.log, target/build-clippy.log and target/flutter-build-tests.log.
- Limits: smoke is same-host functional evidence, not physical wired 1080p/40-PC acceptance. Hardware Control and installed Guard service/device enforcement were not validated or deployed by this build.
- Exact next step: owner can extract the complete Teacher ZIP and run classroom_console.exe; investigate recorded test failures before declaring all checks green, then run the documented physical Windows LAN acceptance matrix.
# 2026-10-12: four physical Windows Students — screenshot diagnosis

Idle screenshot failed on all four Students with DXGI E_INVALIDARG (0x80070057),
while screenshots during presentation succeeded. Monitoring already owns the
desktop duplication in the idle Student process. Windows permits one duplication
per output per process (Microsoft IDXGIOutput1::DuplicateOutput documentation).
Implement a bounded, cancellable one-shot child process for Windows screenshots;
retain the existing capture/PNG path, media pipeline and non-Windows behavior.
This is a file-job operation, not a second classroom encoder. Verify idle
screenshots on all four real Windows endpoints after supported updater deployment.
Terminal remains unpaired; no reboot or hardware disconnection is authorized.

## 2026-10-12 ? Four physical Windows Students: autonomous validation

Screenshot DXGI ownership bug fixed with bounded one-shot Windows child; fresh
paired Teacher/Student built, supported updater confirmed all four Student builds.
Idle screenshot + fresh thumbnail regression PASS 4/4, keyboard input proof 4/4,
VS Code settings/ACL restored 4/4, badge/source switching/policy ACK checks complete.
Smoke harness now refuses concurrent Teacher to preserve shared AppData logs.
Clean 1600x900 run: render 22.48?22.54 FPS, decode errors/missing packets zero,
one encoder. Not 1080p/40-PC acceptance. Terminal unpaired; hardware mock only,
no service/reboot/device disconnection. Teacher fresh bundle build/smoke verified,
active Teacher kept at old path because isolated new path failed LAN connectivity
(firewall filter query Access denied); automatic review rejected old-build deletion.
See [full evidence and limits](diagnostics/2026-10-12-four-windows-students.md).
Rust tests 246 pass/1 ignored/1 known baseline filtered failure; strict lint/Flutter
baseline failures remain documented. No checkpoint commit: Git identity unset.
Next: resolve new Teacher path firewall in an authorized admin context, then long
physical x64 media runs and receiver/network measurements per docs/06.
# 2026-10-12 — Owner changes Remote Terminal authorization model

Owner explicitly requests no pairing or per-Student admin provisioning. Replace
the managed-policy prerequisite with per-session authorization delivered by the
Student's existing configured Teacher tools connection. Retain TLS with the
offered Teacher certificate, random session secret, roster/local IPC checks,
non-elevated worker and bounded PTY/session lifecycle. This intentionally trusts
the configured Teacher connection for initial authorization; it is no longer
out-of-band certificate provisioning and must not be described as such. Remove
pairing UI. Build paired roles, deploy via Classroom updater and test all four
Windows Students, including shell identity, input/output, close and reconnect.


### 2026-10-12 — chốt nhanh và dọn bàn giao phòng máy

Bản đồng bộ Teacher/Student `room-fixes-input-20261012` đã build; 709-17 và 709-18 xác nhận cập nhật. Terminal keyboard regression pass, App Control Apply cả hai complete và rule test đã hoàn nguyên. Các sửa badge/native sizing chưa được runtime xác minh; Ctrl+Alt+Del chưa hoàn tất vì Keyboard Filter disabled và không được reboot. Bản chạy/logs lưu ngoài repo trên Desktop. Code/test/report được stage, giữ .git, dọn build/cache theo yêu cầu owner; không commit. Chi tiết, giới hạn và bước tiếp theo: [room final handoff](diagnostics/2026-10-12-room-final-handoff.md).

### 2026-10-12 — Làm lại UI Teacher: ribbon có nhãn chữ, tiếng Việt

Thay thanh ~17 icon không nhãn trên màn hình chính Teacher bằng ribbon 4 tab có nhãn chữ (Kiểm soát lớp / Tin nhắn & tệp / Giám sát / Hệ thống) trong `lib/teacher_ribbon.dart`; chip phạm vi luôn nói rõ thao tác áp dụng cho "Cả lớp" hay "N máy đã chọn"; nút "Chọn máy" thay icon checklist ẩn, kèm "Chọn tất cả máy online/Bỏ chọn" ngay trên thanh; thẻ Student gom 3 icon thành một nút "⋯" mở cùng menu chuột phải; ribbon tự thu gọn khi cửa sổ cao <640px. Chuỗi màn hình chính chuyển sang tiếng Việt (bảng `Vi` trong `teacher_ribbon.dart`). Logic agent, lệnh và cửa sổ con không đổi.
Đã xác minh: `flutter analyze lib` không thêm cảnh báo; `flutter test` — test mới `test/ribbon_test.dart` (4) pass, test cũ cập nhật chuỗi pass. Còn 9 test lỗi sẵn có từ trước thay đổi (8 trong observe_test, 1 trong class_control_test), không do lượt này. Chưa runtime-verified trên Windows/macOS thật; chưa build/deploy.
Rủi ro/chưa làm: cửa sổ con (Observe, Files, Activity, Internet, App control, Settings…) vẫn tiếng Anh; test widget dùng font Ahem nên chỉ kiểm tra cấu trúc, chưa kiểm tra thẩm mỹ thật; số cột lưới ở 740×520 nay là ≥2 (toolbar cao hơn).
Next: build cặp Teacher+Student theo PROJECT_NOTES, chạy thử trên máy thật, chỉnh theo phản hồi; sau đó Việt hóa các cửa sổ con.

### 2026-10-12 — Teacher: mọi cài đặt lưu tập trung trong một SQLite (`classroom.db`)

Chủ dự án yêu cầu chuẩn hóa: tất cả thông tin Teacher nhớ lại phải nằm ở một nơi duy nhất là local database. Quyết định: **SQLite (`rusqlite` bundled) do agent Rust sở hữu**, một bảng `documents(namespace, key, value JSON, updated_at)` với namespace cố định (`prefs`, `config`, `roster`, `inventory`, `activity`, `rules`, `badges`, `recordings`, `hardware`, `ui`); crate mới `crates/classroom-store` (migrate theo `user_version`, WAL + synchronous=FULL, file hỏng thì đổi tên `.corrupt-<ts>` rồi tạo mới, schema mới hơn thì từ chối chứ không hạ cấp). Mọi cửa sổ Flutter (kể cả cửa sổ phụ) đọc/ghi qua IPC `store_list`/`store_apply` — một writer duy nhất, không thêm phụ thuộc native cho Flutter. Console chỉ ghi được `prefs/config/roster/inventory/ui/activity`; `rules/badges/recordings/hardware` chỉ agent ghi qua lệnh đã kiểm tra.
Đã chuyển vào DB: `console-settings.json`, `teacher-settings.toml` (overlay cấu hình, agent tự đọc khi khởi động; Flutter không còn truyền `--config`/`--bind-ip`/cổng), `known-students.json`, `app-inventory.json`, `monitoring-badges.json`, `recording-sessions.json`, `hardware-policy.json`, `class-rules.json`, cache báo cáo Activity theo Student (trước là thư mục `activity-cache/`), và cờ onboarding macOS (trước là NSUserDefaults). Lần chạy đầu tự nhập các tập tin cũ đúng một lần (đánh dấu trong `meta`, cùng transaction với dữ liệu) rồi đổi tên `.migrated` — dữ liệu cũ được giữ, không bao giờ đọc lại làm nguồn sự thật.
Cố ý KHÔNG chuyển (cần chủ dự án quyết định): (1) Student: `student-settings.toml`, trạng thái guard/VS Code/App control/`activity-monitoring.json` — đây là máy Student, ngoài phạm vi "phía giảng viên"; (2) bookmark quyền Desktop (security-scoped, do macOS cấp, gắn với chữ ký app, không phải cài đặt); (3) `terminal-identity.json` (khóa riêng TLS của Teacher, quyền 0600) và `remote-terminal-audit.jsonl` (nhật ký append-only) — là danh tính/nhật ký, không phải cài đặt; (4) log, diagnostics ZIP, student-downloads, outbox, `teacher-endpoint.json` (bắt tay IPC lúc chạy). `class-rules` vẫn hết hiệu lực khi sang ngày mới (thiết kế cũ, khớp Student).
Đã xác minh: `cargo test --workspace` pass (263 test agent, trước đó 246; crate store 8 test; thêm test IPC/namespace/overlay); `flutter test` chỉ còn 9 lỗi nền có sẵn; build cặp Teacher+Student (gồm cross-compile Windows x64 với SQLite bundled) thành công; **chạy thật trên Mac**: dữ liệu thật của owner được nhập (prefs 12 khóa, config, inventory, roster 3, recordings) và đối chiếu giống hệt bản sao lưu; khởi động lại lần hai đọc từ DB, không lỗi, hai Student (.2, .166) kết nối lại. Bản sao lưu trước khi migrate: `scratchpad/legacy-backup`.
Chưa xác minh: chạy Windows Teacher thật với DB (chỉ cross-compile Student/guard); ghi từ cửa sổ phụ (Internet/VS Code/Apps) và cài đặt `Settings → Save & reconnect` trên runtime thật (có test đơn vị, chưa bấm tay); migrate trên Windows.
Next: owner thử Settings/Internet/VS Code/Apps trên Mac rồi Windows, xác nhận đóng-mở lại vẫn còn; quyết định các mục "không chuyển" ở trên.

### 2026-10-12 — Student cũng lưu cài đặt vào SQLite (`classroom.db` trong thư mục profile)

Theo yêu cầu "chuẩn hóa cả phía Student": mỗi thư mục profile (Student: `%APPDATA%\Classroom` / `~/Library/Application Support/Classroom`) có `classroom.db`, namespace `profile`, mở qua `classroom_store::open_shared` (một kết nối dùng chung mỗi tiến trình). Đã chuyển: `profile.toml` (role + Teacher IP), `device-id.txt` (tạo bằng `insert_if_absent` để hai tiến trình chung một ID), `teacher-ip.txt`, `student-settings.toml`, `ui-preferences.toml`, `connection-popup.txt`, `monitoring-badge.json`, `activity-monitoring.json`, `guard-policy.json`. Khác Teacher, **tập tin gốc của Student được giữ nguyên** (không đổi tên): updater có thể rollback về bản chưa có DB và bản đó vẫn phải đọc được Teacher IP/cài đặt để kết nối lại; DB mới là nguồn sự thật, tập tin cũ chỉ còn là bản tạm cho rollback và sẽ không bị nhập lại.
Cố ý giữ ngoài DB (trạng thái thực thi ở mức hệ điều hành, phải nằm đúng chỗ OS/service đọc): HKLM registry của App Control + IFEO + stub ProgramData, WFP/DNS/hosts của Guard, cài đặt VS Code của người dùng; lock instance `student-<port>.lock`; thư mục việc đang chạy `recordings/`, `terminal-workers/`; `classroom-bootstrap.toml` do bộ cài cấp; khóa TLS + audit của Remote Terminal; bookmark Desktop macOS; log/diagnostics/downloads.
Đã xác minh: `cargo test --workspace` pass (263 test agent, 12 test store); `cargo check` target Windows pass (phát hiện và sửa import `PathBuf` chỉ lộ ở bản Windows); build cặp Teacher+Student pass; **chạy thật trên Windows Student 192.168.50.166** (cập nhật qua Classroom updater, không SSH binary): DB được tạo, nhập đủ 6 tập tin (launch, device_id giữ nguyên `1790990215799937300-6188`, teacher_ip, settings, badge, connection_popup), Student khởi động lại và Teacher xác nhận bản mới; tập tin gốc còn nguyên. Sao lưu trước đó: `scratchpad/win-backup`. Mac Student (.2) chưa cập nhật (cần owner cấp lại quyền thủ công).
Chưa xác minh: bật/tắt Internet/Guard (cần admin) và ghi `guard_policy`/`activity_monitoring` trên Windows thật chỉ có test đơn vị; rollback updater thực tế sang bản cũ chưa thử.

### 2026-10-12 — Đại tu dialog Teacher: khung `AppDialog` chung + Việt hóa

Thêm `lib/app_dialog.dart`: `AppDialog` (header icon/tiêu đề/mô tả/đóng, thân cuộn, footer nút), nút `primary/secondary/cancel`, `DialogSection`, `InlineNotice`, `DialogEmpty`, `DialogPanel`, `confirmDialog`; `classroomTheme()` dùng chung cho cửa sổ chính và cửa sổ phụ. Đã chuyển sang khung này và Việt hóa: Nhắn tin, Chạy ứng dụng (danh sách, sửa, xác nhận xóa), Mở website, Thu tệp (+ tiến độ), Ghi màn hình, Phản hồi, Mất kết nối, Chẩn đoán, Huy hiệu giám sát, Khóa/Tắt máy/Khởi động lại/Đăng xuất, Cài đặt giảng viên (sidebar có icon, 7 trang), quyền macOS, xác nhận Chặn ứng dụng/VS Code/Xóa lịch sử Activity, nội dung clipboard, tên danh sách & trình duyệt quét (Internet), Cài đặt Terminal, Nhắn tin trong Quan sát; thẻ thông báo nổi (tiến độ gửi tệp, kết quả, phản hồi); tiêu đề các cửa sổ phụ. Chưa chuyển: nội dung bên trong các cửa sổ phụ (Internet, Chặn ứng dụng, VS Code, Activity, Files, Hardware, Observe, Terminal) vẫn là giao diện/ngôn ngữ cũ.
Đã xác minh: `flutter analyze` sạch lỗi/warning mới; `flutter test` — thêm `app_dialog_test` (3), cập nhật chuỗi của các test cũ (diagnostics, tools_activity, settings, permissions, recording, roster…), chỉ còn 9 lỗi nền có sẵn; kiểm tra bằng mắt ảnh chụp từng dialog (phông Arial Unicode + Material Icons trong test tạm, đã xóa) — không tràn, tiếng Việt có dấu hiển thị đúng; ảnh chụp cửa sổ Teacher thật (bản trước đó) khớp thiết kế.
Chưa xác minh: dialog trên Windows thật; macOS dialog quyền khi chưa cấp quyền; các cửa sổ phụ chưa Việt hóa/đại tu.

### 2026-10-12 — Đại tu các cửa sổ phụ lớn của Teacher

Thêm `lib/window_shell.dart` (`WindowShell` header icon/tiêu đề/mô tả/nút + dải thông báo + thân + footer; `WindowCard`, `StatusChip`) cùng phong cách với `AppDialog`. Đã làm: **Hardware Control** (thẻ nhóm thiết bị, chip trạng thái màu, công tắc Chặn/Cho phép), **Kiểm soát ứng dụng**, **Tiện ích VS Code**, **Truy cập Internet** (4 chế độ, danh sách, nhóm website; sửa lỗi assert `Autocomplete` thiếu `focusNode` ở chế độ cho phép danh sách), **Hoạt động** (Việt hóa toàn bộ, định dạng ngày d/m), **Tệp** (WindowShell + Việt hóa), **Quan sát** và **Terminal từ xa** (Việt hóa nhãn/tooltip/menu; giữ nguyên logic và giao diện terminal tối). Ghi chú quét website, lỗi trình duyệt ẩn cũng đã Việt hóa. Footer `WindowShell` đã sửa để rộng hết cửa sổ (lỗi lộ ra khi xem ảnh chụp).
Đã xác minh: `flutter analyze` sạch; `flutter test` chỉ còn 9 lỗi nền có sẵn — danh sách 8 test `observe_test` lỗi **giống hệt** trước/sau đổi chuỗi; kiểm tra bằng ảnh chụp dựng sẵn (phông thật) cho Hardware, Ứng dụng, VS Code, Internet, Hoạt động.
Chưa xác minh: chạy thật với Student (hai Student đang tắt theo owner 2026-10-12 nên chưa deploy/thử); Observe/Terminal/Files chỉ Việt hóa, chưa có ảnh chụp trực quan; Windows Teacher.


## 2026-10-13 — Ubuntu Student receiving milestone

Owner authorized Ubuntu Student implementation and task prioritization. The
owner's target is Ubuntu 22.04 ARM64 in Parallels at 10.211.55.5; it uses a slow
disk. Owner explicitly requested functional validation only, not performance.
Windows Phase 1 acceptance and existing paired release-build rules remain intact.

### Implemented

- `codec-linux`: bundled OpenH264 0.9.8 software H.264 decode, immediate output for
  the no-B-frame profile, packed BGRA, source timestamps and bounded stage metrics.
  Rejects H.265, malformed/oversized AUs and decoded geometry beyond 1920×1080.
  Linux backend effective MSRV is 1.89; no hardware/runtime codec dependency.
- `renderer-linux`: native X11/XWayland, aspect-preserving minifb scaling, black
  bars, local pointer overlay, latest retained surface, dirty-only pointer redraw
  and resize repaint. EWMH fullscreen/above requests are compositor policy, not
  privileged enforcement. Native Wayland is not implemented.
- Linux selects those backends in the existing bounded UDP/jitter/depacketize/
  decode/render receiver. No wire protocol, Teacher encoder/fan-out, media queue
  size or recovery timeout changed. Linux Teacher is explicitly rejected.
- Linux capture/monitoring endpoint is not registered. Unsupported capture,
  terminal, power/launch, updater, badge and connection-popup capabilities are
  not advertised. Portable file/diagnostic plumbing remains existing code.
- Vendored minifb X11 now honors `visible:false` and real show/hide. The hidden
  Student host initializes before control registration and keeps XWayland warm
  across sessions. Patch provenance is in `vendor/minifb/CLASSROOM_PATCH.md`.
- Native Linux development packaging script, source SHA-256 fingerprint/compiler/
  nullable Git metadata, ELF x86_64/ARM64 package identification and four tests.
  Usage, scope and subsequent tasks: `docs/28_UBUNTU_STUDENT.md`.

### Verification

Host checks (toolchain explicitly selected where necessary):

```text
cargo test --workspace --exclude codec-macos-vt --offline
cargo test -p codec-macos-vt --offline  # outside sandbox for VideoToolbox
cargo check -p classroom-app --bin Classroom --bin ClassroomStudent
cargo check -p classroom-app --bin Classroom --bin ClassroomStudent --target x86_64-pc-windows-gnu
cargo check -p classroom-app --tests --target x86_64-unknown-linux-gnu  # Zig C/C++ cross tools
cargo clippy -p codec-linux -p renderer-linux --tests -- -D warnings
cargo clippy -p renderer-linux --tests --target x86_64-unknown-linux-gnu -- -D warnings
cargo clippy -p classroom-app --bin ClassroomStudent --target x86_64-unknown-linux-gnu
python3 scripts/test-student-linux-package.py
bash -n scripts/build-student-linux.sh
```

409 workspace tests passed in the portable and macOS-codec runs; five manual
checks remained ignored. Four packaging tests passed. Windows/macOS role checks
and Linux x64 compile checks passed. Strict new-crate Clippy passed; broad app/
vendor checks retain existing warnings. New Rust files pass rustfmt and the diff
passes whitespace checks; full-file formatting of existing app files is already
inconsistent and was not broadly rewritten.

Native Ubuntu ARM64 (Rust 1.99.0, isolated `/tmp/classroom-ubuntu-validation`):

```text
cargo +stable test --locked -p codec-linux -p renderer-linux
xvfb-run -a cargo +stable test --locked -p renderer-linux hidden_host -- --ignored --nocapture
xvfb-run -a cargo +stable test --locked -p classroom-app --bin Classroom linux_student_smoke -- --ignored --nocapture
# Same built smoke executable run as parallels with the existing GNOME
# DISPLAY=:0 and session XAUTHORITY (path only; no grants changed).
cargo +stable build --locked --bin ClassroomStudent
ClassroomStudent --version
ClassroomStudent --diagnose --loopback --launch-profile <isolated profile> --log-file <isolated log>
ldd ClassroomStudent
```

Five decoder + three surface tests passed. Native X11 attributes proved the 1×1
background host remains unmapped after event pumping. Both Xvfb and normal-user
GNOME/XWayland receiver smoke tests passed active late join, real UDP receive,
depacketization, decode, native window rendering, Stop/Start on one TCP connection
and disconnect teardown. Assertions require nonzero receive/decode/render reports
for each stream; diagnostic headless mode was not used. The two desktop streams
reported 60 decoded frames each, with nonzero actual rendered frames. These are
64×48 synthetic loopback H.264 fixtures, not real screen capture/LAN acceptance.
The actual ARM64 executable built and its version/diagnostics/library checks
passed. No performance conclusion is drawn from any VM timings or cadence.

### Findings, corrections and limits

- Initial sandbox-wide tests hit two VideoToolbox initialization errors (-12903).
  The same existing codec tests passed 7/7 with VideoToolbox access outside sandbox.
- Parallels Tools updated/restarted during setup; transient guest-channel errors
  interrupted setup before recovering. The VM source/toolchain/test area is
  isolated; no existing Classroom deployment was replaced.
- The first fixed-three-second fake Teacher closed stream 102 before Xvfb's next
  window was ready. The harness now waits for an actual rendered report, sends
  another second, and retains bounded 12/15-second test deadlines/assertions.
- A normal-user XWayland cold start consumed about 25 seconds before registration
  warmup was restored, causing control liveness expiry. Hidden host initialization
  now occurs before TCP registration and persists across cycles. The final tests
  passed without changing production liveness deadlines or enlarging media queues.
- Native build/runtime was ARM64 only; Linux x64 is compile-checked only. No actual
  Windows/macOS Teacher LAN session, full-agent automatic reconnect, long soak,
  visual cursor/text/aspect QA, induced network-loss acceptance, native Wayland,
  Ubuntu 24.04 or physical-classroom benchmark is claimed.
- Release tar.gz creation has not been executed; script/package source checks do
  not constitute a shipped release. No Teacher release was built/replaced and no
  v1 assets, settings/profiles/device IDs or privacy grants changed. The temporary
  debug executable is a validation artifact, not an installed Student package.
- Existing unrelated working-tree edits were preserved. No checkpoint commit was
  made: this session's Git metadata is read-only and the checkout was already dirty.

Logs: `target/ubuntu-validation/{native-final,desktop-final,native-executable-final,
linux-renderer-final-clippy,linux-app-clippy,macos-final-check,windows-final-check,
portable-tests,macos-codec-tests,package-tests}.log`. Earlier failed runs are retained
in the same folder rather than relabeled as passes.

Exact next step: run the complete Ubuntu Student agent against a real current
Windows/macOS Teacher over LAN, using paired packages when a Teacher build is
requested. Verify full-agent reconnect, actual screen/cursor/aspect behavior and a
longer functional session on this slow VM. Then tackle launcher/autostart and
Ubuntu bootstrap/updater as separate tasks; capture/Observe and OS controls need
separate Linux backends. Performance measurement is deferred per owner request.


## 2026-10-13 — Ubuntu Student: real Teacher LAN functional validation

Status: **complete for the receiving/lifecycle functional gate**, performance
explicitly deferred by owner because the Parallels VM is on a slow disk.
Detailed evidence and limits:
`checkpoints/diagnostics/2026-10-13-ubuntu-student-lan.md`.

New reproducible tools: `scripts/test-ubuntu-student-lan.py`,
`ubuntu-student-lan-worker.py`, `ubuntu-lan-workload.swift`. They run an isolated
room with the existing permitted macOS Teacher package and the complete native
Ubuntu Student executable; no fixture receiver substitutes for the Student agent.
The Student has its own temporary profile/SQLite and binary directory. All process
signals are restricted to children or exact run-owned binary/process-group paths.
No SSH credential is embedded, printed or saved. No product Rust/Flutter code,
privacy grant, deployment/package or production default changed in this step.

Verification:

```text
python3 -m py_compile scripts/test-ubuntu-student-lan.py scripts/ubuntu-student-lan-worker.py
python3 scripts/test-ubuntu-student-lan.py --soak-seconds 180
# Workload is compiled with swiftc by the harness.
git diff --check
```

Final run `target/ubuntu-lan-validation/20261013-114418/results.json` has
`passed:true`. Passed: idle registration/Start and real screen decode/render;
Stop with Student still alive; active late join after Student restart with the
same identity; Teacher crash/restart automatic reconnect without restarting
Student; silent Teacher liveness expiry/release/recovery; 180-second functional
session with 18 status samples. Final owned Student status was `alive:false`.

Teacher capture was **1512×982**, protocol 8, using ScreenCaptureKit/VideoToolbox.
The test profile retained resolution and capped cadence at 5 FPS, target 2 Mbps;
no FPS/CPU/latency requirement was assessed. Teacher and VM communicated through
Parallels virtual LAN at 10.211.55.2/10.211.55.5, isolated ports 49000–49011.
Foreground audit: 239/239 workload samples. Final Student session logged 905
successful decodes and 903 renders, zero decode/depacketizer errors, valid cursor
image/state reception and zero malformed pointer packets. These are functional
progress counters, not physical-LAN/performance acceptance or manual visual QA.

The Teacher binary was reused from the existing paired-20261012-212529 package;
this is compatibility validation, not a new paired release. Manifest records
both executable hashes and paths. The framework retains earlier failed runs:
113727 had a transient Parallels job-handle error; 114022 received/rendered but a
raw Teacher restart refused the killed process's stale IPC endpoint. Bounded
retries for the specific guest-tool error are safe because setup/start are
journaled/idempotent. A restarted Teacher now uses a fresh test IPC file, retaining
same room ports/profile directory and preserving the stale file as evidence.
No product crash-recovery or media timeout/buffer behavior was changed.

Rust/Flutter suites were not rerun for these test-tool/documentation-only changes;
previous receiving-backend regression results remain separately recorded above.
No checkpoint commit: Git metadata is read-only and unrelated working-tree edits
were already present and preserved.

Exact next step: implement a per-user Ubuntu launcher/install package and graphical
session autostart while retaining SQLite profiles outside the executable directory.
Then add Ubuntu bootstrap/updater integration and fresh Student package bundling
according to the paired-build rule. Manual cursor/text/aspect/display-change QA,
Linux x64/24.04, native Wayland and capture/Observe/OS-control backends remain open.


## 2026-10-13 — Owner-confirmed Ubuntu screen sharing and handoff

Owner manually confirmed that the macOS Teacher screen is visible on Ubuntu
Student and requested pausing implementation here, saving history, and committing
the current work to main/GitHub. This confirms the basic screen-sharing function,
not performance acceptance. Performance remains deferred on the slow-disk VM.

Manual session was launched from existing Mac Classroom Console/Agent under
`target/Classroom Console.app` and the isolated Ubuntu Student test binary.
Teacher 10.211.55.2 / Student 10.211.55.5, control 49000, media 49010, feedback
49011; profile capped at 5 FPS and 2 Mbps. Host session artifacts:
`target/ubuntu-lan-validation/manual-20261013-122601/`. Both applications were
left running for owner testing; this is a transient development session, not an
installed/autostart deployment. Ubuntu worker uses a 1200-second session limit.
Do not assume either process or temporary VM paths survive until the next task.

Resume from `docs/28_UBUNTU_STUDENT.md` and
`checkpoints/diagnostics/2026-10-13-ubuntu-student-lan.md`. Next implementation:
per-user Ubuntu launcher/install and graphical-session autostart with persistent
profiles separate from binaries, followed by Ubuntu bootstrap/updater and paired
Student bundling. Native Wayland, Linux x64/24.04, Linux capture/Observe and OS
controls remain unverified or unimplemented as documented. Ubuntu is actually
22.04 ARM64 (owner initially described 22.05), VM `Ubuntu 22.04 ARM64`; use
Parallels guest tools without persisting SSH credentials.

Publication checkpoint includes the existing UI/store work and Ubuntu additions.
No new release build requested or produced; the permitted existing Teacher was
reused. This publication step runs whitespace/package checks; prior runtime and
Rust/Flutter validation remains recorded separately, rather than claiming a new
full test run.

## 2026-10-13 — Paired Windows build and live Teacher launch

Owner requested Windows Teacher + Student build, Teacher launch and bundled
Student ZIP ready to serve. Built both Rust x64 MSVC roles from source revision
9c6f201 with label paired-20261013-132814. Teacher portable bundle:
`target/Classroom-Console-Windows-x64/classroom_console.exe`; standalone Student:
`target/Classroom-Student-Windows-x64/ClassroomStudent.exe`. Fresh package is
included at `target/Classroom-Console-Windows-x64/student-packages/student-windows-x64.zip`.
Student SHA-256: 6c9dd294bf4de077b869634372b0b4b1c5b480e9fdfa9aefbe73ab923657af64.

System Flutter 3.0/Dart 2.17 was incompatible. Prepared isolated Flutter 3.47.7 /
Dart 3.13.5 in target/toolchains/flutter; stopped its optional git tag-history
fetch after it delayed setup. Used explicit x64 CRT directory under Visual Studio
18 Community, because runtime auto-discovery selected SQL Server tools. Both
portable bundles contain the matching runtime DLLs. Flutter SDK pins resolved
six lockfile dependencies differently; preserved the actual build lockfile at
`target/teacher-live/build-pubspec.lock` and restored generated tracked files to
their original contents. No product source changes.

Runtime verified: Teacher window and owned Rust agent running. Initial saved
binding was loopback; gracefully quit/relaunched with CLI --bind-ip 10.0.68.200,
the physical Ethernet 4/default-gateway adapter. Preserved settings/profile/logs.
Download service left running on http://10.0.68.200:8888/ (80 occupied by OS;
8888 was selected in the live UI). Actual HTTP GET returned 5,512,831-byte ZIP;
CRC, Student executable SHA-256 and bootstrap teacher_ip/control_port verified.
Bootstrap points to 10.0.68.200:47800. Evidence:
`target/teacher-live/verification.json`. Student --help executable smoke passed.
This is a local HTTP download check, not a second-PC/firewall or media acceptance.

Checks: release builds passed; git diff --check passed. New-SDK Flutter analyze:
8 informational findings, no warnings/errors (nonzero lint status). Flutter tests:
79 passed, 9 failed in class_control/observe UI cases; no claim of clean regression
suite. An initial analyzer invocation accidentally selected the old system SDK;
its incompatible-SDK findings were discarded and analysis rerun explicitly with
the local SDK. cargo fmt --check reports existing repository formatting differences;
no broad formatting changes applied. No physical 40-PC/performance test performed.

Exact next step: on a Windows Student PC open http://10.0.68.200:8888/, extract the
ZIP and run ClassroomStudent.exe; verify roster registration and classroom media.
Investigate the nine Flutter UI test failures separately before release acceptance.

Checkpoint commit could not be created: this Windows Git installation has no author name/email configured. No identity was invented or global Git configuration changed.


## 2026-10-13 — Owner override: Student presentation stretch default (in progress)

Owner explicitly changes the previous aspect-preserving Student display decision: Windows class presentation defaults to stretch-to-fill, accepting distortion; Teacher Settings must offer stretch and preserve-aspect. Source capture/encode dimensions remain unchanged. During the live class, only background non-disruptive checks are allowed on occupied endpoints; visible presentation and test update are restricted to the vacant IP suffix .117. Do not broadcast to the whole class or update other Students. Plan: additive Protobuf StartPresentation flag, persisted Teacher video setting, shared GPU/GDI display geometry, focused tests, paired build in an isolated output, and targeted .117 validation.


## 2026-10-13 — Student stretch default, Observe Fill, targeted .117 update

Completed implementation/build and targeted updater deployment; owner visual QA
is pending. Windows Student presentation now stretches to the complete display
by default; Teacher Settings -> Phat man hinh offers stretch or preserve-aspect,
persisted as video.presentation_preserve_aspect. Applies to the next presentation
after Save & reconnect. GPU/GDI use shared geometry. Additive Protobuf tag 9 in
StartPresentation, no protocol-version bump; old Students ignore it and retain
letterboxing. Existing capture/encode/transport/queues and non-Windows renderers
retain their behavior. Student-source relay uses Teacher's chosen display policy.

Owner also requested removing Goc/Native from Observe/Control toolbar. English
Fill and Stretch remain, default Fill. Initial source/DPI/work-area window sizing
is retained; manual resizing now expands the image with the window by default.
Stretch retains existing aspect-preserving cover/scroll and input mapping.

Validation:
- cargo test -p renderer-windows -p protocol -p classroom-config -p control-transport
  --target x86_64-pc-windows-msvc: 25 passed.
- cargo clippy for those four packages with --all-targets: completed, warnings
  remain (including independent renderer boolean fields); no errors.
- Flutter presentation_display/settings/observe_startup tests: 6 passed.
- Focused Observe test 'Fill is the only default': passed for default toolbar,
  resizing in Observe and Control, and normalized midpoint coordinates. Total
  focused Flutter checks: 7 passed.
- Broader existing 'Independent fit window' test still fails on its pre-existing
  English window-title expectation versus Vietnamese runtime title and toolbar
  overflow at 800px. Not a clean full Observe regression claim. The prior full
  suite's nine failures remain recorded above; no unrelated repair was attempted.
- Flutter analyze: 9 informational lints, no warnings/errors, nonzero info status.
- Dart formatting applied to changed Dart files; targeted new geometry formatted.
  Whole-repo Rust formatting differences remain as recorded above.

Paired x64 build label stretch-20261013-135048. Isolated Flutter 3.47.7, explicit
VC145 x64 CRT; both fresh Rust roles and fresh bundled Student ZIP. Full paired
script rerun after the Observe toolbar changes (same label, unchanged Rust input).
Student SHA-256 f1e5a70cee83c8f769a704af048e0ccd98e3d7d4d1f44c6b7d00311ba5f4fb12.
Build lockfile preserved at target/teacher-live/stretch-build-pubspec.lock;
SDK-generated tracked files restored as before. No dependency migration intended.

Only 10.0.68.117 was deployed, using classroom-dev-update.py with explicit
--student 10.0.68.117. Job 02bc8992b4edc65ab4c9831eabdf8cb7 reached complete after
restart, same Student ID, build 1791874255-stretch-20261013-135048. Other Students
were not updated. No Share/Observe/Control/input/lock command was issued to any
Student by the agent during this task. Owner may test the vacant .117 manually.

Teacher was idle before graceful quit. Automatic command review blocked removal
of target/Classroom-Console-Windows-x64, so no replacement/deletion was retried.
Per owner fallback rule, active Teacher is now the isolated package at
`target/Classroom-Console-Windows-x64-stretch/classroom_console.exe`, same saved
settings/profile/log locations and same private IPC endpoint. No old Teacher
process remained when the new package launched. Old package remains on disk.
Do not launch both. Teacher binds physical Ethernet 10.0.68.200. Fresh Student
ZIP share restarted on http://10.0.68.200:8888/; actual GET verified ZIP CRC,
executable hash and bootstrap 10.0.68.200:47800, 5,511,914 bytes. Evidence:
`target/teacher-live/verification.json`, `stretch-roster-verification.json`.

No physical display/media test was initiated while class was active. Unit/UI and
update/download evidence do not prove visual ratio/cursor behavior or performance.
Exact next step: owner tests Share selected to .117 and both display settings,
then Observe/Control opening size and resize behavior on .117. Update remaining
Students only after class and owner authorization. Git commit remains unavailable
because this machine has no author name/email configured; do not invent identity.

## Recording status in roster — 2026-10-13

- Added a small red recording dot with tooltip to Grid, List and Compact student rows; right-click switches to “Dừng ghi màn hình” for the selected student while recording or starting.
- Right-click stop now identifies both session and student. Added target-scoped stop state in the journal so stopping one student does not stop other students recorded in the same session; older journal entries default the new flag to false.
- Verification: Flutter widget tests pass for all three views and the context menu; focused Rust recording test passes, including proof that stopping Student A leaves Student B recording. `flutter analyze` reports only existing informational lints. `git diff --check` passes.
- Paired Windows x64 build `recording-indicator-20261013` completed in isolated `target/Classroom-Console-Windows-x64-recording-indicator`; fresh Student ZIP CRC verified, version `recording-indicator-20261013`, SHA-256 `9a04620a0d36b44996c79d36e01c08849f843ba91aac4fcb06fd3c437797f4f0`.
- Runtime verification: launched the isolated Teacher with the existing `target/teacher-live` room store and explicit LAN bind `10.0.68.200`; verified listener `10.0.68.200:47800`, phase idle, and 15/15 Students online. The Student share is listening on `10.0.68.200:8888`; HTTP GET returned 200, full ZIP CRC passed, and embedded bootstrap names Teacher `10.0.68.200:47800`. No presentation/Observe/Control was started.
## 2026-10-14 — Mac Teacher/Student signed with Apple Development cert

- Valid identity `A9F06E151E345F70B12858DF2BC32945BFF60945` (Apple Development, team `JM63BUYLH3`, expires 2027-10-03); three older identities are revoked. Build with `CLASSROOM_MAC_SIGN_MODE=apple CLASSROOM_MAC_SIGN_IDENTITY=<sha1> scripts/build-flutter-macos.sh`.
- `scripts/sign-macos-app.py`: `verify_app` now skips only the `spctl` Gatekeeper assessment for non-Developer-ID certs (Apple Development is never accepted by Gatekeeper). Integrity, OCSP, team-ID and manifest requirement checks remain; Developer ID still requires Gatekeeper + notarization.
- Verified: Teacher, Agent and Student share team ID; designated requirement is bundle ID + Apple anchor + cert CN (not cdhash). Two consecutive builds in the same path produced identical requirements and manifests; deep/strict codesign passes.
- Runtime: owner granted permissions once, rebuilt in place, reopened Teacher and saw no permission prompts (owner-observed, including Screen Recording/Accessibility/Local Network). Mac Student `192.168.50.2` not yet redeployed with the Apple-signed build; it will need one manual approval.
- Next step: deploy Apple-signed Student to the Mac Student when needed and keep bundle ID/path/cert unchanged.

## 2026-10-14 — Compact recording dialog

- `apps/classroom_console/lib/recording.dart`: session cards redesigned to be compact for both tabs (Đang ghi / chờ tải, Lịch sử). One-machine session is two lines (name + status badge + actions; date · phase · duration · resolution · FPS · chunks); multi-machine session lists one row per Student with right-aligned metrics. Folder path moved into the "Mở thư mục" tooltip; thin upload progress bar; red/amber/grey status colours; error and offline notes only when present. Dialog height 470 → 500. All strings used by existing tests unchanged.
- Verified: `flutter analyze lib/recording.dart` clean; `test/recording_test.dart` 5/5 pass; rendered screenshot of both single- and multi-target cards reviewed (icon glyphs are tofu only because the test environment lacks the icon font).
- Full `flutter test` has 9 failures in `observe_test.dart`, `class_control_test.dart` and `lifecycle_test.dart`; identical set with the original `recording.dart`, so pre-existing and unrelated. Not investigated here.
- Not built into a Mac/Windows package and not run on a live Teacher. Teacher-only UI change; no Student binary change, so the Windows test PC was not used.

## 2026-10-14 — Observe/Control open latency (Windows Student 192.168.50.166)

- Change under test (`apps/classroom/src/monitoring.rs`): Teacher dispatches the Observe start immediately while a thumbnail reply is pending; Student ends its snapshot-interval wait as soon as a command is readable (empty `ThumbnailFrame`, no pixels) instead of sleeping out the interval.
- Verified: `cargo test -p classroom-app --bin ClassroomStudent monitoring` 16/16 pass, including `observe_command_interrupts_pending_student_snapshot_interval` (<500 ms). Built in an isolated target dir because `target/` held artifacts from a different rustc.
- Paired build `observe-fast-20261014` via `build-flutter-macos.sh` (Apple Development signing, stable requirements verified). Old Teacher GUI quit, owned Agent helper (PID 42352, verified path) sent SIGTERM, Teacher relaunched from the same path. Only Windows Student 192.168.50.166 deployed via `classroom-dev-update.py` (complete, build confirmed). Mac Student not updated.
- Runtime: 6 `observe_open` trials against the real Windows Student, spaced 3.5 s apart so the 2 s snapshot cadence was active: IPC ack 4–10 ms, phase live 35–45 ms. Teacher log agrees: dispatch queue wait 0.6–13 ms, Student handshake 9–23 ms, source ready ~29 ms.
- Limits: measures control-plane readiness only; no Observe GUI window was opened, so first-video-frame and render time are unmeasured. No baseline run of the old build was taken. Only the Windows Student has the fix.
- Next: owner clicks a Student to confirm it feels instant with the window; then update remaining Students after class.

## 2026-10-14 — Audit of Teacher child windows (open latency)

- Method: `winwatch` (CGWindowList, 5 ms poll) timestamps the first on-screen window of each spawned `classroom_console` child versus the IPC command; Windows Student 192.168.50.166, Mac Teacher. Scratch scripts only (not committed).
- Finding 1: every child window (terminal, files, activity, internet, vscode, apps, hardware, observe) appears ~215–260 ms after the command on the Mac Teacher; ack 2–6 ms. All spawn at once and show after the first frame, independent of network. This is the Flutter process-start floor; no per-window outlier. Windows Teacher startup was NOT measured.
- Finding 2 (fixed): Remote Terminal status (`READY`, kind 5) was sent by `interval(1s)`; "connected" reached the UI at a constant ~1008 ms, and `sendInput` drops keystrokes until connected. `remote_terminal/teacher.rs` `attach()` now pushes status on change (50 ms check) with a 1 s keep-alive. Measured `terminal_open` -> shell connected: 1008 ms constant before; after 16 trials 208–1159 ms, typically 260–560 ms (median ~330 ms).
- Fix 3: result-poll loops in files (120 ms), activity (250), apps (600), vscode scan (700) now use `resultPollDelay` (40 ms backoff, ceiling = old interval). Student tools poll no longer sleeps 200 ms after receiving a real command (queued work is fetched immediately).
- Not changed: Student idle poll still 200 ms (avg ~100 ms to deliver a command); Student-side shell spawn time; Internet 900 ms is an intentional confirmation delay before auto-close.
- Verified: classroom-app Student tests (67 in remote_terminal/classroom_tools) pass; `flutter analyze` only pre-existing infos. Paired build `window-open-20261014`, Apple-signed; Mac Teacher relaunched, only Windows Student .166 updated. Window-open recheck after the build: see numbers in chat; runtime UI feel not owner-verified yet.
- Next: owner checks the Terminal window feels immediate; consider Teacher-side long-poll to cut Student poll latency; measure Windows Teacher window start on a Windows machine.

## 2026-10-14 — Slow / unresponsive Student handling in Teacher windows

- Owner requirement: lab PCs will be slower than the dev machines; every Teacher window must stay usable and informative when a Student is slow, late or silent.
- Remote Terminal (`remote_terminal.dart`, `remote_terminal/{teacher,wire}.rs`): status line now reports elapsed wait ("phản hồi chậm (Ns)" from 3 s, "Vẫn đang chờ…" from 10 s), names an offline Student, and a failed/ended session offers "Thử lại" / "Mở phiên mới" (closes the old session, opens a new one) — previously a failed start left a dead window with no retry. Never-connected give-up 15 s -> 30 s (`CONNECT_GRACE`), reconnect give-up 15 s -> 30 s; messages in Vietnamese. Window accepts an injectable clock for tests.
- `Agent.failure` translates raw errors (TimeoutException, "Student is offline", "not connected") into Vietnamese guidance; three "Student is offline" strings in `agent.dart` localized.
- Files/Activity/Apps/VS Code already had offline notices, 20–60 s result deadlines with a message, and refresh; unchanged apart from the earlier faster polling.
- Verified: new `test/remote_terminal_slow_student_test.dart` (3) + existing terminal/agent tests pass; Rust remote_terminal tests 18/18. Runtime on real Windows Student 192.168.50.166 (paired build `slow-student-ux-20261014`): suspended ClassroomStudent via NtSuspendProcess, `terminal_open` ack OK, session stayed "connecting", Teacher ended it at 30.0 s with the Vietnamese message; Student resumed and 3 further sessions connected in 208–461 ms.
- Not verified: the visual banner/button in the real window (widget-tested only); Students on genuinely slow hardware; high-latency or lossy network (only a full process stall was simulated); other windows under a stalled Student.
- Next: owner views the Terminal against a suspended/slow Student; consider the same wait-time + retry pattern for Hardware/Internet/Apps apply steps (15 s request timeout) and a Teacher long-poll to cut Student poll latency.

## 2026-10-14 — Apply/Save buttons enabled only by a real change

- Audit of every dialog/window with an apply-type button. Already correct: VS Code (`changed` vs baseline). Fixed:
  - Internet (`internet_access.dart`): Apply was always lit. Now compares a policy signature (mode + effective rules) with what is in force: last applied in this window, else "open" when none of the target Students is restricted, else the stored `internet_applied[scope]` preference (written on each successful apply). Window opens on the mode actually in force (nothing restricted -> "Cho phép tất cả"), so nothing looks pending until the Teacher edits; reverting an edit disables Apply again; unknown state with restrictions in force keeps Apply enabled.
  - Apps (`app_access.dart`): with no restriction active, Apply was enabled with nothing selected (clicking only produced an error); now needs at least one ticked app. With a restriction active it still needs an actual edit vs the applied set.
  - Teacher settings (`settings.dart`): "Lưu & kết nối lại" enabled only when a saved value differs from the opening values (incl. the connection-popup switch that acts immediately but is remembered by Save). The Student-share port is not part of Save and is excluded.
  - Terminal settings dialog, Launch-program edit dialog, Internet browser-path dialog and list name dialog: Save/OK enabled only on a real difference (and non-empty where required).
- Not Apply-type (left alone): Hardware toggles apply immediately; send-message/Open-website/Start-collection/Run are actions.
- Tests: new `internet_apply_test.dart` (2), `app_access_test.dart` (+1), `settings_test.dart` (+1, existing test now pumps a frame after typing, as a real user has). `flutter analyze`: only pre-existing infos. Full `flutter test`: 94 pass, same 9 pre-existing Observe/class-control failures as before.
- Build `apply-gating-20261014` (Teacher + Student, Apple-signed); Teacher relaunched. No Student change, so no Student redeploy. Visual check in the real windows not yet done by the owner.

## 2026-10-14 — VS Code extension icons in the allowlist window

- Same two-phase pattern as application icons. Phase 1 (unchanged cost): each Student returns the extension list with no images. Phase 2: for ids whose icon the Teacher lacks, exactly one Student that has the extension is asked (`vscode_list` now carries `{"icons_for":[ids]}`, chunks of 40, owners rotated), so one icon is never uploaded by many machines. Old Students ignore the field and simply send no icons.
- Student (`vscode_control.rs`): reads the manifest `icon` (PNG next to `package.json`); path must be relative, without `..`, `.png`, canonicalized inside the extension folder, <= 2 MB. A file <= 16 KB and <= 128 px is forwarded byte-for-byte; anything larger is decoded and box-filtered (alpha-weighted) to <= 64 px and re-encoded, dropped if still > 48 KB. SVG and unreadable files are skipped (generic puzzle icon shown).
- Teacher (`vscode_access.dart`): icon decoded once into bytes (no per-rebuild decode), shown at 30 px with `cacheWidth: 64`; persisted per extension in store namespace `vscode_icons`, so reopening shows icons immediately and only new extensions are requested.
- Verified: Rust tests for raw forward, shrink (512 px noisy -> 64 px <= 48 KB), path escape/absolute/SVG/garbage, only-requested-ids; Flutter test `vscode_icons_test.dart` (each id asked of exactly one Student, image shown, generic fallback, cache written). Real Windows Student 192.168.50.166 (build `vscode-icons-20261014`): list without icons 1140 B / 191 ms; with icons for all 6 extensions 46,908 B / 340 ms, six icons 3.3-11.4 KB (128x128 and 64x64) all received.
- Not verified: the rendered window (owner to look); macOS Student extensions (same code path, not run); multi-Student distribution beyond the unit test; extensions whose icon is SVG-only get the generic icon by design.

## 2026-10-14 — Application control: block-only list, path-rule tab, new footer

- Owner request: redesign the bottom bar, drop "Chỉ cho phép ứng dụng đã chọn" (ticked app = blocked), add an independent tab that blocks by path (a `.exe` or a folder; folder = every `.exe` below it) with Windows-environment-style CRUD.
- UI (`app_access.dart`): two tabs "Theo ứng dụng" / "Theo đường dẫn" (badge = what is in force). Footer is now one line — summary + "CHƯA ÁP DỤNG" marker on the left, [Gỡ hạn chế] [Áp dụng] on the right; the segmented mode switch and its caption are gone. Path tab: list with file/folder chip, Mới… (several lines at once) / Sửa… (also double-click) / Xóa, selection is instant (raw pointer listener — a double-tap recognizer delayed plain taps ~300 ms). Entry dialog validates (absolute path, no `"<>|*?`, no `..`, not a drive root, not C:\Windows, duplicates) and its Save/Add button needs a real difference.
- Independence: each tab's Apply sends the other tab's *applied* state unchanged, so editing one never ships the other's draft. Legacy "allow_only" rules still in force are shown with a notice, ticks start empty, and the next Apply replaces them.
- Protocol/Student (`app_control.rs`): `AppPolicy.paths` (<=100, <=300 chars); `active()` = ids rule enabled OR any path; fingerprint appends `|p:<sorted lower paths>` only when paths exist (policies without paths keep the old hash). Rules expand `%VAR%`, normalize (case, `/`, repeated separators), refuse `..`, drive roots, Windows dir and the Classroom folders. Enforcement is by the watcher (200 ms tick when path rules exist, 500 ms otherwise) comparing each process image path; matches are killed and the Student shows the usual "blocked" notice (rate-limited per exe). IFEO is not used for paths because it is keyed by file name and would also block same-named programs elsewhere. Rules persist in the same HKLM state, are re-read when the Student starts, and lift at the end of the day like other blocks. Tool packet limit 8 KB -> 64 KB, policy text limit 8000 -> 48000 (old Students still accept the unchanged small policies).
- Bug found by the new tests and fixed everywhere: dialogs disposed their `TextEditingController` the moment `showDialog` returned, while the closing animation still rebuilt widgets listening to it (Internet browser/list-name dialogs and the Terminal settings dialog from the previous Apply work had the same flaw). New `disposeAfterClose` in `app_dialog.dart`.
- Verified: Rust `app_control` tests (path normalize/match/refusal/validation/hash) and full Student suite 271 pass; Windows cross-check compiles; Flutter: new path-tab test (validation, add/edit/delete, Apply gating, exact policy sent: paths + baseline ids + enabled) and earlier app tests pass; full `flutter test` 96 pass, the same 9 Observe/Class-control failures as before. Real Windows Student 192.168.50.166 (build `app-paths-20261014`), using a copy of ping.exe: control runs; folder rule kills it and a sub-folder copy within the 2 s check; same-named program outside stays alive; file rule `%SystemDrive%\cbt\SINGLE.exe` (env + case) blocks only that file; `C:\Windows\System32` rejected by the Student with "System folders cannot be blocked"; clearing restores the program.
- Not verified: the rendered window; the notice box for path kills; a rule covering a program that is already running at Apply time was not exercised separately; reaction time is up to ~200 ms plus process start (a blocked program may flash before it is closed) — a path-aware launch stub could remove that but risks breaking legitimate launches; macOS/Linux Students ignore app rules as before.

## 2026-10-14 — Toasts, Dock/taskbar raise, site favicons

- Teacher toast cards (`classroom_tools_ui.dart`): restyled to the console's light theme (white card, coloured left stripe, dark text). Titles now name the operation ("Không áp dụng được chặn ứng dụng", VS Code, Copilot, message, launch, file) instead of defaulting to "Tin nhắn chưa được chuyển"; common Student errors are shown in Vietnamese. Identical failures merge into one card ("×3"); at most three cards plus "Ẩn tất cả". Failures needing no action hide after 10 s (success 5 s, as before); a failed file/update transfer stays because it offers Retry. Close button 36 px.
- Why "X sometimes works": the overlay showed only two cards, so after dismissing one the next queued identical failure (every test/retry of the same rejected policy created another job) took its place. A widget test confirmed the dismiss itself, including a rebuild mid-tap, was fine. Now one dismissal removes the whole group and nothing identical replaces it.
- macOS Dock: root cause was `AppDelegate.applicationShouldHandleReopen` — a Dock "reopen" delivered to a child-window process (Observe, files, activity, Internet, VS Code, apps) was forwarded as `reopenConsole`, raising the *console* over the window. Now a child with a visible/minimized window raises itself (deminiaturize, key, activate); only a child with no window left routes to the console. Terminal and Hardware windows were added to the child list; `applicationDidBecomeActive` also orders a child's window front. Reproduced with a tool that sends the real reopen AppleEvent to a PID: before — console stayed on top; after — apps and files windows come above the console. Windows: no code redirects a child's taskbar click to the console (each child is its own process/taskbar button), so nothing was changed; not measured (no Windows Teacher in this session).
- Favicons (`app_icons.dart`, `open_website.dart`, `internet_access.dart`, `main.dart`): the existing Teacher-side `SiteIcon`/`FaviconCache` is reused for recent websites, the Internet suggestions and the top-level website groups (not for dependency domains). The cache now keeps icons as <=64 px PNG in the Teacher store (`favicons`, keyed by host, refreshed after 30 days) and every window process binds to it, so a freshly opened window shows them without refetching; a failed fetch keeps an older icon. Real sites: github.com 966 B, wikipedia.org 676 B, python.org 2.5 KB, stackoverflow.com 2.9 KB in 0.4–1.1 s; an unreachable site yields none (globe).
- Verified: new/updated tests (toast grouping/dismiss/auto-hide/file retry stays, favicon cache with a loopback server and store reuse); full flutter test 100 pass, same 9 Observe/Class-control failures. Build `dock-toast-favicon-20261014` (paired, Apple-signed) running.
- Not verified: the rendered toast/favicon UI by eye; Windows taskbar; Student .166 was unreachable this session (ARP incomplete; not started, no SSH key to the Mac Studio host), so nothing was deployed to a Student — these changes are Teacher-only.

## 2026-10-14 — Windows re-run on 192.168.50.76 (Student reconnect + latest build)

- The Windows VM came back at 192.168.50.76; the Teacher (DHCP) listens on 192.168.50.27 while the Student still pointed at the old Teacher addresses. Updated `C:\Users\mvmanh\Classroom\student-launch-args.txt` (the effective source), `profile.toml` and `teacher-ip.txt`, restarted ClassroomStudent: online at once. Then deployed Student `dock-toast-favicon-20261014` via `classroom-dev-update.py` (confirmed).
- Re-run on the real Windows Student: path rules — folder rule kills `C:\CBT\np.exe` and `sub\deep.exe`, an outside program survives, file rule `%SystemDrive%\cbt\SINGLE.exe` blocks only that file, `C:\Windows\System32` refused, clearing restores; VS Code extension icons — 6/6 received (46,908 B, 185 ms); Terminal — shell connected 312–457 ms; Teacher-opened windows (terminal, files, apps, VS Code, Internet) appear 212–261 ms; Dock reopen of Teacher-opened apps and files windows puts them above the console.
- Not re-run: Windows Teacher/taskbar behaviour (no Windows Teacher in this environment).

## 2026-10-14 — Student finds the Teacher by itself (build teacher-discovery-20261014)

- Implemented the design recorded in `docs/11_DECISION_LOG.md` ("Teacher location"): new `apps/classroom/src/teacher_discovery.rs`. Student: manual address → last good (profile doc `teacher_last`) → `.200` of its own /24 probed together at start; on failure retries 0.5/1/2/3 s then 5 s (±20 % jitter); after 12 s of silence probes its own /24 (128 parallel, 300 ms, every 45 s); a candidate is accepted only after a real registration, remembered, then the Student restarts once so Guard/tools/monitor use the new address. A Teacher verified in the last 60 s outranks a manual address (no flip-flop). `network.teacher_discovery` (default on; Student settings field "Find the Teacher automatically", 0 = only the configured address, nothing probed, knocks ignored). Teacher: remembers Students seen (store `knock/students`) and sends a UDP knock (control port + 20) to those not connected after 1/2/4/8/16 s then every 30 s; a disconnected Student tries the sender at once.
- Why restart instead of live re-pointing: the Teacher address is baked into Guard firewall rules, hardware control, recorder and tools/monitor workers at start; one clean restart is simpler and safer than threading a mutable address through all of them.
- Verified: 8 new unit tests (priority order, recent-Teacher precedence, discovery-off pinning, /24 scan list with .200 first and no self, knock packet round trip and wrong-port rejection, listener on a real UDP socket, probing order on real TCP listeners, back-off/jitter bounds, alternatives incl. knock first); Student suite 279 pass (settings test updated for the new field); Windows target compiles; paired Mac build ok, Teacher running (192.168.50.27:47800).
- NOT verified over the network: the Windows VM (192.168.50.76) went unreachable again right after the build, so the new Student was not deployed to it and no end-to-end discovery, knock or restart-on-new-address run has happened yet. A Student on the same machine as the Teacher cannot discover it (the scan skips the Student's own address).
- Known gap: no Teacher authentication beyond protocol registration; a room key / pinned identity is the next hardening step.
- Mac Student for manual testing: `target/Classroom-Student-macOS.zip` (Apple-signed `target/Classroom Student.app`, sha256 1dfaa5f84569d8591732486e87099f61f9a9d5aeb887a88385d8daff2bf5ef0c).

## 2026-10-14 — Discovery verified on the Mac Student (192.168.50.2), build teacher-discovery2-20261014

- Why the owner's first test showed "Teacher .28": the new zip was unpacked to `~/Downloads`, but macOS runs one process per bundle id, so opening it only brought the already-running OLD Student (`~/Classroom`, build recording-validation-20261012, stale configured Teacher .28, no discovery) to the front. Killed the old process and ran the new app.
- Real run on 192.168.50.2 from a clean profile with a wrong manual address (`--teacher-ip 192.168.50.28`, nothing remembered): scan of 251 hosts in 0.63 s found the Teacher at .27, verified by registration, restarted, connected. First build: 16.6 s from start; after capping a registration attempt at 5 s (a macOS connect to a missing host hangs ~13 s) and starting the scan after 6 s instead of 12 s: 9.7 s.
- Teacher quit and restarted at the same address: Student back online 5.4 s after the Teacher listener came up (knocks sent at 0.5/1.5/3.5 s). The knock is only consumed when the Student's retry loop next looks, so the worst case equals the 5 s retry cap; waking the back-off sleep on a knock would shave that (not done).
- Deployed: Mac Student replaced at `/Users/mvmanh/Downloads/Classroom Student.app` on .2 (Apple-signed, same bundle id/path, so the permissions the owner granted should hold) and relaunched; Teacher relaunched. Windows VM (.76) was unreachable, so Windows still runs the previous Student build.

## 2026-10-14 — Windows Student (192.168.50.165): old vs new connection time, build teacher-discovery3-20261014

- VM moved again, now 192.168.50.165. Measured the previous Student (no discovery, fixed 0.5 s retry) first, then deployed the new paired build through the updater (old `.classroom-update-*` folder removed by the updater, `ClassroomStudent.exe` replaced; the new update's staging folder stays until the next update) and measured the new one. Mac Student 192.168.50.2 updated to the same build.
- Results (Teacher = Mac 192.168.50.27; times from the Teacher log):
  - A) Teacher quit 20 s, then started: old 0.1 and 0.4 s after the Teacher is ready; new 0.5 and 0.6 s (Teacher knock wakes the back-off; the first build before this change took ~5 s here, so knock-wake + 3 s cap were added).
  - B) Student started with a wrong Teacher address (.28) and a fresh profile: old never connected (70 s); new connected after 17.2, 7.4 and 6.9 s (first run includes the Student's fresh-profile start; scan after 6 s, 5 s attempt cap).
- Side effect of test B: the three fresh test profiles registered as extra offline "MAIVANMANHED30" rows in the Teacher roster.
- Windows Student config restored (`--teacher-ip 192.168.50.27 --control-port 47800`, default profile).

## 2026-10-14 — Extension names, recording manager, dialog buttons, Student popups (build student-dialogs-20261014)

- VS Code extension names: manifests that use `%displayName%` / `%description%` placeholders are resolved from the extension's `package.nls.json` on the Student (`vscode_control::manifest_text`, Windows and macOS lists); the Teacher also falls back to the extension's own name for an older Student. Real Mac Student: 53 extensions, 0 unresolved; Windows: 6/6.
- Recording: the manager dialog no longer has a bulk "Ghi N máy đã chọn" button (the ribbon "Ghi màn hình" only opens the manager; recording is started per computer by right-click). Each session card has a delete button (disabled while capturing) that removes the entry and the downloaded `Part-NNNN.mp4` files (and then the emptied folders; never anything else, never outside the Desktop). The Student drops what it still holds when the Teacher no longer knows the session (`discard` verdict). Real Windows Student: record 12 s, stop, 1 chunk received, delete -> session gone, file and folder removed, second delete rejected cleanly.
- Recording bug found while testing: the Student rejected a recording when its clock was even slightly behind the Teacher's ("Invalid recording deadline", zero tolerance). Now tolerates a Teacher clock up to 10 minutes ahead.
- Dialogs: removed the "Ðóng" button from every `AppDialog` footer that already has the X (monitoring badge, recording, diagnostics, replies/disconnects lists, file collection, launch, open website, activity copy). Internet window (a window, not a dialog) keeps its Hủy/Đóng.
- Student popups (native, software-drawn): new Canvas primitives (anti-aliased rounded rectangles, discs, round-cap lines, text at any size). Message/reply popup: white card with accent strip, chat badge, title, × button, 15 px text, rounded Reply/OK buttons, reply box with focus border and character counter, pointing-hand cursor on buttons, I-beam over the reply box. File popup: Show button removed; a finished file row highlights across its whole width on hover, shows "Click to show in folder", the cursor becomes a pointing hand, and a click reveals the file; header has quiet minimise and close icon buttons (close disabled during transfers); in-flight rows show size, percent and a rounded progress bar; finished rows show a green check. `vendor/minifb`: `OpenHand` now maps to the macOS pointing-hand cursor (Windows/X11 already used a hand) and macOS no longer caches the cursor.
- Verified on the real Mac Student (.2) by sending a message and files from the Teacher and taking a Student screenshot: popups render as designed. Hover/click/cursor behaviour is by code and unit tests only (no way to move the mouse remotely). Windows popup rendering not looked at.
- Tests: Rust 281 pass; Flutter 102 pass, same 9 Observe/Class-control failures as before.
- Deployed: Windows Student .165 via updater; Mac Student .2 replaced under ~/Downloads (updater cannot replace an app running from there). Test files created on the Student Desktop were removed.

## 2026-10-14 — Reply toast title, file-list Clear, protected paths, recording IPs (build toast-clear-protected-20261014)

- Teacher reply toast: one reply -> title "Phản hồi từ hostname (ip)" and only the reply text below; several replies keep the per-line "name (ip): text".
- Student "Files from your teacher": new footer with "+ N earlier" and a "Clear" link (bin glyph, hover highlight, pointing hand) that forgets the finished files and keeps anything still arriving; the window closes if nothing is left. The X now only hides the window and keeps the list (before, reading the code, X also forgot the finished files, which did not match what the owner saw); a new offer reopens the window with the whole list, detected by transfer id instead of list length. Footer is always reserved so the window is not rebuilt when a transfer finishes. Click/hover behaviour verified by code and unit tests only.
- Protected paths (Student `app_control.rs` + Teacher input check, nothing listed in the UI): refused with everything below: Windows (`SystemRoot`); refused as a whole or as an ancestor but allowed below: Program Files, Program Files (x86), ProgramW6432, ProgramData, Users, the user profile and Public, Recovery, Boot, $Recycle.Bin, System Volume Information, PerfLogs, Documents and Settings, plus the Classroom folders (both directions). Real Windows Student: Windows, System32\drivers, Program Files(+x86), ProgramData, Users, the profile, %ProgramFiles%, %WINDIR%\Temp refused; `C:\Program Files\SomeGame` and `C:\Games` accepted; cleaned up.
- Recording manager: each computer is shown as "hostname (ip)" (address from the roster, which keeps offline computers too).
- Tests: Rust 281 pass; Flutter +reply-toast, +recording address, +protected-path input checks; deployed to Windows .165 (updater) and Mac .2 (manual replace).

## 2026-10-14 — Student sharing is one switch (build share-switch-20261014)

- Root cause of the endless "loading" in Teacher settings > Cài đặt máy học viên: the agent wrote the share's `preparing` state into the published snapshot once, at start, and never re-read it, so the UI never saw `sharing` (nor the download counter). The publish step now copies the share's current state every tick.
- UI: the port field, the Share / Stop buttons, "Chọn ZIP Student Windows…" and the progress bar are gone. One switch "Chia sẻ ứng dụng Student đi kèm", off by default, always port 8080 (Rust default is 8080 too). When on it shows the address, architecture/size, download count and a copy button; a bind/firewall failure is explained under the switch and the switch stays off; a failed share no longer blocks switching on again.
- Real Teacher (192.168.50.27): start -> `preparing` -> `sharing` after 1.5 s, GET http://192.168.50.27:8080/ = 200 with the 15.6 MB ZIP, downloads counter 1, stop -> `off`. Flutter test (switch off, no port field/progress bar, start sends port 8080, sharing shows address and counts, off sends stop) passes. Teacher-only change; no Student redeploy.

## 2026-10-14 — Economy mode: weak network / weak Student PCs (build economy-mode-final-20261014)

- Plan and numbers: `docs/29_LOW_BANDWIDTH_MODE.md`; the other settings ideas from the NetSupport/Veyon comparison are saved in `PROJECT_NOTES.md`.
- `[performance]` config: `mode` normal / economy / minimal, `auto_wifi` (economy while the Teacher's adapter is wireless), `bandwidth_cap_mbps`. The Teacher agent runs on `Config::with_profile` (only ever lowers 1080p30 / bitrate / Observe fps / thumbnail interval, size and quality, turns off enhanced Observe and HiDPI thumbnails) while the settings window reads and saves the raw values; each presentation gets `Config::for_stream(recipients)`: the class-wide cap becomes a per-Student share (75 % of the cap, floor 300 kbps) that also steps the picture down (>=4000 as chosen, 2000-3999 720p20, 900-1999 540p12, below 640x360 at 10 fps), and a wire budget of 2.4 x the cap. Thumbnails: new `monitoring.quality_floor` (default 85 = old behaviour) and `monitoring.hidpi`.
- Settings window (Phát màn hình): three-state selector with a plain description, the Wi-Fi switch (shown for normal mode), the cap field with the per-Student result and picture note (and a warning when the 300 kbps floor makes the cap unreachable), and a line stating what is in force (and why, e.g. "do đang dùng Wi-Fi"). Bug found by its test: Save did not light when only the cap field changed.
- Real measurements (Teacher on Wi-Fi, 2 Students, scrolling coloured text as workload, 20 s each): normal 1080p30 9.4 Mbps / 15 fps; economy 720p20 6.0 Mbps / 19.9 fps; minimal 540p12 2.4 Mbps / 12.0 fps; auto Wi-Fi resolved to economy; cap 6 Mbps 3.3 Mbps / 16 fps; static screen normal 5.1 Mbps / 29.6 fps vs economy 3.3 Mbps / 19.4 fps. Defaults unchanged (mode normal, no cap).
- Two bugs found by measuring: (1) a mode that also lowered the wire budget (100/40 Mbps) would starve a class of 40; removed. (2) A cap used directly as the wire budget left 5 % for keyframes and sent nothing; the budget is now 2.4 x the cap. The last factor (1.6 -> 2.4) is covered by unit tests but was not re-measured live: the worst-case workload (incompressible random text) under a 2 Mbps cap to two Students still sent almost nothing with 1.6.
- The measurement workload was a Terminal window of scrolling text that the Teacher shared to the Students for about 20 s per run (the owner saw it on the Students). It was closed afterwards with "close every window" in Terminal, which may also have closed any Terminal window the owner had open.
- Not done: per-Student mode (only the weak PCs), Student-side load limits, live re-measure of the 2.4 factor, Windows Student receive-side stats in each mode.

## Live performance toggle (2026-10-14)

Bottom bar of the Teacher window: three icon buttons (Hiệu năng cao / Tiết kiệm / Rất tiết kiệm) next to the online counter, default high, tooltip per mode saying what it does and when to use it. Command `set_performance {mode}` switches live (new presentations, thumbnails and Observe offer; no restart), persists to the `config/overlay` document and updates the settings dialog's values. Choosing high clears `auto_wifi`. Verified: cargo check, Flutter toggle + settings tests. Live LAN check still to do.

## 2026-10-15 — Owner-requested paired build and Teacher restart

- Built the current working tree (HEAD `13c5d8b`, including existing uncommitted changes) through `scripts/build-flutter-macos.sh`, label `paired-20261015-080453`: native Teacher, macOS Student, Windows x64 Student/Guard and Flutter release succeeded. Apple Development signing and stable designated requirements verified for Teacher/helper and macOS Student.
- Embedded both fresh Student ZIPs in `target/Classroom Console.app`; verified embedded files match the produced packages by SHA-256 and both manifests use the same build label.
- Stopped the old Teacher/helper (95659/95663) before replacing the stable bundle. Relaunched the same path; verified new GUI/helper PIDs 24475/24480 and log `Flutter console attached to Teacher agent` with processed commands. Settings/profile/logs preserved; no remote Student deployment performed.
- Verification scope: release builds, signing/package checks and local Teacher startup. No application test suite or LAN/media benchmark rerun for this build-only request. Next step: owner uses the restarted Teacher; runtime LAN/scale acceptance remains unchanged.

## 2026-10-18 — Test cleanup, commits and paired build (no LAN measurement yet)

- The nine Flutter failures (Observe x8, Class-control x1) predate the uncommitted work (also failed at 13c5d8b). Causes: (1) the Observe toolbar `Row` overflowed in narrow windows — it now scrolls its controls below 900 px; (2) fixtures never marked the Student online, which `live` requires; (3) stale expectations: English window titles/"Giãn" label, the 500 ms preview cross-fade, and Flutter key forwarding that macOS replaces with the native event tap (gated to non-macOS).
- Verified: `flutter test` all pass; `cargo test --workspace` 330 pass, 2 ignored (separate target dir, so `target/` was not cleaned). `flutter analyze` 11 infos; `cargo clippy` only pedantic warnings; `cargo fmt --check` still reports diffs in ~50 files (not applied, to avoid churn).
- Committed in two commits (Rust/docs/config, then Flutter). Paired build `paired-20261018-201234` (Apple Development signed) of Teacher + Mac Student + Windows x64 Student; Teacher relaunched from `target/Classroom Console.app`.
- NOT done: Windows Student 192.168.1.16 (Parallels ARM VM on this Mac) was not updated or measured. Host load average was 34–49 (Android emulator ~215 % CPU, another project's Dart process) and SSH to the VM repeatedly timed out at banner exchange; any media measurement on this host would be invalid. Next step: quiet the host, point the Student at this Teacher, then run the no-decode vs decode comparison.
