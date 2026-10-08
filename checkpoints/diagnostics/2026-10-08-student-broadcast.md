# Student broadcast, receiver health and diagnostic ZIP — 2026-10-08

## Scope and architecture
Owner authorizes receiver health, manual diagnostic ZIP collection and presenting one selected Student to the class. Owner reports the existing Windows Teacher broadcast worked well for 35 real Windows Students at 1920×1080; that result is the owner's evidence and is not a newly measured result here.

Student publishing reuses `observation::Publisher` and the existing DXGI/H.264 media publisher. The selected Student captures, encodes and packetizes once. Teacher relays compressed RTP, rewriting only session SSRC; Teacher does not decode or re-encode. Downstream uses existing UDP pacing, repair cache, NACK and keyframe requests. Source is excluded from class presentation; current and future recipients can join. No video history or unbounded queue is added.

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
