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


## Native sau owner cấp quyền — phát hiện race bàn giao

Lượt đầu (`target/source-switch-native-first-20261009`) đã nhận đủ hai Students:
Teacher phát tới cả hai trong4.240s theo health-render report; Teacher → Windows
nguồn/Control và Mac nhận trong2.468s. Windows → Mac nguồn thất bại với
`Student is watching a presentation`. Đây không phải lỗi Recording/LAN: Mac
monitoring gửi reply lúc1791518816499, trong khi log receiver ghi teardown xong
`waiting for next presentation` lúc1791518816500. Stop đến trên control connection,
ObserveStart đến trên monitoring connection độc lập, nên có race khoảng1ms ở lượt
này. Cleanup xác nhận không lỗi; chưa native pass toàn vòng.

Sửa Teacher monitoring completion: chỉ class-source với đúng lỗi đang xem được
requeue trong budget4s tính từ request gốc. Không kéo dài budget qua từng retry,
không tăng jitter/video queue, không che lỗi permission hoặc Observe thường.
Reply generation phải khớp requested_at để response cũ không ghi đè job mới.
Rust201 passed/2 ignored gồm3 regression về retry/deadline/real-error/stale reply.

Đang build một cặp Teacher/Student mới paired-source-switch-handoff-20261009.
Student runtime code không đổi; remote Mac/Windows tiếp tục dùng package đã cấp
quyền, tương thích protocol8. Fresh Student packages vẫn nhúng Teacher mới theo
PROJECT_NOTES. Không deploy/sign lại remote Mac Student cho sửa chỉ ở Teacher.
Native kiểm chứng bản sửa chưa hoàn tất; cần thử cùng workload/vòng chuyển nguồn.


## Bản sửa build xong; tạm dừng do Teacher Recording sau ký lại

Paired-source-switch-handoff-20261009 build thành công. Embedded fresh Mac/Windows
Student và Guard byte/hash được kiểm tra (`target/source-switch-handoff-package-checks.json`).
Remote Students giữ nguyên bản đã cấp quyền đúng như quy tắc Teacher-only deployment.
Lượt rerun exit1 bị chặn ngay khi Teacher capture: ScreenCaptureKit báo user
 declined TCCs. Native handoff fix vì thế chưa được xác nhận, không gọi là passed.
Evidence riêng: `target/source-switch-native-permission-blocked-20261009`;
cleanup.json errors=[]; Teacher còn idle và error đã clear bằng IPC.

Dừng test đúng yêu cầu owner, không dùng Computer Use hay sửa privacy grants.
Cần owner cấp lại Screen & System Audio Recording cho đúng hai app Teacher:
- `/Users/mvmanh/Development/netsupport-school/target/student-broadcast-build/Classroom Console.app`
- `/Users/mvmanh/Development/netsupport-school/target/student-broadcast-build/Classroom Console.app/Contents/Helpers/Classroom Agent.app`

Không cần ký/deploy/cấp lại Mac Student ở .254 vì app đó không đổi. Sau owner
báo đã cấp, restart cùng Teacher bundle và chạy lại harness. Giữ bản build này.


## Sau cấp lại Teacher Recording — chặn ở Mac Student input permission

Restart cùng bundle, không build/sign/deploy. Lượt mới: Teacher capture và hai
receiver nhận; chuyển Teacher→Windows Control/relay thành công. Khi Windows→Mac,
Teacher ghi class-source teardown retry (wait6ms) rồi đi qua handshake đến bước
Control; lỗi watching trước đây không kết thúc phiên ngay nữa. Mac Student trả:
`Remote input permission/init: the application does not have the permission to simulate input`.
Không coi đầy đủ Windows→Mac Control là passed. Đây là bước cấp quyền input trên
Mac Student; Recording đã hoạt động. Full-cycle/stale-close/Stop-cancel native
acceptance vẫn chưa hoàn tất. Evidence: `target/source-switch-native-accessibility-blocked-20261009`.
Runner exit1, cleanup.json errors=[]; Teacher idle và lỗi đã clear.

Dừng ngay tại quyền theo owner yêu cầu, không Computer Use hay tự đổi quyền.
Trên Mac192.168.100.254, kiểm tra/bật Accessibility cho đúng:
`/Users/lolem/Classroom-source-switch/Classroom Student.app`.
Teacher Recording không cần cấp lại, cả ba bundles đều giữ nguyên. Sau owner
báo xong, khởi động lại Mac Student bằng GUI LaunchAgent của harness và tiếp tục.

## Native hoàn tất sau owner cấp Mac Student Accessibility

Giữ nguyên bundle đã cấp quyền, không build/sign/deploy thêm. Harness restart
Mac Student trong GUI session và chạy thành công, exit0:
`target/source-switch-native-student-grant-run.txt`. Evidence cuối nằm trong
`target/source-switch-native-20261009` (events/states/Teacher và hai Student logs,
cleanup, verified-summary); các lượt thất bại trước đó được giữ riêng như trên.

- Hai vòng Teacher → Windows → Mac → Windows → Teacher: 10 stream mới, gồm hai
  lần bắt đầu và tám lần bàn giao. Mọi recipient online ngoài Source đều báo
  receiving với render FPS >0 ở stream mới.
- Cả sáu phiên nguồn Student xác nhận mode=control, class_control=true. Mac đã
  đi qua capture/input initialization; không còn lỗi permission hay rejection
  watching làm thất bại toàn vòng ở lượt này.
- Chọn lại nguồn hiện tại giữ stream; chọn offline hoặc hai nguồn bị từ chối
  mà phiên đang chiếu vẫn tiếp tục. Đóng Control bằng surface cũ không kết thúc
  phiên Windows mới. Cả hai lần Stop ngay sau queue Mac source hủy chuyển nguồn,
  giữ idle sau hai giây, không source/next_source/error/Control tồn dư.
- Cleanup errors=[]; test tasks/rules/LaunchAgent được dọn bởi harness. Teacher
  để idle. Không thay installed Student/Guard, không tự sửa privacy grants.

Thời gian từ yêu cầu đến báo cáo nhận có render FPS dương: min2.505s,
median3.025s, max6.042s (Mac nguồn ở vòng hai). Đây bao gồm startup và nhịp báo cáo
health, không phải độ trễ quang học hay thời điểm frame đầu tiên. Chưa xác định
nguyên nhân lượt Mac dài hơn; không coi chuyển nguồn tức thời hoặc hết mọi race.

Giới hạn: runner gọi cùng backend IPC của toolbar, widget tests kiểm tra UI;
chưa xác nhận thao tác nút bằng native GUI. Control handshake/init đã pass nhưng
chưa gửi thao tác chuột/phím thực trên Mac nguồn trong bài này. Popup onboarding
đã có unit tests và marker native persisted; chưa kiểm tra trực quan lần mở thứ
hai. Không suy rộng kết quả thành 1080p30 hoặc35–40 PC vật lý.

Bước kế tiếp khi vào phòng máy: chạy ladder1/5/10/20/35–40 Windows x64 Ethernet
với workload chữ/cuộn/chuyển động foreground cố định1920×1080; đo first-frame và
optical latency khi đổi Teacher↔Student, Source upload/Teacher preview CPU/network,
Teacher và Student cùng thao tác, late join/recovery/Stop, soak liên tục. Giữ các
bài sleep/DPI/Defender máy từng detection trong checklist chưa nghiệm thu.
