# Kiểm thử Teacher Windows và hồ sơ phát hành v1

Ngày ghi nhận: 04-10-2026. Báo cáo chi tiết về kiểm thử vòng đời, cài đặt, con trỏ, truyền tệp và thu thập chẩn đoán nằm trong [checkpoint kiểm thử](../checkpoints/diagnostics/2026-10-04-lifecycle-settings-diagnostics.md).

## Phạm vi đã chạy

Máy Windows 11 ARM tại `192.168.50.94` đã chạy ứng dụng Teacher Flutter cùng tiến trình điều phối Rust trên Wi-Fi LAN. Máy nhận kết nối từ Student macOS `192.168.50.206` và Student Windows 11 ARM `192.168.50.205`. Student Windows x64 cũng được build và kiểm tra riêng. Student ARM chạy trong Parallels trên máy Mac và chia sẻ Desktop, CPU cùng card mạng với macOS; vì vậy đây là hai hệ điều hành/endpoint, không phải hai máy vật lý độc lập.

Trên Windows Teacher, đã mở Settings dạng sidebar, lưu cấu hình và khởi động lại agent Rust; mở Teacher bằng executable không tham số để xác nhận helper và cấu hình được phục hồi; thoát qua menu khay hệ thống khi Observe đang chạy và xác nhận các tiến trình Flutter, điều phối và Observe đều kết thúc. Student Windows được kiểm tra tương tự về thoát và mở lại. Khi phát màn hình, con trỏ cục bộ trên Student Windows bị ẩn và hiện lại sau Stop; chủ dự án xác nhận hành vi ẩn con trỏ trên Student macOS.

Teacher Windows mở được hộp thoại chọn tệp gốc và gửi tệp đến lớp. Tệp 90 byte và một tệp 14.136 byte đến Student, khớp SHA-256; gửi lại tệp trùng lặp được bỏ qua. Thu thập ZIP chẩn đoán thủ công và tự động sau Stop đã lấy được log của hai Student đang kết nối, đánh dấu máy mất kết nối, và hoàn tất sau khi sửa race giữa Stop bất đồng bộ với yêu cầu upload. Tốc độ upload bị giới hạn 512 KiB/s; dữ liệu chỉ gồm log runtime và metadata đã lọc, không lấy ảnh màn hình, nội dung chat/tệp hay token IPC. Flutter analyze và 21 bài kiểm tra Flutter, Rust 105 bài, config 5 bài, transport 20 bài, fmt và Clippy đều đã qua theo checkpoint.

## Giới hạn còn tồn tại

Thoát bằng menu trạng thái macOS chưa được xác minh bằng thao tác GUI; Accessibility chưa cho phép tự động hóa menu. Bài phát 63 giây từ Teacher Windows tới ARM Student qua Wi-Fi vẫn ghi nhận mất 19 gói, 12 lỗi depacketizer, 18 lần yêu cầu keyframe và tốc độ render khoảng 12 FPS; lượt trước có phục hồi chậm nhất 2,775 giây. Chưa có kiểm thử mạng dây với các máy Windows x64 vật lý, chưa chạy thang đo 1/5/10/20/40 máy và chưa đạt tiêu chí chấp nhận hiệu năng Phase 1. Không diễn giải các kiểm tra chức năng thành cam kết hiệu năng.

## Gói phát hành

Các ZIP trong `target/release-v1` được đóng từ bản build đã kiểm tra: Teacher Windows x64 (thư mục portable đầy đủ gồm Flutter và helper Rust), Student Windows x64, Student Windows ARM64, Teacher macOS và Student macOS. Log cục bộ trong thư mục build không được đưa vào gói. Các ZIP Windows kèm hướng dẫn tiếng Việt; hướng dẫn macOS nằm trong `RELEASE-NOTES-VI.md`. `SHA256SUMS.txt` ghi checksum để xác minh tải xuống. Bản macOS là ứng dụng ký ad-hoc, nên Gatekeeper/privacy có thể yêu cầu xác nhận quyền lần đầu. Student macOS cần quyền Screen Recording và Accessibility khi dùng chức năng tương ứng.

Tên tài sản phát hành:

- `Classroom-Teacher-Windows-x64-v1.zip`
- `Classroom-Student-Windows-x64-v1.zip`
- `Classroom-Student-Windows-arm64-v1.zip`
- `Classroom-Teacher-macOS-v1.zip`
- `Classroom-Student-macOS-v1.zip`

Gói này là bản phát hành chức năng v1; không thay thế điều kiện nghiệm thu hiệu năng tại `docs/06_PERFORMANCE_ACCEPTANCE.md`.
