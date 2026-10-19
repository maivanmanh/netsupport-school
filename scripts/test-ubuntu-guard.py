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
tag = 'guard-' + time.strftime('%Y%m%d-%H%M%S')
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

    def http(url):
        out = guest(f"curl -s -o /dev/null -m 8 -w '%{{http_code}}' {url}; true", user=True).strip()
        return out
    def policy(mode, rules=()):
        r = cmd('tool_policy', {'policy': {'internet': {'mode': mode, 'rules': list(rules)}, 'copilot_block': False, 'lease_minutes': 240}}, selected=[sid], timeout=90)
        time.sleep(2)
        return r
    caps = ipc.snapshot()['tools']['capabilities'][sid]
    check('guard_capability', caps.get('guard') == 1, {k: caps.get(k) for k in ('guard', 'internet')})
    base = (http('https://example.com'), http('https://www.wikipedia.org'))
    check('baseline_open', base[0].startswith('2') or base[0].startswith('3'), base)
    r = policy('allow_only', ['example.com'])
    check('allow_only_accepted', r.get('ok'), r.get('error'))
    a, b = http('https://example.com'), http('https://www.wikipedia.org')
    check('allow_only_permits_listed_and_blocks_others', a[:1] in '23' and b in ('000', ''), (a, b))
    check('student_still_connected_to_teacher', any(x.get('online') for x in ipc.snapshot()['students']))
    check('capability_reports_mode', ipc.snapshot()['tools']['capabilities'][sid].get('internet') == 'allow_only', ipc.snapshot()['tools']['capabilities'][sid].get('internet'))
    r = policy('block_only', ['wikipedia.org'])
    a, b = http('https://example.com'), http('https://www.wikipedia.org')
    check('block_only_blocks_listed_only', a[:1] in '23' and b in ('000', ''), (a, b))
    r = policy('block_all')
    a, b = http('https://example.com'), http('https://www.wikipedia.org')
    check('block_all_blocks_internet', a in ('000', '') and b in ('000', ''), (a, b))
    check('teacher_link_survives_block_all', any(x.get('online') for x in ipc.snapshot()['students']))
    r = policy('open')
    a, b = http('https://example.com'), http('https://www.wikipedia.org')
    check('open_restores_access', a[:1] in '23' and b[:1] in '23', (a, b))
    tables = guest('nft list tables; true').strip()
    check('no_guard_tables_after_open', 'classroom_guard' not in tables, tables)
    # crash safety: block, kill the Student, rules must disappear
    policy('block_all')
    tables = guest('nft list tables; true')
    check('rules_present_while_blocked', 'classroom_guard' in tables, tables.strip())
    guest('pkill', '-9', '-f', f'/tmp/classroom-ubuntu-lan-{tag}/ClassroomStudent')
    time.sleep(3)
    tables = guest('nft list tables; true')
    check('rules_removed_when_student_dies', 'classroom_guard' not in tables, tables.strip())
    check('internet_back_after_student_crash', http('https://example.com')[:1] in '23')
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
