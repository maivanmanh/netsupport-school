#!/usr/bin/env python3
"""Native Windows MFT single-VM recovery, isolated tasks/ports; not LAN acceptance."""
import argparse
import getpass
import importlib.util
import json
from pathlib import Path
import time

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('remote', ROOT / 'scripts/run-four-machine-test.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--remote', required=True)
    p.add_argument('--teacher', type=Path, required=True)
    p.add_argument('--student', type=Path, default=ROOT / 'target/rustup-windows/x86_64-pc-windows-gnu/release/ClassroomStudent.exe')
    p.add_argument('--output', type=Path, required=True)
    args = p.parse_args()
    run = args.output.resolve()
    run.mkdir(parents=True, exist_ok=False)
    remote = module.Remote(args.remote, getpass.getpass('Windows SSH password: '))
    task = 'Classroom-Loopback-Recovery-' + str(int(time.time()))
    folder = f'C:/Users/{args.remote.split("@")[0]}/{task}'
    remote.ps(f"New-Item -ItemType Directory -Force '{folder}' | Out-Null")
    remote.copy(args.teacher.resolve(), f'{args.remote}:{folder}/Teacher.exe')
    remote.copy(args.student.resolve(), f'{args.remote}:{folder}/Student.exe')
    config = (ROOT / 'config/default.toml').read_text().replace('47800', '48950').replace('47810', '48960').replace('47811', '48961')
    (run / 'config.toml').write_text(config)
    remote.copy(run / 'config.toml', f'{args.remote}:{folder}/config.toml')
    script = f'''$ErrorActionPreference='Stop'
$results=@()
foreach($case in @('sps','pps')) {{
    Remove-Item Env:CLASSROOM_TEST_DROP_FIRST_PARAMETER_SET -ErrorAction SilentlyContinue
    $env:CLASSROOM_TEST_DROP_PERCENT='0'
    $common=@('--loopback','--control-port','48950','--media-port','48960','--feedback-port','48961','--config','{folder}/config.toml')
    $t=$null; $s=$null
    try {{
        $a=@('--role','teacher','--bind-ip','127.0.0.1','--share-seconds','40','--launch-profile','{folder}/teacher-profile.toml','--log-file',"{folder}/$case-teacher.log")+$common
        $t=Start-Process '{folder}/Teacher.exe' -ArgumentList $a -PassThru -RedirectStandardOutput "{folder}/$case-teacher.stdout" -RedirectStandardError "{folder}/$case-teacher.stderr"
        Start-Sleep -Seconds 1
        $env:CLASSROOM_TEST_DROP_FIRST_PARAMETER_SET=$case
        $a=@('--role','student','--headless','--teacher-ip','127.0.0.1','--share-seconds','45','--launch-profile','{folder}/student-profile.toml','--log-file',"{folder}/$case-student.log")+$common
        $s=Start-Process '{folder}/Student.exe' -ArgumentList $a -PassThru -RedirectStandardOutput "{folder}/$case-student.stdout" -RedirectStandardError "{folder}/$case-student.stderr"
        $null=$t.Handle; $null=$s.Handle
        if(!$t.WaitForExit(70000)){{throw 'Teacher timeout'}}
        if(!$s.WaitForExit(70000)){{throw 'Student timeout'}}
        $results+=@{{scenario=$case;teacher_exit=$t.ExitCode;student_exit=$s.ExitCode}}
    }} finally {{
        foreach($child in @($t,$s)){{if($child -and !$child.HasExited){{$child.Kill()}}}}
    }}
}}
$results | ConvertTo-Json | Set-Content '{folder}/results.json'
'''
    (run / 'worker.ps1').write_text(script, encoding='utf-8-sig')
    remote.copy(run / 'worker.ps1', f'{args.remote}:{folder}/worker.ps1')
    try:
        remote.ps(f"$a=New-ScheduledTaskAction -Execute powershell.exe -Argument '-NoProfile -ExecutionPolicy Bypass -File {folder}/worker.ps1'; $p=New-ScheduledTaskPrincipal -UserId '{args.remote.split('@')[0]}' -LogonType Interactive -RunLevel Limited; $s=New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries; Register-ScheduledTask -TaskName '{task}' -Action $a -Principal $p -Settings $s -Force | Out-Null; Start-ScheduledTask '{task}'")
        end = time.monotonic() + 180
        while time.monotonic() < end:
            value = remote.ps(f"if(Test-Path '{folder}/results.json'){{Get-Content '{folder}/results.json' -Raw}}else{{'PENDING'}}")
            if '[' in value:
                results = json.loads(value[value.index('['):value.rindex(']') + 1])
                (run / 'results.json').write_text(json.dumps(results, indent=2))
                print(results, flush=True)
                break
            time.sleep(10)
        else:
            raise TimeoutError('Loopback diagnostic did not finish')
    finally:
        remote.ps(f"Stop-ScheduledTask '{task}' -ErrorAction SilentlyContinue; Unregister-ScheduledTask '{task}' -Confirm:$false -ErrorAction SilentlyContinue; Get-CimInstance Win32_Process | Where-Object {{$_.ExecutablePath -eq '{folder}/Teacher.exe'.Replace('/','\\') -or $_.ExecutablePath -eq '{folder}/Student.exe'.Replace('/','\\')}} | ForEach-Object {{Stop-Process -Id $_.ProcessId -ErrorAction SilentlyContinue}}")
        remote.copy(f'{args.remote}:{folder}', str(run / 'windows-raw'), recursive=True)
        # Retain remote raw evidence; installed roles/Guard and their profiles are untouched.
    for case in ('sps', 'pps'):
        logs = list((run / 'windows-raw').rglob(f'{case}-student.log'))
        if not logs or 'student first decoded frame' not in logs[0].read_text(errors='replace'):
            raise RuntimeError(f'{case}: no decoded output evidence')


if __name__ == '__main__':
    main()
