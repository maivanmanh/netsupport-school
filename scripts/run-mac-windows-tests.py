#!/usr/bin/env python3
"""Interactive-session Windows media checks. Password stays in process memory.

These VM/same-host checks never imply physical 40-PC acceptance. Only owned
processes/tasks are stopped. Full logs are retained even when checks fail.
"""
import argparse
import base64
import errno
import getpass
import hashlib
import json
import os
from pathlib import Path
import pty
import select
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--remote', required=True)
    parser.add_argument('--local-ip', required=True)
    parser.add_argument('--scenarios', default='capture,local,mac-baseline,win-baseline,mac-loss,win-loss,mac-latejoin,win-latejoin,mac-disconnect,win-disconnect,mac-reconnect,win-reconnect,mac-fanout3,win-pressure,mac-cpu,benchmark')
    parser.add_argument('--seconds', type=int, default=35)
    args = parser.parse_args()
    password = getpass.getpass('Windows SSH password (not saved): ')
    run = ROOT / 'target/mac-windows-runs' / (time.strftime('%Y%m%d-%H%M%S') + f'-{os.getpid()}')
    run.mkdir(parents=True)
    remote_ip = args.remote.split('@')[-1]
    username = args.remote.split('@')[0]
    remote_root = f'C:/Users/{username}/Classroom-phase1-{run.name}'
    task = f'Classroom-Phase1-{run.name}'
    owned = []

    def command(argv, timeout=40):
        pid, fd = pty.fork()
        if pid == 0:
            os.execvp(argv[0], argv)
        output = bytearray(); auth_buffer = bytearray(); deadline = time.monotonic() + timeout
        try:
            while True:
                if time.monotonic() > deadline:
                    os.kill(pid, 15)
                    os.waitpid(pid, 0)
                    raise TimeoutError(f'{argv[0]} timeout')
                if not select.select([fd], [], [], 0.2)[0]:
                    continue
                try:
                    chunk = os.read(fd, 65536)
                except OSError as error:
                    if error.errno == errno.EIO:
                        break
                    raise
                if not chunk:
                    break
                output.extend(chunk); auth_buffer.extend(chunk)
                if b'password:' in auth_buffer.lower():
                    os.write(fd, (password + '\n').encode())
                    auth_buffer.clear()
            _, status = os.waitpid(pid, 0)
            result = output.decode(errors='replace')
            if os.waitstatus_to_exitcode(status) != 0:
                raise RuntimeError(result[-4000:])
            return result
        finally:
            os.close(fd)

    opts = ['-o', 'ConnectTimeout=8', '-o', 'NumberOfPasswordPrompts=1',
            '-o', 'PreferredAuthentications=password', '-o', 'PubkeyAuthentication=no']

    def ps(source):
        encoded = base64.b64encode(source.encode('utf-16-le')).decode()
        return command(['ssh', *opts, args.remote, 'powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand ' + encoded])

    def upload(path, name):
        if path.suffix == '.ps1':
            # Windows PowerShell 5 reads non-ASCII scripts correctly with a BOM.
            staged = run/('deploy-'+name)
            staged.write_text(path.read_text(),encoding='utf-8-sig')
            path = staged
        command(['scp', *opts, str(path), args.remote + ':' + remote_root + '/' + name])

    def local(argv, name, env=None):
        (run/(name+'.launch.json')).write_text(json.dumps({
            'binary_sha256':hashlib.sha256((ROOT/'target/release/Classroom').read_bytes()).hexdigest(),
            'args':list(map(str,argv)),
            'profile_override':(env or os.environ).get('CLASSROOM_VT_PROFILE'),
            'low_latency_override':(env or os.environ).get('CLASSROOM_VT_LOW_LATENCY'),
        },indent=2))
        stream = (run / (name + '.stdout.log')).open('w')
        proc = subprocess.Popen([str(ROOT / 'target/release/Classroom'), *map(str, argv)], stdout=stream, stderr=subprocess.STDOUT,env=env)
        stream.close(); owned.append(proc)
        return proc

    def launch(scenario, role, seconds, drop=0, count=1, pressure=False, cpu=False,
               width=0, height=0, audit=False, threads=0, gdi=False, live=False):
        ip = args.local_ip if role == 'student' else remote_ip
        extra = (' -Pressure' if pressure else '') + (' -CpuConversion' if cpu else '')
        extra += f' -DisplayWidth {width} -DisplayHeight {height} -DecoderThreads {threads}'
        if audit: extra += ' -UiAudit'
        if gdi: extra += ' -Gdi'
        if live: extra += ' -LiveMode'
        argument = f'-NoProfile -ExecutionPolicy Bypass -File {remote_root}/phase1-windows-worker.ps1 -Root {remote_root} -Scenario {scenario} -Role {role} -TeacherIp {ip} -Seconds {seconds} -DropEvery {drop} -Count {count}{extra}'
        ps(f"$ErrorActionPreference='Stop'; $a=New-ScheduledTaskAction -Execute powershell.exe -Argument '{argument}' -WorkingDirectory '{remote_root}'; $p=New-ScheduledTaskPrincipal -UserId '{username}' -LogonType Interactive -RunLevel Limited; $s=New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries; Register-ScheduledTask -TaskName '{task}' -Action $a -Principal $p -Settings $s -Force | Out-Null; Start-ScheduledTask '{task}'")

    def done(scenario, timeout):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            answer = ps(f"if(Test-Path '{remote_root}/{scenario}/result.json'){{Get-Content '{remote_root}/{scenario}/result.json' -Raw}}else{{'PENDING'}}")
            begin = answer.find('{'); end = answer.rfind('}')
            if begin >= 0 and end > begin:
                return json.loads(answer[begin:end+1])
            time.sleep(2)
        raise TimeoutError('Windows task timed out: ' + scenario)

    def teacher_ready(path, proc):
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            if path.exists() and any(message in path.read_text(errors='replace') for message in ('teacher M3 media loop ready','teacher room control ready')):
                return
            if proc.poll() is not None:
                raise RuntimeError('Local Teacher stopped before ready')
            time.sleep(0.1)
        raise TimeoutError('Local Teacher readiness')

    results = {}
    try:
        ps(f"New-Item -ItemType Directory -Force '{remote_root}' | Out-Null; quser; Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version")
        for path, name in [(ROOT/'target/rustup-windows/x86_64-pc-windows-gnu/release/Classroom.exe','Classroom.exe'),
                           (ROOT/'scripts/phase1-windows-worker.ps1','phase1-windows-worker.ps1'),
                           (ROOT/'scripts/phase1-windows-workload.ps1','phase1-windows-workload.ps1'),
                           (ROOT/'scripts/phase1-display-modes.ps1','phase1-display-modes.ps1'),
                           (ROOT/'scripts/phase1-window-audit.ps1','phase1-window-audit.ps1'),
                           (ROOT/'scripts/phase1-process-fault.ps1','phase1-process-fault.ps1'),
                           (ROOT/'config/default.toml','default.toml')]:
            upload(path, name)
        (run/'manifest.txt').write_text(f'remote={args.remote}\nlocal_ip={args.local_ip}\nremote_root={remote_root}\n' + ps(f"Get-FileHash '{remote_root}/Classroom.exe' -Algorithm SHA256 | Format-List"))
        # Exact binary/peer/ports only; fresh deployment paths otherwise trigger
        # Windows Firewall confirmation that unattended Teacher tests cannot click.
        remote_program = (remote_root + '/Classroom.exe').replace('/', '\\')
        ps(f"$ErrorActionPreference='Stop'; New-NetFirewallRule -Name '{task}-tcp' -DisplayName '{task}-tcp' -Program '{remote_program}' -Direction Inbound -Protocol TCP -LocalPort 47800 -RemoteAddress '{args.local_ip}' -Action Allow -Profile Any | Out-Null; New-NetFirewallRule -Name '{task}-udp' -DisplayName '{task}-udp' -Program '{remote_program}' -Direction Inbound -Protocol UDP -LocalPort 47811 -RemoteAddress '{args.local_ip}' -Action Allow -Profile Any | Out-Null")
        print('RUN_DIR=' + str(run), flush=True)
        for scenario in args.scenarios.split(','):
            print('START ' + scenario, flush=True)
            procs = []; workload = None
            try:
                duration = 180 if scenario.endswith('soak') else 60 if scenario.endswith('baseline') else args.seconds
                if scenario in ('capture','local','benchmark','inventory'):
                    launch(scenario, scenario, 20)
                    results[scenario] = done(scenario, 100)
                elif scenario.startswith('mac-'):
                    kind = scenario[4:]
                    workload = subprocess.Popen([str(ROOT/'target/phase1-mac-workload'),str(duration+20)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                    time.sleep(1)
                    teacher_seconds = 10 if kind in ('disconnect','reconnect') else duration+10
                    teacher_log = run/(scenario+'-teacher.log')
                    env = dict(os.environ, CLASSROOM_VT_PROFILE=kind[8:]) if kind.startswith('profile-') else None
                    if kind=='compat': env=dict(os.environ,CLASSROOM_VT_LOW_LATENCY='0')
                    teacher_flags = ['--teacher-ui','--teacher-test-cycles',3] if kind=='room' else []
                    teacher = local([*teacher_flags,'--role','teacher','--bind-ip',args.local_ip,'--share-seconds',teacher_seconds,'--drop-every-nth-rtp-packet',100 if kind=='loss' else 0,'--log-file',teacher_log],scenario+'-teacher',env=env)
                    procs.append(teacher); teacher_ready(teacher_log,teacher)
                    if kind=='latejoin': time.sleep(8)
                    launch(scenario,'student',duration,count=3 if kind=='fanout3' else 1,cpu=kind=='cpu',audit=kind=='ui',threads=int(kind[6:]) if kind.startswith('thread') else 0,gdi=kind=='gdi')
                    if kind in ('silent','teachercrash'):
                        # Wait for actual visible output before fault injection.
                        deadline=time.monotonic()+20
                        while time.monotonic()<deadline:
                            ready=ps(f"if(Test-Path '{remote_root}/{scenario}/student-0.log'){{Select-String -Quiet -Path '{remote_root}/{scenario}/student-0.log' -Pattern 'student first presented frame'}}")
                            if 'True' in ready: break
                            time.sleep(.25)
                        else: raise TimeoutError('Windows Student did not present before injection')
                        if kind=='silent':
                            import signal
                            teacher.send_signal(signal.SIGSTOP)
                            try: time.sleep(8)
                            finally: teacher.send_signal(signal.SIGCONT)
                        else:
                            teacher.kill(); teacher.wait(timeout=10); time.sleep(2)
                            procs.append(local(['--role','teacher','--bind-ip',args.local_ip,'--share-seconds',duration,'--log-file',run/(scenario+'-restart.log')],scenario+'-restart'))
                    if kind=='reconnect':
                        teacher.wait(timeout=20); time.sleep(2)
                        procs.append(local(['--role','teacher','--bind-ip',args.local_ip,'--share-seconds',duration,'--log-file',run/(scenario+'-restart.log')],scenario+'-restart'))
                    results[scenario] = done(scenario,duration+60)
                elif scenario.startswith('win-'):
                    kind=scenario[4:]
                    width,height=(map(int,kind[5:].split('x')) if kind.startswith('size-') else (0,0))
                    launch(scenario,'teacher-room' if kind=='room' else 'teacher',10 if kind in ('disconnect','reconnect') else duration+10,drop=100 if kind=='loss' else 0,pressure=kind=='pressure',width=width,height=height,live=kind=='live-mode')
                    time.sleep(8 if kind=='latejoin' else 3)
                    procs.append(local(['--role','student','--teacher-ip',remote_ip,'--share-seconds',duration+15 if kind=='teachercrash' else duration,'--log-file',run/(scenario+'-student.log')],scenario+'-student'))
                    if kind=='silent':
                        local_log=run/(scenario+'-student.log')
                        deadline=time.monotonic()+20
                        while time.monotonic()<deadline:
                            if local_log.exists() and 'student first presented frame' in local_log.read_text(): break
                            time.sleep(.1)
                        else: raise TimeoutError('Mac Student did not present before network block')
                        # Pause only the owned Classroom process, preserving TCP.
                        pid_text=ps(f"(Get-CimInstance Win32_Process | Where-Object {{ $_.ExecutablePath -eq '{remote_program}' }} | Select-Object -First 1 -ExpandProperty ProcessId)")
                        process_id=int(next(line.strip() for line in pid_text.splitlines() if line.strip().isdigit()))
                        try:
                            ps(f"& '{remote_root}/phase1-process-fault.ps1' -ProcessId {process_id} -Operation Suspend")
                            time.sleep(8)
                            text=local_log.read_text()
                            if 'teacher control liveness timeout' not in text or 'student preview closed after Teacher disconnect' not in text:
                                raise RuntimeError('Silent Teacher did not release Mac preview')
                        finally:
                            ps(f"& '{remote_root}/phase1-process-fault.ps1' -ProcessId {process_id} -Operation Resume")
                        deadline=time.monotonic()+15
                        while time.monotonic()<deadline:
                            if local_log.read_text().count('student first presented frame')>=2: break
                            time.sleep(.1)
                        else: raise TimeoutError('Presentation did not recover after resuming Teacher')
                    results[scenario]=done(scenario,duration+60)
                    if kind=='reconnect':
                        time.sleep(2); launch(scenario+'-restart','teacher',duration)
                        results[scenario+'-restart']=done(scenario+'-restart',duration+60)
                else:
                    raise ValueError('Unknown scenario '+scenario)
                for proc in procs:
                    proc.wait(timeout=duration+30)
                results[scenario]['local_exits']=[p.returncode for p in procs]
                results[scenario]['fault_injected']=scenario.endswith(('silent','teachercrash','studentcrash','blackout'))
            except Exception as error:
                results[scenario]={'error':str(error)}
                ps(f"Stop-ScheduledTask '{task}' -ErrorAction SilentlyContinue; Get-CimInstance Win32_Process | Where-Object {{ $_.ExecutablePath -eq '{remote_program}' }} | ForEach-Object {{ Stop-Process -Id $_.ProcessId -ErrorAction SilentlyContinue }}")
            finally:
                for proc in procs:
                    if proc.poll() is None: proc.terminate(); proc.wait(timeout=10)
                if workload and workload.poll() is None: workload.terminate(); workload.wait(timeout=10)
                (run/'status.json').write_text(json.dumps(results,indent=2))
                print('END '+scenario+' '+json.dumps(results[scenario]),flush=True)
        command(['scp',*opts,'-r',args.remote+':'+remote_root,str(run/'windows')],timeout=120)
    finally:
        for proc in owned:
            if proc.poll() is None: proc.terminate(); proc.wait(timeout=10)
        ps(f"if(Get-ScheduledTask -TaskName '{task}' -ErrorAction SilentlyContinue){{Stop-ScheduledTask '{task}'; Unregister-ScheduledTask '{task}' -Confirm:$false}}")
        ps(f"$rules=@(Get-NetFirewallRule -Name '{task}-tcp','{task}-udp' -ErrorAction SilentlyContinue); if($rules.Count){{$rules | Remove-NetFirewallRule -ErrorAction Stop}}; exit 0")
    print('COLLECTED '+str(run),flush=True)
    if any(value.get('error') for value in results.values()):
        raise SystemExit(1)


if __name__ == '__main__':
    main()
