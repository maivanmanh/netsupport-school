#!/usr/bin/env python3
"""Real macOS Teacher + complete Ubuntu Student functional LAN checks.

Uses existing binaries, isolated ports/profiles and only owned processes.
Does not rebuild/replace Teacher packages or change privacy/network permissions.
"""
import argparse
from functools import partial
import hashlib
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import signal
import socket
import struct
import subprocess
import threading
import time

ROOT = Path(__file__).resolve().parents[1]
PRESENTED = 'student first presented frame'
STOPPED = 'student preview closed after Teacher stop'
DISCONNECTED = 'student preview closed after Teacher disconnect'


class IPC:
    def __init__(self, endpoint):
        info = json.loads(endpoint.read_text())
        self.socket = socket.create_connection(('127.0.0.1', info['port']), timeout=10)
        self.socket.settimeout(None)
        self.condition = threading.Condition()
        self.state, self.acks, self.error, self.serial = {}, {}, None, 0
        self.send({'type': 'hello', 'protocol': 1, 'token': info['token'], 'ui_role': 'automation'})
        threading.Thread(target=self.read, daemon=True).start()

    def send(self, value):
        body = json.dumps(value).encode()
        self.socket.sendall(struct.pack('!I', len(body)) + body)

    def exact(self, count):
        data = bytearray()
        while len(data) < count:
            chunk = self.socket.recv(count - len(data))
            if not chunk:
                raise EOFError('Teacher IPC closed')
            data.extend(chunk)
        return data

    def read(self):
        try:
            while True:
                size = struct.unpack('!I', self.exact(4))[0]
                if not 0 < size <= 8 * 1024 * 1024:
                    raise ValueError('IPC frame exceeds bounds')
                value = json.loads(self.exact(size))
                with self.condition:
                    if value.get('type') == 'state':
                        self.state = value
                    elif 'id' in value:
                        self.acks[value['id']] = value
                    self.condition.notify_all()
        except Exception as error:
            with self.condition:
                self.error = error
                self.condition.notify_all()

    def command(self, action):
        with self.condition:
            self.serial += 1
            serial = self.serial
            self.send({'id': serial, 'action': action})
            end = time.monotonic() + 30
            while serial not in self.acks:
                if self.error:
                    raise self.error
                remaining = end - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError('IPC command ' + action)
                self.condition.wait(remaining)
            ack = self.acks.pop(serial)
            if not ack.get('ok'):
                raise RuntimeError(f'{action}: {ack.get("error")}')

    def snapshot(self):
        with self.condition:
            return dict(self.state)

    def close(self):
        try:
            self.socket.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self.socket.close()


def wait_for(label, probe, timeout=90, interval=2):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        value = probe()
        if value:
            return value
        time.sleep(interval)
    raise TimeoutError(label)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--vm', default='Ubuntu 26.04 ARM64')
    parser.add_argument('--student-source', default='/home/parallels/src/netsupport-school/target/ubuntu-student/Classroom-Student-Ubuntu-aarch64/ClassroomStudent')
    parser.add_argument('--student-interface', default='eth1')
    parser.add_argument('--xvfb', action='store_true', help='no logged-in desktop: use an owned Xvfb')
    parser.add_argument('--teacher-ip', default='10.211.55.2')
    parser.add_argument('--control-port', type=int, default=49000)
    parser.add_argument('--fps', type=int, default=5, help='Teacher max FPS; 5 keeps the functional profile')
    parser.add_argument('--soak-seconds', type=int, default=180)
    parser.add_argument('--teacher-binary', type=Path, default=ROOT / 'target/Classroom Console.app/Contents/Helpers/Classroom Agent.app/Contents/MacOS/Classroom')
    args = parser.parse_args()
    if not 1024 <= args.control_port <= 65524 or not 30 <= args.soak_seconds <= 600:
        parser.error('Use a high control port and a 30–600 second functional soak')
    tag = time.strftime('%Y%m%d-%H%M%S')
    run = ROOT / 'target/ubuntu-lan-validation' / tag
    transfer = run / 'transfer'
    transfer.mkdir(parents=True)
    worker = 'ubuntu-student-lan-worker.py'
    (transfer / worker).write_bytes((ROOT / 'scripts' / worker).read_bytes())
    teacher = args.teacher_binary.resolve()
    workload = run / 'workload'
    subprocess.run(['swiftc', str(ROOT / 'scripts/ubuntu-lan-workload.swift'),
                    '-o', str(workload)], check=True, timeout=180)
    permission = json.loads(subprocess.check_output([str(teacher), '--permission-status'], text=True))
    if permission.get('screen') is not True:
        raise RuntimeError(f'Screen Recording must be granted manually to {teacher}')
    manifest = {'teacher_binary': str(teacher),
                'teacher_sha256': hashlib.sha256(teacher.read_bytes()).hexdigest(),
                'teacher_reused_existing_package': True, 'vm': args.vm,
                'teacher_ip': args.teacher_ip, 'control_port': args.control_port,
                'functional_only': args.fps == 5, 'run': tag, 'max_fps': args.fps}
    # Preserve screen geometry; a low configured cadence keeps this a functional
    # lifecycle check on the owner's slow two-core VM, not a speed benchmark.
    config = run / 'functional.toml'
    if args.fps == 5:
        config.write_text('[video]\nmax_fps = 5\nadaptive_idle_fps = 2\n'
                          'adaptive_typing_fps = 5\ntarget_bitrate_kbps = 2000\n')
    else:
        config.write_text(f'[video]\nmax_fps = {args.fps}\n')
    (run / 'manifest.json').write_text(json.dumps(manifest, indent=2))
    handler = partial(SimpleHTTPRequestHandler, directory=str(transfer))
    server = ThreadingHTTPServer((args.teacher_ip, 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    guest_worker = f'/tmp/classroom-ubuntu-lan-worker-{tag}.py'

    def guest(*command, timeout=45):
        # Parallels guest tools occasionally return a failed job handle after
        # executing the command. Worker mutations are journaled/idempotent so
        # these specific transport errors can safely be retried.
        for attempt in range(3):
            result = subprocess.run(['prlctl', 'exec', args.vm, *map(str, command)],
                                    text=True, capture_output=True, timeout=timeout)
            if result.returncode == 0:
                return result.stdout
            reason = result.stderr.strip() or result.stdout.strip()
            if 'PrlJob_' not in reason or 'Invalid argument' not in reason or attempt == 2:
                raise RuntimeError(reason)
            time.sleep(1)

    def remote(action):
        return json.loads(guest('python3', guest_worker, action, '--run', tag,
                                '--teacher-ip', args.teacher_ip,
                                '--control-port', args.control_port,
                                '--source', args.student_source,
                                '--interface', args.student_interface,
                                *(['--xvfb', '--stop-xvfb'] if args.xvfb else [])))

    owned, teacher_process, ipc, configured = [], None, None, False
    result = {'checks': {}, 'functional_only': True}

    def checkpoint(name, details=True):
        result['checks'][name] = details
        (run / 'results.json').write_text(json.dumps(result, indent=2))
        print(name, json.dumps(details), flush=True)

    def launch_teacher(name):
        # A raw agent deliberately refuses an existing IPC file. Keep the
        # killed agent's private endpoint as evidence and use a fresh test IPC
        # endpoint while retaining the same room ports and database directory.
        endpoint = run / (name + '-endpoint.json')
        launched = time.time()
        command = [str(teacher), '--role', 'teacher', '--teacher-agent',
                   '--config', str(config),
                   '--bind-ip', args.teacher_ip, '--interface', 'bridge100',
                   '--control-port', str(args.control_port),
                   '--media-port', str(args.control_port + 10),
                   '--feedback-port', str(args.control_port + 11),
                   '--ipc-file', str(endpoint), '--launch-profile', str(run / 'teacher-profile/profile.toml'),
                   '--log-file', str(run / (name + '.log'))]
        with (run / (name + '.stdout')).open('w') as output:
            process = subprocess.Popen(command, stdout=output, stderr=subprocess.STDOUT)
        owned.append(process)
        def ready():
            if process.poll() is not None:
                raise RuntimeError('Teacher exited before IPC readiness')
            return endpoint.exists() and endpoint.stat().st_mtime >= launched
        wait_for('Teacher IPC endpoint', ready)
        connection = IPC(endpoint)
        wait_for('Teacher idle', lambda: connection.snapshot().get('phase') == 'idle')
        return process, connection

    def row():
        return next((r for r in ipc.snapshot().get('students', []) if r.get('online')), None)

    def frame(count=1):
        value = remote('status')
        if not value['alive']:
            raise RuntimeError('Student stopped before expected output: ' + value['tail'][-2000:])
        return value if value['events'][PRESENTED] >= count else None

    def start_sharing():
        ipc.command('share_class')
        wait_for('Teacher capture/encode start', lambda: ipc.snapshot().get('phase') == 'sharing')

    print('RUN_DIR=' + str(run), flush=True)
    try:
        guest('curl', '-fsS', '-o', guest_worker,
              f'http://{args.teacher_ip}:{server.server_address[1]}/{worker}')
        setup = remote('setup')
        configured = True
        manifest['student_sha256'] = setup['sha256']
        manifest['student_binary'] = setup['binary']
        manifest['desktop'] = setup['desktop']
        (run / 'manifest.json').write_text(json.dumps(manifest, indent=2))
        with (run / 'workload.stdout').open('w') as output:
            owned.append(subprocess.Popen([str(workload), str(args.soak_seconds + 300), str(run / 'workload-audit.json')],
                                          stdout=output, stderr=subprocess.STDOUT))
        teacher_process, ipc = launch_teacher('teacher-1')
        remote('start')
        first = wait_for('Ubuntu idle registration', row, timeout=180)
        identity = first['id']
        start_sharing()
        first_output = wait_for('First real screen', frame)
        checkpoint('real_screen_received', {'student_id': identity, 'pid': first_output['pid']})
        time.sleep(8)
        first_metrics = row()
        if not first_metrics or not first_metrics.get('media', {}).get('receive_mbps'):
            raise RuntimeError('No real media receiver report at Teacher')
        checkpoint('teacher_receiver_report', first_metrics['media'])
        ipc.command('stop')
        wait_for('Teacher stop', lambda: ipc.snapshot().get('phase') == 'idle')
        stopped = wait_for('Ubuntu preview close', lambda: remote('status')['events'][STOPPED] >= 1)
        checkpoint('stop_without_student_restart', bool(stopped and remote('status')['pid'] == first_output['pid']))
        remote('stop')
        wait_for('Ubuntu offline', lambda: row() is None, timeout=30)
        start_sharing()
        time.sleep(3)
        remote('start')
        wait_for('Late join render', frame, timeout=180)
        late = wait_for('Late join roster', row)
        if late['id'] != identity:
            raise RuntimeError('Student identity changed after restart')
        checkpoint('active_late_join_same_identity')
        current_pid = remote('status')['pid']
        teacher_process.kill()
        teacher_process.wait(timeout=15)
        ipc.close()
        wait_for('Preview release after Teacher exit', lambda: remote('status')['events'][DISCONNECTED] >= 1)
        teacher_process, ipc = launch_teacher('teacher-2')
        rejoined = wait_for('Automatic full-agent reconnect', row)
        if rejoined['id'] != identity or remote('status')['pid'] != current_pid:
            raise RuntimeError('Reconnect changed Student identity/process')
        start_sharing()
        wait_for('Render after Teacher restart', lambda: frame(2))
        checkpoint('teacher_restart_auto_reconnect_same_student_process')
        closed_before = remote('status')['events'][DISCONNECTED]
        teacher_process.send_signal(signal.SIGSTOP)
        try:
            wait_for('Silent Teacher preview release',
                     lambda: remote('status')['events'][DISCONNECTED] > closed_before, timeout=25)
        finally:
            teacher_process.send_signal(signal.SIGCONT)
        wait_for('Render after control liveness recovery', lambda: frame(3))
        checkpoint('silent_teacher_release_and_recovery')
        samples, end = [], time.monotonic() + args.soak_seconds
        while time.monotonic() < end:
            current = row()
            remote_state = remote('status')
            if not current or current['id'] != identity or not remote_state['alive']:
                raise RuntimeError('Student left the room during functional soak')
            samples.append({'elapsed_seconds': round(args.soak_seconds - (end - time.monotonic()), 1),
                            'media': current.get('media', {})})
            (run / 'soak-samples.json').write_text(json.dumps(samples, indent=2))
            time.sleep(min(10, max(0, end - time.monotonic())))
        if not any((s['media'].get('decode_fps') or 0) > 0 and (s['media'].get('render_fps') or 0) > 0 for s in samples):
            raise RuntimeError('Soak never observed decode and render progress')
        checkpoint('functional_soak', {'seconds': args.soak_seconds, 'samples': len(samples), 'performance_assessed': False})
        ipc.command('stop')
        wait_for('Final idle', lambda: ipc.snapshot().get('phase') == 'idle')
        result['passed'] = True
    except BaseException as error:
        result['passed'] = False
        result['error'] = str(error)
        raise
    finally:
        if teacher_process is not None and teacher_process.poll() is None:
            teacher_process.send_signal(signal.SIGCONT)
            if ipc is not None:
                try:
                    ipc.command('quit')
                except Exception:
                    pass
        if ipc is not None:
            ipc.close()
        if configured:
            try:
                remote('stop')
                for name, text in remote('logs').items():
                    (run / name).write_text(text)
            except Exception as error:
                result['cleanup_error'] = str(error)
        for process in owned:
            if process.poll() is None:
                process.send_signal(signal.SIGCONT)
                process.terminate()
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        server.shutdown()
        server.server_close()
        (run / 'results.json').write_text(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
