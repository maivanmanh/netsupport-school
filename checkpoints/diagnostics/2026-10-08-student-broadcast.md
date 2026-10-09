# Student broadcast, receiver health and diagnostic ZIP — 2026-10-08

## Scope and architecture
Owner authorizes receiver health, manual diagnostic ZIP collection and presenting one selected Student to the class. Owner reports the existing Windows Teacher broadcast worked well for 35 real Windows Students at 1920×1080; that result is the owner's evidence and is not a newly measured result here.

Student publishing reuses `observation::Publisher` and the existing DXGI/H.264 media publisher. The selected Student captures, encodes and packetizes once. Teacher relays compressed RTP, rewriting only session SSRC; The relay does not decode or re-encode; the later Control feature decodes a separate interactive preview on Teacher. Downstream uses existing UDP pacing, repair cache, NACK and keyframe requests. Source is excluded from class presentation; current and future recipients can join. No video history or unbounded queue is added.

Teacher displays session-scoped receiver progress and freshness rather than equating TCP Connected with receiving an image. Legacy reports without health version show unknown. First report has no measured FPS interval, so its FPS is zero until a second sample. Student policy actions remain silent; this milestone adds Teacher-facing status only.

Diagnostic collection uses the existing bounded collection service: serial transfers, 512 KiB/s, 2 MiB log tail, maximum 128 rows. During presentation collection is queued until Stop. ZIP records offline/missing rows and excludes screenshots, messages, file contents and control tokens.

## Build and automated checks
- Paired trace build succeeded: Mac Teacher/helper, Mac Student, Windows x64 Student and Guard from the same working tree. Teacher contains fresh Mac/Windows Student ZIPs.
- Embedded Windows Student SHA256 matches freshly built EXE: `1d0a1431dde9be68c2a32c6556f1b20b367081ffc47995d0aa4c2ce6f418febb`.
- Classroom Rust tests: 189 passed, 2 ignored; diagnostics tests included (also separately 2 passed).
- Control transport: 13 passed; protocol: 1 passed.
- Earlier focused Flutter tests: 4 passed, including health labels and narrow-window toolbar regression. Latest analysis finds 2 informational braces lints in new diagnostics dialog; no analyzer errors. Clippy completes with existing and new pedantic warnings; these are not presented as a clean lint run.
- Python runner compilation and Git diff whitespace checks pass. Workspace-wide Rust formatting has pre-existing differences; no mass formatting performed.

## Native evidence — incomplete
Environment: Mac coordinator `192.168.50.206`, Windows ARM Parallels source `.94`, Windows ARM Parallels receiver `.106`. Windows Student binaries are x64. This is functional testing on VMs/Wi-Fi, not physical x64 Ethernet acceptance.

Isolated per-run Student folders and interactive Limited tasks preserve installed Student, Guard, profiles and settings. Temporary inbound firewall rules are limited to test EXE and the Teacher IP; removed during cleanup. Defender remains enabled. Runner saves Teacher/source/receiver logs even after failures and checks Teacher endpoint before deployment.

- Earlier network trial: source connected and encoded 1728×1080 using one software MF encoder. Receiver late joined but relay ended with `Student broadcast source disconnected/stopped` before decode/render. Root cause is not yet established.
- Debug and trace trials: reverse TCP connect from packaged Mac app immediately fails `EHOSTUNREACH / No route to host`, both Tokio and system-connect fallback. Terminal `nc` reaches the exact source IP and ephemeral port successfully. Source publisher starts, but receiver does not start in these failed runs.
- Current UI Local Network shows all classroom_console rows enabled. This does not identify each ad-hoc build unambiguously. Permission attribution after rebuild is a hypothesis, not a confirmed root cause. Owner was asked to manually toggle Classroom Local Network and agent will restart Teacher afterward; no privacy automation.
- Teacher log can be specified via Console `--log-file`, preventing other test instances from overwriting evidence.
- Native ZIP from both online Students, late join, reconnect, repeated sharing and final Stop are still pending. Do not describe this feature as runtime-verified.

Raw evidence: `target/student-broadcast-native-network-20261008`, `target/student-broadcast-native-debug-20261008`, `target/student-broadcast-native-trace-20261008`; trace log `target/student-broadcast-teacher-trace.log`.

## Exact next step
After owner refreshes Local Network permission, restart the same paired bundle, rerun isolated two-endpoint test and inspect control reader/writer logs. If transport starts, resolve original relay disconnect and verify rendering, late join, receiver reconnect, Stop/restart, source failure and diagnostic ZIP. Complete UI/lint cleanup and paired final build before declaring completion.

## Deferred lab verification
- Windows x64 physical Student source → Windows Teacher → 35–40 physical wired receivers; 1/5/10/20/35/40 ladder.
- Fixed 1920×1080 workload (text, scrolling, motion), actual bitrate and optical latency; compare with established Teacher-source baseline.
- One source encoder under class fan-out; source CPU/GPU/network and Teacher aggregate send/receiver metrics.
- Multiple late joins, simultaneous loss/reconnect, source disconnect/exit and clean recovery; no recursive source full-screen.
- Long soak, low-spec source, DPI 125%/150%, actual sleep/resume, Windows 10 Defender quarantine reproduction.
- Upstream Student publishing currently has one relay viewer; aggregate downstream feedback/adaptation and repair behavior under class-scale pressure still need measurements.

## Follow-up after owner Local Network refresh
Owner manually refreshed Local Network. Same trace bundle was restarted without rebuilding. Reverse TCP registration now succeeded, but relay ended with EHOSTUNREACH after receiver joined; TCP writer logged Broken pipe. Source continued its one-encoder pipeline until cleanup, so Source process exit is not established as the cause. Raw `target/student-broadcast-native-regrant-20261008`.

Added source-specific error contexts and first-RTP/class-route logs. Class route receive errors now warn and break that bounded receive pass, matching existing Teacher publisher rather than killing the entire class. Paired route build succeeded; 189 Classroom tests pass again (2 ignored), focused Flutter tests 4 pass. However rebuilding/signing preceded a renewed reverse TCP EHOSTUNREACH in `target/student-broadcast-native-route-20261008`; `nc` still succeeds. Runtime outcome remains incomplete and original UDP failure is unresolved. No privacy grants changed by agent. Fresh Mac package paths remain `target/student-broadcast-build/Classroom Console.app` and its `Contents/Helpers/Classroom Agent.app`; next test must restart this same package after owner permission refresh without rebuilding. Flutter tracked lock restored after build.

## Final development package before next permission refresh
Checked existing Apple Development identity with the project signer: certificate trust passed, but Gatekeeper rejected signed Mac Student. No rejected package launched or deployed, no purchase/notarization/upload. Repaired Bash 3.2 empty-array expansion in Apple signing branches; no signing/trust policy weakened. Returned to the owner-approved development ad-hoc mode and rebuilt paired roles at the stable test path.

Final Flutter analyze: no issues. Class route handling retains source-specific failures and native relay is still unverified. Final package ZIP hashes match fresh Mac/Windows executables; codesign deep/strict integrity passes. Windows SHA256: df92cf740715636fc89d86557721367087acf3a60fa542939c2a6aad8252dfb0; Mac Student SHA256: c6d057c34332d570590c1a763e8ee19adcd779938e788bb7314e2cfaa17ca87a. Raw package check: `target/student-broadcast-final-package-checks.json`; build log `target/student-broadcast-paired-build-release.txt`. Next is manual Local Network refresh and same-bundle restart without rebuilding. Keep latest package unchanged during that test.

## Local Network prompt attempt with unchanged bundle

Owner clarified that Local Network has no manual add/remove control. Launched the existing `target/student-broadcast-build/Classroom Console.app` unchanged, through its Flutter parent, then initiated an authorized reverse TCP request to Windows Source `192.168.50.94`. No Allow dialog was observed. Student registration and thumbnail reception succeeded, but reverse TCP to Source port 57313 failed with EHOSTUNREACH in both Tokio and system fallback. This does not establish the underlying cause or a permission fix. The test stopped, quit its owned Teacher session and removed temporary Windows tasks/firewall rules. Evidence: `target/student-broadcast-native-permission-request-20261008/`; the narrow system-log query returned no matching records. Do not ask the owner to manually add an app to Local Network; future attempts must originate from the exact app and leave privacy decisions to the owner. No rebuild, signing, or privacy modification was performed in this attempt.

## Native functional validation after owner Local Network Allow

Owner enabled a previously listed Console entry, then observed and accepted a
Local Network dialog (Yes). Restarted the same paired-permissions-20261008 bundle,
without rebuilding/signing or modifying privacy grants. First allowed trial
`target/student-broadcast-native-allowed-20261008` still ended source-disconnected
while permission was changing. Second post-Yes trial completed with exit code 0:
`target/student-broadcast-native-after-yes-20261008`.

Passed: source .94 presents while .106 is absent; receiver late joins and reaches
receiving health; sustained 20-second wait; receiver process reconnects while source
remains live and receives again; diagnostic request queues until Stop; Stop returns
idle and produces ZIP with Teacher metadata/log plus both Student metadata/logs,
collected=2/missing=0; a new stream starts, receives and stops cleanly. Cleanup of
owned Windows tasks/rules/processes completed with no reported error. Installed
Students/Guard/profiles preserved. Source excluded from class recipient reports.

Evidence: 76 safe state samples, 46 receiving samples. Receiver report medians
~18.959 decode FPS / 18.802 render FPS; maximum reported missing packets 0; no
Teacher error states. Span ~28.775 seconds is across lifecycle stages, not a single
uninterrupted soak. Relay interval reports recipients=1, active_encoder_instances=0,
send_errors=0; no advisory keepalive error in this post-Yes run. Source logs show
1920x1200 capture bounded to 1728x1080 preserving aspect ratio and a single source
encoder. This is not full-width 1920x1080/30fps or a motion workload benchmark.
Source/receiver are two ARM Parallels guests running x64 binaries; Mac coordinates.
No physical 35–40-PC scaling/optical latency acceptance claimed.

ZIP: `target/diagnostics/Classroom-diagnostics-1791450284290-session-3164851532613725673.zip`.
Machine-readable summary and raw logs/state samples are in the post-Yes directory.
Previously reported macOS EHOSTUNREACH is absent in this unchanged-bundle trial;
this does not prove permanent ad-hoc identity continuity across another build.

Native permission checklist subsequently shows both Recording grants green and
Console/Agent LAN connections to .106:22 successful. A separate native idle
Restart button test found loss of custom Flutter CLI arguments: the replacement
GUI opened default 47800 while the restarted helper remained 48900. Source now
passes explicit Dart Agent.args to a bounded normal `/usr/bin/open -n ... --args`
launcher, with regression coverage. No new Mac build per owner's batching rule;
this fix is not in the current signed bundle and awaits native validation at the
next grouped build. Duplicate test sessions were stopped; one same-bundle GUI and
helper left idle at the explicit test endpoint. Do not rely on this bundle's
checklist Restart button to preserve custom dev IPC/port flags.

Deferred lab checks: selected-source broadcast to 35–40 physical x64 Ethernet
receivers, multi-recipient join/reconnect/load and slow/lossy receiver isolation,
repeatable foreground text/scroll/motion at fixed actual 1920x1080, optical latency,
long soak, source crash/recovery, sleep/DPI, original Defender quarantine case.

Restart source checks: Swift typecheck passed (registrar stub; existing activation deprecation), Flutter analyzer no issues, focused UI 11 passed. Evidence: `target/macos-permissions-restart-{swift-check,analyze,tests}.txt`. No paired build/deployment of this restart source fix.


## 2026-10-09 — resumed artifact verification: cooperative class Control

The paired-class-control-20261008 native run completed both source directions.
Evidence: `target/student-class-control-1080p-20261008/verified-summary.json`,
585 safe state samples, events, source/receiver logs and workload JSONL. The daemon
restart lost the original runner exit status; completion is verified from retained
artifacts, not an invented exit code.

Selecting A automatically opens Teacher Control; both Teacher and A retain input.
One Source publisher feeds compressed relay and Teacher preview; relay intervals
report zero encoder instances. Teacher decodes its interactive preview only.
Class source cursor remains in video. Ordinary Observe remains read only.
Closing Control stops the class session. Native mode acknowledgements and one
harmless Teacher Home-key event at each Source establish the input route; pixel
cursor appearance and optical latency still require visual/physical verification.

Each direction used 90 seconds of foreground workload at fixed 1920×1080:

| Source | Receiver | Median decode FPS | Median render FPS | Max reported missing packets |
| --- | --- | ---: | ---: | ---: |
| .94 | .106 | 27.880 | 26.817 | 0 |
| .106 | .94 | 26.891 | 26.533 | 44 |

All recorded workload samples were foreground 1920×1080 (114 / 113 samples,
including surrounding lifecycle time). Each workload restored its original mode:
1920×1200 and 1512×949. Receiver FPS medians describe health reports in the workload
stage, not optical latency or an exact frame counter for only the 90-second window.

Source process termination ended Teacher presentation in 1.368 / 1.301 seconds.
Temporary per-test-EXE network blocking ended it in 6.460 / 6.947 seconds. Restart
and reconnection produced receiving health in both directions; final state idle,
no error, no Observe view, both endpoints online. Receiver late join and process
restart passed. Diagnostic collection queued during presentation and completed
on Stop: collected=2, missing=0. ZIP integrity and its Teacher plus two Student
log/metadata entries were checked on resume.

ZIP: `target/diagnostics/Classroom-diagnostics-1791458860082-session-1952094455784981871.zip`.
Relay logged up to 5 send errors; this is not a zero-loss claim. Added bounded last
send-error text and timeout counter for subsequent diagnosis, with no queue,
quality or transport-policy change. No decoder redesign is justified by this run.

### Deferred to physical lab

- Windows x64 Teacher and 35–40 physical Windows x64 Students over wired Ethernet;
  ladder 1/5/10/20/40 with identical active 1080p workload and actual bitrate.
- Student-source Control + class relay scaling, Source upload for relay and preview,
  Teacher preview CPU/decoder load, optical latency and visible cursor correctness.
- Fixed-resolution 1080p long soak, real sleep/resume and 125%/150% DPI.
- Defender false-positive reproduction with detection name, affected file/hash and
  protection history; the current ARM guests cannot prove lab AV compatibility.

The present endpoints are ARM Parallels guests running x64 binaries on a shared
Mac/Wi-Fi environment. These results are functional verification, not Phase 1
physical Ethernet performance acceptance.


On 2026-10-09, fresh SSH audits found no Classroom-Broadcast test tasks, firewall
rules or owned runtime processes on either .94 or .106; Defender realtime was
still enabled. Evidence: `target/final-cleanup-audit-192.168.50.94.json` and `.106.json`.
Final grouped build passed and embedded package byte/hash checks matched fresh
roles (paired-class-control-20261009). Focused Rust relay checks5 passed. Native
final short lifecycle/Control regression is being run; results follow below.


## Final grouped native regression — 2026-10-09

`target/student-class-control-final-20261009` completed with retained runner exit0
(`target/student-class-control-final-run.txt`). Default cooperative Control,
late join, 20-second receiving wait, receiver restart, queued diagnostic ZIP2/2,
new stream, and Control-close ending the presentation passed. 89 safe state
samples verify acknowledged Control and final idle. Relay intervals show one
recipient, zero encoder instances and zero send/deadline errors in this short
static regression; that does not supersede losses in the prior motion workload.
No 90-second workload was repeated because media behavior was unchanged; the
final changes were Restart launch and relay error diagnostics.

Final Classroom test suite:195 passed/2 ignored. Prior protocol/control tests
1/14 passed, focused Flutter12 passed and analyzer clean; no claim that the full
Flutter suite passes (existing Observe layout failures remain documented).

A second SSH audit after the final regression again found zero owned test tasks,
firewall rules or runtime processes on both Windows; Defender realtime enabled.
