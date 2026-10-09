# macOS permission onboarding — 2026-10-08

## Changes

Owner asked for in-app checklist, reasons, live indicators and buttons instead of
repeated manual path selection. Added macOS-only startup dialog and toolbar entry,
separate Console/Agent Screen Recording checks and requests, full bundle paths and
Finder reveal, bounded private-LAN TCP probes from both process identities, manual
Settings links, Continue, and idle-only explicit Teacher/Console restart. Existing
screen-permission banner now opens this checklist. Windows UI unaffected.

Permissions remain owner controlled; no grant/reset/old-item deletion automation.
Accessibility and Full Disk Access are explained as unnecessary for Teacher's
current screen-broadcast/Windows-control workflows. Mac Student needs its own
grant for remote input. Local Network green means successful connection to the
selected endpoint, not a readout of system policy or evidence for UDP media.

## Checks before bundled build

- Rust `cargo check -p classroom-app --bin Classroom`: passed with warnings.
- Swift full window source typechecked against FlutterMacOS; generated plugin
  registrar was stubbed for this source-only check. Existing activation deprecation
  warning. Bundled build must verify real plugin linkage.
- Flutter analyzer: no issues.
- Focused Flutter: 11 passed (permission dialog 4, existing Console 3,
  diagnostics 3, health 1). Console tests now dismiss startup onboarding before
  exercising their original interactions; permission banner test explicitly
  requests settings through the new dialog.
- Broad suite initially 30 passed / 9 failed. Three Console failures caused by
  new modal expectations were fixed. The 6 Observe failures reproduced in a
  standalone Observe run (3 passed / 6 failed); its test/production files are
  unchanged from HEAD. They report existing narrow-width RenderFlex overflow.
  Do not describe the full suite as green.
- Evidence: `target/macos-permissions-{cargo-check,swift-typecheck,
  flutter-analyze,focused-tests,unchanged-observe-tests}.txt`.

## Bundle/runtime status

Paired ad-hoc build in progress, label `paired-permissions-20261008`, output
`target/student-broadcast-build/`. Exact old staging Agent PID33068 and output
Console PID37368 were stopped before Flutter bundle replacement. No installed
Student, profile or log was removed. Build log:
`target/macos-permissions-paired-build.txt`.

Native prompts, status transitions after actual grants, full GUI restart and
private-LAN probes remain to verify on the built bundle. Current Mac EHOSTUNREACH
has not been proven fixed. Previous partial Student relay got 20 decoded/19
rendered frames before advisory route keepalive failure; current source makes
that advisory failure nonfatal and has loopback coverage, but native relay
lifecycle/reconnect/ZIP remains incomplete. 35–40 physical x64 wired acceptance
is deferred.

## Paired build and native UI evidence

Paired build completed successfully with real Flutter/plugin linkage and strict
codesign verification. Embedded Mac/Windows Student executable hashes and labels
match the fresh binaries; Windows ZIP also contains the fresh Guard. Evidence:
`target/macos-permissions-package-checks.json` and paired build log. SDK-generated
lock changes restored to HEAD; no dependency change intended.

Actual Console startup showed the checklist with separate Not granted indicators
for Console and Agent and Not verified for LAN. Continue was used, then banner
Open Settings reopened the checklist. Request / Open Settings navigated to the
Screen & System Audio Recording page. Settings shows Classroom entries enabled,
but process API still reports false after this ad-hoc build; no privacy grant was
modified by the agent. Owner was asked to refresh manually using exact paths and
restart. Native transition-to-green/full restart is pending that owner action.

LAN probe to `192.168.50.106:22` ran from the actual dialog. Console returned
unverified timeout and Agent returned unverified `No route to host (os error 65)`;
UI showed both details and retained the unverified indicator. No Allow dialog was
observed during this attempt. This does not resolve the macOS connection failure.
UI automatically repeats the probe while this dialog remains open; Continue
closes the dialog and stops the timer.

Rust suite on the final source: 194 passed, 2 ignored. Focused Flutter 11 passed;
full suite is not clean because of the six unchanged Observe layout failures.
Do not rebuild/sign before the owner grant/restart check. No push performed.

The optional Windows-native Teacher branch in `scripts/test-student-broadcast.py` is staged for the next diagnostic comparison; Python syntax passes, but that branch has not been exercised. It requires fresh matching Windows Teacher/Student binaries and its specified Student ZIP. Do not claim a Windows Teacher or end-to-end relay pass from it.

## Owner Recording grant follow-up

Owner confirmed Recording grant. Native checklist now shows Granted for both
Console and Agent. Initiated the actual Request / Test LAN access button to
192.168.50.106:22 without rebuilding. Console reports unverified timeout, Agent
reports unverified EHOSTUNREACH; Terminal nc succeeds to the same endpoint.
No Local Network Allow dialog observed. Navigated the Settings pane via its Local
Network row (the URL opened Privacy & Security). Current AX state shows all three
Classroom Console entries off, plus both Classroom Student entries off. No helper
entry appears. Owner informed to enable Console manually; no toggles/reset/grants
changed by automation. This current off-state differs from earlier on-state and
must not be interpreted as proof that the earlier signing/cache problem is fixed.
Next: owner enables Console, restart the same exact Teacher bundle, rerun bounded
LAN probe and verify both identities. No further build/sign required.

## Post-Yes result and restart regression

Owner enabled an existing listed Console entry and later clicked Yes on an actual
Local Network dialog. Without rebuilding, native checklist shows Console/Agent
Recording Granted and both LAN TCP probes to .106:22 connected (green). Native
Student relay lifecycle/queued diagnostics test passed; see the broadcast report
and `target/student-broadcast-native-after-yes-20261008/summary.json`.

Native idle Restart test exposed custom-argument loss: old helper restarted at
48900 (Unix exec can preserve PID), new GUI launched without custom flags and
spawned a second helper at 47800. Do not call native custom-argument restart passed.
Source changed to pass Agent.args explicitly and use bounded normal macOS open
launcher rather than deriving arguments from native CommandLine. Regression test
checks actual transmitted IPC/port arguments. Source-only checks are separate from
signed bundle: no rebuild per batching preference. Next grouped build must run
native restart and verify exactly one helper, same IPC path/ports, preserved grants.
Duplicate sessions stopped; final GUI49239/helper49249 on same signed test bundle
are idle at 48900. Native Record/Network grant checks remain passed for this bundle.

Restart source checks: Swift typecheck passed (registrar stub; existing activation deprecation), Flutter analyzer no issues, focused UI 11 passed. Evidence: `target/macos-permissions-restart-{swift-check,analyze,tests}.txt`. No paired build/deployment of this restart source fix.


## 2026-10-09 — direct executable Restart verified

The grouped paired-class-control-20261009 build includes the final Restart fix.
The earlier open/LaunchServices argument fix still failed native testing, creating
an argument-less GUI and default47800 helper. Direct Process launch of the exact
bundled Console executable now preserves explicit Dart arguments. Native checklist
Restart replaced GUI9235 with9545; helper9240 exec-restarted/reconnected on48900.
Exactly one GUI and one helper remained, retaining bind192.168.50.206, control48900,
custom IPC and log paths. No duplicate default helper appeared. Evidence:
`target/final-restart-before.txt`, `target/final-restart-after.txt` and runtime log.

Both Console and Agent LAN probes to192.168.50.94:22 succeeded, shown as Connected
(last test). This is TCP reachability at that endpoint, not media/UDP acceptance.
Initial Screen Recording status after signing was Not granted for both; owner
subsequently reported granting both. Restart after that grant and actual Teacher
capture are the remaining native confirmation steps for this build.

Paired package byte/hash checks passed for embedded Mac Student, Windows x64
Student and Guard; both manifests label paired-class-control-20261009. Evidence:
`target/grouped-class-control-final-package-checks.json`. Five focused Rust relay
checks passed after adding bounded send-error/timeout diagnostics.


After owner granted both Recording permissions, closed test Teacher then reopened
the exact final bundle with the same flags. Native checklist shows both Granted.
Actual Teacher share_class capture/encode ran11.88 seconds and returned idle on
Stop, no error, one encoder, about29.87 encoded FPS. No Students were connected:
this confirms usable capture after grants, not receiver or network performance.
Evidence: `target/final-recording-capture-check.json` and runtime media-complete log.
Final bundle remains unchanged after these grants.
