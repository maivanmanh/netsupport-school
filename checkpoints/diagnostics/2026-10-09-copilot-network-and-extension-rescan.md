# Independent Copilot network block and extension rescan

## Owner scope

Windows enforcement only. Put the independent checkbox in the existing
Copilot row, apply to all capable online Students, report X/Y, preserve the
extension allowlist's original target selection. Rescan every currently online
Student, deduplicate by extension ID, highlight newly found IDs at the top,
wait until every snapshot member answers unless Teacher presses Stop. Sort
checked extensions first on initial dialog load only. Confirm VS Code restart
before Apply.

## Implementation

- Dedicated authenticated copilot_policy job and capability. Teacher publishes
  confirmed per-Student block state; unsupported Macs do not enter denominator.
- Guard persists a separate copilot_block flag with its existing lease. Internet
  changes preserve it; Copilot changes preserve Internet mode/rules/lease.
- Guard's shared DNS proxy refuses githubcopilot.com (including subdomains),
  copilot-proxy.githubusercontent.com and origin-tracker.githubusercontent.com.
  Original upstream DNS resolves nine known inference/proxy hosts; cached
  Windows DNS answers are included before flushing. WFP blocks resolved public
  IPv4/IPv6 endpoints, above ordinary permits. Refresh every 60 seconds; retain
  earlier addresses while enabled, bounded to 512 entries on every update;
  oldest addresses are evicted so newly resolved IPs can still be blocked.
- WFP plan replacement uses transactions, preserving prior filters on refresh
  failure. ALE policy changes trigger reauthorization of affected existing
  flows; this is not claimed as an instantaneous barrier for in-flight data.
- Failed policy changes attempt to restore the prior policy. If restoration
  fails, clear restrictions and report the failure instead of claiming success.
- Known public IPv4 encrypted-DNS resolver blocks reuse existing Guard behavior.
  No blanket github.com, githubusercontent.com, or all-Internet block added.
- Extension allowlist explicitly denies wildcard and the four Copilot IDs,
  then reads the registry policy back before reporting completion/restarting.
- Mac Student can now report installed extensions, without gaining Windows
  enforcement support. List capability is distinct from policy capability.
- Rescan snapshots all online IDs; polling/retries continue until a response
  arrives, with Stop/dispose cancelling outstanding listing slots. Partial
  discoveries remain visible. Lists are additive within the open dialog.
- New IDs are inserted above existing rows with green tint and NEW badge.
  Checkbox changes do not resort the list. Initial scan sorts checked first.
- Copilot checkbox applies immediately and independently; errors expose Retry.
  Extension Apply opens a Save-work/restart confirmation first.

## Sources and limits

Dedicated service domains: https://docs.github.com/en/copilot/reference/copilot-allowlist-reference

Existing-flow behavior: https://learn.microsoft.com/en-us/windows/win32/fwp/ale-re-authorization

Domain/IP filtering is stronger than disabling the extension but not a universal
AI prohibition. Unknown private DoH, VPN/proxy/tunnel, custom/BYOK endpoints,
Enterprise GHE hosts, changed service endpoints, and offline models remain
outside this preset. Shared CDN IPs can affect other services using the same IP.
Newly connected machines are reflected in X/Y; the toggle targets the online
snapshot when used, not a permanent Teacher-side auto-apply rule for future
arrivals. Existing Student policy persists/resumes within its Guard lease.

## Verification

Mac cargo check and Flutter analyze passed. No automated tests run. Paired build
completed, including Windows x64 Student/Guard cross compilation and Flutter.
Teacher was replaced at its stable path and reopened; the matching Student ZIPs
are embedded. Student deployment is blocked: `.155` and `.254` stopped being
reachable after the Teacher restart. Both ping/TCP22 probes failed, Mac SSH
timed out, and updater refused offline targets before sending any binary.
Neither Student has been replaced in this milestone. Owner was asked to confirm
power/connectivity/current IPs. This is not attributed to macOS privacy from
connection failures alone. Manual checks
after deployment: toggle with allowlist off, enable extension again, try inline
chat and panel chat, toggle off, verify ordinary GitHub/package downloads and
Internet policy coexistence. Verify scan/new highlighting/Stop/reopen ordering.

Deferred lab checks: 35–40 Windows x64 PCs, mixed slow/offline/rejoining Students,
Rescan cancellation under load, endpoint refresh and existing active chat streams,
and normal broadcast/control during network enforcement. No scaling acceptance
or complete Copilot runtime blocking is claimed from build checks.
