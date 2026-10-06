# Student popup, paired builds and stale Observe error — 06-10-2026

## Changes

- Student diagnostic notification:300×78,16px from monitor work-area right/bottom;
  titleless/borderless, Windows WS_POPUP and HWND_TOPMOST with NOACTIVATE. Mac uses
  floating native window. Connected auto-hide remains3s; Teacher toggle remains.
- PROJECT_NOTES.md and AGENTS.md require fresh Teacher+Student builds together.
  Mac build script builds Mac roles and Windows x64 Student, bundles both freshly
  packaged Student ZIPs with a common build label. Missing packages fail the build.
  Windows script builds Rust x64 MSVC roles before Flutter and packages Student;
  that Windows Teacher script path was not runtime-tested in this milestone.
- Observe GUI no longer latches a prior session's receiver error as a mode error.
  Nonzero receiver exits include the bounded last8KiB log's fatal error, when present.

## Investigation

Teacher logs observe-frame-1791249350093082 and1791249355282722 recorded
`No route to host (os error65)` connecting to Windows. Third session
observe-frame-1791249357395695 connected and decoded1728×1080 (~15–20FPS,
zero decode errors). This establishes transient network failure and later recovery;
privacy permission/startup as the initial cause is unproven. UI incorrectly retained
previous receiver error despite live frames; regression now covers error→new surface.

## Verification

Rust119passed/2ignored; Flutter28passed; analyze clean. Fresh Mac Teacher,Mac
Student and Windows x64 Student build verified. Teacher bundled ZIPs byte-match
fresh updater packages. Windows Student update uses Classroom's existing updater,
not SSH deployment, and confirms the new build after restart.

A short interactive Windows native probe measures300×78,16px margins,TOPMOST;
first probe caught implicit WS_CAPTION despite title:false. WS_POPUP correction
applied before final build. Native probe is diagnostic UI validation, not a media
benchmark. Existing failure/recovery behavior tested through Flutter regression;
no claim that underlying transient route failure is eliminated.

No physical Windows wired-LAN/40-PC acceptance. macOS popup compiled, not remotely
runtime-tested. Preserve settings/profiles/logs and published v1 assets.

Final probe:style94000000 (no WS_CAPTION/WS_SYSMENU/min/max bits),exstyle8
(WS_EX_TOPMOST);300×78,16px right/bottom. Visible initially and absent by later
samples after connected auto-hide. Native measurement task removed afterwards.
Final build label paired-20261006-082848. Windows .94 updater confirms complete
job e76981f9a5cdb8bbc3fac55ad2db70c8. Final running Teacher:
`target/popup-final-build/Classroom Console.app`; fresh Mac Student:
`target/popup-final-build/Classroom Student.app`. Both Student ZIPs bundled under
Teacher Contents/Resources/student-packages and byte-match target/updates files.
