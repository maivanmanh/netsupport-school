#!/usr/bin/env python3
"""Isolated Ubuntu functional-test worker, invoked through Parallels guest tools.

Never installs a service or changes the desktop's permissions. Only processes
whose command line contains this run's exact binary path may be stopped.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import pwd
import re
import shutil
import signal
import subprocess
import time


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest() if hasattr(hashlib, 'file_digest') else hashlib.sha256(stream.read()).hexdigest()


def alive(state):
    try:
        command = Path(f"/proc/{state['pid']}/cmdline").read_bytes().split(b'\0')
        return str(state['binary']).encode() in command
    except (OSError, KeyError):
        return False


def desktop_environment(user):
    result = subprocess.run(['runuser', '-u', user, '--', 'env',
                             f'XDG_RUNTIME_DIR=/run/user/{pwd.getpwnam(user).pw_uid}',
                             'systemctl', '--user', 'show-environment'],
                            capture_output=True, text=True, check=True)
    allowed = {'DISPLAY', 'XAUTHORITY', 'XDG_RUNTIME_DIR', 'WAYLAND_DISPLAY'}
    values = dict(line.split('=', 1) for line in result.stdout.splitlines()
                  if '=' in line and line.split('=', 1)[0] in allowed)
    if not values.get('DISPLAY') or not values.get('XAUTHORITY'):
        raise RuntimeError('User has no existing X11/XWayland desktop environment')
    return values


def xvfb_environment(root, user):
    # Fallback when nobody is logged in to the desktop: an owned virtual X server.
    # It verifies network/decode/present, not compositor fullscreen behaviour.
    display = ':97'
    log = (root / 'xvfb.log').open('w')
    child = subprocess.Popen(['runuser', '-u', user, '--', 'Xvfb', display,
                              '-screen', '0', '1920x1080x24', '-nolisten', 'tcp'],
                             stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    for _ in range(50):
        if Path('/tmp/.X11-unix/X97').exists():
            return {'DISPLAY': display}, child.pid
        time.sleep(.1)
    raise RuntimeError('Xvfb did not start')


def status(root, state):
    path = root / state.get('log', 'student-1.log')
    text = path.read_text(errors='replace') if path.exists() else ''
    phrases = ('student first presented frame', 'student first decoded frame',
               'student preview closed after Teacher stop',
               'student preview closed after Teacher disconnect',
               'teacher control liveness timeout', 'student room connection ready',
               'student discarded unrecoverable access unit')
    summaries = [line for line in text.splitlines()
                 if 'student M3 media complete' in line or 'student M3 media interval' in line]
    return {'alive': alive(state), 'pid': state.get('pid'),
            'log': str(path), 'bytes': len(text),
            'events': {phrase: text.count(phrase) for phrase in phrases},
            'latest_summary': summaries[-1] if summaries else None,
            'tail': text[-16000:]}


def stop(state):
    if not alive(state):
        return
    # Popen started a new process session for this exact runuser + Student pair.
    group = os.getpgid(state['pid'])
    if group != state['pid']:
        raise RuntimeError('Refusing to signal an unexpected process group')
    os.killpg(group, signal.SIGCONT)
    os.killpg(group, signal.SIGINT)
    for _ in range(50):
        if not alive(state):
            return
        time.sleep(.1)
    if alive(state):
        os.killpg(group, signal.SIGTERM)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['setup', 'start', 'status', 'stop', 'logs'])
    parser.add_argument('--run', required=True)
    parser.add_argument('--user', default='parallels')
    parser.add_argument('--teacher-ip', default='10.211.55.2')
    parser.add_argument('--interface', default='auto')
    parser.add_argument('--source', default='/tmp/classroom-ubuntu-validation/source/target/debug/ClassroomStudent')
    parser.add_argument('--xvfb', action='store_true', help='use an owned Xvfb instead of the logged-in desktop')
    parser.add_argument('--control-port', type=int, default=49000)
    parser.add_argument('--stop-xvfb', action='store_true')
    args = parser.parse_args()
    if not re.fullmatch(r'[A-Za-z0-9_-]{1,80}', args.run):
        parser.error('Invalid isolated run identifier')
    root = Path('/tmp') / ('classroom-ubuntu-lan-' + args.run)
    state_path = root / 'state.json'
    state = json.loads(state_path.read_text()) if state_path.exists() else {}
    if args.action == 'setup':
        if root.exists():
            if state.get('binary') == str(root / 'ClassroomStudent') and digest(Path(state['binary'])) == state.get('sha256'):
                print(json.dumps(state))
                return
            raise RuntimeError('Run directory exists without a matching test journal')
        source = Path(args.source)
        root.mkdir(mode=0o755)
        account = pwd.getpwnam(args.user)
        os.chown(root, account.pw_uid, account.pw_gid)
        binary = root / 'ClassroomStudent'
        shutil.copyfile(source, binary)
        binary.chmod(0o755)
        xvfb_pid = None
        if args.xvfb:
            desktop, xvfb_pid = xvfb_environment(root, args.user)
        else:
            desktop = desktop_environment(args.user)
        state = {'binary': str(binary), 'generation': 0, 'sha256': digest(binary),
                 'desktop': desktop, 'user': args.user, 'xvfb_pid': xvfb_pid,
                 'interface': args.interface,
                 'teacher_ip': args.teacher_ip, 'control_port': args.control_port}
        state_path.write_text(json.dumps(state))
        print(json.dumps(state))
        return
    if not state:
        raise RuntimeError('Run was not set up')
    if args.action == 'start':
        if alive(state):
            print(json.dumps(status(root, state)))
            return
        state['generation'] += 1
        state['log'] = f"student-{state['generation']}.log"
        profile = root / 'profile'
        profile.mkdir(exist_ok=True)
        account = pwd.getpwnam(state['user'])
        os.chown(profile, account.pw_uid, account.pw_gid)
        command = ['runuser', '-u', state['user'], '--', 'env',
                   *[f'{key}={value}' for key, value in state['desktop'].items()],
                   state['binary'], '--teacher-ip', state['teacher_ip'],
                   '--interface', state.get('interface', 'auto'), '--control-port', str(state['control_port']),
                   '--media-port', str(state['control_port'] + 10),
                   '--feedback-port', str(state['control_port'] + 11),
                   '--launch-profile', str(profile / 'profile.toml'), '--remember',
                   '--log-file', str(root / state['log']), '--share-seconds', '1200']
        with (root / f"student-{state['generation']}.stdout").open('w') as output:
            child = subprocess.Popen(command, stdout=output, stderr=subprocess.STDOUT,
                                     start_new_session=True)
        state['pid'] = child.pid
        state_path.write_text(json.dumps(state))
    elif args.action == 'stop':
        stop(state)
    elif args.action == 'logs':
        logs = {p.name: p.read_text(errors='replace')
                for p in root.glob('student-*.*')
                if p.suffix in ('.log', '.stdout')}
        if state.get('xvfb_pid') and args.stop_xvfb and not alive(state):
            try:
                os.killpg(state['xvfb_pid'], signal.SIGTERM)
            except ProcessLookupError:
                pass
        print(json.dumps(logs))
        return
    print(json.dumps(status(root, state)))


if __name__ == '__main__':
    main()
