## Mac Teacher to Windows 11 Parallels Student — 2026-10-01

Teacher 192.168.100.219; Student 192.168.100.155. SSH authenticated and
SHA256 verified Windows binary 5b0f94b7a936058c4c8c3c6a470c215b7c975e18c00a7fe6b7b81267eedfa3c6.
Student launched with CLI through an Interactive/limited scheduled task in the
active console session. Battery-safe task settings were necessary; initial task
attempts did not launch. Temporary task removed after completion. No firewall
settings or production code changed; credentials were not saved.

Student completed successfully: 689 decoded / 576 rendered in 24.047 seconds
of media, 28.65 decode FPS / 23.95 render FPS, zero decode/depacketizer errors
and zero final missing packets. One NACK sequence was retransmitted by Teacher.
Decoder: Microsoft H264 Video Decoder MFT (hardware decode is not established).
Renderer: D3D11, Parallels Display Adapter (WDDM), GPU NV12 conversion.
Decode p50/p95 8.19/16.16 ms; render CPU p50/p95 0.277/0.454 ms.
Sample-weighted source capture-to-present estimate 205.3 ms (512 samples);
source send-to-present estimate 168.3 ms (576 samples). These estimates depend
on clock synchronization and are not optical glass-to-glass measurements.
An initial control deadline retried successfully while Teacher was starting.

Full logs and derived stage metrics: checkpoints/evidence/windows-2026-10-01-parallels/.
This is a functional VM/Wi-Fi smoke test, not physical Windows LAN or scaling
acceptance. Next: automate repeatable workload and lifecycle/recovery scenarios
on this Windows endpoint; investigate timing from the complete collected logs
before changing codec settings. No claim of visually verified text quality.

