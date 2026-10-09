# Control & present Student: stale Observe blocker

## Confirmed cause

The owner reported both the toolbar button and Student context menu appearing
to do nothing. Teacher received `switch_source` each time, but rejected it with
`Close Observe/Control windows before presenting a Student`. The retained
Observe state was already `ended`, with `Input lease expired; Student input
released`. The admission guard checked `observe.is_some()` and treated this
completed diagnostic state as an active session.

The Flutter command handler stored the rejection in `agent.error`, then the
next authoritative state publication replaced it with the backend's empty
session error. This explains why the failure was not visibly retained.

## Changes

- Retire completed/error Observe views on an explicit Student presentation
  request. Preserve the guard for starting/live views and owned live windows.
- Both presentation UI entry points now use checked acknowledgement and show
  a SnackBar on rejection; subsequent state publications cannot erase it.
- Log presentation command rejection reasons in Teacher diagnostics.
- Include the previously prepared Control recovery and Windows shifted-key
  fixes in the same grouped paired build.

## Verification

- Rust Observe coordinator regressions: 4 passed.
- Full Rust Classroom suite after the fix: 205 passed, 2 ignored.
- Flutter source selection/error feedback and recovery regressions: 7 passed.
- Flutter analyze: no issues. Whitespace check: clean.
- Paired build: `paired-present-recovery-20261009`, output
  `target/present-recovery-build/Classroom Console.app`. Mac and Windows Student
  packages rebuilt and embedded; executable package hashes and strict deep
  codesign verification passed. Existing Student deployments were retained
  because this correction is in Teacher; no Student rebuild was deployed.
- Corrected Teacher launched at `192.168.100.171:48900`.
- Windows `.155` regression: deliberately create an unrenewed standalone
  Observe session. It ended with a receiver `No route to host` error, then
  `switch_source` succeeded instead of being blocked by that retained error.
  Class Control was acknowledged, and Mac `.254` reported receiving/rendering
  the class stream. Stop returned Teacher to idle with both Students online.
- Mac `.254` request reached its publishing/Control path, but OS input
  initialization failed: `the application does not have the permission to
  simulate input`. This is an Accessibility grant required on the Mac Student,
  not a passed Mac Control/presentation test. Screen Recording thumbnails work.

## Pending manual grant and next step

On Mac Student `.254`, grant Accessibility to:

`/Users/lolem/Classroom-control-recovery-20261009/Classroom Student.app`

Restart that Student after the grant and rerun Mac source → Windows recipient.
Do not modify TCC through automation. Keep Teacher and both Students open and
idle for the owner. No 35–40 physical PC or optical latency acceptance is claimed.

Evidence: `target/control-recovery-20261009/present-*`,
`teacher-present-fixed.log`, and current Mac Student `student.log`.
