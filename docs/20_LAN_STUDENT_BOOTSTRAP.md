# Cài Student lần đầu qua LAN của Teacher

Owner authorization: 06-10-2026, phục vụ debug/test phòng máy. Không cần Internet,
SSH, Flutter trên Student hoặc Windows Service. Media và private IPC không dùng
HTTP; chỉ package Student được phục vụ qua endpoint tải riêng do Teacher bật.

## Sử dụng

1. Teacher: **Settings → Student setup → Share bundled Student app**.
2. Mặc định cổng **80**: Student mở `http://<IP-Teacher>/` trong Chrome.
   GET tại `/` trả thẳng ZIP attachment, không cần thêm path. Nếu cổng 80 không
   mở được/đang dùng, đổi sang 8080 và dùng URL Teacher hiển thị, có `:8080`.
   Không tự mở firewall hoặc thay chính sách OS.
3. Student chọn **Extract All** vào thư mục local có quyền ghi, rồi double-click
   `ClassroomStudent.exe`. Giữ `classroom-bootstrap.toml` bên cạnh executable.
4. App đọc IP/control port Teacher từ ZIP, chạy ngầm và đăng ký roster. Không cần
   nhập Teacher IP. Không chạy executable ngay bên trong ZIP hoặc từ UNC share.
5. **Stop sharing app** đóng endpoint và ngắt download đang chạy; Teacher agent
   thoát cũng đóng endpoint. Đóng riêng Console vẫn để agent/chia sẻ hoạt động.

ZIP chứa executable, cấu hình phòng và README, không chứa identity, logs, token
IPC hay cấu hình cá nhân của Teacher. Profile/device ID Student được tạo tại dữ
liệu user theo khóa phòng IP/port; không ghi trong bundle và không nhân bản ID
qua ZIP. CLI `--teacher-ip`, `--control-port`, `--config`, `--launch-profile`
vẫn override; settings Student đã lưu trong profile phòng được giữ khi mở lại.
Không hỗ trợ đổi kiến trúc tự động; package hiện mặc định Windows x64, có thể chạy
qua x64 emulation trên Windows ARM64. Chưa có nghiệm thu lab 40 PC vật lý.

## Popup và tray

**Student connection popup** trên Teacher có hiệu lực ngay qua tools TCP hiện có
cho Student có capability `connection_popup: 1`. Student chỉ hiện Connected,
tự ẩn sau ba giây. Connecting/reconnecting và mất kết nối luôn im lặng,
kể cả khi bật tùy chọn popup; Teacher vẫn ghi nhận offline và cảnh báo khi bật
activity monitoring. Popup nonmodal, không lấy focus; tắt popup
không đổi tray hay kết nối. Popup không che phiên Teacher presentation. Người dùng
có thể đóng riêng popup; trạng thái thay đổi mới làm nó hiện lại.

Preference được Student ghi theo profile cho thông báo Connected lần sau;
lúc nối tools, Teacher gửi lại giá trị hiện tại. Teacher
**Save & reconnect** lưu công tắc qua restart. ZIP ghi giá trị tại lúc bắt đầu Share;
nếu cần thông báo Connected, bật popup trước khi tạo ZIP/chia sẻ.
Student cũ không có capability không nhận action mới; cập nhật qua Classroom trước.

Windows Student không có menu **Exit Classroom** hoặc **Restart Classroom**; tray
vẫn hiển thị trạng thái và Connection settings. Không khóa Task Manager, đăng xuất
hoặc quyền quản trị; đây chưa phải bảo vệ tiến trình bằng privileged service.
Mac Student dev giữ Exit/Restart để test. Updater vẫn có thể đóng/thay/restart app.

## Packaging / tự động hóa

Mac Teacher build luôn build lại Teacher/Student và bundle package tương ứng:

```sh
python3 scripts/classroom-dev-update.py package \
  target/rustup-windows/x86_64-pc-windows-gnu/release/ClassroomStudent.exe \
  --version dev-20261006-bootstrap --output target/updates/student-windows-x64.zip
CLASSROOM_MAC_OUTPUT_DIR="$PWD/target/student-bootstrap-build" \
  CLASSROOM_MAC_SIGN_MODE=adhoc bash scripts/build-flutter-macos.sh
```

Teacher không dùng package cũ/optional. Build script Mac tạo cả Student Mac dev
và Windows x64, cùng label; Windows Teacher script build Rust x64 roles rồi tạo
package trước Flutter. Xem [project notes](../PROJECT_NOTES.md).
Nếu chưa bundle, UI **Choose Windows Student ZIP…** chọn dev-update ZIP có manifest
Windows, executable `ClassroomStudent.exe` ở root, checksum đúng. Cần binary mới
có đọc bootstrap; không dùng Student/Teacher legacy cho lần cài này.

HTTP share lấy đúng IP Teacher đã bind và control port hiện tại, không hard-code
phòng. Chuẩn bị ZIP/checksum ở blocking worker; 4 downloads, 48 request handlers
có deadline, chunk queue 2×64 KiB, pacing khoảng 8 MiB/s mỗi download. Khi bận,
browser nhận trang 503 tự thử lại sau 3 giây. Header ≤8 KiB/3 s; chỉ GET/HEAD `/`.
Không directory listing, range/resume hay file tùy ý. File tạm thuộc phiên và được
xóa khi stop/cancel/fail; app crash có thể để lại file cho cleanup thủ công.

Local authenticated IPC có các action, không cần click UI:

- `student_download_start`, input `{ "port": 80 }`, dùng gói bundled; hoặc thêm
  `"path": "/path/to/student-windows-x64.zip"`.
- `student_download_stop`.
- `connection_popup`, input `{ "enabled": true }`.

Snapshot `student_download` cho biết off/preparing/sharing/error, URL, bytes,
architecture và số download hoàn tất. `tools.connection_popup` là preference live.
Logs có sharing-ready/download/disabled và Student popup preference/show/hide.
Thu logs và updater sau lần bootstrap tiếp tục dùng chức năng hiện có theo
[19_DEV_STUDENT_UPDATES.md](19_DEV_STUDENT_UPDATES.md), không tải qua HTTP nữa.

HTTP dev LAN không có xác thực nhà phát hành/TLS; checksum kiểm tra package nội
bộ, không thay thế release signing. Không công bố endpoint ra Internet. Các cảnh
báo/quyền OS vẫn do người dùng xử lý; code không bypass chính sách hệ điều hành.
