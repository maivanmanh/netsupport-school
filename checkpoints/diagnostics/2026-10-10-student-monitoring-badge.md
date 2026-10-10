# Student Monitoring Badge — 2026-10-10

## User-visible behavior

- Toolbar eye icon opens the `Đang giám sát` dialog. It targets selected
  checkboxes, or all room computers when the selection is empty. The tooltip says
  this explicitly. Right-click on one computer still opens the per-computer menu.
- Teacher receives per-Student status. The Student creates or removes one native
  click-through overlay and acknowledges the requested revision.
- Red background alpha is 75%. The white label and dot are opaque; the dot pulses
  on a two-second cycle.
- Student preference and Teacher desired state persist. Disconnect and Teacher
  shutdown/restart do not hide an enabled badge. Only a later explicit OFF does.

## Paired build and deployment

Build label: `paired-monitoring-badge-final-20261010`.

The paired build produced Mac Teacher, Mac Student and Windows x64 Student plus
Guard. Windows Student `.94` completed the updater and confirmed the new version.
The first Mac `.2` transfer timed out during reconnect and reported rollback. The
Student log showed a temporary new-process start, but its manifest afterward was
the old version, so that first attempt was not counted. After restarting Teacher,
the Mac update was retried and completed with build ID
`1791600016-paired-monitoring-badge-final-20261010`. Both endpoints were online
afterward.

Teacher installer had two orchestration issues: it initially looked for its
staging bundle under the previous stage path; then its state polling continued
printing duplicate `teacher_ready` replies until timeout even though the paired
Teacher was running and both Students were online. The bundle itself passed deep
strict code-signature verification and was installed. The old bundles were kept
in staging by the installer after its timeout.

## Verification

- `cargo test -p classroom-app --bin Classroom monitoring_badge -- --test-threads=1`:
  5 passed.
- Flutter analyze for `main.dart` and `monitoring_badge.dart`: clean.
- `flutter test test/monitoring_badge_test.dart`: passed.
- `scripts/test-student-monitoring-badge.py` on `.94` and `.2` after final update:
  bulk enable/native display ACK, individual OFF isolation, rapid latest preference
  wins, and 5 repeated create/destroy cycles passed. Initial states restored.
- Teacher restart with both badges ON: both Students reconnected and acknowledged
  ON from the persisted desired state. Initial states were restored afterward.
- Mac Student native window had previously been observed through the macOS window
  list in this session. Current final build again passed native display ACK.
- Windows Student verifies required passive/layered styles internally during
  overlay creation/draw. Separate external EnumWindows enumeration through a
  scheduled task did not write its output JSON, so independent external style
  inspection remains unverified.

## Not covered

No physical 35–40 Windows lab test was run. Additional physical checks for DPI,
multiple displays, fullscreen applications, click-through and pulse appearance
remain. Ordinary Student logout/exit removes the process-owned overlay; persistence
is across network/Teacher interruption and Student restart, not a tamper-proof
service.

The working tree also contains unrelated in-flight Native Remote Terminal work,
so these changes were documented without creating a commit that would include
unrelated files.
