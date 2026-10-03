#!/usr/bin/env python3
"""Visible lifecycle fault tests on one SSH-connected Mac; keeps complete logs.
Uses an existing SSH key. Only children/PIDs owned by this run are signaled.
"""
import argparse
import json
import hashlib
from pathlib import Path
import shlex
import signal
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--remote', required=True)
    parser.add_argument('--local-ip', required=True)
    args = parser.parse_args()
    run = ROOT/'target/remote-mac-lifecycle-runs'/time.strftime('%Y%m%d-%H%M%S')
    run.mkdir(parents=True)
    ssh = ['ssh','-i',str(Path.home()/'.ssh/classroom_two_mac'),'-o','BatchMode=yes','-o','ConnectTimeout=8',args.remote]
    scp = ['scp','-i',str(Path.home()/'.ssh/classroom_two_mac'),'-o','BatchMode=yes','-o','ConnectTimeout=8']
    remote = 'Classroom-lifecycle-' + run.name
    results = {}; owned = []
    def command(source):
        return subprocess.check_output([*ssh,source],timeout=35,text=True)
    command(f'mkdir -p {remote}')
    subprocess.run([*scp,str(ROOT/'target/release/Classroom'),args.remote+':'+remote+'/Classroom'],check=True,timeout=35)
    command(f'chmod 755 {remote}/Classroom; xattr -d com.apple.quarantine {remote}/Classroom 2>/dev/null || true')
    checksum=hashlib.sha256((ROOT/'target/release/Classroom').read_bytes()).hexdigest()
    if command(f'shasum -a 256 {remote}/Classroom').split()[0] != checksum:
        raise RuntimeError('Remote Mac binary checksum mismatch')
    (run/'binary-sha256.txt').write_text(checksum+'\n')

    def teacher(name):
        log=run/(name+'.log')
        with (run/(name+'.stdout')).open('w') as out:
            p=subprocess.Popen([str(ROOT/'target/release/Classroom'),'--role','teacher','--bind-ip',args.local_ip,'--share-seconds','100','--log-file',str(log)],stdout=out,stderr=subprocess.STDOUT)
        owned.append(p)
        deadline=time.monotonic()+20
        while time.monotonic()<deadline:
            if log.exists() and 'teacher M3 media loop ready' in log.read_text(): return p
            time.sleep(.1)
        raise RuntimeError('Teacher not ready')

    def student(name):
        with (run/(name+'.ssh')).open('w') as out:
            source=f'{remote}/Classroom --role student --teacher-ip {args.local_ip} --share-seconds 100 --log-file {remote}/{name}.log >{remote}/{name}.stdout 2>&1 & classroom_pid=$!; echo $classroom_pid >{remote}/{name}.pid; wait $classroom_pid'
            p=subprocess.Popen([*ssh,source],stdout=out,stderr=subprocess.STDOUT)
        owned.append(p)
        return p

    def wait(name,phrase,count=1):
        source=f"for n in {{1..100}}; do classroom_count=$(grep -c {shlex.quote(phrase)} {remote}/{name}.log 2>/dev/null || true); if test \"${{classroom_count:-0}}\" -ge {count}; then exit 0; fi; sleep 0.2; done; exit 1"
        command(source)
        return time.monotonic()

    def stop_remote(name, sig='TERM'):
        command(f'if test -f {remote}/{name}.pid; then kill -{sig} $(cat {remote}/{name}.pid) 2>/dev/null || true; fi')

    def stop(p):
        if p and p.poll() is None:
            p.send_signal(signal.SIGCONT); p.terminate(); p.wait(timeout=10)

    print('RUN_DIR='+str(run),flush=True)
    try:
        for scenario in ('teacher-silent','teacher-crash','student-crash'):
            t=s=None; name=scenario+'-student'; restart=name+'-restart'
            try:
                t=teacher(scenario+'-teacher'); s=student(name)
                wait(name,'student first presented frame'); time.sleep(2)
                at=time.monotonic()
                if scenario=='teacher-silent':
                    t.send_signal(signal.SIGSTOP)
                    closed=wait(name,'student preview closed after Teacher disconnect')
                    wait(name,'teacher control liveness timeout')
                    t.send_signal(signal.SIGCONT)
                    wait(name,'student first presented frame',2)
                    results[scenario]={'pass':True,'release_observed_seconds':closed-at,'recovered':True}
                elif scenario=='teacher-crash':
                    t.kill();t.wait(timeout=10)
                    closed=wait(name,'student preview closed after Teacher disconnect')
                    t=teacher(scenario+'-teacher-restart')
                    wait(name,'student first presented frame',2)
                    results[scenario]={'pass':True,'release_observed_seconds':closed-at,'recovered':True}
                else:
                    stop_remote(name,'KILL');s.wait(timeout=10)
                    s=student(restart);wait(restart,'student first presented frame')
                    results[scenario]={'pass':True,'recovered':True}
            except Exception as error:
                results[scenario]={'pass':False,'error':str(error)}
            finally:
                stop(t);stop_remote(name);stop_remote(restart);stop(s)
                (run/'results.json').write_text(json.dumps(results,indent=2))
                print(scenario+' '+json.dumps(results.get(scenario)),flush=True)
                time.sleep(1)
    finally:
        for p in owned: stop(p)
        subprocess.run([*scp,'-r',args.remote+':'+remote,str(run/'remote')],check=True,timeout=60)
    if not all(r['pass'] for r in results.values()): raise SystemExit(1)

if __name__=='__main__':main()
