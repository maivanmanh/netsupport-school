# Student context menu and Screenshot — 2026-10-09

- Added matching icons to Collect files, Observe screen, Control & present
  Student, and the new Screenshot item in each online student's context menu.
- Screenshot opens/focuses the existing independent Observe window and requests
  one automatic save. It uses the Observe image rather than the roster thumbnail.
  No keyboard/mouse Control request is made by Screenshot.
- The window waits for a fresh native frame (up to 15 seconds), acknowledges the
  request once, then saves through the existing platform Desktop writer. PNG
  names include the student name and timestamp; existing files are not replaced.
  macOS may ask the user to select Desktop once and retains its bookmark.
- An existing class-source Control window can be focused for Screenshot without
  starting another session or switching its mode. Other students remain subject
  to the existing broadcast/Observe admission rules.
- Flutter analyze passed; Rust `cargo check -p classroom-app --bin Classroom`
  passed with existing warnings. Formatting and diff whitespace checks passed.
  No test suite was added or run in this turn.

Runtime screenshot saving and the Windows Teacher path remain unverified.

Paired release build `paired-screenshot-20261009` completed for Mac Teacher,
Mac Student and Windows x64 Student/Guard. Initial Windows build selected the
Homebrew compiler; rerun with explicit Rustup RUSTC/RUSTDOC succeeded. Flutter
resolved SDK-pinned dependencies locally; resolved lock is retained at
`target/screenshot-pubspec-resolved.lock`, with tracked dependency lock unchanged.
Both app signatures were verified. Replaced Teacher at its existing
`target/present-recovery-build/Classroom Console.app` path, reopened it and
confirmed IPC response and Mac Student online. Old paired bundles were removed
after the new Teacher responded; staging folder was removed. Remote Students
were not redeployed. No screenshot was triggered automatically.
