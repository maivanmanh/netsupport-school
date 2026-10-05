# Observe/Control: cửa sổ, scroll, kéo thả và giữ frame Windows

## Phạm vi / nguyên nhân

- Teacher trước đây `Process.start` mỗi click, không giữ instance khi UI đang
  khởi động. Rust coordinator giờ giữ child theo Student; request nối tiếp được
  gộp và kích hoạt child đang có, kể cả console mở lại. Native activation ở
  console xử lý quyền foreground của Windows sau click thật; không đổi OS policy.
  X trước đây chờ command tới 5 giây mới destroy nên lộ Connecting trống. Bản mới
  hide trước, close theo surface/session, flush bounded và thoát UI process rõ ràng.
- Windows texture callback trả `nullptr` khi producer lock/pool không rảnh, có
  thể mất ảnh trong một raster frame. Bản mới trả last-good frame với lease count,
  không ghi đè storage đang được raster dùng, vẫn ba buffer. Không lọc các frame
  màu đen hợp lệ từ Student. Engine teardown diễn ra trước khi hủy buffers.
- Stretch cover trước đây crop không thể tiếp cận phần ngoài viewport. Nay cuộn
  hai chiều, có gutter riêng cho scrollbar: test phát hiện và sửa việc scrollbar
  drag cũng gửi click tới Student. Wheel/trackpad Control vẫn thao tác remote.
- DropTarget native Windows/macOS, security-scoped bookmark Mac chỉ trong lúc
  chuẩn bị; Dart isolate stream-copy/ZIP, dùng lại Rust file transfer. Desktop giữ
  nguyên ở Student. `tools.max_file_mb=300`, configurable 1..4096; Hub kiểm tra
  trước hashing/queue và retry dùng limit, dev-update limit không bị ảnh hưởng.

## Kiểm chứng

- Flutter: 27 tests, analyze no issues. Có kiểm tra đóng khi remote ACK không
  trả về, callback thoát UI process, không tự steal focus khi console mở lại,
  mapping frame sau scroll/resolution change, scrollbar không click Student,
  wheel/trackpad vẫn remote, ZIP giữ nội dung/empty folder và xóa staging khi quá hạn.
- Rust: 12 tools tests + 6 config tests passed; release Mac và Windows x64 built.
- Windows native texture harness: producer exclusive lock giữ ảnh đã có; raster
  lease bất biến; pool đầy không trả ảnh rỗng; frame mới phục hồi; pause ngừng ảnh.
- .94 Windows Teacher → .206 Windows Student: Observe live, hai request startup
  và request lặp khi live chỉ có một process. X ẩn sau 30 ms, process thoát, mở
  phiên mới được. ZIP 197 byte xác nhận complete và tồn tại trên Desktop Student.
  Cold UI startup khoảng 1875 ms trong lượt đo IPC, không gọi là mở tức thời.
- Observe native log thật đã có `cached_frame_returns=5`, giữ copy_failures=33
  ổn định trong các sample live, source sequence tiếp tục tiến. Không diễn giải
  counters thành chứng minh đã loại bỏ mọi kiểu chớp đen trên Windows vật lý.
- Raw runtime evidence ignored under `target/observe-tools-test/`. Only Teacher
  was built/deployed; Student versions/binaries were not changed through SSH.

## Artifact / giới hạn

- Mac: `target/observe-tools-build/Classroom Console.app`.
- Windows .94: `C:/Users/mvmanh/Classroom-shortcut-validation/Teacher/classroom_console.exe`.
- .206 Student: `C:/Users/mvmanh/Classroom-Student-dev/ClassroomStudent.exe` unchanged.
- Hai Windows là ARM64 Parallels chạy x64 emulation, không phải Windows x64 vật
  lý, không nghiệm thu 40 PC Ethernet. Cần người dùng xác nhận tình huống chớp
  đen thường xuyên trên Windows x64 khi có máy. Native drag/drop OS thực chưa
  được chứng minh chỉ bằng unit ZIP/transport test; cần smoke thủ công Explorer/
  Finder. Không thay đổi permission grants; Mac build ad-hoc có thể cần cấp lại.
- Native activation được build và unit-test; bài click roster bằng tọa độ không
  mở được cửa sổ trong harness, chưa xác minh đường click thật cuối cùng. Script
  lifecycle giữ đường IPC đã kiểm chứng; foreground IPC nền không là acceptance UI.
- Mac release cuối chứa scrollbar gutters. Windows .94 đã build/deploy các sửa
  lifecycle, cache và transfer; thay đổi gutter cuối chưa được build/deploy Windows
  vì chủ máy shutdown .94. Windows Rust helper x64 cuối đã cross-build thành công.
- Chủ máy yêu cầu dừng khi đủ dữ liệu; không chạy thêm mô hình Teacher/Student cùng
  .206. Task instrumentation tạm trên .94 dọn khi máy mở lại, không thay settings.
