# Adjustable settings (2026-10-16)

Values that used to be fixed in code are now settings. **Every default is how the
application behaved before**, so nothing changes until a teacher changes it.
`settings_params.dart` is the one list of what the Settings screen can change
(label, unit, range, default, where it is stored); a test checks each default
against `config/default.toml`, and the Rust side validates every value again.

## Where things are

| Page (group) | Settings | Stored in | Takes effect |
| --- | --- | --- | --- |
| Danh sách & giám sát (Máy học viên) | label shown on a tile, click action, view/sort/tile size/offline defaults, "picture is old" threshold, minimum gap between connection-lost alerts | console preferences (alert gap: `activity.alert_cooldown_seconds`) | at once |
| Theo dõi hoạt động | scan interval, apps per tile (1–5), clipboard on/off, downloads on/off, AI evidence screenshots on/off, pictures per site (1–10), seconds of use before the first picture and minimum gap between pictures | `[activity]` | at once, also on Students (sent with the monitoring switch) |
| Quy tắc lớp | warning before shutdown/restart (0–300 s) | `[class]` | at once; Student uses the value sent with each command |
| Ghi màn hình (Lưu & truyền) | max session length, 360/480/720/1080p, fps, bitrate, part length, save folder, disk stop/warn limits for Teacher and Student, "Teacher out of touch" limit | `[recording]`, `tools.recording_max_minutes` | next recording; running ones keep theirs. Quality and Student limits travel with Start, so Students keep no settings |
| Truyền tệp | file size limit, speed per transfer | `tools.max_file_mb`, `[transfer]` | at once |
| Thông báo (Hệ thống) | how long toasts stay, replies kept | console preferences | at once |
| Phát màn hình → Nâng cao | jitter buffer, repair wait, keyframe interval | `[video]` | needs the "Lưu & kết nối lại" restart |

"Lưu & áp dụng ngay" (no restart) is offered when nothing that needs a restart changed;
the Teacher then reloads the stored settings (`settings_apply`) and the Students are not
disconnected. Each setting has a reset button for its default, and each page has "Đặt lại
trang này về mặc định". The Phát màn hình page also has quick profiles (wired room, Wi-Fi,
weak machines) that fill the existing quality fields.

## Safety rules

- A part of a recording must stay under the 1 GiB transfer limit: part length × bitrate
  is capped at 100000 (kbps·minutes).
- Disk limits: stop ≥ 256 MiB, warning ≥ stop. "Out of touch" is 0 (never) or 5–240 min.
- A Student clamps whatever it is sent to what its helper can run.
- The recording folder is a free path (an absolute path is required). A slow or network
  drive can make recording fail; the dialog says so.

## Behaviour notes / not done

- Single click on a computer opens Observe (as before); the new option "chỉ chọn máy"
  turns that into selection. A double-click action was not added: it would conflict with
  the single click that already acts.
- **Duration of Internet / application / VS Code blocks** (until end of day, 30/60/90 min,
  …) is **not done**. It needs Students to expire three kinds of rules and the Teacher not
  to re-send them; it is the largest remaining item (see PROJECT_NOTES).
- No native folder picker yet (type the path).
- Student-side effects (recording quality, activity options, shutdown warning) need an
  updated Windows Student; older Students ignore them and use the old behaviour.
