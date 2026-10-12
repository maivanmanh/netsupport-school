# Kiểm tra tự động 4 Student Windows — 12-10-2026

## Phạm vi và môi trường

Owner cho phép test phần mềm, thu log, sửa code, build theo cặp và cập nhật qua
Classroom. Không reboot/ngắt thiết bị. Terminal/CMD chưa ghép cặp quản trị:
ghi nhận thiếu điều kiện, không bypass pairing.

Teacher `10.0.79.200`, màn hình nguồn **1600×900**, DXGI Microsoft Basic Render
Driver, H264 Encoder MFT software. Student Windows 10 Education 19045, i7-4770,
4 core/8 thread, khoảng 8 GB RAM, Ethernet vật lý báo 1 Gbps:

| Máy | IP | Màn hình |
|---|---|---|
| 709-17 | 10.0.79.117 | 1280×1024 |
| 709-18 | 10.0.79.118 | 1280×1024 |
| 709-19 | 10.0.79.119 | 1024×768 |
| 709-20 | 10.0.79.120 | 1280×1024 |

Đây là kiểm tra chức năng/đo ngắn trên 4 máy thật. Không phải 1080p, 40 máy,
soak dài, hay phép đo độ trễ quang học. Không ép tăng độ phân giải trên máy cũ.

## Lỗi đã xác định và vá

**Screenshot khi thumbnail đang chạy:** cả 4 máy thất bại trước sửa với
`duplicate selected desktop output: E_INVALIDARG (0x80070057)`. Khi nhận chiếu,
thumbnail dừng và screenshot lại thành công. Screenshot và monitoring tạo hai
DXGI duplications của cùng output trong một process. Windows cấm trường hợp này:
[IDXGIOutput1::DuplicateOutput](https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_2/nf-dxgi1_2-idxgioutput1-duplicateoutput).

Windows screenshot nay dùng child process một lần, vẫn capture DXGI/encode PNG
như cũ; không thêm encoder broadcast, không dừng monitoring. Parent giới hạn
12 giây, xử lý cancel, kill/wait khi lỗi và xóa file lỗi tạm. Child capture giới
hạn 10 giây. Non-Windows giữ nguyên đường capture cũ.

Regression thực tế `scripts/test-windows-student-screenshot.py` PASS 4/4:
PNG đúng kích thước, thumbnail còn fresh sau capture. Audit không có child
screenshot sót và mỗi máy chỉ có một Student process.

**Script smoke dùng chung AppData:** dù control/IPC ports riêng, hai Flutter
Teacher vẫn dùng chung logs/settings. Lượt smoke song song đã thay log Teacher
đang chạy. Đã thêm guard từ chối smoke nếu có Teacher/console đang chạy; đã thử
guard và xác nhận từ chối trước khi tạo phiên thử. Lượt media sau đó được chạy
lại riêng. Không dùng teacher log bị thay làm bằng chứng của lượt cuối.

## Ma trận kết quả

| Hạng mục | Bằng chứng / giới hạn |
|---|---|
| Badge monitoring | 4/4 PASS: bulk/individual, rapid latest preference, 5 vòng create/destroy; trả về off |
| Screenshot | Trước sửa fail idle 4/4; sau update PASS idle + đang nhận chiếu; PNG và thumbnail được kiểm tra |
| Teacher chiếu cả lớp | Hai lượt workload WinForms chữ cuộn/chuyển động; hình chụp receiver có nội dung đúng |
| Student làm nguồn | Cả 4 lần lượt chiếu cho 3 máy còn lại, sau đó về Teacher; `source-media-samples.json` và ảnh recipient |
| Observe/Control | Native window live 4/4; `classroom123` nhập vào textbox fixture, tải lại và assert đúng 4/4 |
| File system / transfer / Launch | Liệt kê Desktop, push script, chạy PowerShell ẩn, fetch JSON/text/PNG thực tế 4/4 |
| App inventory / extension inventory | Đọc danh sách 4/4; một số tên packaged Windows app còn là resource identifiers |
| Copilot network | Bật/tắt, ACK complete 4/4, fingerprint/status; chưa chứng minh request Copilot có tài khoản thật bị chặn |
| Internet | WFP/DNS policy block-only một IP TEST-NET, ACK 4/4, về open; không cắt LAN; chưa test toàn bộ trình duyệt/DoH |
| App policy | Empty block list bật/tắt ACK 4/4; chưa coi là bằng chứng chặn executable thật |
| VS Code policy | Allowlist các extension đang cài bật/tắt ACK 4/4, registry readback trong enforcer; không có Code đang chạy lúc thử |
| Phục hồi VS Code | Sao lưu trước thử; audit hash settings + SDDL ACL khớp bản gốc 4/4; AllowedExtensions đã bỏ |
| Terminal/CMD | `terminal=0`; terminal_open bị từ chối đúng 4/4; end-to-end chưa đủ điều kiện |
| Hardware Control | PowerShell mock contracts PASS trên Windows; không có Guard service trên 4 Student; không thử disable/hotplug/reboot |
| Diagnostics | Thu đủ Teacher + 4/4 Student, missing=0 |
| Teacher mới | Paired build thành công; smoke loopback registration/thumbnail/share-stop×2/reconnect/native Observe PASS |

Không suy ACK policy thành bằng chứng mọi ứng dụng bên thứ ba đã thực thi policy.
Không thay đổi scope để cài service hoặc ghép cặp quản trị.

## Số đo lượt riêng cuối

Workload foreground chữ cuộn + hình chuyển động, khoảng 42 giây, nguồn 1600×900.
Teacher target 6 Mbps, một encoder, không send errors/cache misses ở interval cuối;
actual mỗi receiver trung bình khoảng 4,68 Mbps. Capture khoảng 16 FPS và encode
khoảng 22 FPS ở interval cuối; pipeline có repeated-frame submissions, không
đồng nhất encode FPS với số hình nguồn mới.

| IP | Decode FPS | Render FPS | Decode p95 ms | Missing packets | Decode errors |
|---|---:|---:|---:|---:|---:|
| .119 | 22,63 | 22,48 | 19,46 | 0 | 0 |
| .118 | 22,62 | 22,53 | 21,28 | 0 | 0 |
| .117 | 22,63 | 22,52 | 20,40 | 0 | 0 |
| .120 | 22,63 | 22,54 | 21,50 | 0 | 0 |

Ví dụ .119 có ingress drops=0, stale backlog=0, recovery episode=1 và recovery
peak khoảng 462 ms (startup/recovery, không phải độ trễ quang học). Receiver
theo sát tốc độ encode; chưa có bằng chứng cần tăng buffer hay redesign receiver.
Nguồn Teacher dùng Basic Render Driver/software MFT: cần đối chứng Teacher có
driver GPU vật lý trước khi quy mức ~22 FPS cho cấu hình Student.

Lượt trước chạy gần smoke có render ~20,4–20,6 FPS, missing 163/165 trên hai máy,
stale backlog 18/19 và recovery ~630 ms. Lượt riêng về zero missing, nhưng không
đủ thiết kế A/B để quy nhân quả duy nhất cho smoke. Giữ số đo đó trong audit
history; không sửa tuning network theo một lượt bị nhiễu.

## Build / triển khai / giới hạn công cụ

Source baseline `f5f25a0` + working-tree screenshot fix. Fresh paired build:
`1791766942-windows-room-fix-20261012`. Student exe SHA256
`027898cf89cd30bb3180548ed7eabd6121f4e59fc8cd16a0373938f0df5db230`.
Teacher mới ở `target/room-validation-build`, có Student ZIP mới tương ứng.
Updater job `42205524f2969c3b5770069dda49435b`: complete 4/4, cùng ID/đường dẫn
cài đặt được giữ, reconnect xác nhận build mới. Không dùng SSH/SCP.

Tự động review từ chối lệnh dừng/xóa build Teacher cũ: `blocked by policy`.
Không xóa thư mục cũ. Gói Teacher mới chạy từ thư mục cô lập không có kết nối
Student; quay về đường dẫn cũ thì 4 máy nối lại ngay. Có dấu hiệu firewall theo
đường dẫn; truy vấn firewall application filters bị Access denied. Không đổi
firewall toàn hệ thống hoặc kết luận đã chứng minh chính xác rule nào.

Teacher đang hoạt động giữ đường dẫn cũ
`target/Classroom-Console-Windows-x64/classroom_console.exe`, build cũ
`1791765510-paired-20261012-073748`; Student chạy bản vá mới. Không gọi đây là
đã deploy Teacher mới. Gói Teacher mới build/smoke-verified sẵn, không thay v1 assets.

## Kiểm tra source

- Rust app tests: 246 pass, 1 ignored, 1 filtered baseline failure
  `chrome_timestamps_convert_to_unix_milliseconds` (actual 1704470400000,
  expected 1704067200000); lượt đầy đủ trước đó xác nhận failure này.
- Clippy chạy xong với warnings khi allow baseline `items_after_statements`;
  strict baseline chưa xanh. Không format cả repository để tránh diff không liên quan.
- Rustfmt screenshot file và Python py_compile regression script.
- Flutter baseline: 56 pass/9 fail Observe/Control native-channel/layout tests;
  cùng nhóm lỗi đã ghi nhận từ Mac. Không báo cả suite pass.

## Bằng chứng lưu cục bộ

`target/room-validation/`: events badge/preflight/sources/features/policies/vscode/audit,
screens, screenshot-regression/results.json, input-proof, inventory, deploy.log,
build.log, rust-tests.log, clippy.log, diagnostics-final.json, diag-final,
media-final-summary.json. Không commit endpoint/token hay screenshot người dùng.
Smoke mới: `target/windows-flutter-smoke/20261012-080859/results.json`.

Thứ tự tiếp theo: dùng log lượt riêng để tách packet gap/recovery khỏi tải Teacher
và receiver, đo dài hơn ở cấu hình phòng hiện tại; sau đó Windows x64 vật lý,
1080p cố định và ladder 1/5/10/20/40 theo docs/06. Terminal/service/hardware thật
vẫn cần môi trường đủ điều kiện; không đánh dấu pass.
