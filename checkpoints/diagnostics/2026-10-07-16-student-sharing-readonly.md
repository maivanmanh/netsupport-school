# 16-Student screen sharing — read-only log analysis

Owner prohibits deployment, tests or interruption of live exam endpoints. This
analysis only reads local Teacher logs and source; no IPC commands, Student log
collection requests, restarts, configuration changes or tests were performed.

Source: local `%APPDATA%/com.classroom/classroom_console/teacher-log.txt` and
`ui.log.txt`. Session stream_id=1463046557256934658, 11:20:09–11:23:31
Asia/Bangkok on 2026-10-07; media elapsed 202.18075 seconds. Prior two-recipient
session was excluded. 196 interval records and reports from 16 distinct receivers.

- Actual capture/stream 1920x1080, Intel UHD 770; hardware Intel Quick Sync H.264,
  one encoder, four UDP fan-out workers, 16 selected recipients. NVIDIA candidate
  activation failed; Intel fallback initialized and accepted low-latency mode.
- Teacher encoded 4430 frames, average 21.91 FPS; encode p50 11.09 ms/p95
  17.37 ms. Actual encoded stream 4.387 Mbps average; aggregate sends 72.142 Mbps,
  highest logged interval 201.695 Mbps. Capture 35.71 FPS is source acquisition,
  not transmitted FPS. 3245 waiting-frame replacements are freshness shedding.
- Receiver cadence from first/last report counter differences: decode
  20.22–20.83 FPS, render 19.75–20.41 FPS. All 16 reported rendering enabled;
  no receiver collapsed to near-zero cadence. These counters do not measure
  optical smoothness or exclude short stalls.
- Each receiver last report has 310–573 missing packets out of roughly 109k
  received (roughly 0.28–0.52%). Missing counters combine expiry/late effects;
  this is not a physical network-loss measurement. Teacher recorded 2989 NACK
  requested sequences, 2111 resent packets, 555 cache misses, 323 repair send
  errors. Other requested packets must not be assumed successfully repaired.
- Teacher recorded 260 WouldBlock occurrences, 22 retry recoveries, 238 send
  errors and 9001 abandoned datagrams; pointer send errors 150. Warnings explicitly
  name WSAEWOULDBLOCK 10035. Aggregate Mbps alone cannot exclude burst/backpressure.
- Adaptive bitrate changed 24 times, 6 Mbps down to 1.5 Mbps; median interval
  target 1.65 Mbps and target FPS 20 (range 5–30). Actual bitrate exceeds target
  in several intervals; source includes refinement with configured 250% bitrate.
  Log evidence here does not isolate refinement versus encoder rate-control effects.
- Teacher CPU interval median 88.07%, p95 134.85%, max 155.73%, in sysinfo summed
  core units on 28 logical CPUs, not total-machine percentages. Memory interval
  median ~155.5 MiB, max ~158.4 MiB. quality.rs compares summed CPU directly with
  adaptive_pressure_cpu_percent=80; it can trigger pressure while using under
  one full core. This merits review; it does not prove the sole reason for every
  adaptive transition. Encode p95 intervals remain below the configured 28 ms
  pressure threshold (maximum logged interval p95 27.804 ms).
- Per-receiver median true control RTT 0.467–1.113 ms; reported RTT spikes reach
  183–249 ms. Many render-call p95 reports rise near 30–35 ms. Neither measurement
  is capture-to-display optical latency. Monitoring thumbnail round_trip_ms is
  an interval-paced request cycle and must not be used as network RTT.
- Teacher agent logs remain online=36 around and after sharing; selected media
  recipient count stays 16 after join. No room disconnection evidence found in
  the analyzed session. Teacher Stop is logged; remote preview closure timing
  requires Student evidence, not available from this read-only Teacher analysis.

Conclusion: real 1080p delivery/rendering to 16 endpoints is supported by reports,
at approximately 20 FPS. This run has transport errors and strong adaptation,
so it does not demonstrate stable 1080p30 or 40-PC acceptance. Next step during
exam remains passive source/log review, particularly CPU threshold semantics and
UDP burst scheduling. Any deployment or active validation needs later explicit
owner authorization after the exam.
