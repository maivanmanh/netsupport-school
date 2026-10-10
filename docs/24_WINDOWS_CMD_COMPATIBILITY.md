# Unix-like command helpers in managed Windows CMD

Owner-authorized extension to Native Remote Terminal, 2026-10-10.

## Scope and precedence

The shell remains the same persistent `cmd.exe` in ConPTY. CMD owns command-line
editing, history, quoting, command chains, pipelines and redirection. No raw PTY
input is buffered into lines or rewritten, and no arbitrary argument is evaluated
as PowerShell/Rust-generated shell code. Compatibility commands are session-only
`.cmd` entry points at the **end** of PATH, invoking the current Student executable
in a literal-argument helper mode. Builtins, DOSKEY aliases, commands in the
current directory, and programs earlier in PATH retain priority. macOS zsh does
not receive these helpers or environment changes.

This is a supported-command layer using CMD syntax. It is not a POSIX shell:
use double quotes for paths containing spaces, CMD pipes/redirection/escaping,
and native environment expansion. Single-quoted POSIX shell expressions,
substitution, shell scripts and regex forms outside the supported subset are
not translated. Existing native commands continue to run as native commands.
Unsupported helper switches fail with a nonzero status before file mutation;
they are never guessed into a stronger operation.

## Commands

| Input | Implementation / supported forms |
| --- | --- |
| `ls` | Native `dir`; `-l`, `-a`, combined `-la`; `-lh` uses a Rust listing with human-readable sizes. Native dir wildcard handling applies to ordinary listings; human-size listings require literal paths. |
| `clear`, `pwd` | VT screen clear/home; current working directory. |
| `cat` | Binary streaming from literal files or stdin, preserves bytes. |
| `cp`, `mv` | Rust file copy / rename; `cp -r` / `-R`; multiple sources require a destination directory. |
| `rm` | Literal file deletion; directory deletion requires `-r` / `-R`; `-f` permits missing targets. No elevation, permission modification, root/ancestor deletion or traversal of reparse points. |
| `rmdir`, `whoami` | Existing CMD/native commands, kept intact. |
| `touch` | Create without truncation; update access/modification timestamps. |
| `head`, `tail` | Default 10 lines; `-n N`; `tail -f FILE` streams appends and detects truncation. |
| `grep` | Literal search, `-i`, `-n`, `-F`; status 0 for a match, 1 for no match. Ambiguous regex constructs are rejected rather than approximated as findstr regex. |
| `which`, `ps`, `kill` | `where`, `tasklist` (`ps aux` accepted), `taskkill /PID`; no implicit `/F`. |
| `env`, `printenv` | Listing or one environment variable; no environment-clearing/process-launch syntax. |
| `export` | `NAME=value` assignments persist in the parent CMD via a generated, validated batch; Unicode uses a temporary UTF-8 code page and restores the prior page. Unsupported quotation/control characters are rejected. |
| `history`, `man` | `doskey /history`; native `help` with a validated command name. |
| `date` | CMD builtin retained by native-precedence requirement; `date /t` displays without requesting a date change. `time /t` displays time. Bare native `date` retains native behavior. |
| `sleep` | Finite nonnegative seconds, including fractional seconds. |
| `du`, `df` | Rust recursive byte accounting (`-s`, `-h`); Windows logical-disk capacity/free-space query. |
| `wc` | `-l`, `-w`, `-c`, `-m`; UTF-8 words/scalars; byte/newline counts also accept binary input. |
| `basename`, `dirname`, `realpath` | Literal native path components / canonical existing absolute path. |
| `open` | Literal ShellExecute target, disabled unless administrator policy enables `allow_desktop_open`. No `runas` or command interpolation. |

CMD builtins take priority even when their syntax differs from Unix. In
particular, this implementation does not replace `date`, `rmdir`, `cd`, `mkdir`,
`echo`, `ping`, `set`, or `whoami` with another interpretation. A native command
can still be called with its native flags.

## File-operation safeguards

Destructive wildcard expansion is rejected; use explicit paths or an explicit
native CMD operation. Source trees and existing destination trees are checked
for links/reparse points. Copy rejects source/destination overlap and same-file
hard-link aliases before truncation. Deletion refuses filesystem roots and the
current directory or an ancestor. No forced ACL/attribute changes, recursive
force upgrade, or administrator fallback occurs. Ordinary filesystem errors
remain visible. These checks do not add a filesystem transaction or eliminate
races caused by concurrent local modifications.

## Configuration and lifecycle

Terminal settings contain **Unix-like commands for CMD**, enabled by default,
persisted in Teacher preferences, applied to the next session. Changing it does
not restart a running shell or discard its environment/history. Helper files
are per worker and removed on orderly close. Managed policy and pairing are
unchanged; no additional LAN listener or port is introduced.

## Verification

Run focused Rust tests:

```sh
cargo test -p classroom-app --bin Classroom remote_terminal::compat::tests -- --test-threads=1
```

Run the owned temporary Windows PTY integration workload with an online, paired
Student `.94` and a local Teacher endpoint:

```sh
python3 scripts/test-native-terminal-compat.py --endpoint target/source-switch-endpoint.json
```

The integration workload uses a unique folder under the Student's TEMP directory.
It covers native CMD persistence, mappings/flags, Unicode/spaces, pipes and
redirection, parent environment exports, native command/DOSKEY precedence,
unsupported/destructive forms, and tail-follow interruption. A passing build or
Mac unit suite is not Windows runtime evidence. See the dated diagnostic report
for actual results and remaining limits.
