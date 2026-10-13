#!/usr/bin/env python3
"""ELF manifest coverage; fixture headers are not runnable Linux builds."""
import argparse
import importlib.util
import json
from pathlib import Path
import struct
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location('dev_update', Path(__file__).with_name('classroom-dev-update.py'))
dev_update = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dev_update)

snapshot_spec = importlib.util.spec_from_file_location('build_info', Path(__file__).with_name('student-linux-build-info.py'))
build_info = importlib.util.module_from_spec(snapshot_spec)
snapshot_spec.loader.exec_module(build_info)


def elf(machine=62, kind=3):
    header = bytearray(64)
    header[:7] = b'\x7fELF\x02\x01\x01'
    struct.pack_into('<HHI', header, 16, kind, machine, 1)
    struct.pack_into('<H', header, 52, 64)
    return header


class LinuxPackageTests(unittest.TestCase):
    def test_architecture_and_manifest_follow_header_not_filename(self):
        for machine, arch in [(62, 'x86_64'), (183, 'aarch64')]:
            with self.subTest(arch=arch), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                source = root / 'ClassroomStudent'
                source.write_bytes(elf(machine))
                output = root / 'student.zip'
                dev_update.package(argparse.Namespace(source=source, output=output, version='test'))
                with zipfile.ZipFile(output) as archive:
                    manifest = json.loads(archive.read('manifest.json'))
                    self.assertEqual((manifest['os'], manifest['arch']), ('linux', arch))
                    self.assertEqual(archive.read('ClassroomStudent'), source.read_bytes())
                    self.assertEqual(manifest['sha256'], dev_update.sha256(source))

    def test_rejects_invalid_or_unsupported_elf(self):
        fixtures = [b'\x7fELF', elf(kind=1), elf(machine=40), elf()[:63]]
        wrong_class = elf()
        wrong_class[4] = 1
        fixtures.append(wrong_class)
        wrong_endian = elf()
        wrong_endian[5] = 2
        fixtures.append(wrong_endian)
        wrong_abi = elf()
        wrong_abi[7] = 9  # FreeBSD, not Linux.
        fixtures.append(wrong_abi)
        wrong_size = elf()
        struct.pack_into('<H', wrong_size, 52, 32)
        fixtures.append(wrong_size)
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / 'ClassroomStudent'
            for header in fixtures:
                with self.subTest(header=header[:20]):
                    source.write_bytes(header)
                    with self.assertRaises(ValueError):
                        dev_update.architecture(source, 'linux')

    def test_snapshot_tracks_dirty_source_without_git(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'Cargo.toml'
            source.write_text('[workspace]\n')
            before = build_info.snapshot(root)
            self.assertIsNone(build_info.git(root, 'rev-parse', 'HEAD'))
            self.assertIn('Cargo.toml', before['files'])
            source.write_text('[workspace]\nmembers = []\n')
            self.assertNotEqual(before['sha256'], build_info.snapshot(root)['sha256'])
            # Generated build outputs must not change the source fingerprint.
            (root / 'crates/test/target').mkdir(parents=True)
            (root / 'crates/test/target/generated').write_text('not source')
            after = build_info.snapshot(root)
            self.assertFalse(any('target/' in name for name in after['files']))

    def test_existing_macos_and_windows_architectures(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / 'fixture'
            source.write_bytes(struct.pack('<II', 0xFEEDFACF, 0x100000C))
            self.assertEqual(dev_update.architecture(source, 'macos'), 'aarch64')
            pe = bytearray(128)
            pe[:2] = b'MZ'
            struct.pack_into('<I', pe, 0x3C, 64)
            pe[64:68] = b'PE\0\0'
            struct.pack_into('<H', pe, 68, 0x8664)
            source.write_bytes(pe)
            self.assertEqual(dev_update.architecture(source, 'windows'), 'x86_64')


if __name__ == '__main__':
    unittest.main()
