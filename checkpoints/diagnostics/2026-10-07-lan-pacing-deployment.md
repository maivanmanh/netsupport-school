# 2026-10-07: paired pacing/recovery deployment and minimal smoke

## Implementation and verification

Session-wide 400 Mbps wire pacing now covers video, repair, pointer and retries.
AU wire-time admission and absolute 120 ms normal / 300 ms IDR budgets replace
8 ms burst acceleration. Bounded fan-out waits before acquiring latest capture;
failed endpoint reference chains request a coalesced IDR. Student abandons expired
gap repair and re-anchors recovery at parameter-set/IDR-start packets. Refinement
respects sender headroom/pressure. Silent disconnect popup fix is included.
Protocol remains 8; no schema/port change. New pacing/cache/recovery telemetry
records remaining bottlenecks. See docs/17_BROADCAST_OPTIMIZATION_PLAN.md section 11.

73 distinct focused local tests passed; one manual benchmark ignored. Cargo check
and clippy completed successfully with existing warnings; scoped formatting passed.
No further build or extended benchmark after the owner's request to finish quickly.

## Paired build and deployment

Paired MSVC Rust/Flutter build label `lan-paced-20261007`, Student probe build
`1791350009-lan-paced-20261007`. Fresh matching Student ZIP included in Teacher.
Teacher helper SHA256: `32db5ef6de6ccb1fac2bfcbf30616ebfd9ec11d6db001e7ed0add85a94eb70b4`.
Student SHA256: `b2abab91d2e8d285eb3911d31c408c7d67159346295621efa5c6b04a2b8fd870`.
Bundled ZIP SHA256: `794788739e7d3d0d2736ed31329b2805e941db7e92382d1a01656af73b0b8710`.

Old Teacher GUI/helper exited gracefully after idle/no-Observe verification. Old
bundle was moved into a verified backup path; fresh bundle copied to the existing
approved executable path, preserving firewall context/settings/profiles/logs.
Running Teacher: `target/Classroom-Teacher-adaptive-20261007`, bind `10.0.55.10`.
GUI PID 14152; Rust helper PID 7348 at deployment verification.

Classroom updater job `243c5f92a63bd53ff34879479f774092` completed for **35/35 online
Students**, all verified at the fresh build with the same baseline device IDs.
`505-25 / 10.0.55.125` was already offline and was not deployed. No SSH overwrite.

## Minimal runtime smoke (owner time constraint)

Stream `7916656969460464599`, 10-second current Teacher desktop share to 35 endpoints;
media duration 9.499 s, actual 1080p, one Quick Sync encoder. All 35 reports decoded
158 frames; last reported rendered counts 152-158. Encode 22.42 FPS, p95 13.88 ms;
aggregate send 148.18 Mbps. Sent 168,840 recipient datagrams, abandoned 23,415.
Socket send errors/WouldBlock/retry exhaustion: zero. NACK 35, cache misses zero,
repair errors zero. AU deadline drops/reference skips occurred; the whole-AU
admission/freshness policy still sheds frames and reduced cadence. This is a
functional start/receive/stop check, **not proof of smooth video or performance
improvement**. Do not compare this desktop workload numerically with prior YouTube.
Teacher returned idle. Diagnostics collected from 3/3 representative Students.

An earlier native workload attempt failed its foreground precheck before Share;
no classroom share was started by that attempt. Long A/B/video/40-PC tests were
cancelled at the owner's request. Next step is a matched video/basic workload
comparison, especially AU shedding and encode/send serialization cadence.

## Repository cleanup

Owner requested removal of generated/temp/unrelated files and local commit only.
Raw diagnostic/build/deployment evidence and generated outputs are archived outside the repository at
`../netsupport-school-local-evidence-20261007`. Automatic review rejected recursive deletion twice. Generated Rust/Flutter output,
old duplicate packages and the final Teacher bundle were moved outside the repository
using checked paths. The owner then requested removal of Teacher binaries as well:
Teacher quit gracefully from idle, owned processes exited, and target was moved to
`../netsupport-school-local-evidence-20261007/stopped-teacher-target`. No build/output
remains in this repository; files outside the repository are archived, not deleted. Keep the build-generated
pubspec.lock dependency resolution for reproducibility; registrants have no
semantic changes. Owner waived the commit after the Git identity question. No commit or push.
