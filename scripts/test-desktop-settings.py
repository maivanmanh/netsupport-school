#!/usr/bin/env python3
"""Isolated native Teacher -> saved Student restart, with exact PID cleanup."""
import argparse
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--local-ip', default='192.168.100.219')
    args = parser.parse_args()
    run = ROOT / 'target/phase2-settings' / time.strftime('%Y%m%d-%H%M%S')
    run.mkdir(parents=True)
    child_pid = None
    parent = None
    try:
        with (run / 'stdout.log').open('w') as out:
            parent = subprocess.Popen([
                str(ROOT / 'target/release/Classroom'), '--role', 'teacher',
                '--teacher-ui', '--bind-ip', args.local_ip, '--control-port', '48500',
                '--launch-profile', str(run / 'profile.toml'),
                '--teacher-test-settings-restart', '--log-file', str(run / 'teacher.log'),
            ], stdout=out, stderr=subprocess.STDOUT)
        code = parent.wait(timeout=25)
        text = (run / 'teacher.log').read_text()
        match = re.search(r'replacement_pid=(\d+)', text)
        if match:
            child_pid = int(match[1])
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            output = (run / 'stdout.log').read_text()
            if 'student background agent ready' in output and 'native Classroom tray ready' in output:
                break
            time.sleep(.1)
        profile = (run / 'profile.toml').read_text()
        checks = {
            'teacher_exits_cleanly': code == 0,
            'replacement_pid_logged': child_pid is not None,
            'student_settings_saved': 'role = "student"' in profile and args.local_ip in profile,
            'student_background_ready': 'student background agent ready' in output,
            'student_tray_ready': 'teacher=false' in output and 'native Classroom tray ready' in output,
            'no_preview_without_presentation': 'student forced presentation window ready' not in output,
            'no_process_errors': ' ERROR ' not in output,
        }
        result = {'passed': all(checks.values()), 'checks': checks, 'replacement_pid': child_pid}
        (run / 'analysis.json').write_text(json.dumps(result, indent=2))
        print(str(run)); print(json.dumps(result, indent=2))
        if not result['passed']:
            raise RuntimeError('Settings restart assertions failed')
    finally:
        if parent and parent.poll() is None:
            parent.terminate(); parent.wait(timeout=10)
        if child_pid:
            try:
                os.kill(child_pid, signal.SIGTERM)
            except ProcessLookupError:
                pass


if __name__ == '__main__':
    main()
