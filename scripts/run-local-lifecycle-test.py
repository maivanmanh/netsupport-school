#!/usr/bin/env python3
"""Visible Mac loopback lifecycle tests; not remote/Windows/LAN acceptance.
SIGSTOP preserves TCP without EOF, exercising silent-peer release. All signals
are sent only to children started here; collected logs are kept on failures.
"""
import json
import os
from pathlib import Path
import signal
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]
RUN = ROOT / 'target/lifecycle-runs' / time.strftime('%Y%m%d-%H%M%S')


def main():
    RUN.mkdir(parents=True)
    owned = []
    results = {}

    def launch(role, name):
        log = RUN / (name + '.log')
        args = [str(ROOT / 'target/release/Classroom'), '--role', role,
                '--log-file', str(log), '--share-seconds', '100']
        args += ['--bind-ip', '127.0.0.1'] if role == 'teacher' else ['--teacher-ip', '127.0.0.1']
        with (RUN / (name + '.stdout')).open('w') as output:
            proc = subprocess.Popen(args, stdout=output, stderr=subprocess.STDOUT)
        owned.append(proc)
        return proc, log

    def wait(log, phrase, count=1, seconds=20):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            if log.exists() and log.read_text(errors='replace').count(phrase) >= count:
                return time.monotonic()
            time.sleep(.05)
        raise RuntimeError(f'{log.name}: missing {phrase} #{count}')

    def stop(proc):
        if proc.poll() is None:
            proc.send_signal(signal.SIGCONT)
            proc.terminate()
            proc.wait(timeout=10)

    print('RUN_DIR=' + str(RUN), flush=True)
    try:
        for scenario in ('teacher-silent', 'teacher-crash', 'student-crash'):
            teacher = student = None
            try:
                teacher, teacher_log = launch('teacher', scenario + '-teacher')
                wait(teacher_log, 'teacher M3 media loop ready')
                student, student_log = launch('student', scenario + '-student')
                wait(student_log, 'student first presented frame')
                time.sleep(2)
                at = time.monotonic()
                if scenario == 'teacher-silent':
                    teacher.send_signal(signal.SIGSTOP)
                    closed = wait(student_log, 'student preview closed after Teacher disconnect', seconds=9)
                    wait(student_log, 'teacher control liveness timeout')
                    teacher.send_signal(signal.SIGCONT)
                    wait(student_log, 'student first presented frame', count=2)
                    results[scenario] = {'pass': True, 'release_observed_seconds': closed-at, 'recovered': True}
                elif scenario == 'teacher-crash':
                    teacher.kill(); teacher.wait(timeout=10)
                    closed = wait(student_log, 'student preview closed after Teacher disconnect', seconds=9)
                    teacher, _ = launch('teacher', scenario + '-teacher-restart')
                    wait(student_log, 'student first presented frame', count=2)
                    results[scenario] = {'pass': True, 'release_observed_seconds': closed-at, 'recovered': True}
                else:
                    student.kill(); student.wait(timeout=10)
                    student, restarted_log = launch('student', scenario + '-student-restart')
                    wait(restarted_log, 'student first presented frame')
                    results[scenario] = {'pass': True, 'recovered': True}
            except Exception as error:
                results[scenario] = {'pass': False, 'error': str(error)}
            finally:
                for proc in (student, teacher):
                    if proc is not None: stop(proc)
                (RUN / 'results.json').write_text(json.dumps(results, indent=2))
                print(scenario + ' ' + json.dumps(results.get(scenario, {'pass': False, 'error': 'interrupted'})), flush=True)
                time.sleep(1)
    finally:
        for proc in owned: stop(proc)
    if not all(row['pass'] for row in results.values()):
        raise SystemExit(1)


if __name__ == '__main__':
    main()
