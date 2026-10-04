# Classroom

Classroom là hệ thống quản lý và chia sẻ màn hình trong phòng học. Giáo viên
có thể chia sẻ màn hình cho cả lớp, xem màn hình từng sinh viên theo thời gian
thực, và điều khiển máy sinh viên khi được phép. Hệ thống được xây dựng theo
mô hình Windows-first, đồng thời có backend macOS để phát triển và kiểm thử.

## Kiến trúc tổng thể

```text
Teacher
  Flutter Teacher Console
        │ IPC / JSON commands
  Rust Teacher Agent
        │ capture → preprocess → H.264 encode một lần
        │ RTP/H.264 over UDP unicast fan-out
        ├──────────────► Student 1: Rust receive → decode → fullscreen render
        ├──────────────► Student 2: Rust receive → decode → fullscreen render
        └──────────────► Student N
```

### Teacher

- Flutter chỉ dùng cho giao diện Teacher.
- Rust Agent giữ toàn bộ media engine, control plane và kết nối với Student.
- Teacher capture màn hình một lần, encode một lần, sau đó phân phối cùng
  luồng H.264 tới nhiều Student.
- Giao diện có Grid view, List view và Compact List; trạng thái online được
  hiển thị bằng màu; kích thước Grid có chế độ Auto hoặc điều chỉnh thủ công.
- Có chế độ Share with Class và Share với các Student được chọn.
- Có cửa sổ Observe độc lập cho từng Student, mặc định chỉ xem.
- Observe hỗ trợ chuyển sang Control, đồng bộ chuột/bàn phím, cuộn trang,
  giữ trạng thái Control khi cửa sổ mất focus, và tiêu đề phân biệt Observe/
  Control.
- Có các chế độ hiển thị Fill, Stretch và Full screen; cửa sổ Observe tự fit
  theo vùng làm việc của màn hình Teacher.
- Có bảng thống kê phiên Observe/Control: mode, độ phân giải, FPS, bitrate,
  delay và các chỉ số texture/receiver liên quan.
- Có nút mở System Settings khi macOS thiếu quyền Screen Recording.
- Có thanh công cụ gửi message, gửi file, hủy và retry; Student có thể trả lời
  message một lần.
- Screenshot của màn hình Student được lưu trên Desktop của Teacher với tên
  gồm tên Student và thời gian.

### Student

Student là tiến trình Rust nhẹ, không cần Flutter GUI trong triển khai thực tế.

- Tự đăng ký với Teacher và tự reconnect khi control connection bị đóng.
- Nhận media qua UDP, dùng jitter buffer nhỏ, phát hiện mất gói và yêu cầu IDR
  khi cần.
- Dùng nguyên tắc latest-frame-wins: bỏ frame cũ khi bị dồn để giữ độ trễ thấp.
- Hiển thị nội dung Teacher ở chế độ fullscreen khi Teacher bắt đầu chia sẻ.
- Tự đóng preview khi Teacher dừng chia sẻ hoặc mất kết nối.
- Có native popup cho message; Student có thể đóng popup hoặc gửi một reply.
- File nhận được lưu trực tiếp trên Desktop của Student, áp dụng cho cả Windows
  và macOS. File được kiểm tra SHA-256 trước khi publish, không ghi đè file cũ,
  và dùng hậu tố khi trùng tên.
- macOS Student có status-bar menu gồm Exit và Restart.

## Media pipeline

```text
Capture
  → latest-frame slot (bounded, capacity effectively 1)
  → color conversion / resize
  → H.264 encoder
  → RTP packetizer
  → UDP fan-out
  → Student jitter buffer
  → H.264 decoder
  → renderer
```

Các bất biến quan trọng:

1. Không encode lại theo số lượng Student.
2. Không dùng queue video không giới hạn.
3. Ưu tiên frame mới và độ trễ thấp hơn việc phát đủ frame cũ.
4. Không cần classroom server riêng trong Phase 1.
5. Audio chưa nằm trong phạm vi hiện tại.

Windows ưu tiên DXGI Desktop Duplication, D3D11 và Media Foundation H.264;
hardware encoder/decoder là tối ưu tùy máy, không phải dependency bắt buộc.
macOS dùng ScreenCaptureKit, VideoToolbox và renderer native tương ứng.

Teacher giới hạn stream tối đa ở 1920×1080 và giữ tỷ lệ. Màn hình lớn hơn sẽ
được scale xuống, màn hình nhỏ hơn không bị upscale. FPS mục tiêu tối đa 30,
bitrate mặc định khoảng 6 Mbps và có adaptive quality trong khoảng cấu hình.

## Control và protocol

- Control plane dùng TCP đáng tin cậy, có TCP_NODELAY.
- Media dùng UDP unicast với RTP/H.264-compatible packetization.
- Có sequence number, timestamp, loss detection, NACK/repair ngắn và IDR recovery.
- Control port mặc định `47800`, media port mặc định `47810`, feedback port
  mặc định `47811`.
- Classroom tools dùng một TCP connection version riêng trên `control port + 2`,
  không đi chung với media queue.

## Telemetry và log

Log được ghi theo phiên cạnh executable, thường có dạng:

```text
logs/teacher-47800/log.txt
logs/student-47810/log.txt
```

Teacher thu thập capture FPS/latency, dirty ratio, encode FPS và p95 encode
latency, bitrate, frame drop/replacement, CPU, memory, network Mbps và số
Student kết nối. Student gửi thông tin RTT, packet loss, NACK, decode/render
FPS, dropped/late frame, receive bitrate và jitter delay.

Log runtime, build output và raw test evidence không được đưa vào repository;
xem [docs/12_TELEMETRY.md](docs/12_TELEMETRY.md) để biết cách đọc metrics.

## Chạy local

Yêu cầu:

- Rust stable theo `rust-toolchain.toml`.
- Flutter SDK cho Teacher Console.
- Windows 10/11 x64 là nền tảng triển khai chính.
- macOS cần cấp Screen Recording, Accessibility và Local Network khi được hỏi.

Build Rust:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release --bin Classroom --bin ClassroomStudent
```

Chạy hai role trên cùng máy:

```bash
Classroom --role teacher --loopback
Classroom --role student --loopback
```

Chạy trong LAN:

```bash
Classroom --role teacher --bind-ip 10.0.55.200
Classroom --role student --teacher-ip 10.0.55.200
```

Khi chạy không có `--role`, launcher native cho phép chọn Teacher/Student và
ghi nhớ Teacher IP. Địa chỉ mặc định trong launcher là `192.168.100.115`.

## Flutter Teacher Console

Mã nguồn ở `apps/classroom_console/`:

```bash
cd apps/classroom_console
flutter pub get
flutter test
flutter analyze
```

Build macOS Teacher:

```bash
scripts/build-flutter-macos.sh
```

Kết quả nằm ở `target/Classroom Console.app`. Mỗi bản ad-hoc build mới có thể
khiến macOS yêu cầu cấp lại quyền riêng tư.

## Windows build

Build native Windows nên thực hiện trên máy Windows để kiểm tra đầy đủ DXGI,
Media Foundation, firewall và quyền desktop. Cross-build từ macOS dùng cho
artifact kiểm tra:

```bash
RUSTC=/path/to/rustc \
CARGO_TARGET_DIR=target/rustup-windows \
rustup run stable cargo build --release \
  --target x86_64-pc-windows-gnu --bin Classroom --bin ClassroomStudent
```

### Clone và chạy trên một máy Windows khác

Yêu cầu: Windows 10/11 x64, Git, [rustup](https://rustup.rs) (toolchain
`stable-x86_64-pc-windows-msvc`) và Visual Studio Build Tools với workload
"Desktop development with C++". Không cần protoc hay truy cập vendor ngoài repo
(`vendor/` đã nằm trong repository).

```powershell
git clone https://github.com/maivanmanh/netsupport-school.git
cd netsupport-school
cargo build --release --bin Classroom --bin ClassroomStudent
# Teacher (thay IP bằng IP Ethernet của máy Teacher):
.\target\release\Classroom.exe --role teacher --bind-ip 10.0.55.200
# Student:
.\target\release\Classroom.exe --role student --teacher-ip 10.0.55.200
# Thử cả hai trên cùng một máy:
.\target\release\Classroom.exe --role teacher --loopback
.\target\release\Classroom.exe --role student --loopback
```

Cho phép `Classroom.exe` qua Windows Firewall (TCP control + UDP media) khi được
hỏi. Teacher Console Flutter (`apps/classroom_console`) là tùy chọn; build bằng
`scripts/build-flutter-windows.ps1` sau khi cài Flutter SDK.

Trong Windows/Parallels, `scripts/Run-Classroom.bat` lấy `Classroom.exe` từ
thư mục Downloads được chia sẻ, copy vào thư mục local có quyền executable,
unblock file, rồi mở hai cửa sổ Teacher và Student. Log loopback được ghi về
thư mục `logs` trong Downloads được chia sẻ.

## Trạng thái hiện tại

Đã hoàn thành và kiểm thử:

- Phase 1 capture → encode → transfer → decode → display.
- Latest-frame, adaptive quality, reconnect, late join và fullscreen Student.
- Teacher Flutter Console với Grid/List/Compact, Observe/Control và telemetry.
- Điều khiển chuột, bàn phím, scroll và clipboard hai chiều.
- Message, reply một lần, screenshot Desktop và file delivery có checksum,
  progress, cancel/retry.
- Build Rust macOS, cross-build Windows x64 và Flutter macOS Teacher.

Các giới hạn còn lại:

- Acceptance đầy đủ với 40 máy Windows vật lý trên wired LAN chưa hoàn tất.
- Windows Teacher Flutter build cần được compile và kiểm thử trên Windows.
- Screenshot Windows Teacher cần được xác nhận bằng bản Flutter Windows thực tế.
- macOS ad-hoc app có thể cần cấp lại quyền privacy sau mỗi lần thay bundle.

## Tài liệu

- [Yêu cầu kỹ thuật](AGENTS.md)
- [Kiến trúc Teacher/Student](docs/15_ROLE_ARCHITECTURE.md)
- [Kế hoạch Phase 2](docs/13_PHASE2_PLAN.md)
- [Hướng dẫn sử dụng Phase 2](docs/14_PHASE2_USAGE.md)
- [Telemetry](docs/12_TELEMETRY.md)
- [Tiến độ và bằng chứng kiểm thử](checkpoints/PROGRESS.md)
