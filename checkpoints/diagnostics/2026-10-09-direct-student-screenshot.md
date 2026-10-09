# Direct Student Screenshot — owner correction, 2026-10-09

The previous Observe-based context Screenshot did not satisfy the owner's
requirement. Its IPC/UI automatic Observe-save bridge has been removed. The
existing manual screenshot button inside Observe remains independent.

## Implemented flow

1. Teacher context Screenshot sends `student_screenshot` through authenticated
   local IPC. The tools hub validates the online Student and its new advertised
   `screenshot=1` capability and queues a token-bound request.
2. Student handles `screenshot` on the existing bounded file-job worker (one
   active worker, four waiting jobs). A new one-shot native capture is started
   only after the request. No Observe publisher or Control lease is involved.
3. Student encodes BGRA pixels as PNG, maximum 1920×1080 without upscaling, and
   streams the binary using the existing separately paced file-tree transfer
   with checksum verification. Temporary Student files are removed on completion
   or error. Capture has a 10-second deadline and supports cancellation.
4. Teacher receives a private staged file, waits for completion, then writes
   the exact PNG bytes to Desktop through its existing native platform channel.
   macOS now accepts PNG bytes on `saveScreenshot`, with no UI capture or
   re-encoding. Desktop permission may require a one-time folder selection.
   UI has a 60-second deadline, duplicate-request suppression and failure feedback.

## Verification and deployment

- Rust Teacher/Student cargo check passed; Flutter analyze passed. No tests added
  or run. Actual menu-to-Desktop capture still requires manual verification.
- Paired Mac Teacher, Mac Student and Windows x64 Student/Guard build succeeded,
  label `paired-direct-screenshot-20261009`. Teacher was replaced at the same
  path and restarted, removing its obsolete paired bundles after IPC readiness.
- Mac Student first updated through Classroom, stopping the old process and
  confirming the new build. Removed that confirmed old backup and swap helper.
- Owner subsequently deleted the Mac and Windows home deployment directories.
  Rebootstrapped via SSH into stable paths:
  `/Users/lolem/Classroom/Classroom Student.app` and
  `C:\Users\lolem\Classroom\ClassroomStudent.exe`.
  Mac global profile/device ID were preserved; its saved ports now match489xx.
  Mac was opened through LaunchServices and fresh thumbnails were received.
  Windows fresh binary hash matched; the interactive task is `Classroom Student`.
- Final Teacher IPC confirmed both `192.168.100.254` and `192.168.100.155` online,
  advertising `screenshot=1` and `paired-direct-screenshot-20261009`. Windows has
  exactly one new Student process at the canonical path, and its obsolete task
  was unregistered. macOS capture permission already allowed fresh thumbnails.
- No macOS privacy grants were changed by automation. No Observe/Control window
  was opened for screenshot verification.
- Flutter's build-time SDK dependency resolution is retained locally at
  `target/direct-screenshot-pubspec-resolved.lock`; tracked pubspec lock unchanged.

Next: manually click Screenshot on each Student, confirm Desktop PNG and that
no Observe/Control window opens. Physical classroom performance remains deferred.
