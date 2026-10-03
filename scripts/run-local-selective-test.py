#!/usr/bin/env python3
"""Native local two-Student selective presentation fallback, not a scale benchmark."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time

ROOT=Path(__file__).resolve().parents[1]

def main():
    p=argparse.ArgumentParser();p.add_argument('--local-ip',default='192.168.100.219');p.add_argument('--min-thumbnails',type=int,default=2);a=p.parse_args()
    run=ROOT/'target/phase2-selected-local'/time.strftime('%Y%m%d-%H%M%S');run.mkdir(parents=True)
    binary=ROOT/'target/release/Classroom';owned=[]
    def start(args,name):
        with (run/(name+'.stdout.log')).open('w') as out:
            child=subprocess.Popen([str(binary),*map(str,args),'--log-file',str(run/(name+'.log'))],stdout=out,stderr=subprocess.STDOUT)
        owned.append(child);return child
    print('RUN_DIR='+str(run),flush=True)
    try:
        for index,port in enumerate((48320,48330)):
            start(['--role','student','--teacher-ip',a.local_ip,'--control-port',48300,'--media-port',port,'--feedback-port',port+1,'--share-seconds',65,'--launch-profile',run/(f'student-{index}-profile.toml')],f'student-{index}')
        teacher=start(['--role','teacher','--teacher-ui','--bind-ip',a.local_ip,'--control-port',48300,'--feedback-port',48311,'--teacher-test-cycles',2,'--teacher-test-min-students',2,'--teacher-test-min-thumbnails',a.min_thumbnails,'--teacher-test-output',run/'ui','--teacher-test-selective'],'teacher')
        deadline=time.monotonic()+110
        while any(c.poll() is None for c in owned):
            if time.monotonic()>deadline:raise TimeoutError('Local selective deadline')
            time.sleep(.2)
        teacher_text=(run/'teacher.log').read_text()
        receivers=[(run/f'student-{i}.log').read_text() for i in range(2)]
        starts=[l for l in teacher_text.splitlines() if 'teacher dashboard Start Sharing' in l]
        checks={
            'idle_thumbnails_ready': len({l.split('client_id=')[1].split()[0] for l in teacher_text.splitlines() if 'teacher thumbnail received' in l and 'state=ready' in l}) >= a.min_thumbnails,
            'no_capture_startup_timeout': all('completion callback did not fire within 30s' not in s for s in receivers),
            'child_exits':all(c.returncode==0 for c in owned),
            'class_then_selected':len(starts)==2 and 'scope=class' in starts[0] and 'scope=selected' in starts[1] and 'recipient_count=1' in starts[1],
            'both_receive_class':all(any('student received presentation start' in l and 'stream_id=1' in l for l in s.splitlines()) for s in receivers),
            'only_one_receives_selected':sum(any('student received presentation start' in l and 'stream_id=2' in l for l in s.splitlines()) for s in receivers)==1,
            'preview_release':all('student preview closed after Teacher stop' in s for s in receivers),
            'no_decode_errors':all(not any('decode_errors=' in l and 'decode_errors=0' not in l for l in s.splitlines()) for s in receivers),
            'one_encoder': 'encoder_instances=1' in teacher_text and 'encoder_instances=2' not in teacher_text,
            'two_online':any('teacher dashboard interval' in l and 'online=2' in l for l in teacher_text.splitlines()),
        }
        result={'passed':all(checks.values()),'checks':checks,'local_simulation':True,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest()}
        (run/'analysis.json').write_text(json.dumps(result,indent=2));print(json.dumps(result),flush=True)
        if not result['passed']:raise RuntimeError('Local selective assertions failed')
    finally:
        for c in owned:
            if c.poll() is None:c.terminate()
        for c in owned:
            try:c.wait(timeout=10)
            except subprocess.TimeoutExpired:c.kill();c.wait()

if __name__=='__main__':main()
