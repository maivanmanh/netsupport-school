# Windows .94: Copilot WFP and connection retest

## Findings and fixes

- `0x80320025` is FWP_E_INVALID_WEIGHT. The endpoint filter used
  FWP_UINT8 range index 16, while Windows accepts 0–15. Use explicit ordered
  ranges: endpoint 15, essential Teacher/upstream DNS 14, listed block 13,
  ordinary permit 12, default block 1. Retain IPv4 and IPv6 endpoint support;
  earlier attribution to an incompatible IPv6 layer was incorrect.
- First runtime attempt installed the policy but reported a failed job after
  tools disconnected. Student logs show enforcement completed before tools
  reconnected. `keepalive()` waited for the Guard mutex held by slow policy
  application, blocking tools polling beyond its socket timeout. Heartbeat and
  offline lease checks now use try_lock; policy application refreshes its lease
  on completion. Added a regression test while the policy mutex is held.
- Manual executable launch did not use the task's explicit connection args.
  Restarting the exact Student via its elevated interactive task with Teacher
  192.168.50.206/control port 48900 restored connectivity. Added adjacent
  classroom-bootstrap.toml with that IP/port for direct executable launch.

## Deployment and verification

- Endpoint: Windows ARM64 Parallels, x64 Student through emulation, SSH .50.94.
  This is functional evidence, not physical x64/wired performance acceptance.
- Paired build `paired-copilot-heartbeat-fix-20261010` completed: Mac Teacher,
  Mac Student, Windows x64 Student/Guard, matching ZIPs embedded in Teacher.
- Teacher replaced at target/present-recovery-build/Classroom Console.app.
  Existing .94 Student updated through Classroom; updater confirmed complete,
  preserved identity, and reported build
  1791588814-paired-copilot-heartbeat-fix-20261010.
- Student remains at
  C:\Users\mvmanh\Classroom-dev-update-20261005-194235\ClassroomStudent.exe.
  Existing profile/logs preserved. Task Classroom-Manual-Dev-Student runs
  interactive/elevated; obsolete Classroom-Dev-Student task disabled.
- 15 existing/new Guard tests passed; changed Rust files formatted and diff
  checked. Windows compilation and actual WFP application succeeded.
- Before block: api.githubcopilot.com resolved, HTTPS returned 404 (reachable);
  github.com and example.com returned 200.
- Block enabled: job complete, confirmed blocked set contains .94;
  api.githubcopilot.com DNS returned no address and curl failed with code 6;
  github.com and example.com still returned 200. Control remained online.
- Block disabled: job complete, blocked set empty; Copilot resolved again and
  HTTPS returned 404; ordinary sites remained 200. Automated test ended with
  block off. Subsequent UI jobs enabled Copilot block and VS Code allowlist;
  final live snapshot confirms both jobs complete and Student online. Those
  later interactive selections were preserved.

Local ignored evidence: target/copilot94-final-build.log,
target/copilot94-final-update.log, target/copilot94-guard-tests-final.log,
target/copilot94-network-results.json.

## Remaining checks

Owner should retry actual VS Code panel and inline Chat with the checkbox on.
This test did not submit a signed-in chat request, exercise unknown/VPN/BYOK
endpoints, or prove existing authenticated stream termination. IPv6 filters
install successfully but IPv6 Internet connectivity was not measured separately.
Direct Run-as-admin launch after adding adjacent configuration has not yet been
repeated; the running elevated task is confirmed connected.
Independent Internet allow/block policy coexistence, refresh soak, rescan UI,
and 35–40 real Windows x64 machines remain deferred.

Reference: https://learn.microsoft.com/en-us/windows/win32/api/fwpmtypes/ns-fwpmtypes-fwpm_filter0
