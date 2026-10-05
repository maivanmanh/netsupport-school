# Cập nhật Student qua Teacher trong quá trình phát triển

Chủ dự án cho phép bổ sung ngày 05-10-2026 để giảm thao tác deploy/test. Đây là
updater dành cho LAN dev tin cậy, không phải hệ thống phân phối bản production.
Không cần SSH, HTTP, Internet hoặc thao tác trên giao diện Student khi đã có bản
bootstrap hỗ trợ updater. Media protocol và capture/encode/fan-out không đổi.

## Cài lần đầu và cách hoạt động

Teacher Rust và Student cần bản có capability `dev_update: 1`. Cài Student này
thủ công/SSH **một lần**; bản Student cũ chưa hỗ trợ không thể tự nhận updater.
Teacher chạy bằng Flutter Console hoặc Rust `--teacher-agent --ipc-file PATH`;
CLI phát màn hình cũ không có endpoint quản lý update.

Teacher gửi yêu cầu qua tools TCP hiện có (control port + 2). Dữ liệu ZIP dùng
kết nối download TCP riêng đã có, giữ heartbeat/input/video khỏi I/O file. Tối đa
hai download đồng thời, khoảng 1 MiB/s mỗi download. Student không cần đồng ý
trong popup. Dừng Share và đóng Observe trước khi update. Một máy chỉ có một job
update đang chạy. Máy offline không được tự khởi động hoặc đánh dấu cập nhật.

Gói có manifest chỉ rõ OS, kiến trúc, version dev và SHA-256 executable. ZIP bị
giới hạn 512 MiB, dữ liệu giải nén 1 GiB/10.000 entries; đường dẫn thoát staging
và symlink bị từ chối. Student kiểm tra SHA-256 toàn bộ download, SHA-256 binary,
OS/kiến trúc và compatibility probe của **Student** mới trước khi đóng bản cũ.
Gói Teacher hoặc binary không có probe Student không được thay bản đang chạy.

Updater được sao chép ra staging cạnh app để chờ PID Student cũ thoát hoàn toàn,
đổi tên bản cũ thành backup rồi thay đúng đường dẫn cũ. Windows thay `.exe`,
macOS thay toàn bộ `.app` (bao gồm Info.plist và tài nguyên ký); raw Mac binary
chỉ nhận raw binary. Không thực thi shell command nhận từ Teacher. Các argument,
profile/device ID, Teacher IP và cấu hình được giữ nguyên. Log cạnh raw executable
giữ nguyên; log bên trong Mac bundle được sao chép sang bundle mới. Log ở đường
dẫn `--log-file` khác không bị di chuyển.

Teacher hiển thị `restarting` sau transfer/verify; `complete` chỉ xuất hiện khi
Student mới đăng ký room rồi kết nối tools lại với receipt khớp job/token/version/
checksum. Worker chỉ hoàn tất khi Teacher xác nhận receipt. Nếu bản mới thoát
hoặc không được xác nhận trong 90 giây, worker dừng child của chính nó, khôi phục
bản cũ và chạy lại. Teacher báo rollback khi bản cũ reconnect, hoặc timeout sau
120 giây. Download bị ngắt/checksum/probe sai giữ bản cũ đang hoạt động.

Đây không phải signed-release verification: checksum phát hiện lỗi truyền, không
xác thực nhà phát hành. Cần chọn gói tin cậy. Quyền Screen Recording/Accessibility,
Local Network và các chặn của OS vẫn do chủ máy xử lý; updater không tự cấp quyền,
tắt bảo vệ hoặc xóa quarantine. Mac ad-hoc build có thể phải cấp lại quyền.

## UI và CLI

UI: **Settings → Student updates → Update compatible Students…**. Khi không có
selection, gửi tới các Student online hỗ trợ update và khớp OS/arch của ZIP.
Selection rõ ràng phải gồm các máy đều tương thích. Progress và kết quả hiện
trong Settings và thẻ tiến độ Teacher. Không cho cancel sau khi đã commit restart.
Thu log vẫn ở **Settings → Diagnostics → Collect diagnostics**.

CLI chỉ dùng Python standard library và endpoint IPC riêng tư trên máy Teacher.
Endpoint mặc định của Console là `teacher-endpoint.json` trong Application Support
của ứng dụng; có thể chỉ định khi launch bằng `--ipc-file`. Endpoint chứa token,
không đưa file đó vào Git hoặc gửi ra LAN. Chạy CLI dưới cùng user quản lý Teacher.

```sh
# Tạo gói Windows x64; kiến trúc được đọc từ PE header, không đoán theo tên.
python3 scripts/classroom-dev-update.py package \
  target/rustup-windows/x86_64-pc-windows-gnu/release/ClassroomStudent.exe \
  --version dev-20261005 --output target/updates/student-windows-x64.zip

# Tạo gói Mac từ bundle đã build/ký; không đưa logs vào ZIP.
python3 scripts/classroom-dev-update.py package \
  'target/dev-update-build/Classroom Student.app' \
  --version dev-20261005 --output target/updates/student-macos-arm64.zip

# Liệt kê Student/ID/IP/capability hiện có, không click UI.
python3 scripts/classroom-dev-update.py deploy \
  --endpoint '/path/to/teacher-endpoint.json' --list

# Chọn một hoặc nhiều máy bằng IP hoặc stable Student ID.
python3 scripts/classroom-dev-update.py deploy \
  --endpoint '/path/to/teacher-endpoint.json' \
  --package target/updates/student-windows-x64.zip \
  --student 192.168.50.94

# Hoặc cập nhật tất cả máy online tương thích với kiến trúc binary đang chạy.
python3 scripts/classroom-dev-update.py deploy \
  --endpoint '/path/to/teacher-endpoint.json' \
  --package target/updates/student-macos-arm64.zip --all-compatible
```

CLI xuất JSON progress theo từng target, chờ reconnect và trả exit code 0 chỉ
khi mọi target đã xác nhận. Timeout mặc định 900 giây gồm thời gian chờ download;
đổi bằng `--timeout`. Nếu CLI timeout/đóng terminal, job vẫn chạy trên Teacher,
không tự gửi lại khi chưa biết kết quả. Nếu Teacher mất toàn bộ process/job state
trong lúc update, worker không nhận được xác nhận và có thể rollback; chưa có
khôi phục job update sau khi Teacher crash. Không cập nhật Teacher bằng cơ chế này.

Mac universal/fat Mach-O và bundle có symlink chưa hỗ trợ; gói Student hiện tại
chỉ có một thin executable và không có symlink. Gói Windows x64/ARM64 riêng biệt.
Không tự đổi kiến trúc trên Student, chạy UAC hoặc cài Windows service.

## Artifacts, logs và kiểm thử

Build riêng để không đè các app/gói v1 đang dùng:

```sh
CLASSROOM_MAC_OUTPUT_DIR="$PWD/target/dev-update-build" \
  CLASSROOM_MAC_SIGN_MODE=adhoc bash scripts/build-student-macos.sh
CLASSROOM_MAC_OUTPUT_DIR="$PWD/target/dev-update-build" \
  CLASSROOM_MAC_SIGN_MODE=adhoc bash scripts/build-flutter-macos.sh
```

Updater giữ transaction mới nhất và một bản backup tại
`<installation-parent>/.classroom-update-<job-id>/`. Có `updater.log`, manifest/plan
và receipt; plan/receipt chứa token dev nên không commit/share nguyên thư mục.
Các transaction thành công cũ của **đúng installation này** được dọn sau update
thành công tiếp theo. Các lần lỗi được giữ lại để điều tra. Không xóa profile,
student identity, logs hoặc ứng dụng khác.

Test tự động riêng (tạo native Teacher background host, không capture màn hình):

```sh
python3 scripts/test-dev-update.py --output target/dev-update-tests/raw-mac-run
python3 scripts/test-dev-update.py \
  --student 'target/dev-update-build/Classroom Student.app' \
  --output target/dev-update-tests/bundle-mac-run
```

Test kiểm tra package checksum sai không đóng bản cũ; package hợp lệ khiến PID
cũ thoát, replacement reconnect bằng cùng identity và báo complete. Các process,
ports và profile được cô lập; cleanup chỉ dừng process group do test tạo.
Đây là kiểm thử deploy/lifecycle, không phải nghiệm thu media 1080p/40 PC.

## Kết quả kiểm chứng 05-10-2026

- Mac local: raw binary và cả Student.app đều vượt qua bài checksum sai rồi cập
  nhật/restart/reconnect qua CLI. Không dùng SSH trong các bài Mac này; không coi
  đây là kiểm chứng quyền capture của một Mac remote.
- Windows 11 Parallels ARM64 `.94`, chạy Student x64 qua emulation: hai lượt update
  liên tiếp bằng gói có binary/build ID khác nhau. PID 11700 → 6432 → 5940, cùng
  Student ID; mỗi lượt được Teacher xác nhận complete. Sau bootstrap, không có
  SSH/SCP; diagnostics cũng được lấy qua Teacher. Teacher local lúc test `.50.205`.
- Bootstrap đầu gặp lỗi 740 khi mở helper. Chủ dự án cho phép sửa bootstrap qua
  SSH thêm một lần. Bản sửa dùng helper `ClassroomSwap.exe`, manifest `asInvoker`
  và thử khởi chạy helper trước commit; test Windows mới không gặp lỗi elevation.
  Đây là manifest của ứng dụng, không thay đổi chính sách UAC của hệ điều hành.
- Rust: 114 tests passed, 2 ignored; Flutter analyze và 24 tests passed. Mac Teacher,
  Mac Student và Windows Student x64 release đã build. Media/40-PC acceptance
  không nằm trong lượt kiểm chứng này. Không thay assets release v1.

Evidence cục bộ: `target/dev-update-tests/final-raw-mac/REPORT.json`,
`target/dev-update-tests/final-bundle-mac/REPORT.json` và
`target/dev-update-tests/windows-fixed-two-updates/REPORT.json` cùng ZIP diagnostics.
Các file runtime này bị ignore; báo cáo bền vững nằm trong checkpoint.
