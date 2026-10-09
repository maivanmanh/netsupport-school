#!/usr/bin/env python3
"""Summarize phase-matched interval telemetry, not pooled frame percentiles."""
import argparse
import bisect
import json
import math
import re
import statistics
from pathlib import Path


def value(text):
    if text == 'true':
        return True
    if text == 'false':
        return False
    if text == 'None':
        return None
    if text.startswith('Some(') and text.endswith(')'):
        text = text[5:-1]
    try:
        return float(text)
    except ValueError:
        return text


def read_log(path):
    result = []
    for line in path.read_text(errors='replace').splitlines():
        fields = {key: value(quoted or bare) for key, quoted, bare in
                  re.findall(r'(\w+)=(?:"([^"]*)"|(\S+))', line)}
        if 'unix_ms' in fields:
            result.append((line, fields))
    return result


def stats(values):
    values = [float(v) for v in values if isinstance(v, (int, float)) and not isinstance(v, bool) and math.isfinite(v)]
    if not values:
        return None
    return {'n': len(values), 'median': statistics.median(values), 'min': min(values), 'max': max(values)}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('folder', type=Path)
    args = parser.parse_args()
    folder = args.folder
    meta = json.loads((folder / 'manifest.json').read_text())
    audits = {'windows': [json.loads(line) for line in (folder / 'windows-workload.jsonl').read_text(encoding='utf-8-sig').splitlines()],
              'mac': json.loads((folder / 'mac-workload.json').read_text())}
    logs = {name: read_log(folder / f'{name}-student.log') for name in ['windows', 'mac']}
    relay_log = read_log(folder / 'teacher.log')
    clocks = meta.get('clock_bounds', {})
    offsets = {k: (v['offset_min_ms'] + v['offset_max_ms']) / 2 for k, v in clocks.items()}
    uncertainty = sum(v['round_trip_ms'] / 2 for v in clocks.values())
    assert uncertainty < 1500, 'Clock alignment bounds too wide for the phase guard'
    report = {'scope': meta['scope'], 'method': 'Median/min/max of per-interval telemetry; interval p95 medians are not pooled p95. Keep phase seconds 2.5 through 8.5; foreground/visible audits required. Receiver clocks aligned by bounded query midpoint.',
              'cross_host_phase_alignment_uncertainty_ms': uncertainty, 'sources': {}}
    sender_keys = ['capture_fps', 'encode_fps', 'encode_latency_p95_ms', 'encoded_stream_mbps', 'send_mbps',
                   'target_fps', 'target_bitrate_bps', 'process_cpu_capacity_percent', 'waiting_frames_replaced']
    receiver_keys = ['decode_fps', 'render_fps', 'decode_latency_interval_p95_ms', 'render_latency_interval_p95_ms',
                     'decoded_frame_wait_interval_p95_ms', 'render_gap_interval_p95_ms', 'access_unit_gap_interval_p95_ms',
                     'packet_interarrival_interval_p95_ms', 'receive_mbps', 'jitter_delay_ms', 'process_cpu_percent',
                     'render_slot_replacements_interval']
    for source in ['windows', 'mac']:
        audit = sorted(audits[source], key=lambda row: row['unix_ms'])
        stamps = [r['unix_ms'] for r in audit]
        assert len(audit) >= meta['seconds'] - 5, f'{source} incomplete workload audit'
        assert all(i in {r['phase'] for r in audit} for i in range(7))
        scroll_key = 'scroll_y' if source == 'mac' else 'scroll_anchor'
        scroll_positions = {r.get(scroll_key) for r in audit if r['phase'] == 2}
        assert len(scroll_positions) > 3, f'{source} scroll movement not audited; exclude this trial'

        def phase(fields, clock_adjust_ms=0):
            stamp = fields['unix_ms'] + clock_adjust_ms
            index = bisect.bisect_right(stamps, stamp) - 1
            if index < 0:
                return None
            row = audit[index]
            if stamp - row['unix_ms'] > 1700 or not row['foreground'] or not row['visible']:
                return None
            elapsed = row['elapsed_s'] + (stamp - row['unix_ms']) / 1000
            if elapsed % 10 < 2.5 or elapsed % 10 > 8.5:
                return None
            return row['phase']

        receiver = 'mac' if source == 'windows' else 'windows'
        receiver_adjustment = offsets.get(source, 0) - offsets.get(receiver, 0)
        preview_path = folder / f'{source}-preview.log'
        preview_log = read_log(preview_path) if preview_path.exists() else []
        preview_rows = [f for line, f in preview_log if 'student M3 media interval ' in line]
        send = [f for line, f in logs[source] if 'teacher M3 media interval ' in line]
        receive = [f for line, f in logs[receiver] if 'student M3 media interval ' in line]
        stages = [f for line, f in logs[source] if 'media stage interval ' in line and f.get('role') == 'teacher']
        overall = {'audit_samples': len(audit), 'foreground_samples': sum(bool(r['foreground']) for r in audit),
                   'screen_sizes': sorted({(r['screen_width'], r['screen_height']) for r in audit}),
                   'timer_updates_per_second': stats([(audit[i]['updates'] - audit[i-1]['updates']) / (audit[i]['elapsed_s'] - audit[i-1]['elapsed_s']) for i in range(1, len(audit))]),
                   'scroll_positions': len(scroll_positions),
                   'phases': {}}
        start, end = stamps[0], stamps[-1] + 1000
        ready = [f for line, f in logs[source] if 'teacher M3 media loop ready' in line and start - 20000 <= f['unix_ms'] <= end]
        overall['capture_output'] = [{k: f.get(k) for k in ['width', 'height', 'encoder']} for f in ready]
        if not ready:
            # Ready can precede the first workload audit by compilation/setup time.
            before = [f for line, f in logs[source] if 'teacher M3 media loop ready' in line and f['unix_ms'] <= start]
            overall['capture_output'] = [{k: before[-1].get(k) for k in ['width', 'height', 'encoder']}] if before else []
        for index, name in enumerate(meta['phases']):
            ss = [f for f in send if phase(f) == index]
            rr = [f for f in receive if phase(f, receiver_adjustment) == index]
            timing = {}
            for f in stages:
                if phase(f) == index:
                    timing.setdefault(f['stage'], []).append(f['p95_us'] / 1000)
            overall['phases'][name] = {'source': {k: stats([f.get(k) for f in ss]) for k in sender_keys},
                                      'receiver': {k: stats([f.get(k) for f in rr]) for k in receiver_keys},
                                      'teacher_preview': {k: stats([f.get(k) for f in preview_rows if phase(f, offsets.get(source, 0)) == index]) for k in ['decode_fps', 'render_fps']},
                                      'stage_interval_p95_ms': {k: stats(v) for k, v in timing.items()},
                                      'encode_pressure_samples': sum(f.get('adaptive_encode_pressure') is True for f in ss),
                                      'source_interval_samples': len(ss), 'receiver_interval_samples': len(rr)}
        # Counter deltas within the workload, not sums of cumulative values.
        counter_keys = ['missing_packets', 'nacks_sent', 'nack_sequences_sent', 'keyframe_requests', 'decode_errors', 'depacketizer_errors']
        workload_recv = [f for f in receive if start <= f['unix_ms'] + receiver_adjustment <= end]
        overall['receiver_counter_deltas'] = {k: max(0, workload_recv[-1].get(k, 0) - workload_recv[0].get(k, 0)) if workload_recv else None for k in counter_keys}
        freshness = [f for line, f in logs[receiver] if 'student freshness recovery interval ' in line and start <= f['unix_ms'] + receiver_adjustment <= end]
        ingest = [f for line, f in logs[receiver] if 'student UDP ingest interval ' in line and start <= f['unix_ms'] + receiver_adjustment <= end]
        overall['freshness_counter_deltas'] = {k: max(0, freshness[-1].get(k, 0) - freshness[0].get(k, 0)) if freshness else None for k in ['late_packets', 'stale_backlog_packets', 'recovery_episodes', 'unrepaired_gap_resets', 'repaired_gaps', 'reordered_packets', 'decoder_no_output_resets']}
        overall['receiver_session_maxima'] = {k: max((f.get(k, 0) for f in freshness), default=None) for k in ['decoded_recovery_peak_ms', 'recovery_peak_ms', 'jitter_queue_depth']}
        overall['ingress_counter_deltas'] = {k: max(0, ingest[-1].get(k, 0) - ingest[0].get(k, 0)) if ingest else None for k in ['ingress_dropped_packets', 'socket_errors', 'wrong_peer_packets', 'wire_packets']}
        overall['relay_session_maxima'] = {k: max((f.get(k, 0) for line, f in relay_log if 'Student broadcast relay interval ' in line and start <= f['unix_ms'] + offsets.get(source, 0) <= end), default=None) for k in ['send_errors', 'send_deadline_errors', 'recipients', 'active_encoder_instances']}
        overall['sender_max_counters'] = {k: max((f.get(k, 0) for f in send if start <= f['unix_ms'] <= end), default=None) for k in ['send_errors', 'send_deadline_exhausted', 'nack_cache_misses']}
        report['sources'][source] = overall
    (folder / 'summary.json').write_text(json.dumps(report, indent=2))
    lines = ['# Matched Student-source measurement', '', report['method'], '', '| Source | Phase | Capture FPS | Encode FPS | Recipient decode FPS | Recipient render FPS | Encode interval p95 ms |', '| --- | --- | ---: | ---: | ---: | ---: | ---: |']

    def median(fields, key):
        item = fields.get(key)
        return f"{item['median']:.2f}" if item else 'N/A'

    for source, info in report['sources'].items():
        for name, phase in info['phases'].items():
            lines.append(f"| {source} | {name} | {median(phase['source'], 'capture_fps')} | {median(phase['source'], 'encode_fps')} | {median(phase['receiver'], 'decode_fps')} | {median(phase['receiver'], 'render_fps')} | {median(phase['source'], 'encode_latency_p95_ms')} |")
    (folder / 'summary.md').write_text('\n'.join(lines) + '\n')
    print('\n'.join(lines))


if __name__ == '__main__':
    main()
