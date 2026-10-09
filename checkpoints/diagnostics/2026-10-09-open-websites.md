# Open website and shorter screenshot filenames — 2026-10-09

## Owner requests

- Screenshot filename must be `Student-name_yyyy-MM-dd_HH-mm-ss-SSS.png`,
  without `Classroom_` or random suffix. macOS and Windows writers now use this
  format; only a genuine collision gets an incremental suffix, never overwrite.
- Open a typed URL on the whole class or checkbox-selected computers, in each
  Student's default browser, with durable clickable recent URLs on Teacher.

## Implementation

- Added labeled globe **Open website** toolbar button. Uses the same recipient
  selection rules as other classroom tools; no checkbox selection mode means
  the online class. With selection mode, only selected online computers receive it.
- Dialog shows the audience, URL field, recent addresses with one-click opening
  and removal, and live per-job completion/failure counts. Missing URL scheme is
  normalized to HTTPS; only HTTP/HTTPS URLs with a host and no embedded login are
  accepted. Recents are deduplicated, newest first, capped at 20.
- Before dispatch, await existing flushed `savePreferences` persistence under
  `recent_websites` in Teacher's application-support `console-settings.json`.
  App startup reloads this file. This is independent of transient sessions,
  deploy folders and Student installation directories. Persistence errors are
  surfaced and do not silently dispatch an unsaved address.
- Teacher queues `website` jobs through the existing authenticated tools channel,
  with a new capability check. Students execute on a background worker, reporting
  each OS handoff result. Windows uses ShellExecuteW with the `open` verb;
  macOS uses `/usr/bin/open` with one URL argument. No browser is hard-coded;
  URL query characters and percent escapes are not passed through a shell.
- Build script now selects Rustup's rustc/rustdoc explicitly to avoid the
  previously observed Homebrew/Rustup compiler mismatch during Windows builds.

## Verification

Rust checks passed for Mac Teacher/Student and Windows x64 Student; Flutter
analyze passed, formatting and diff whitespace checks passed. No tests added or
run. Browser opening, recent URL use after Teacher restart, and Windows Teacher
UI remain manual runtime verification items. Owner confirmed a saved Mac Student
PNG from the previous direct Screenshot implementation by supplying its filename.

Physical 35–40-machine simultaneous browser launch remains deferred to the lab.

## Paired deployment

Build `paired-websites-20261009` completed; both embedded Student ZIPs match the
fresh packages. Teacher restarted at its existing path. Both Students updated
through Classroom, waiting for the old parent to exit, replacing in place and
confirming the new version. An observed LAN timeout briefly disconnected both;
Teacher logs show both re-registering afterwards. This is not a performance test.

Mac Student's new ad-hoc build is explicitly denied TCC capture in its log:
`The user declined TCCs for application, window, display capture`.
Screen Recording must be granted manually for
`/Users/lolem/Classroom/Classroom Student.app`, followed by Student restart.
This blocks Mac screenshots/preview, not opening a website. No privacy settings
were changed automatically. Browser/history runtime verification remains pending.
Resolved Flutter dependency lock retained locally at
`target/websites-pubspec-resolved.lock`; tracked pubspec lock unchanged.
