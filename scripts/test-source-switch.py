#!/usr/bin/env python3
"""One Mac Teacher, Windows x64 Student and Mac dev Student; isolated LAN test.

Prepare packages once, grant macOS permissions manually, then run handoffs.
Credentials stay in memory. No privacy automation or production replacement.
"""
import argparse
import getpass
import hashlib
import importlib.util
import json
import plistlib
import shlex
import time
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--prepare', action='store_true')
    parser.add_argument('--windows', default='mvmanh@192.168.100.102')
    parser.add_argument('--mac', default='lolem@192.168.100.254')
    parser.add_argument('--local-ip', required=True)
    parser.add_argument('--endpoint', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--teacher-log', type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    remote_module = module('remote', ROOT / 'scripts/run-four-machine-test.py')
    remote_module.OPTS += ['-o', 'StrictHostKeyChecking=accept-new']
    windows = remote_module.Remote(args.windows, getpass.getpass('Windows SSH password: '))
    mac = remote_module.Remote(args.mac, getpass.getpass('Mac Student SSH password: '))
    wuser = args.windows.split('@')[0]
    muser = args.mac.split('@')[0]
    mac_root = f'/Users/{muser}/Classroom-source-switch'
    mac_exe = mac_root + '/Classroom Student.app/Contents/MacOS/ClassroomStudent'
    package_root = ROOT / 'target/student-broadcast-build/Classroom Console.app/Contents/Resources/student-packages'
    if args.prepare:
        output.mkdir(parents=True, exist_ok=False)
        label = 'Classroom-Source-Switch-' + str(int(time.time()))
        win_root = f'C:/Users/{wuser}/{label}'
        manifest = {'label': label, 'local_ip': args.local_ip, 'windows': args.windows,
                    'mac': args.mac, 'mac_root': mac_root, 'win_root': win_root,
                    'scope': 'Mixed VM/dev endpoints; not physical x64 Ethernet acceptance',
                    'packages': {}}
        for name in ['student-windows-x64.zip', 'student-macos-arm64.zip']:
            with zipfile.ZipFile(package_root / name) as archive:
                meta = json.loads(archive.read('manifest.json'))
                assert hashlib.sha256(archive.read(meta['executable'])).hexdigest() == meta['sha256']
                manifest['packages'][name] = meta
        assert len({p['version'] for p in manifest['packages'].values()}) == 1
        # New isolated folders only. Refuse replacement instead of deleting an
        # installed/running Student or invalidating an already granted bundle.
        mac.ssh(f'test ! -e {shlex.quote(mac_root)} && mkdir {shlex.quote(mac_root)}')
        windows.ps(f"$ErrorActionPreference='Stop'; if(Test-Path '{win_root}'){{throw 'Test folder already exists'}}; New-Item -ItemType Directory '{win_root}' | Out-Null")
        mac.copy(package_root / 'student-macos-arm64.zip', f'{args.mac}:{mac_root}/student.zip')
        mac.ssh(f'ditto -xk {shlex.quote(mac_root + "/student.zip")} {shlex.quote(mac_root)}; codesign --verify --deep --strict {shlex.quote(mac_root + "/Classroom Student.app")}')
        binary = ROOT / 'target/rustup-windows/x86_64-pc-windows-gnu/release/ClassroomStudent.exe'
        windows.copy(binary, f'{args.windows}:{win_root}/Classroom.exe')
        actual = windows.ps(f"(Get-FileHash '{win_root}/Classroom.exe' -Algorithm SHA256).Hash").lower()
        assert manifest['packages']['student-windows-x64.zip']['sha256'] in actual
        actual = mac.ssh('shasum -a256 ' + shlex.quote(mac_exe))
        assert manifest['packages']['student-macos-arm64.zip']['sha256'] in actual
        (output / 'manifest.json').write_text(json.dumps(manifest, indent=2))
        # Read-only check from the GUI login session. It neither requests nor
        # changes Recording/Accessibility/Local Network permissions.
        # Owner requests removing a stale installed Student before new LAN builds.
        migration_result = windows.remove_old_student(f'{win_root}/Classroom.exe', manifest['packages']['student-windows-x64.zip']['sha256'])
        (output / 'windows-old-install-removal.txt').write_text(migration_result)
        check_label = 'com.classroom.test.source-switch.permission-check'
        check = {'Label': check_label, 'ProgramArguments': [mac_exe, '--permission-probe-network', args.local_ip + ':48900'],
                 'RunAtLoad': True, 'StandardOutPath': mac_root + '/permission-status.json',
                 'StandardErrorPath': mac_root + '/permission-check.err'}
        check_file = output / 'permission-check.plist'
        check_file.write_bytes(plistlib.dumps(check))
        mac.copy(check_file, f'{args.mac}:{mac_root}/permission-check.plist')
        try:
            mac.ssh('launchctl bootstrap gui/$(id -u) ' + shlex.quote(mac_root + '/permission-check.plist'))
            text = mac.ssh('python3 -c ' + shlex.quote(
                "import pathlib,time; p=pathlib.Path(" + repr(mac_root + '/permission-status.json') + "); "
                "\nfor _ in range(75):\n if p.exists() and p.stat().st_size: break\n time.sleep(.2)\nprint(p.read_text())"))
            status = json.JSONDecoder().raw_decode(text[text.index('{'):])[0]
            (output / 'mac-permission-status.json').write_text(json.dumps(status, indent=2))
            print('Prepared verified paired packages. Mac Student Recording:', status.get('screen'))
            print('Manual permission target:', mac_root + '/Classroom Student.app')
        finally:
            mac.ssh('launchctl bootout gui/$(id -u)/' + check_label + ' 2>/dev/null || true')
        return

    manifest = json.loads((output / 'manifest.json').read_text())
    assert all(manifest[k] == getattr(args, k) for k in ['windows', 'mac', 'local_ip'])
    assert manifest['mac_root'] == mac_root
    label, win_root = manifest['label'], manifest['win_root']
    job = 'com.classroom.test.source-switch.student'
    ipc = None
    samples, events = [], []
    stage = 'setup'
    active_windows = active_mac = False
    config = (ROOT / 'config/default.toml').read_text().replace('47800', '48900').replace('47810', '48910').replace('47811', '48911')
    (output / 'config.toml').write_text(config)

    def wait_for(predicate, seconds=40, allow_error=False):
        end = time.monotonic() + seconds
        while time.monotonic() < end:
            ipc.read()
            state = ipc.state
            samples.append({'time': time.time(), 'stage': stage, **{k: state.get(k) for k in
                            ['phase', 'error', 'stream_id', 'source_id', 'next_source']},
                            'observe': {k: (state.get('observe') or {}).get(k) for k in
                                        ['id', 'phase', 'mode', 'class_control']},
                            'students': [{k: r.get(k) for k in ['id', 'ip', 'online', 'media']}
                                         for r in state.get('students', [])]})
            if state.get('error') and not allow_error:
                raise RuntimeError(state['error'])
            if predicate(state):
                return state
        raise TimeoutError('State wait expired at ' + stage)

    def receiving(state, source):
        recipients = [r for r in state.get('students', []) if r['online'] and r['id'] != source]
        return bool(recipients) and all(r.get('media', {}).get('state') == 'receiving'
                                     and (r['media'].get('render_fps') or 0) > 0 for r in recipients)

    try:
        ipc = module('ipc', ROOT / 'scripts/classroom-dev-update.py').IPC(args.endpoint)
        assert ipc.state['phase'] == 'idle', 'Stop the active presentation before this test'
        assert ipc.state['teacher_ip'] == args.local_ip and ipc.state['control_port'] == 48900
        for remote, address, root in [(windows, args.windows, win_root), (mac, args.mac, mac_root)]:
            remote.copy(output / 'config.toml', f'{address}:{root}/config.toml')
        active_windows = True
        for protocol in ['TCP', 'UDP']:
            windows.ps(f"New-NetFirewallRule -Name '{label}-{protocol}' -DisplayName '{label}-{protocol}' -Program '{win_root}/Classroom.exe'.Replace('/','\\') -Direction Inbound -Protocol {protocol} -RemoteAddress '{args.local_ip}' -Action Allow -Profile Any | Out-Null")
        arguments = f'--teacher-ip {args.local_ip} --control-port 48900 --media-port 48910 --feedback-port 48911 --config {win_root}/config.toml --launch-profile {win_root}/profile.toml --log-file {win_root}/student.log'
        windows.ps(f"$a=New-ScheduledTaskAction -Execute '{win_root}/Classroom.exe' -Argument '{arguments}'; $p=New-ScheduledTaskPrincipal -UserId '{wuser}' -LogonType Interactive -RunLevel Limited; $s=New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries; Register-ScheduledTask -TaskName '{label}' -Action $a -Principal $p -Settings $s -Force | Out-Null; Start-ScheduledTask '{label}'")
        launch = {'Label': job, 'ProgramArguments': [mac_exe, '--teacher-ip', args.local_ip,
                  '--control-port', '48900', '--media-port', '48910', '--feedback-port', '48911',
                  '--config', mac_root + '/config.toml', '--launch-profile', mac_root + '/profile.toml',
                  '--log-file', mac_root + '/student.log'], 'RunAtLoad': True,
                  'ProcessType': 'Interactive', 'LegacyTimers': True,
                  'KeepAlive': {'SuccessfulExit': False},
                  'StandardOutPath': mac_root + '/stdout.log', 'StandardErrorPath': mac_root + '/stderr.log'}
        (output / 'student.plist').write_bytes(plistlib.dumps(launch))
        mac.copy(output / 'student.plist', f'{args.mac}:{mac_root}/student.plist')
        active_mac = True
        mac.ssh('launchctl bootstrap gui/$(id -u) ' + shlex.quote(mac_root + '/student.plist'))
        state = wait_for(lambda s: sum(r['online'] and r['ip'] in [args.windows.split('@')[1], args.mac.split('@')[1]] for r in s['students']) == 2)
        source_ids = {r['ip']: r['id'] for r in state['students'] if r['online']}
        win_id = source_ids[args.windows.split('@')[1]]
        mac_id = source_ids[args.mac.split('@')[1]]
        old_surface = None
        for cycle in range(2):
            for source in [None, win_id, mac_id, win_id, None]:
                stage = f'cycle-{cycle}-' + ('teacher' if source is None else ('windows' if source == win_id else 'mac'))
                previous_stream = ipc.state['stream_id']
                start = time.monotonic()
                ipc.command('switch_source', [] if source is None else [source])
                state = wait_for(lambda s: s['phase'] == 'sharing' and s['source_id'] == source and
                                 s['stream_id'] != previous_stream and receiving(s, source) and
                                 (source is None or ((s.get('observe') or {}).get('mode') == 'control' and s['observe'].get('class_control') is True)))
                events.append({'stage': stage, 'source': source, 'stream_id': state['stream_id'],
                               'receiving_after_seconds': time.monotonic() - start})
                print('PASS', stage, events[-1]['receiving_after_seconds'], flush=True)
                # Current-source selection is a no-op. Teacher class target at
                # the end of one cycle is stopped below before the next cycle.
                current = state['stream_id']
                ipc.command('switch_source', [] if source is None else [source])
                wait_for(lambda s: s['phase'] == 'sharing' and receiving(s, source))
                assert ipc.state['stream_id'] == current
                for invalid in [['offline-source'], [win_id, mac_id]]:
                    try:
                        ipc.command('switch_source', invalid)
                    except RuntimeError:
                        pass
                    else:
                        raise AssertionError('Invalid source was accepted')
                    wait_for(lambda s: s['phase'] == 'sharing' and receiving(s, source))
                    assert ipc.state['stream_id'] == current
                if source == win_id:
                    if old_surface:
                        ipc.command('observe_close', [win_id], {'surface': old_surface})
                        wait_for(lambda s: s['phase'] == 'sharing' and receiving(s, source))
                        assert ipc.state['stream_id'] == current
                    old_surface = ipc.state['observe'].get('surface')
            stage = f'cancel-{cycle}'
            ipc.command('switch_source', [mac_id])
            ipc.command('stop')
            wait_for(lambda s: s['phase'] == 'idle' and s.get('next_source') is None and s.get('source_id') is None)
            until = time.monotonic() + 2
            wait_for(lambda s: time.monotonic() >= until)
            assert ipc.state['phase'] == 'idle'
        print('PASS source handoffs, invalid/current targets, stale close and Stop cancellation', flush=True)
    finally:
        (output / 'states.json').write_text(json.dumps(samples, indent=2))
        (output / 'events.json').write_text(json.dumps(events, indent=2))
        if args.teacher_log.is_file():
            (output / 'teacher.log').write_bytes(args.teacher_log.read_bytes())
        if ipc:
            try:
                ipc.command('stop')
            finally:
                ipc.socket.close()
        cleanup_errors = []
        for name in ['windows', 'mac']:
            try:
                if name == 'windows' and active_windows:
                    windows.ps(f"Stop-ScheduledTask '{label}' -ErrorAction SilentlyContinue; Unregister-ScheduledTask '{label}' -Confirm:$false -ErrorAction SilentlyContinue; Get-CimInstance Win32_Process | Where-Object {{$_.ExecutablePath -eq '{win_root}/Classroom.exe'.Replace('/','\\')}} | ForEach-Object {{Stop-Process -Id $_.ProcessId -ErrorAction SilentlyContinue}}; Remove-NetFirewallRule -Name '{label}-TCP','{label}-UDP' -ErrorAction SilentlyContinue; exit 0")
                    exists = windows.ps(f"if(Test-Path '{win_root}/student.log'){{'exists'}}")
                    if 'exists' in exists:
                        windows.copy(f'{args.windows}:{win_root}/student.log', str(output / 'windows-student.log'))
                if name == 'mac' and active_mac:
                    mac.ssh('launchctl bootout gui/$(id -u)/' + job + ' 2>/dev/null || true')
                    exists = mac.ssh('test -f ' + shlex.quote(mac_root + '/student.log') + ' && printf exists || true')
                    if 'exists' in exists:
                        mac.copy(f'{args.mac}:{mac_root}/student.log', str(output / 'mac-student.log'))
            except Exception as error:
                cleanup_errors.append(name + ': ' + str(error))
        (output / 'cleanup.json').write_text(json.dumps({'errors': cleanup_errors}, indent=2))
        if cleanup_errors:
            raise RuntimeError('Owned test cleanup needs inspection; see cleanup.json')


if __name__ == '__main__':
    main()
