# Deployment cleanup — 2026-10-09

Owner requested removing obsolete deployments on Teacher and Students and
keeping future deployments in a single reused path.

## Teacher

Current Teacher remains at
`target/present-recovery-build/Classroom Console.app`, with its paired Student.
No rebuild, signing or restart was performed for this cleanup. Process paths
were checked before deleting obsolete bundles. The system ClassroomSettings
extension was unrelated and was left alone.

Automatic review rejected wholesale directory deletion with selective data
preservation. The accepted narrower operation archives complete obsolete bundles,
removes only those bundles, and consolidates all remaining files without deleting
them. Recoverable local artifacts are under `target/retired-deployments-20261009`;
these are not release packages and should not be used for permission grants.
The machine-readable inventory is `teacher-cleanup.json` within that directory.

## Future updates

Student updater now attempts to remove the obsolete backup immediately after the
new Student reconnects. Rollback remains available before that confirmation;
cleanup errors are reported and do not roll back a working installation. This
source change has not been built, tested or deployed in this cleanup turn.
`git diff --check` passed. Existing remote Student deployments have not been
changed by the Teacher cleanup operation.
