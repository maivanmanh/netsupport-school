# Chuyển nguồn chiếu và checklist quyền — 2026-10-09

## Phạm vi

Owner đồng ý chuyển nhanh Teacher ↔ Student A ↔ Student B; nguồn Student mặc
định Control cooperative, cả giáo viên và Student cùng thao tác. Thiết bị mới:
Teacher Mac192.168.100.171; Windows192.168.100.102 (SSH mvmanh), Mac Student
192.168.100.254 (SSH lolem). Windows đang có Student/Guard trong Program Files;
không thay hoặc dừng bản installed, dùng package kiểm thử riêng. Mac test cũ trong
Classroom-two-mac/bin được giữ nguyên. Không lưu mật khẩu hay token trong báo cáo.

## Đã triển khai

- Nút Control & present Student dùng được khi đang chiếu; Back to Teacher quay về
  nguồn giáo viên. Hiển thị nguồn hiện tại và trạng thái Switching source.
- Một target đang chờ; kiểm tra target trước khi Stop, reject offline/multiple/busy.
  Target hiện tại no-op. Stop hủy target; chờ worker cũ kết thúc/join và giải phóng
  Control trước khi tạo stream mới. Deadline chờ Stop10s; không tăng buffer.
- Mỗi nguồn dùng lại publisher/receiver hiện có; một capture/encoder/packetizer;
  class relay không encode. Nguồn Student tự mở Control cooperative và giữ cursor.
- Checklist tự mở một lần khi cả hai Recording rõ ràng chưa cấp; trạng thái đã xem
  được lưu trong UserDefaults. Đã có một grant/đã xem/unknown không mở popup tự động.
  Nút toolbar và banner vẫn hoạt động; không coi dấu đã xem là đã có quyền.

## Kiểm tra và build

- Rust Classroom198 passed,2 ignored; có tests target parsing, queue1, chỉ chạy sau
  worker cũ, deadline và Stop cancel (`target/source-switch-rust-tests.txt`).
- Flutter13 passed: toolbar handoff/current target/Stop/không minimize Student,
  responsive layout; onboarding first/missing/partial/unknown/seen; regression
  permissions/manual dialog/widget/class Control (`target/source-switch-flutter-tests.txt`).
- Flutter analyze: no issues (`target/source-switch-flutter-analyze.txt`). Python
  mixed-endpoint runner biên dịch; Git diff whitespace check không lỗi.
- Build gộp paired-source-switch-20261009 thành công, cùng working tree cho Teacher,
  Mac Student, Windows x64 Student/Guard và hai Student ZIP nhúng Teacher.
  Chưa có native handoff pass từ môi trường mới. Không suy rộng test cũ sang IP mới.

## Native tiếp theo

Runner `scripts/test-source-switch.py`: prepare package cô lập/verify hash, đọc
Recording từ GUI LaunchAgent và phát sinh LAN probe; không tự cấp quyền. Sau khi
owner cấp quyền, chạy Teacher/Windows/Mac → đổi nguồn hai vòng; đợi actual render
health của recipients ở stream mới; invalid/current targets; stale Control-close;
Stop giữa lúc chuyển; thu log và dọn task/rule/LaunchAgent đúng namespace.

Teacher permission targets:
1. `/Users/mvmanh/Development/netsupport-school/target/student-broadcast-build/Classroom Console.app`
2. `/Users/mvmanh/Development/netsupport-school/target/student-broadcast-build/Classroom Console.app/Contents/Helpers/Classroom Agent.app`
Mac Student192.168.100.254 target:
`/Users/lolem/Classroom-source-switch/Classroom Student.app`

Quyền được chủ máy cấp thủ công; không dùng Computer Use. Restart Teacher sau
Recording; bundle không build/ký lại trong lúc xác nhận quyền/native test.

## Chưa nghiệm thu tại phòng máy

Chuyển nguồn với35–40 Windows x64 vật lý Ethernet; thời gian frame mới đầu tiên
và độ trễ quang học; Source upload + Teacher preview load; 1080p workload/soak cố
định, sleep/DPI; Defender compatibility ở máy từng báo detection. Mac Student là
endpoint development, Windows hiện tại là môi trường Parallels; functional mixed
LAN không thay thế Phase1 wired physical acceptance.


## Tạm dừng đúng yêu cầu owner — chờ cấp quyền thủ công

Prepare đã xong: package Windows vào folder mới theo manifest, Mac Student vào
`/Users/lolem/Classroom-source-switch/Classroom Student.app`. Verify manifest/hash
local và remote, codesign Mac hợp lệ. Không thay installed Windows Student/Guard
hoặc Mac package cũ. Chạy kiểm tra chỉ đọc từ gui/501, rồi bootout job kiểm tra.
`target/source-switch-native-20261009/mac-permission-status.json` ghi screen=false.
Do đó dừng phần native handoff và đợi owner cấp Recording + Accessibility cho
Mac Student; Teacher mới ký lại cần kiểm tra/cấp Recording cho hai target phía
trên, rồi restart. Không dùng Computer Use hoặc tự cấp/đổi privacy grants.

Native UserDefaults file xác nhận onboardingSeen=true đã được lưu sau lần mở
Teacher mới. Đây là bằng chứng lưu marker, không chứng minh quyền hoặc thay thế
native kiểm tra lần mở kế tiếp. Bundle giữ nguyên trong lúc cấp quyền. Teacher
đang idle; chưa bắt đầu Source/Receiver Students hay native handoff workload.

Sau owner báo xong: restart cùng Teacher với bind192.168.100.171/port48900,
IPC `target/source-switch-endpoint.json`, log `target/source-switch-teacher.log`;
chạy `scripts/test-source-switch.py` không --prepare, output
`target/source-switch-native-20261009`. Nếu còn OS LAN prompt thì owner tự Allow;
mọi lỗi quyền cần dừng/báo đường dẫn theo chỉ dẫn. Không rebuild/sign lại trước test.
