#!/usr/bin/env python3
"""Isolated Mac publisher / interactive Windows receiver. Password is memory-only."""
import argparse
import getpass
import hashlib
import json
import re
import shutil
from pathlib import Path
import subprocess
import time

import importlib.util

_remote_spec = importlib.util.spec_from_file_location("classroom_remote", Path(__file__).with_name("run-four-machine-test.py"))
_remote_module = importlib.util.module_from_spec(_remote_spec)
_remote_spec.loader.exec_module(_remote_module)
Remote = _remote_module.Remote

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--remote', required=True)
    parser.add_argument('--endpoint', type=Path)
    parser.add_argument('--local-ip', required=True)
    parser.add_argument('--seconds', type=int, default=50)
    parser.add_argument('--teacher', type=Path, default=ROOT / 'target/local-recovery-build/Classroom Console.app/Contents/Helpers/Classroom Agent.app/Contents/MacOS/Classroom')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--scenarios', default='baseline,loss,latejoin')
    args = parser.parse_args()
    if not 20 <= args.seconds <= 600:
        parser.error('seconds must be 20..600')
    selected = args.scenarios.split(',')
    if not selected or len(set(selected)) != len(selected) or not set(selected) <= {'baseline', 'loss', 'latejoin'}:
        parser.error('scenarios must be unique baseline/loss/latejoin names')
    remote = Remote(args.remote, getpass.getpass('Windows SSH password: '))
    run = args.output.resolve()
    run.mkdir(parents=True, exist_ok=False)
    task = 'Classroom-Recovery-' + str(int(time.time()))
    remote_root = f'C:/Users/{args.remote.split("@")[0]}/{task}'
    windows_binary = ROOT / 'target/rustup-windows/x86_64-pc-windows-gnu/release/Classroom.exe'
    mac_binary = args.teacher.resolve()
    manifest = {'scope': 'Mac Teacher / Windows ARM VM, isolated ports; no wired scale acceptance',
                'mac_sha256': hashlib.sha256(mac_binary.read_bytes()).hexdigest(),
                'windows_sha256': hashlib.sha256(windows_binary.read_bytes()).hexdigest(),
                'inventory': remote.ps('quser; $env:PROCESSOR_ARCHITECTURE'), 'remote_root': remote_root}
    (run / 'manifest.json').write_text(json.dumps(manifest, indent=2))
    remote.ps(f"New-Item -ItemType Directory -Force '{remote_root}' | Out-Null")
    remote.copy(windows_binary, f'{args.remote}:{remote_root}/Classroom.exe')
    config = (ROOT / 'config/default.toml').read_text().replace('47800', '48900').replace('47810', '48910').replace('47811', '48911')
    (run / 'config.toml').write_text(config)
    remote.copy(run / 'config.toml', f'{args.remote}:{remote_root}/config.toml')
    results = {}
    ipc = None
    if args.endpoint:
        spec = importlib.util.spec_from_file_location('classroom_ipc', ROOT / 'scripts/classroom-dev-update.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        ipc = module.IPC(args.endpoint)
    try:
        for scenario, drop, delay in [('baseline', 0, 0), ('loss', 100, 0), ('latejoin', 0, 8000)]:
            if scenario not in selected:
                continue
            print('START', scenario, flush=True)
            powershell = f'''$ErrorActionPreference='Stop'
$env:CLASSROOM_TEST_DROP_PERCENT='{1 if drop else 0}'
$a=@('--role','student','--teacher-ip','{args.local_ip}','--control-port','48900','--media-port','48910','--feedback-port','48911','--share-seconds','{args.seconds + 10}','--student-connect-delay-ms','{delay}','--config','{remote_root}/config.toml','--launch-profile','{remote_root}/profile.toml','--log-file','{remote_root}/{scenario}.log')
$p=Start-Process '{remote_root}/Classroom.exe' -ArgumentList $a -PassThru -RedirectStandardOutput '{remote_root}/{scenario}.stdout' -RedirectStandardError '{remote_root}/{scenario}.stderr'
$null=$p.Handle
if(!$p.WaitForExit({(args.seconds + 60) * 1000})){{$p.Kill(); throw 'Student timeout'}}
@{{exit=$p.ExitCode}} | ConvertTo-Json | Set-Content '{remote_root}/{scenario}.result.json'
'''
            worker = run / 'worker.ps1'
            worker.write_text(powershell, encoding='utf-8-sig')
            remote.copy(worker, f'{args.remote}:{remote_root}/worker.ps1')
            remote.ps(f"$a=New-ScheduledTaskAction -Execute powershell.exe -Argument '-NoProfile -ExecutionPolicy Bypass -File {remote_root}/worker.ps1'; $p=New-ScheduledTaskPrincipal -UserId '{args.remote.split('@')[0]}' -LogonType Interactive -RunLevel Limited; $s=New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries; Register-ScheduledTask -TaskName '{task}' -Action $a -Principal $p -Settings $s -Force | Out-Null; Start-ScheduledTask '{task}'")
            workload = subprocess.Popen([str(ROOT / 'target/phase1-mac-workload'), str(args.seconds + 10), str(run / f'{scenario}-workload.json')])
            time.sleep(2)
            teacher = None
            try:
                if ipc:
                    deadline = time.monotonic() + 20
                    while delay == 0 and time.monotonic() < deadline:
                        time.sleep(0.05)
                        ipc.read()
                        if any(row['online'] for row in ipc.state.get('students', [])):
                            break
                    if delay == 0 and not any(row['online'] for row in ipc.state.get('students', [])):
                        raise TimeoutError('Student did not register in isolated room')
                    ipc.command('share_class')
                    end = time.monotonic() + args.seconds
                    saw_sharing = False
                    while time.monotonic() < end:
                        time.sleep(0.05)
                        ipc.read()
                        if ipc.state.get('phase') == 'sharing':
                            saw_sharing = True
                        if ipc.state.get('error'):
                            raise RuntimeError('Teacher media failed: ' + ipc.state['error'])
                    if not saw_sharing:
                        raise RuntimeError('Teacher never reached sharing')
                    ipc.command('stop')
                    results[scenario] = {'teacher_mode': 'Flutter-parented agent', 'loss_injection_percent': 1 if drop else 0}
                else:
                    with (run / f'{scenario}-teacher.stdout').open('w') as log:
                        teacher = subprocess.Popen([str(mac_binary), '--role', 'teacher', '--bind-ip', args.local_ip,
                            '--control-port', '48900', '--media-port', '48910', '--feedback-port', '48911',
                            '--share-seconds', str(args.seconds), '--drop-every-nth-rtp-packet', str(drop),
                            '--config', str(run / 'config.toml'), '--launch-profile', str(run / 'teacher-profile.toml'),
                            '--log-file', str(run / f'{scenario}-teacher.log')], stdout=log, stderr=subprocess.STDOUT)
                    teacher.wait(timeout=args.seconds + 25)
                    results[scenario] = {'teacher_exit': teacher.returncode}
                    if teacher.returncode != 0:
                        raise RuntimeError('Teacher failed; inspect preserved log before more scenarios')
                deadline = time.monotonic() + 25
                while time.monotonic() < deadline:
                    result = remote.ps(f"if(Test-Path '{remote_root}/{scenario}.result.json'){{Get-Content '{remote_root}/{scenario}.result.json' -Raw}}else{{'PENDING'}}")
                    if '{' in result:
                        results[scenario]['student'] = json.loads(result[result.index('{'):result.rindex('}') + 1])
                        break
                    time.sleep(1)
                else:
                    raise TimeoutError('Student result not written')
            finally:
                for child in (teacher, workload):
                    if child is not None and child.poll() is None:
                        child.terminate()
                        child.wait(timeout=10)
                (run / 'results.json').write_text(json.dumps(results, indent=2))
            remote.copy(f'{args.remote}:{remote_root}/{scenario}.log', str(run / f'{scenario}-student.log'))
            text = (run / f'{scenario}-student.log').read_text(errors='replace')
            results[scenario]['presented'] = 'student first presented frame' in text
            results[scenario]['max_decoded_frames'] = max([int(x) for x in re.findall(r'\bdecoded_frames=(\d+)', text)] or [0])
            (run / 'results.json').write_text(json.dumps(results, indent=2))
            if not results[scenario]['presented'] or results[scenario]['max_decoded_frames'] == 0:
                raise RuntimeError('Student has no verified decode/render output')
            print('END', scenario, results[scenario], flush=True)
    finally:
        if ipc:
            teacher_log = Path.home() / 'Library/Containers/com.classroom.classroomConsole/Data/Library/Application Support/com.classroom.classroomConsole/teacher-log.txt'
            if teacher_log.exists():
                shutil.copyfile(teacher_log, run / 'teacher.log')
            try:
                ipc.command('stop')
                ipc.command('quit')
            except (EOFError, OSError):
                pass
            finally:
                ipc.socket.close()
        remote.ps(f"Stop-ScheduledTask '{task}' -ErrorAction SilentlyContinue; Unregister-ScheduledTask '{task}' -Confirm:$false -ErrorAction SilentlyContinue; Get-CimInstance Win32_Process | Where-Object {{$_.ExecutablePath -eq '{remote_root}/Classroom.exe'.Replace('/','\\')}} | ForEach-Object {{Stop-Process -Id $_.ProcessId -ErrorAction SilentlyContinue}}")
        remote.copy(f'{args.remote}:{remote_root}', str(run / 'windows-raw'), recursive=True)
        # Keep remote raw evidence and installed Student untouched.


if __name__ == '__main__':
    main()
