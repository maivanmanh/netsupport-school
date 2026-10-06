# Cursor Control và badge FPS Student — 06-10-2026

## Hình dạng cursor

Observe vẫn có cursor Student trong video. Control giấu cursor đó trong video,
Teacher dùng vị trí chuột cục bộ để tránh phải đợi video. Khi Control đang active,
Student lấy hình cursor/hotspot từ PointerSource dùng chung với media backend,
kiểm tra hình dạng khoảng20Hz và chỉ gửi bitmap khi serial thay đổi. Không có pool
encoder hoặc capture khác cho cursor; nguồn chỉ tồn tại trong phiên Observe.

Một slot cursor mới nhất và Notify trên TCP control của phiên Observe tách biệt
slot xác nhận mode. Không chờ heartbeat1s, không giữ lịch sử bitmap. Cursor chuẩn
128px được giới hạn96px trước base64, giữ hotspot và tỷ lệ; gói vẫn dưới64KiB.
Khôi phục Control gửi lại shape. Receiver stdout→Coordinator→private IPC→Flutter
truyền metadata; revision cursor làm Teacher publish ngay thay vì đợi500ms.

Flutter giải mã BGRA một lần/serial. Vị trí cục bộ cập nhật ValueNotifier trong
RepaintBoundary/IgnorePointer; không rebuild cửa sổ Observe mỗi lần rê chuột.
Khi có bitmap hợp lệ, ẩn OS arrow trong viewport và vẽ cursor Student/hotspot tại
vị trí chuột Teacher. Chưa có bitmap thì giữ arrow; Observe/toolbar giữ cursor OS.
Shape có thể chậm hơn vị trí bởi sampling/capture/network nhưng không làm vị trí
chuột chờ frame video. Mac dùng sampler NSCursor hiện có, Windows dùng DXGI shape.

## Badge FPS

Khi Student nhận chiếu lớp, badge native hình tròn56×56, alpha200/255, cách góc trên trái12px,
không viền/title/control buttons, topmost, click-through và không lấy focus.
Đếm các frame video thực sự renderer.render thành công; pointer-only redraw không
được tính. Cập nhật nhãn mỗi1s, bơm sự kiện native tối đa10Hz. Không copy frame,
không gửi thêm telemetry qua mạng, không encode lại, không thêm process.

Badge thuộc scope UI phiên chiếu, đóng cùng preview/Stop/disconnect. Không tạo khi
headless hoặc receiver của Observe/Control. Mặc định bật; tắt trong config Student:

```toml
[diagnostics]
student_fps_badge = false
```

Không tuyên bố chi phí bằng0: thêm một surface nhỏ do desktop compositor quản lý.
Số FPS là render throughput theo cửa sổ1s, không phải capture FPS, refresh-rate hay
độ trễ quang học. Nền hình tròn bán trong suốt; phần ngoài hình tròn không che nội dung.

## Verification

Rust Classroom121pass/2ignored,control-transport10pass và doctests,Flutter30pass,
analyze clean. Tests kiểm tra bitmap/hotspot/64KiB bound, cursor không ghi đè mode
ack, local pointer entry hiển RawImage và ẩn arrow. Paired Teacher/Mac Student/
Windows x64 Student build; Windows .94 updated through Classroom and confirms
paired-20261006-093510. Live .94 nhận bitmap32×32 với hai serial/hash/hotspot khác
nhau. Đây là kiểm tra luồng shape, chưa phải đo độ trễ thao tác hoặc mọi CSS cursor.

Kiểm tra tiếp theo ngày06-10 sau owner cấp quyền: class broadcast chạy được.
Badge Windows đã xác minh56×56, margin trái/trên12px, alpha200, topmost,
click-through/noactivate, native elliptical region (góc ngoài region, tâm bên trong).
40 mẫu native có17 mẫu badge hiện và mẫu cuối không còn badge sau Stop.
Mac Student compiled/bundled, không remote-tested. Không phải nghiệm thu40PC.
