#!/usr/bin/env python3
"""Sign Mac bundles in explicit local development or Apple signing mode.

CLASSROOM_MAC_SIGN_IDENTITY can select a certificate name or SHA1 fingerprint.
CLASSROOM_NOTARY_PROFILE selects existing Keychain notarization credentials.
Apple mode never falls back to ad-hoc. Private keys remain in Keychain.
Local ad-hoc mode is explicit and does not preserve privacy grants on updates.
"""
import argparse
import base64
import hashlib
import json
import os
import plistlib
from pathlib import Path
import re
import subprocess
import sys
import tempfile


def run(*args, timeout=30):
    return subprocess.check_output(args, stderr=subprocess.STDOUT, text=True, timeout=timeout)


def verify_certificate(path):
    # find-identity and codesign --verify check usability/integrity, but can
    # still accept a revoked identity. Require a positive OS revocation check.
    try:
        run('security', 'verify-cert', '-c', str(path), '-p', 'codeSign',
            '-R', 'ocsp', '-R', 'require')
    except subprocess.CalledProcessError as error:
        raise RuntimeError('Signing certificate trust rejected: ' + error.output.strip()) from error
    except subprocess.TimeoutExpired as error:
        raise RuntimeError('Signing certificate trust check timed out; deployment stopped') from error


def verify_identity(fingerprint, name):
    public = run('security', 'find-certificate', '-a', '-c', name, '-p')
    for certificate in re.findall(r'-----BEGIN CERTIFICATE-----.*?-----END CERTIFICATE-----', public, re.S):
        der = base64.b64decode(''.join(certificate.splitlines()[1:-1]))
        if hashlib.sha1(der).hexdigest().upper() == fingerprint.upper():
            with tempfile.TemporaryDirectory(prefix='classroom-cert-') as folder:
                path = Path(folder) / 'leaf.der'
                path.write_bytes(der)
                verify_certificate(path)
            return
    raise RuntimeError('Selected signing certificate could not be located; signing stopped')


def verify_app(app, gatekeeper=True):
    run('codesign', '--verify', '--deep', '--strict', str(app))
    signature(app)
    with tempfile.TemporaryDirectory(prefix='classroom-app-cert-') as folder:
        prefix = str(Path(folder) / 'certificate-')
        run('codesign', '--display', '--extract-certificates=' + prefix, str(app))
        leaf = Path(prefix + '0')
        if not leaf.exists():
            raise RuntimeError('App signing certificate missing; deployment stopped')
        verify_certificate(leaf)
    if not gatekeeper:
        return 'Gatekeeper assessment skipped (Apple Development local signing)'
    # Do not deploy or launch code that Gatekeeper rejects. Integrity and a
    # non-revoked certificate alone do not establish Gatekeeper acceptance.
    try:
        assessment = run('spctl', '--assess', '--type', 'execute', '--verbose=4', str(app))
    except subprocess.CalledProcessError as error:
        raise RuntimeError('Gatekeeper rejected app; do not deploy or launch: ' + error.output.strip()) from error
    return assessment.strip()


def select_identity():
    candidates = []
    for line in run('security', 'find-identity', '-v', '-p', 'codesigning').splitlines():
        match = re.match(r'\s*\d+\)\s+([A-Fa-f0-9]{40})\s+"([^"]+)"\s*$', line)
        if match:
            candidates.append((match[1], match[2]))
    requested = os.environ.get('CLASSROOM_MAC_SIGN_IDENTITY')
    if requested:
        candidates = [v for v in candidates if requested in v]
    else:
        distribution = [v for v in candidates if v[1].startswith('Developer ID Application:')]
        candidates = distribution or [v for v in candidates if v[1].startswith('Apple Development:')]
    if len(candidates) != 1:
        raise RuntimeError('Select one valid signing identity with CLASSROOM_MAC_SIGN_IDENTITY; ad-hoc signing is not permitted')
    return candidates[0]


def notarize(app, profile):
    # Opt-in: no upload occurs unless the owner configures a Keychain profile.
    # Submit only the signed bundle, never credentials/private keys/source tree.
    with tempfile.TemporaryDirectory(prefix='classroom-notary-') as folder:
        archive = Path(folder) / 'app.zip'
        run('ditto', '-c', '-k', '--keepParent', str(app), str(archive), timeout=120)
        result = json.loads(run('xcrun', 'notarytool', 'submit', str(archive),
                               '--keychain-profile', profile, '--wait',
                               '--output-format', 'json', timeout=600))
        if result.get('status') != 'Accepted':
            raise RuntimeError('Notarization not accepted; publication stopped. '
                               f'Submission {result.get("id", "unknown")}: {result.get("status", "unknown")}')
        run('xcrun', 'stapler', 'staple', str(app), timeout=120)
        run('xcrun', 'stapler', 'validate', str(app), timeout=120)
        return {'submission_id': result.get('id'), 'status': 'Accepted'}


def signature(path, development_ad_hoc=False):
    detail = run('codesign', '--display', '--verbose=4', str(path))
    requirement = run('codesign', '--display', '-r-', str(path))
    if development_ad_hoc:
        if 'Signature=adhoc' not in detail:
            raise RuntimeError(f'{path}: expected explicitly selected ad-hoc development signing')
        return {'signing_mode': 'adhoc', 'designated_requirement': requirement.split('designated => ', 1)[1].strip()}
    if 'Signature=adhoc' in detail or 'cdhash ' in requirement:
        raise RuntimeError(f'{path}: hash-based identity would invalidate privacy grants on update')
    team = re.search(r'^TeamIdentifier=(.+)$', detail, re.M)
    if not team or team[1] == 'not set':
        raise RuntimeError(f'{path}: Apple team identity missing')
    return {'team_id': team[1], 'designated_requirement': requirement.split('designated => ', 1)[1].strip()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('app', type=Path, nargs='?')
    parser.add_argument('--helper', type=Path)
    parser.add_argument('--helper-entitlements', type=Path)
    parser.add_argument('--entitlements', type=Path)
    parser.add_argument('--identifier')
    parser.add_argument('--manifest', type=Path)
    parser.add_argument('--verify-only', action='store_true', help='Require integrity, certificate trust and Gatekeeper acceptance without signing')
    parser.add_argument('--check-identity', action='store_true', help='Check certificate trust before building')
    parser.add_argument('--development-ad-hoc', action='store_true', help='Owner-approved local development signing; privacy permissions may need reapproval on each update')
    args = parser.parse_args()
    if args.check_identity:
        verify_identity(*select_identity())
        print('Signing certificate trust verified')
        return
    if args.app is None:
        parser.error('app is required')
    if args.verify_only:
        print(verify_app(args.app))
        return
    if not args.identifier or not args.manifest:
        parser.error('--identifier and --manifest are required for signing')
    if args.development_ad_hoc:
        fingerprint, name = '-', 'Ad-hoc local development'
    else:
        fingerprint, name = select_identity()
        verify_identity(fingerprint, name)
    distribution = name.startswith('Developer ID Application:')
    notary_profile = None if args.development_ad_hoc else os.environ.get('CLASSROOM_NOTARY_PROFILE')
    if distribution and not notary_profile:
        raise RuntimeError('Developer ID publication requires CLASSROOM_NOTARY_PROFILE '
                           '(existing notarytool Keychain profile); no signing or upload performed')
    if not distribution and notary_profile:
        raise RuntimeError('Notarization requires Developer ID Application, not Apple Development')

    def sign(path, identifier=None, entitlements=None):
        command = ['codesign', '--force', '--sign', fingerprint]
        command += ['--timestamp', '--options', 'runtime'] if distribution else ['--timestamp=none']
        if identifier:
            command += ['--identifier', identifier]
        if entitlements:
            # AMFI rejects some CRLF XML that plutil accepts. A Windows checkout
            # must remain buildable on Mac; serialize the same entitlement values
            # as canonical XML, without modifying the source or adding privileges.
            with tempfile.TemporaryDirectory(prefix='classroom-entitlements-') as folder:
                normalized = Path(folder) / 'entitlements.plist'
                try:
                    normalized.write_bytes(plistlib.dumps(plistlib.loads(entitlements.read_bytes())))
                except (ValueError, plistlib.InvalidFileException) as error:
                    raise RuntimeError(f'Invalid entitlements: {entitlements}') from error
                run(*command, '--entitlements', str(normalized), str(path))
        else:
            run(*command, str(path))

    # Sign inside out. Keep framework identifiers from their own Info.plist.
    for path in sorted(args.app.rglob('*.dylib'), key=lambda p: len(p.parts), reverse=True):
        if not path.is_symlink():
            sign(path)
    for path in sorted(args.app.rglob('*.framework'), key=lambda p: len(p.parts), reverse=True):
        if not path.is_symlink():
            sign(path)
    components = {}
    if args.helper:
        sign(args.helper, args.identifier + '.agent', args.helper_entitlements)
        components['agent'] = signature(args.helper, args.development_ad_hoc)
    sign(args.app, args.identifier, args.entitlements)
    run('codesign', '--verify', '--deep', '--strict', str(args.app))
    components['app'] = signature(args.app, args.development_ad_hoc)
    if not args.development_ad_hoc and len({v['team_id'] for v in components.values()}) != 1:
        raise RuntimeError('App and agent must have the same signing team')
    manifest = {'certificate_sha1': fingerprint, 'identity': name, 'components': components}
    # A certificate renewal with a compatible default DR is fine. A different
    # team/DR must be a deliberate migration, not a silent signing fallback.
    if not args.development_ad_hoc and args.manifest.exists():
        previous = json.loads(args.manifest.read_text())
        if previous['components'] != components:
            raise RuntimeError('Signing requirements changed; privacy identity migration must be reviewed')
    if distribution:
        manifest['notarization'] = notarize(args.app, notary_profile)
    if not args.development_ad_hoc:
        verify_app(args.app, gatekeeper=distribution)
    args.manifest.write_text(json.dumps(manifest, indent=2))
    result = 'manual privacy approval required on updates' if args.development_ad_hoc else 'stable requirements verified'
    print(f'Signed {args.app.name}: {name}; {result}')



if __name__ == '__main__':
    try:
        main()
    except (RuntimeError, subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
        detail = error.output if isinstance(error, subprocess.CalledProcessError) else str(error)
        print('macOS signing stopped: ' + str(detail).strip(), file=sys.stderr)
        sys.exit(1)
