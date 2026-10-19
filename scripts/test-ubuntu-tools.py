#!/usr/bin/env python3
"""Real macOS Teacher <-> Ubuntu Student file-transfer checks (fs capability).
Isolated ports/profile; only touches ~/ClassroomFsTest on the Student."""
import hashlib, importlib.util, json, os, signal, subprocess, sys, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('lan', ROOT / 'scripts/test-ubuntu-student-lan.py')
lan = importlib.util.module_from_spec(spec); spec.loader.exec_module(lan)
VM, TIP, PORT = 'Ubuntu 26.04 ARM64', '10.211.55.2', 49000
SRC = '/home/parallels/src/netsupport-school/target/ubuntu-student/Classroom-Student-Ubuntu-aarch64/ClassroomStudent'
tag = 'tools-' + time.strftime('%Y%m%d-%H%M%S')
run = ROOT / 'target/ubuntu-lan-validation' / tag; run.mkdir(parents=True)
work = run / 'work'; work.mkdir()
(run / 'worker.py').write_text((ROOT / 'scripts/ubuntu-student-lan-worker.py').read_text())
teacher = Path(os.environ.get('TEACHER_BIN') or ROOT / 'target/Classroom Console.app/Contents/Helpers/Classroom Agent.app/Contents/MacOS/Classroom')
results = {}

_n = [0]
def guest(*c, user=False):
    if user:
        # prlctl exec does not preserve quoting: pass a script file via the shared home.
        _n[0] += 1
        script = run / f'cmd{_n[0]}.sh'; script.write_text(' '.join(c) + '\n')
        c = ['runuser', '-u', 'parallels', '--', 'bash', str(Path('/media/psf/Home') / script.relative_to(Path.home()))]
    cmd = ['prlctl', 'exec', VM] + list(map(str, c))
    for i in range(6):
        r = subprocess.run(cmd, text=True, capture_output=True, timeout=120)
        if r.returncode == 0: return r.stdout
        if 'PrlJob_' not in r.stderr: raise RuntimeError(r.stderr + r.stdout)
        time.sleep(2)
def remote(act):
    return json.loads(guest('python3', f'/tmp/{tag}.py', act, '--run', tag, '--teacher-ip', TIP, '--control-port', PORT,
                            '--source', SRC, '--interface', 'eth1'))
def sha(p): return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def vsha(p):
    out = guest(f"sha256sum '{p}' 2>&1", user=True).split()
    return out[0] if out and len(out[0]) == 64 else None
def check(name, ok, detail=''):
    results[name] = {'ok': bool(ok), 'detail': str(detail)[:400]}
    print(('PASS ' if ok else 'FAIL ') + name, str(detail)[:300], flush=True)

guest('cp', str(Path('/media/psf/Home') / run.relative_to(Path.home()) / 'worker.py'), f'/tmp/{tag}.py')
remote('setup')
endpoint = run / 't.json'; cfg = run / 'f.toml'; cfg.write_text('[video]\nmax_fps = 5\n')
proc = subprocess.Popen([str(teacher), '--role', 'teacher', '--teacher-agent', '--config', str(cfg), '--bind-ip', TIP,
    '--interface', 'bridge100', '--control-port', str(PORT), '--media-port', str(PORT+10), '--feedback-port', str(PORT+11),
    '--ipc-file', str(endpoint), '--launch-profile', str(run / 'tp/profile.toml'), '--log-file', str(run / 't.log')],
    stdout=open(run / 't.out', 'w'), stderr=subprocess.STDOUT)
ipc = None
try:
    lan.wait_for('ipc', lambda: endpoint.exists()); ipc = lan.IPC(endpoint)
    lan.wait_for('idle', lambda: ipc.snapshot().get('phase') == 'idle')
    remote('start')
    row = lan.wait_for('student', lambda: next((r for r in ipc.snapshot().get('students', []) if r.get('online')), None), timeout=120)
    sid = row['id']
    # tools channel readiness
    lan.wait_for('tools ready', lambda: sid in ipc.snapshot().get('tools', {}).get('ready', []), timeout=60)

    def cmd(action, input=None, selected=None, timeout=60):
        with ipc.condition:
            ipc.serial += 1; n = ipc.serial
            ipc.send({'id': n, 'action': action, 'input': input or {}, 'selected': selected or []})
            end = time.monotonic() + timeout
            while n not in ipc.acks:
                if ipc.error: raise ipc.error
                ipc.condition.wait(max(0.1, end - time.monotonic()))
                if time.monotonic() > end: raise TimeoutError(action)
            return ipc.acks.pop(n)
    def job(jid, timeout=180):
        def done():
            for j in ipc.snapshot().get('tools', {}).get('fs_jobs', []):
                if j['id'] == jid:
                    st = [t['status'] for t in j['targets']] if isinstance(j.get('targets'), list) else [t['status'] for t in j['targets'].values()]
                    if st and all(s in ('complete', 'error', 'cancelled', 'canceled') for s in st): return j
        return lan.wait_for('job ' + jid, done, timeout=timeout, interval=1)
    def listing(path, side='student'):
        a = cmd('fs_list', {'side': side, 'student': sid, 'path': path, 'hidden': True})
        if not a.get('ok'): return a
        for _ in range(60):
            r = cmd('fs_result', {'id': a['job_id']})['result']
            if not r.get('pending'): return r
            time.sleep(0.5)
        return {'error': 'timeout'}

    def tool_job(jid, timeout=60, accept=('complete', 'error', 'cancelled')):
        def done():
            for j in ipc.snapshot().get('tools', {}).get('jobs', []):
                if j['id'] == jid:
                    t = j['targets'][0] if j['targets'] else {}
                    if t.get('status') in accept: return t
        return lan.wait_for('tool job', done, timeout=timeout, interval=1)
    def tool(action, inp, **kw):
        a = cmd(action, inp, selected=[sid])
        if not a.get('ok'): return None, a
        return tool_job(a['job_id'], **kw), a
    caps = ipc.snapshot()['tools']['capabilities'][sid]
    check('caps_power_launch_website_terminal', all(caps.get(k) == 1 for k in ('power', 'launch', 'website', 'terminal')), {k: v for k, v in caps.items() if v == 1})
    t, a = tool('tool_message', {'message': 'Xin chào từ Teacher — kiểm tra thông báo', 'text': 'Xin chào từ Teacher'}, accept=('delivered', 'complete', 'error'))
    check('message_delivered', t and t['status'] in ('delivered', 'complete'), t or a.get('error'))
    time.sleep(3)
    subprocess.run(['prlctl', 'capture', VM, '--file', str(run / 'message.png')], capture_output=True)
    guest('rm -f /tmp/classroom-launch-ok', user=True)
    t, a = tool('tool_launch', {'path': '/usr/bin/touch', 'args': '/tmp/classroom-launch-ok'})
    time.sleep(1)
    ok = 'classroom-launch-ok' in guest('ls /tmp/classroom-launch-ok 2>&1', user=True)
    check('launch_program_runs', t and t['status'] == 'complete' and ok, (t or a.get('error')))
    t, a = tool('tool_launch', {'path': '/nonexistent/prog', 'args': ''})
    check('launch_missing_program_errors', t and t['status'] == 'error', t or a.get('error'))
    t, a = tool('tool_website', {'url': 'https://example.com/'})
    check('website_open_accepted', t and t['status'] == 'complete', t or a.get('error'))
    t, a = tool('tool_website', {'url': 'file:///etc/passwd'})
    check('website_rejects_non_http', t is None or t['status'] == 'error', t or a.get('error'))
    t, a = tool('tool_power', {'op': 'lock', 'message': 'Tạm dừng — nghe giảng'})
    check('lock_accepted', t and t['status'] == 'complete', t or a.get('error'))
    time.sleep(4)
    subprocess.run(['prlctl', 'capture', VM, '--file', str(run / 'locked.png')], capture_output=True)
    t, a = tool('tool_power', {'op': 'unlock'})
    check('unlock_accepted', t and t['status'] == 'complete', t or a.get('error'))
    time.sleep(3)
    subprocess.run(['prlctl', 'capture', VM, '--file', str(run / 'unlocked.png')], capture_output=True)
    a = cmd('terminal_open', {'compatibility': False}, selected=[sid])
    time.sleep(3)
    procs = guest('ps -eo pid,ppid,tty,cmd | grep -E "terminal-worker|pts" | grep -v grep ; true')
    check('terminal_worker_and_pty_shell', a.get('ok') and 'terminal-worker' in procs and 'pts' in procs, procs.strip()[:300])
    if a.get('ok'): cmd('terminal_close', {'session': a['result']['session']})
    shot = work / 'student.png'
    a = cmd('student_screenshot', {'dest': str(shot)}, selected=[sid])
    if a.get('ok'):
        j = job(a['job_id'], timeout=60)
    pngs = sorted(shot.rglob('*.png')) if shot.is_dir() else ([shot] if shot.exists() else [])
    shot = pngs[0] if pngs else shot
    ok = a.get('ok') and shot.is_file() and shot.read_bytes()[:8] == b'\x89PNG\r\n\x1a\n'
    check('screenshot_png_received', ok, a.get('error') or (f'{shot.stat().st_size} bytes' if shot.is_file() else 'missing'))
    if ok: (run / 'student.png').write_bytes(shot.read_bytes())
    check('student_alive_after_tools', remote('status')['alive'])
finally:
    (run / 'results.json').write_text(json.dumps(results, indent=2))
    try: ipc.command('quit')
    except Exception: pass
    try: remote('stop'); guest('python3', f'/tmp/{tag}.py', 'logs', '--run', tag)
    except Exception as e: print('cleanup', e)
    proc.send_signal(signal.SIGCONT); proc.terminate()
    try: proc.wait(15)
    except Exception: proc.kill()
    print('RESULTS', run / 'results.json')
