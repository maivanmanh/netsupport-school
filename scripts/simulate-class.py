#!/usr/bin/env python3
"""Simulate a classroom on one powerful machine and measure the Teacher.

Starts N headless Students (distinct UDP ports, isolated launch profiles,
HOSTNAME=sim-NN, monitoring disabled) on a remote Mac over SSH, optionally
emulates random packet loss (CLASSROOM_TEST_DROP_PERCENT, test only, no
firewall changes), starts a class share through the running Teacher agent's
authenticated IPC, optionally runs the repeatable motion workload on the
Teacher screen, then stops everything and writes a report:

    target/sim-class/<timestamp>/report.md (+ report.json, raw logs)

Password: CLASSROOM_SIM_SSH_PASSWORD (else prompted; kept in memory only).
Students named sim-* are not persisted in the Teacher console roster.
"""
import argparse
import getpass
import json
import os
import re
import select
import shlex
import socket
import struct
import subprocess
import time
from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = spec_from_file_location('remote', ROOT / 'scripts/run-four-machine-test.py')
remote = module_from_spec(spec)
spec.loader.exec_module(remote)
ENDPOINT = Path.home() / ('Library/Containers/com.classroom.classroomConsole/Data/Library/'
                          'Application Support/com.classroom.classroomConsole/teacher-endpoint.json')
TEACHER_LOG = ENDPOINT.with_name('teacher-log.txt')
REMOTE_BIN = '"$HOME/Classroom-two-mac/Classroom Student.app/Contents/MacOS/ClassroomStudent"'


class Ipc:
    """Framed JSON IPC to the Teacher agent (token never printed)."""

    def __init__(self, endpoint):
        info = json.loads(Path(endpoint).read_text())
        self.socket = socket.create_connection(('127.0.0.1', info['port']), timeout=5)
        self.serial = 1000
        self.send({'type': 'hello', 'protocol': 1, 'token': info['token']})

    def send(self, value):
        data = json.dumps(value).encode()
        self.socket.sendall(struct.pack('!I', len(data)) + data)

    def receive(self):
        def exact(n):
            buffer = b''
            while len(buffer) < n:
                chunk = self.socket.recv(n - len(buffer))
                if not chunk:
                    raise EOFError('Teacher agent disconnected')
                buffer += chunk
            return buffer
        return json.loads(exact(struct.unpack('!I', exact(4))[0]))

    def state(self):
        while True:
            value = self.receive()
            if 'students' in value:
                return value

    def drain(self, deadline):
        # select() before each whole frame so a timeout never splits one.
        while (left := deadline - time.time()) > 0:
            if select.select([self.socket], [], [], min(1.0, left))[0]:
                self.receive()

    def command(self, action):
        self.serial += 1
        self.send({'id': self.serial, 'action': action, 'selected': [], 'input': {}})
        while True:
            value = self.receive()
            if value.get('id') == self.serial:
                return value


def numbers(line, *keys):
    out = {}
    for key in keys:
        match = re.search(rf'\b{key}=([0-9.]+)', line)
        if match:
            out[key] = float(match.group(1))
    return out


def summarize(values):
    values = sorted(v for v in values if v is not None)
    if not values:
        return None
    pick = lambda q: values[min(len(values) - 1, int(q * (len(values) - 1)))]
    return {'n': len(values), 'p50': round(pick(0.5), 2), 'p95': round(pick(0.95), 2),
            'min': round(values[0], 2), 'max': round(values[-1], 2)}


def teacher_lines(start_ms, end_ms):
    lines = []
    for line in TEACHER_LOG.read_text(errors='replace').splitlines():
        match = re.match(r'unix_ms=(\d+)', line)
        if match and start_ms <= int(match.group(1)) <= end_ms:
            lines.append(line)
    return lines


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--host', default='mvmanh@192.168.50.2')
    parser.add_argument('--teacher', default='192.168.50.206')
    parser.add_argument('--count', type=int, default=20)
    parser.add_argument('--seconds', type=int, default=60)
    parser.add_argument('--drop-percent', type=int, default=0)
    parser.add_argument('--base-port', type=int, default=48100)
    parser.add_argument('--workload', action='store_true', help='run the motion workload on the Teacher screen')
    parser.add_argument('--label', default='')
    parser.add_argument('--student-interface', default='',
                        help='adapter for the Students (e.g. bridge0) when the default route is another link')
    args = parser.parse_args()
    if not 1 <= args.count <= 60 or not 10 <= args.seconds <= 600 or not 0 <= args.drop_percent <= 50:
        parser.error('count 1..60, seconds 10..600, drop-percent 0..50')
    stamp = time.strftime('%Y%m%d-%H%M%S')
    out = ROOT / 'target/sim-class' / f'{stamp}{"-" + args.label if args.label else ""}'
    out.mkdir(parents=True, exist_ok=True)
    host = remote.Remote(args.host, os.environ.get('CLASSROOM_SIM_SSH_PASSWORD')
                         or getpass.getpass(f'{args.host} SSH password: '))
    sim = '$HOME/Classroom-sim'
    host.ssh(f"pkill -f 'Classroom-sim/profile' 2>/dev/null; rm -rf {sim}; mkdir -p {sim}; "
             f"printf '[monitoring]\\nenabled = false\\n' > {sim}/sim.toml")
    launches = []
    for i in range(args.count):
        port = args.base_port + 2 * i
        name = f'sim-{i + 1:02d}'
        env = f'HOSTNAME={name} CLASSROOM_TEST_DROP_PERCENT={args.drop_percent}'
        command = (f'{env} nohup {REMOTE_BIN} --role student --teacher-ip {args.teacher} --headless '
                   f'--media-port {port} --feedback-port {port + 1} --config {sim}/sim.toml '
                   f'{"--interface " + shlex.quote(args.student_interface) + " " if args.student_interface else ""}'
                   f'--launch-profile {sim}/profile-{i}.toml --log-file {sim}/{name}.log '
                   f'--share-seconds {args.seconds + 90} >/dev/null 2>&1 &')
        launches.append(command)
    host.ssh('; '.join(launches))
    print(f'STARTED {args.count} simulated Students on {args.host}', flush=True)
    ipc = Ipc(ENDPOINT)
    deadline = time.time() + 30
    while True:
        state = ipc.state()
        online = [s for s in state['students'] if s.get('online') and str(s.get('name', '')).startswith('sim-')]
        if len(online) >= args.count or time.time() > deadline:
            break
    print(f'REGISTERED {len(online)}/{args.count}; phase={state.get("phase")}', flush=True)
    workload = None
    started_ms = int(time.time() * 1000)
    owned_share = state.get('phase') != 'sharing'
    if owned_share:
        print('SHARE', ipc.command('share_class').get('ok'), flush=True)
    if args.workload:
        binary = ROOT / 'target/phase1-mac-workload'
        source = ROOT / 'scripts/phase1-mac-workload.swift'
        if not binary.exists() or source.stat().st_mtime > binary.stat().st_mtime:
            subprocess.run(['swiftc', str(source), '-o', str(binary)], check=True, timeout=180)
        workload = subprocess.Popen([str(binary), str(args.seconds)],
                                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    # Keep draining state pushes: the agent drops IPC clients that stop reading
    # for 3 s, and 40+ Students fill the socket buffer quickly.
    ipc.drain(time.time() + args.seconds)
    ended_ms = int(time.time() * 1000)
    if owned_share:
        print('STOP', ipc.command('stop').get('ok'), flush=True)
    if workload and workload.poll() is None:
        workload.terminate()
    time.sleep(3)
    host.ssh(f"pkill -f 'Classroom-sim/profile' 2>/dev/null; true")
    # scp resolves relative remote paths against the home directory.
    host.copy(f'{host.address}:Classroom-sim', out / 'students', recursive=True)
    host.ssh(f'rm -rf {sim}')

    # Teacher-side measurements over the share window.
    lines = teacher_lines(started_ms, ended_ms)
    (out / 'teacher.log').write_text('\n'.join(lines))
    media = [numbers(l, 'send_mbps', 'encode_fps', 'process_cpu_percent', 'connected_students',
                     'retransmitted_packets', 'nack_requested_sequences', 'encoded_stream_mbps',
                     'target_fps', 'send_errors')
             for l in lines if 'teacher M3 media interval' in l]
    keyframes = [numbers(l, 'keyframe_requests', 'keyframe_requests_coalesced', 'keyframes_forced',
                         'keyframes_emitted', 'keyframe_max_bytes') for l in lines if 'teacher keyframe interval' in l]
    refine = [numbers(l, 'refinements', 'refinement_frames', 'refinement_bytes') for l in lines
              if 'teacher refinement interval' in l]
    jitter_line = [l for l in lines if 'teacher pointer channel interval' in l]
    report = {
        'count': args.count, 'seconds': args.seconds, 'drop_percent': args.drop_percent,
        'workload': args.workload, 'registered': len(online),
        'teacher': {k: summarize([m.get(k) for m in media]) for k in
                    ('send_mbps', 'encode_fps', 'process_cpu_percent', 'connected_students',
                     'encoded_stream_mbps', 'target_fps')},
        'teacher_totals': {
            'retransmitted_packets_last': media[-1].get('retransmitted_packets') if media else None,
            'nack_requested_sequences_last': media[-1].get('nack_requested_sequences') if media else None,
            'keyframe_requests': sum(k.get('keyframe_requests', 0) for k in keyframes),
            'keyframe_requests_coalesced': sum(k.get('keyframe_requests_coalesced', 0) for k in keyframes),
            'keyframes_forced': sum(k.get('keyframes_forced', 0) for k in keyframes),
            'keyframes_emitted': sum(k.get('keyframes_emitted', 0) for k in keyframes),
            'keyframe_max_bytes': max([k.get('keyframe_max_bytes', 0) for k in keyframes] or [0]),
            'refinements': sum(r.get('refinements', 0) for r in refine),
            'refinement_bytes': sum(r.get('refinement_bytes', 0) for r in refine),
            'pointer_intervals': len(jitter_line),
        },
        'students': {},
    }
    decode, missing, jitter, keyreq = [], [], [], []
    for log in sorted((out / 'students').glob('sim-*.log')):
        rows = [numbers(l, 'decode_fps', 'missing_packets', 'jitter_delay_ms', 'keyframe_requests',
                        'jitter_target_ms', 'nack_sent', 'late_packets')
                for l in log.read_text(errors='replace').splitlines() if 'student M3 media interval' in l]
        rows = rows[2:]  # skip join intervals
        if not rows:
            report['students'][log.stem] = 'no media intervals'
            continue
        decode += [r.get('decode_fps') for r in rows]
        jitter += [r.get('jitter_delay_ms') for r in rows]
        missing.append(rows[-1].get('missing_packets'))
        keyreq.append(rows[-1].get('keyframe_requests'))
        report['students'][log.stem] = {'decode_fps': summarize([r.get('decode_fps') for r in rows]),
                                        'missing_packets_last': rows[-1].get('missing_packets')}
    report['students_aggregate'] = {'decode_fps': summarize(decode), 'jitter_delay_ms': summarize(jitter),
                                    'missing_packets_last': summarize(missing),
                                    'keyframe_requests_last': summarize(keyreq)}
    (out / 'report.json').write_text(json.dumps(report, indent=2))
    md = [f'# Simulated class {stamp}', '',
          f'- Students: {args.count} headless on {args.host} (registered {len(online)})',
          f'- Duration: {args.seconds}s, emulated loss {args.drop_percent}%, workload {args.workload}', '',
          '| Teacher metric | p50 | p95 | max |', '|---|---|---|---|']
    for key, value in report['teacher'].items():
        if value:
            md.append(f"| {key} | {value['p50']} | {value['p95']} | {value['max']} |")
    md += ['', '| Teacher totals | value |', '|---|---|']
    md += [f'| {k} | {v} |' for k, v in report['teacher_totals'].items()]
    md += ['', '| Students (aggregate) | p50 | p95 | min |', '|---|---|---|---|']
    for key, value in report['students_aggregate'].items():
        if value:
            md.append(f"| {key} | {value['p50']} | {value['p95']} | {value['min']} |")
    (out / 'report.md').write_text('\n'.join(md) + '\n')
    print('\n'.join(md))
    print(f'REPORT {out}')


if __name__ == '__main__':
    main()
