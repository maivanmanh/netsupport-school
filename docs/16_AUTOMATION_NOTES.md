# Integration automation on this development host

## Current owner instruction — manual permissions, 2026-10-03

Permission continuity/certificate deployment is deferred. Teacher and Student builds default
to explicit local ad-hoc development signing (`CLASSROOM_MAC_SIGN_MODE=adhoc`),
with ordinary manual macOS permission approval after updates. This does not
delete Keychain certificates, remove quarantine, or modify Gatekeeper settings.
The Apple signing/notarization path below is optional future work; enable it
explicitly with `CLASSROOM_MAC_SIGN_MODE=apple`.

For this local development mode, deploy the newly built Student with
`python3 scripts/deploy-responsive-control.py --development-ad-hoc --teacher ADDRESS`.
The flag requires an ad-hoc signature and strict integrity verification on both
ends; Apple mode retains its certificate/Gatekeeper gates. Deployment stops a
manually opened copy at the exact installed Student path as well as the owned
LaunchAgent before replacing that bundle, to avoid occupied media ports.
The development launch command remembers the supplied Teacher IP, so Finder
launches use the same room. Verified deployment removes its previous bundle
backups. Student takes a per-user/media-port instance lock before log creation;
opening it twice cannot truncate the live log or compete for the UDP port.

Student tray offers Exit Classroom and Restart Classroom, including during a
presentation or a pending connection attempt. Both stop/join monitoring and
reverse publishing, release native windows and sockets, and drop the Tokio
runtime. Restart then replaces the Mac process via exec (retaining launchd
scheduling) or launches a replacement on Windows. It retains runtime options
such as log path and ports while reading the saved role/Teacher IP. An old control
connection's reader/writer tasks are canceled when their last StudentControl
owner drops. Teacher Flutter ignores callbacks from superseded IPC sockets,
so an old socket closing cannot mark the replacement connection's roster offline.

Teacher tray also offers Restart Classroom. The Flutter Teacher agent tears down
presentations, Observe receiver, listeners and its endpoint, then requests a
same-arguments restart after Tokio runtime shutdown. This retains Teacher role,
bind IP, IPC descriptor path and console executable; GUI IPC and Students reconnect
automatically. The authenticated `restart` IPC action uses the same teardown path
for bounded integration testing.

Observe requests signal a condition variable to wake the selected monitoring
peer immediately instead of waiting for its 2-second thumbnail interval. Normal
thumbnail cadence is unchanged; shutdown wakes sleeping monitoring peers too.
Logs expose request queue wait, source handshake and receiver start wait. Flutter
polls native first-frame readiness every 100 ms during startup, then returns to
1-second stats/keepalive sampling after the first frame (or after a bounded
10-second startup budget/error). These Teacher-side changes need no protocol change.
Student additionally bounds cached-thumbnail frame polling to 20 ms and retires
one old thumbnail capture on a joined cleanup worker so native capture teardown
does not block the Observe reply. Initial thumbnail acquisition retains its
1200 ms budget; cached static thumbnails do not wait for a new frame. This
Student-side optimization requires updated Student binaries and normal manual
Mac privacy reapproval after an ad-hoc update. Cleanup is bounded to one worker
and joined on exit; full-resolution media continues through the shared publisher.

Rebuild/redeploy/restart Students only when a change affects Student runtime or
the protocol. Flutter-only Teacher fixes do not require Student updates. Keep
the Windows Student already running when testing Teacher UI/input fixes. Mac
Student is optional for this current test and may remain stopped.

Password SSH/scp must run outside the workspace sandbox with approved escalation.
OpenSSH inside the sandbox can fail to open /dev/tty (Operation not permitted),
then send an empty password and misleadingly report Permission denied. Even
pty.fork wrappers are affected. Before treating credentials as wrong, check this
condition. Use Remote/RemoteProcess in scripts/run-four-machine-test.py outside
sandbox; credentials remain in memory. Key-based Mac SSH works in sandbox.

Mac test Student should run in its graphical login domain (gui/<uid>) through an
ephemeral LaunchAgent to use granted Screen Recording/Local Network permissions.
Set `ProcessType=Interactive` and `LegacyTimers=true` in that development job.
The default Standard job coalesces timers and can throttle resources: on the
M1 Ultra, changing only these settings raised reverse-source encode throughput
from 9.4 to 22.1 FPS in a mostly static Observe probe, without changing the binary.
Active publisher/receiver sessions also hold a scoped NSProcessInfo latency-critical
activity so manually launched or minimized agents are not treated as idle work.
The activity ends on session completion/error; it does not run while merely
connected. `publisher_pacing_wait` and `publisher_pacing_overshoot` expose actual
sleep duration and lateness after wakeup (pre-sleep tick lateness alone misses it).
Direct sshd launches can receive different TCC treatment. Stop and bootout the
job after bounded tests; this is not production auto-start installation.

Windows temporary interactive test tasks on Parallels laptops must set
-AllowStartIfOnBatteries -DontStopIfGoingOnBatteries. Otherwise a registered task
may remain queued and never create the Student log. Resolve native executable
paths and inspect task status instead of assuming Start-ScheduledTask means running.

## Stable macOS bundles and signing

Teacher build: `scripts/build-flutter-macos.sh`. Mac test Student build:
`scripts/build-student-macos.sh`. Both use `scripts/sign-macos-app.py`, one Apple
signing identity in Keychain and stable bundle identifiers. Set
`CLASSROOM_MAC_SIGN_IDENTITY` to a certificate name or SHA1 when multiple eligible
identities exist. No private keys are exported or saved. Ad-hoc fallback is
forbidden. Default designated requirements are retained; update manifests reject
silent changes of signing team/requirements.

Build preflight requires a positive OS certificate trust/revocation check. Signed
bundles also require Gatekeeper acceptance before publication, and deployment
checks both locally and remotely. `codesign --verify` integrity success alone
does not establish trust. An expired/revoked certificate, an unavailable positive
revocation result, or a rejected Gatekeeper assessment stops the workflow. These
are build/deployment checks, not an Internet dependency in classroom runtime.

Apple Development signing is for development and does not guarantee distribution
acceptance. Production delivery needs an appropriate Developer ID certificate
and notarization. Never bypass a malware/revocation block by stripping quarantine,
changing trust, disabling Gatekeeper or selecting another old identity.

Mac Student stable deployment path:
`~/Classroom-two-mac/Classroom Student.app`; graphical LaunchAgent executes its
`Contents/MacOS/ClassroomStudent`. The old `bin/Classroom` path becomes a symlink.
One initial privacy grant may be required when migrating from the raw ad-hoc
binary. Stable compatible signatures permit permission identity continuity;
macOS still controls permission approval and any restart/reconfirmation policy.
App-default logs go to `~/Library/Logs/Classroom/<role-port>/log.txt`, outside
the signed resources; explicit `--log-file` remains supported.

On 2026-10-03 the available signing identity was rejected with
`CSSMERR_TP_CERT_REVOKED`; Gatekeeper moved Teacher to Trash. Remote Student using
the same certificate was stopped. No blocked artifacts may be launched. Owner
must obtain a valid certificate (Xcode Settings > Accounts > Manage Certificates)
and rerun the trust gates; further Gatekeeper rejection must also be resolved
through valid signing/distribution rather than a bypass.

The owner's replacement on 2026-10-03 (Xcode label "Mai's MacBook Pro") is
Apple Development identity A9F06E151E345F70B12858DF2BC32945BFF60945.
Its OS certificate trust/revocation check succeeds. Teacher and Student build
and sign with it, but Gatekeeper assessment still rejects both development
bundles, so publication/deployment remain stopped. Do not create another
Apple Development certificate to solve distribution assessment. Developer ID
Application and notarization are Apple's distribution path:
https://developer.apple.com/help/account/certificates/create-developer-id-certificates/
https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution

Developer ID support is implemented in the signer: secure timestamp, hardened
runtime, opt-in notarytool submission with an existing Keychain credential
profile, stapling/validating the accepted ticket, then Gatekeeper assessment.
Set `CLASSROOM_MAC_SIGN_IDENTITY` to the Developer ID Application fingerprint
and `CLASSROOM_NOTARY_PROFILE` to the saved profile name for either Mac build
script. Only configuring this profile enables uploading the signed app to
Apple; this is not performed automatically with Apple Development credentials.
Credentials remain in Keychain, not scripts, manifests or command arguments.
Rejected submissions or invalid tickets stop publication. A change from Apple
Development to Developer ID may change designated requirements: existing
manifests deliberately stop that migration for review before notarization;
retain the old manifest when reviewing and document any initial privacy grant.


## Windows Teacher control shortcuts (2026-10-05)

Windows Observe/Control enables a foreground-scoped low-level Win-key hook only
while acknowledged Control is live and focused. Win shortcuts route through the
existing Rust input channel; Observe and other local applications retain normal
Windows shortcuts. Hook callbacks post window messages instead of blocking on
network or Flutter. Ctrl+Alt+Delete/secure-desktop input is not supported by this
change. Footer and its bottom hover trigger are absent in Control/exclusive;
Observe retains auto hide/show. See diagnostics/2026-10-05-windows-control-shortcuts.md
under checkpoints for the actual Windows-to-Windows Win+R result and test commands.
