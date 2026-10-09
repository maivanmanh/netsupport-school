# macOS permission setup

Owner-authorized scope (2026-10-08): add a first-launch permission checklist to
Teacher, with reasons, status indicators, request/settings buttons and Continue.
Windows does not show this macOS-only dialog. The toolbar can reopen it.

## What is checked

- Console Screen Recording: `CGPreflightScreenCaptureAccess` in Flutter's native
  process. Request uses `CGRequestScreenCaptureAccess`, then opens Settings.
- Agent Screen Recording: a short-lived process at the exact bundled helper
  executable checks/requests its own access. It does not start a classroom,
  capture frames, load a profile, or overwrite runtime logs. Unknown/old helper
  support stays "Not verified", never green.
- Local Network: Apple provides no general permission-state API. User initiates a
  bounded TCP probe to a known private IPv4 classroom endpoint. Console uses
  Network.framework; the bundled Agent uses its own system TCP connection. Both
  must connect before the indicator says "Connected (last test)". This verifies
  that endpoint, not UDP or every classroom service. Offline service, wrong port,
  firewall, timeout and EHOSTUNREACH remain unverified, not a claimed denial.
- Teacher does not request Full Disk Access or require Accessibility just to
  broadcast or control a Windows Student. Mac Student remote input needs that
  Student's own Accessibility grant; this Teacher dialog explains the distinction.
  Files use existing explicit system pickers.

OS status checks run every 2 seconds while the dialog is open and refresh on
resume. After the user starts a LAN probe, it repeats every 10 seconds until
closing the dialog or editing the endpoint. Each connection/helper process is
bounded. Checks stop on dialog disposal. Test port defaults to 22, explicitly
labelled as SSH-only; it is editable and not a classroom media default.

Continue only closes onboarding, without granting any OS access. No permissions
are silently enabled, no TCC database or network preferences are edited, and no
old privacy entries are deleted/reset. Finder reveal and full exact paths help
manual setup. Local Network has no add-app control: a real access attempt causes
first-use registration/prompt; a recorded decision need not prompt again.

## Restart and builds

After changing Screen Recording, restart Teacher. The explicit button is enabled
only for an idle, connected Teacher; it invokes the existing Rust restart lifecycle
and starts the exact bundled Console executable with explicit Dart launch
arguments before closing this GUI. A bounded immediate-exit check retains the old
window on launch failure. LaunchServices/open dropped custom arguments in native
tests, so this path uses the existing dependent-window Process launch mechanism. This is not an automatic promise of permission continuity across ad-hoc
updates. Settings/profiles remain intact. If a launch fails, report the error.

Batch source changes and checks before rebuilding/signing macOS apps. Every
requested build still produces fresh paired roles and embedded Student packages.
Never request a paid certificate as a prerequisite for this development setup.

## Verification

See `checkpoints/diagnostics/2026-10-08-macos-permissions.md`. Unit/widget checks
are distinct from real macOS prompts, privacy cache behavior and media acceptance.

Apple reference:
https://developer.apple.com/documentation/technotes/tn3179-understanding-local-network-privacy
