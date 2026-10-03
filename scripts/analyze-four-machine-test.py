#!/usr/bin/env python3
"""Analyze full, unmodified mixed-platform 1Teacher/3Student run logs."""
import importlib.util
import json
from pathlib import Path
import re
import sys

spec=importlib.util.spec_from_file_location('two_mac',Path(__file__).with_name('analyze-two-mac-logs.py'))
helper=importlib.util.module_from_spec(spec);spec.loader.exec_module(helper)

def fields(line):return dict(re.findall(r'(\w+)=([^\s]+)',line))
def number(row,key,default=0):
    try:return float(row.get(key,default))
    except (ValueError,TypeError):return default

def analyze(root):
    statuses=json.loads((root/'status.json').read_text())
    manifest=json.loads((root/'manifest.json').read_text())
    windows_teacher=manifest.get('teacher_role')=='windows-0'
    report={'run':str(root),'teacher_role':manifest.get('teacher_role','mac-local'),'scenarios':{}}
    for scenario,status in statuses.items():
        folder=root/scenario
        teacher=(folder/('windows-0/teacher.log' if windows_teacher else 'teacher.log')).read_text(errors='replace')
        intervals=[fields(line) for line in teacher.splitlines() if 'teacher M3 media interval' in line]
        steady=[row for row in intervals if number(row,'connected_students_min')==3 and number(row,'connected_students_max')==3]
        complete=[fields(line) for line in teacher.splitlines() if 'teacher M3 media complete' in line]
        endpoints=set(re.findall(r'student registered for media[^\n]*endpoint=([^\s]+)',teacher))
        # Native dashboard's persistent peers log a separate registration marker.
        endpoints.update(re.findall(r'teacher room Student online[^\n]*endpoint=([^\s]+)',teacher))
        encoded_instances=helper.values(teacher,'encoder_instances')
        students={};passed=(not status.get('error') and status.get('teacher_exit')==0 and status.get('mac_exit')==0 and len(status.get('windows',{}))==2)
        for item in status.get('windows',{}).values():passed &= not item.get('error') and item.get('exits')==[0]
        passed &= len(endpoints)==3 and bool(steady) and bool(encoded_instances) and max(encoded_instances)==1
        if windows_teacher:passed &= status.get('local_student_exit')==0
        student_paths=[('mac-local',folder/'local-student.log'),('mac',folder/'remote-mac/student.log'),('windows-1',folder/'windows-1/student-0.log')] if windows_teacher else [('mac',folder/'remote-mac/student.log'),('windows-0',folder/'windows-0/student-0.log'),('windows-1',folder/'windows-1/student-0.log')]
        for label,path in student_paths:
            if not path.exists():
                students[label]={'error':'Missing log'};passed=False;continue
            text=path.read_text(errors='replace')
            summaries=[fields(line) for line in text.splitlines() if 'student M3 media complete' in line]
            stages=helper.stage_report(text)
            rows=[fields(line) for line in text.splitlines() if 'student M3 media interval' in line]
            seconds=sum(number(row,'elapsed_seconds') for row in summaries)
            decoded=sum(number(row,'decoded_frames') for row in summaries)
            rendered=sum(number(row,'rendered_frames') for row in summaries)
            health=bool(summaries) and all(number(row,'decoded_frames')>0 and number(row,'rendered_frames')>0 and number(row,'decode_errors')==0 for row in summaries)
            stop_at={};close_ms=[]
            for line in text.splitlines():
                row=fields(line)
                if 'student received presentation stop' in line:stop_at[row['stream_id']]=int(row['unix_ms'])
                if 'student preview closed after Teacher stop' in line and row.get('stream_id') in stop_at:close_ms.append(int(row['unix_ms'])-stop_at[row['stream_id']])
            if scenario=='room':health &= len(summaries)==3 and len(close_ms)==3 and text.count('student room connection ready')==1
            passed &= health
            students[label]={'healthy':bool(health),'sessions':summaries,'decode_fps':decoded/max(seconds,.001),'render_fps':rendered/max(seconds,.001),'stages':stages,'stop_close_ms':close_ms,
                'preview_starts':text.count('student forced presentation window ready'),'errors':[line for line in text.splitlines() if re.search(r'\bERROR\s',line)],
                'warnings':[line for line in text.splitlines() if re.search(r'\bWARN\s',line)],
                'max_source_capture_to_present_interval_p95_ms':stages.get('source_capture_to_present_estimate',{}).get('interval_p95_max_us',0)/1000,
                'rtt_ms_mean':sum(number(row,'control_rtt_ms') for row in rows)/max(len(rows),1)}
        total_seconds=sum(number(row,'interval_seconds') for row in steady)
        def weighted(key):return sum(number(row,key)*number(row,'interval_seconds') for row in steady)/max(total_seconds,.001)
        report['scenarios'][scenario]={'functional_ok':bool(passed),'status':status,'endpoints':sorted(endpoints),'teacher_max_encoder_instances':max(encoded_instances,default=0),'all_three_connected_interval_seconds':total_seconds,'teacher_steady':{key:weighted(key) for key in ['capture_fps','encode_fps','send_mbps','process_cpu_percent','process_working_set_bytes','target_bitrate_bps']},'teacher_summaries':complete,'students':students}
        print(scenario,'PASS' if passed else 'CHECK_FAILED','3-student seconds',round(total_seconds,1),'teacher encode FPS',round(weighted('encode_fps'),1))
        for name,item in students.items():
            if 'render_fps' in item:
                age=item['stages'].get('source_capture_to_present_estimate',{}).get('weighted_mean_us',0)/1000
                print(' ',name,'render FPS',round(item['render_fps'],1),'capture→present estimate ms',round(age,1),'decode errors',sum(number(row,'decode_errors') for row in item['sessions']),'Stop close ms',item['stop_close_ms'])
            else:print(' ',name,item)
    (root/'analysis.json').write_text(json.dumps(report,indent=2))
    return all(row['functional_ok'] for row in report['scenarios'].values())

if __name__=='__main__':sys.exit(0 if analyze(Path(sys.argv[1])) else 1)
