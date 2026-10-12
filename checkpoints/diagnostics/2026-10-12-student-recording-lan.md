# Student recording — LAN validation, 2026-10-12

**Recording functional validation passed on one Windows ARM VM:** two real
ten-minute rollovers, automatic per-chunk collection, short final Stop, retained
session across Teacher restart and concurrent Observe/presentation. Limitations
and a separate black one-shot screenshot observation are recorded below.

## Environment and paired deployment

- Source: `f3c1062`; no media/recording implementation changes during this test.
- Teacher: macOS ARM64, `192.168.50.28`, stable existing bundle
  `/Users/mvmanh/Development/netsupport-school/target/Classroom Console.app`.
- Windows Student: `192.168.50.166`, Windows 11 Pro 26100 ARM64, Parallels ARM VM,
  4 logical processors, 8 GB RAM, virtual Ethernet. Student is the x64 build under
  emulation, using **H264 Encoder MFT (software)**. This is not physical Windows
  x64 or wired 40-PC acceptance.
- Mac Student: `192.168.50.2`, Apple Silicon, existing bundle
  `/Users/mvmanh/Classroom/Classroom Student.app`.
- Fresh paired build label: `recording-validation-20261012` via
  `CLASSROOM_BUILD_LABEL=recording-validation-20261012 CLASSROOM_MAC_OUTPUT_DIR="$PWD/target/recording-validation-build" bash scripts/build-flutter-macos.sh`.
  Teacher, Mac Student, Windows x64 Student/Guard built successfully. Build log:
  `target/recording-validation-build.log`.
- Both fresh Student ZIPs were byte-verified against Teacher's embedded packages;
  manifest hashes match their executables. Windows Student SHA-256:
  `22524e764af276c9c717ae94fdbe4eed09dd84d16ac8907f1c6ffcf0fcaa6b29`;
  Mac Student: `aa92be35cbf78c60ba13320d0e4eba29e1a69d20d676bf929adc33166fd80218`.
- Existing Students were reconnected from old Teacher IP `.206` to `.28`, then
  updated **through Classroom updater** at the same install paths. Stable device
  IDs were retained and both replacement builds reconnected/confirmed. The Guard
  service was restarted to reload the Windows launch target; its installed binary
  was not replaced over SSH. Update logs: `target/recording-{mac,windows}-update.log`.
- Teacher's exact stopped bundle was replaced once; settings/profile/logs stayed
  separate. The owner manually granted Mac Student Screen Recording/Accessibility
  and Teacher Screen Recording. No TCC database or execution policy was changed.
  Student was restarted through LaunchServices; its fresh thumbnail and 1920×1080
  one-shot screenshot succeeded. A sandboxed Teacher `open` retry reported
  `kLSNoExecutableErr`; verifying the existing executable/signature and launching
  outside that sandbox succeeded without replacing or re-signing the app.

## Runtime evidence obtained

1. **Short Stop/upload/decode:** session `4ecfd3a3830fa8bcb07f6366b3c6396f`.
   Output `/Users/mvmanh/Desktop/MAIVANMANHED30/Recording-1791798303-4ecfd3a3/Part-0001.mp4`:
   H.264, 960×720, 30.151278 seconds, 637 frames, 1,313,450 bytes, no audio.
   The 4:3 source retained its ratio; the cursor is visible. `ffmpeg -v error ...
   -f null -` decoded without errors. Source MP4 was removed after ACK.
2. **Long session:** `3225056a9ddc4fee6987d11361b9abe0`, destination
   `/Users/mvmanh/Desktop/MAIVANMANHED30/Recording-1791798416-3225056a`.
   Source display stayed 1440×1080; recording stayed 960×720. Initial desktop/text
   was static; the owner-approved native workload began about five minutes into
   the session, alternating code, scrolling, timestamps and full-screen color.
   Its independent log records foreground status and geometry once per second.
   The workload was launched inline only after explicit owner approval, because
   automatic review rejected doing that under Windows' file-script restriction.
3. **Teacher restart/offline rollover:** near the first ten-minute boundary,
   Teacher exited for the owner-granted Screen Recording permission. Windows
   recording helper PID **9604**, parent Student PID **7496**, remained unchanged;
   its local report advanced to chunk 2 while Teacher was offline. Teacher reloaded
   the same journal/session, Student reconnected, and chunk 1 uploaded automatically.
   Chunk 1 was received while chunk 2 was still recording; the acknowledged source
   vanished from the Student spool.
4. **First ten-minute file:** `Part-0001.mp4`, 73,083,007 bytes, H.264 960×720,
   599.979878 seconds, 14,507 frames (24.18 average FPS), initial I/keyframe,
   no audio. Independent full decode passed; log `target/recording-part1-decode.log`
   is empty, process exit 0. Probe: `target/recording-part1-probe.json`.
5. **Concurrent features:** Teacher recording dialog visibly displayed live phase,
   elapsed time, measured FPS, dimensions and received/pending counts. One-shot
   Windows screenshot succeeded during recording (1440×1080), monitoring recovered
   afterwards. Observe reached `live` with H.264 metrics and recording continued;
   evidence `target/recording-observe-evidence.json`. Selected Teacher presentation
   reached `sharing`, Windows decode/render reports were nonzero, and a Student
   screenshot visibly confirmed the presented Teacher screen while recording kept
   running. Stopping presentation restored fresh thumbnails on both Students.
6. **Unsupported Mac recording:** Start for Mac was rejected and retained-session
   count stayed unchanged. Its capability is `recording:0`; capture/Observe support
   does not imply a Mac MP4 recording backend. IPC currently returns the generic
   `File operation failed` acknowledgement for this rejection.

## Final long-run result

Teacher received chunk 2 while Student was recording chunk 3. Stop then finalized
and uploaded the last short part. Final report: `phase=stopped`, three completed
and three received chunks, zero queued chunks, no upload error. The helper exited;
only the normal Student remained. Source MP4s and partials were absent after ACK.
Total received size: **202,957,971 bytes**.

| File | Duration (seconds) | Frames | Average FPS | Bytes |
| --- | ---: | ---: | ---: | ---: |
| Part-0001.mp4 | 599.979878 | 14,507 | 24.18 | 73,083,007 |
| Part-0002.mp4 | 600.012067 | 15,262 | 25.44 | 125,197,459 |
| Part-0003.mp4 | 19.056733 | 507 | 26.60 | 4,677,505 |

All files independently start on an I/keyframe, contain one H.264 960×720 video
track and no audio, and match their durable Teacher receipts by size and SHA-256.
Every input packet has strictly increasing PTS/DTS and a positive duration. Full
decode of every frame passed with empty error logs using the variable-rate timeline:

`ffmpeg -v error -i <part> -fps_mode passthrough -enc_time_base demux -f null -`

The default null-output command on parts 2/3 reported duplicate **output** DTS
after frame-rate/time-base quantization. These were not damaged input packets:
the independent packet audit and the explicit demux-time-base decode above pass.
Logs: `target/Part-000{1,2,3}-decode-vfr.log`; file/hash/probe summary:
`target/recording-files-verified.json`. Ten minutes is a rotation target; the second
file exceeds 600 seconds by 12 ms. Frame-exact/gapless rollover is not established.

The moving-workload interval's 174 sampled FPS reports had median **26.70**,
range **17.75–29.55**. This includes simultaneous feature tests and is not a
guarantee of a 24-FPS floor. Workload log: 917 samples, fixed 1440×1080, approximately
916 seconds; all samples reported foreground, but Teacher's fullscreen presentation
temporarily covered that workload, as separately verified by screenshot. Extracted
MP4 frames confirm readable text/scrolling after presentation and in the final part.
Part 2's full `blackdetect` scan found no >=0.2-second interval with >=98% near-black
pixels (threshold 0.02). This is a defined check, not a claim that every frame was
manually inspected.

**Separate issue:** one Windows one-shot screenshot during the long run was black
apart from the cursor while the owner and the Mac-host screenshot confirmed a
normally visible VM. MP4 samples at 300/500 seconds of part 2 and 10 seconds of
part 3 remained correct. Its root cause/reproduction is pending; do not label it
a minimized VM or claim all screenshot/capture combinations passed. No capture
backend was redesigned during this recording validation.

The owned workload exited normally (`LastTaskResult=0`); its temporary scheduled
task was unregistered. Teacher is idle; both Students remain on the fresh build.

## Evidence and limits

Live status samples: `target/recording-runtime-status.jsonl`; monitor output:
`target/recording-long-test.log`, `target/recording-watch.log`. Build packages:
`target/recording-validation-build`, `target/updates`. Raw screen artifacts stay
local in Desktop/ignored target directories; they are not committed to Git.

Not exercised: multiple Windows machines/bulk runtime collection, hardware MFT,
physical x64, 16:9/portrait/small display runtime variants, disk exhaustion, lock/
sleep, forced Student/helper crash, byte-interrupted upload or classroom-scale load.
Existing unit tests cover several admission/retry/format cases; they are not runtime
evidence for these omitted scenarios. No broadcast/scaling acceptance is upgraded.
