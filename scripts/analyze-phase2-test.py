#!/usr/bin/env python3
"""Validate collected Phase 2 lifecycle/thumbnail evidence, not scale acceptance."""
import argparse
import json
from pathlib import Path
import re
import statistics


def field(line, name):
    match = re.search(r'\b' + name + r'=([\d.]+)', line)
    return float(match[1]) if match else None


def analyze(run):
    statuses = json.loads((run / 'status.json').read_text())
    manifest = json.loads((run / 'manifest.json').read_text())
    selective = manifest.get('selective', False)
    desktop = manifest.get('desktop', False)
    results = {}
    for scenario, status in statuses.items():
        folder = run / 'windows' / scenario
        teacher = run / (scenario + '-teacher.log') if scenario == 'mac-teacher' else folder / 'teacher.log'
        if not teacher.exists() and scenario == 'mac-teacher': teacher = run / (scenario + '-teacher-teacher.log')
        students = ([folder / 'student.log'] if scenario == 'mac-teacher' else [run / (scenario + '-student.log')])
        if (folder / 'student-before-restart.log').exists():
            students.insert(0, folder / 'student-before-restart.log')
        extra = run / (scenario + '-extra-student.log')
        if extra.exists(): students.append(extra)
        remote = run / 'mac' / scenario / 'student.log'
        if remote.exists(): students.append(remote)
        lines = teacher.read_text().splitlines() if teacher.exists() else []
        thumbnails = [l for l in lines if 'teacher thumbnail received' in l]
        ready = [l for l in thumbnails if 'state=ready' in l]
        paused = [l for l in thumbnails if 'state=paused' in l]
        starts = [l for l in lines if 'teacher dashboard Start Sharing' in l]
        stops = [l for l in lines if 'teacher dashboard Stop Sharing' in l]
        receivers = [p.read_text() for p in students if p.exists()]
        all_student = '\n'.join(receivers)
        ui = run / (scenario + '-ui') if scenario == 'mac-teacher' else folder / 'ui'
        errors = [l for l in lines + all_student.splitlines() if ' ERROR ' in l]
        decode_errors = [l for l in all_student.splitlines() if re.search(r'\bdecode_errors=[1-9]', l)]
        checks = {
            'process_exit': not status.get('error') and status.get('windows', {}).get('exit') == 0 and all(x == 0 for x in status.get('local_exits', [])) and status.get('mac_exit') in (None, 0),
            'two_start_stop_cycles': len(starts) == 2 and len(stops) == 2,
            'expected_online_students': any('teacher dashboard interval' in l and field(l, 'online') == status.get('expected_students') for l in lines),
            'bounded_thumbnail_dimensions': bool(ready) and all(2 <= field(l, 'width') <= 480 and 2 <= field(l, 'height') <= 270 and 0 < field(l, 'bytes') <= 49152 for l in ready),
            'paused_and_resumed': bool(paused) and any(lines.index(l) > lines.index(paused[-1]) for l in ready),
            'preview_release': all('student preview closed after Teacher stop' in s for s in receivers),
            'one_encoder': any('encoder_instances=1' in l for l in lines) and not any(re.search(r'encoder_instances=[2-9]', l) for l in lines),
            'no_decode_or_process_errors': not errors and not decode_errors,
            'all_ui_views_exported': all((ui / (name + '.ppm')).exists() for name in ('grid', 'list', 'inspect', 'sharing')),
        }
        if status.get('windows', {}).get('restart_performed'):
            intervals = [l for l in lines if 'teacher dashboard interval' in l]
            checks['restart_without_duplicate_online_rows'] = bool(intervals) and max(field(l, 'online') for l in intervals) == status['expected_students'] and 'student-before-restart.log' in [p.name for p in students]
        if desktop:
            if scenario == 'mac-teacher' and manifest.get('desktop_revision',0) >= 2:
                checks['retina_backing_frame'] = any('teacher first GUI frame shown' in l and 'backing_scale=2' in l and field(l,'framebuffer_width') == 2400 and field(l,'framebuffer_height') == 1520 for l in lines)
                checks['quick_native_tap_activates_view'] = any('native zero-pressure quick tap activated List view' in l for l in lines)
            checks['prepared_first_frame'] = any('teacher first GUI frame shown' in l and 'first_frame_prepared=true' in l for l in lines)
            checks['native_tray'] = any('native Classroom tray ready' in l for l in lines)
            checks['close_hides_while_sharing'] = any('Teacher native close-to-tray verified' in l and 'presentation_active=true' in l for l in lines)
            checks['tray_reopens'] = any('Teacher tray reopen verified' in l for l in lines)
            checks['settings_cancel'] = any('native Settings diagnostic cancelled' in l for l in lines)
            checks['desktop_states_exported'] = all((ui / (name + '.ppm')).exists() for name in ('first-frame','hover','pressed','settings'))
            if checks['desktop_states_exported']:
                checks['button_states_distinct'] = len({(ui / (name + '.ppm')).read_bytes() for name in ('first-frame','hover','pressed')}) == 3
        if selective:
            received = [set(int(field(l, 'stream_id')) for l in s.splitlines() if 'student received presentation start' in l) for s in receivers]
            checks['class_then_selected_scope'] = len(starts) == 2 and 'scope=class' in starts[0] and 'scope=selected' in starts[1] and field(starts[1], 'recipient_count') == 1
            checks['excluded_student_never_starts_selected_stream'] = len(received) == status.get('expected_students',0) and all(1 in ids for ids in received) and sum(2 in ids for ids in received) == 1
        results[scenario] = {
            'passed': all(checks.values()), 'checks': checks,
            'ready_snapshots': len(ready), 'paused_responses': len(paused),
            'median_jpeg_bytes': statistics.median(field(l, 'bytes') for l in ready) if ready else None,
            'median_decode_ms': statistics.median(field(l, 'decode_ms') for l in ready) if ready else None,
            'median_snapshot_round_trip_ms': statistics.median(field(l, 'round_trip_ms') for l in ready) if ready else None,
            'limitations': 'No physical Windows scale/optical latency acceptance; UI images require visual inspection.',
        }
    output = {'run': str(run), 'passed': bool(results) and all(r['passed'] for r in results.values()), 'scenarios': results}
    (run / 'analysis.json').write_text(json.dumps(output, indent=2))
    return output


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('run', type=Path)
    result = analyze(parser.parse_args().run)
    print(json.dumps(result, indent=2))
    raise SystemExit(0 if result['passed'] else 1)
