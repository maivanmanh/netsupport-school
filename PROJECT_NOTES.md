# Project notes — quy tắc vận hành của chủ dự án

## Dọn bản deploy cũ (09-10-2026)

Giữ một đường dẫn triển khai đang dùng cho mỗi vai trò, cả Teacher và Student.
Mỗi lần deploy Student: kill đúng process/helper cũ, xóa binary/deployment cũ,
rồi chạy bản mới; không để hai bản chạy song song. Settings/profile/device ID/log
phải được giữ riêng. Updater dùng cùng thư mục cài đặt và chỉ giữ backup tạm
trong lúc chờ reconnect để còn rollback nếu bản mới lỗi.

Sau khi owner tự xóa các thư mục deploy home ngày 09-10, hai Student đã bootstrap
lại qua SSH và dùng đường dẫn cố định:
- Mac `192.168.100.254`: `/Users/lolem/Classroom/Classroom Student.app`.
- Windows `192.168.100.155`: `C:\Users\lolem\Classroom\ClassroomStudent.exe`,
  interactive scheduled task `Classroom Student`.
- Teacher giữ `target/present-recovery-build/Classroom Console.app`.

Tái sử dụng đường dẫn đó cho các lượt triển khai tiếp theo; không tạo thêm thư mục
phiên bản trong home khi không cần thiết. Dừng đúng app/helper trước khi thay.
Giữ settings, profile, device ID và logs ngoài phần binary cần thay. Student
updater chỉ xóa bản backup sau khi bản mới kết nối thành công; nếu chưa kết nối
thì vẫn giữ đường lui để rollback. Không tự xóa thư mục người dùng chưa xác minh.

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

## Gom build macOS và hướng dẫn cấp quyền (08-10-2026)

Chủ dự án yêu cầu gom nhiều thay đổi/code/test trước một lượt build/ký macOS,
tránh build lắt nhắt khiến phải cấp quyền lại nhiều lần. Giữ bundle đang test
trong lúc kiểm tra source. Local Network không có nút thêm/xóa app: phải để
chính app phát sinh yêu cầu LAN, owner tự nhấn Allow/bật quyền. Không hướng dẫn
owner tự add đường dẫn ở mục này. Checklist trong app phải phân biệt trạng thái
quyền đọc được với kết quả thử kết nối; lỗi kết nối không tự chứng minh bị từ chối
quyền. Screen Recording cần restart Teacher sau khi cấp. Không sửa privacy grants
hoặc xóa/reset các mục của hệ thống bằng tự động hóa. Xem
`docs/22_MACOS_PERMISSION_ONBOARDING.md`.


## Cấp quyền thủ công, không dùng Computer Use (09-10-2026)

Chủ dự án yêu cầu: khi gặp quyền macOS còn thiếu trong lượt thử nghiệm, tạm dừng
và liệt kê đầy đủ đường dẫn app/helper cùng loại quyền để chủ máy cấp thủ công.
Không dùng Computer Use để thao tác cấp quyền. Không reset/xóa/đổi privacy grants.
Checklist chỉ tự hiện lần đầu khi cả hai quyền Recording chưa được cấp; lưu dấu
đã xem, không hiện lại mỗi lần mở Teacher. Nút toolbar vẫn mở theo yêu cầu.

## Build Mac luôn ký bằng Apple Development cert (14-10-2026)

Chủ dự án yêu cầu từ nay mọi lượt build Teacher/Student macOS đều ký bằng Apple
Development cert để quyền macOS (Screen Recording, Accessibility, Local Network)
được giữ giữa các lần build, không cấp lại nhiều lần. Không dùng ad-hoc mặc định.

```
CLASSROOM_MAC_SIGN_MODE=apple \
CLASSROOM_MAC_SIGN_IDENTITY=A9F06E151E345F70B12858DF2BC32945BFF60945 \
scripts/build-flutter-macos.sh
```

- Giữ nguyên bundle ID, đường dẫn output (`target/Classroom Console.app`) và cert.
  Thoát đúng Teacher/helper đang chạy trước khi build đè cùng đường dẫn.
- Cert hết hạn 03-10-2027; các cert cũ cùng tên đã bị revoke. Khi gia hạn, giữ
  cùng CN để designated requirement không đổi (script sẽ dừng nếu requirement đổi).
- `sign-macos-app.py` chỉ bỏ bước Gatekeeper `spctl` cho cert không phải Developer
  ID (Apple Development không bao giờ qua Gatekeeper); các kiểm tra khác giữ nguyên.
- Lần đầu dùng đường dẫn/danh tính mới vẫn cần cấp quyền thủ công một lần. Mac
  Student chưa deploy bản ký Apple sẽ cần cấp một lần khi deploy.

## Ý tưởng cài đặt giảng viên chưa làm (so sánh NetSupport School / Veyon, 14-10-2026)

Chủ dự án yêu cầu lưu lại để sau này hỏi thì gợi ý implement. Nguồn: tài liệu chính
thức NetSupport School (Tutor Configurator: Start Options, Network and Wireless,
Performance, Student Selection, Administration; Tutor UI settings; Monitor mode) và
Veyon (Configurator: General, Service, Master, Access control, Authentication keys,
Demo server, File transfer, WebAPI). Danh sách NetSupport không đầy đủ từng tuỳ chọn.

Đã làm: **#1 chế độ Wi-Fi / băng thông thấp / máy yếu** (xem `docs/29_LOW_BANDWIDTH_MODE.md`).

Còn lại, theo thứ tự đề xuất:

2. **Trang "Giao diện giám sát"** (chỉ Teacher): nhãn thumbnail (tên máy / IP / cả hai),
   sắp xếp (tên, IP theo số — `.9` trước `.10` —, trạng thái), hành động nhấp đôi
   (Observe / Control / không làm gì; kiểm tra hành vi hiện tại trước), tự vừa cỡ, ẩn
   máy offline mặc định; gom các lựa chọn đang rải ở thanh công cụ.
3. **Xác nhận thao tác nguy hiểm** (tắt máy / khởi động lại / đăng xuất; hiện luôn có
   hộp xác nhận trong `power_controls.dart`): công tắc (mặc định bật; khi chọn cả lớp
   vẫn bắt xác nhận), nút "Reset các nhắc nhở không hiện lại", tự nối lại sinh viên.
4. **Bí danh và nguồn tên máy**: đặt tên theo máy (nhớ theo ID máy, không theo IP),
   hiện ở lưới, dialog ghi màn hình, toast phản hồi, báo lỗi; nguồn tên mặc định
   hostname / tên đăng nhập / bí danh. Lưu trong kho dữ liệu Teacher.
5. **Thư mục lưu và chất lượng ghi hình**: thư mục gốc cho ảnh chụp, video ghi, tệp
   thu về (hiện luôn Desktop); độ phân giải ghi (hiện 720p), fps, độ dài đoạn MP4 (10
   phút); cách gom tệp thu (theo máy / theo ngày); cảnh báo ổ đĩa đầy. Chất lượng ghi
   cần cập nhật cả Student.
6. **Thời hạn của quy tắc chặn** (Internet / ứng dụng / VS Code): "đến hết ngày / 30 /
   60 / 90 phút / 2 giờ / đến khi gỡ". Student phải tự gỡ khi hết hạn (như khi sang
   ngày mới), nên sửa cả hai phía; tốn công nhất (~1–2 ngày).

Làm sau (đầu tư lớn): **hồ sơ buổi học** (ví dụ chế độ thi: chặn Internet + ứng dụng
cùng lúc), **mật khẩu bảo vệ cài đặt** và **mã phòng / khoá xác thực Teacher**
(đã nêu ở docs/11 "Known gap"), **Wake-on-LAN** (nhớ MAC từ danh sách máy đã thấy).

Tuỳ chọn / ưu tiên thấp: giới hạn dung lượng và xoay vòng log, hiện cổng điều khiển /
media / phản hồi, ngôn ngữ vi/en và giao diện sáng/tối, WebAPI REST, LDAP/AD, ngoại lệ
tường lửa tự động (nên để installer), nguồn tên đăng nhập / ảnh sinh viên.
