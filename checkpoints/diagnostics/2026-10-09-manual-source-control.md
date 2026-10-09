# Manual Student-source faults and corrective build

## Findings from the owner's two sessions

Evidence: `target/manual-source-review-20261009/` contains original Teacher,
Student logs, Mac crash report, FPS summaries and removal audits.

Mac Student crashed at 11:37:20.3377 on 2026-10-09 (PID15704, SIGTRAP).
The faulting thread calls Enigo's Unicode keyboard-layout lookup through
HIToolbox, which asserts its required dispatch queue. This is a confirmed input
thread fault, not evidence of a Screen Recording or network permission failure.
Teacher sees all three Mac control channels EOF together. Mac becomes offline;
the old toolbar excluded offline selections, explaining its disappearance.

| Source | Capture median FPS | Encode median FPS | Class receiver decode median FPS | Class render median FPS |
| --- | ---: | ---: | ---: | ---: |
| Windows, 1510×948 | 13.436 | 18.777 | 17.980 (Mac) | 16.969 |
| Mac, 1470×956 | 24.298 | 21.613 | 20.953 (Windows) | 18.895 |

Windows source has 110 interval samples; Mac has 26 before the crash. Static
desktop capture, startup and stopping affect these rates; this was not a fixed
1080p workload. Windows target bitrate falls 6→4.8→3.84→3.072→2.4576 Mbps with
logged local encoder pressure (29/110 samples); encode interval p95 median
17.715 ms, maximum117.021 ms. Mac encode p95 median11.685 ms, no reported local
encode/CPU/send pressure; publisher pacing overshoot p95 median47.933 ms.
The old Mac launch job lacked Interactive/LegacyTimers metadata. Correcting it
is a measurable hypothesis, not proof of an FPS improvement.

Source send errors are zero, but Teacher relay records three deadline/send
errors in the Windows-source session. Receiver missing-sequence counters peak
at six (Mac receiving Windows) and22 (Windows receiving Mac). These counters
are cumulative, not additive interval losses or proof of a particular NIC fault.
No optical latency, physical Ethernet ladder or 35–40-PC acceptance was measured.

## Changes

- Resolve Mac Unicode keycodes on the main dispatch queue with one pending
  lookup per input session and a250 ms deadline. Inject cached raw keycodes on
  the input owner; releases never perform layout lookup. Expired lookup does not
  inject delayed input. Named keys need no asynchronous lookup.
- Right click an online Student → **Control & present Student**, using the same
  validated source-switch command. Checkbox selection is unnecessary. Offline
  selected Student retains a disabled button with an explanatory tooltip.
- Native Teacher foreground Control event tap forwards app and system shortcuts,
  including Cmd+Tab/Cmd+Space. Releases on blur/tap loss; bounded event queue256.
  Teacher Accessibility is required. Standalone Teacher/helper remove App Sandbox
  to support assistive APIs; legacy profile/settings and onboarding UI state remain.
- Mac launch jobs become Interactive with LegacyTimers and restart on crash,
  without restarting a normally exited Student.
- Verified Windows installed Guard/Student targeted172.16.48.237. Stop/remove its
  exact service, task, executable, launch args and owned firewall/startup rules;
  retain settings, profiles, logs and migration record. Remove known stale dev
  autostart too. Audited replacement executable hash precedes removal. Shared
  helper is used by source-switch prepare, responsive-control deploy and native
  Student-broadcast test deployment. It does not change ExecutionPolicy or Defender.

## Verified and pending

- Rust203 passed,2 ignored; focused Flutter11 passed; Flutter analyzer no issues.
- Python deployment helpers compile; diff whitespace check clean.
- Paired native build `paired-source-control-fix-20261009` succeeds, including
  Swift event tap, Mac Student and fresh Windows x64 Student/Guard package.
  Both embedded ZIP executable hashes verify and version matches; Teacher deep
  signature verification succeeds. No release upload/push.
- Windows removal native audit: old installed executables absent, old Guard
  service absent, current isolated Student retained, Defender realtime enabled.
- New Mac Student replaces only the stopped verified test bundle at
  `/Users/lolem/Classroom-source-switch/Classroom Student.app`; external data and
  logs retained. GUI-session read-only probe returns `screen:false` after signing.
  New Student has not been started for functional testing.

Per owner instructions, native testing pauses for manual privacy grants. Teacher
new bundle is at `/Users/mvmanh/Development/netsupport-school/target/source-control-fix-build/Classroom Console.app`.
Grant its Recording and Accessibility, bundled Agent Recording, and Mac Student
Recording/Accessibility. Restart Teacher and Student after granting. Do not use
computer automation to change grants or reset TCC. The previous Teacher remains
untouched until the new build is ready to launch.

Next: run actual Mac letters, Cmd+Tab/Cmd+Space with Teacher remaining focused and
Student changing applications/Spotlight; repeated source handoffs and several
minutes of controlled motion, compare pacing/FPS and check for crash. Until then
the input correction is build/test verified, not a native runtime pass. Defer
fixed1080p Ethernet1/5/10/20/35–40 ladder, optical latency and physical Windows
Defender/DPI/sleep acceptance to the lab. No media buffers were enlarged.

## Native verification after manual grants

Owner grants all requested permissions; restart exact new Teacher and GUI-session
Mac Student. Read-only GUI probe now reports screen:true (the status file appends
old false and new true records; newest record is authoritative). No signing,
rebuild or privacy automation during this follow-up.

Windows initially offline: TCP22 accepts but SSH banner times out; the Parallels
read-only guest command also stalls. Owner says Windows remains responsive, then
sees the Mac presentation on Windows. SSH recovers and native inventory confirms
only the retained new Student PID1748 at its isolated Source-Switch path, targeting
192.168.100.171/48900; sshd running. No forced VM restart and no new Windows deploy.
The exact cause of this transient access failure remains unproven.

Actual Teacher right-click menu **Control & present Student** opens Mac source
without checkbox selection. IPC confirms class_control:true, mode:control, live
source. Mac PID16163 stays alive through the initial several-minute source session
and input, unlike the previous SIGTRAP. Direct native input Cmd+Space does not
crash or report a control error. Owner separately tests the actual Control window
and explicitly confirms Cmd+Tab and Cmd+Space affect only Mac Student, not Teacher.
This is owner-observed native UI validation; it is not an automated optical test.

One bounded Teacher→Windows→Mac cycle then passes. Ready times3.983/3.541/3.009s
are control/receiver-health readiness, not first-frame or optical latency. Every
non-source online recipient reports positive render FPS; both Student sources
report cooperative Control. Eight seconds of additional state checks per source
show no disconnect/error. Final Stop reaches idle; all apps remain available.
Evidence: final-handoff-run.txt, final-handoff-events.json, final-handoff-states.json,
state-after-owner-windows.json, windows-after-grant-inventory.txt and Mac full log
in the existing evidence folder.

Windows current Mac-source snapshot decode20.760/render17.794 FPS is a single
interval on uncontrolled content, not an improvement benchmark. Cumulative missing
packet counter243 includes recovery/history; do not treat it as this cycle's loss.
No matched foreground motion A/B, full soak or physical35–40-PC test is claimed.
Next: owner may manually exercise text input and class operation with these unchanged
bundles; lab acceptance still requires fixed1080p, physical Windows x64 Ethernet
ladder and optical latency.
