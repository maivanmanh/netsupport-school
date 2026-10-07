#!/usr/bin/env python3
"""Extract bounded recovery evidence; no optical latency or physical scale claim."""
import argparse
import json
from pathlib import Path
import re


def fields(line):
    return {key: float(value) for key, value in re.findall(
        r'\b([a-z][a-z0-9_]*)=(-?\d+(?:\.\d+)?)(?=\s|$)', line)}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('run', type=Path)
    args = parser.parse_args()
    report = {'scope': 'Windows ARM VM receiver, Mac publisher; application metrics only', 'scenarios': {}}
    for name in ('baseline', 'loss', 'latejoin'):
        path = args.run / f'{name}-student.log'
        if not path.exists():
            report['scenarios'][name] = {'status': 'no evidence'}
            continue
        lines = path.read_text(errors='replace').splitlines()
        complete = [fields(line) for line in lines if 'student M3 media complete' in line]
        intervals = [fields(line) for line in lines if 'student M3 media interval' in line]
        recovery = [fields(line) for line in lines if 'student freshness recovery interval' in line]
        ingress = [fields(line) for line in lines if 'student UDP ingest interval' in line]
        summary = complete[-1] if complete else {}
        first_decode_event = next((fields(line) for line in lines if 'student first decoded frame' in line), {})
        first_present_event = next((fields(line) for line in lines if 'student first presented frame' in line), {})
        first_decode = first_decode_event.get('unix_ms')
        first_present = first_present_event.get('unix_ms')
        # IDR arrival is earlier than successful decode/presentation; keep it named.
        result = {'summary': summary,
                  'first_present_logged': any('student first presented frame' in line for line in lines),
                  'zero_decode_intervals': sum(row.get('decode_fps') == 0 for row in intervals),
                  'startup_zero_decode_intervals': sum(row.get('decode_fps') == 0 and (first_decode is None or row.get('unix_ms', 0) < first_decode) for row in intervals),
                  'idr_arrival_recovery_peak_ms': max([row.get('recovery_peak_ms', 0) for row in recovery] or [0]),
                  'last_recovery': recovery[-1] if recovery else {},
                  'last_ingress': ingress[-1] if ingress else {}}
        result['first_decode_ms'] = first_decode_event.get('first_decode_ms')
        result['first_present_ms'] = first_present_event.get('first_present_ms')
        result['decoded_output_recovery_peak_ms'] = max([row.get('decoded_recovery_peak_ms', 0) for row in recovery] or [0])
        result['decoder_no_output_resets'] = max([row.get('decoder_no_output_resets', 0) for row in recovery] or [0])
        result['incomplete_recovery_units'] = max([row.get('incomplete_recovery_units', 0) for row in recovery] or [0])
        result['post_first_decode_zero_intervals'] = sum(row.get('decode_fps') == 0 and first_decode is not None and row.get('unix_ms', 0) >= first_decode for row in intervals)
        result['first_decode_to_first_present_log_ms'] = first_present - first_decode if first_decode is not None and first_present is not None else None
        result['functional'] = bool(result['first_present_logged'] and summary.get('decoded_frames', 0) > 0
                                    and summary.get('rendered_frames', 0) > 0)
        workload = args.run / f'{name}-workload.json'
        if workload.exists():
            audit = json.loads(workload.read_text())
            result['workload_audit'] = {
                'samples': len(audit),
                'foreground_percent': 100 * sum(row['foreground'] for row in audit) / len(audit) if audit else 0,
                'visible_percent': 100 * sum(row['visible'] for row in audit) / len(audit) if audit else 0,
                'phases_seen': sorted(set(row['phase'] for row in audit)),
            }
        wire = result['last_ingress']
        injected = wire.get('injected_dropped_packets')
        result['injected_percent_of_peer_datagrams'] = (100 * injected / wire['wire_packets']
            if injected is not None and wire.get('wire_packets', 0) > 0 else None)
        report['scenarios'][name] = result
    (args.run / 'analysis.json').write_text(json.dumps(report, indent=2))
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
