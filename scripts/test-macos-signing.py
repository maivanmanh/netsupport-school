"""Safety gates: untrusted signing must not mutate, publish or launch artifacts."""
import importlib.util
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('signer', Path(__file__).with_name('sign-macos-app.py'))
signer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(signer)


class SigningGateTests(unittest.TestCase):
    def test_revoked_identity_stops_before_any_codesign(self):
        with patch.object(sys, 'argv', ['signer', 'app', '--identifier', 'com.classroom.student', '--manifest', 'manifest']), \
             patch.object(signer, 'select_identity', return_value=('fingerprint', 'identity')), \
             patch.object(signer, 'verify_identity', side_effect=RuntimeError('revoked')), \
             patch.object(signer, 'run') as run:
            with self.assertRaisesRegex(RuntimeError, 'revoked'):
                signer.main()
            run.assert_not_called()

    def test_integrity_success_cannot_override_revocation(self):
        def run(*args):
            if '--extract-certificates=' in ' '.join(args):
                prefix = next(a.split('=', 1)[1] for a in args if a.startswith('--extract-certificates='))
                Path(prefix + '0').write_bytes(b'public certificate')
            if args[0] == 'spctl':
                self.fail('Revoked certificate reached Gatekeeper assessment')
            return ''
        with patch.object(signer, 'run', side_effect=run), \
             patch.object(signer, 'signature', return_value={}), \
             patch.object(signer, 'verify_certificate', side_effect=RuntimeError('revoked')):
            with self.assertRaisesRegex(RuntimeError, 'revoked'):
                signer.verify_app(Path('app'))

    def test_gatekeeper_rejection_stops_even_with_trusted_certificate(self):
        def run(*args):
            if '--extract-certificates=' in ' '.join(args):
                prefix = next(a.split('=', 1)[1] for a in args if a.startswith('--extract-certificates='))
                Path(prefix + '0').write_bytes(b'public certificate')
            if args[0] == 'spctl':
                raise subprocess.CalledProcessError(1, args, output='rejected')
            return ''
        with patch.object(signer, 'run', side_effect=run), \
             patch.object(signer, 'signature', return_value={}), \
             patch.object(signer, 'verify_certificate'):
            with self.assertRaisesRegex(RuntimeError, 'Gatekeeper rejected'):
                signer.verify_app(Path('app'))

    def test_revocation_timeout_fails_closed(self):
        with patch.object(signer, 'run', side_effect=subprocess.TimeoutExpired('security', 30)):
            with self.assertRaisesRegex(RuntimeError, 'timed out'):
                signer.verify_certificate(Path('certificate'))

    def test_rejected_notarization_never_staples(self):
        def run(*args, **kwargs):
            if args[:3] == ('xcrun', 'notarytool', 'submit'):
                return '{"id":"rejected-submission", "status":"Invalid"}'
            if args[:2] == ('xcrun', 'stapler'):
                self.fail('Rejected app reached stapling')
            return ''
        with patch.object(signer, 'run', side_effect=run):
            with self.assertRaisesRegex(RuntimeError, 'Notarization not accepted'):
                signer.notarize(Path('app'), 'test-profile')

    def test_accepted_notarization_still_requires_valid_ticket(self):
        def run(*args, **kwargs):
            if args[:3] == ('xcrun', 'notarytool', 'submit'):
                return '{"id":"accepted-submission", "status":"Accepted"}'
            if args[:3] == ('xcrun', 'stapler', 'validate'):
                raise subprocess.CalledProcessError(1, args, output='Ticket invalid')
            return ''
        with patch.object(signer, 'run', side_effect=run):
            with self.assertRaises(subprocess.CalledProcessError):
                signer.notarize(Path('app'), 'test-profile')

    def test_distribution_without_notary_profile_stops_before_signing(self):
        with patch.object(sys, 'argv', ['signer', 'app', '--identifier', 'com.classroom.student', '--manifest', 'manifest']), \
             patch.dict(signer.os.environ, {}, clear=True), \
             patch.object(signer, 'select_identity', return_value=('fingerprint', 'Developer ID Application: test')), \
             patch.object(signer, 'verify_identity'), \
             patch.object(signer, 'run') as run:
            with self.assertRaisesRegex(RuntimeError, 'CLASSROOM_NOTARY_PROFILE'):
                signer.main()
            run.assert_not_called()


if __name__ == '__main__':
    unittest.main()
