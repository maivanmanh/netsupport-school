# AGENTS.md — Mandatory instructions for the coding agent

You are implementing a performance-sensitive native desktop system. Read every project document before coding. Treat the requirements in this package as the current authoritative baseline unless the human owner explicitly changes them.

## 1. Primary engineering goal

The highest-priority requirement is **smooth low-latency 1080p teacher-screen broadcast to up to 40 Windows student PCs over wired LAN**. Everything else is secondary in Phase 1.

Do not spend meaningful time on file sharing, student monitoring, app blocking, Internet blocking, keyboard/mouse locking, macOS, installers, security hardening, or polished UI until the media vertical slice works and is benchmarked.

## 2. Platform priority

- Primary runtime: Windows 10 Education 22H2 x64 and Windows 11 x64.
- Development host may be macOS.
- The codebase must preserve platform abstractions for future macOS support, but **do not implement macOS backends in Phase 1**.
- Produce a portable Windows executable first.
- Cross-build from macOS should be supported when practical, but actual integration/performance tests must run on real Windows machines.

## 3. Application form

Phase 1 should prefer one portable executable:

`Classroom.exe`

At startup it can offer Teacher and Student roles. Also support development CLI flags such as:

- `Classroom.exe --role teacher`
- `Classroom.exe --role student`

If a single executable causes serious technical friction, two binaries are acceptable, but document why before changing direction.

Future production packaging may provide `TeacherSetup.exe` and `StudentSetup.exe`; that is not a Phase 1 deliverable.

## 4. No web application stack

This is a native desktop application. Do not implement the product as a browser app, local web server UI, Electron app, or WebView-dependent application. Use native Rust desktop/windowing/rendering technologies. Keep the media engine independent of the UI framework.

## 5. Core media invariants

These are architectural invariants, not suggestions:

1. Capture the teacher screen once.
2. Encode the teacher stream once regardless of whether 1 or 40 students are watching.
3. Packetize once.
4. Fan out encoded packets to student endpoints without re-encoding.
5. Prefer current visual state over delivery of old frames: **latest frame wins**.
6. Never allow an unbounded video queue.
7. Do not introduce a dedicated classroom server in Phase 1. The teacher PC is the coordinator and media source/distributor.
8. Audio is out of scope.
9. Internet access must not be required for classroom operation.

## 6. Windows media baseline

Preferred Phase 1 stack:

- Rust.
- Windows APIs via `windows` / windows-rs where appropriate.
- Screen capture: DXGI Desktop Duplication + D3D11.
- Include the mouse cursor in the transmitted presentation.
- Codec: H.264.
- Initial encoder backend: Windows Media Foundation H.264, selecting an available hardware MFT when usable and falling back to a software MFT.
- Hardware acceleration is an optional optimization, not a functional dependency.
- Decoder: Windows Media Foundation H.264.
- Render: native Windows rendering, preferably D3D11 or another path that does not force a browser/WebView.
- No B-frames in the low-latency profile.
- Cache SPS/PPS and make clean joining/recovery possible.
- Support on-demand keyframes/IDR.

Keep encoder, decoder, capture, converter, transport, and renderer behind interfaces so additional backends can be introduced later.

## 7. Network baseline

- All classroom PCs use wired 1 Gbps Ethernet.
- Phase 1 media distribution is **UDP unicast fan-out**.
- Use an RTP/H.264-compatible packetization design rather than inventing opaque raw-H264-over-TCP framing.
- Control plane should be simple and reliable: length-prefixed Protobuf messages over TCP is the preferred initial design.
- Keep multicast as a future `MediaDistributor` backend, but do not require or depend on multicast in Phase 1.
- Do not depend on subnet-wide UDP broadcast discovery.

Default room example:

- Teacher: `10.0.55.200`
- Students: e.g. `10.0.55.11`, `.12`, `.13`, ...
- Mask seen in the lab: `255.255.240.0` (/20)
- Gateway example: `10.0.48.1`
- Another room may use the same pattern with a different third octet, e.g. teacher `10.0.63.200`.

Student default teacher resolution strategy may infer `10.0.<local-third-octet>.200`, but it must always be configurable/overridable.

The machines contain virtual adapters (Hyper-V and VMware). Do **not** select the first IPv4 adapter naively. Prefer the active physical Ethernet interface associated with the default route/gateway, and allow manual override.

## 8. Quality policy

Initial quality target:

- Up to 1920×1080.
- Maximum 30 fps.
- Initial target bitrate about 6 Mbps.
- Configurable/adaptive range roughly 1.5–8 Mbps.
- Low-latency design.

For classroom content, prioritize:

1. low latency / freshness,
2. text readability / resolution,
3. image quality,
4. frame rate.

Under pressure, reduce frame rate and bitrate before aggressively reducing resolution. Example policy may move 1080p30 -> 1080p24 -> 1080p20 -> 1080p15 before reducing to 900p/720p.

Use screen activity / dirty-region information where useful so static slides/code do not need 30 newly encoded frames per second. Treat thresholds as tunable parameters, not hard-coded truths.

## 9. Threading and back-pressure

Design explicit bounded stages. Suggested conceptual pipeline:

Capture -> latest-frame slot -> preprocess/color conversion -> encoder -> packetizer -> network fan-out.

- The capture-to-encode queue should normally have effective capacity 1.
- If a new frame arrives before the previous queued frame is consumed, replace/drop the older waiting frame.
- Do not allow video latency to grow through buffering.
- Keep blocking Windows/media operations away from the async control/network runtime.
- Let the codec/backend perform appropriate internal multithreading; do not create unsafe ad-hoc per-frame parallel encoding that breaks temporal dependencies.

## 10. Receiver design

Student path:

UDP receive -> small jitter buffer -> RTP/H.264 depacketize -> H.264 decode -> render.

Initial jitter budget can be around 30–50 ms and must be configurable.

Plan for:

- RTP sequence numbers and timestamps,
- loss detection,
- late packet discard,
- short retransmission/NACK mechanism,
- on-demand keyframe request after unrecoverable reference loss,
- clean late join while teacher is already presenting.

Do not trade seconds of latency for perfect completeness.

## 11. Metrics are part of Phase 1

Instrument from the beginning. The agent must not postpone observability.

At minimum expose/log:

Teacher:
- capture FPS and capture latency,
- frame-change/dirty-region ratio if available,
- encode FPS,
- p50/p95 encode latency,
- current bitrate,
- dropped/replaced frames,
- process CPU and memory,
- aggregate network send Mbps,
- connected student count.

Per student / receiver reports:
- RTT or equivalent control round trip,
- packet loss / missing sequences,
- NACK/retransmission counts,
- decode FPS,
- render FPS,
- late/dropped frames,
- receive bitrate,
- jitter-buffer delay.

Metrics may start as logs/console output; a polished dashboard is not required.

## 12. Implementation workflow

Work milestone-by-milestone. After each milestone:

1. run formatting/linting/tests that exist,
2. run the relevant executable test,
3. update `checkpoints/PROGRESS.md` with what changed, what was verified, remaining risks, and the exact next step,
4. if Git is available and project policy permits commits, create a descriptive checkpoint commit; otherwise do not invent a commit history.

Do not silently redesign a locked decision. If a locked decision proves infeasible, write the evidence and proposed change into the progress checkpoint before implementing the alternative.

## 13. Engineering quality

- Prefer clear, modular Rust over premature cleverness.
- Use `unsafe` only at Windows FFI boundaries or where technically justified; wrap it behind small safe abstractions and document invariants.
- Avoid copies in the hot video path where practical, but get a correct measurable vertical slice before micro-optimizing.
- Avoid global mutable state.
- Use structured error types and meaningful logs.
- Keep configuration externalizable.
- Do not hard-code the A505 IP range into media/control logic; it is only a default configuration.
- Do not make hardware acceleration mandatory.
- Do not assume every PC is as fast as the i7-14700 lab.

## 14. Stop conditions

Do not proceed to future features until Phase 1 has demonstrated the core path and scaling evidence described in `docs/06_PERFORMANCE_ACCEPTANCE.md`.

## Owner-authorized Windows Teacher integration test (2026-10-04)

The owner expanded the active integration scope to include the Flutter/Rust Teacher running on Windows. The Windows 11 ARM development machine was exercised as Teacher over Wi-Fi LAN with macOS and Windows ARM Student endpoints. Lifecycle, settings, cursor hiding, file transfer and diagnostics were tested; this does not constitute wired-LAN performance acceptance or a 40-physical-PC test. See the detailed Vietnamese evidence, limitations and release package record in [checkpoints/diagnostics/2026-10-04-lifecycle-settings-diagnostics.md](checkpoints/diagnostics/2026-10-04-lifecycle-settings-diagnostics.md) and [docs/18_WINDOWS_TEACHER_TEST_AND_V1_RELEASE.md](docs/18_WINDOWS_TEACHER_TEST_AND_V1_RELEASE.md). The Phase 1 acceptance criteria above remain in force.

### Owner instructions for replacing LAN test builds

Before copying a replacement build to a test machine, stop the app and its owned helpers, verify the exact old binary/bundle path, then remove that old build. Preserve settings, profiles, logs and unrelated applications. Announce the exact macOS `.app` path so the owner can grant Screen Recording and Accessibility; do not change privacy grants through automation. The current Mac Student permission target is `/Users/mvmanh/Classroom-teacher-validation/Classroom Student.app` on `192.168.50.205`; Windows ARM Student is now `192.168.50.105`. When a deletion is rejected by automatic tool review, report that limitation and use an isolated new test package without replacing a running binary.

### Follow-up Windows Teacher validation

See [the four-group follow-up report](checkpoints/diagnostics/2026-10-04-windows-teacher-followup.md) for encode pacing/GOP and crash-recovery changes. The shared-host 1080p simulation reached 1/5/10 receivers but failed decode at 20/40; it is not physical wired acceptance. A/B captures were 1488×768, the 30-minute soak had resolution changes, and actual sleep plus 125%/150% DPI remain unverified. Do not describe those cases as passed or overwrite the published v1 assets with a validation build.

### Kết luận và thứ tự ưu tiên sau bài test Windows Teacher

Windows Teacher đã được kiểm chứng về chức năng và vòng đời trong môi trường
Parallels/LAN Wi-Fi hiện tại; chưa đạt nghiệm thu chiếu mượt 1080p cho 40 PC.
Ba lỗi đã sửa và kiểm tra lại: admission encoder vượt giới hạn FPS, software MFT
không áp dụng cadence GOP qua thuộc tính cũ, và endpoint IPC timeout ngăn Flutter
khởi động lại Rust helper đã crash. Sau bản sửa, helper tự khởi động lại, hai
Student nhận lại phiên chiếu và Exit đóng GUI/helper thử nghiệm.

Agent tiếp theo phải ưu tiên các việc sau trước khi bổ sung tính năng mới:

1. **Tách nguyên nhân thất bại ở mức 20/40 receiver.** Teacher vẫn encode khoảng
   29 FPS bằng một encoder, nhưng decode phía nhận giảm mạnh. Các simulator và
   Windows ARM Student dùng chung host Mac/NIC; chưa thể quy nguyên nhân cho
   Teacher, Wi-Fi hoặc decoder riêng lẻ. Đo UDP receive/depacketize với sink không
   decode, sau đó bật decode và so sánh ingress drops, late packets, backlog,
   NACK/cache misses, keyframe recovery và tải host. Giữ cùng workload và cấu hình
   giữa các lượt; zero send errors không chứng minh receiver nhận và render tốt.
2. **Giảm thời gian phục hồi và khoảng cách decode/render trên Windows ARM.**
   A/B bản cuối ở 1488×768 có decode khoảng 28,49 FPS, render 24,31 FPS và recovery
   peak khoảng 1063 ms. Xác định thời gian nằm ở ingress, chờ keyframe, decode hay
   renderer trước khi sửa. Giữ bounded queues/latest-frame-wins; không tăng buffer
   để che mất gói hoặc đổi độ trễ thấp lấy phát đủ frame cũ. Mọi thay đổi repair,
   NACK, pacing hoặc renderer cần lượt đo đối chứng, không coi giả thuyết là lỗi
   đã được chứng minh.
3. **Đo đúng mục tiêu nghiệm thu.** Xác nhận workload chữ, cuộn trang và chuyển
   động thực sự ở foreground; ghi độ phân giải và bitrate thực tế. Ladder vừa rồi
   là 1080p nhưng nội dung ít chuyển động, stream khoảng 0,60–1,19 Mbps dù target
   6 Mbps. Dùng Windows x64 vật lý qua Ethernet, đo độ trễ quang học và ladder
   1/5/10/20/40 theo `docs/06_PERFORMANCE_ACCEPTANCE.md`. Không dùng kết quả
   simulator trên một host để tuyên bố hỗ trợ 40 PC vật lý.
4. **Hoàn tất các trường hợp chưa xác minh.** Chạy sleep/resume thật trên môi
   trường hỗ trợ, DPI 125%/150% và soak 1080p liên tục với độ phân giải cố định.
   Lượt hiện tại có `ERROR_NOT_SUPPORTED` khi sleep, custom scaling khóa lựa chọn
   DPI và Parallels tự đổi độ phân giải trong soak; các trường hợp này chưa pass.

Chưa có bằng chứng yêu cầu redesign Teacher. Giữ các quyết định media hiện tại,
ưu tiên xác định nút thắt receiver/network và giảm thời gian phục hồi. Các mốc
nghiệm thu Phase 1 phía trên vẫn áp dụng; báo cáo chi tiết liên kết ở mục trước
là nguồn bằng chứng, không suy rộng số đo A/B thành hiệu năng 1080p/40 máy.
