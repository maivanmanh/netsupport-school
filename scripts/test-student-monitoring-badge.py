#!/usr/bin/env python3
"""Exercise the real Teacher/tools TCP/native UI acknowledgement path.
Restores initial preferences. Does not change privacy settings or capture screens.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import time

spec = importlib.util.spec_from_file_location('updater', Path(__file__).with_name('classroom-dev-update.py'))
updater = importlib.util.module_from_spec(spec)
spec.loader.exec_module(updater)

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--endpoint', type=Path, required=True)
    parser.add_argument('--student', action='append', required=True, help='Student IP')
    args = parser.parse_args()
    client = updater.IPC(args.endpoint)
    deadline = time.monotonic() + 20
    while True:
        rows = [r for r in client.state.get('students', []) if r['ip'] in args.student and r['online']]
        caps = client.state.get('tools', {}).get('capabilities', {})
        if len(rows) == len(set(args.student)) and all(caps.get(r['id'], {}).get('monitoring_badge') == 1 for r in rows):
            break
        if time.monotonic() >= deadline:
            raise RuntimeError('Requested Students must be online with badge support')
        client.read()
    ids = [r['id'] for r in rows]
    initial = {i: bool(client.state.get('tools', {}).get('monitoring_badges', {}).get(i, {}).get('enabled')) for i in ids}
    def apply(selected, enabled):
        client.command('monitoring_badge', selected, {'enabled': enabled})
    def wait(expected):
        deadline = time.monotonic() + 15
        while True:
            badges = client.state.get('tools', {}).get('monitoring_badges', {})
            if all(badges.get(i, {}).get('status') == 'applied' and badges[i].get('actual') == enabled and badges[i].get('enabled') == enabled for i, enabled in expected.items()):
                return
            if time.monotonic() >= deadline:
                raise RuntimeError('Badge confirmation failed: ' + json.dumps({i: badges.get(i) for i in ids}))
            client.read()
    try:
        apply(ids, True); wait(dict.fromkeys(ids, True)); print('PASS bulk enable/native display ACK', flush=True)
        apply(ids[:1], False); wait({i: n != 0 for n, i in enumerate(ids)}); print('PASS individual off preserves other computers', flush=True)
        apply(ids, True); apply(ids, False); wait(dict.fromkeys(ids, False)); print('PASS rapid latest preference wins', flush=True)
        for _ in range(5):
            apply(ids, True); wait(dict.fromkeys(ids, True))
            apply(ids, False); wait(dict.fromkeys(ids, False))
        print('PASS repeated native create/destroy on all targets', flush=True)
    finally:
        for enabled in (False, True):
            selected = [i for i, old in initial.items() if old == enabled]
            if selected:
                apply(selected, enabled)
        wait(initial)
        client.socket.close()
        print('Initial preferences restored', flush=True)

if __name__ == '__main__':
    main()
