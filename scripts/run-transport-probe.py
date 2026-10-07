#!/usr/bin/env python3
"""Sequential same-host production fan-out probes; never physical scale acceptance."""
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/debug/transport_probe')
    parser.add_argument('--output', type=Path, default=ROOT / 'target/transport-probe/matrix')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    rows = []
    cases = [(f'ladder-{n}', n, 361003, 25000, 0) for n in (1, 5, 10, 20, 35, 40)]
    cases += [('oversized-normal', 35, 361003, 200000, 0),
              ('oversized-idr', 35, 600000, 25000, 0),
              ('auxiliary', 35, 361003, 25000, 12)]
    for name, receivers, idr, normal, auxiliary in cases:
        path = args.output / f'{name}.json'
        command = [str(args.binary), '--receivers', str(receivers), '--frames', '60',
                   '--idr-bytes', str(idr), '--normal-bytes', str(normal),
                   '--auxiliary-packets', str(auxiliary), '--output', str(path)]
        with (args.output / f'{name}.log').open('w') as log:
            subprocess.run(command, check=True, timeout=45, stdout=log, stderr=log)
        report = json.loads(path.read_text())
        sender = report['sender']
        received = sum(s['packets'] for s in report['sinks'])
        complete = [s['complete_aus'] for s in report['sinks']]
        durations = sorted(a['send_complete_ms'] for a in report['aus'])
        row = dict(case=name, receivers=receivers, submission_fps=report['submission_fps'],
                   sent=sender['sent_packets'], received=received,
                   sink_unreceived=sender['sent_packets'] - received,
                   complete_au_min=min(complete), complete_au_max=max(complete),
                   idrs=sum(a['idr'] for a in report['aus']),
                   completion_p95_ms=durations[int((len(durations) - 1) * .95)], **sender)
        # Delivery loss remains a measured result, never masked as a sender failure.
        assert received <= sender['sent_packets'], 'sink packet accounting exceeds sender'
        assert all(s['duplicates'] == 0 for s in report['sinks']), 'duplicate synthetic indexes'
        if name == 'oversized-idr':
            assert sender['sent_packets'] == 0 and sender['au_deadline_drops'] > 0
        rows.append(row)
        print(json.dumps(row), flush=True)
    (args.output / 'summary.json').write_text(json.dumps(rows, indent=2))


if __name__ == '__main__':
    main()
