# Guard: kiểm soát máy Student (Internet hôm nay, chống tắt app sau này)

## Bổ sung 09-10-2026: chặn mạng Copilot độc lập

Policy có thêm `copilot_block`, được thay đổi qua job riêng và giữ nguyên luật
Internet đang áp dụng. Internet UI cũng không được gỡ flag này. Dùng chung DNS
proxy/NRPT và WFP: từ chối domain Copilot, chặn IP public IPv4/IPv6 đã phân giải
và cache DNS, cập nhật định kỳ. Thay filter bằng transaction để không có khoảng
trống khi refresh. Checkbox nằm trong VS Code extensions, đếm máy Windows hỗ trợ
riêng với máy không hỗ trợ. Xem
[phạm vi, giới hạn và kiểm chứng](../checkpoints/diagnostics/2026-10-09-copilot-network-and-extension-rescan.md).

Cập nhật 06-10-2026. Student chạy với quyền Administrator (chủ dự án xác nhận:
app được admin cài sẵn trước khi sinh viên dùng). Chỉ Windows; macOS chưa làm.

## Cấu trúc (apps/classroom/src/guard/)

- `Policy` (mod.rs): tài liệu trạng thái mong muốn do Teacher gửi nguyên bản mỗi
  lần (`internet`, `lease_minutes`, sau này thêm `protect`…). Trường mới là trường
  tùy chọn nên bản cũ vẫn đọc được.
- `trait Enforcer`: mỗi mối quan tâm là một enforcer (`apply`/`clear`). Hiện có
  `InternetEnforcer`. Thêm tính năng = thêm enforcer + một field trong `Policy`.
- `Guard`: giữ danh sách enforcer, lưu policy xuống `guard-policy.json` (resume sau
  khi Student khởi động lại), `lease` (tự gỡ nếu mất Teacher quá lâu), `restore()`
  khi khởi động, `clear()` khi thoát.
- Teacher → Student qua job `policy` trên kênh tools hiện có; Student báo
  `capability guard` (Windows + elevated) và `internet` (mode đang áp dụng).

## Internet (guard/internet.rs, windows.rs, dns.rs, rules.rs)

Chặn ở tầng mạng, áp dụng cho MỌI tiến trình (trình duyệt, app native, curl, code tự viết):

1. **WFP** (Windows Filtering Platform, lớp `ALE_AUTH_CONNECT` v4/v6, session *dynamic*):
   permit loopback + mạng riêng/LAN + Teacher + DNS upstream + địa chỉ đã học; block
   mặc định mọi thứ còn lại (block_all, allow_only). Dynamic session ⇒ Student chết thì
   Windows tự gỡ filter (không bao giờ kẹt mạng); policy được áp lại khi Student mở lại.
2. **DNS proxy cục bộ** (127.0.0.1:53, UDP+TCP) + luật **NRPT** `.` → 127.0.0.1: mọi
   truy vấn tên của máy đi qua proxy. `allow_only`: tên khớp luật được forward và các A
   record của nó được *học* thành permit (trước khi trả lời app); tên khác bị NXDOMAIN
   (trừ tên chỉ trỏ vào mạng nội bộ). `block_only`: tên khớp bị NXDOMAIN, kèm chặn các
   DoH/DoT resolver công cộng để không né proxy. AAAA/HTTPS/SVCB bị trả rỗng khi đang
   giới hạn để app dùng IPv4 (tránh IP “lọt” qua hint).
3. Luật: `example.com` (cả subdomain), `*.example.com`, `a*.b.com`, IPv4/CIDR, URL dán
   vào được rút về host. Cổng và đường dẫn không áp dụng được (HTTPS ẩn chúng).

Giới hạn đã biết: block_only là chặn “mềm” với app dùng IP cứng hoặc DoH riêng chưa
nằm trong danh sách; allow_only/block_all là chặn cứng. CDN dùng chung IP có thể mở
nhầm trang khác trên cùng IP trong allow_only. Ctrl+Alt+Del/Task Manager vẫn có thể
dừng Student (xem dưới).

## Chống tắt Student (kế hoạch, chưa làm)

Ưu tiên theo độ bền: (1) tách Guard thành **Windows service** (SYSTEM, auto-start,
recovery = restart) mà Student nói chuyện qua IPC cục bộ — `Guard` đã tách khỏi UI
nên chuyển nguyên; (2) service giám sát và tự mở lại Student trong phiên người dùng;
(3) đặt DACL của process Student từ chối `PROCESS_TERMINATE` cho user thường;
(4) filter WFP *persistent* để chặn mạng vẫn còn khi cả service bị dừng; (5) ngăn
sửa/xóa binary & dịch vụ bằng ACL. Mỗi mục là một `Enforcer` mới (`protect`).

## Khôi phục khẩn cấp

`scripts/windows-guard-reset.ps1` (PowerShell admin): gỡ rule NRPT, dừng Student
(giải phóng filter WFP), xóa `guard-policy.json`.
