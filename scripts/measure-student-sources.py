#!/usr/bin/env python3
"""Matched native seven-phase workload on already deployed Windows/Mac Students.

No Classroom build, deployment, privacy changes or display mode changes. The
owned workload tasks/jobs terminate after a bounded duration and are removed.
"""
import argparse
import getpass
import hashlib
import ipaddress
import importlib.util
import json
import plistlib
import re
import shlex
import subprocess
import threading
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class LiveIPC:
    """Drain state updates during blocking SSH setup to avoid stale socket backlog."""
    def __init__(self, client):
        self.client = client
        self.changed = threading.Condition()
        self.acks = {}
        self.failure = None
        self.counter = 0
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()

    @property
    def state(self):
        return self.client.state

    def _read(self):
        try:
            while True:
                value = self.client.read()
                with self.changed:
                    if value.get('type') == 'ack':
                        self.acks[value['id']] = value
                    self.changed.notify_all()
        except Exception as error:
            with self.changed:
                self.failure = error
                self.changed.notify_all()

    def read(self):
        with self.changed:
            if self.failure:
                raise self.failure
            self.changed.wait(.2)
        return self.state

    def command(self, action, selected=()):
        self.counter += 1
        serial = self.counter
        self.client.send({'id': serial, 'action': action, 'selected': list(selected), 'input': {}})
        end = time.monotonic() + 20
        with self.changed:
            while serial not in self.acks:
                if self.failure:
                    raise self.failure
                remaining = end - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError('Teacher command acknowledgement')
                self.changed.wait(remaining)
            reply = self.acks.pop(serial)
        if not reply.get('ok'):
            raise RuntimeError(reply.get('error') or 'Teacher rejected command')
        return reply

    def close(self):
        self.client.socket.close()


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--seconds', type=int, default=140)
    parser.add_argument('--windows', default='mvmanh@192.168.100.102')
    parser.add_argument('--windows-student-root', default='C:/Users/mvmanh/Classroom-Source-Switch-1791518542')
    args = parser.parse_args()
    assert 70 <= args.seconds <= 280 and args.seconds % 70 == 0
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    rem = module('remote', ROOT / 'scripts/run-four-machine-test.py')
    rem.OPTS += ['-o', 'StrictHostKeyChecking=accept-new']
    windows_user, windows_ip = args.windows.split('@')
    ipaddress.IPv4Address(windows_ip)
    assert re.fullmatch(r'[A-Za-z0-9_-]+', windows_user)
    windows = rem.Remote(args.windows, getpass.getpass('Windows SSH password: '))
    label = 'Classroom-Source-Measure-' + str(int(time.time()))
    win_root = f'C:/Users/{windows_user}/' + label
    mac_root = '/Users/lolem/' + label
    job = 'com.classroom.test.source-measure.' + str(int(time.time()))
    mac_args = ['-i', str(Path.home() / '.ssh/classroom_two_mac'), '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=8']

    def mac(command):
        return subprocess.check_output(['ssh', *mac_args, 'lolem@192.168.100.254', command], text=True, timeout=45)

    def mac_copy(source, target):
        subprocess.run(['scp', *mac_args, str(source), str(target)], check=True, timeout=45)

    ipc = LiveIPC(module('ipc', ROOT / 'scripts/classroom-dev-update.py').IPC(ROOT / 'target/source-switch-endpoint.json'))
    samples, events, cleanup = [], [], []
    win_task = mac_job = False
    owns_presentation = False

    def compact(state):
        observe = state.get('observe') or {}
        return {'unix_ms': int(time.time() * 1000), **{k: state.get(k) for k in ['phase', 'error', 'source_id', 'stream_id']},
                'observe': {k: observe.get(k) for k in ['id', 'phase', 'mode', 'class_control', 'error', 'metrics']},
                'students': [{k: row.get(k) for k in ['id', 'ip', 'online', 'media']} for row in state['students']]}

    def wait(predicate, seconds):
        deadline = time.monotonic() + seconds
        while True:
            state = ipc.state
            samples.append(compact(state))
            if state.get('error'):
                raise RuntimeError(state['error'])
            if predicate(state):
                return state
            if time.monotonic() >= deadline:
                raise TimeoutError('Source measurement state deadline')
            ipc.read()

    try:
        assert ipc.state['phase'] == 'idle', 'Stop sharing before measurement'
        ids = {r['ip']: r['id'] for r in ipc.state['students'] if r['online']}
        assert all(ip in ids for ip in [windows_ip, '192.168.100.254'])
        assert len(ids) == 2, 'Close extra Students for an isolated one-recipient comparison'
        windows.ps(f"$ErrorActionPreference='Stop'; if(Test-Path '{win_root}'){{throw 'Workload folder exists'}}; New-Item -ItemType Directory '{win_root}' | Out-Null")
        mac('mkdir ' + shlex.quote(mac_root))
        workload = ROOT / 'scripts/source-motion-workload.ps1'
        windows.copy(workload, args.windows + ':' + win_root + '/workload.ps1')
        expected = hashlib.sha256(workload.read_bytes()).hexdigest()
        assert expected in windows.ps(f"(Get-FileHash '{win_root}/workload.ps1' -Algorithm SHA256).Hash").lower()
        mac_copy(ROOT / 'target/source-motion-workload', 'lolem@192.168.100.254:' + mac_root + '/workload')
        mac('chmod +x ' + shlex.quote(mac_root + '/workload'))
        meta = {'label': label, 'seconds': args.seconds, 'phase_seconds': 10,
                'phases': ['static', 'typing', 'scroll', 'terminal', 'motion', 'move_window', 'switch_windows'],
                'scope': 'Mixed native Mac/Parallels LAN; unchanged apps, actual display modes, not physical 1080p/40-PC acceptance',
                'mac_workload_sha256': hashlib.sha256((ROOT / 'target/source-motion-workload').read_bytes()).hexdigest(),
                'windows_workload_sha256': expected, 'windows': args.windows,
                'windows_student_root': args.windows_student_root}
        clocks = {}
        for name, get_clock in [('windows', lambda: windows.ps('[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()')),
                                ('mac', lambda: mac("python3 -c 'import time;print(int(time.time()*1000))'"))]:
            before = time.time() * 1000
            result = get_clock()
            after = time.time() * 1000
            stamp = int(re.findall(r'\b(1\d{12})\b', result)[0])
            clocks[name] = {'offset_min_ms': stamp - after, 'offset_max_ms': stamp - before,
                            'round_trip_ms': after - before}
        meta['clock_bounds'] = clocks
        (output / 'manifest.json').write_text(json.dumps(meta, indent=2))
        for name, ip in [('windows', windows_ip), ('mac', '192.168.100.254')]:
            target = ids[ip]
            began = int(time.time() * 1000)
            owns_presentation = True
            ipc.command('switch_source', [target])
            wait(lambda s: s['phase'] == 'sharing' and s['source_id'] == target and
                 (s.get('observe') or {}).get('mode') == 'control' and
                 sum(bool(r['online']) for r in s['students']) == 2 and
                 all(r['media']['state'] == 'receiving' and (r['media'].get('render_fps') or 0) > 0
                     for r in s['students'] if r['online'] and r['id'] != target), 40)
            print('SOURCE READY', name, flush=True)
            if name == 'windows':
                # Execute our hash-verified native workload through the already
                # authorized inline command path; do not change ExecutionPolicy.
                command = (f"$ErrorActionPreference='Stop'; if((Get-FileHash '{win_root}/workload.ps1' -Algorithm SHA256).Hash -ne '{expected}'){{throw 'Workload hash mismatch'}}; "
                           f"& ([ScriptBlock]::Create((Get-Content -Raw -Encoding UTF8 '{win_root}/workload.ps1'))) -Seconds {args.seconds} -EvidencePath '{win_root}/workload.jsonl'")
                command = command.replace('"', '\\"')
                task_args = f'-NoLogo -NoProfile -WindowStyle Hidden -Command "{command}"'
                task_args = task_args.replace("'", "''")
                win_task = True
                windows.ps(f"$ErrorActionPreference='Stop'; $a=New-ScheduledTaskAction -Execute 'powershell.exe' -Argument '{task_args}'; $p=New-ScheduledTaskPrincipal -UserId '{windows_user}' -LogonType Interactive -RunLevel Limited; $s=New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries; Register-ScheduledTask -TaskName '{label}' -Action $a -Principal $p -Settings $s -Force | Out-Null; Start-ScheduledTask '{label}'; $end=(Get-Date).AddSeconds(20); while(-not(Test-Path '{win_root}/workload.jsonl')){{if((Get-Date) -gt $end){{Get-ScheduledTaskInfo '{label}' | ConvertTo-Json; throw 'Windows workload did not start'}}; Start-Sleep -Milliseconds 200}}")
            else:
                launch = {'Label': job, 'ProgramArguments': [mac_root + '/workload', str(args.seconds), mac_root + '/audit.json'],
                          'ProcessType': 'Interactive', 'LegacyTimers': True, 'RunAtLoad': True,
                          'StandardOutPath': mac_root + '/stdout.log', 'StandardErrorPath': mac_root + '/stderr.log'}
                path = output / 'workload.plist'; path.write_bytes(plistlib.dumps(launch))
                mac_copy(path, 'lolem@192.168.100.254:' + mac_root + '/workload.plist')
                mac_job = True
                mac('launchctl bootstrap gui/501 ' + shlex.quote(mac_root + '/workload.plist'))
            workload_started = int(time.time() * 1000)
            print('MEASURING', name, args.seconds, 'seconds', flush=True)
            end = time.monotonic() + args.seconds + 3
            next_update = time.monotonic() + 30
            while time.monotonic() < end:
                ipc.read(); samples.append({'workload': name, **compact(ipc.state)})
                if ipc.state.get('error') or not all(r['online'] for r in ipc.state['students'] if r['ip'] in [windows_ip, '192.168.100.254']):
                    raise RuntimeError('Disconnect/error during ' + name)
                if time.monotonic() >= next_update:
                    print('PROGRESS', name, round(args.seconds + 3 - (end - time.monotonic())), 's', flush=True)
                    next_update += 30
            events.append({'source': name, 'source_started_ms': began, 'workload_start_observed_ms': workload_started,
                           'ended_ms': int(time.time() * 1000)})
            ipc.command('stop'); wait(lambda s: s['phase'] == 'idle', 20)
            owns_presentation = False
            if name == 'windows':
                windows.ps(f"$ErrorActionPreference='Stop'; $i=Get-ScheduledTaskInfo '{label}'; if((Get-ScheduledTask '{label}').State -eq 'Running' -or $i.LastTaskResult -ne 0){{throw ('Workload incomplete: '+$i.LastTaskResult)}}; Unregister-ScheduledTask '{label}' -Confirm:$false")
                win_task = False
                windows.copy(args.windows + ':' + win_root + '/workload.jsonl', output / 'windows-workload.jsonl')
            else:
                mac_copy('lolem@192.168.100.254:' + mac_root + '/audit.json', output / 'mac-workload.json')
                mac('launchctl bootout gui/501/' + job)
                mac_job = False
            print('DONE', name, flush=True)
    finally:
        if owns_presentation:
            try:
                ipc.command('stop')
            except Exception as error:
                cleanup.append('Teacher stop: ' + str(error))
        ipc.close()
        if win_task:
            try:
                windows.ps(f"Stop-ScheduledTask '{label}' -ErrorAction SilentlyContinue; Unregister-ScheduledTask '{label}' -Confirm:$false")
            except Exception as error:
                cleanup.append('Windows workload: ' + str(error))
        if mac_job:
            try:
                mac('launchctl bootout gui/501/' + job)
            except Exception as error:
                cleanup.append('Mac workload: ' + str(error))
        (output / 'events.json').write_text(json.dumps(events, indent=2))
        (output / 'states.json').write_text(json.dumps(samples, indent=2))
        (output / 'cleanup.json').write_text(json.dumps(cleanup, indent=2))
        for action in [lambda: windows.copy(args.windows + ':' + args.windows_student_root + '/student.log', output / 'windows-student.log'),
                       lambda: mac_copy('lolem@192.168.100.254:/Users/lolem/Classroom-source-switch/student.log', output / 'mac-student.log')]:
            try:
                action()
            except Exception as error:
                print('LOG COLLECTION ERROR', error, flush=True)
        source = ROOT / 'target/manual-source-review-20261009/teacher-fixed.log'
        if source.exists():
            (output / 'teacher.log').write_bytes(source.read_bytes())
    assert not cleanup, cleanup
    print('COMPLETE; applications retained idle', flush=True)


if __name__ == '__main__':
    main()
