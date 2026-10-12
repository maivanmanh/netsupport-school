# Chốt phòng máy Windows — 2026-10-12

Owner yêu cầu chốt nhanh, giảm test và dọn workspace để mang về laptop.

## Bản chốt
- Build đồng bộ Teacher + Student: `room-fixes-input-20261012`, build ID `1791770906-room-fixes-input-20261012`.
- Bundle độc lập ngoài repository: `C:\Users\admin\Desktop\Classroom-room-fixes-20261012\Teacher`.
- Teacher: `classroom_console.exe`; Student: `ClassroomStudent.exe`; Teacher có sẵn `student-packages/student-windows-x64.zip` mới cùng revision.
- Logs/harness được lưu ngoài repository tại `C:\Users\admin\Desktop\Classroom-room-fixes-20261012\validation-logs`.
- 709-17 / 709-18 xác nhận cập nhật thành công bản chốt. Không tiếp tục sử dụng 709-19 / 709-20 đã offline.

## Thay đổi và bằng chứng
- Terminal bỏ bước pairing thủ công theo yêu cầu owner; vẫn kiểm tra Teacher được cấu hình, grant phiên và TLS. Sửa Windows keyboard input qua hardwareKeyboardOnly và xử lý KeyUp để tránh mất/nhân ký tự. Widget regression nhập ký tự + Enter pass; chưa chạy lại thao tác bàn phím thật trên GUI bản chốt. Rust Terminal 17 tests pass; pipe/compat có các lượt kiểm tra thực tế trước đó.
- App Control giải tên Windows resource, lưu/khôi phục checkbox từ rule authoritative, force-close app đang chạy và kiểm tra lại tiến trình. Tên app trên hai Windows đã kiểm tra; force-close trên 709-18 đã xác minh. Bản chốt sửa thêm sharing violation của executable thông báo chặn bằng thư mục theo build ID; Apply chặn Calculator + Notepad++ trả complete trên cả 709-17/18. Đã hoàn nguyên rule gốc chặn Task Manager, scope toàn lớp; cả hai ACK complete. App-control unit 5 tests pass.
- Observe/Control mặc định kích thước native, không phóng lớn vượt pixel nguồn, xét DPI và work area; 4 focused UI tests pass. Chưa xác minh kích thước cửa sổ thực tế trên hai máy sau bản chốt.
- Sửa thứ tự z-order của lock window/monitoring notice để giảm chớp badge; chưa runtime xác minh bản chốt.
- Ctrl+Alt+Del: chưa chặn được. Hai Student Windows Education có Keyboard Filter Disabled, không có service/CIM class. Không bật optional component/reboot vì owner yêu cầu chỉ test phần mềm, không reboot/ngắt thiết bị. Không tuyên bố tính năng này đã hoàn tất.
- Sửa screenshot Windows bounded child capture trước đó đã kiểm tra 4/4 máy.
- Không có bằng chứng nghiệm thu 1080p/40 máy; phòng máy hiện chỉ còn hai máy online, màn hình Student 1280x1024.

## Dọn bàn giao
Giữ .git, toàn bộ file tracked và thay đổi code/test/report mới đã stage; không tạo commit khi chưa cấu hình Git identity. Loại build/cache/file tạm khỏi repository sau khi copy bundle/logs ra ngoài. Giữ nguyên settings/profiles/logs AppData và các ứng dụng không thuộc Classroom. Khi zip để mang về laptop phải bao gồm thư mục ẩn .git để giữ history và index.

Bước tiếp theo trên laptop: review diff đã stage, test GUI Windows thật cho Terminal, badge và native sizing; cấu hình Git identity rồi commit/push. Ctrl+Alt+Del cần quyết định triển khai Keyboard Filter riêng.

Sau cleanup đã mở Teacher từ bundle ngoài repository. Thêm firewall rule cho đường dẫn mới bị Windows từ chối `Access is denied` (process không có quyền admin); chưa xác minh Student reconnect với Teacher ở đường dẫn mới. Bản Student trên 17/18 đã xác nhận cập nhật trước khi đổi đường dẫn Teacher. `git clean -ndx` sau cleanup không còn nội dung; `git diff --cached --check` pass.
