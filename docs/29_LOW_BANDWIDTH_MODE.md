# Economy mode: weak network and weak Student PCs

Owner request (2026-10-14). Some school labs have Students that stall with two or
three Chrome tabs open and take minutes to boot, and some rooms rely on Wi-Fi. One
class-wide Teacher setting trades picture quality for less network traffic and
less work on the Student PC. It must never make the default experience worse.

## What costs what

| Cost | Where it is paid | What drives it |
|---|---|---|
| Network (Teacher uplink, Wi-Fi air time) | Teacher to every Student | bitrate x number of Students, repair and keyframe bursts, thumbnails |
| Student decode and render | Student PC | stream resolution x fps (bitrate matters little) |
| Student capture and encode | Student PC, only for Observe/Control and when a Student presents | Observe fps, resolution, the profile the Teacher offers |
| Student thumbnail capture + JPEG | Student PC, every thumbnail interval | thumbnail size, quality, interval |

So one mode has to lower resolution and fps (Student decode), bitrate (network),
Observe load and thumbnail work, together.

## Settings (Teacher, page "Phát màn hình")

`[performance]` in the Teacher config overlay (kept apart from the values the
Teacher chose for video, so switching the mode off restores them exactly):

- `mode`: `normal` (default, nothing changes), `economy`, `minimal` (very weak).
- `auto_wifi` (default off): behave as `economy` while the Teacher's adapter is
  wireless and `mode` is `normal`.
- `bandwidth_cap_mbps` (0 = no extra limit): the most the Teacher sends to the
  whole class per second.

## What each mode changes

A mode only ever lowers a value; a Teacher who already chose less keeps it.

| Setting | normal | economy | minimal |
|---|---|---|---|
| Class picture, largest size | as chosen | 1280x720 | 960x540 (aspect kept) |
| Max fps / typing fps / idle fps | as chosen | 20 / 15 / 4 | 12 / 10 / 3 |
| Bitrate target / max / min (kbps) | as chosen | 3000 / 4000 / 800 | 1200 / 1800 / 400 |
| Refinement frame (% of target) | 250 | 150 | 100 |
| Observe/Control | enhanced, up to 60 fps | broadcast profile, up to 30 fps | broadcast profile, up to 20 fps |
| Thumbnail interval | 2 s | 4 s | 6 s |
| Thumbnail size, JPEG quality | 480x270, 85+ | 360x203, 60 | 240x135, 50 |
| Sharper thumbnails on HiDPI Teachers | yes | no | no |

Reading the table: 720p15 is what a weak PC can decode, readable for text; the
bitrate cut protects Wi-Fi; Observe drops the 60 fps hardware profile because a
weak Student would be the one encoding it.

## Bandwidth cap

The cap is for the class, so it is turned into a per-Student share when a
presentation starts: `each = cap x 0.75 / recipients` (at least 300 kbps; the
other 25 % is headroom for keyframes and repair). Target and maximum bitrate are
clamped to it. The wire budget (`send_pacing_mbps`, a safety limit the pacer lets
the video use 80 % of) becomes 2.4 x the cap, or 2.4 x what the Students were
actually allotted if the 300 kbps floor makes that larger, so keyframes and repair
still fit. (A first version set the budget equal to the cap and sent nothing; a keyframe needs roughly 2-3x the average bitrate to leave in time. Measured worst case: incompressible scrolling random text under a 2 Mbps cap to 2 Students still sent almost nothing, 6 Mbps worked at 3.3 Mbps.) The
picture steps down
with the share, because a large keyframe that does not fit the wire budget in time
is dropped (a first test with 1080p under a 2 Mbps cap sent nothing at all):

| Share per Student | Picture and frame rate |
|---|---|
| 4000 kbps or more | as chosen |
| 2000 to 3999 | at most 1280x720, 20 fps |
| 900 to 1999 | at most 960x540, 12 fps |
| under 900 | at most 640x360, 10 fps |

Example: 30 Mbps and 40 Students leave 562 kbps each, so 640x360 at 10 fps. The
settings window shows the share for the Students online now.

## Where it is applied

- `classroom-config`: `PerformanceConfig`, `Config::with_profile(wireless)` and
  `Config::for_stream(recipients)`. Two new monitoring fields (`quality_floor`,
  `hidpi`) let the thumbnail request go below the old hard floor of 85 only when
  a mode asks for it.
- Teacher agent: runs on the effective config; the settings the dialog reads and
  saves stay the raw ones. Each presentation gets `for_stream(recipients)`.
  The published state carries what is in force and why.
- `monitoring.rs`: thumbnail request quality and HiDPI doubling follow the config.
- Settings dialog: a three-state selector, the Wi-Fi checkbox, the cap field, a
  line saying what is in force now.

Changing the mode needs "Lưu & kết nối lại" like every other Teacher setting.

## Not in this change (ideas)

- Per-Student mode (only the weak PCs), automatic detection from Student load.
- Student-side limits (pause the activity collector while presenting, lower
  thumbnail work when the Student CPU is busy).
- Lowering Student boot time is outside the product.

## Verification plan

1. Unit tests: a mode never raises a value; all values stay valid; aspect kept;
   `normal` is byte-identical; `minimal` <= `economy`; cap arithmetic; Wi-Fi rule.
2. Settings dialog: mode, checkbox, cap saved in the overlay; raw video values
   untouched.
3. Real run: Teacher in each mode, class presentation to the Windows and Mac
   Students: published state, encoder settings in the log, measured bitrate and
   fps well below normal; Observe still opens fast; thumbnails smaller.
