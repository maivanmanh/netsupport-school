# Native Remote Terminal — implementation and runtime checkpoint

## Current state (2026-10-10)

Owner authorized the Windows/macOS native terminal feature and Copilot dialog
changes. Implementation is present but **not accepted as fully verified**.
Latest paired build label: `paired-native-terminal-interrupt-20261010`.
Teacher and both Student packages were rebuilt together; Teacher embeds both
fresh Student ZIPs. Teacher and Windows `.94` were deployed and Windows confirmed
the new build through the updater. Mac `.2` was bootstrapped by SSH at owner request.

Current test endpoints:
- Teacher: `192.168.50.206`, control 48900, tools 48902.
- Windows Student: `192.168.50.94` (ARM64 Parallels, x64 binary).
- Mac Student: `192.168.50.2`, `/Users/mvmanh/Classroom/Classroom Student.app`.
Both Students were observed online through authenticated Teacher IPC.

## Implemented

See `docs/23_NATIVE_REMOTE_TERMINAL.md` for the locked design. Native PTY,
certificate-pinned TLS on the existing tools listener, bounded streaming,
sequence acknowledgements/replay, 15-second reconnect lifetime, ordinary-user
Windows worker, administrator-managed pairing policy, audit metadata and native
Flutter xterm window are implemented. Teacher pairing identity is retained across
updates. No raw terminal input/output is written to audit logs.

Copilot dialog requested label/count, removal of bottom Close, and dirty-state
Apply enablement are implemented. Existing allowlist/policy selections preserved.

## Evidence

- Mac Rust checks, Windows x64 GNU cross-check, native paired build and focused
  Flutter analyze succeeded.
- Existing Guard tests: 15 passed.
- Flutter existing suite: 45 passed, 9 failed. Clean HEAD `380f46e` reproduced the
  same 9 native-channel/platform failures; not reported as a passing suite.
- Real Windows CMD through ConPTY: native environment expansion, 110-column
  resize, medium integrity token, UI socket reconnection, and output arriving
  before newline/process termination passed.
- Creating a worker with an Explorer token requesting only 0xB rights failed;
  adding TOKEN_ADJUST_DEFAULT/SESSIONID (0x18B) succeeded, without elevation.
- Native Windows PathBuf construction fixed mixed slash CMD executable parsing.
- ConPTY's initial cursor-position query requires the terminal emulator response;
  protocol-generated replies are allowed before the first connected status.
- **Ctrl+C still fails to interrupt `ping -t` in the automated runtime case.**
  Raw ETX, Win32 encoded input, and the latest scoped console-signal attempt have
  not passed. Native Remote Terminal is incomplete until this is resolved.
- TLS network reconnect fixture remains pending (Ctrl+C failure stopped the run).
- Mac `.2` Student is online, but `--permission-status` reports `screen:false`.
  Owner must grant privacy permissions manually; no TCC edits were made.
- Old Mac native Student bundle and old Student ZIP were removed. Old logs were
  moved under `/Users/mvmanh/Classroom/logs/legacy-two-mac`. Nested legacy binary logs were also preserved; the old deployment folder was
  subsequently removed after verifying no unrelated files remained.

## New owner request: Unix-like compatibility for Windows CMD

Not implemented yet. Requirements are the latest user message; keep this distinct
from the deployed native terminal feature. Must retain persistent CMD, native
command/executable/alias precedence, safe command boundaries and file-operation
scope; no string substitution over raw PTY input. Only Windows CMD may opt in,
default enabled, with persistent configuration and per-mapping unit/integration
coverage. Native command precedence conflicts with replacing existing builtins
such as `date` and `rmdir`; precedence must remain authoritative and unsupported
forms must not be silently approximated. Do not claim all mappings work before
runtime evidence, especially `rm -rf`, `cp -r`, `tail -f`, quoting and pipelines.

## Exact next steps

1. Owner grants Mac Student Screen Recording/Accessibility at the full path above;
   restart that exact Student afterward. Teacher recording is also currently
   missing after ad-hoc signing; full paths in PROGRESS.
2. Provision administrator-owned
   terminal policy through trusted SSH (no secrets in logs), test native zsh.
3. Resolve Windows Ctrl+C using only the session's owned pseudoconsole, verify
   raw-mode applications are unaffected, rerun the TLS reconnect fixture.
4. Implement the Windows compatibility layer and its requested tests; make one
   consolidated paired build after source checks, then deploy/verify.
5. Record limits and create a checkpoint commit when the milestone is complete.

Not verified: Windows Teacher runtime, all cross-OS combinations, full-screen
interactive applications, all shell shortcuts/history, slow-consumer stress,
35–40 physical x64 PCs. GUI currently shares the existing two child-window limit
with file windows, despite the backend's larger session limit. No media scaling
acceptance is implied.
