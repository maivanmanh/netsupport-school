#!/usr/bin/env python3
"""Windows .94 Teacher → local Mac, MacStudio and Windows .189 GUI Students."""
import argparse
import getpass
import hashlib
import importlib.util
import ipaddress
import json
from pathlib import Path
import re
import shlex
import subprocess
import time

spec=importlib.util.spec_from_file_location('four',Path(__file__).with_name('run-four-machine-test.py'))
four=importlib.util.module_from_spec(spec);spec.loader.exec_module(four)
ROOT=four.ROOT

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--teacher',default='mvmanh@192.168.50.94')
    parser.add_argument('--student',default='mvmanh@192.168.50.189')
    parser.add_argument('--mac',default='mvmanh@macstudio')
    parser.add_argument('--local-ip',default='192.168.50.188')
    parser.add_argument('--scenarios',default='baseline,loss,room')
    parser.add_argument('--seconds',type=int,default=90)
    args=parser.parse_args()
    if not 20<=args.seconds<=600 or any(x not in ('smoke','baseline','loss','room') for x in args.scenarios.split(',')):parser.error('Invalid scenario or duration')
    password=getpass.getpass('Windows SSH password (memory only): ')
    mac_password=getpass.getpass('Remote Mac SSH password (memory only): ')
    windows=[four.Remote(args.teacher,password),four.Remote(args.student,password)]
    mac=four.Remote(args.mac,mac_password)
    teacher_ip=args.teacher.split('@')[-1]
    run=ROOT/'target/four-machine-runs'/(time.strftime('%Y%m%d-%H%M%S')+f'-winteacher-{four.os.getpid()}')
    run.mkdir(parents=True)
    task='Classroom-WinThree-'+run.name
    roots=[f'C:/Users/{r.address.split("@")[0]}/Classroom-four-{run.name}' for r in windows]
    mac_root=f'/Users/{args.mac.split("@")[0]}/Classroom-four-{run.name}'
    manifest={'teacher_role':'windows-0','teacher_ip':teacher_ip,'local_ip':args.local_ip,'mac':args.mac,'windows':[args.teacher,args.student],'sha256':{}}
    for name,path in [('mac',ROOT/'target/release/Classroom'),('windows',ROOT/'target/rustup-windows/x86_64-pc-windows-gnu/release/Classroom.exe')]:manifest['sha256'][name]=hashlib.sha256(path.read_bytes()).hexdigest()
    statuses={};owned=[];mac_session=None
    def launch(index,scenario,role,seconds,drop=0):
        remote=windows[index];root=roots[index]
        argument=f'-NoProfile -ExecutionPolicy Bypass -File {root}/phase1-windows-worker.ps1 -Root {root} -Scenario {scenario} -Role {role} -TeacherIp {teacher_ip} -Seconds {seconds} -DropEvery {drop} -MinStudents 3'
        remote.ps(f"$ErrorActionPreference='Stop';$a=New-ScheduledTaskAction -Execute powershell.exe -Argument '{argument}' -WorkingDirectory '{root}';$p=New-ScheduledTaskPrincipal -UserId '{remote.address.split('@')[0]}' -LogonType Interactive -RunLevel Limited;$s=New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries;Register-ScheduledTask -TaskName '{task}' -Action $a -Principal $p -Settings $s -Force | Out-Null;Start-ScheduledTask '{task}'")
    try:
        inventory=mac.ssh('uname -sm;hostname;stat -f %Su /dev/console;ifconfig | grep "inet "')
        manifest['mac_inventory']=inventory
        subnet=ipaddress.ip_network(args.local_ip+'/24',strict=False)
        addresses=[x for x in re.findall(r'inet (\d+\.\d+\.\d+\.\d+)',inventory) if ipaddress.ip_address(x) in subnet]
        if len(addresses)!=1:raise RuntimeError('Cannot unambiguously identify MacStudio LAN IP')
        peers=[args.local_ip,addresses[0],args.student.split('@')[-1]]
        mac.ssh(f'mkdir -p {shlex.quote(mac_root)}')
        mac.copy(ROOT/'target/release/Classroom',f'{mac.address}:{mac_root}/Classroom')
        manifest['mac_binary']=mac.ssh(f'chmod 755 {shlex.quote(mac_root)}/Classroom;shasum -a256 {shlex.quote(mac_root)}/Classroom')
        assert manifest['sha256']['mac'] in manifest['mac_binary'].lower()
        print('DEPLOYED MacStudio',flush=True)
        for index,(remote,root) in enumerate(zip(windows,roots)):
            manifest[f'windows_{index}_inventory']=remote.ps('quser;Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version')
            remote.ps(f"New-Item -ItemType Directory -Force '{root}' | Out-Null")
            for path in [ROOT/'target/rustup-windows/x86_64-pc-windows-gnu/release/Classroom.exe',ROOT/'scripts/phase1-windows-worker.ps1',ROOT/'scripts/phase1-windows-workload.ps1',ROOT/'config/default.toml']:
                source=path
                if path.suffix=='.ps1':source=run/('deploy-'+path.name);source.write_text(path.read_text(),encoding='utf-8-sig')
                remote.copy(source,f'{remote.address}:{root}/{path.name}')
            manifest[f'windows_{index}_binary']=remote.ps(f"Get-FileHash '{root}/Classroom.exe' | Format-List")
            assert manifest['sha256']['windows'] in manifest[f'windows_{index}_binary'].lower()
            program=(root+'/Classroom.exe').replace('/','\\')
            remote_addresses=','.join(f"'{x}'" for x in peers) if index==0 else f"'{teacher_ip}'"
            udp_port=47811 if index==0 else 47810
            remote.ps(f"$ErrorActionPreference='Stop';New-NetFirewallRule -Name '{task}-udp' -DisplayName '{task}-udp' -Program '{program}' -Direction Inbound -Protocol UDP -LocalPort {udp_port} -RemoteAddress @({remote_addresses}) -Action Allow -Profile Any | Out-Null")
            if index==0:remote.ps(f"$ErrorActionPreference='Stop';New-NetFirewallRule -Name '{task}-tcp' -DisplayName '{task}-tcp' -Program '{program}' -Direction Inbound -Protocol TCP -LocalPort 47800 -RemoteAddress @({remote_addresses}) -Action Allow -Profile Any | Out-Null")
            print('DEPLOYED '+remote.address,flush=True)
        (run/'manifest.json').write_text(json.dumps(manifest,indent=2));print('RUN_DIR='+str(run),flush=True)
        for scenario in args.scenarios.split(','):
            seconds=25 if scenario=='smoke' else 45 if scenario=='loss' else 55 if scenario=='room' else args.seconds
            folder=run/scenario;folder.mkdir();remote_folder=mac_root+'/'+scenario
            local=None;mac_session=None;status={'windows':{},'mac_exit':None,'teacher_exit':None,'local_student_exit':None}
            print('START '+scenario,flush=True)
            try:
                launch(0,scenario,'teacher-room' if scenario=='room' else 'teacher',seconds+15,100 if scenario=='loss' else 0)
                deadline=time.monotonic()+30
                while True:
                    text=windows[0].ps(f"if(Test-Path '{roots[0]}/{scenario}/teacher.log'){{$log=Get-Content '{roots[0]}/{scenario}/teacher.log' -Raw;if($log -match 'teacher M3 media loop ready|teacher room control ready'){{'READY'}}else{{Get-Content '{roots[0]}/{scenario}/teacher.log' -Tail 15}}}}")
                    if 'READY' in text or 'teacher M3 media loop ready' in text or 'teacher room control ready' in text:break
                    if time.monotonic()>deadline:raise RuntimeError('Windows Teacher failed readiness: '+text[-2000:])
                    time.sleep(.2)
                stream=(folder/'local-student.stdout.log').open('w')
                local=subprocess.Popen([str(ROOT/'target/release/Classroom'),'--role','student','--teacher-ip',teacher_ip,'--share-seconds',str(seconds),'--log-file',str(folder/'local-student.log')],stdout=stream,stderr=subprocess.STDOUT)
                stream.close();owned.append(local)
                mac.ssh(f'mkdir -p {shlex.quote(remote_folder)}')
                mac_session=mac.ssh(f'echo $$ >{shlex.quote(remote_folder)}/student.pid;exec caffeinate -di {shlex.quote(mac_root)}/Classroom --role student --teacher-ip {shlex.quote(teacher_ip)} --share-seconds {seconds} --log-file {shlex.quote(remote_folder)}/student.log >{shlex.quote(remote_folder)}/student.stdout.log 2>&1',wait=False)
                launch(1,scenario,'student',seconds)
                started=time.monotonic();next_progress=0;deadline=started+seconds+70
                while time.monotonic()<deadline:
                    mac_session.pump()
                    if time.monotonic()-started>=next_progress:
                        text=windows[0].ps(f"if(Test-Path '{roots[0]}/{scenario}/teacher.log'){{Select-String -Path '{roots[0]}/{scenario}/teacher.log' -Pattern 'teacher M3 media interval' | Select-Object -Last 1}}")
                        counts=re.findall(r'connected_students=(\d+)',text)
                        print(f'PROGRESS {scenario} elapsed={int(time.monotonic()-started)}s students={counts[-1] if counts else "0"}',flush=True);next_progress+=10
                    for index,(remote,root) in enumerate(zip(windows,roots)):
                        if str(index) in status['windows']:continue
                        answer=remote.ps(f"if(Test-Path '{root}/{scenario}/result.json'){{Get-Content '{root}/{scenario}/result.json' -Raw}}else{{'PENDING'}}")
                        begin,end=answer.find('{'),answer.rfind('}')
                        if begin>=0 and end>begin:status['windows'][str(index)]=json.loads(answer[begin:end+1])
                    if mac_session.code is not None and len(status['windows'])==2 and local.poll() is not None:break
                    time.sleep(.2)
                else:raise TimeoutError('Windows Teacher three-Student test deadline')
                status['mac_exit']=mac_session.code;status['local_student_exit']=local.returncode
                status['teacher_exit']=status['windows']['0']['exits'][0] if status['windows']['0'].get('exits') else None
            except Exception as error:status['error']=str(error)
            finally:
                if local and local.poll() is None:local.terminate();local.wait(timeout=10)
                if mac_session and mac_session.code is None:
                    mac.ssh(f"if test -f {shlex.quote(remote_folder)}/student.pid;then classroom_owned_pid=$(cat {shlex.quote(remote_folder)}/student.pid);pkill -P \"$classroom_owned_pid\" || true;kill \"$classroom_owned_pid\" 2>/dev/null || true;fi")
                    mac_session.stop()
                for index,(remote,root) in enumerate(zip(windows,roots)):
                    program=(root+'/Classroom.exe').replace('/','\\')
                    remote.ps(f"Stop-ScheduledTask '{task}' -ErrorAction SilentlyContinue;Get-CimInstance Win32_Process | Where-Object {{ $_.ExecutablePath -eq '{program}' }} | ForEach-Object {{ Stop-Process -Id $_.ProcessId -ErrorAction SilentlyContinue }}")
                    try:remote.copy(f'{remote.address}:{root}/{scenario}',folder/f'windows-{index}',True)
                    except Exception as error:status[f'collect_windows_{index}_error']=str(error)
                try:mac.copy(f'{mac.address}:{remote_folder}',folder/'remote-mac',True)
                except Exception as error:status['collect_mac_error']=str(error)
                statuses[scenario]=status;(run/'status.json').write_text(json.dumps(statuses,indent=2));print('END '+scenario+' '+json.dumps(status),flush=True)
    finally:
        for process in owned:
            if process.poll() is None:process.terminate();process.wait(timeout=10)
        for remote in windows:
            try:remote.ps(f"if(Get-ScheduledTask -TaskName '{task}' -ErrorAction SilentlyContinue){{Stop-ScheduledTask '{task}';Unregister-ScheduledTask '{task}' -Confirm:$false}};Get-NetFirewallRule -Name '{task}-udp','{task}-tcp' -ErrorAction SilentlyContinue | Remove-NetFirewallRule;exit 0")
            except Exception as error:print('CLEANUP WARNING '+str(error),flush=True)
    print('COLLECTED '+str(run),flush=True)

if __name__=='__main__':main()
