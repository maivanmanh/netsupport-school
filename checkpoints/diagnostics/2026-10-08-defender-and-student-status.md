# Defender, mất kết nối lúc chiếu và thông báo Student — 08-10-2026

## Phạm vi và bằng chứng

Owner báo nhiều Windows 10 phòng thật tự xóa Student khi giải nén/cài; một máy
kết nối lúc idle nhưng mất kết nối lúc Teacher chiếu và trở lại khi dừng chiếu.
Không có máy/binary bị phát hiện, tên threat hoặc Defender event của phòng thật.
Chưa xác định hai hiện tượng có cùng nguyên nhân. Không kết luận false positive
chỉ vì đây là binary tự build.

Windows hiện tại: SSH `mvmanh@192.168.50.106`; host key khớp các IP cũ .215 và
172.20.10.3. Truy vấn chỉ đọc, không scan/chạy app/thay bảo vệ. Defender service,
antivirus và realtime đều true; signature version 1.459.576.0. Truy vấn threat
detection và event 1116/1117 trong 14 ngày không trả bản ghi. Đây không phải máy
gặp lỗi, không chứng minh binary phòng thật an toàn hoặc hết bị phát hiện.

Binary test còn trên máy:
`C:\Users\mvmanh\Classroom-Recovery-1791382275\Classroom.exe`.
Authenticode: **NotSigned**.
SHA256: `B758B4E3D3DF7491694248163922FC789740354EB201615157B74C7E47BE34C4`.
Raw evidence: `target/defender-diagnostics-20261008/windows-defender-status.txt`.
Lần SSH đầu đợi xác nhận host key của IP mới rồi timeout; không phải lỗi app.

## Các yếu tố cần kiểm tra về Defender

- Scripts build Windows hiện không có bước Authenticode signing. Ký macOS không
  ký Windows EXE. Chưa có certificate publisher trong pipeline này.
- Guard tạo service tự khởi động, khởi chạy lại Student và scheduled task;
  `guard/windows.rs` chạy PowerShell với ExecutionPolicy Bypass để đặt/xóa NRPT
  DNS rule. App có remote control/monitoring và tự cập nhật executable.
- Những hành vi quản trị hợp lệ này cần giải thích trong submission. Chúng là
  các giả thuyết nhận nhầm, **không phải trigger Defender đã được chứng minh**.
- Defender quarantine khác cảnh báo SmartScreen publisher chưa được biết đến.
  Ký số giúp xác minh publisher; không bảo đảm Defender không phát hiện.

Hướng xử lý:
1. Lấy tên threat, file/resources, event 1116/1117, thời gian, hash và đúng ZIP/EXE
   bị phát hiện. Không đổi tên/rebuild liên tục làm mất identity của mẫu.
2. Gửi đúng mẫu qua Microsoft Security Intelligence dưới vai trò Software
   developer / incorrectly detected, giữ submission ID và phản hồi. Chưa gửi
   mẫu ra dịch vụ bên ngoài trong lượt này.
3. Khi có certificate hợp lệ, ký Student và Guard bằng publisher nhất quán,
   timestamp, verify chữ ký **trước** tạo ZIP và manifest hash. Ký lại sau package
   sẽ làm package/hash sai. Chưa có certificate nên chưa thực hiện signing.
4. Scan đúng release trên Windows 10 x64 với intelligence mới, sau đó thử nhận
   ZIP, extract, launch, restart, update, idle và show/stop. Máy ARM VM hiện tại
   không thay thế kiểm tra này. Không tắt realtime hay thêm exclusion đại trà.

Nguồn chính thức:
- https://learn.microsoft.com/en-us/defender-xdr/developer-faq
- https://www.microsoft.com/en-us/wdsi/filesubmission

## Mất kết nối chỉ khi chiếu

TCP roster/control và UDP media là các đường riêng. UDP bị chặn đơn thuần
không đủ để giải thích roster mất kết nối. Show bật thêm renderer/decoder và tải
media; `run_student_media` còn propagate lỗi setup/session bằng `?`, nên lỗi
khởi tạo có thể kết thúc agent. Đây là đường cần điều tra, chưa chứng minh đã xảy ra.
Control cũng có liveness timeout 5s; quá tải, runtime starvation hoặc TCP bị
gián đoạn có thể gây reconnect. Không tăng timeout/buffer để che vấn đề.

Cần đồng bộ Teacher/Student full logs và Defender/Application events:
- Event Defender trùng thời điểm: kiểm tra quarantine/termination đúng file.
- Application 1000/1001, process PID đổi: kiểm tra crash/Guard restart.
- `teacher control liveness timeout`: đo control heartbeat/tải CPU và media.
- PID/control vẫn sống nhưng không có hình: kiểm tra UDP/firewall/adapter,
  ingress/depacketize, decoder và renderer; không gọi là app disconnect.
- Roster chỉ mất thumbnail/ready nhưng TCP sống: kiểm tra ý nghĩa UI trạng thái.

Thêm `scripts/collect-windows-defender.ps1` để thu bằng chứng chỉ đọc trên máy
phòng thật (admin giúp đủ quyền event). Ví dụ:

```powershell
& .\scripts\collect-windows-defender.ps1 -StudentPath 'C:\Classroom\ClassroomStudent.exe'
```

Mặc định ghi Desktop\Classroom-Defender-Evidence; giữ log Classroom riêng và
ghi giờ thao tác Show/Stop. Script không thu memory dump, không đổi Defender hay
firewall. Review dữ liệu paths/machine trước khi chia sẻ. Script chưa chạy đầy đủ
trên Windows; chỉ các query trạng thái/event/signature đã chạy qua SSH.

## Sửa thông báo kết nối

Phát hiện lỗi: popup reset trạng thái khi mất kết nối hoặc đang presenting, nên
Connected có thể xuất hiện lại sau reconnect/Stop. Thêm latch theo lifetime
`ConnectionStatus`: chỉ lần kết nối đầu của lần chạy app được phép thông báo;
không rearm sau mất kết nối, kết thúc chiếu, đổi preference. Nếu lần đầu popup
tắt/đang chiếu, không phát thông báo muộn. Restart app tạo lần chạy mới.

Internet/VS Code/activity policy handler hiện không đưa action vào hàng đợi
message popup; message cố ý gửi từ Teacher là một tính năng riêng. Không thêm
thông báo tiến độ/hoàn tất cho policy. Thay đổi thật vẫn có thể nhìn thấy (Internet
không truy cập được, VS Code bị relaunch); im lặng không có nghĩa che được tác động.

Verified: 5 focused connection UI tests pass, targeted rustfmt và diff check
pass; clippy Classroom thành công với 104 warnings. Workspace fmt check báo
các file ngoài patch không đúng format; không format lại toàn repo. Chưa build
release/deploy bản popup mới, chưa chạy UI Windows. Không sửa media hoặc Defender
policy dựa vào giả thuyết.

Next: thu event từ máy phòng thật và đúng mẫu bị chặn; build cặp khi cần deploy
popup; test reconnect/Show/Stop trên Windows rồi lab Windows 10 x64. Signing và
Microsoft false-positive review vẫn cần certificate/mẫu thực tế.
