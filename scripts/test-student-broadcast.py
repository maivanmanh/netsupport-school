#!/usr/bin/env python3
"""Two native Windows endpoints, Flutter-parented Teacher, isolated test tasks.

Functional relay/health/diagnostic evidence only; not 40-PC acceptance.
Passwords stay in memory. Installed Students, Guard and settings are preserved.
"""
import argparse
import getpass
import hashlib
import importlib.util
import json
import subprocess
import shutil
import socket
from pathlib import Path
import time
import zipfile

ROOT = Path(__file__).resolve().parents[1]

def module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--source', required=True)
    parser.add_argument('--receiver', required=True)
    parser.add_argument('--local-ip', required=True)
    parser.add_argument('--endpoint', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--teacher-log', type=Path, required=True)
    parser.add_argument('--teacher', help='Optional Windows Teacher SSH address; native Rust agent, not Flutter GUI')
    args = parser.parse_args()
    if not args.teacher and not args.endpoint.is_file():
        parser.error('Teacher IPC endpoint is missing; launch the paired Console first')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    remote_module = module('remote', ROOT / 'scripts/run-four-machine-test.py')
    # Normal first-use SSH host trust; changed known host keys still fail.
    remote_module.OPTS += ['-o', 'StrictHostKeyChecking=accept-new']
    password = getpass.getpass('Windows SSH password: ')
    peers = [(name, remote_module.Remote(address, password), address)
             for name, address in [('source', args.source), ('receiver', args.receiver)]]
    binary = ROOT / 'target/rustup-windows/x86_64-pc-windows-gnu/release/ClassroomStudent.exe'
    task = 'Classroom-Broadcast-' + str(int(time.time()))
    roots = {}
    owned = []
    client = None
    samples = []
    tunnel = None
    teacher_remote = None
    manifest = {'scope': 'Two Windows endpoints / Mac coordinator; functional only',
                'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                'source': args.source, 'receiver': args.receiver,
                'teacher': args.teacher or 'Mac Flutter parent',
                'receiver_colocated_with_teacher': bool(args.teacher and args.teacher == args.receiver)}
    (output / 'manifest.json').write_text(json.dumps(manifest, indent=2))
    try:
        if args.teacher:
            teacher_remote = remote_module.Remote(args.teacher, password)
            root = f'C:/Users/{args.teacher.split("@")[0]}/{task}-teacher'
            roots['teacher'] = root
            teacher_binary = ROOT / 'target/rustup-windows/x86_64-pc-windows-gnu/release/Classroom.exe'
            package = ROOT / 'target/updates/student-broadcast-batch-windows-x64.zip'
            # A native Teacher test folder also includes its fresh Student ZIP.
            with zipfile.ZipFile(package) as archive:
                assert hashlib.sha256(archive.read('ClassroomStudent.exe')).hexdigest() == manifest['binary_sha256']
            teacher_remote.ps(f"New-Item -ItemType Directory -Force '{root}/student-packages' | Out-Null")
            owned.append(('teacher', teacher_remote, args.teacher))
            teacher_remote.copy(teacher_binary, f'{args.teacher}:{root}/Classroom.exe')
            teacher_remote.copy(package, f'{args.teacher}:{root}/student-packages/student-windows-x64.zip')
            allowed = ','.join("'" + address.split('@')[1] + "'" for _, _, address in peers)
            for protocol in ['TCP', 'UDP']:
                teacher_remote.ps(f"New-NetFirewallRule -Name '{task}-teacher-{protocol}' -DisplayName '{task}-teacher-{protocol}' -Program '{root}/Classroom.exe'.Replace('/','\\') -Direction Inbound -Protocol {protocol} -RemoteAddress {allowed} -Action Allow -Profile Any | Out-Null")
            arguments = f'--role teacher --teacher-agent --bind-ip {args.local_ip} --control-port 48900 --media-port 48910 --feedback-port 48911 --ipc-file {root}/endpoint.json --launch-profile {root}/profile.toml --log-file {root}/student.log'
            teacher_remote.ps(f"$a=New-ScheduledTaskAction -Execute '{root}/Classroom.exe' -Argument '{arguments}'; $p=New-ScheduledTaskPrincipal -UserId '{args.teacher.split('@')[0]}' -LogonType Interactive -RunLevel Limited; $s=New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries; Register-ScheduledTask -TaskName '{task}-teacher' -Action $a -Principal $p -Settings $s -Force | Out-Null; Start-ScheduledTask '{task}-teacher'")
            teacher_remote.ps(f"$until=(Get-Date).AddSeconds(20); while (-not(Test-Path '{root}/endpoint.json')) {{if((Get-Date) -gt $until){{throw 'Teacher endpoint timeout'}}; Start-Sleep -Milliseconds 100}}")
            teacher_remote.copy(f'{args.teacher}:{root}/endpoint.json', str(args.endpoint))
            descriptor = json.loads(args.endpoint.read_text())
            with socket.socket() as reserve:
                reserve.bind(('127.0.0.1', 0))
                local_port = reserve.getsockname()[1]
            tunnel = remote_module.RemoteProcess(['ssh', *remote_module.OPTS,
                '-o', 'ExitOnForwardFailure=yes', '-N', '-L',
                f'127.0.0.1:{local_port}:127.0.0.1:{descriptor["port"]}', args.teacher], password)
            deadline = time.monotonic() + 10
            while True:
                tunnel.pump()
                if tunnel.code is not None:
                    raise RuntimeError('Teacher SSH tunnel closed before IPC connection')
                try:
                    with socket.create_connection(('127.0.0.1', local_port), timeout=.2):
                        break
                except OSError:
                    if time.monotonic() >= deadline:
                        raise TimeoutError('Teacher SSH tunnel did not become ready')
                    time.sleep(.05)
            descriptor['port'] = local_port
            args.endpoint.write_text(json.dumps(descriptor))
        for name, remote, address in peers:
            root = f'C:/Users/{address.split("@")[0]}/{task}-{name}'
            roots[name] = root
            inventory = remote.ps("$ProgressPreference='SilentlyContinue'; hostname; quser; $env:PROCESSOR_ARCHITECTURE")
            (output / f'{name}-inventory.txt').write_text(inventory)
            remote.ps(f"New-Item -ItemType Directory -Force '{root}' | Out-Null")
            owned.append((name, remote, address))
            remote.copy(binary, f'{address}:{root}/Classroom.exe')
            # Isolated executable has no installed-app firewall allowance.
            # Restrict inbound access to the named Teacher and remove on exit.
            for protocol in ['TCP', 'UDP']:
                remote.ps(f"New-NetFirewallRule -Name '{task}-{name}-{protocol}' -DisplayName '{task}-{name}-{protocol}' -Program '{root}/Classroom.exe'.Replace('/','\\') -Direction Inbound -Protocol {protocol} -RemoteAddress '{args.local_ip}' -Action Allow -Profile Any | Out-Null")
            config = (ROOT / 'config/default.toml').read_text().replace('47800', '48900').replace('47810', '48910').replace('47811', '48911')
            (output / 'config.toml').write_text(config)
            remote.copy(output / 'config.toml', f'{address}:{root}/config.toml')
            arguments = f'--role student --teacher-ip {args.local_ip} --control-port 48900 --media-port 48910 --feedback-port 48911 --config {root}/config.toml --launch-profile {root}/profile.toml --log-file {root}/student.log'
            remote.ps(f"$a=New-ScheduledTaskAction -Execute '{root}/Classroom.exe' -Argument '{arguments}'; $p=New-ScheduledTaskPrincipal -UserId '{address.split('@')[0]}' -LogonType Interactive -RunLevel Limited; $s=New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries; Register-ScheduledTask -TaskName '{task}-{name}' -Action $a -Principal $p -Settings $s -Force | Out-Null")
        client = module('ipc', ROOT / 'scripts/classroom-dev-update.py').IPC(args.endpoint)

        def wait_for(predicate, seconds=25):
            deadline = time.monotonic() + seconds
            while time.monotonic() < deadline:
                client.read()
                state = client.state
                samples.append({'time': time.time(), 'phase': state.get('phase'), 'error': state.get('error'),
                                'source_id': state.get('source_id'), 'students': [
                                    {k: r.get(k) for k in ['id', 'name', 'ip', 'online', 'media']}
                                    for r in state.get('students', [])]})
                if state.get('error'):
                    raise RuntimeError(state['error'])
                if predicate(state):
                    return state
            raise TimeoutError('Teacher state predicate timed out')

        source_ip = args.source.split('@')[1]
        receiver_ip = args.receiver.split('@')[1]
        peers[0][1].ps(f"Start-ScheduledTask '{task}-source'")
        state = wait_for(lambda s: any(r['online'] and r['ip'] == source_ip for r in s.get('students', [])))
        source_id = next(r['id'] for r in state['students'] if r['online'] and r['ip'] == source_ip)
        print('START Student source; receiver will late join', flush=True)
        client.command('share_student', [source_id])
        wait_for(lambda s: s.get('phase') == 'sharing')
        peers[1][1].ps(f"Start-ScheduledTask '{task}-receiver'")
        wait_for(lambda s: any(r['ip'] == receiver_ip and r.get('media', {}).get('state') == 'receiving' for r in s.get('students', [])), 35)
        receiver_id = next(r['id'] for r in client.state['students'] if r['online'] and r['ip'] == receiver_ip)
        end = time.monotonic() + 20
        wait_for(lambda s: time.monotonic() >= end)
        print('RECONNECT receiver while Student source remains live', flush=True)
        peers[1][1].ps(f"Stop-ScheduledTask '{task}-receiver'; Get-CimInstance Win32_Process | Where-Object {{$_.ExecutablePath -eq '{roots['receiver']}/Classroom.exe'.Replace('/','\\')}} | ForEach-Object {{Stop-Process -Id $_.ProcessId}}")
        wait_for(lambda s: any(r['id'] == receiver_id and not r['online'] for r in s.get('students', [])))
        peers[1][1].ps(f"Start-ScheduledTask '{task}-receiver'")
        wait_for(lambda s: any(r['id'] == receiver_id and r.get('media', {}).get('state') == 'receiving' for r in s.get('students', [])), 35)
        client.command('collect_diagnostics', [source_id, receiver_id])
        wait_for(lambda s: s.get('diagnostics', {}).get('phase') == 'waiting_for_stop')
        print('STOP and collect both native Student logs', flush=True)
        client.command('stop')
        done = wait_for(lambda s: s.get('diagnostics', {}).get('phase') == 'complete', 60)
        result = done['diagnostics']
        (output / 'diagnostics-result.json').write_text(json.dumps(result, indent=2))
        archive_path = result['path']
        if teacher_remote:
            archive_path = output / 'classroom-diagnostics.zip'
            teacher_remote.copy(f'{args.teacher}:{result["path"]}', str(archive_path))
        with zipfile.ZipFile(archive_path) as archive:
            entries = archive.namelist()
            evidence = json.loads(archive.read('manifest.json'))
        assert result['collected'] == 2, result
        assert len([p for p in entries if p.startswith('students/') and p.endswith('/log.txt')]) == 2
        assert all(r['status'] == 'collected' for r in evidence['students'])
        assert done['phase'] == 'idle'
        # Repeat with a new stream ID to catch stale report/SSRC state.
        client.command('share_student', [source_id])
        wait_for(lambda s: any(r['id'] == receiver_id and r.get('media', {}).get('state') == 'receiving' for r in s.get('students', [])), 35)
        client.command('stop')
        wait_for(lambda s: s.get('phase') == 'idle')
        print('PASS late join, reconnect, restart, receiver health, diagnostic ZIP', flush=True)
    except Exception:
        if client and 'source' in roots:
            root = roots['source']
            value = peers[0][1].ps(f"$p=Get-CimInstance Win32_Process | Where-Object {{$_.ExecutablePath -eq '{root}/Classroom.exe'.Replace('/','\\')}}; @(Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object {{$_.OwningProcess -in $p.ProcessId}} | Select-Object LocalAddress,LocalPort) | ConvertTo-Json -Compress; exit 0")
            (output / 'source-listeners.txt').write_text(value)
            # Compare direct host connectivity with the packaged app failure.
            import re
            ports = re.findall(r'"LocalPort":(\d+)', value)
            for port in ports:
                probe = subprocess.run(['nc', '-vz', '-G', '3', args.source.split('@')[1], port], capture_output=True, text=True)
                (output / f'tcp-probe-{port}.txt').write_text(probe.stdout + probe.stderr)
        raise
    finally:
        (output / 'states.json').write_text(json.dumps(samples, indent=2))
        if not args.teacher and args.teacher_log.is_file():
            shutil.copy2(args.teacher_log, output / 'teacher.log')
        if client:
            try:
                client.command('stop')
                client.command('quit')
            except (OSError, EOFError, RuntimeError):
                pass
            client.socket.close()
        if tunnel:
            tunnel.stop()
        for name, remote, address in owned:
            root = roots[name]
            try:
                remote.ps(f"Stop-ScheduledTask '{task}-{name}' -ErrorAction SilentlyContinue; Unregister-ScheduledTask '{task}-{name}' -Confirm:$false -ErrorAction SilentlyContinue; Get-CimInstance Win32_Process | Where-Object {{$_.ExecutablePath -eq '{root}/Classroom.exe'.Replace('/','\\')}} | ForEach-Object {{Stop-Process -Id $_.ProcessId -ErrorAction SilentlyContinue}}")
                remote.ps(f"Remove-NetFirewallRule -Name '{task}-{name}-TCP','{task}-{name}-UDP' -ErrorAction SilentlyContinue")
            except Exception as error:
                print(f'{name} cleanup/evidence: {error}', flush=True)
            try:
                filename = 'teacher.log' if name == 'teacher' else f'{name}-student.log'
                remote.copy(f'{address}:{root}/student.log', str(output / filename))
            except Exception as error:
                print(f'{name} log collection: {error}', flush=True)

if __name__ == '__main__':
    main()
