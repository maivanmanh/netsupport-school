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
