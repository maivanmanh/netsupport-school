#!/usr/bin/env python3
"""Checks fault-test logs; same-host close delays, not optical latency."""
import json
from pathlib import Path
import re
import sys


def analyze(folder):
    folder=Path(folder)
    status=json.loads((folder/'status.json').read_text())
    rows={}
    for scenario,result in status.items():
        win=scenario.startswith('mac-')
        logs=sorted(p for p in (folder/'windows'/scenario).glob('student*.log') if re.fullmatch(r'student-\d+(?:-restart)?\.log',p.name)) if win else [folder/(scenario+'-student.log')]
        text='\n'.join(p.read_text(errors='replace') for p in logs if p.exists())
        first=text.count('student first presented frame')
        closed=text.count('student preview closed after Teacher disconnect')
        timeout=text.count('teacher control liveness timeout')
        fault=scenario.split('-',1)[1]
        ok=result.get('error') is None and result.get('exits')==[0] and first>=2
        if fault!='studentcrash': ok=ok and closed>=1
        if fault in ('silent','blackout'): ok=ok and timeout>=1
        intervals=[]; pending=None
        for line in text.splitlines():
            match=re.search(r'unix_ms=(\d+)',line)
            if not match: continue
            at=int(match[1])
            if 'teacher control liveness timeout' in line: pending=at
            if 'student preview closed after Teacher disconnect' in line and pending is not None:
                intervals.append(at-pending);pending=None
        errors=[int(v) for v in re.findall(r'\bdecode_errors=(\d+)',text)]
        rows[scenario]={'pass':bool(ok),'first_presented_events':first,'disconnect_closes':closed,
                        'liveness_timeouts':timeout,'timeout_to_preview_close_ms':intervals,
                        'max_logged_decode_errors':max(errors,default=0),
                        'worker_error':result.get('error')}
    (folder/'lifecycle-analysis.json').write_text(json.dumps(rows,indent=2))
    print(json.dumps(rows,indent=2))
    return all(r['pass'] for r in rows.values())


if __name__=='__main__':
    raise SystemExit(0 if analyze(sys.argv[1]) else 1)
