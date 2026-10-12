#!/usr/bin/env python3
"""Regression: one-shot screenshots coexist with live Windows thumbnails.

Requires an idle Teacher and real connected Windows Students. Does not launch,
stop, or replace any Student application. Retains screenshots locally as evidence.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import struct
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--endpoint', type=Path, required=True)
    parser.add_argument('--student', action='append', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location(
        'classroom_update', Path(__file__).with_name('classroom-dev-update.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    client = module.IPC(args.endpoint)
    results = []
    try:
        assert client.state['phase'] == 'idle', 'Stop presentation before this regression'
        assert not client.state.get('observe'), 'Close Observe before this regression'
        for requested in args.student:
            matches = [r for r in client.state['students']
                       if r['online'] and requested in (r['id'], r['ip'])]
            assert len(matches) == 1, f'Expected one online Student: {requested}'
            row = matches[0]
            assert client.state['tools']['capabilities'][row['id']]['os'] == 'windows'
            assert row.get('monitoring') == 'ready', 'Thumbnail capture must be active'
            assert row.get('thumbnail_age_ms', 100000) < 6000, 'Require fresh thumbnail'
            dest = (args.output / row['ip']).resolve()
            dest.mkdir(parents=True, exist_ok=True)
            ack = client.command('student_screenshot', [row['id']], {'dest': str(dest)})
            deadline = time.monotonic() + 30
            while True:
                job = next((j for j in client.state['tools']['fs_jobs']
                            if j['id'] == ack['job_id']), None)
                if job and all(t['status'] in ('complete', 'error', 'cancelled')
                               for t in job['targets']):
                    assert all(t['status'] == 'complete' for t in job['targets']), job
                    break
                assert time.monotonic() < deadline, 'Screenshot deadline exceeded'
                client.read()
            png = dest / 'Screenshot.png'
            with png.open('rb') as stream:
                header = stream.read(24)
            assert header[:8] == b'\x89PNG\r\n\x1a\n' and header[12:16] == b'IHDR'
            width, height = struct.unpack('!II', header[16:24])
            assert 0 < width <= 1920 and 0 < height <= 1080
            # Let monitoring produce a new frame after the child has exited.
            until = time.monotonic() + 3
            while time.monotonic() < until:
                client.read()
            latest = next(r for r in client.state['students'] if r['id'] == row['id'])
            assert latest['online'] and latest['monitoring'] == 'ready'
            assert latest.get('thumbnail_age_ms', 100000) < 6000
            results.append({'ip': row['ip'], 'width': width, 'height': height, 'pass': True})
            print(json.dumps(results[-1]), flush=True)
        args.output.mkdir(parents=True, exist_ok=True)
        (args.output / 'results.json').write_text(json.dumps(results, indent=2))
    finally:
        client.socket.close()


if __name__ == '__main__':
    main()
