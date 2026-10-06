# Classroom v1.1.0

Bản cập nhật Teacher Flutter và Student Rust, đóng gói từ cùng mã nguồn phiên bản 1.1.0.

## Những thay đổi chính

- Teacher chia sẻ gói Student qua HTTP: Settings → Student setup → Share bundled Student app. Student mở URL được hiển thị, tải ZIP, giải nén và mở ClassroomStudent.exe. ZIP tự kèm cấu hình IP Teacher hiện tại.
- Thu logs và cập nhật Student qua kết nối Classroom, có công cụ CLI phục vụ triển khai tự động khi phát triển.
- Observe/Control, hình dạng con trỏ đồng bộ, stats và điều chỉnh thumbnail theo kích thước lưới; badge FPS nhỏ phía Student.
- Kéo thả file/folder trực tiếp lên máy trong Grid/List/Compact, xác nhận đúng người nhận; folder nén ZIP. File nhận và screenshot lưu Desktop.
- Popup nhận file/message không có viền hệ điều hành, có viền riêng; nội dung message bấm để copy. Reply dùng một cửa sổ và đóng toàn bộ popup khi gửi.
- Sửa sự kiện message cũ làm ngắt kênh tools và báo lỗi gửi tin sau khi Teacher khởi động lại.

## Các gói

- macOS Teacher/Student: Apple Silicon. Teacher kèm gói Student Windows x64 và macOS tương ứng.
- Windows Teacher/Student: x64. Teacher gồm Flutter, Rust helper, runtime và gói Student phục vụ HTTP. Giải nén toàn bộ thư mục, không chỉ sao chép riêng EXE của Teacher.
- macOS là bản ký ad-hoc phục vụ phát triển, chưa notarize; có thể cần mở cho phép và cấp quyền Screen Recording/Accessibility/Local Network thủ công.

## Kiểm chứng và giới hạn

126 kiểm thử Rust thành công, 2 bài bỏ qua. Các sửa popup/reply đã được kiểm chứng native Windows Parallels ở mốc trước. Build phát hành và HTTP download được kiểm tra riêng; việc build Windows bằng CI không thay thế kiểm thử hiệu năng native.

Chưa nghiệm thu 1080p cho 40 PC Windows vật lý qua Ethernet. Không dùng bản phát hành này để suy ra đã đạt tiêu chí đó. Không thay đổi tài sản bản v1.
