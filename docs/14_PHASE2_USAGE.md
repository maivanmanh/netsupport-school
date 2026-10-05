> Current role and packaging baseline: [Teacher/Student architecture](15_ROLE_ARCHITECTURE.md).

# Phase 2 development build

## Launch

First launch shows Teacher/Student and Teacher IPv4 setup. Save once; subsequent
double-clicks open the Teacher console or run the Student agent without a launcher.
Local development Teacher address is currently `192.168.100.219`.

- `Classroom --setup`: change the remembered role/address.
- `Classroom --role teacher --teacher-ui`: explicitly open the native console.
- `Classroom --role teacher`: legacy immediate presentation CLI for automation.
- `Classroom --role student --teacher-ip ADDRESS --remember`: save and start Student.
- `--launch-profile PATH`: isolate profiles and device identity for automated tests.

Profiles are per OS user: Windows `%APPDATA%/Classroom/profile.toml`; macOS
`~/Library/Application Support/Classroom/profile.toml`. Student identity is saved
beside that profile. Copying a binary does not copy the role/address configuration.
Windows Release double-click does not open a CMD console; use `log.txt` diagnostics.
All peers must use protocol version 8 builds together.

## Console

Grid/List show online/offline machines and latest screen snapshots. Click cards or
rows to toggle selection. Inspect enlarges one selected snapshot; Escape closes it.
Share Class broadcasts to the room. Share Selected broadcasts only to selected
Students that are online at Start. Stop releases recipients to their desktops.
The recipient set stays fixed for that presentation; selection changes apply to
the next Start. Selected Students can reconnect; other late joins stay on their
desktop. Each group presentation still captures/encodes once.
Files/Devices Ribbon headings reserve future groups and have no machine commands.
Home/View headings are visual grouping labels in this first console implementation.

Snapshots are low-resolution overviews, not text-reading remote-control streams.
Default refresh is about two seconds, aspect-fit within 480x270, JPEG quality65.
They pause while students watch Teacher fullscreen, and resume after Stop.
No product screenshot archive is written. A stale/unavailable state is visible.
On macOS the Student needs Screen Recording permission to provide snapshots.

Optional external TOML configuration:

```toml
[monitoring]
enabled = true
interval_ms = 2000 # 1000..10000
width = 480       # 2..480
height = 270      # 2..270
jpeg_quality = 65 # 30..85
```

Monitoring uses TCP control-port+1 (default47801), independent of control47800
and UDP media47810/feedback47811. An OS firewall may require permission for the
executable. Test automation uses temporary, peer-restricted rules and removes them.
No persistent firewall rules were added by this delivery.

## Development deployment

- Local Mac binary: `target/release/Classroom`, remembered Teacher.
- Windows `.155`: `C:/Users/lolem/Classroom-two-mac/bin/Classroom.exe`, remembered Student.
- Mac `.254`: `~/Classroom-two-mac/bin/Classroom`, remembered Student.
- Windows portable copy: `/Users/mvmanh/Downloads/Classroom.exe` on development Mac.

## Desktop lifecycle and settings

The native tray/menu bar shows connection status, Settings and Exit. Teacher also
has Open Teacher console. Closing Teacher hides its console when tray creation
succeeds; sharing and room connections continue. Exit Classroom ends the room.
If tray creation fails, closing exits normally. Student stays hidden until receiving
a fullscreen presentation and runs a hidden native event host while idle.
Headless diagnostics omit this desktop host/tray.

Settings saves role/Teacher IPv4 and restarts with that profile. It is disabled
during presentation. Student disconnects while its Settings dialog is open.
Cancel leaves settings unchanged. Restart uses saved GUI defaults rather than
preserving temporary development CLI overrides. Grid/List and window dimensions
are stored beside the profile in `ui-preferences.toml`, with bounded size defaults.
There is no OS startup installer or Windows Service yet.

Teacher and setup frames are prepared with the OS system font before exposing
the window. Buttons have hover, pressed and disabled states, subtle bevel/shadow
and sentence-case labels. macOS uses SF system font; Windows uses Segoe UI.

## Teacher messages, screenshots and file delivery

Teacher toolbar buttons send a short message or one file to the class, or to the
online computers currently selected in Select mode. Inspect has the same actions
for one Student and a screenshot button. Student messages appear in a small native
Rust window; the Student can close it or send one reply when replies are enabled.
Replies appear in the Teacher status card. Messages are limited to 2,000 characters.

Screenshots are PNG files on the Teacher's Desktop named with the Student and
local date/time. On macOS, the first screenshot asks the Teacher to select Desktop
once so the sandbox can save there; Windows resolves Desktop through the OS
known-folder API. Received files are also saved directly on each
Student's Desktop on both Windows and macOS; the original filename is retained,
with a unique prefix/suffix to avoid replacing an existing file. File delivery has
a progress bar, cancel and retry. It uses a separately paced TCP connection on
Teacher control port + 2; it does not share video or media queues. Transfers are
limited to 4 GiB each, at most two concurrent receivers, with SHA-256 verification.


## Automated verification

`python3 scripts/run-phase2-test.py` deploys through SSH, runs both Teacher roles,
collects logs and exports actual native console frames. Passwords stay in memory.
`--skip-mac --min-thumbnails 1` tests the available local Mac/Windows pair.
`--scenarios mac-teacher --restart-student --probe-screen` adds a Windows Student
kill/relaunch probe and a diagnostic desktop application.
`python3 scripts/analyze-phase2-test.py target/phase2-runs/RUN` checks outcomes.
Diagnostic UI exports are opt-in and contain desktop snapshots; normal operation
never writes them. Retain full logs for acceptance. These development tests do not
replace physical 40-PC wired LAN or optical latency acceptance.

`--selective` tests class-wide stream1 followed by selected-only stream2.
Both Students receive stream1; exactly one receives stream2. The analyzer checks
recipient scope and the excluded Student log, not only process exit status.

`--desktop --selective --min-thumbnails 1` adds prepared startup frames, Settings
cancel and native close-to-tray/reopen while sharing, with both Teacher roles.

## Retina and thumbnails

Teacher and Settings draw at the macOS window's native backing density (2x on
Retina), including glyph rasterization, without enlarging the logical UI controls.
Moving/resizing rechecks backing density; Windows keeps its existing sizing.
Grid preview regions are 16:9; Grid/List fill while preserving source aspect ratio,
center-cropping excess content rather than introducing letterbox/pillarbox bands.
The enlarged inspection view still shows the entire source using aspect fit.
Black areas already present in a Student desktop remain part of that desktop.
Quick taps are retained through the native event queue, including down/up that
both occur inside one update. `--desktop` also exercises this on macOS with an
explicit zero-pressure diagnostic mouse pair; normal runs never inject input.

Use `--port-base 48700` for an isolated integration run when another Classroom
instance uses production ports. TCP/monitor/UDP/feedback ports and temporary
peer-restricted firewall rules follow the chosen base; normal app defaults do not change.

## Flutter roster and compact view (2026-10-02)

Teacher offers Grid, List with thumbnails, and Compact List without images.
Compact rows show hostname, IP and a connection-status dot. View is remembered;
selection survives view/filter changes. Select online selects visible online rows;
Clear selection clears all. Share Selected includes only selected online Students
at Start; offline selected rows remain selected but are not recipients.

Known IDs/names/IPs are stored in console Application Support/known-students.json
(no thumbnails on disk), bounded to256 entries. At startup remembered machines
are offline until confirmed by the Rust agent. Sorting is online first, then name,
then numeric IP. The current room is keyed by Teacher address and control port;
changing rooms resets the cache. This is current-room history, not a multi-room
management database. Roster persistence currently belongs to the Teacher console.

Rescan refreshes the authoritative agent roster without ending presentation.
It does not sweep subnets or force an unreachable/offline PC to start. Students
continuously connect/reconnect to their configured Teacher; new registrations and
heartbeat failures update the roster automatically. Existing control liveness
budgets determine when a silent peer becomes offline. The UI shows command progress,
connection refresh feedback, snapshot stale status (>10s), and a readable capture
permission error. Capture permissions still depend on the OS-approved build.

## Contextual Teacher toolbar

Thin header shows room counts and active/connecting status as text. Unified toolbar
contains contextual presentation action, search, Rescan, Selection popup and three
view icons. Idle: Share class; Share selected only if selection contains online
machines. Starting/Sharing: Stop sharing. Stopping: progress label. Settings is
disabled while presenting or processing a command. Selection popup contains Select
visible online and Clear selection. Narrow windows wrap selected-idle controls.

Teacher console minimizes once when the Rust agent confirms Sharing, after capture
startup succeeds. Starting/error does not minimize it. Reopening during a live
presentation leaves the console visible; tray Show restores a minimized console.
The Teacher agent and media continue running while the console is minimized.

## macOS permission shortcut and roster dots

At console startup a native permission preflight displays a Screen Recording
banner if access is missing (advisory only; Rust capture errors remain authoritative).
Open Settings requests the OS first-use dialog when applicable and opens Privacy
& Security > Screen Recording; the owner enables permission and restarts the app.
The app never changes the permission itself. Deep link uses the macOS13+ Settings
extension, with older System Preferences and a generic security-pane fallback.

All three roster views use a colored dot instead of visible Connected/Ready labels.
Green = online without a snapshot warning. Orange = snapshot unavailable/outdated,
while still connected (presentation-paused snapshots are not stale warnings).
Red = a machine seen online in this console session has disconnected.
Dark gray = not connected yet in this console session, including remembered PCs
at startup. Hover reveals the reason. This does not claim a never-online historical
machine; cached identity history and current-session connection evidence differ.

## Inspect versus Select

Default click opens an independent live Observe window using the same Rust H.264
media pipeline as Teacher broadcast. Checkboxes are hidden until toolbar Select
is enabled; then click toggles selection for sharing. Finish selecting clears
selection and restores Observe-on-click. Observe is read-only by default; Control
is explicit and cooperative by default. Inspect supports Desktop screenshots,
Student messages with an optional reply, and file delivery with progress/cancel/retry.

### Observe display and controls

- **Stretch**: cover the window while preserving aspect ratio; crop
  overflow rather than leave black side bars. Pointer coordinates include the crop.
- **Fill** (default): stretch the source to both window dimensions, with no letterbox/pillarbox.
- **Full screen**: enter the native full-screen window and use Fill automatically.
- New Observe windows initially fill the Teacher monitor's usable work area and
  are centered there. Menu bar/Dock remain available; no fullscreen Space is
  entered automatically. Monitor geometry uses logical Retina coordinates.
- Header and status float over the video, auto-hide after two seconds away from
  controls, and return only within the top/bottom 5 logical pixels. Hidden
  controls do not intercept pointer events. Pin keeps them visible.
- Control uses the immediate local OS pointer, independent of video delivery.
  Student cursor composition is disabled in reverse video while input control is
  active, then restored in Observe or when the input lease expires. Normal
  Teacher broadcasts continue to include the source cursor.
- Teacher tracks actual held mouse buttons. Release/cancel sends releases only
  for buttons previously pressed; a left click must never synthesize a right
  button-up that could open a Windows context menu. Chord transitions reported
  as Flutter move events are handled without swapping primary/secondary buttons.
- Pointer hit testing uses the displayed full-image dimensions and includes viewport
  scroll offset; remote injection uses the captured display's desktop bounds. macOS
  uses points (not Retina pixels); Windows uses physical virtual-desktop coordinates.
- Wheel over video scrolls the remote desktop in Control and the local viewport in
  Observe. Blur, close, disconnect or an expired input lease releases remote input.
- Native texture stats can be zero before the first copied frame; the initial
  texture uses a valid temporary16:9 layout without requiring a mode switch.

Mouse-rate dispatch regression (existing Teacher must be running):

```text
python3 scripts/test-observe-ipc-latency.py /path/to/teacher-endpoint.json \
  --output target/control-responsiveness/ipc-latency.json
```

This sends 300 harmless commands at 125 Hz to a nonexistent Observe identity.
It fails when IPC p95 exceeds 25 ms. It measures local command dispatch only,
not Student injection, network delay or glass-to-glass latency.

## Grid item size

In Grid view the bottom-right slider changes the preferred item width (180–480
logical pixels). Dragging switches Auto off; actual widths fill equal columns
without a remainder strip. Auto picks a balanced column count from viewport width,
known row count and available height, retaining16:9 thumbnails and visible titles.
Large classes scroll vertically. Both mode and manual size are remembered.
Slider/Auto are hidden in List/Compact, whose row sizing remains unchanged.


## Observe/Control: kéo thả file và folder (2026-10-05)

Kéo file hoặc folder từ Explorer/Finder vào cửa sổ Observe/Control của Student.
Folder được đóng gói ZIP trong isolate riêng; file/ZIP dùng đường truyền Rust
hiện có, hiển thị progress và nhận trên Desktop của Student. Không cần cập nhật
Student cho tính năng này. Wheel/trackpad trong Control vẫn gửi đến ứng dụng
Student; muốn cuộn phần frame bị khuất trong Stretch, kéo scrollbar cục bộ.

Teacher Settings → **File transfer** → **Maximum file size (MB)**: mặc định 300,
phạm vi 1–4096, áp dụng cho mỗi file/ZIP thành phẩm, gồm retry. Cấu hình TOML:

```toml
[tools]
max_file_mb = 300
```

Đơn vị cấu hình là MiB (1024² byte). ZIP kiểm tra dung lượng sau khi hoàn tất;
folder có tối đa 100.000 entry và 4 GiB dữ liệu nguồn, không hỗ trợ symlink.
Một lượt drop tối đa 32 item và chuẩn bị tuần tự. Các giới hạn package dev update
độc lập, không thay bởi `max_file_mb`. File lỗi chuẩn bị được xóa; staged file bỏ
dở do tiến trình bị tắt được cơ chế outbox hiện có dọn sau 24 giờ ở lần khởi động.
