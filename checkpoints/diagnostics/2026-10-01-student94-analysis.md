## Windows Student .94 retest — 2026-10-01

Teacher Mac 192.168.50.188; Windows Student mvmanh@192.168.50.94.
Existing latest release deployed; SHA256 verified as
5b0f94b7a936058c4c8c3c6a470c215b7c975e18c00a7fe6b7b81267eedfa3c6.
Interactive limited scheduled task launched Student in active console session;
Student task exit 0 and Teacher exit 0. Temporary task removed. Credentials
not saved. No production code or firewall changes.

Student: 1138 decoded /1075 rendered over 43.742 media seconds; decode26.02FPS,
render24.58FPS; zero decode errors, 2 depacketizer sequence-gap events,
4 legacy missing packets, 6 NACK sequences and 2 keyframe recovery requests.
Teacher retransmitted all6 requested packets, zero cache misses/send errors.
Render recovered after each gap; no unrecovered decoder failure.
Decode p50/p95 8.62/12.82ms; render CPU p95 0.466ms; final control RTT6.271ms.
Sample-weighted capture-to-present estimate207.34ms (906samples),
send-to-present169.57ms (1075samples). These are clock-dependent application
estimates, not optical latency. Largest interval capture-to-present p95 530ms,
maximum sample568ms: occasional freshness spikes remain.

Host telemetry identifies Parallels Display Adapter (WDDM),4virtual CPUs,
8GB RAM; this remains a VM test. No controlled moving-content workload or
optical measurement. Similar mean latency to previous .155 test, but workload
and network differ, so no causal hardware/performance comparison is claimed.
Full logs: checkpoints/evidence/windows-2026-10-01-student94/.
Next: investigate stage timing and recovery spikes with repeatable content on
this automated endpoint; real wired Windows scaling acceptance remains open.

