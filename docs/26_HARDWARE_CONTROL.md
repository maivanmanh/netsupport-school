# Hardware Control — 2026-10-11

**CHƯA TEST WINDOWS THỰC TẾ.** Chỉ kiểm tra source, unit/widget tests,
PowerShell mock và cross-target compilation. Chưa xác minh thực thi/chặn/khôi
phục thiết bị trên Windows thật; không coi đây là bản đã nghiệm thu Windows.

Owner-authorized addition to the existing Flutter Teacher / Rust Student. Owner
explicitly states no Windows test machine is available: implement and validate
source locally; do not deploy or claim Windows runtime acceptance.

## Architecture and conventions

Teacher uses a dedicated dependent Flutter process/window, the existing Material
theme, `Agent.request`, private authenticated loopback IPC, tools Hub, roster and
bounded length-prefixed Protobuf `ToolPacket` poll/ack transport. The toolbar USB
button opens/focuses one Hardware Control window. Selection is intentionally
ignored; no per-device commands or parallel classroom server/listener exist.

The existing Guard policy expires by day/lease and clears on exit. Hardware must
survive those events, so its durable desired document is owned by the same tools
Hub but has **no day, lease or disconnect expiry**. It does not change Internet,
VS Code or application rules. This separate lifecycle is required by the owner's
latest instruction, not an extension of those transient rules.

Teacher saves `hardware-policy.json` beside the existing persistent IPC/profile
files before acknowledging a change. Each single-group edit preserves the other
groups and gets a fresh random revision. Every connected capable peer receives
the complete document on its next poll; offline and previously unknown late
joiners get the current document on reconnect. Teacher restart reloads it. An old
or mismatched revision cannot change current state. Missing ACKs time out at 60s;
unanswered policies retry after 65s. Current OS reports arrive about every 10s;
reports older than 70s fail rather than remaining green indefinitely.

Student owns one latest-request slot and one OS worker. File/Windows operations
never execute on control, media or native UI threads. The existing SYSTEM Guard
Service runs the same reconciler before login and while Student/Teacher are absent.
A protected global mutex serializes service and Agent transactions. Every OS
invocation has a 45s deadline. A fixed embedded PowerShell script is compressed
inside the binary; JSON arrives on stdin, never interpolated into commands. No
PowerShell profile, internet download or external script installation is required
on Student. Error and report sizes are bounded by the existing tools packet limit.

## Enforcement and honest status

| Switch | Windows mechanism | Safety / limitations |
| --- | --- | --- |
| USB Storage | HKLM machine Removable Disks `Deny_Read` and `Deny_Write` | Does not touch HID, USB hubs, drivers or disk contents. Open handles/cached policy can survive a live change: Pending until a subsequent Windows boot. Fixed-media USB disks are reported Unsupported because the removable-disk policy cannot guarantee all driver coverage. No forced dismount/offline/reboot. |
| Wi-Fi | Enumerate `Get-NetAdapter` by NDIS physical medium; `Disable-NetAdapter` | Refuses to disable an adapter routing to Teacher. Unknown/virtual management routes fail conservatively. Physical management LAN remains enabled. |
| Bluetooth | Disable hardware radio PnP instances | Walks input-device parent chains first. Bluetooth keyboard/mouse/HID, unverified ancestry or active Bluetooth networking causes refusal. No blanket Bluetooth-class deletion. |
| USB Tethering | Identify USB network interfaces by RNDIS, CDC ECM/NCM compatible IDs or Apple VID; disable matching adapters | Same management-route protection. Unclassified USB Ethernet is preserved and reported Unsupported; no name guessing or USB-controller disable. |
| Camera / Microphone | Disable Camera/video Image PnP nodes and capture AudioEndpoint IDs (`0.0.1`) | Does not disable speakers (`0.0.0`), all audio controllers or all Image devices. Drivers that refuse disable produce Failed. Legacy/vendor capture drivers outside these identities are not guaranteed. |

`Applied` means the OS configuration was checked, including adapter administrative
state/PnP disabled code 22. It does not assert a physical device/application test.
`Pending` explains an offline machine, outstanding ACK, cached USB decisions or
an unplugged device awaiting restoration. `Failed` includes OS/permission/route
errors and timeout. `Unsupported` includes older/non-Windows/non-elevated Agents
and unclassified driver cases. Each group shows all four counts and per-machine
details. A partial group failure does not hide the results of other groups.

Reconciliation is periodic, not a kernel device-arrival veto: a newly introduced
radio/camera/network interface can be usable briefly before the next scan. The
10s interval is a scheduling target, not a hard deadline when CIM/OS calls stall.
Guaranteed zero-window hotplug enforcement and complete fixed USB-disk coverage
would require a supported device-control/filter driver or managed endpoint
policy backend. This implementation makes neither claim.

## Persistence, privileges and recovery

State is machine-wide at `%ProgramData%\Classroom\HardwareControl\state.json`,
outside the app/update directory. It contains desired policy, Teacher endpoint,
original registry values, only the device IDs Classroom changed, and the latest
group report. Directory/file owner and ACL are restricted to SYSTEM and Builtin
Administrators. Reparse points are refused. Atomic flushed replacement persists
intent and rollback entries **before** OS changes; after a crash, the service
retries from that journal. Existing settings, device identity and profiles remain
untouched. Already-disabled devices never enter the journal. Unblock restores
original registry values and only Classroom-owned disabled devices; disconnected
devices retain restore entries until replugged. Partial restore commits each
completed item so retries are idempotent. Divergent administrator/GPO registry
values produce errors rather than being silently overwritten during restore.

The current project privilege model remains: the Student handling a new policy
must run elevated, and the **updated** `ClassroomGuardService` must be installed
and running. Administrator installation/startup provisioning is required; a
standard-token Student reports Unsupported rather than elevating itself. The
service continues an already-persisted policy without a logged-in administrator,
but this change does not introduce a privileged broker allowing standard-token
Students to accept new commands. Provision the existing elevated Agent startup
for managed machines. Existing service installations must be updated/restarted
by an administrator; merely replacing Student does not update a running service.

Protection applies to ordinary local users with correctly provisioned Agent and
service ACLs. It does not defend against local Administrators, offline boot/disk
editing, or a compromised Teacher. Teacher authorization remains the existing
OS operator/private IPC and tools roster/IP model. Tools LAN messages remain
plaintext: this feature does not add certificate authentication or promise
protection from active LAN impersonation. Network trust must match the existing
managed classroom deployment model.

The service writes a protected hardware heartbeat. Agent requests and periodic
status checks require a running service and a heartbeat no older than 70 seconds;
an older installed Guard binary cannot silently count as durable support.

Default: blocks persist through disconnect, Teacher exit, stopping presentation,
closing this window, midnight and both machines restarting. Optional **restore
on session end** only enables the explicit **Kết thúc phiên** button in this
window. It publishes a new all-Unblock class policy; offline machines restore on
reconnect. Teacher exit/disconnect is never inferred as session end.

Local emergency recovery (Administrator terminal): stop the owned Student first,
then run `ClassroomStudent.exe --hardware-reset`. It uses the same rollback journal
even with the service stopped, prints per-group results and fails if restoration
fails. Do not delete the journal: disconnected devices may still need it. Set the
Teacher policy to Unblock too before reconnecting, otherwise the latest classroom
policy intentionally supersedes the local emergency reset. USB may still need a
manual reboot; existing independent OS/GPO restrictions are preserved.

## Validation

Commands and measured results are recorded in `checkpoints/PROGRESS.md`.

- Rust tests: independent edits, persistence, late join/reconnect, stale/mismatched
  ACK, timeout, offline status, driver error and explicit session restore.
- `scripts/test-hardware-control.ps1`: parses and loads the **actual enforcer
  functions** with all OS calls mocked. Covers route/virtual adapter refusal,
  tether classification, unknown USB Ethernet, journal-before-mutation, camera
  and microphone versus speaker/keyboard, pre-disabled baseline, Bluetooth input
  ancestry, unplug/replug restoration, storage baseline, reboot and fixed media.
- Flutter tests: offline/unsupported cannot count as Applied; separate class UI,
  five device groups, persistent lifecycle notice and a real switch callback
  sending a class-wide edit without selected device IDs.
- Cross-target Rust checks compile Teacher, Student and Guard Service for Windows
  x64; no package is shipped by this implementation-only task.

Still required when Windows hardware becomes available: real read/write attempts
(including already-open files), external fixed/removable media, all tether driver
variants, actual camera/microphone app access, local standard-user tampering,
service reboot/startup/update, disconnect/rejoin, disabled-device hotplug,
emergency reset and sustained media-load overhead. No runtime pass is claimed.

Primary Windows references:
[Removable Storage policy](https://learn.microsoft.com/en-us/windows/client-management/mdm/policy-csp-admx-removablestorage),
[Disable-NetAdapter](https://learn.microsoft.com/en-us/powershell/module/netadapter/disable-netadapter),
[Disable-PnpDevice](https://learn.microsoft.com/en-us/powershell/module/pnpdevice/disable-pnpdevice).
