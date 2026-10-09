# VS Code policy confirmation — 2026-10-09

## Report and evidence

Owner reports the policy badge only updates after reopening the window, and
Windows `.155` can still use AI Chat despite the red “Blocked now” badge.
Read-only SSH inspection found VS Code 1.141.0, a machine AllowedExtensions
allowlist containing ritwickdey.liveserver, and no chat.disableAIFeatures key or
deny ACL in the interactive account's Code/User/settings.json. Student and Code
processes both run as lolem. Hub retained two policy jobs marked complete.
This confirms a mismatch between reported completion and the current file state;
it does not establish which process changed the file or when.

## Changes

- Keep a live Agent listener for the entire dialog lifetime. Scope counts to
  selected computers and require all selected computers for the active badge.
- Wait for terminal Student job responses instead of accepting the dispatch ACK
  as completion. Surface errors/timeouts; retain the dialog after Apply.
- Editing remains possible after Apply. Show “Allowlist applied”, with an
  explicit built-in AI limitation, rather than claiming verified AI blocking.
- Read back the AI setting after writing; log its exact path and result.
- Preserve an unreadable/non-object/JSONC settings file with an explicit error
  instead of silently replacing all user settings with an empty object.
- Propagate failed Code termination/relaunch to the Teacher instead of claiming
  successful immediate application.

## Limits and next step

AllowedExtensions restricts extensions. chat.disableAIFeatures is a user/workspace
setting, not a machine policy. Microsoft's request for a machine policy is closed
as not planned: https://github.com/microsoft/vscode/issues/268012 . The existing
file ACL layer is best effort and is not proof that all AI chat routes are blocked.
Network policy has not been changed. Alternative user-data/profile/workspace
settings, replacement of the file, and other AI clients remain possible.

Mac cargo check and Flutter analyze passed; no automated tests run. The paired
build completed and Teacher/Windows were deployed; Windows reconnected and
confirmed paired-vscode-confirmation-20261009. Mac Student was not replaced.
The embedded Student ZIPs match the update outputs. Owner should Apply
with the window open and retry Chat on `.155`. Inspect the new path/readback log
if Chat remains usable. Do not report full AI enforcement or 35–40 PC acceptance.
