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
tag = 'files-' + time.strftime('%Y%m%d-%H%M%S')
run = ROOT / 'target/ubuntu-lan-validation' / tag; run.mkdir(parents=True)
work = run / 'work'; work.mkdir()
(run / 'worker.py').write_text((ROOT / 'scripts/ubuntu-student-lan-worker.py').read_text())
teacher = ROOT / 'target/Classroom Console.app/Contents/Helpers/Classroom Agent.app/Contents/MacOS/Classroom'
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

    check('tools_capability_fs', ipc.snapshot()['tools']['capabilities'][sid].get('fs') == 1)
    home = listing('~home')
    check('list_student_home', isinstance(home, dict) and not home.get('error'), json.dumps(home)[:250])
    dest = '/home/parallels/ClassroomFsTest'
    guest('rm -rf /home/parallels/ClassroomFsTest /tmp/escape', user=True); guest('rm -rf /home/parallels/ClassroomFsTest', 'ls -ld /home/parallels', user=False) if False else None
    guest(f'mkdir -p {dest}', user=True)

    # push: text, unicode name, space, binary 20 MB
    files = {'hello.txt': b'xin chao\n', 'Bài tập số 1 (nháp).txt': 'Tiếng Việt ✓'.encode(), 'blob.bin': os.urandom(20 * 1024 * 1024)}
    paths = []
    for n, d in files.items():
        (work / n).write_bytes(d); paths.append(str(work / n))
    a = cmd('fs_transfer', {'student': sid, 'push': True, 'paths': paths, 'dest': dest})
    check('push_accepted', a.get('ok'), a.get('error'))
    j = job(a['job_id'])
    check('push_complete', all(t['status'] == 'complete' for t in (j['targets'] if isinstance(j['targets'], list) else j['targets'].values())), json.dumps(j)[:300])
    for n in files:
        check('push_hash_' + n, vsha(f'{dest}/{n}') == sha(work / n))

    # pull back into a Teacher folder
    back = run / 'pulled'; back.mkdir()
    a = cmd('fs_transfer', {'student': sid, 'push': False, 'paths': [f'{dest}/blob.bin', f'{dest}/hello.txt'], 'dest': str(back)})
    j = job(a['job_id']) if a.get('ok') else None
    check('pull_complete', j and (back / 'blob.bin').exists() and sha(back / 'blob.bin') == sha(work / 'blob.bin') and (back / 'hello.txt').read_bytes() == files['hello.txt'], a.get('error') or '')

    # collect (one student) by extension
    guest(f"mkdir -p {dest}/sub; echo a > {dest}/sub/a.py; echo b > {dest}/b.py; echo c > {dest}/c.docx", user=True)
    a = cmd('fs_collect', {'path': dest, 'exts': 'py', 'recursive': True, 'label': 'probe-' + tag}, selected=[sid])
    check('collect_accepted', a.get('ok'), a.get('error'))
    if a.get('ok'):
        j = job(a['job_id'])
        got = sorted(p.name for p in Path(a['result']['folder']).rglob('*') if p.is_file())
        check('collect_filtered_py_only', got == ['a.py', 'b.py'], got)

    # edge cases
    a = cmd('fs_transfer', {'student': sid, 'push': True, 'paths': [paths[0]], 'dest': '/proc/forbidden'})
    j = job(a['job_id']) if a.get('ok') else None
    st = [t['status'] for t in (j['targets'] if isinstance(j['targets'], list) else j['targets'].values())] if j else ['rejected']
    check('push_to_unwritable_dest_errors_cleanly', st[0] in ('error', 'rejected'), st)
    a = cmd('fs_transfer', {'student': sid, 'push': False, 'paths': ['/etc/shadow'], 'dest': str(back)})
    j = job(a['job_id']) if a.get('ok') else None
    leaked = (back / 'shadow').exists()
    check('pull_unreadable_file_not_leaked', not leaked, 'shadow present' if leaked else 'absent')
    a = cmd('fs_transfer', {'student': sid, 'push': True, 'paths': [paths[0]], 'dest': dest + '/../../../tmp/escape'})
    j = job(a['job_id']) if a.get('ok') else None
    esc = guest('ls /tmp/escape 2>&1 | head -2', user=True)
    # The Teacher is trusted to pick any absolute folder (same as Windows/macOS); this only
    # documents that the normalized destination was honoured, it is not a containment claim.
    check('push_teacher_chosen_absolute_dest_honoured', 'hello.txt' in esc, esc.strip()[:120])
    guest('rm -rf /tmp/escape', user=True)
    # cancel a large transfer
    big = work / 'big.bin'
    with big.open('wb') as f:
        for _ in range(200): f.write(os.urandom(1024 * 1024))
    a = cmd('fs_transfer', {'student': sid, 'push': True, 'paths': [str(big)], 'dest': dest})
    time.sleep(2); cmd('fs_cancel', {'id': a['job_id']})
    j = job(a['job_id'], timeout=60)
    time.sleep(2)
    left = guest(f'ls -A {dest} | grep -i -E "big|part|tmp" ; true', user=True).strip()
    states = [t['status'] for t in (j['targets'] if isinstance(j['targets'], list) else j['targets'].values())]
    check('cancel_large_transfer_stops_and_leaves_no_partial', states[0] in ('cancelled', 'canceled', 'error') and not left, f'{states} leftovers={left!r}')
    alive = remote('status')['alive']
    check('student_alive_after_all', alive)
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
