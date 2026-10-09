#!/usr/bin/env python3
"""One local Mac Teacher + one remote Mac and two Windows Students.

Credentials stay in memory; remote processes/tasks/rules are owned by this run.
GUI Students run in each user's active desktop. Full logs are always collected.
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
import re
import select
import shlex
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
OPTS = ['-o','ConnectTimeout=8','-o','NumberOfPasswordPrompts=1',
        '-o','PreferredAuthentications=password','-o','PubkeyAuthentication=no']


class RemoteProcess:
    def __init__(self, argv, password):
        self.password = password
        self.output = bytearray()
        self.auth = bytearray()
        self.code = None
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            os.execvp(argv[0], argv)
    def pump(self):
        if self.code is not None:
            return
        if not select.select([self.fd], [], [], 0)[0]:
            return
        try:
            chunk = os.read(self.fd, 65536)
        except OSError as error:
            if error.errno != errno.EIO:
                raise
            chunk = b''
        if chunk:
            self.output.extend(chunk)
            self.auth.extend(chunk)
            if b'password:' in self.auth.lower():
                os.write(self.fd, (self.password+'\n').encode())
                self.auth.clear()
        else:
            pid, status = os.waitpid(self.pid, os.WNOHANG)
            if pid:
                self.code = os.waitstatus_to_exitcode(status)
                os.close(self.fd)
    def finish(self, timeout=45):
        deadline = time.monotonic()+timeout
        while self.code is None:
            self.pump()
            if time.monotonic()>deadline:
                self.stop()
                raise TimeoutError('Remote command deadline')
            time.sleep(.05)
        text = self.output.decode(errors='replace')
        if self.code:
            raise RuntimeError(text[-3000:])
        return text
    def stop(self):
        if self.code is None:
            # An SSH child can close its PTY before it finishes exiting. Never
            # let cleanup's waitpid defeat the remote-command deadline.
            for sig in (15, 9):
                try:
                    os.kill(self.pid, sig)
                except ProcessLookupError:
                    break
                deadline = time.monotonic() + 3
                while time.monotonic() < deadline:
                    pid, _ = os.waitpid(self.pid, os.WNOHANG)
                    if pid:
                        break
                    time.sleep(.05)
                if pid:
                    break
            os.close(self.fd)
            self.code = -15


class Remote:
    def __init__(self, address, password):
        self.address, self.password = address, password
    def ssh(self, source, wait=True):
        process=RemoteProcess(['ssh',*OPTS,self.address,source], self.password)
        return process.finish() if wait else process
    def ps(self, source):
        encoded=base64.b64encode(source.encode('utf-16-le')).decode()
        return self.ssh('powershell -NoProfile -NonInteractive -EncodedCommand '+encoded)
    def remove_old_student(self, replacement, sha256):
        if len(sha256) != 64 or any(c not in '0123456789abcdefABCDEF' for c in sha256):
            raise ValueError('Expected replacement SHA-256')
        # The host permits interactive commands but may prohibit .ps1 files.
        # Send our reviewed migration commands through the existing PS channel;
        # do not change ExecutionPolicy or use -ExecutionPolicy Bypass.
        script = Path(__file__).resolve().parent / 'remove-old-classroom-student.ps1'
        remote_script = replacement.replace('\\', '/').rsplit('/', 1)[0] + '/remove-old-student.ps1'
        self.copy(script, self.address + ':' + remote_script)
        digest = hashlib.sha256(script.read_bytes()).hexdigest()
        script_path = remote_script.replace("'", "''")
        quoted = replacement.replace("'", "''")
        return self.ps("$ErrorActionPreference='Stop'; if ((Get-FileHash '" + script_path +
                       "').Hash -ne '" + digest + "') { throw 'Migration script checksum mismatch' }; " +
                       "$migration=[scriptblock]::Create([IO.File]::ReadAllText('" + script_path +
                       "')); & $migration -ReplacementExecutable '" + quoted +
                       "' -ReplacementSha256 '" + sha256 + "' -Confirm:$false; " +
                       "$legacy=Join-Path $env:USERPROFILE 'Classroom-Student-dev'; " +
                       "if ((Test-Path (Join-Path $legacy 'ClassroomStudent.exe')) -and " +
                       "([IO.Path]::GetFullPath((Split-Path '" + quoted + "')).TrimEnd('\\') -ne $legacy)) { " +
                       "& $migration -OldInstallPath $legacy -ReplacementExecutable '" + quoted +
                       "' -ReplacementSha256 '" + sha256 + "' -Confirm:$false }")
    def copy(self, source, destination, recursive=False):
        return RemoteProcess(['scp',*OPTS,*(['-r'] if recursive else []),str(source),str(destination)],self.password).finish(90)


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--mac',default='mvmanh@macstudio')
    parser.add_argument('--windows',nargs=2,default=['mvmanh@192.168.50.94','mvmanh@192.168.50.189'])
    parser.add_argument('--local-ip',default='192.168.50.188')
    parser.add_argument('--scenarios',default='smoke,baseline,loss,room')
    parser.add_argument('--seconds',type=int,default=90)
    args=parser.parse_args()
    if not 20 <= args.seconds <= 600:
        parser.error('--seconds must be20..600')
    if any(x not in ('smoke','baseline','loss','room') for x in args.scenarios.split(',')):
        parser.error('unsupported scenario')
    workload_binary=ROOT/'target/phase1-mac-workload'
    workload_source=ROOT/'scripts/phase1-mac-workload.swift'
    if not workload_binary.exists() or workload_source.stat().st_mtime>workload_binary.stat().st_mtime:
        workload_binary.parent.mkdir(parents=True,exist_ok=True)
        print('BUILD repeatable Mac workload',flush=True)
        subprocess.run(['swiftc',str(workload_source),'-o',str(workload_binary)],check=True,timeout=120)
    windows_password=getpass.getpass('Windows SSH password (memory only): ')
    mac_password=getpass.getpass('Remote Mac SSH password (memory only): ')
    mac=Remote(args.mac,mac_password)
    windows=[Remote(address,windows_password) for address in args.windows]
    run=ROOT/'target/four-machine-runs'/(time.strftime('%Y%m%d-%H%M%S')+f'-{os.getpid()}')
    run.mkdir(parents=True)
    task='Classroom-Four-'+run.name
    mac_root=f'/Users/{args.mac.split("@")[0]}/Classroom-four-{run.name}'
    roots=[f'C:/Users/{r.address.split("@")[0]}/Classroom-four-{run.name}' for r in windows]
    owned=[]; mac_session=None; statuses={}
    manifest={'local_ip':args.local_ip,'mac':args.mac,'windows':args.windows,'sha256':{}}
    for name,path in [('mac',ROOT/'target/release/Classroom'),('windows',ROOT/'target/rustup-windows/x86_64-pc-windows-gnu/release/Classroom.exe')]:
        manifest['sha256'][name]=hashlib.sha256(path.read_bytes()).hexdigest()
    try:
        manifest['mac_inventory']=mac.ssh('uname -sm; hostname; stat -f %Su /dev/console; ifconfig | grep "inet "')
        mac.ssh(f'mkdir -p {shlex.quote(mac_root)}')
        mac.copy(ROOT/'target/release/Classroom',f'{mac.address}:{mac_root}/Classroom')
        manifest['mac_binary']=mac.ssh(f'chmod 755 {shlex.quote(mac_root)}/Classroom; shasum -a256 {shlex.quote(mac_root)}/Classroom')
        print('DEPLOYED remote Mac',flush=True)
        for index,(remote,remote_root) in enumerate(zip(windows,roots)):
            manifest[f'windows_{index}_inventory']=remote.ps('quser; Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version')
            remote.ps(f"New-Item -ItemType Directory -Force '{remote_root}' | Out-Null")
            for path in [ROOT/'target/rustup-windows/x86_64-pc-windows-gnu/release/Classroom.exe', ROOT/'scripts/phase1-windows-worker.ps1',ROOT/'config/default.toml']:
                source=path
                if path.suffix=='.ps1':
                    source=run/('deploy-'+path.name)
                    source.write_text(path.read_text(),encoding='utf-8-sig')
                remote.copy(source,f'{remote.address}:{remote_root}/{path.name}')
            program=(remote_root+'/Classroom.exe').replace('/','\\')
            manifest[f'windows_{index}_binary']=remote.ps(f"Get-FileHash '{remote_root}/Classroom.exe' | Format-List")
            # Temporary allow rule scoped to this executable, Teacher and Student UDP port.
            remote.ps(f"$ErrorActionPreference='Stop'; New-NetFirewallRule -Name '{task}-udp' -DisplayName '{task}-udp' -Program '{program}' -Direction Inbound -Protocol UDP -LocalPort 47810 -RemoteAddress '{args.local_ip}' -Action Allow -Profile Any | Out-Null")
            print('DEPLOYED '+remote.address,flush=True)
        (run/'manifest.json').write_text(json.dumps(manifest,indent=2))
        print('RUN_DIR='+str(run),flush=True)
        for scenario in args.scenarios.split(','):
            seconds=25 if scenario=='smoke' else 45 if scenario=='loss' else 55 if scenario=='room' else args.seconds
            folder=run/scenario;folder.mkdir()
            teacher=None;workload=None;mac_session=None
            print('START '+scenario,flush=True)
            status={'windows':{},'mac_exit':None,'teacher_exit':None}
            try:
                workload=subprocess.Popen([str(ROOT/'target/phase1-mac-workload'),str(seconds+25)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
                owned.append(workload)
                time.sleep(1)
                flags=['--teacher-ui','--teacher-test-cycles','3','--teacher-test-min-students','3'] if scenario=='room' else ['--share-seconds',str(seconds+15),'--drop-every-nth-rtp-packet','100' if scenario=='loss' else '0']
                stdout=(folder/'teacher.stdout.log').open('w')
                teacher=subprocess.Popen([str(ROOT/'target/release/Classroom'),'--role','teacher','--bind-ip',args.local_ip,'--log-file',str(folder/'teacher.log'),*flags],stdout=stdout,stderr=subprocess.STDOUT)
                stdout.close();owned.append(teacher)
                deadline=time.monotonic()+20
                while True:
                    text=(folder/'teacher.log').read_text(errors='replace') if (folder/'teacher.log').exists() else ''
                    if any(x in text for x in ['teacher M3 media loop ready','teacher room control ready']):break
                    if teacher.poll() is not None or time.monotonic()>deadline:raise RuntimeError('Teacher not ready')
                    time.sleep(.1)
                remote_folder=mac_root+'/'+scenario
                mac.ssh(f'mkdir -p {shlex.quote(remote_folder)}')
                # Keep the authenticated SSH session open through the native GUI run.
                mac_session=mac.ssh(f'echo $$ >{shlex.quote(remote_folder)}/student.pid; exec caffeinate -di {shlex.quote(mac_root)}/Classroom --role student --teacher-ip {shlex.quote(args.local_ip)} --share-seconds {seconds} --log-file {shlex.quote(remote_folder)}/student.log >{shlex.quote(remote_folder)}/student.stdout.log 2>&1',wait=False)
                for remote,remote_root in zip(windows,roots):
                    argument=f'-NoProfile -ExecutionPolicy Bypass -File {remote_root}/phase1-windows-worker.ps1 -Root {remote_root} -Scenario {scenario} -Role student -TeacherIp {args.local_ip} -Seconds {seconds}'
                    remote.ps(f"$ErrorActionPreference='Stop'; $a=New-ScheduledTaskAction -Execute powershell.exe -Argument '{argument}' -WorkingDirectory '{remote_root}'; $p=New-ScheduledTaskPrincipal -UserId '{remote.address.split('@')[0]}' -LogonType Interactive -RunLevel Limited; $s=New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries; Register-ScheduledTask -TaskName '{task}' -Action $a -Principal $p -Settings $s -Force | Out-Null; Start-ScheduledTask '{task}'")
                started=time.monotonic(); next_progress=0; deadline=started+seconds+65
                while time.monotonic()<deadline:
                    mac_session.pump()
                    if time.monotonic()-started >= next_progress:
                        text=(folder/'teacher.log').read_text(errors='replace')
                        counts=re.findall(r'connected_students=(\d+)',text)
                        print(f'PROGRESS {scenario} elapsed={int(time.monotonic()-started)}s students={counts[-1] if counts else "0"}',flush=True)
                        next_progress+=10
                    for index,(remote,remote_root) in enumerate(zip(windows,roots)):
                        if str(index) in status['windows']:continue
                        answer=remote.ps(f"if(Test-Path '{remote_root}/{scenario}/result.json'){{Get-Content '{remote_root}/{scenario}/result.json' -Raw}}else{{'PENDING'}}")
                        begin,end=answer.find('{'),answer.rfind('}')
                        if begin>=0 and end>begin:status['windows'][str(index)]=json.loads(answer[begin:end+1])
                    if mac_session.code is not None and len(status['windows'])==2 and teacher.poll() is not None:break
                    time.sleep(.2)
                else:raise TimeoutError('Four-machine scenario deadline')
                status['mac_exit']=mac_session.code
                status['teacher_exit']=teacher.returncode
            except Exception as error:
                status['error']=str(error)
            finally:
                if teacher and teacher.poll() is None:teacher.terminate();teacher.wait(timeout=10)
                if workload and workload.poll() is None:workload.terminate();workload.wait(timeout=10)
                if mac_session and mac_session.code is None:
                    mac.ssh(f"if test -f {shlex.quote(remote_folder)}/student.pid; then classroom_owned_pid=$(cat {shlex.quote(remote_folder)}/student.pid); pkill -P \"$classroom_owned_pid\" || true; kill \"$classroom_owned_pid\" 2>/dev/null || true; fi")
                    mac_session.stop()
                for index,(remote,remote_root) in enumerate(zip(windows,roots)):
                    program=(remote_root+'/Classroom.exe').replace('/','\\')
                    remote.ps(f"Stop-ScheduledTask '{task}' -ErrorAction SilentlyContinue; Get-CimInstance Win32_Process | Where-Object {{ $_.ExecutablePath -eq '{program}' }} | ForEach-Object {{ Stop-Process -Id $_.ProcessId -ErrorAction SilentlyContinue }}")
                    try:remote.copy(f'{remote.address}:{remote_root}/{scenario}',folder/f'windows-{index}',True)
                    except Exception as error:status[f'collect_windows_{index}_error']=str(error)
                try:mac.copy(f'{mac.address}:{remote_folder}',folder/'remote-mac',True)
                except Exception as error:status['collect_mac_error']=str(error)
                statuses[scenario]=status
                (run/'status.json').write_text(json.dumps(statuses,indent=2))
                print('END '+scenario+' '+json.dumps(status),flush=True)
    finally:
        for process in owned:
            if process.poll() is None:process.terminate();process.wait(timeout=10)
        for remote in windows:
            try:remote.ps(f"if(Get-ScheduledTask -TaskName '{task}' -ErrorAction SilentlyContinue){{Stop-ScheduledTask '{task}';Unregister-ScheduledTask '{task}' -Confirm:$false}}; Get-NetFirewallRule -Name '{task}-udp' -ErrorAction SilentlyContinue | Remove-NetFirewallRule; exit 0")
            except Exception as error:print('CLEANUP WARNING '+str(error),flush=True)
    print('COLLECTED '+str(run),flush=True)

if __name__=='__main__':main()
