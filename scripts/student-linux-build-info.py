#!/usr/bin/env python3
"""Record the actual Student source snapshot, including dirty/non-Git copies."""
import argparse
import datetime
import hashlib
import json
from pathlib import Path
import re
import subprocess

SOURCE_ROOTS = ('Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml', '.cargo',
                'apps/classroom', 'crates', 'vendor', 'config', 'proto',
                'scripts/build-student-linux.sh', 'scripts/student-linux-build-info.py',
                'docs/28_UBUNTU_STUDENT.md')


def snapshot(root):
    entries = {}
    for name in SOURCE_ROOTS:
        path = root / name
        files = path.rglob('*') if path.is_dir() else [path]
        for source in files:
            relative = source.relative_to(root)
            if any(part in ('.git', 'target', '__pycache__') for part in relative.parts):
                continue
            if source.is_file():
                entries[relative.as_posix()] = hashlib.sha256(source.read_bytes()).hexdigest()
    encoded = json.dumps(entries, sort_keys=True, separators=(',', ':')).encode()
    return {'sha256': hashlib.sha256(encoded).hexdigest(), 'files': entries}


def git(root, *args):
    try:
        result = subprocess.run(['git', '-C', str(root), *args], text=True,
                                capture_output=True, check=False)
        return result.stdout.strip() if result.returncode == 0 else None
    except FileNotFoundError:
        return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument('--target', required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--verify', type=Path)
    args = parser.parse_args()
    source = snapshot(args.root)
    if args.verify:
        expected = json.loads(args.verify.read_text())['source_snapshot']['sha256']
        if source['sha256'] != expected:
            parser.error('Source changed during the build; rebuild from a stable source snapshot')
        return
    revision = git(args.root, 'rev-parse', 'HEAD')
    status = git(args.root, 'status', '--porcelain') if revision else None
    args.output.write_text(json.dumps({
        'component': 'Ubuntu Student development build',
        'target': args.target,
        'version': re.search(r'(?m)^version = "([^"]+)"',
                             (args.root / 'Cargo.toml').read_text()).group(1),
        'revision': revision,
        'working_tree_dirty': None if status is None else bool(status),
        'source_snapshot': source,
        'rustc': subprocess.run(['rustc', '+stable', '-V'], text=True,
                                capture_output=True, check=True).stdout.strip(),
        'built_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    }, indent=2) + '\n')


if __name__ == '__main__':
    main()
