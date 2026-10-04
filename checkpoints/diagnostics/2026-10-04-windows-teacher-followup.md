# Kiểm tra tiếp Windows Teacher trên LAN Wi-Fi — 2026-10-04

## Phạm vi và môi trường

Chủ dự án yêu cầu thực hiện cả bốn nhóm: so sánh receiver Mac/Windows ARM;
đổi màn hình và phục hồi; chạy lâu, reconnect và crash; fan-out 1/5/10/20/40.
Teacher là Windows 11 ARM trong Parallels, IP `192.168.50.94`, chạy Flutter/Rust
x64 qua giả lập, DXGI và software `H264 Encoder MFT`. Windows ARM Student
`192.168.50.105` thực tế là VM trên chính Mac Student `192.168.50.205` (đã xác
nhận bằng `prlctl`). Vì vậy hai hệ điều hành Student dùng chung máy vật lý/NIC.
Không có kiểm chứng 40 PC vật lý, Ethernet, hardware encoder hay độ trễ quang học.

Mac Student đã được dừng, xóa riêng bundle cũ rồi chép vào:
`/Users/mvmanh/Classroom-teacher-validation/Classroom Student.app`.
Chủ dự án xác nhận cấp Screen Recording và Accessibility cho đường dẫn này.
Binary `Contents/MacOS/ClassroomStudent` có SHA-256
`4d1ec3ed667a1b71f24a200d0ede349836f759e8a5e5a611782dddddc4f2f20b`,
trùng gói Student v1; không tuyên bố đây là một bản Student mới sửa trong lượt này.
Settings, profile, log và ứng dụng Apple không bị xóa.

## Những lỗi đã sửa

1. Admission encoder cũ có thể vượt 30 FPS khi liên tục có capture mới. Thay bằng
   ngân sách trượt một giây, tối đa `max_fps` admission, giữ khoảng cách tối thiểu
   giữa frame và không chạy bù thành burst sau stall. Hàng đợi vẫn bounded/latest.
   Test kiểm tra capture liên tục, stall 200 ms và deadline tương lai không được
   làm ngân sách hết hạn sớm. Cách chỉ ép khoảng cách cứng ban đầu bị xuống khoảng
   26 FPS trên scheduler này nên không được giữ lại.
2. Software MFT nhận `AVEncVideoMaxKeyframeDistance` nhưng vẫn phát IDR khoảng mỗi
   giây. Dùng `AVEncMPVGOPSize`, fallback thuộc tính cũ nếu bị từ chối. Không đổi
   codec, B-frame hay số encoder. IDR do yêu cầu phục hồi vẫn được phép.
3. Lượt force-kill helper cuối phát hiện Flutter không respawn Rust: cổng endpoint
   cũ trả timeout 10060, trong khi startup chỉ xóa endpoint khi connection refused.
   Thêm kiểm tra PID bằng Windows query-only API trước attach. Chỉ xóa descriptor
   khi xác nhận chủ đã kết thúc; PID còn sống hoặc không truy vấn được vẫn giữ.
   Không coi helper đang pause là crash để tạo hai Teacher. Bổ sung log lỗi startup
   và test PID đang chạy/PID đã thoát dù handle vẫn được giữ.

## 1. Receiver A/B

Các lượt A/B dưới đây có capture thực tế **1488×768**, không phải 1080p.
Tên workload không chứng minh nó ở foreground; không coi đây là tải full-screen
chuyển động hoặc so sánh bandwidth được kiểm soát hoàn toàn.

| Receiver/lượt 120 giây | Decode FPS | Render FPS | Keyframe requests | Decode errors |
| --- | ---: | ---: | ---: | ---: |
| Mac, baseline | 30.58 | 26.52 | 1 | 0 |
| Windows ARM, baseline | 25.85 | 20.47 | 57 | 0 |
| Windows ARM, bản sửa cuối | 28.49 | 24.31 | 20 | 0 |
| Mac, bản sửa cuối | 29.45 | 26.34 | 1 | 0 |

Teacher bản cuối encode khoảng 29.44–29.59 FPS, một encoder, không send error.
Windows baseline có 71 missing sequences, 93 late packets, 186 stale backlog;
bản cuối còn 16 missing sequences, 56 late packets, stale backlog 0. Recovery peak
giảm từ khoảng 1429 ms xuống 1063 ms; vẫn chưa đạt smoothness/latency acceptance.
Mac bản cuối không missing/decode error, recovery peak khoảng 300 ms.
Windows headless decode khoảng 30.06 FPS cho thấy tải renderer/scheduler đáng
điều tra; đây là dấu hiệu, chưa đủ kết luận nguyên nhân hay hiệu quả riêng từng fix.

## 2. Màn hình, khóa và sleep

- Đã đổi desktop qua 2560×1440, 3840×2160, 2560×1600, 1280×800 và 1920×1080.
  Kích thước fit đổi thì phiên dừng với thông báo restart presentation; Share lại
  hoạt động với hai Student. 2560×1440 → 3840×2160 cùng fit 1920×1080 giữ phiên
  và DXGI phục hồi access loss. Không thay quyết định restart khi đổi kích thước.
- Đã gọi LockWorkStation; chủ dự án xác nhận mở khóa. Log ghi AccessDenied và
  capture phục hồi khoảng 313 ms. Không có audit chứng minh giữ khóa 20–30 giây,
  vì vậy không tuyên bố đã đo đầy đủ thời lượng khóa đó.
- DPI cửa sổ thực tế 96/100%. Windows Settings vô hiệu hóa scale combo do custom
  scaling đang đặt; không thay cấu hình hoặc logoff. 125%/150% chưa kiểm chứng.
- `powercfg /a` chỉ có S0, không S1/S2/S3/hibernate. Native SetSuspendState trả
  `ERROR_NOT_SUPPORTED (50)` dù privilege của tiến trình gọi được bật tạm rồi
  phục hồi. Không có sleep/resume thật. Wake task thử nghiệm đã được gỡ.
- Đã pause chính helper Teacher 5 giây rồi resume bằng native API với finally;
  IPC quay lại trạng thái sharing. Đây là process stall, không thay thế sleep OS.

## 3. Chạy 30 phút và fault

Soak kéo dài khoảng 17:39–18:09, có Share/Stop, ngắt/chạy lại Windows Student và
late join. Parallels tự đổi desktop xuống 1452×810 rồi 1436×808, làm phiên dừng
theo chính sách kích thước. Chủ dự án đồng ý cố định 1920×1080; native/log xác
nhận 1080p từ khoảng 17:54, sau đó dùng cho fan-out. Không gọi đây là 30 phút
chiếu 1080p liên tục: một phần thời gian Student thật idle khi chọn simulator.

Force-kill Flutter giữ helper cũ sống; mở GUI lại attach đúng helper, không tạo
coordinator thứ hai. Observe Mac được yêu cầu mở trong lượt fault. Force-kill Rust
ban đầu tái hiện lỗi respawn nêu trên. Bản sửa native thay endpoint PID 8320 bằng
helper 3652; khi kill 3652, cùng GUI PID 6396 tự tạo helper 7376. Cả hai lần Share
lại có hai Student; phiên sau cùng ghi Windows decode/render 66/51 frame, Mac
70/60 frame, đều không decode error. Đây là phiên phục hồi ngắn/static có adaptive
FPS, không phải lượt đo 30 FPS. IPC Exit đóng GUI mã 0 và helper 7376 đã thoát.
Kiểm tra theo đường dẫn xác nhận không còn GUI/helper test, ScheduledTask Windows
hay LaunchAgent simulator. Student mở tạm để cấp quyền cũng đã được dừng.
RAM/handle thay đổi qua nhiều phiên MFT và cache,
không dùng một lượt này để kết luận không có memory leak.

## 4. Fan-out cùng một Teacher, mỗi mức 90 giây

Capture thực tế 1920×1080; cùng Teacher PID trong cả năm mức. Các receiver là
process headless trên một Mac, không phải các PC độc lập. Foreground kiểm tra
được là terminal/Codex; stream chỉ khoảng 0.60–1.19 Mbps dù target 6 Mbps.
Do đó bảng không chứng minh tải chuyển động 6 Mbps × 40.

| Số receiver | Teacher encode FPS | Tổng send Mbps | Decode FPS trên receiver |
| ---: | ---: | ---: | ---: |
| 1 | 29.17 | 0.76 | 28.87 |
| 5 | 29.21 | 3.26 | 29.05–29.09 |
| 10 | 29.17 | 7.66 | 28.42–29.03 |
| 20 | 29.11 | 25.04 | 2.19–4.14 |
| 40 | 28.81 | 48.98 | 0–0.011 |

Mỗi mức vẫn đúng một encoder; send errors và abandoned packets đều 0. Mức 20/40
**không đạt receiver delivery/decode**. Một receiver ở mức 40 chỉ decode một frame
với latency khoảng 453 ms, 179 keyframe requests, 7027 stale backlog frames,
96 ingress drops; Teacher có 31958 NACK cache misses. Decoder dùng chung Mac và
burst/loss Wi-Fi đều là giả thuyết cần tách riêng, không quy lỗi duy nhất cho một bên.
Tất cả simulator LaunchAgent được dừng sau ladder.

## Kiểm tra mã và bằng chứng

- Rust app x64: 107 tests pass, 1 ignored; `cargo fmt --all` pass; release build pass.
- Clippy thông thường hoàn tất với warnings. `-D warnings` vẫn lỗi do lint có sẵn
  ở GPU/dependency và deprecated API dưới Rust 1.99; không ghi strict lint pass.
- Flutter analyze không issue, 24 tests pass trước release build bản sửa lifecycle.
- Package Teacher mới:
  `C:\Users\mvmanh\Desktop\netsupport-school\target\Classroom-Teacher-Validation-final-x64\classroom_console.exe`;
  GUI SHA-256 `a6c8ae10ec403dc541796b3ceb26377d800aaa87d4b9d5c6026dab1970cc3355`.
  Có runtime DLL, data và Student x64 v1 dùng kèm; Mac/Windows ARM Student không
  đổi binary ở lượt sửa Teacher này.
- Rust helper cuối SHA-256:
  `5253756c08631726ae67f14fdfa3e3d441e6578048185ac876e6ecdae6da5c8e`.
- Controller tái sử dụng: `scripts/test-windows-teacher-validation.py`; SSH password
  chỉ trong bộ nhớ. Chỉ stop task/job/process được tạo bởi controller.
- Dữ liệu riêng: `target/windows-teacher-validation/20261004-172552/` (A/B cuối,
  display, soak, fault), `.../20261004-175616/` (scale-summary và năm mức).
  Baseline: `.../20261004-165842/`; headless: `.../20261004-171212/`.
  Không commit raw log, endpoint/token IPC hoặc credential.

Automatic approval review đã từ chối xóa local `ClassroomAgent.exe` ở package
staging cũ. Không vượt qua từ chối; dùng thư mục package mới riêng biệt. Không
thay asset GitHub v1 bằng bản thử nghiệm này.

## Bước tiếp theo chính xác

Tách UDP/depacketize sink khỏi decode trên host simulator để đo burst/loss mà
không ép 40 decoder dùng chung VideoToolbox. Sau đó dùng Windows x64 vật lý qua
Ethernet với workload foreground xác nhận, đo latency quang học và ladder
1/5/10/20/40 theo `docs/06_PERFORMANCE_ACCEPTANCE.md`. Cần lượt riêng cho DPI
125%/150% và sleep/resume thật khi môi trường hỗ trợ.
