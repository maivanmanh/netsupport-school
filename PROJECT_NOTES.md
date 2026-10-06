# Project notes — quy tắc vận hành của chủ dự án

## Build luôn theo cặp Teacher + Student (06-10-2026)

Mỗi khi chủ dự án yêu cầu build, phải build lại **Teacher và Student từ cùng source
revision/working tree**, kể cả thay đổi chỉ ở Teacher. Teacher luôn bundle Student
vừa build, đúng platform/architecture, sẵn sàng cho **Share Student app**. Không
được dùng ZIP Student cũ chỉ vì file đang tồn tại hoặc bỏ qua package khi thiếu.
Thiếu toolchain/build lỗi thì báo rõ và không gọi Teacher package là hoàn tất.

- Mac development: `scripts/build-flutter-macos.sh` build Mac Teacher + Mac Student
  + Windows x64 Student; tạo hai dev-update ZIP cùng label, bundle vào Teacher.
- Windows Teacher: `scripts/build-flutter-windows.ps1` build cả Rust x64 roles từ
  source bằng MSVC, tạo Student ZIP, build Flutter và bundle package. Cần Rust
  target/toolchain và Python; không âm thầm dùng RustAgent/binary cũ truyền vào.
- Student Mac là endpoint test, không phải deployment production. Browser bootstrap
  hiện phục vụ Windows Student; ZIP Mac có sẵn cho updater/dev testing.
- Build đồng thời không có nghĩa phải deploy/restart mọi endpoint. Chỉ ship Student
  nếu cần dùng bản mới. Đã bootstrap thì update qua Classroom, không SCP/SSH binary.
- Giữ settings/profile/device ID/logs. Không overwrite bundle/exe đang chạy; dừng
  đúng owned processes, xác minh đường dẫn rồi thay hoặc dùng package mới cô lập.
- Không sửa privacy grants. Mac signing ad-hoc có thể cần owner cấp quyền lại.
- Log/report quan trọng cập nhật `checkpoints/PROGRESS.md`, ghi rõ build-verified
  và runtime-verified; không kéo dài test không cần thiết.
