# LAN bootstrap ZIP và connection popup — 06-10-2026

## Triển khai

- Teacher Rust tùy chọn bind HTTP trên đúng IP đã chọn, GET/HEAD `/` tải ZIP;
  UI mới **Settings → Student setup** có bundled/custom ZIP, port, Copy URL,
  Stop sharing và công tắc popup. Không đổi media/control/IPC sang HTTP.
- ZIP chuẩn bị off-thread, streamed/hash checked từ dev-update Windows Student
  package; chứa `.exe`, room config IP/port và README; không sao chép device ID.
  Bounded requests/downloads/chunks, pacing, timeout, cancellation/cleanup.
- Windows Student đọc adjacent bootstrap, tạo profile theo phòng trong user data,
  CLI/config/saved settings vẫn override. Sau bootstrap dùng updater/logs cũ.
- Capability `connection_popup:1` qua tools: Teacher gửi preference trên poll đầu
  và khi thay đổi. Student lưu vào profile, có Connecting/Connected nonmodal,
  auto-hide 3s, tắt trong Teacher presentation. Windows tray bỏ Exit/Restart;
  không ngăn Task Manager/administrator, Mac dev giữ hai menu đó.
- Mac/Windows Teacher build script bundle package khi có. Source/docs là portable;
  Flutter Windows Teacher UI mới chưa build ở milestone này (scope Mac Teacher).

## Kiểm chứng ngắn

- Rust toàn suite: 117 passed, 2 ignored; sau sửa cancellation/checksum, hai HTTP/
  ZIP tests pass lại. HEAD không body, `/teacher-endpoint.json` trả404, ZIP room
  config đúng và checksum sai không để staging rác. Clippy chạy được; các warning
  mới về literal/stack array/clone đã sửa; pre-existing warnings không là gate mới.
- Flutter analyze sạch, 27 tests passed. Sidebar Student setup cuộn được trong
  desktop 740×520; test ban đầu tap item đang bị che, sửa scroll tới cả ListTile.
- Mac Teacher và Windows Student x64 release built. Mac Teacher bundle gồm ZIP.
- IP local kiểm tra trực tiếp là **192.168.50.206**, không dùng .205 cũ.
- `.94` Student cũ khởi chạy với IP mới bằng SSH, **không copy binary**. Windows
  Teacher cũ của đúng folder test được dừng để `.94` chỉ đóng vai trò Student.
- Update Student qua Classroom tools từ binary cũ: receipt complete, bản mới
  reconnect đúng stable ID, version `dev-20261006-bootstrap`; không SSH/SCP binary.
- Mac sandbox/OS không cho bind80 (`Permission denied`, os13). Cổng8080 được bind,
  endpoint `http://192.168.50.206:8080/`. Không thay firewall/OS policies.
- `.94` tải 4.145.214-byte ZIP qua HTTP root bằng PowerShell IWR (không SCP), hash
  executable `b5b6a4d56d2bc4501f0f19e6c0f403b3cea1188de1b2644be9a82e6ae1e12dcc`.
  Giải nén, dừng đúng Student manual cũ, khởi chạy bản tải về **không args** trong
  desktop user bằng task test. Đăng ký Teacher206:47800, thumbnail nhận bình thường.
- Native popup visible ở sample315ms tới3199ms, gone ở3514ms. Student log:
  Connecting shown1791248126104; room-ready1791248126229; popup-hide1791248129259,
  tức khoảng3,03s sau kết nối. Đây là thời gian lifecycle, không latency video.
- IPC công tắc off/on cập nhật Teacher state; Student log xác nhận received=true.
  Hai giá trị đổi nhanh có thể coalesce giữa polls; không coi lượt đó là native
  visibility acceptance riêng cho off. Code tắt hủy popup trong UI pump hiện có.
- Collect diagnostics qua Classroom: 1/1 Student collected, 0 missing. ZIP có
  Teacher/Student logs và metadata sanitized, giữ evidence trong target/bootstrap-tests.
- Task đo popup đã unregister; Student tải HTTP và Mac Teacher để chạy cho owner.
  Không chạy thêm media/40-client tests; owner yêu cầu giữ test ngắn.

## Artifacts / giới hạn

- Mac: `target/student-bootstrap-build/Classroom Console.app`.
- Windows Student hiện chạy: `C:/Users/mvmanh/Classroom-browser-bootstrap-20261006/ClassroomStudent.exe`.
- Package dùng updater/bundle: `target/updates/student-windows-x64.zip`.
- Evidence ignored: `target/bootstrap-deploy-result.txt`,
  `target/bootstrap-tests/REPORT.json`, `Classroom-diagnostics.zip`.
- Chrome GUI download chưa tự click; HTTP attachment transport và no-args desktop
  launch đã xác minh. Windows là ARM64 Parallels chạy x64 emulation, không physical
  Windows x64 hay nghiệm thu 40PC. Cổng80 phụ thuộc quyền/availability; dùng8080
  trên Mac hiện tại. Service/process protection, autostart, TLS/signing production,
  range/resume không thuộc milestone này. Preserve v1 assets/older diagnostic logs.
