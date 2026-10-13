# Ubuntu Student — kiểm tra chức năng với Teacher thật (13-10-2026)

## Kết quả

**Pass luồng chức năng nhận màn hình của toàn bộ Ubuntu Student agent.** Teacher
macOS capture màn hình thật bằng ScreenCaptureKit, encode H.264 bằng VideoToolbox,
gửi qua mạng ảo Parallels đến Ubuntu Student. Không dùng nguồn H.264 fixture trong
lượt này. Không đánh giá hiệu năng: owner xác nhận VM nằm trên ổ đĩa chậm và chỉ
cần kiểm tra chức năng.

| Trường hợp | Bằng chứng | Kết quả |
| --- | --- | --- |
| Kết nối lúc Teacher idle rồi Start | Roster online, `student first decoded frame`, `student first presented frame`, báo cáo nhận/decode/render tại Teacher | Pass |
| Stop rồi tiếp tục giữ Student | Preview đóng theo Stop; tiến trình Student vẫn sống với cùng PID | Pass |
| Student vào khi Teacher đang chiếu | Dừng Student thử nghiệm, Start khi chưa có receiver, chạy lại cùng profile; có frame đầu tiên và cùng instance ID | Pass |
| Teacher crash/restart | Kill đúng Teacher do harness tạo; preview đóng; Teacher mới nghe cùng cổng; Student tự đăng ký và render lại, cùng tiến trình và danh tính | Pass |
| Teacher ngừng phản hồi | SIGSTOP đúng Teacher thử nghiệm; Student ghi một control-liveness timeout và đóng preview; SIGCONT, tự kết nối lại và render | Pass |
| Phiên chức năng 3 phút | 180 giây, 18 mẫu trạng thái; Student luôn online, nhận/decode/render tiến triển; không đặt ngưỡng FPS/CPU/latency | Pass |
| Kết thúc | Stop/quit Teacher, SIGINT Student thử nghiệm; kiểm tra cuối `alive:false`; đóng workload và HTTP transfer server của harness | Pass |

## Môi trường và phạm vi

- Teacher: Mac hiện tại, `10.211.55.2`, interface `bridge100`. Dùng helper có sẵn
  tại `target/Classroom Console.app/Contents/Helpers/Classroom Agent.app/Contents/MacOS/Classroom`.
  Build ID thực tế: `1791815129-paired-20261012-212529`, protocol 8. Kiểm tra
  Screen Recording ngoài sandbox trả `screen:true`; không cấp/reset quyền.
- Student: Ubuntu **22.04 ARM64**, user `parallels`, `10.211.55.5`, `enp0s5`, GNOME
  Wayland qua XWayland. Chạy toàn bộ executable `ClassroomStudent` đã build native
  ở milestone trước, không chạy unit-test receiver thay cho agent.
- Đây là mạng ảo Mac ↔ Parallels VM trên cùng host; không phải Ethernet phòng máy.
- Room cô lập: TCP control 49000, monitoring 49001, tools 49002; UDP Student 49010,
  Teacher source 49011. Không kết nối các Student đang triển khai ngoài VM.
- Profile, SQLite, log và binary Student thử nghiệm nằm riêng trong
  `/tmp/classroom-ubuntu-lan-20261013-114418/`; profile giữ qua hai lượt chạy Student.
  Không thay thế deployment hay settings/profile thật.
- Teacher dùng package có sẵn, không rebuild/publish Teacher hoặc thay ZIP Student
  bên trong package. Đây là kiểm tra tương thích protocol với Ubuntu backend mới,
  không phải gói release cặp mới. SHA-256 hai executable được lưu trong manifest.

## Nguồn hình và bằng chứng

Capture thực tế **1512×982** (giới hạn cấu hình vẫn 1920×1080), không gọi đó là
1080p. Profile chức năng giới hạn 5 FPS, target 2 Mbps; giữ nguyên các timeout và
bounded queues của sản phẩm. Cấu hình riêng không thay default production.

`scripts/ubuntu-lan-workload.swift` tạo cửa sổ native có chữ Tiếng Việt, mã nguồn,
chữ thay đổi/cuộn mô phỏng và đổi màu. Audit ghi **239/239 mẫu ở foreground**;
không lưu nội dung màn hình hay tên ứng dụng khác. Đây là nguồn screen capture
thật của Teacher, dù nội dung hiển thị là workload kiểm thử có kiểm soát.

Log Student ở phiên cuối có 905 decoded và 903 rendered frames, `decode_errors=0`
và `depacketizer_errors=0`. Các số đếm này chỉ chứng minh hình tiếp tục đi qua
pipeline; không dùng để kết luận hiệu năng VM. Cursor channel nhận state/image,
hoàn tất ảnh cursor, `pointer_malformed=0`; unit test trước đó kiểm tra blend/
di chuyển/xóa overlay. Chưa đánh giá bằng mắt độ rõ chữ, hình con trỏ hoặc black
bars ở mọi tỷ lệ màn hình, không suy diễn các counter thành visual QA.

Student sau lần khởi động thứ hai có ba lần first-present, ba lần room connection,
hai lần preview-close do disconnect và một control-liveness timeout. Không có
`student discarded unrecoverable access unit` trong lượt cuối. Instance ID được
Teacher đối chiếu giống nhau sau restart và reconnect.

## Chạy lại và các sửa ở harness

```sh
python3 scripts/test-ubuntu-student-lan.py --soak-seconds 180
```

Harness dùng binary Teacher hiện có, biên dịch workload Swift nhỏ, copy Student
validation binary trong VM sang thư mục test riêng, chạy đúng graphical session
của user bằng Parallels guest tools. Không cần SSH password trong script hoặc log.
Worker chỉ signal process group có command line khớp chính xác binary của run.
Mỗi lần start/setup có journal để retry an toàn, không tạo Student thứ hai.

Các lượt trước được giữ riêng, không ghi lại thành pass:

1. `20261013-113727`: Parallels Tools trả lỗi job handle `PrlJob_GetRetCode: Invalid
   argument`. Bổ sung tối đa ba retry chỉ cho lỗi transport cụ thể đó; setup/start
   đã idempotent và stop kiểm tra process ownership.
2. `20261013-114022`: profile default 30 FPS đã nhận/render, Stop và late join;
   raw Teacher agent restart từ chối file IPC cũ của tiến trình bị kill. Harness
   mới dùng IPC endpoint riêng cho Teacher mới, cùng room ports/database; giữ
   endpoint cũ làm bằng chứng. Đây không phải kiểm chứng Flutter tự phục hồi helper.
3. `20261013-114418`: tất cả các trường hợp trong bảng trên pass với profile 5 FPS.

Không sửa decoder, renderer, giao thức, jitter/buffer, pacing hoặc timeout sản phẩm
trong milestone này. Các thay đổi là harness tái lập và tài liệu bằng chứng.

## Artifacts và việc còn lại

Artifact đầy đủ tại `target/ubuntu-lan-validation/20261013-114418/`: `manifest.json`,
`results.json` (`passed:true`), `functional.toml`, `teacher-{1,2}.log`,
`student-{1,2}.log`, `soak-samples.json`, `workload-audit.json` và stdout tương ứng.
Các endpoint IPC có token riêng tư nằm trong thư mục test bị Git ignore; không
đưa chúng vào commit hay chuyển sang máy khác.

Chưa xác minh: Ubuntu x64/24.04, native Wayland, screenshot visual QA, đổi DPI/
độ phân giải/multi-monitor, sleep/resume, loss/reorder có chủ đích và soak dài hơn.
Không có kết luận hiệu năng 1080p hoặc 40 PC. Kết quả này cũng không chứng minh
Ubuntu đã có capture/Observe, remote input, khóa máy hoặc hardware/Guard controls.

Bước triển khai tiếp theo: launcher/cài đặt per-user và autostart trong graphical
session, giữ profile SQLite bên ngoài binary; sau đó bổ sung Ubuntu package vào
bootstrap/updater của Teacher theo rule build cặp. Capture/Observe và OS controls
vẫn là các backend Linux riêng cần triển khai sau.
