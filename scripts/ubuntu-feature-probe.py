#!/usr/bin/env python3
"""Interactive probe: real macOS Teacher + Ubuntu Student on the desktop session.
Starts both, dumps the Teacher's view of the Student (capabilities, roster row)
and runs the IPC actions given with --action NAME[:JSON]. Reuses the LAN harness."""
import argparse, importlib.util, json, signal, subprocess, sys, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('lan', ROOT / 'scripts/test-ubuntu-student-lan.py')
lan = importlib.util.module_from_spec(spec); spec.loader.exec_module(lan)

p = argparse.ArgumentParser()
p.add_argument('--vm', default='Ubuntu 26.04 ARM64')
p.add_argument('--teacher-ip', default='10.211.55.2')
p.add_argument('--port', type=int, default=49000)
p.add_argument('--student-source', default='/home/parallels/src/netsupport-school/target/ubuntu-student/Classroom-Student-Ubuntu-aarch64/ClassroomStudent')
p.add_argument('--hold', type=int, default=0, help='seconds to keep running for manual poking')
a = p.parse_args()
tag = 'probe-' + time.strftime('%Y%m%d-%H%M%S')
run = ROOT / 'target/ubuntu-lan-validation' / tag; run.mkdir(parents=True)
teacher = ROOT / 'target/Classroom Console.app/Contents/Helpers/Classroom Agent.app/Contents/MacOS/Classroom'
worker = (ROOT / 'scripts/ubuntu-student-lan-worker.py').read_text()
(run / 'worker.py').write_text(worker)

def guest(*c):
    for i in range(6):
        r = subprocess.run(['prlctl', 'exec', a.vm, *map(str, c)], text=True, capture_output=True, timeout=60)
        if r.returncode == 0: return r.stdout
        if 'PrlJob_' not in r.stderr: raise RuntimeError(r.stderr + r.stdout)
        time.sleep(2)
def remote(act):
    return json.loads(guest('python3', f'/tmp/{tag}.py', act, '--run', tag, '--teacher-ip', a.teacher_ip,
                            '--control-port', a.port, '--source', a.student_source, '--interface', 'eth1'))
# ship worker through the shared home folder (no HTTP server needed)
guest('cp', str(Path('/media/psf/Home') / run.relative_to(Path.home()) / 'worker.py'), f'/tmp/{tag}.py')
print(remote('setup')['binary'])
endpoint = run / 't.json'
cfg = run / 'f.toml'; cfg.write_text('[video]\nmax_fps = 30\n')
proc = subprocess.Popen([str(teacher), '--role', 'teacher', '--teacher-agent', '--config', str(cfg), '--bind-ip', a.teacher_ip,
    '--interface', 'bridge100', '--display-index', '1', '--control-port', str(a.port), '--media-port', str(a.port+10),
    '--feedback-port', str(a.port+11), '--ipc-file', str(endpoint), '--launch-profile', str(run / 'tp/profile.toml'),
    '--log-file', str(run / 't.log')], stdout=open(run / 't.out', 'w'), stderr=subprocess.STDOUT)
try:
    lan.wait_for('ipc', lambda: endpoint.exists())
    ipc = lan.IPC(endpoint)
    lan.wait_for('idle', lambda: ipc.snapshot().get('phase') == 'idle')
    remote('start')
    row = lan.wait_for('student', lambda: next((r for r in ipc.snapshot().get('students', []) if r.get('online')), None), timeout=120)
    time.sleep(8)
    snap = ipc.snapshot()
    (run / 'state.json').write_text(json.dumps(snap, indent=2, default=str))
    print('STATE_FILE', run / 'state.json')
    print('student row:', json.dumps(next(r for r in snap['students'] if r.get('online')), default=str)[:3000])
    if a.hold: time.sleep(a.hold)
finally:
    try: ipc.command('quit')
    except Exception: pass
    try: remote('stop'); guest('python3', f'/tmp/{tag}.py', 'logs', '--run', tag, '--stop-xvfb')
    except Exception as e: print('cleanup', e)
    proc.send_signal(signal.SIGCONT); proc.terminate()
    try: proc.wait(15)
    except Exception: proc.kill()
