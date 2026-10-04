#!/usr/bin/env python3
"""Bounded Windows Teacher/LAN validation controller.

Run on Windows with Python + paramiko. SSH passwords stay in memory. The owner
supplies two development endpoints; their host fingerprints are recorded, without
changing OpenSSH configuration. Commands arrive in the printed run directory's
command.json; completed results and full logs are retained there. Only processes,
interactive tasks and LaunchAgents created by this controller are stopped.

This measures a Windows VM and simulated OS endpoints, never physical-PC or
optical acceptance. It does not change firewall, privacy grants or user settings.
"""
import argparse
import base64
import ctypes
from ctypes import wintypes
import getpass
import hashlib
import json
import os
from pathlib import Path
import shlex
import socket
import statistics
import struct
import subprocess
import threading
import time
import traceback
import xml.sax.saxutils

import paramiko

ROOT = Path(__file__).resolve().parents[1]
CREATE_NO_WINDOW = 0x08000000


def atomic_json(path, value):
    temp = path.with_suffix('.next.json')
    temp.write_text(json.dumps(value, indent=2, ensure_ascii=False), encoding='utf-8')
    for _ in range(10):
        try:
            os.replace(temp, path)
            return
        except PermissionError:
            time.sleep(0.02)
    raise PermissionError(f'Cannot publish {path.name}')


def ps(source, timeout=60):
    encoded = base64.b64encode(source.encode('utf-16-le')).decode()
    result = subprocess.run(['powershell.exe', '-NoLogo', '-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
                             '-EncodedCommand', encoded], capture_output=True,
                            creationflags=CREATE_NO_WINDOW, timeout=timeout)
    text = result.stdout.decode('utf-8-sig', errors='replace')
    if result.returncode:
        raise RuntimeError(result.stderr.decode('utf-8', errors='replace'))
    return text.strip()


class Remote:
    def __init__(self, address, password, windows=False):
        self.address, self.password, self.windows = address, password, windows
        self.connection = None
        self.fingerprint = None

    def connect(self):
        if self.connection and self.connection.get_transport().is_active():
            return self.connection
        value = paramiko.SSHClient()
        value.set_missing_host_key_policy(paramiko.AutoAddPolicy())
        user, host = self.address.split('@', 1)
        value.connect(host, username=user, password=self.password, look_for_keys=False,
                      allow_agent=False, timeout=10, auth_timeout=15, banner_timeout=15)
        transport = value.get_transport()
        transport.set_keepalive(15)
        self.fingerprint = 'SHA256:' + base64.b64encode(
            hashlib.sha256(transport.get_remote_server_key().asbytes()).digest()).decode().rstrip('=')
        self.connection = value
        return value

    def run(self, source, timeout=90):
        if self.windows:
            source = "$ProgressPreference='SilentlyContinue'; [Console]::OutputEncoding=[Text.UTF8Encoding]::new(); " + source
            encoded = base64.b64encode(source.encode('utf-16-le')).decode()
            source = 'powershell.exe -NoLogo -NoProfile -NonInteractive -EncodedCommand ' + encoded
        stdin, stdout, stderr = self.connect().exec_command(source, timeout=timeout)
        stdin.close()
        output = stdout.read().decode('utf-8-sig', errors='replace')
        error = stderr.read().decode('utf-8', errors='replace')
        code = stdout.channel.recv_exit_status()
        if code:
            raise RuntimeError(f'{self.address} exit {code}: {error or output}')
        return output.strip()

    def put(self, local, remote):
        with self.connect().open_sftp() as sftp:
            sftp.put(str(local), remote)

    def get(self, remote, local):
        with self.connect().open_sftp() as sftp:
            sftp.get(remote, str(local))

    def close(self):
        if self.connection:
            self.connection.close()


class Ipc:
    def __init__(self, endpoint):
        info = json.loads(endpoint.read_text(encoding='utf-8-sig'))
        self.agent_pid = info['pid']
        self.socket = socket.create_connection(('127.0.0.1', info['port']), timeout=10)
        self.socket.settimeout(None)
        self.serial = 1000
        self.state = {}
        self.pending = {}
        self.lock = threading.Lock()
        self.error = None
        self.send({'type': 'hello', 'protocol': 1, 'token': info['token'], 'ui_role': 'observe'})
        self.thread = threading.Thread(target=self.reader, daemon=True)
        self.thread.start()

    def send(self, value):
        data = json.dumps(value).encode()
        self.socket.sendall(struct.pack('!I', len(data)) + data)

    def reader(self):
        def exact(n):
            result = bytearray()
            while len(result) < n:
                block = self.socket.recv(n - len(result))
                if not block:
                    raise EOFError('Teacher IPC closed')
                result.extend(block)
            return bytes(result)
        try:
            while True:
                n = struct.unpack('!I', exact(4))[0]
                if not 0 < n <= 8 * 1024 * 1024:
                    raise ValueError('Invalid IPC frame')
                value = json.loads(exact(n))
                if value.get('type') == 'state':
                    for row in value.get('students', []):
                        row.pop('thumbnail', None)
                    self.state = value
                else:
                    item = self.pending.get(value.get('id'))
                    if item:
                        item[1].append(value)
                        item[0].set()
        except Exception as exc:
            self.error = str(exc)

    def command(self, action, selected=(), value=None, timeout=20):
        with self.lock:
            self.serial += 1
            serial = self.serial
            event, values = threading.Event(), []
            self.pending[serial] = (event, values)
            started = time.monotonic()
            self.send({'id': serial, 'action': action, 'selected': list(selected), 'input': value or {}})
            if not event.wait(timeout):
                self.pending.pop(serial, None)
                raise TimeoutError(f'IPC {action}: {self.error or "no reply"}')
            self.pending.pop(serial, None)
            reply = values[0]
            if not reply.get('ok'):
                raise RuntimeError(f'IPC {action}: {reply.get("error")}')
            return {'action': action, 'round_trip_ms': round((time.monotonic() - started) * 1000, 3),
                    'reply': reply}

    def close(self):
        self.socket.close()


def summary(values):
    values = sorted(v for v in values if v is not None)
    if not values:
        return None
    return {'n': len(values), 'p50': values[int((len(values)-1)*.5)],
            'p95': values[int((len(values)-1)*.95)], 'max': values[-1], 'min': values[0]}


def fields(line):
    import re
    result = {}
    for key, value in re.findall(r'\b([a-z][a-z0-9_]*)=(Some\([^)]*\)|[^ ]+)', line):
        if value.startswith('Some('):
            value = value[5:-1]
        try:
            result[key] = float(value)
        except ValueError:
            result[key] = value
    return result


def log_report(path, role):
    lines = path.read_text(encoding='utf-8-sig', errors='replace').splitlines()
    intervals = [fields(line) for line in lines if f'{role} M3 media interval' in line]
    finals = [fields(line) for line in lines if f'{role} M3 media complete' in line]
    recovery = [fields(line) for line in lines if 'student freshness recovery interval' in line]
    ingress = [fields(line) for line in lines if 'student UDP ingest interval' in line]
    keyframes = [fields(line) for line in lines if 'teacher keyframe interval' in line]
    stages = {}
    for line in lines:
        if 'media stage interval' in line or 'media stage complete' in line:
            row = fields(line)
            stages.setdefault(row.get('stage', '?'), []).append(row)
    keys = ('capture_fps', 'encode_fps', 'encode_latency_p95_ms', 'send_mbps', 'encoded_stream_mbps',
            'connected_students', 'encoder_instances', 'fanout_handoff_wait_interval_ms',
            'process_cpu_percent', 'process_working_set_bytes', 'decode_fps', 'render_fps',
            'decode_latency_interval_p95_ms', 'render_latency_interval_p95_ms',
            'packet_interarrival_interval_max_ms', 'access_unit_gap_interval_max_ms',
            'control_rtt_ms', 'pending_render_frames')
    return {'interval_count': len(intervals),
            'metrics': {k: summary([r.get(k) for r in intervals[2:] if isinstance(r.get(k), (int, float))])
                        for k in keys},
            'final': finals[-1] if finals else None,
            'sessions': finals,
            'ingress_last': ingress[-1] if ingress else None,
            'keyframe_totals': {k: sum(r.get(k, 0) for r in keyframes)
                                for k in ('keyframe_requests', 'keyframes_forced', 'keyframes_emitted')},
            'recovery_last': recovery[-1] if recovery else None,
            'stages_p95_us': {key: summary([r.get('p95_us') for r in rows]) for key, rows in stages.items()}}


class MemoryCounters(ctypes.Structure):
    _fields_ = [('cb', wintypes.DWORD), ('PageFaultCount', wintypes.DWORD),
                ('PeakWorkingSetSize', ctypes.c_size_t), ('WorkingSetSize', ctypes.c_size_t),
                ('QuotaPeakPagedPoolUsage', ctypes.c_size_t), ('QuotaPagedPoolUsage', ctypes.c_size_t),
                ('QuotaPeakNonPagedPoolUsage', ctypes.c_size_t), ('QuotaNonPagedPoolUsage', ctypes.c_size_t),
                ('PagefileUsage', ctypes.c_size_t), ('PeakPagefileUsage', ctypes.c_size_t)]


def process_sample(pid):
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
    kernel.OpenProcess.restype = wintypes.HANDLE
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    handle = kernel.OpenProcess(0x410, False, pid)
    if not handle:
        return {'pid': pid, 'alive': False}
    try:
        code = wintypes.DWORD()
        kernel.GetExitCodeProcess.argtypes = [wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD)]
        if not kernel.GetExitCodeProcess(handle, ctypes.byref(code)) or code.value != 259:
            return {'pid': pid, 'alive': False}
        memory = MemoryCounters()
        memory.cb = ctypes.sizeof(memory)
        api = ctypes.WinDLL('psapi')
        api.GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.POINTER(MemoryCounters), wintypes.DWORD]
        api.GetProcessMemoryInfo(handle, ctypes.byref(memory), memory.cb)
        handles = wintypes.DWORD()
        kernel.GetProcessHandleCount.argtypes = [wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD)]
        kernel.GetProcessHandleCount(handle, ctypes.byref(handles))
        return {'pid': pid, 'alive': True, 'working_set_bytes': memory.WorkingSetSize,
                'handles': handles.value}
    finally:
        kernel.CloseHandle(handle)


class Controller:
    def __init__(self, args, windows, mac):
        self.args, self.remotes = args, {'windows': windows, 'mac': mac}
        self.root = ROOT / 'target/windows-teacher-validation' / time.strftime('%Y%m%d-%H%M%S')
        self.root.mkdir(parents=True)
        self.bundle = (ROOT / args.bundle).resolve()
        self.agent = self.bundle / 'ClassroomAgent.exe'
        self.console = self.bundle / 'classroom_console.exe'
        self.mac_app = args.mac_app
        self.mac_bin = self.mac_app + '/Contents/MacOS/ClassroomStudent'
        self.remote_win_root = 'C:/Users/mvmanh/Classroom-teacher-validation'
        self.remote_mac_root = '/Users/mvmanh/Classroom-teacher-validation'
        self.task = 'Classroom-TeacherValidation-' + self.root.name
        self.job = 'com.classroom.teacher-validation.' + self.root.name
        self.active = None
        self.results = []
        self.start_time = time.time()
        self.cache_path = Path(os.environ['APPDATA']) / 'com.classroom/classroom_console/known-students.json'
        self.original_cache = self.cache_path.read_bytes() if self.cache_path.exists() else None

    def status(self, **values):
        state = {'time': time.time(), 'root': str(self.root), 'active': self.active.get('label') if self.active else None,
                 'results': self.results, **values}
        if self.active:
            state['teacher'] = self.active['ipc'].state
        atomic_json(self.root / 'status.json', state)

    def inventory(self):
        win = self.remotes['windows'].run("$ErrorActionPreference='Stop'; @{name=$env:COMPUTERNAME; os=(Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version,OSArchitecture,TotalVisibleMemorySize); cpu=@(Get-CimInstance Win32_Processor | Select-Object Name,NumberOfLogicalProcessors); ip=@(Get-NetIPAddress -AddressFamily IPv4 | Select-Object InterfaceAlias,IPAddress); session=(quser | Out-String); student_hash=(Get-FileHash 'C:/Users/mvmanh/Classroom-windows-teacher-test/ClassroomStudent.exe' -Algorithm SHA256).Hash} | ConvertTo-Json -Depth 5")
        mac = self.remotes['mac'].run('uname -sm; hostname; sw_vers; sysctl -n hw.model hw.memsize hw.ncpu; stat -f %Su /dev/console; shasum -a256 ' + shlex.quote(self.mac_bin))
        local = ps("@{name=$env:COMPUTERNAME; os=(Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version,OSArchitecture,TotalVisibleMemorySize); display=(& '" + str(ROOT / 'scripts/phase1-display-modes.ps1') + "' | Out-String)} | ConvertTo-Json -Depth 4")
        record = {'windows': json.loads(win), 'mac': mac, 'teacher': json.loads(local),
                  'fingerprints': {k: r.fingerprint for k, r in self.remotes.items()},
                  'agent_sha256': hashlib.sha256(self.agent.read_bytes()).hexdigest()}
        atomic_json(self.root / 'inventory.json', record)
        return record

    def prepare(self):
        windows = self.remotes['windows']
        if self.active:
            raise RuntimeError('Finish the active scenario before replacing binaries')
        # The owner requires stop/remove/copy for each replacement. Only this
        # test package is owned here; retain profiles and diagnostic evidence.
        windows.run("$ErrorActionPreference='Stop'; $package='C:\\Users\\mvmanh\\Classroom-teacher-validation\\bin'; "
                    "New-Item -ItemType Directory -Force $package | Out-Null; "
                    "$resolved=(Resolve-Path -LiteralPath $package).Path; "
                    "if($resolved -ne $package){throw 'Unexpected package path'}; "
                    "Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -and "
                    "$_.ExecutablePath.StartsWith($resolved+'\\', [StringComparison]::OrdinalIgnoreCase) } "
                    "| ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }; "
                    "Get-ChildItem -LiteralPath $resolved -File | Where-Object { $_.Extension -in '.exe','.dll' } "
                    "| Remove-Item -Force")
        source = ROOT / 'target/Classroom-Student-Windows-arm64'
        for file in source.iterdir():
            if file.is_file() and file.suffix.lower() in ('.exe', '.dll'):
                windows.put(file, self.remote_win_root + '/bin/' + file.name)
        mac = self.remotes['mac']
        mac.run('mkdir -p ' + shlex.quote(self.remote_mac_root))
        remote_hash = mac.run('shasum -a256 ' + shlex.quote(self.mac_bin)).split()[0]
        import zipfile
        with zipfile.ZipFile(ROOT / 'target/release-v1/Classroom-Student-macOS-v1.zip') as archive:
            expected = hashlib.sha256(archive.read('Classroom Student.app/Contents/MacOS/ClassroomStudent')).hexdigest()
        if remote_hash != expected:
            raise RuntimeError('Mac Student differs from v1: announce and deploy a matching bundle before continuing')
        win_hash = windows.run("(Get-FileHash 'C:/Users/mvmanh/Classroom-teacher-validation/bin/ClassroomStudent.exe' -Algorithm SHA256).Hash").lower()
        expected_win = hashlib.sha256((source / 'ClassroomStudent.exe').read_bytes()).hexdigest()
        if win_hash != expected_win:
            raise RuntimeError('Windows Student deployment hash mismatch')
        return {'mac_bundle': self.mac_app, 'mac_sha256': remote_hash,
                'windows_package_sha256': win_hash}

    def start_peer(self, name, headless=False, wifi=False, drop=0):
        run = self.active
        config = '[monitoring]\nenabled = false\n'
        if wifi:
            config += '[video]\njitter_buffer_ms = 50\nnack_repair_ms = 100\n'
        config_path = run['directory'] / (name + '-config.toml')
        config_path.write_text(config, encoding='utf-8')
        remote = self.remotes[name]
        args = ['--teacher-ip', self.args.teacher_ip, '--control-port', str(self.args.port),
                '--media-port', str(self.args.port + 10), '--feedback-port', str(self.args.port + 11)]
        if headless:
            args.append('--headless')
        if name == 'windows':
            folder = self.remote_win_root + '/' + run['label']
            native = folder.replace('/', '\\')
            remote.run("$ErrorActionPreference='Stop'; New-Item -ItemType Directory -Force '" + native + "' | Out-Null")
            remote.put(config_path, folder + '/config.toml')
            args += ['--config', folder + '/config.toml', '--launch-profile', folder + '/profile.toml',
                     '--log-file', folder + '/student.log']
            arg_line = subprocess.list2cmdline(args)
            source = ("$ErrorActionPreference='Stop'; $action=New-ScheduledTaskAction -Execute '" + self.remote_win_root.replace('/', '\\') + "\\bin\\ClassroomStudent.exe' -Argument '" + arg_line.replace("'", "''") + "'; $principal=New-ScheduledTaskPrincipal -UserId 'mvmanh' -LogonType Interactive -RunLevel Limited; $settings=New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries; Register-ScheduledTask -TaskName '" + self.task + "' -Action $action -Principal $principal -Settings $settings -Force | Out-Null; Start-ScheduledTask '" + self.task + "'; (Get-ScheduledTask '" + self.task + "').State")
            remote.run(source)
        else:
            folder = self.remote_mac_root + '/' + run['label']
            remote.run('mkdir -p ' + shlex.quote(folder))
            remote.put(config_path, folder + '/config.toml')
            args = [self.mac_bin, *args, '--config', folder + '/config.toml', '--launch-profile',
                    folder + '/profile.toml', '--log-file', folder + '/student.log']
            escape = xml.sax.saxutils.escape
            plist = ('<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict>'
                     '<key>Label</key><string>' + self.job + '</string><key>ProgramArguments</key><array>'
                     + ''.join('<string>' + escape(arg) + '</string>' for arg in args)
                     + '</array><key>ProcessType</key><string>Interactive</string><key>LegacyTimers</key><true/>'
                     '<key>EnvironmentVariables</key><dict><key>CLASSROOM_TEST_DROP_PERCENT</key><string>'
                     + str(drop) + '</string></dict><key>StandardOutPath</key><string>' + escape(folder)
                     + '/stdout.log</string><key>StandardErrorPath</key><string>' + escape(folder)
                     + '/stderr.log</string><key>RunAtLoad</key><true/></dict></plist>')
            local = run['directory'] / 'student.plist'
            local.write_text(plist, encoding='utf-8')
            remote.put(local, folder + '/student.plist')
            remote.run('launchctl bootstrap gui/$(id -u) ' + shlex.quote(folder + '/student.plist'))
        run['peers'][name] = {'folder': folder, 'headless': headless, 'wifi': wifi, 'drop': drop}

    def stop_peer(self, name):
        if not self.active or name not in self.active['peers']:
            return
        if name == 'windows':
            self.remotes[name].run("$task=Get-ScheduledTask -TaskName '" + self.task + "' -ErrorAction SilentlyContinue; if($task){Stop-ScheduledTask -TaskName '" + self.task + "'; Unregister-ScheduledTask -TaskName '" + self.task + "' -Confirm:$false}")
        else:
            self.remotes[name].run('launchctl bootout gui/$(id -u)/' + self.job + ' 2>/dev/null || true')

    def wait_state(self, predicate, timeout=30):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            state = self.active['ipc'].state
            if predicate(state):
                return state
            if self.active['ipc'].error:
                raise RuntimeError(self.active['ipc'].error)
            self.status(waiting=True)
            time.sleep(.15)
        raise TimeoutError('Teacher state wait expired')

    def begin(self, req):
        if self.active:
            self.finish()
        label = req.get('label', 'scenario')
        if not label.replace('-', '').replace('_', '').isalnum():
            raise ValueError('Invalid scenario label')
        directory = self.root / label
        directory.mkdir(exist_ok=False)
        config = '[monitoring]\nenabled = false\n[video]\nadaptive = ' + ('true' if req.get('adaptive', False) else 'false') + '\n[network]\nsend_pacing_mbps = ' + str(req.get('pacing', 400)) + '\n'
        (directory / 'teacher-config.toml').write_text(config, encoding='utf-8')
        endpoint = directory / 'endpoint.json'
        command = [str(self.agent), '--role', 'teacher', '--teacher-agent', '--ipc-file', str(endpoint),
                   '--console-path', str(self.console), '--log-file', str(directory / 'teacher.log'),
                   '--config', str(directory / 'teacher-config.toml'), '--launch-profile',
                   str(directory / 'teacher-profile.toml'), '--bind-ip', self.args.teacher_ip,
                   '--control-port', str(self.args.port), '--media-port', str(self.args.port+10),
                   '--feedback-port', str(self.args.port+11)]
        agent = subprocess.Popen(command, creationflags=CREATE_NO_WINDOW)
        deadline = time.monotonic() + 30
        while not endpoint.exists() and time.monotonic() < deadline:
            if agent.poll() is not None:
                raise RuntimeError('Teacher helper failed: inspect ' + str(directory / 'teacher.log'))
            time.sleep(.1)
        ipc = Ipc(endpoint)
        self.active = {'label': label, 'directory': directory, 'endpoint': endpoint, 'agent': agent,
                       'ipc': ipc, 'peers': {}, 'events': [], 'samples': [], 'started': time.time(),
                       'workload': None, 'ui': None, 'request': req}
        if req.get('gui', True):
            self.active['ui'] = subprocess.Popen([str(self.console), '--ipc-file', str(endpoint),
                                                  '--bind-ip', self.args.teacher_ip, '--control-port', str(self.args.port)])
        for name in req.get('peers', ['mac', 'windows']):
            self.start_peer(name, req.get('headless', False), req.get('wifi', False))
        count = len(self.active['peers'])
        self.wait_state(lambda s: sum(bool(row.get('online')) for row in s.get('students', [])) >= count)
        self.status(stage='ready')
        return {'directory': str(directory), 'students': self.active['ipc'].state.get('students')}

    def ipc(self, action, selected=(), value=None):
        result = self.active['ipc'].command(action, selected, value)
        self.active['events'].append({**result, 'unix_ms': int(time.time()*1000)})
        return result

    def reattach(self):
        run = self.active
        previous = run['ipc'].agent_pid
        run['ipc'].close()
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            try:
                info = json.loads(run['endpoint'].read_text(encoding='utf-8-sig'))
                if info['pid'] != previous and process_sample(info['pid'])['alive']:
                    run['ipc'] = Ipc(run['endpoint'])
                    self.wait_state(lambda s: bool(s.get('phase')), 10)
                    run['events'].append({'action': 'reattach', 'previous_pid': previous,
                                          'pid': run['ipc'].agent_pid, 'unix_ms': int(time.time()*1000)})
                    return {'pid': run['ipc'].agent_pid, 'state': run['ipc'].state}
            except (OSError, ValueError):
                pass
            time.sleep(.2)
        raise TimeoutError('Teacher did not replace the failed helper')

    def workload(self, seconds):
        old = self.active.get('workload')
        if old and old.poll() is None:
            old.terminate()
            old.wait(5)
        self.active['workload'] = subprocess.Popen(['powershell.exe', '-NoLogo', '-NoProfile',
            '-ExecutionPolicy', 'Bypass', '-File', str(ROOT / 'scripts/phase1-windows-workload.ps1'),
            '-Seconds', str(seconds)], creationflags=CREATE_NO_WINDOW)

    def start_simulators(self, count):
        if not 1 <= count <= 40:
            raise ValueError('Simulator count must be 1..40')
        run = self.active
        remote = self.remotes['mac']
        folder = self.remote_mac_root + '/' + run['label'] + '/simulators'
        remote.run('mkdir -p ' + shlex.quote(folder))
        config = run['directory'] / 'simulator-config.toml'
        config.write_text('[monitoring]\nenabled = false\n', encoding='utf-8')
        remote.put(config, folder + '/config.toml')
        run['simulators'] = {}
        commands = []
        escape = xml.sax.saxutils.escape
        for index in range(count):
            name = f'sim-{index+1:02d}'
            label = self.job + '.' + name
            args = [self.mac_bin, '--role', 'student', '--headless', '--teacher-ip', self.args.teacher_ip,
                    '--control-port', str(self.args.port), '--media-port', str(50000 + 2*index),
                    '--feedback-port', str(50001 + 2*index), '--config', folder + '/config.toml',
                    '--launch-profile', folder + '/' + name + '.toml', '--log-file', folder + '/' + name + '.log']
            plist = ('<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict>'
                     '<key>Label</key><string>' + label + '</string><key>ProgramArguments</key><array>'
                     + ''.join('<string>' + escape(arg) + '</string>' for arg in args)
                     + '</array><key>ProcessType</key><string>Interactive</string><key>LegacyTimers</key><true/>'
                     '<key>EnvironmentVariables</key><dict><key>HOSTNAME</key><string>' + name + '</string></dict>'
                     '<key>StandardOutPath</key><string>' + folder + '/' + name + '.stdout</string>'
                     '<key>StandardErrorPath</key><string>' + folder + '/' + name + '.stderr</string>'
                     '<key>RunAtLoad</key><true/></dict></plist>')
            local = run['directory'] / (name + '.plist')
            local.write_text(plist, encoding='utf-8')
            remote.put(local, folder + '/' + name + '.plist')
            run['simulators'][name] = {'label': label, 'log': folder + '/' + name + '.log'}
            commands.append('launchctl bootstrap gui/$(id -u) ' + shlex.quote(folder + '/' + name + '.plist'))
            self.status(stage='preparing_simulators', prepared=index+1, count=count)
        remote.run('set -e; ' + '; '.join(commands), timeout=120)
        self.wait_state(lambda s: sum(bool(row.get('online')) and str(row.get('name', '')).startswith('sim-')
                                     for row in s.get('students', [])) == count, 90)
        return {'registered': count}

    def stop_simulators(self):
        entries = self.active.get('simulators', {})
        if entries:
            self.remotes['mac'].run('; '.join('launchctl bootout gui/$(id -u)/' + peer['label']
                                   + ' 2>/dev/null || true' for peer in entries.values()), timeout=120)

    def run_scale(self, req):
        req = {**req, 'peers': []}
        self.begin(req)
        self.start_simulators(int(req['count']))
        seconds = int(req.get('seconds', 120))
        self.workload(seconds + 8)
        time.sleep(2)
        self.ipc('share_class')
        self.wait_state(lambda s: s.get('phase') == 'sharing' and s.get('recipients') == int(req['count']))
        self.sample(seconds)
        return self.finish()

    def sample(self, seconds):
        deadline = time.monotonic() + seconds
        report_at = time.monotonic()
        while time.monotonic() < deadline:
            state = self.active['ipc'].state
            samples = {'unix_ms': int(time.time()*1000), 'phase': state.get('phase'),
                       'agent': process_sample(self.active['ipc'].agent_pid),
                       'console': process_sample(self.active['ui'].pid) if self.active.get('ui') else None}
            self.active['samples'].append(samples)
            self.status(stage='sampling', remaining_seconds=round(deadline-time.monotonic()))
            if time.monotonic() >= report_at:
                print('SAMPLE ' + self.active['label'] + ' remaining=' + str(round(deadline-time.monotonic())) + ' phase=' + str(state.get('phase')), flush=True)
                report_at = time.monotonic() + 30
            if self.active['ipc'].error:
                raise RuntimeError(self.active['ipc'].error)
            time.sleep(min(2, max(0, deadline-time.monotonic())))
        return {'samples': len(self.active['samples']), 'state': self.active['ipc'].state}

    def run(self, req):
        self.begin(req)
        seconds = int(req.get('seconds', 120))
        self.workload(seconds + 8)
        time.sleep(2)
        self.ipc('share_class')
        self.wait_state(lambda s: s.get('phase') == 'sharing')
        self.sample(seconds)
        return self.finish()

    def collect(self):
        run = self.active
        for name, peer in run['peers'].items():
            self.remotes[name].get(peer['folder'] + '/student.log', run['directory'] / (name + '.log'))
        for name, peer in run.get('simulators', {}).items():
            self.remotes['mac'].get(peer['log'], run['directory'] / (name + '.log'))
        result = {'label': run['label'], 'directory': str(run['directory']), 'request': run['request'],
                  'events': run['events'], 'samples': run['samples'], 'teacher': log_report(run['directory'] / 'teacher.log', 'teacher'),
                  'students': {name: log_report(run['directory'] / (name + '.log'), 'student') for name in run['peers']}}
        result['students'].update({name: log_report(run['directory'] / (name + '.log'), 'student')
                                   for name in run.get('simulators', {})})
        atomic_json(run['directory'] / 'report.json', result)
        return result

    def finish(self):
        run = self.active
        if not run:
            return None
        if run['ipc'].state.get('phase') in ('starting', 'sharing', 'stopping') and not run['ipc'].error:
            self.ipc('stop')
            self.wait_state(lambda s: s.get('phase') == 'idle', 30)
        time.sleep(2)
        if run.get('workload') and run['workload'].poll() is None:
            run['workload'].terminate()
            run['workload'].wait(5)
        for name in run['peers']:
            self.stop_peer(name)
        self.stop_simulators()
        time.sleep(1)
        result = self.collect()
        if not run['ipc'].error:
            try:
                self.ipc('quit')
            except (EOFError, TimeoutError):
                pass
        natural_agent = None
        try:
            run['agent'].wait(10)
            natural_agent = True
        except subprocess.TimeoutExpired:
            natural_agent = False
            run['agent'].terminate()
            run['agent'].wait(5)
        if run['ipc'].agent_pid != run['agent'].pid:
            # Flutter may have replaced a crashed helper. Waiting on the old
            # Popen handle alone cannot establish that the replacement exited.
            deadline = time.monotonic() + 10
            while process_sample(run['ipc'].agent_pid)['alive'] and time.monotonic() < deadline:
                time.sleep(.1)
            natural_agent = not process_sample(run['ipc'].agent_pid)['alive']
            if not natural_agent:
                ps('Stop-Process -Id ' + str(run['ipc'].agent_pid) + ' -Force')
        ui = run.get('ui')
        natural_ui = None
        if ui:
            try:
                ui.wait(8)
                natural_ui = True
            except subprocess.TimeoutExpired:
                natural_ui = False
                ui.terminate()
                ui.wait(5)
        result['shutdown'] = {'natural_agent': natural_agent, 'natural_ui': natural_ui,
                              'agent_alive': process_sample(run['ipc'].agent_pid)['alive']}
        run['ipc'].close()
        atomic_json(run['directory'] / 'report.json', result)
        self.results.append({'label': run['label'], 'directory': str(run['directory']), 'shutdown': result['shutdown']})
        self.active = None
        self.status(stage='finished')
        return result

    def dispatch(self, req):
        action = req['action']
        if action == 'inventory': return self.inventory()
        if action == 'prepare': return self.prepare()
        if action == 'begin': return self.begin(req)
        if action == 'run': return self.run(req)
        if action == 'scale': return self.run_scale(req)
        if action == 'ipc': return self.ipc(req['command'], req.get('selected', []), req.get('input'))
        if action == 'sample': return self.sample(int(req['seconds']))
        if action == 'workload': self.workload(int(req['seconds'])); return {'started': True}
        if action == 'finish': return self.finish()
        if action == 'state': return self.active['ipc'].state if self.active else {}
        if action == 'reattach': return self.reattach()
        if action == 'local': return {'stdout': ps(req['source'], req.get('timeout', 60))}
        if action == 'remote': return {'stdout': self.remotes[req['host']].run(req['source'], req.get('timeout', 90))}
        if action == 'put': self.remotes[req['host']].put(req['local'], req['remote']); return {'copied': True}
        if action == 'get': self.remotes[req['host']].get(req['remote'], req['local']); return {'copied': True}
        if action == 'stop_peer': self.stop_peer(req['host']); return {'stopped': True}
        if action == 'start_peer': self.start_peer(req['host'], req.get('headless', False), req.get('wifi', False)); return {'started': True}
        raise ValueError('Unknown test action: ' + action)

    def close(self):
        if self.active:
            try:
                self.finish()
            except Exception:
                traceback.print_exc()
                # A failed scenario still owns only its exact recorded tasks/jobs.
                for name in tuple(self.active['peers']):
                    try: self.stop_peer(name)
                    except Exception: traceback.print_exc()
                try: self.stop_simulators()
                except Exception: traceback.print_exc()
                for key in ('workload', 'ui', 'agent'):
                    process = self.active.get(key)
                    if process and process.poll() is None:
                        process.terminate()
                        process.wait(5)
        if self.original_cache is not None:
            self.cache_path.write_bytes(self.original_cache)
        for remote in self.remotes.values():
            remote.close()


def scale_existing(args, mac):
    """Change selected fan-out counts within an already running soak.

    This owns simulator jobs only. It never starts/stops/replaces the Teacher,
    and leaves its two physical OS endpoints registered but idle during each
    selected simulator phase. All decoding load shares the remote Mac.
    """
    controller = Controller(args, None, mac)
    endpoint = Path(args.scale_existing).resolve()
    teacher_log = endpoint.with_name('teacher.log')
    ipc = Ipc(endpoint)
    reports = []
    print('SCALE_RUN=' + str(controller.root), flush=True)
    print('SCALE_WAIT_UNTIL=' + str(args.scale_start_unix), flush=True)
    try:
        while time.time() < args.scale_start_unix:
            time.sleep(min(1, args.scale_start_unix-time.time()))
        for count in args.scale_counts:
            label = f'scale-{count:02d}'
            directory = controller.root / label
            directory.mkdir()
            controller.active = {'label': label, 'directory': directory, 'ipc': ipc,
                                 'peers': {}, 'events': [], 'samples': [], 'ui': None,
                                 'request': {'count': count, 'seconds': args.scale_seconds,
                                             'shared_teacher': True, 'headless': True}}
            if ipc.state.get('phase') == 'sharing':
                controller.ipc('stop')
                controller.wait_state(lambda s: s.get('phase') == 'idle', 30)
            controller.start_simulators(count)
            selected = [row['id'] for row in ipc.state.get('students', [])
                        if row.get('online') and str(row.get('name', '')).startswith('sim-')]
            if len(selected) != count:
                raise RuntimeError('Unexpected simulator registration count')
            started = int(time.time()*1000)
            controller.ipc('share_selected', selected)
            controller.wait_state(lambda s: s.get('phase') == 'sharing' and s.get('recipients') == count)
            print(f'SCALE_REGISTERED={count} TEACHER_PID={ipc.agent_pid}', flush=True)
            controller.sample(args.scale_seconds)
            controller.ipc('stop')
            controller.wait_state(lambda s: s.get('phase') == 'idle', 30)
            ended = int(time.time()*1000)
            time.sleep(3)  # receiver session summaries follow asynchronous Stop
            controller.stop_simulators()
            time.sleep(1)
            # Keep each phase's complete Teacher session and its receiver logs.
            import re
            lines = []
            for line in teacher_log.read_text(encoding='utf-8-sig', errors='replace').splitlines():
                match = re.match(r'unix_ms=(\d+)', line)
                if match and started <= int(match.group(1)) <= ended:
                    lines.append(line)
            (directory / 'teacher.log').write_text('\n'.join(lines)+'\n', encoding='utf-8')
            report = controller.collect()
            reports.append({'count': count, 'directory': str(directory), 'teacher': report['teacher'],
                            'receivers': {name: data['final'] for name, data in report['students'].items()}})
            atomic_json(controller.root / 'scale-summary.json', {'teacher_pid': ipc.agent_pid,
                        'shared_mac_host': True, 'phases': reports})
            controller.active = None
            print('SCALE_COMPLETE=' + str(count), flush=True)
        physical = [row['id'] for row in ipc.state.get('students', [])
                    if row.get('online') and not str(row.get('name', '')).startswith('sim-')]
        if physical:
            ipc.command('share_selected', physical)
        print('SCALE_LADDER_COMPLETE', flush=True)
    finally:
        if controller.active:
            controller.stop_simulators()
        ipc.close()
        mac.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--windows', default='mvmanh@192.168.50.105')
    parser.add_argument('--mac', default='mvmanh@192.168.50.205')
    parser.add_argument('--teacher-ip', default='192.168.50.94')
    parser.add_argument('--port', type=int, default=48900)
    parser.add_argument('--bundle', default='target/Classroom-Console-Windows-x64')
    parser.add_argument('--mac-app', default='/Users/mvmanh/Classroom-teacher-validation/Classroom Student.app')
    parser.add_argument('--scale-existing', help='Reuse this Teacher IPC endpoint for selected 1/5/10/20/40 fan-out')
    parser.add_argument('--scale-start-unix', type=float, default=0)
    parser.add_argument('--scale-seconds', type=int, default=90)
    parser.add_argument('--scale-counts', type=int, nargs='+', default=[1, 5, 10, 20, 40])
    args = parser.parse_args()
    if os.name != 'nt': parser.error('Run this controller on Windows')
    if not 1024 <= args.port <= 65400: parser.error('Test port out of range')
    if args.scale_existing:
        if not 30 <= args.scale_seconds <= 600: parser.error('Scale duration must be 30..600 seconds')
        if any(not 1 <= count <= 40 for count in args.scale_counts): parser.error('Scale counts must be 1..40')
        mac = Remote(args.mac, getpass.getpass('Mac Student SSH password (memory only): '))
        scale_existing(args, mac)
        return
    windows = Remote(args.windows, getpass.getpass('Windows Student SSH password (memory only): '), True)
    mac = Remote(args.mac, getpass.getpass('Mac Student SSH password (memory only): '))
    controller = Controller(args, windows, mac)
    print('VALIDATION_RUN=' + str(controller.root), flush=True)
    try:
        controller.status(stage='ready_for_commands')
        while True:
            path = controller.root / 'command.json'
            if not path.exists():
                time.sleep(.1)
                continue
            try:
                req = json.loads(path.read_text(encoding='utf-8-sig'))
                path.unlink()
                if req.get('action') == 'quit': break
                controller.status(stage='executing', command=req.get('id'))
                result = controller.dispatch(req)
                reply = {'id': req.get('id'), 'ok': True, 'result': result}
            except Exception as exc:
                reply = {'id': req.get('id'), 'ok': False, 'error': str(exc), 'traceback': traceback.format_exc()}
                print('FAILED ' + str(req.get('id')) + ': ' + str(exc), flush=True)
            atomic_json(controller.root / ('result-' + str(req.get('id')) + '.json'), reply)
            print('RESULT ' + str(req.get('id')) + ' ok=' + str(reply['ok']), flush=True)
            controller.status(stage='ready_for_commands')
    finally:
        controller.close()


if __name__ == '__main__':
    main()
