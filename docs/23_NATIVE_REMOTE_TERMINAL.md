# Native Remote Terminal

## Owner change — 2026-10-12: direct classroom Terminal

The owner explicitly replaces manual administrator pairing with direct access
from the Teacher to a connected Student. No `remote-terminal.json` installation,
copy-policy dialog or per-machine pairing step is required. Windows/macOS
Students advertise Terminal support when the platform supports the worker.

The authenticated local Teacher UI selects an online roster member. Teacher
generates a fresh random secret per session and sends its certificate and that
secret through the Student's existing configured Teacher tools connection.
Student validates the offered Teacher address against its configured endpoint,
passes only this session grant to the ordinary-user worker, and removes the
temporary worker context after reading it. TLS validates the offered certificate
and authenticates the session secret before any shell starts. No persistent
Student trust file is written; old pairing files are unused and left untouched.

This intentionally trusts the configured classroom connection for bootstrap:
it is not out-of-band certificate verification, and does not authenticate a
hostile LAN independently of the existing classroom control/tools channel.
Non-elevated token checks, bounded queues/session limits, reconnect grace,
process-tree cleanup, metadata-only auditing and TLS remain in force.
The pairing/revocation paragraphs below record the superseded original design.

Owner-authorized scope 2026-10-10: Windows/macOS Teacher and Student. Shell runs
on Student only: cmd.exe/ConPTY, /bin/zsh login-interactive/POSIX PTY through
portable-pty; Flutter uses a native xterm emulator, no browser/WebView.

## Original design (pairing rules superseded above)

- Reuse roster, tools listener and authenticated local IPC. Terminal connects
  back to the existing Teacher tools port; no inbound Student listener/server.
  Upgrade a typed tools connection to TLS and use bounded binary frames for
  immediate raw bytes, input, resize, close, acknowledgements and status.
- Separately provision each managed Student with Teacher certificate and unique
  secret. Certificate pin/trust is mandatory; no trust-on-first-use from an
  unauthenticated LAN offer. Authenticate the Student secret inside TLS. Opening
  requires a valid roster member, paired identity and the authenticated Teacher
  user. Local OS login is the operator identity; this is not a directory/RBAC
  server. Pairing/revocation are administrator operations over a trusted channel.
- Disabled on unprovisioned Students. Student managed policy resides outside
  the executable/update folder, writable only by administrators. Do not enable
  through an ordinary untrusted tools message or bundle a room-wide password.
- Windows elevated Student launches a separate worker with the active desktop
  user's non-elevated token; the worker checks again before starting ConPTY.
  Root/macOS workers refuse shell creation. Fail closed if safe launch fails.
  No ShellExecute runas, sudo, hidden elevation or Student desktop focus changes.
- Student indicates active management sessions via tray status; no modal prompt
  or shell window. School deployment must disclose the remote management policy.
- Audit session open/attach/detach/reconnect/close/error and byte counts/resize
  metadata. Do not log raw keystrokes, passwords or terminal output. Rotate logs.
- Bound concurrent sessions globally/per Student and use bounded queues.
  Backpressure stalls the PTY rather than dropping terminal output. A short
  disconnect retains the shell and queued bytes; expiry closes the session and
  process tree. Input/output sequence acknowledgements prevent duplicate input
  or output during retry. A new session never resumes a terminated shell.
- UI: independent windows, Student OS determines Consolas/Menlo with configurable
  fallback, default 12pt, persistent appearance/scrollback, native resize,
  VT/ANSI/Unicode/selection/clipboard and interactive key forwarding.

## Verification boundary

Build checks do not prove native PTY integration. Required runtime cases include
Windows/macOS cross combinations, initialization/PATH/cwd, tab/history, Unicode,
colors/cursor, Ctrl+C/Ctrl+D, full-screen apps, slow consumers, window resize,
brief disconnect, expired disconnect, denial/revocation, non-elevated token and
cleanup of long-running child processes. No 35–40-machine/media-load acceptance
is implied by local terminal tests.

Sources: https://docs.rs/portable-pty/0.9.0/portable_pty/
https://docs.rs/tokio-rustls/0.26.4/tokio_rustls/
https://pub.dev/packages/xterm
