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
