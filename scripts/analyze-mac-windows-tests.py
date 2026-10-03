#!/usr/bin/env python3
"""Summarize full media logs without conflating functional and lab acceptance."""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import sys

helper_path = Path(__file__).with_name('analyze-two-mac-logs.py')
spec = importlib.util.spec_from_file_location('two_mac', helper_path)
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)


def fields(line):
    return dict(re.findall(r'(\w+)=([^\s]+)', line))


def analyze(root, max_decoder_mean_ms=None):
    report = {'logs': {}, 'scenarios': {}, 'physical_lan_acceptance': False}
    for path in sorted(root.rglob('*.log')):
        relative = path.relative_to(root)
        if len(relative.parts)>1 and relative.parts[0]!='windows':
            continue
        if path.name.endswith(('.stdout.log', '.stderr.log')):
            continue
        text = path.read_text(errors='replace')
        report['logs'][str(path.relative_to(root))] = {
            'stages': helper.stage_report(text),
            'complete': [fields(line) for line in text.splitlines()
                         if any(marker in line for marker in ('M3 media complete', 'capture benchmark complete', 'local native-media benchmark complete'))],
            'warnings': [line for line in text.splitlines() if re.search(r'\b(?:WARN|ERROR)\s', line)],
            'registrations': text.count('student presentation registration complete'),
            'preview_starts': text.count('student forced presentation window ready'),
            'preview_closes': text.count('student preview closed after Teacher disconnect'),
            'preview_stops': text.count('student preview closed after Teacher stop'),
            'room_connections': text.count('student room connection ready'),
            'room_starts': text.count('teacher dashboard Start Sharing'),
            'room_stops': text.count('teacher dashboard Stop Sharing'),
        }
    statuses = json.loads((root/'status.json').read_text())
    for scenario, status in statuses.items():
        if scenario.endswith('-restart'):
            continue
        matching = {name: row for name, row in report['logs'].items()
                    if name.startswith(scenario+'-') or f'/{scenario}/' in '/'+name
                    or f'/{scenario}-restart/' in '/'+name}
        complete = [row for item in matching.values() for row in item['complete']]
        summaries = [row for row in complete if 'decoded_frames' in row and 'encode_fps' not in row]
        role = 'mac_teacher' if scenario.startswith('mac-') else 'windows_teacher'
        native = scenario in ('capture','local','benchmark','inventory')
        functional = not status.get('error') and bool(complete)
        functional &= all(code in (0,None) for code in status.get('exits',[]))
        functional &= all(code==0 for code in status.get('local_exits',[]))
        if scenario == 'inventory':
            paths=list(root.glob('windows/inventory/display-modes.json'))
            functional = not status.get('error') and bool(paths)
            if paths:
                functional &= bool(json.loads(paths[0].read_text(encoding='utf-8-sig')).get('modes'))
        elif scenario == 'capture':
            functional &= any(float(row.get('frames',0))>0 for row in complete)
        elif scenario == 'local':
            functional &= any(float(row.get('rendered_frames',0))>0 for row in complete)
        elif scenario == 'benchmark':
            functional &= len(summaries)==3 and all(float(row.get('decoded_frames',0))>0 for row in summaries)
        if not native:
            functional &= bool(summaries) and all(float(row.get('decoded_frames',0))>0 for row in summaries)
            functional &= any('sent_packets' in row and float(row['sent_packets'])>0 for row in complete)
            if 'fanout' not in scenario:
                functional &= all(float(row.get('rendered_frames',0))>0 for row in summaries)
            if scenario.endswith('room'):
                functional &= len(summaries)==3
                functional &= any(row['room_starts']==3 and row['room_stops']==3 for row in matching.values())
                functional &= any(row['room_connections']==1 and row['registrations']==3 and row['preview_starts']==3 and row['preview_stops']==3 for row in matching.values())
            if scenario.endswith('reconnect'):
                functional &= any(row['registrations']>=2 and row['preview_closes']>=1 for row in matching.values())
            if scenario.endswith('disconnect'):
                functional &= any(row['preview_closes']>=1 for row in matching.values())
            if scenario.endswith('fanout3'):
                functional &= len(summaries)==3
                teacher_logs = [root/name for name in matching if name.endswith('-teacher.log')]
                functional &= any(max(helper.values(p.read_text(),'connected_students'),default=0)==3
                                  and max(helper.values(p.read_text(),'encoder_instances'),default=0)==1 for p in teacher_logs)
        if scenario.startswith('win-size-') or scenario == 'win-live-mode':
            folder = root/'windows'/scenario
            changed = json.loads((folder/'display-change.json').read_text(encoding='utf-8-sig'))
            restored = json.loads((folder/'display-restored.json').read_text(encoding='utf-8-sig'))
            functional &= (restored['width'],restored['height']) == (changed['previousWidth'],changed['previousHeight'])
            if scenario.startswith('win-size-'):
                ratio = min(1,1920/changed['width'],1080/changed['height'])
                expected = (int(changed['width']*ratio)//2*2,int(changed['height']*ratio)//2*2)
                teachers = [fields(line) for line in (folder/'teacher.log').read_text(errors='replace').splitlines() if 'teacher M3 media loop ready' in line]
                functional &= any((int(row.get('width',0)),int(row.get('height',0))) == expected for row in teachers)
            else:
                teacher_text = (folder/'teacher.stdout.log').read_text(errors='replace')
                recovery = [i for i,line in enumerate(teacher_text.splitlines()) if 'DXGI' in line and 'recovered' in line]
                # Teacher must actually send frames after the final capture recovery.
                functional &= len(recovery)>=2 and any('teacher M3 media interval' in line and float(fields(line).get('encode_fps',0))>0 and float(fields(line).get('connected_students',0))>0 for line in teacher_text.splitlines()[recovery[-1]+1:] if recovery)
        if scenario == 'mac-ui':
            audit = json.loads((root/'windows'/scenario/'window-audit.json').read_text(encoding='utf-8-sig'))
            functional &= all(audit[key] for key in ('borderless','topmost','survivedClose'))
            functional &= (audit['width'],audit['height']) == (audit['screenWidth'],audit['screenHeight']) == (audit['restoredWidth'],audit['restoredHeight'])
        quality = all(int(row.get('decode_errors',0))==0 for row in summaries)
        latency_checks = []
        if max_decoder_mean_ms is not None and scenario.startswith('mac-') and scenario != 'mac-compat':
            for item in matching.values():
                stage = item['stages'].get('decode_input_to_output')
                if stage:
                    latency_checks.append(stage['weighted_mean_us'] <= max_decoder_mean_ms*1000)
            quality &= bool(latency_checks) and all(latency_checks)
        report['scenarios'][scenario] = {'functional_ok':bool(functional), 'quality_ok':bool(quality), 'decoder_mean_limit_ms':max_decoder_mean_ms, 'role':role, 'process_status':status, 'student_sessions':summaries}
        print(scenario, 'FUNCTIONAL_OK' if functional else 'CHECK_FAILED',
              'QUALITY_OK' if quality else 'QUALITY_FAILED',
              [(x.get('decode_fps'),x.get('render_fps'),x.get('decode_errors')) for x in summaries])
    (root/'analysis.json').write_text(json.dumps(report,indent=2))
    return report


if __name__=='__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('run',type=Path)
    parser.add_argument('--max-decoder-mean-ms',type=float)
    args = parser.parse_args()
    result = analyze(args.run,args.max_decoder_mean_ms)
    sys.exit(0 if all(row['functional_ok'] and row['quality_ok'] for row in result['scenarios'].values()) else 1)
