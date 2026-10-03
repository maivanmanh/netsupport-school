#!/usr/bin/env python3
"""Saved-profile Teacher console and background agents on Mac/Windows via SSH.
Uses existing remote helper; credentials stay in memory. Keeps full failed runs.
"""
import argparse
import getpass
import hashlib
import importlib.util
import json
from pathlib import Path
import shlex
import subprocess
import time

spec=importlib.util.spec_from_file_location('four',Path(__file__).with_name('run-four-machine-test.py'))
four=importlib.util.module_from_spec(spec);spec.loader.exec_module(four)
ROOT=four.ROOT


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--mac',default='lolem@192.168.100.254')
    parser.add_argument('--windows',default='lolem@192.168.100.155')
    parser.add_argument('--local-ip',default='192.168.100.219')
    parser.add_argument('--scenarios',default='mac-teacher,win-teacher')
    parser.add_argument('--min-thumbnails',type=int,default=2)
    parser.add_argument('--skip-mac',action='store_true')
    parser.add_argument('--local-extra-student',action='store_true')
    parser.add_argument('--restart-student',action='store_true')
    parser.add_argument('--probe-screen',action='store_true')
    parser.add_argument('--selective',action='store_true')
    parser.add_argument('--desktop',action='store_true')
    parser.add_argument('--port-base',type=int,default=47800)
    args=parser.parse_args()
    if not 1024 <= args.port_base <= 65514: parser.error('port-base must be 1024..65514')
    control,media,feedback=args.port_base,args.port_base+10,args.port_base+11
    network_args=['--control-port',str(control),'--media-port',str(media),'--feedback-port',str(feedback)]
    remote_network=' '.join(network_args)
    win=four.Remote(args.windows,getpass.getpass('Windows password: '))
    mac=None if args.skip_mac else four.Remote(args.mac,getpass.getpass('Mac password: '))
    run=ROOT/'target/phase2-runs'/(time.strftime('%Y%m%d-%H%M%S')+f'-{__import__("os").getpid()}')
    run.mkdir(parents=True)
    username=args.windows.split('@')[0];win_ip=args.windows.split('@')[-1];mac_ip=args.mac.split('@')[-1]
    win_root=f'C:/Users/{username}/Classroom-phase2-{run.name}'
    print('RUN_DIR='+str(run),flush=True)
    try:
        mac_home=mac.ssh('printf %s "$HOME"').strip().splitlines()[-1] if mac else ''
    except Exception as error:
        (run/'preflight-error.json').write_text(json.dumps({'error':str(error),'resources_started':False},indent=2))
        raise
    mac_root=mac_home+'/Classroom-phase2-'+run.name
    mac_binary=mac_home+'/Classroom-two-mac/bin/Classroom'
    task='Classroom-Phase2-'+run.name
    program=win_root.replace('/','\\')+'\\Classroom.exe'
    owned=[];statuses={};sessions=[];windows_prepared=False

    def ps(source):
        return win.ps("Set-ExecutionPolicy -Scope Process Bypass -Force; "+source)
    def local(a,name):
        a=list(map(str,a))
        common=[item for flag,value in zip(network_args[::2],network_args[1::2]) if flag not in a for item in (flag,value)]
        with (run/(name+'.stdout.log')).open('w') as out:
            child=subprocess.Popen([str(ROOT/'target/release/Classroom'),*common,*a],stdout=out,stderr=subprocess.STDOUT)
        owned.append(child);return child
    def bootstrap(role,ip,path,name):
        p=local(['--role',role,'--teacher-ip',ip,'--remember','--launch-profile',path,'--diagnose','--log-file',run/(name+'-bootstrap.log')],name+'-bootstrap')
        if p.wait(timeout=20):raise RuntimeError('Local profile bootstrap failed')
    def launch_win(scenario,role,ip,students):
        argument=f'-NoProfile -WindowStyle Hidden -ExecutionPolicy Bypass -File {win_root}/phase2-windows-worker.ps1 -Root {win_root} -Scenario {scenario} -Role {role} -TeacherIp {ip} -Seconds 65 -Students {students} -Thumbnails {args.min_thumbnails} -ControlPort {control} -MediaPort {media} -FeedbackPort {feedback}'
        if args.restart_student: argument+=' -RestartStudent'
        if args.probe_screen: argument+=' -ProbeScreen'
        if args.selective: argument+=' -Selective'
        if args.desktop: argument+=' -Desktop'
        ps(f"$ErrorActionPreference='Stop';$a=New-ScheduledTaskAction -Execute powershell.exe -Argument '{argument}' -WorkingDirectory '{win_root}';$p=New-ScheduledTaskPrincipal -UserId '{username}' -LogonType Interactive -RunLevel Limited;$s=New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries;Register-ScheduledTask -TaskName '{task}' -Action $a -Principal $p -Settings $s -Force | Out-Null;Start-ScheduledTask '{task}'")
    def poll_result(scenario):
        text=ps(f"if(Test-Path '{win_root}/{scenario}/result.json'){{Get-Content '{win_root}/{scenario}/result.json' -Raw}}else{{'PENDING'}}")
        begin=text.find('{');end=text.rfind('}')
        return json.loads(text[begin:end+1]) if begin>=0 and end>begin else None
    def start_mac(scenario,ip):
        folder=mac_root+'/'+scenario
        mac.ssh(f'mkdir -p {shlex.quote(folder)}; {shlex.quote(mac_binary)} {remote_network} --role student --teacher-ip {ip} --remember --launch-profile {shlex.quote(folder)}/profile.toml --diagnose --log-file {shlex.quote(folder)}/bootstrap.log >{shlex.quote(folder)}/bootstrap.stdout 2>&1')
        command=f'echo $$ >{shlex.quote(folder)}/student.pid; exec caffeinate -di {shlex.quote(mac_binary)} {remote_network} --launch-profile {shlex.quote(folder)}/profile.toml --share-seconds 65 --log-file {shlex.quote(folder)}/student.log >{shlex.quote(folder)}/student.stdout 2>&1'
        session=mac.ssh(command,wait=False);sessions.append(session);return session
    def stop_local(p):
        if p and p.poll() is None:p.terminate();p.wait(timeout=10)
    def cleanup_scenario(scenario):
        folder=mac_root+'/'+scenario
        if mac: mac.ssh(f'if test -f {shlex.quote(folder)}/student.pid;then classroom_pid=$(cat {shlex.quote(folder)}/student.pid);pkill -P "$classroom_pid" || true;kill "$classroom_pid" 2>/dev/null || true;fi')
        ps(f"Stop-ScheduledTask '{task}' -ErrorAction SilentlyContinue;Get-CimInstance Win32_Process | Where-Object {{$_.ExecutablePath -eq '{program}'}} | ForEach-Object {{Stop-Process -Id $_.ProcessId -ErrorAction SilentlyContinue}}")

    try:
        ps(f"New-Item -ItemType Directory -Force '{win_root}' | Out-Null;quser")
        windows_prepared=True
        if mac: mac.ssh(f'mkdir -p {shlex.quote(mac_root)} {shlex.quote(str(Path(mac_binary).parent))}')
        exe=ROOT/'target/rustup-windows/x86_64-pc-windows-gnu/release/Classroom.exe'
        win.copy(exe,args.windows+':'+win_root+'/Classroom.exe')
        script=run/'phase2-windows-worker.ps1';script.write_text((ROOT/'scripts/phase2-windows-worker.ps1').read_text(),encoding='utf-8-sig')
        win.copy(script,args.windows+':'+win_root+'/phase2-windows-worker.ps1')
        if mac: mac.copy(ROOT/'target/release/Classroom',args.mac+':'+mac_binary)
        if mac: mac.ssh(f'chmod 755 {shlex.quote(mac_binary)};xattr -d com.apple.quarantine {shlex.quote(mac_binary)} 2>/dev/null || true')
        mac_hash=hashlib.sha256((ROOT/'target/release/Classroom').read_bytes()).hexdigest();win_hash=hashlib.sha256(exe.read_bytes()).hexdigest()
        if mac and mac_hash not in mac.ssh(f'shasum -a 256 {shlex.quote(mac_binary)}'):raise RuntimeError('Mac hash mismatch')
        if win_hash.upper() not in ps(f"(Get-FileHash '{win_root}/Classroom.exe' -Algorithm SHA256).Hash"):raise RuntimeError('Windows hash mismatch')
        (run/'manifest.json').write_text(json.dumps({'port_base':args.port_base,'local_ip':args.local_ip,'mac':args.mac if mac else None,'windows':args.windows,'mac_binary_sha256':mac_hash,'windows_binary_sha256':win_hash,'mac_root':mac_root,'windows_root':win_root,'selective':args.selective,'desktop':args.desktop,'desktop_revision':2 if args.desktop else 0,'local_extra_student':args.local_extra_student},indent=2))
        ps(f"$ErrorActionPreference='Stop';New-NetFirewallRule -Name '{task}-tcp' -DisplayName '{task}-tcp' -Program '{program}' -Direction Inbound -Protocol TCP -LocalPort {control},{control+1} -RemoteAddress '{args.local_ip}','{mac_ip}' -Action Allow -Profile Any | Out-Null;New-NetFirewallRule -Name '{task}-udp' -DisplayName '{task}-udp' -Program '{program}' -Direction Inbound -Protocol UDP -LocalPort {media},{feedback} -RemoteAddress '{args.local_ip}','{mac_ip}' -Action Allow -Profile Any | Out-Null")
        for scenario in args.scenarios.split(','):
            print('START '+scenario,flush=True);children=[];session=None
            try:
                if scenario=='mac-teacher':
                    profile=run/(scenario+'-profile.toml');bootstrap('teacher',args.local_ip,profile,scenario)
                    children.append(local((['--teacher-test-desktop'] if args.desktop else [])+(['--teacher-test-selective'] if args.selective else [])+['--launch-profile',profile,'--teacher-ui','--bind-ip',args.local_ip,'--teacher-test-cycles',2,'--teacher-test-min-students',2 if mac or args.local_extra_student else 1,'--teacher-test-min-thumbnails',args.min_thumbnails,'--teacher-test-output',run/(scenario+'-ui'),'--log-file',run/(scenario+'-teacher.log')],scenario+'-teacher'))
                    launch_win(scenario,'student',args.local_ip,0);session=start_mac(scenario,args.local_ip) if mac else None
                elif scenario=='win-teacher':
                    launch_win(scenario,'teacher',win_ip,2 if mac or args.local_extra_student else 1)
                    profile=run/(scenario+'-profile.toml');bootstrap('student',win_ip,profile,scenario)
                    children.append(local(['--launch-profile',profile,'--share-seconds',65,'--log-file',run/(scenario+'-student.log')],scenario+'-student'))
                    session=start_mac(scenario,win_ip) if mac else None
                else:raise ValueError('Unknown Phase 2 scenario')
                if args.local_extra_student:
                    ip=args.local_ip if scenario=='mac-teacher' else win_ip
                    profile=run/(scenario+'-extra-profile.toml')
                    bootstrap('student',ip,profile,scenario+'-extra')
                    children.append(local(['--launch-profile',profile,'--media-port',args.port_base+20,'--feedback-port',args.port_base+21,'--share-seconds',65,'--log-file',run/(scenario+'-extra-student.log')],scenario+'-extra-student'))
                deadline=time.monotonic()+150;result=None;next_progress=0
                while time.monotonic()<deadline:
                    if session:session.pump()
                    if result is None:result=poll_result(scenario)
                    if result and result.get('error'):raise RuntimeError(result['error'])
                    if result and all(p.poll() is not None for p in children) and (not session or session.code is not None):break
                    if time.monotonic()>next_progress:
                        print('PROGRESS '+scenario,flush=True);next_progress=time.monotonic()+20
                    time.sleep(.2)
                else:raise TimeoutError('Phase 2 scenario deadline')
                statuses[scenario]={'expected_students':2 if mac or args.local_extra_student else 1,'required_thumbnails':args.min_thumbnails,'windows':result,'local_exits':[p.returncode for p in children],'mac_exit':session.code if session else None}
                if result.get('exit')!=0 or any(p.returncode for p in children) or (session and session.code!=0):raise RuntimeError('Nonzero child exit')
            except Exception as error:statuses[scenario]={'error':str(error)}
            finally:
                for p in children:stop_local(p)
                cleanup_scenario(scenario)
                if session and session.code is None:session.stop()
                (run/'status.json').write_text(json.dumps(statuses,indent=2));print('END '+scenario+' '+json.dumps(statuses[scenario]),flush=True)
        win.copy(args.windows+':'+win_root,run/'windows',recursive=True)
        if mac: mac.copy(args.mac+':'+mac_root,run/'mac',recursive=True)
    finally:
        for p in owned:stop_local(p)
        if windows_prepared:
            try:
                ps(f"Stop-ScheduledTask '{task}' -ErrorAction SilentlyContinue;Unregister-ScheduledTask '{task}' -Confirm:$false -ErrorAction SilentlyContinue;Get-NetFirewallRule -Name '{task}-tcp','{task}-udp' -ErrorAction SilentlyContinue | Remove-NetFirewallRule")
            except Exception as error:
                (run/'cleanup-error.json').write_text(json.dumps({'error':str(error),'task':task},indent=2))
                raise
    print('COLLECTED '+str(run),flush=True)
    if any(s.get('error') for s in statuses.values()):raise SystemExit(1)

if __name__=='__main__':main()
