# Windows Teacher — phím Win trong Control (05-10-2026)

## Nguyên nhân và thay đổi

Flutter `Focus.onKeyEvent` không chặn được Windows shell xử lý phím Win trước
khi ứng dụng nhận sự kiện. Windows Teacher thiếu native interception trong
Control; `KeyEventResult.handled` không có nghĩa hệ điều hành đã bỏ qua Win+R.

Thêm `ControlKeyboard` với `WH_KEYBOARD_LL` trên UI thread. Chỉ intercept phím
Win và các phím được hỗ trợ trong chord Win khi cửa sổ Control chính xác đang
foreground. Callback chỉ post message tới window, không chờ TCP/Dart/file I/O;
MethodChannel chuyển tiếp vào input channel Rust đã có. Win events không đồng
thời đi qua Flutter, tránh duplicate/synthetic modifier repair. Các phím thường
vẫn theo đường Flutter cũ. Không thay Student hoặc protocol.

Hook được tắt khi chuyển Observe, có yêu cầu đổi mode, mất focus, mất phiên live,
đóng cửa sổ hoặc teardown native. Native WM_ACTIVATE tắt ngay khi blur trước cả
callback Dart. Held input vẫn được release qua cơ chế hiện có. Secure desktop và
Ctrl+Alt+Delete không nằm trong thay đổi này. Không thay UAC hoặc policy Windows.

Tài liệu API: [Microsoft LowLevelKeyboardProc](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc).

## Kiểm chứng

- Flutter analyze: no issues; 24 tests passed.
- Native C++ hook và harness build bằng MinGW x64 với `-Wall -Wextra -Werror`.
- Harness dùng đúng hook production và Windows shell thật trên Windows 11 ARM64
  Parallels `.94` (binary x64 emulated). PASS: Win/R down/up forward đúng một lần;
  Control không mở Run local; khi hook disabled và khi cửa sổ không foreground,
  Win+R vẫn mở Run local, không forward. Test không sửa registry/firewall và chỉ
  đóng Run dialog do chính test tạo; không có dialog Run trước test.
- `scripts/test-windows-control-keyboard.cpp` là regression harness;
  `scripts/test-windows-shortcut-forwarding.cpp` và
  `scripts/test-windows-win-shortcut.ps1` kiểm tra đường Flutter → Rust → Student.
- Evidence local ignored: `target/shortcut-test/keyboard-test.txt`.
- Windows mới ban đầu báo sai địa chỉ APIPA; owner sửa thành `.206`. SSH lần đầu
  cần `StrictHostKeyChecking=accept-new`, không coi thời gian chờ xác nhận host key
  là lỗi mật khẩu. Student bootstrap tại
  `C:/Users/mvmanh/Classroom-Student-dev/ClassroomStudent.exe`, IP Teacher `.94`.
  Các bản Student tương lai phải deploy qua Classroom, không SCP lại.

## Kết quả Windows → Windows

- Teacher `.94`, Student `.206` đều Windows 11 ARM64 Parallels, chạy binaries x64.
  Windows Teacher build bằng Flutter/MSVC trên `.94`; Rust helper cross-build GNU
  x64. Đây là chức năng remote input, không phải hiệu năng x64 vật lý/wired LAN.
- Ban đầu Teacher dùng cổng 48700 còn lưu trong settings, Student dùng 47800.
  Đã restart đúng bundle test với CLI `--control-port 47800`, giữ nguyên settings
  đã lưu; Student tự reconnect, Observe và Control chạy được.
- Test actual Flutter Control: Teacher injection Win+R exit0, không có Run local;
  probe trên desktop interactive của Student đếm đúng 1 Run dialog. File evidence:
  `target/shortcut-test/forwarding-lifecycle.txt`, `forwarding-result.txt`,
  `student-run-dialog-count.txt` (runtime ignored). Lỗi "Student not online" của
  lượt sai cổng là evidence trước sửa setup, không phải kết quả lượt sau.
- Theo yêu cầu bổ sung: footer/status bar và vùng hover 5px ở đáy **không tồn tại**
  trong Control/exclusive; Observe vẫn auto hide/show. Header vẫn như cũ. Các lỗi
  mode có thể hiện trong header; overlay tiến độ file vẫn hoạt động.
- Updated Flutter regression: 24 tests passed, bao gồm Control không xuất hiện
  footer cả khi hover đáy, và Observe khôi phục bottom trigger. Mac Teacher mới:
  `target/control-shortcuts-build/Classroom Console.app` (đã mở). Windows Teacher:
  `C:/Users/mvmanh/Classroom-shortcut-validation/Teacher/classroom_console.exe`.
  Students không cần bản mới vì thay đổi hoàn toàn ở Teacher.

- Windows footer release rebuild passed; replaced only the verified isolated
  Teacher bundle after stopping GUI/Observe/helper and waiting for locks to clear.
  Local portable package (x64 CRT included):
  `target/shortcut-test/Classroom-Console-Windows-shortcuts.zip`.
  Windows script-file execution policy was left unchanged; packaging helpers were
  supplied as trusted inline commands. No Student runtime version was replaced.
